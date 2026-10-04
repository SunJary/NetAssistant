use crate::config::connection::{ClientConfig, DecoderConfig, ServerConfig, TrailerSetting};
use crate::core::message_processor::{DefaultMessageProcessor, MessageProcessor};
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::{ConnectionEvent, NetCounters, ReceivedBatch, WireMessage};
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::network::protocol::decoder::CodecFactory;
use crate::reply::exec::handle_frame;
use crate::reply::{FrameMeta, FrameOrigin, ReplyRulesStore, RxFrame};
use bytes::BytesMut;
use log::{debug, error, info, warn};
use smol::channel::{Sender, unbounded as smol_unbounded};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// 半包数据强制 flush 的延迟
///
/// 行/长度前缀等分帧解码器的残留半包,在该延迟后作为一条消息强制落地,
/// 避免对端半包后静默时数据长时间滞留。
const FLUSH_DELAY: Duration = Duration::from_millis(50);

/// 处理一个解码出的帧(可与回复规则交互)并累加入批。
///
/// 先跑回复规则（只可能追加一条应答），帧本身照常进明细、照常计数。
#[allow(clippy::too_many_arguments)]
fn process_frame(
    data: BytesMut,
    processor: &Arc<dyn MessageProcessor>,
    batch: &mut ReceivedBatch,
    counters: &Option<NetCounters>,
    message_type: MessageType,
    rules: &Arc<ReplyRulesStore>,
    sender: &Sender<WireMessage>,
    connection_id: &str,
    source: &SocketAddr,
    meta: FrameMeta,
) {
    let raw_len = data.len() as u64;
    // P-6：整帧只复制一次（`BytesMut` → `Arc<[u8]>`），规则引擎的 `RxFrame`
    // 与消息明细共享同一份缓冲，不再各复制一份。
    let raw: Arc<[u8]> = Arc::from(&data[..]);

    if rules.is_enabled() {
        try_rule_reply(
            rules,
            sender,
            batch,
            connection_id,
            source,
            raw.clone(),
            meta,
        );
    }

    let message = processor.process_received_message(raw, message_type);
    if let Some(c) = counters {
        c.add_received(1);
        c.add_received_bytes(raw_len);
    }
    batch.count += 1;
    batch.bytes += raw_len;
    batch.messages.push(message);
}

/// TCP 服务端读循环的单帧处理：走规则引擎。
///
/// 抽成函数是为了让"主解码循环 / 静默强刷 / 解码器切换 / 连接结束"四个产出点
/// 用**同一段决策逻辑**。
#[allow(clippy::too_many_arguments)]
fn process_frame_server(
    data: BytesMut,
    processor: &Arc<dyn MessageProcessor>,
    batch: &mut ReceivedBatch,
    counters: &Option<NetCounters>,
    rules: &Arc<ReplyRulesStore>,
    sender: &Sender<WireMessage>,
    connection_id: &str,
    source: &SocketAddr,
    meta: FrameMeta,
) {
    process_frame(
        data,
        processor,
        batch,
        counters,
        MessageType::Text,
        rules,
        sender,
        connection_id,
        source,
        meta,
    );
}

/// 将本批消息一次性推送为单个 MessagesReceived 事件(洪泛聚合路径)。
fn flush_batch(
    event_sender: &Option<Sender<ConnectionEvent>>,
    connection_id: &str,
    batch: &mut ReceivedBatch,
) {
    if batch.is_empty() {
        return;
    }
    let batch = std::mem::take(batch);
    if let Some(sender) = event_sender {
        if let Err(e) = sender.try_send(ConnectionEvent::MessagesReceived(
            connection_id.to_string(),
            batch,
        )) {
            error!("[TCP] 发送 MessagesReceived 事件失败: {:?}", e);
        }
    }
}

/// 对一帧跑回复规则并投递应答(TCP 服务端与客户端共用)。
///
/// 关键点:
///   1. 走**规则引擎**而非单条固定载荷 —— 按规则集逐条匹配, 命中即停;
///   2. 应答载荷可引用 `${rx.*}`(请求帧字段), 或含 `${crc16modbus:0:6:le}`
///      这类生成型校验(发送前自动填充校验位)。
///
/// 未命中时不产生任何动作；未启用规则时零开销(`is_enabled()` 原子读即返回)。
fn try_rule_reply(
    rules: &Arc<ReplyRulesStore>,
    client_tx: &Sender<WireMessage>,
    batch: &mut ReceivedBatch,
    connection_id: &str,
    source: &SocketAddr,
    raw: Arc<[u8]>,
    meta: FrameMeta,
) {
    if !rules.is_enabled() {
        return;
    }

    // P-6：`raw` 已由调用方与本帧的 Message 共享，这里不再 `to_vec()`
    let frame = Arc::new(RxFrame::from_shared(raw, *source, meta));
    let outcome = handle_frame(rules, &frame, connection_id);

    if let Some(bytes) = &outcome.reply {
        // P-6：应答字节也共享一份 `Arc<[u8]>` —— 明细用 Arc，投递项仍需独立 Vec
        let shared: Arc<[u8]> = Arc::from(bytes.as_slice());
        // 规则自带的编码方式优先（`Raw` = 原样输出，二进制协议必需）；
        // `Inherit` 时由 encoder 追加连接级 trailer，与手动发送完全一致（决策 D-8）
        let wire = match crate::reply::exec::rule_wire_mode(outcome.codec) {
            crate::reply::exec::RuleWireMode::Inherit => WireMessage::inherit(shared.to_vec()),
            crate::reply::exec::RuleWireMode::Override(trailer) => {
                WireMessage::bypass(shared.to_vec(), trailer)
            }
        };
        if client_tx.try_send(wire).is_ok() {
            batch.sent_messages.push(
                Message::new(MessageDirection::Sent, shared, MessageType::Text)
                    .with_source(source.to_string()),
            );
        } else {
            // 通道满/关闭：不阻塞网络任务，只告警（沿用既有 try_send 语义）
            warn!("[reply] 投递应答失败: rule={:?}", outcome.rule_id);
        }
    }
}

/// TCP客户端实现
pub struct TcpClient {
    config: ClientConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    net_counters: Option<NetCounters>,
    is_connected: bool,
    cancel_token: CancellationToken,
    /// 运行期「结尾追加字符」设置(下拉改动即时生效)
    trailer: TrailerSetting,
    /// 回复规则共享状态（客户端 1:1 构造注入，决策 D-12：少一次事件往返）
    reply_rules: Arc<ReplyRulesStore>,
}

impl TcpClient {
    pub fn new(
        config: ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<NetCounters>,
        trailer: TrailerSetting,
        reply_rules: Arc<ReplyRulesStore>,
    ) -> Self {
        TcpClient {
            config,
            event_sender,
            message_processor: Arc::new(DefaultMessageProcessor),
            net_counters,
            is_connected: false,
            cancel_token: CancellationToken::new(),
            trailer,
            reply_rules,
        }
    }
}

impl NetworkConnection for TcpClient {
    fn connect(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>
    {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let net_counters = self.net_counters.clone();
        let cancel_token = self.cancel_token.clone();
        let trailer = self.trailer.clone();
        let reply_rules = self.reply_rules.clone();

        Pin::from(Box::new(async move {
            // 解析地址，支持IPv4和IPv6
            let address =
                if config.server_address.contains(':') && !config.server_address.contains('[') {
                    // IPv6地址需要方括号
                    format!("[{}]:{}", config.server_address, config.server_port)
                } else {
                    format!("{}:{}", config.server_address, config.server_port)
                };

            let socket_addr = SocketAddr::from_str(&address)
                .map_err(|e| format!("无效的地址格式 '{}': {}", address, e))?;

            info!("TCP客户端连接到地址: {}", socket_addr);

            // 按配置绑定本地地址/端口后连接; 未配置时保持系统默认(自动选择网卡与临时端口)
            let socket = match crate::network::bind::resolve_local_bind(&config, socket_addr)? {
                Some(local_addr) => {
                    let sock = if socket_addr.is_ipv4() {
                        TcpSocket::new_v4()?
                    } else {
                        TcpSocket::new_v6()?
                    };
                    sock.bind(local_addr).map_err(|e| {
                        format!(
                            "绑定本地地址 {} 失败: {}（本地端口可能已被占用，或地址不属于本机）",
                            local_addr, e
                        )
                    })?;
                    // 非 Windows 启用 SO_REUSEADDR 缓解固定本地端口快速重连时的 TIME_WAIT 占用;
                    // Windows 上 SO_REUSEADDR 语义不同(允许端口劫持), 不启用
                    #[cfg(not(windows))]
                    sock.set_reuseaddr(true)?;
                    sock.connect(socket_addr).await?
                }
                None => TcpStream::connect(socket_addr).await?,
            };
            let local_addr = socket.local_addr()?;
            info!(
                "TCP客户端连接成功: {} (本地端点: {})",
                socket_addr, local_addr
            );

            // 创建发送器和接收器
            let (tx, rx) = smol_unbounded::<WireMessage>();
            // 创建解码器控制通道(用于运行时下发解码器配置, 无需重连)
            let (decoder_control_tx, decoder_control_rx) = smol_unbounded::<DecoderConfig>();

            // 发送连接成功事件到UI线程
            if let Some(sender) = &event_sender {
                debug!("[TCP客户端] 发送 Connected 事件");
                if let Err(e) = sender
                    .send(ConnectionEvent::Connected(config.id.clone(), local_addr))
                    .await
                {
                    error!("[TCP客户端] 发送 Connected 事件失败: {:?}", e);
                }
                debug!("[TCP客户端] 发送 ClientWriteSenderReady 事件");
                if let Err(e) = sender
                    .send(ConnectionEvent::ClientWriteSenderReady(
                        config.id.clone(),
                        tx.clone(),
                    ))
                    .await
                {
                    error!("[TCP客户端] 发送 ClientWriteSenderReady 事件失败: {:?}", e);
                }
                debug!("[TCP客户端] 发送 DecoderControlSenderReady 事件");
                if let Err(e) = sender
                    .send(ConnectionEvent::DecoderControlSenderReady(
                        config.id.clone(),
                        decoder_control_tx,
                    ))
                    .await
                {
                    error!(
                        "[TCP客户端] 发送 DecoderControlSenderReady 事件失败: {:?}",
                        e
                    );
                }
            } else {
                error!("[TCP客户端] event_sender 为空，无法发送事件");
            }

            // 创建decoder和encoder
            let (mut socket_read, mut socket_write) = tokio::io::split(socket);

            // 启动接收消息任务
            let event_sender_clone = event_sender.clone();
            let config_clone = config.clone();
            let message_processor_clone = message_processor.clone();
            let net_counters_clone = net_counters.clone();
            let decoder_config = config.decoder_config.clone();
            let read_cancel_token = cancel_token.clone();
            let reply_rules_for_read = reply_rules.clone();
            let client_tx_for_reply = tx.clone();
            // 客户端侧的对端地址(远端服务端)：`${rx.src}` 与 `From` 谓词用
            let peer_addr = socket_addr;
            // 用 Option 包装: 控制通道关闭后置 None, 让 select! 中该分支退化为 pending
            let mut decoder_control_rx = Some(decoder_control_rx);
            tokio::spawn(async move {
                let mut buffer = BytesMut::with_capacity(16384);
                let mut batch = ReceivedBatch::with_capacity(64);

                let mut decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(
                    &decoder_config,
                );

                // 半包 flush 截止时间: 出现残留后安排 FLUSH_DELAY 的静默计时。
                // 关键: 每次收包都重新计时(顺延) —— 只要还有新数据进来就不强刷,
                // 因为残留会被后续字节补齐成完整帧; 只有对端真的停发(静默)才需要兜底。
                // 无残留时置 None, select! 该分支被禁用(不注册 timer), 热路径零开销。
                let mut flush_deadline: Option<tokio::time::Instant> = None;

                loop {
                    tokio::select! {
                        result = socket_read.read_buf(&mut buffer) => {
                            match result {
                                Ok(0) => {
                                    info!("TCP连接已关闭");
                                    break;
                                },
                                Ok(n) => {
                                    debug!("TCP客户端读取了 {} 字节数据", n);

                                    // 本次 read 解码出的所有帧聚合为一个批次事件(洪泛聚合路径)
                                    let mut batch = std::mem::replace(&mut batch, ReceivedBatch::with_capacity(64));

                                    loop {
                                        match decoder.decode(&mut buffer) {
                                            Ok(Some(data)) => {
                                                let data: BytesMut = data;
                                                // 【回复规则】先跑规则（可能投递一条应答），
                                                // 帧本身照常进明细与计数。
                                                process_frame(
                                                    data,
                                                    &message_processor_clone,
                                                    &mut batch,
                                                    &net_counters_clone,
                                                    MessageType::Text,
                                                    &reply_rules_for_read,
                                                    &client_tx_for_reply,
                                                    &config_clone.id,
                                                    &peer_addr,
                                                    FrameMeta {
                                                        origin: FrameOrigin::Decoded,
                                                    },
                                                );
                                            },
                                            Ok(None) => {
                                                break;
                                            },
                                            Err(e) => {
                                                error!("TCP解码错误: {:?}", e);
                                                break;
                                            }
                                        }
                                    }
                                    flush_batch(&event_sender_clone, &config_clone.id, &mut batch);

                                    // 静默计时: 有残留则顺延(每次收包重算), 无残留则关闭计时
                                    flush_deadline = if decoder.has_pending() {
                                        Some(tokio::time::Instant::now() + FLUSH_DELAY)
                                    } else {
                                        None
                                    };
                                },
                                Err(e) => {
                                    error!("TCP读取错误: {:?}", e);
                                    break;
                                }
                            }
                        }

                        // 注意: tokio::select! 对被 precondition 禁用的分支仍会求值 async expression,
                        // 因此不能用 flush_deadline.unwrap() —— 初始 None 时该分支每次循环都会 panic,
                        // 静默杀死读任务(socket 因写半句柄存活而保持打开),表现为"已连接但永远收不到消息"。
                        // 与服务器读循环一致,用 async { if let Some(d) } 包裹保证求值安全。
                        _ = async {
                            if let Some(d) = flush_deadline {
                                tokio::time::sleep_until(d).await;
                            }
                        }, if flush_deadline.is_some() => {
                            flush_deadline = None;
                            // 静默到点: 残留被取走并清空缓冲区, 作为一条消息计入统计。
                            // 半帧同样要过规则（F-21 的认知风险：静默强刷的半帧与完整帧无法
                            // 区分会误导读用户），因此 origin 标记为 ForceFlushed。
                            if let Some(data) = decoder.force_flush() {
                                let data: BytesMut = data;
                                process_frame(
                                    data,
                                    &message_processor_clone,
                                    &mut batch,
                                    &net_counters_clone,
                                    MessageType::Text,
                                    &reply_rules_for_read,
                                    &client_tx_for_reply,
                                    &config_clone.id,
                                    &peer_addr,
                                    FrameMeta {
                                        origin: FrameOrigin::ForceFlushed,
                                    },
                                                                    );
                                flush_batch(&event_sender_clone, &config_clone.id, &mut batch);
                            }
                        }

                        // 运行时下发解码器配置: 先刷新旧解码器待处理数据, 再替换为新解码器。
                        // 注意: 发送端被 drop 后 recv() 恒为就绪的 Err(通道为空且无发送端),
                        // 该分支若既不 break 也不 await, select! 每轮都会命中且全程无让出点,
                        // 单次 poll 永不返回 Pending → 整个运行时被独占(100% CPU 忙转), 同
                        // 运行时的其他任务(含超时)全部饿死。故置 None 让分支退化为 pending:
                        // 此后不再有解码器下发, 连接照常收发。
                        new_config = async {
                            match decoder_control_rx.as_ref() {
                                Some(rx) => rx.recv().await,
                                None => std::future::pending::<
                                    Result<DecoderConfig, smol::channel::RecvError>,
                                >()
                                .await,
                            }
                        } => {
                            if let Ok(new_config) = new_config {
                                debug!("[TCP客户端] 收到运行时解码器配置更新: {:?}", new_config);
                                // 交换前把旧解码器残留取出吐出, 计入统计(残留不会重现)
                                if let Some(data) = decoder.force_flush() {
                                    let data: BytesMut = data;
                                    process_frame(
                                        data,
                                        &message_processor_clone,
                                        &mut batch,
                                        &net_counters_clone,
                                        MessageType::Text,
                                        &reply_rules_for_read,
                                        &client_tx_for_reply,
                                        &config_clone.id,
                                        &peer_addr,
                                        FrameMeta {
                                            origin: FrameOrigin::ForceFlushed,
                                        },
                                                                            );
                                    flush_batch(&event_sender_clone, &config_clone.id, &mut batch);
                                }
                                decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&new_config);
                                info!("[TCP客户端] 解码器已运行时更新");
                            } else {
                                debug!("[TCP客户端] 解码器控制通道已关闭, 不再接收运行时解码器下发");
                                decoder_control_rx = None;
                            }
                        }

                        _ = read_cancel_token.cancelled() => {
                            info!("TCP客户端读任务收到取消信号，退出");
                            break;
                        }
                    }
                }

                // 连接结束: 消费式强刷下残留已被前一次静默取走, 这里补最后一次,
                // 否则"对端发半条后立刻断开"的残留永远不会显示(计入统计)
                if let Some(data) = decoder.force_flush() {
                    let data: BytesMut = data;
                    process_frame(
                        data,
                        &message_processor_clone,
                        &mut batch,
                        &net_counters_clone,
                        MessageType::Text,
                        &reply_rules_for_read,
                        &client_tx_for_reply,
                        &config_clone.id,
                        &peer_addr,
                        FrameMeta {
                            origin: FrameOrigin::EofFlushed,
                        },
                    );
                    flush_batch(&event_sender_clone, &config_clone.id, &mut batch);
                }

                if let Some(sender) = &event_sender_clone {
                    if let Err(e) = sender
                        .send(ConnectionEvent::Disconnected(config_clone.id.clone()))
                        .await
                    {
                        error!("[TCP客户端] 发送 Disconnected 事件失败: {:?}", e);
                    }
                }
            });

            // 启动发送消息任务
            let encoder_for_write = CodecFactory::create_encoder(&config.decoder_config, trailer);
            let write_cancel_token = cancel_token.clone();
            let net_counters_clone_write = net_counters.clone();
            tokio::spawn(async move {
                let mut encoder = encoder_for_write;
                loop {
                    tokio::select! {
                        data = rx.recv() => {
                            match data {
                                Ok(data) => {
                                    let buffer = match encode_wire(&mut encoder, data) {
                                        Ok(buffer) => buffer,
                                        Err(e) => {
                                            error!("TCP编码错误: {:?}", e);
                                            break;
                                        }
                                    };

                                    if let Err(e) = socket_write.write_all(&buffer).await {
                                        error!("TCP写入错误: {:?}", e);
                                        break;
                                    }
                                    // 网络层发送计数
                                    if let Some(c) = &net_counters_clone_write {
                                        c.add_sent(1);
                                        c.add_sent_bytes(buffer.len() as u64);
                                    }
                                },
                                Err(_) => {
                                    debug!("消息发送通道已关闭");
                                    break;
                                }
                            }
                        }

                        _ = write_cancel_token.cancelled() => {
                            info!("TCP客户端写任务收到取消信号，执行优雅关闭");
                            let _ = socket_write.shutdown().await;
                            break;
                        }
                    }
                }
            });

            Ok(())
        }))
    }

    fn disconnect(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>
    {
        self.is_connected = false;
        self.cancel_token.cancel();

        Pin::from(Box::new(async move { Ok(()) }))
    }
}

/// 把一个待发项编码成最终要写到 socket 上的字节。
///
/// 两个分支：
///   - `WireMessage::inherit`（默认，手动发送 / 发送任务 / 旧固定回复）→ 走连接 encoder，
///     由它按连接级 trailer 设置决定是否追加结尾。**与既有行为逐字一致**。
///   - `WireMessage::bypass`（回复规则要求原样输出）→ 直接写原始字节，跳过 encoder。
///     二进制协议（Modbus / JT808）必须这样，否则 CRLF 会追加在 CRC 后面。
fn encode_wire(
    encoder: &mut Box<
        dyn tokio_util::codec::Encoder<BytesMut, Error = std::io::Error> + Send + Sync,
    >,
    message: WireMessage,
) -> Result<BytesMut, std::io::Error> {
    if message.bypasses_trailer() {
        let data = message.into_data();
        return Ok(BytesMut::from(data.as_slice()));
    }
    let mut buffer = BytesMut::with_capacity(message.len());
    encoder.encode(BytesMut::from(message.data()), &mut buffer)?;
    Ok(buffer)
}

/// TCP服务器实现
pub struct TcpServer {
    config: ServerConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    clients: Arc<Mutex<HashMap<SocketAddr, Sender<WireMessage>>>>,
    message_processor: Arc<dyn MessageProcessor>,
    net_counters: Option<NetCounters>,
    is_running: bool,
    listener_handle: Option<JoinHandle<()>>,
    client_handles: Arc<Mutex<HashMap<SocketAddr, JoinHandle<()>>>>,
    listener: Option<Arc<TcpListener>>,
    /// 回复规则集共享状态（多客户端共享同一 store；server 先于 UI 就绪，
    /// 因此走运行时事件下发而非构造注入 —— 决策 D-12）
    reply_rules: Arc<ReplyRulesStore>,
    /// 运行期「结尾追加字符」设置(下拉改动即时生效)
    trailer: TrailerSetting,
}

/// 实现Drop trait，确保资源被正确释放
impl Drop for TcpServer {
    fn drop(&mut self) {
        // 当服务器实例被销毁时，取消监听任务
        if let Some(handle) = self.listener_handle.take() {
            handle.abort();
            debug!("TCP服务器监听任务已取消");
        }
        // 同时终止所有客户端任务: 正常路径走 stop() 已处理,
        // 但绕过 stop 直接 drop 时,若无此兜底会留下持有连接的孤儿 client task
        if let Ok(handles) = self.client_handles.try_lock() {
            for (_, handle) in handles.iter() {
                handle.abort();
            }
            let count = handles.len();
            if count > 0 {
                debug!("TCP服务器 Drop: 已终止 {} 个客户端任务", count);
            }
        }
    }
}

impl TcpServer {
    pub fn new(
        config: ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<NetCounters>,
        trailer: TrailerSetting,
    ) -> Self {
        TcpServer {
            config,
            event_sender,
            clients: Arc::new(Mutex::new(HashMap::new())),
            message_processor: Arc::new(DefaultMessageProcessor),
            net_counters,
            is_running: false,
            listener_handle: None,
            client_handles: Arc::new(Mutex::new(HashMap::new())),
            listener: None,
            reply_rules: ReplyRulesStore::new(),
            trailer,
        }
    }
}

impl NetworkServer for TcpServer {
    fn start(
        &mut self,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>,
    > {
        // 如果服务器已经在运行，直接返回
        if self.is_running {
            debug!("TCP服务器已经在运行中");
            return Pin::from(Box::new(async move { Ok(()) }));
        }

        // 绑定地址，支持IPv4和IPv6
        let address = if self.config.listen_address.contains(':')
            && !self.config.listen_address.contains('[')
        {
            // IPv6地址需要方括号
            format!(
                "[{}]:{}",
                self.config.listen_address, self.config.listen_port
            )
        } else {
            format!("{}:{}", self.config.listen_address, self.config.listen_port)
        };

        let socket_addr = match SocketAddr::from_str(&address) {
            Ok(addr) => addr,
            Err(e) => {
                error!("无效的监听地址格式 '{}': {}", address, e);
                let error_msg = format!("无效的监听地址格式: {}", e);
                return Pin::from(Box::new(async move { Err(error_msg.into()) }));
            }
        };

        info!("TCP服务器启动在地址: {}", socket_addr);

        // 保存需要在异步块中使用的字段的克隆
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let net_counters = self.net_counters.clone();
        let clients = self.clients.clone();
        let client_handles = self.client_handles.clone();
        let reply_rules = self.reply_rules.clone();
        let trailer = self.trailer.clone();

        // bind 直接在返回的 future 内执行: 失败带上下文返回 Err(对齐 UDP 模式),
        // 不再静默回退绑定随机端口,也不再留 panic 兜底
        Pin::from(Box::new(async move {
            let listener = TcpListener::bind(socket_addr).await.map_err(|e| {
                format!(
                    "绑定 {} 失败: {}（端口可能已被其他进程或另一个实例占用）",
                    address, e
                )
            })?;
            info!("TCP服务器开始监听: {}", address);
            self.is_running = true;

            // 发送监听事件到UI线程
            if let Some(sender) = &event_sender {
                if let Err(e) = sender
                    .send(ConnectionEvent::Listening(config.id.clone()))
                    .await
                {
                    error!("[TCP服务器] 发送 Listening 事件失败: {:?}", e);
                }
                // 回复规则集共享状态就绪(UI 运行时下发整表)
                if let Err(e) = sender
                    .send(ConnectionEvent::ReplyRulesStoreReady(
                        config.id.clone(),
                        reply_rules.clone(),
                    ))
                    .await
                {
                    error!("[TCP服务器] 发送 ReplyRulesStoreReady 事件失败: {:?}", e);
                }
            }

            // 将listener包装在Arc中
            let listener_arc = Arc::new(listener);

            // 启动独立的监听任务
            let listener_task = tokio::spawn({
                let listener_clone = listener_arc.clone();
                let config = config.clone();
                let event_sender = event_sender.clone();
                let message_processor = message_processor.clone();
                let net_counters = net_counters.clone();
                let clients = clients.clone();
                let client_handles = client_handles.clone();
                let reply_rules = reply_rules.clone();
                async move {
                    loop {
                        match listener_clone.accept().await {
                            Ok((socket, addr)) => {
                                debug!("TCP服务器接收到来自 {} 的连接", addr);

                                // 创建客户端连接的发送器和接收器
                                let (tx, rx) = smol_unbounded::<WireMessage>();
                                // 创建解码器控制通道(用于运行时下发解码器配置, 无需重连)
                                let (decoder_control_tx, decoder_control_rx) =
                                    smol_unbounded::<DecoderConfig>();

                                // 保存客户端连接到共享的clients哈希表
                                let mut clients_guard: tokio::sync::MutexGuard<
                                    '_,
                                    HashMap<SocketAddr, Sender<WireMessage>>,
                                > = clients.lock().await;
                                clients_guard.insert(addr, tx.clone());
                                drop(clients_guard);

                                // 发送客户端连接事件到UI线程
                                if let Some(sender) = &event_sender {
                                    if let Err(e) = sender
                                        .send(ConnectionEvent::ServerClientConnected(
                                            config.id.clone(),
                                            addr,
                                            tx.clone(),
                                        ))
                                        .await
                                    {
                                        error!(
                                            "[TCP服务器] 发送 ServerClientConnected 事件失败: {:?}",
                                            e
                                        );
                                    }
                                    debug!("[TCP服务器] 发送 ServerDecoderControlSenderReady 事件");
                                    if let Err(e) = sender
                                        .send(ConnectionEvent::ServerDecoderControlSenderReady(
                                            config.id.clone(),
                                            addr,
                                            decoder_control_tx,
                                        ))
                                        .await
                                    {
                                        error!(
                                            "[TCP服务器] 发送 ServerDecoderControlSenderReady 事件失败: {:?}",
                                            e
                                        );
                                    }
                                }

                                // 处理客户端连接
                                let client_id_clone = config.id.clone();
                                let client_event_sender = event_sender.clone();
                                let client_message_processor = message_processor.clone();
                                let client_net_counters = net_counters.clone();
                                let clients_clone_for_disconnect = clients.clone();
                                let config_clone_for_client = config.clone();
                                let client_handles_clone_for_client = client_handles.clone();
                                let reply_rules_for_client = reply_rules.clone();
                                let trailer_for_client = trailer.clone();

                                // 创建客户端连接的任务句柄
                                let client_task = tokio::spawn(async move {
                                    // 创建decoder和encoder
                                    let (mut socket_read, mut socket_write) =
                                        tokio::io::split(socket);

                                    // 根据配置创建具体的解码器
                                    let decoder_config =
                                        config_clone_for_client.decoder_config.clone();
                                    let encoder = CodecFactory::create_encoder(
                                        &config_clone_for_client.decoder_config,
                                        trailer_for_client.clone(),
                                    );
                                    // 用 Option 包装: 控制通道关闭后置 None, 让 select! 中该分支退化为 pending
                                    let mut decoder_control_rx = Some(decoder_control_rx);
                                    let reply_rules = reply_rules_for_client;

                                    // 启动接收消息循环
                                    let recv_fut = async {
                                        let mut buffer = BytesMut::with_capacity(16384); // 16KB缓冲区
                                        let mut batch = ReceivedBatch::with_capacity(64);

                                        // 使用CodecFactory创建解码器（所有解码器现在都支持force_flush）
                                        let mut decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&decoder_config);

                                        // 半包 flush 截止时间(与客户端读循环同一策略):
                                        // 出现残留后安排静默计时, 每次收包顺延, 无残留则禁用该分支
                                        let mut flush_deadline: Option<tokio::time::Instant> = None;

                                        loop {
                                            tokio::select! {
                                                // 数据读取事件
                                                result = socket_read.read_buf(&mut buffer) => {
                                                    match result {
                                                        Ok(0) => {
                                                            // 客户端关闭连接
                                                            debug!("TCP客户端 {} 断开连接", addr);
                                                            break;
                                                        },
                                                        Ok(n) => {
                                                            debug!("TCP服务器从 {} 读取了 {} 字节数据", addr, n);

                                                            // 本次 read 解码出的所有帧聚合为一个批次事件(洪泛聚合路径)
                                                            let mut batch = std::mem::replace(&mut batch, ReceivedBatch::with_capacity(64));

                                                            // 使用decoder解码数据，循环处理所有可用消息
                                                            loop {
                                                                match decoder.decode(&mut buffer) {
                                                                    Ok(Some(data)) => {
                                                                        let data: BytesMut = data;
                                                                        // 规则优先；未启用规则时走旧固定回复轨（行为零变化）
                                                                        process_frame_server(
                                                                            data,
                                                                            &client_message_processor,
                                                                            &mut batch,
                                                                            &client_net_counters,
                                                                            &reply_rules,
                                                                            &tx,
                                                                            &client_id_clone,
                                                                            &addr,
                                                                            FrameMeta {
                                                                                origin: FrameOrigin::Decoded,
                                                                            },
                                                                                                                                                    );
                                                                    },
                                                                    Ok(None) => {
                                                                        // 解码器需要更多数据，退出循环
                                                                        break;
                                                                    },
                                                                    Err(e) => {
                                                                        // 处理解码错误
                                                                        error!("TCP服务器读取来自 {} 的消息时发生错误: {:?}", addr, e);
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                            flush_batch(&client_event_sender, &client_id_clone, &mut batch);

                                                            // 静默计时: 有残留则顺延(每次收包重算), 无残留则关闭计时
                                                            flush_deadline = if decoder.has_pending() {
                                                                Some(tokio::time::Instant::now() + FLUSH_DELAY)
                                                            } else {
                                                                None
                                                            };
                                                        },
                                                        Err(e) => {
                                                            error!("TCP服务器读取来自 {} 的消息时发生错误: {:?}", addr, e);
                                                            break;
                                                        }
                                                    }
                                                }

                                                // flush 截止时间到 - 强制刷新解码器缓冲区
                                                _ = async {
                                                    if let Some(d) = flush_deadline {
                                                        tokio::time::sleep_until(d).await;
                                                    }
                                                }, if flush_deadline.is_some() => {
                                                    flush_deadline = None;
                                                    // 静默到点: 残留被取走并清空缓冲区, 作为一条消息计入统计
                                                    if let Some(data) = decoder.force_flush() {
                                                        let data: BytesMut = data;
                                                        process_frame_server(
                                                            data,
                                                            &client_message_processor,
                                                            &mut batch,
                                                            &client_net_counters,
                                                            &reply_rules,
                                                            &tx,
                                                            &client_id_clone,
                                                            &addr,
                                                            FrameMeta {
                                                                origin: FrameOrigin::ForceFlushed,
                                                            },
                                                                                                                    );
                                                        flush_batch(&client_event_sender, &client_id_clone, &mut batch);
                                                    }
                                                }

                                                // 运行时下发解码器配置: 先刷新旧解码器待处理数据, 再替换为新解码器。
                                                // 与客户端读循环同一处理: 发送端被 drop 后 recv() 恒为就绪的
                                                // Err, 分支无让出点会让 select! 每轮空转并独占整个运行时,
                                                // 故置 None 使其退化为 pending(此后不再有解码器下发)。
                                                new_config = async {
                                                    match decoder_control_rx.as_ref() {
                                                        Some(rx) => rx.recv().await,
                                                        None => std::future::pending::<
                                                            Result<DecoderConfig, smol::channel::RecvError>,
                                                        >()
                                                        .await,
                                                    }
                                                } => {
                                                    if let Ok(new_config) = new_config {
                                                        debug!("[TCP服务器] 客户端 {} 收到运行时解码器配置更新: {:?}", addr, new_config);
                                                        // 交换前把旧解码器残留取出吐出, 计入统计(残留不会重现)
                                                        if let Some(data) = decoder.force_flush() {
                                                            let data: BytesMut = data;
                                                            process_frame_server(
                                                                data,
                                                                &client_message_processor,
                                                                &mut batch,
                                                                &client_net_counters,
                                                                &reply_rules,
                                                                &tx,
                                                                &client_id_clone,
                                                                &addr,
                                                                FrameMeta {
                                                                    origin: FrameOrigin::ForceFlushed,
                                                                },
                                                                                                                            );
                                                            flush_batch(&client_event_sender, &client_id_clone, &mut batch);
                                                        }
                                                        decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&new_config);
                                                        info!("[TCP服务器] 客户端 {} 解码器已运行时更新", addr);
                                                    } else {
                                                        debug!("[TCP服务器] 客户端 {} 解码器控制通道已关闭, 不再接收运行时解码器下发", addr);
                                                        decoder_control_rx = None;
                                                    }
                                                }
                                            }
                                        }

                                        // 连接结束: 消费式强刷下残留已被前一次静默取走, 这里补最后一次,
                                        // 否则"对端发半条后立刻断开"的残留永远不会显示(计入统计)
                                        if let Some(data) = decoder.force_flush() {
                                            let data: BytesMut = data;
                                            process_frame_server(
                                                data,
                                                &client_message_processor,
                                                &mut batch,
                                                &client_net_counters,
                                                &reply_rules,
                                                &tx,
                                                &client_id_clone,
                                                &addr,
                                                FrameMeta {
                                                    origin: FrameOrigin::EofFlushed,
                                                },
                                            );
                                            flush_batch(
                                                &client_event_sender,
                                                &client_id_clone,
                                                &mut batch,
                                            );
                                        }
                                    };

                                    // 启动发送消息循环
                                    let send_fut = async {
                                        let mut encoder = encoder;
                                        loop {
                                            match rx.recv().await {
                                                Ok(message) => {
                                                    // 规则要求"原样输出"时绕过连接 trailer（二进制协议必需）
                                                    let buffer = match encode_wire(
                                                        &mut encoder,
                                                        message,
                                                    ) {
                                                        Ok(buffer) => buffer,
                                                        Err(e) => {
                                                            error!(
                                                                "TCP服务器编码消息时发生错误: {:?}",
                                                                e
                                                            );
                                                            break;
                                                        }
                                                    };

                                                    // 写入数据
                                                    if let Err(e) =
                                                        socket_write.write_all(&buffer).await
                                                    {
                                                        error!(
                                                            "TCP服务器向 {} 发送消息时发生错误: {:?}",
                                                            addr, e
                                                        );
                                                        break;
                                                    }
                                                    // 网络层发送计数(手动发送与自动回复共用此汇聚点)
                                                    if let Some(c) = &client_net_counters {
                                                        c.add_sent(1);
                                                        c.add_sent_bytes(buffer.len() as u64);
                                                    }

                                                    // 尝试将消息转换为文本，如果失败则显示十六进制
                                                    let send_message_str =
                                                        match String::from_utf8(buffer.to_vec()) {
                                                            Ok(s) => s,
                                                            Err(_) => {
                                                                // 转换为十六进制
                                                                let hex: Vec<String> = buffer
                                                                    .iter()
                                                                    .map(|b| format!("{:02x}", b))
                                                                    .collect();
                                                                hex.join(" ")
                                                            }
                                                        };
                                                    debug!(
                                                        "TCP服务器向 {} 发送消息: {}",
                                                        addr, send_message_str
                                                    );
                                                }
                                                Err(_) => {
                                                    debug!("TCP服务器发送消息通道已关闭");
                                                    break;
                                                }
                                            }
                                        }
                                    };

                                    // 同时运行接收和发送循环，任何一个结束都终止另一个
                                    tokio::select! {
                                        _ = recv_fut => {
                                            debug!("TCP服务器接收循环结束");
                                        },
                                        _ = send_fut => {
                                            debug!("TCP服务器发送循环结束");
                                        },
                                    }

                                    // 从共享的clients哈希表中移除断开连接的客户端
                                    let mut clients_guard: tokio::sync::MutexGuard<
                                        '_,
                                        HashMap<SocketAddr, Sender<WireMessage>>,
                                    > = clients_clone_for_disconnect.lock().await;
                                    clients_guard.remove(&addr);
                                    drop(clients_guard);

                                    // 从客户端任务句柄表中移除
                                    let mut handles_guard: tokio::sync::MutexGuard<
                                        '_,
                                        HashMap<SocketAddr, JoinHandle<()>>,
                                    > = client_handles_clone_for_client.lock().await;
                                    handles_guard.remove(&addr);
                                    drop(handles_guard);

                                    // 发送客户端断开连接事件到UI线程
                                    if let Some(sender) = &client_event_sender {
                                        if let Err(e) = sender
                                            .send(ConnectionEvent::ServerClientDisconnected(
                                                client_id_clone.clone(),
                                                addr,
                                            ))
                                            .await
                                        {
                                            error!(
                                                "[TCP服务器] 发送 ServerClientDisconnected 事件失败: {:?}",
                                                e
                                            );
                                        }
                                    }
                                });

                                // 保存客户端任务句柄到client_handles
                                let client_handles_clone = client_handles.clone();
                                let mut handles_guard: tokio::sync::MutexGuard<
                                    '_,
                                    HashMap<SocketAddr, JoinHandle<()>>,
                                > = client_handles_clone.lock().await;
                                handles_guard.insert(addr, client_task);
                                drop(handles_guard);
                            }
                            Err(e) => {
                                // 监听失败，可能是因为listener被关闭
                                debug!("TCP服务器监听失败: {:?}", e);
                                break;
                            }
                        }
                    }
                }
            });

            // 保存listener和accept任务句柄
            self.listener = Some(listener_arc);
            self.listener_handle = Some(listener_task);
            Ok(())
        }))
    }

    fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>
    {
        let event_sender = self.event_sender.clone();
        let server_id = self.config.id.clone();
        let clients = self.clients.clone();
        let client_handles = self.client_handles.clone();

        // 如果服务器已经停止，直接返回
        if !self.is_running {
            debug!("TCP服务器已经停止");
            return Pin::from(Box::new(async move { Ok(()) }));
        }

        // 取消监听任务
        if let Some(handle) = self.listener_handle.take() {
            handle.abort();
            debug!("TCP服务器监听任务已取消");
        }

        // 关闭监听套接字
        if let Some(_listener) = self.listener.take() {
            // 当我们从self.listener中取出listener并drop它时，会自动关闭监听套接字
            // 这将导致所有正在进行的accept()调用返回错误，从而停止接收新连接
            debug!("TCP服务器监听套接字已关闭");
        }

        // 更新状态为停止
        self.is_running = false;

        Pin::from(Box::new(async move {
            // 发送消息通知所有客户端连接关闭
            let mut clients_guard = clients.lock().await;
            let clients = std::mem::take(&mut *clients_guard);
            drop(clients_guard);

            // 关闭所有客户端连接的发送通道
            for (addr, sender) in clients {
                drop(sender); // 关闭发送通道，这会导致客户端的发送任务退出
                debug!("TCP服务器已关闭客户端 {} 的发送通道", addr);
            }

            // 取消所有客户端连接任务
            let mut handles_guard: tokio::sync::MutexGuard<
                '_,
                HashMap<SocketAddr, JoinHandle<()>>,
            > = client_handles.lock().await;
            let handles = std::mem::take(&mut *handles_guard);
            drop(handles_guard);

            for (addr, handle) in handles {
                handle.abort();
                debug!("TCP服务器已取消客户端 {} 的连接任务", addr);
            }

            // 发送断开连接事件到UI线程
            if let Some(sender) = &event_sender {
                if let Err(e) = sender.send(ConnectionEvent::Disconnected(server_id)).await {
                    error!("[TCP服务器] 发送 Disconnected 事件失败: {:?}", e);
                }
            }

            debug!("TCP服务器已停止");
            debug!("TCP服务器已停止监听端口");
            Ok(())
        }))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::message_processor::DefaultMessageProcessor;
    use crate::network::events::{NetCounters, ReceivedBatch};
    use crate::reply::model::{MatchNode, ReplyPayload, ReplyRule, ReplyRulesConfig, RuleCodec};

    fn source() -> SocketAddr {
        "127.0.0.1:5000".parse().unwrap()
    }

    fn processor() -> Arc<dyn MessageProcessor> {
        Arc::new(DefaultMessageProcessor)
    }

    fn rule_on_len(text: &str) -> ReplyRule {
        ReplyRule {
            matcher: MatchNode::Length { min: 1, max: 64 },
            payload: ReplyPayload {
                text: text.to_string(),
                hex_mode: false,
                codec: RuleCodec::Raw,
            },
            ..ReplyRule::new("测试规则", 10)
        }
    }

    fn store(cfg: ReplyRulesConfig) -> Arc<ReplyRulesStore> {
        let store = ReplyRulesStore::new();
        store.replace(&cfg);
        // 该连接总闸缺省 false；测试统一开启 "tab"
        store.set_connection_gates([("tab".to_string(), true)].into_iter().collect());
        store
    }

    /// 构造挂到 "tab" 连接下的规则集
    fn cfg(rules: Vec<ReplyRule>) -> ReplyRulesConfig {
        let mut cfg = ReplyRulesConfig::default();
        if !rules.is_empty() {
            cfg.connections.insert("tab".to_string(), rules);
        }
        cfg
    }

    fn sent_bytes(batch: &ReceivedBatch) -> Vec<Vec<u8>> {
        batch
            .sent_messages
            .iter()
            .map(|m| m.raw_data.to_vec())
            .collect()
    }

    /// T-3 ①：**规则全被禁用** → `is_enabled()` 为 false，
    /// 不产生任何应答（规则引擎是唯一路径，无旧固定回复兜底）。
    #[test]
    fn test_master_on_all_rules_disabled_no_reply() {
        let mut disabled = rule_on_len("RULE");
        disabled.enabled = false;
        let store = store(cfg(vec![disabled]));
        assert!(!store.is_enabled(), "无启用规则时快速路径必须关闭");

        let (tx, _rx) = smol_unbounded::<WireMessage>();
        let mut batch = ReceivedBatch::default();
        process_frame_server(
            BytesMut::from(&b"req"[..]),
            &processor(),
            &mut batch,
            &Some(NetCounters::default()),
            &store,
            &tx,
            "tab",
            &source(),
            FrameMeta::decoded(),
        );

        assert_eq!(batch.count, 1, "帧照常进明细");
        assert!(sent_bytes(&batch).is_empty(), "无启用规则时不产生应答");
    }

    /// T-3 ②：**有启用规则** → 走规则引擎，只回规则应答。
    #[test]
    fn test_enabled_rule_replies() {
        let store = store(cfg(vec![rule_on_len("RULE")]));
        assert!(store.is_enabled());

        let (tx, _rx) = smol_unbounded::<WireMessage>();
        let mut batch = ReceivedBatch::default();
        process_frame_server(
            BytesMut::from(&b"req"[..]),
            &processor(),
            &mut batch,
            &Some(NetCounters::default()),
            &store,
            &tx,
            "tab",
            &source(),
            FrameMeta::decoded(),
        );

        assert_eq!(batch.count, 1, "帧照常进明细");
        assert_eq!(
            sent_bytes(&batch),
            vec![b"RULE".to_vec()],
            "有启用规则时回复规则应答"
        );
        assert_eq!(
            store.hits_snapshot().values().sum::<u64>(),
            1,
            "规则命中计数 +1"
        );
    }
}
