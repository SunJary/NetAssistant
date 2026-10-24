use crate::config::connection::{ClientConfig, ServerConfig, TrailerSetting};
use crate::core::message_processor::{DefaultMessageProcessor, MessageProcessor};
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::{ConnectionEvent, NetCounters, ReceivedBatch, WireMessage};
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::reply::exec::handle_frame;
use crate::reply::{ReplyRulesStore, RxFrame};
use log::{debug, error, info, warn};
use smol::channel::{Sender, unbounded as smol_unbounded};
use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// 单批最多聚合的数据报数(排空循环上界)。
///
/// 排空必须有上界: 持续洪泛时内核接收缓冲可能始终非空, 无界排空会让循环永不退出,
/// 表现为「批次内存无界增长 + 永不下发事件 + 无法响应停止信号」。达到上界即先下发一批。
const UDP_MAX_BATCH_PACKETS: u64 = 1024;

/// 单个 recv 调用的缓冲区大小。
///
/// UDP 数据报协议上限为 65507 字节, 缓冲区必须 ≥ 该值: 过小的缓冲在 Windows 下
/// 触发 `WSAEMSGSIZE`(10040) 使读循环把可恢复的截断错误当致命错误退出,
/// 在类 Unix 下则静默截断(复现基线 B1)。缓冲区在循环外分配一次并复用。
const UDP_RECV_BUFFER_SIZE: usize = 65536;

/// UDP 单个数据报的载荷上限(IP 头 20 + UDP 头 8 = 65535 - 28)。
/// 超过该值的数据报任何协议栈都无法发送, 发送前显式拦截并给出可读提示。
const UDP_DATAGRAM_MAX: usize = 65507;

/// 给一个"未指定目标"的待发项补上目标地址（UDP 服务端按客户端注册发送通道时使用）。
///
/// 已带目标的项（规则应答）保持原目标不变 —— 规则应答必须回到真实的报文来源。
fn retarget(wire: WireMessage, addr: SocketAddr) -> WireMessage {
    if wire.target().is_some() {
        return wire;
    }
    match wire.bypasses_trailer() {
        true => WireMessage::bypass_to(wire.into_data(), None, Some(addr)),
        false => WireMessage::inherit_to(wire.into_data(), addr),
    }
}

/// UDP 的待发项 = `WireMessage`（`target` 携带目标地址）。
///
/// UDP 没有 encoder 链路（发送点直接 `send_to`），因此 trailer 在投递前就由
/// `wire_bytes` 应用好 —— 既能表达"继承连接设置"，也能表达规则要求的
/// "原样输出"（二进制协议必需）。这里用类型别名而非独立结构体，
/// 是为了让 TCP / UDP / app.rs 三处共用同一个写通道类型，避免三套类型互相转换。
type UdpWire = WireMessage;

/// 计算最终要写到线上的字节（继承连接 trailer 或直接用已处理好的字节）
fn wire_bytes(wire: &UdpWire, trailer: TrailerSetting) -> Vec<u8> {
    if wire.bypasses_trailer() {
        return wire.data().to_vec();
    }
    crate::config::connection::apply_trailer(wire.data(), trailer.get()).into_owned()
}

/// 对单个数据报跑回复规则（UDP 客户端与服务端共用）。
///
/// UDP 无粘包 —— 一次 `recv_from` 就是一个数据报，因此这里一次调用即处理完整一帧。
/// 注意 UDP 仍可能有"一个数据报被解码器拆成多帧"的情形（用户配了 LineBased 等），
/// 但现有实现按整包语义处理，本函数保持一致。
///
fn try_udp_rule_reply(
    rules: &Arc<ReplyRulesStore>,
    main_tx: &Sender<UdpWire>,
    batch: &mut ReceivedBatch,
    counters: &Option<NetCounters>,
    processor: &Arc<dyn MessageProcessor>,
    connection_id: &str,
    addr: &SocketAddr,
    raw: &[u8],
    unexpected_host: Option<&str>,
) {
    // 构造行消息的公共逻辑（规则未启用时也走这里，保证展示路径一致）
    // P-6：`bytes` 用 `Arc<[u8]>`，与规则引擎的 `RxFrame` 共享同一份数据报缓冲
    let build_message = |bytes: Arc<[u8]>| -> Message {
        let message = processor.process_received_message(bytes, MessageType::Text);
        match unexpected_host {
            Some(host) => message.with_unexpected_source(addr.to_string(), host),
            None => message.with_source(addr.to_string()),
        }
    };

    if !rules.is_enabled() {
        let raw_len = raw.len() as u64;
        if let Some(c) = counters {
            c.add_received(1);
            c.add_received_bytes(raw_len);
        }
        batch.count += 1;
        batch.bytes += raw_len;
        batch.messages.push(build_message(Arc::from(raw)));
        return;
    }

    // P-6：整个数据报只复制一次
    let shared: Arc<[u8]> = Arc::from(raw);
    let frame = Arc::new(RxFrame::from_shared(shared.clone(), *addr));
    let outcome = handle_frame(rules, &frame, connection_id);

    // 数据报本身照常进明细与计数；规则只可能追加一条应答。
    let raw_len = raw.len() as u64;
    if let Some(c) = counters {
        c.add_received(1);
        c.add_received_bytes(raw_len);
    }
    batch.count += 1;
    batch.bytes += raw_len;
    batch.messages.push(build_message(shared));

    if let Some(bytes) = &outcome.reply {
        // P-6：应答字节共享一份 `Arc<[u8]>`（明细用 Arc，投递项仍需独立 Vec）
        let reply: Arc<[u8]> = Arc::from(bytes.as_slice());
        // 规则自带编码方式优先（`Raw` = 原样输出）；
        // 目标固定为**数据报的真实来源**，这是 UDP 广播发现场景的关键
        let wire = match crate::reply::exec::rule_wire_mode(outcome.codec) {
            crate::reply::exec::RuleWireMode::Inherit => {
                WireMessage::inherit_to(reply.to_vec(), *addr)
            }
            crate::reply::exec::RuleWireMode::Override(trailer) => {
                WireMessage::bypass_to(reply.to_vec(), trailer, Some(*addr))
            }
        };
        if main_tx.try_send(wire).is_ok() {
            batch.sent_messages.push(
                Message::new(MessageDirection::Sent, reply, MessageType::Text)
                    .with_source(addr.to_string()),
            );
        } else {
            warn!("[reply] 投递应答失败: rule={:?}", outcome.rule_id);
        }
    }
}

/// UDP客户端实现
pub struct UdpClient {
    config: ClientConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    net_counters: Option<NetCounters>,
    is_connected: bool,
    cancel_token: CancellationToken,
    /// 运行期「结尾追加字符」设置(下拉改动即时生效)
    trailer: TrailerSetting,
    /// 回复规则共享状态（客户端 1:1 构造注入，决策 D-12）
    reply_rules: Arc<ReplyRulesStore>,
}

impl UdpClient {
    pub fn new(
        config: ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<NetCounters>,
        trailer: TrailerSetting,
        reply_rules: Arc<ReplyRulesStore>,
    ) -> Self {
        UdpClient {
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

impl NetworkConnection for UdpClient {
    fn connect(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        if self.is_connected {
            return Pin::from(Box::new(async move { Ok(()) }));
        }

        let config = self.config.clone();
        let server_host = self.config.server_address.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let net_counters = self.net_counters.clone();
        let cancel_token = self.cancel_token.clone();
        let trailer = self.trailer.clone();
        // 注意：必须在进入 async 块**之前**克隆，否则 async 块会借用 `self`，
        // 与 `&mut self` 的返回 future 生命周期冲突（`'1 must outlive 'static`）。
        let reply_rules = self.reply_rules.clone();

        self.is_connected = true;

        Pin::from(Box::new(async move {
            // 地址解析放在连接期: 无效地址(非法 IP/域名)返回 Err 并经管理器转为 Error 事件上报,
            // 而非在构造函数里 expect panic(panic 会被 tokio 捕获吞掉, UI 表现为"无反应")
            let address = if server_host.contains(':') && !server_host.contains('[') {
                // IPv6地址需要方括号
                format!("[{}]:{}", server_host, config.server_port)
            } else {
                format!("{}:{}", server_host, config.server_port)
            };
            let server_addr = SocketAddr::from_str(&address)
                .map_err(|e| format!("无效的UDP服务器地址 {}: {}", address, e))?;

            info!("UDP客户端连接到地址: {}", server_addr);

            // 按配置解析本地绑定; 未配置时按远端地址族使用通配地址+自动端口(与旧行为一致)
            let bind_addr = match crate::network::bind::resolve_local_bind(&config, server_addr)? {
                Some(addr) => addr,
                None if server_addr.is_ipv4() => {
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
                }
                None => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
            };
            let socket = UdpSocket::bind(bind_addr).await.map_err(|e| {
                format!(
                    "绑定本地地址 {} 失败: {}（本地端口可能已被占用，或地址不属于本机）",
                    bind_addr, e
                )
            })?;
            let local_addr = socket.local_addr().map_err(|e| {
                error!("获取UDP套接字本地地址失败: {:?}", e);
                e
            })?;
            info!("UDP客户端绑定到本地端口: {:?}", local_addr);

            let (tx, rx) = smol_unbounded::<WireMessage>();

            if let Some(sender) = &event_sender {
                info!("[UDP客户端] 发送 Connected 事件");
                if let Err(e) = sender
                    .send(ConnectionEvent::Connected(config.id.clone(), local_addr))
                    .await
                {
                    error!("[UDP客户端] 发送 Connected 事件失败: {:?}", e);
                }
                info!("[UDP客户端] 发送 ClientWriteSenderReady 事件");
                if let Err(e) = sender
                    .send(ConnectionEvent::ClientWriteSenderReady(
                        config.id.clone(),
                        tx.clone(),
                    ))
                    .await
                {
                    error!("[UDP客户端] 发送 ClientWriteSenderReady 事件失败: {:?}", e);
                }
            } else {
                error!("[UDP客户端] event_sender 为空，无法发送事件");
            }

            let shared_socket = Arc::new(socket);
            let socket_read = shared_socket.clone();
            let socket_write = shared_socket.clone();

            let event_sender_clone = event_sender.clone();
            let id_clone = config.id.clone();
            let message_processor_clone = message_processor.clone();
            let net_counters_clone = net_counters.clone();
            let read_cancel_token = cancel_token.clone();
            let expected_host = server_host;
            let reply_rules_for_read = reply_rules;

            // 规则应答直接复用主发送通道：目标地址由 `WireMessage.target` 携带，
            // 因此不需要第二条通道（这是把 UDP 待发项统一成 `WireMessage` 的收益）。
            let udp_reply_tx = tx.clone();

            tokio::spawn(async move {
                let mut buffer = vec![0u8; UDP_RECV_BUFFER_SIZE];
                let mut batch = ReceivedBatch::with_capacity(32);
                loop {
                    tokio::select! {
                        result = socket_read.recv_from(&mut buffer) => {
                            match result {
                                Ok((n, addr)) => {
                                    // 移除源地址过滤，允许接收来自任何地址的回复
                                    // 这对于广播场景很重要：下位机回复来自其真实IP而非广播地址
                                    try_udp_rule_reply(
                                        &reply_rules_for_read,
                                        &udp_reply_tx,
                                        &mut batch,
                                        &net_counters_clone,
                                        &message_processor_clone,
                                        &id_clone,
                                        &addr,
                                        &buffer[..n],
                                        Some(&expected_host),
                                    );

                                    // 非阻塞排空: 聚合突发到达的数据报为单个批次事件(洪泛聚合路径)。
                                    // try_recv_from 无可读数据时返回 WouldBlock 自动终止; 达到上界先下发一批, 保证有界
                                    while batch.count < UDP_MAX_BATCH_PACKETS {
                                        match socket_read.try_recv_from(&mut buffer) {
                                            Ok((n, src_addr)) => {
                                                try_udp_rule_reply(
                                                    &reply_rules_for_read,
                                                    &udp_reply_tx,
                                                    &mut batch,
                                                    &net_counters_clone,
                                                    &message_processor_clone,
                                                    &id_clone,
                                                    &src_addr,
                                                    &buffer[..n],
                                                    Some(&expected_host),
                                                );
                                            }
                                            Err(_) => break,
                                        }
                                    }
                                    if let Some(sender) = &event_sender_clone {
                                        let batch = std::mem::take(&mut batch);
                                        if let Err(e) = sender.try_send(ConnectionEvent::MessagesReceived(id_clone.clone(), batch)) {
                                            error!("[UDP客户端] 发送 MessagesReceived 事件失败: {:?}", e);
                                        }
                                    } else {
                                        batch = ReceivedBatch::with_capacity(32);
                                    }
                                },
                                Err(e) => {
                                    error!("UDP读取错误: {:?}", e);
                                    if let Some(sender) = &event_sender_clone {
                                        // Windows 特有: 上一次 send_to 的 ICMP 回执会在下一次 recv_from
                                        // 报 ConnectionReset(10054), 对应"上一条数据报未送达"。此时消息
                                        // 列表里那条发送记录已存在, 必须先发明确的 Error 事件再断开,
                                        // 否则用户会误以为它已成功发出(复现基线 B2)。断开语义保留不变。
                                        let reason = if e.kind() == std::io::ErrorKind::ConnectionReset {
                                            format!("数据报未送达: 对端端口不可达 ({})", e)
                                        } else {
                                            format!("接收数据报失败: {}", e)
                                        };
                                        if let Err(se) = sender.send(ConnectionEvent::Error(id_clone.clone(), reason)).await {
                                            error!("[UDP客户端] 发送 Error 事件失败: {:?}", se);
                                        }
                                        if let Err(se) = sender.send(ConnectionEvent::Disconnected(id_clone.clone())).await {
                                            error!("[UDP客户端] 发送 Disconnected 事件失败: {:?}", se);
                                        }
                                    }
                                    break;
                                },
                            }
                        }

                        _ = read_cancel_token.cancelled() => {
                            info!("UDP客户端读任务收到取消信号，退出");
                            break;
                        }
                    }
                }
            });

            let event_sender_clone_write = event_sender.clone();
            let id_clone_write = config.id.clone();
            let net_counters_clone_write = net_counters.clone();
            // 运行期结尾追加字符设置: 每包读取当前值, 下拉改动即时生效
            let trailer_write = trailer.clone();
            let write_cancel_token = cancel_token.clone();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        data = rx.recv() => {
                            match data {
                                Ok(wire) => {
                                    // UDP 无 encoder: 发送点直接调用同一纯函数追加结尾。
                                    // 规则要求"原样输出"时 `bypasses_trailer()` 为真，字节不再改动。
                                    let target = wire.target().unwrap_or(server_addr);
                                    let payload = wire_bytes(&wire, trailer_write.clone());
                                    if payload.len() > UDP_DATAGRAM_MAX {
                                        // 超限数据报发不出去, 与其让 send_to 报 WSAEMSGSIZE 后断开,
                                        // 不如提前拦截: 明确提示字节数与上限, 连接保持, 用户缩短内容后可直接重发。
                                        // ("别的软件截断发送"实为静默丢弃用户数据, 调试工具不做这种事)
                                        warn!(
                                            "[UDP客户端] 消息 {} 字节超过 UDP 单包上限 {} 字节, 已跳过",
                                            payload.len(),
                                            UDP_DATAGRAM_MAX
                                        );
                                        if let Some(sender) = &event_sender_clone_write {
                                            let _ = sender.try_send(ConnectionEvent::Warning(
                                                id_clone_write.clone(),
                                                format!(
                                                    "消息 {} 字节超过 UDP 单包上限 {} 字节, 未发送",
                                                    payload.len(),
                                                    UDP_DATAGRAM_MAX
                                                ),
                                            ));
                                        }
                                        continue;
                                    }
                                    if let Err(e) = socket_write.send_to(&payload, target).await {
                                        error!("UDP发送错误: {:?}", e);
                                        if let Some(sender) = &event_sender_clone_write {
                                            // 发送失败同样先报具体原因再断开, 让列表里那条发送记录
                                            // 有明确的失败反馈, 而非"看似成功后莫名断开"
                                            let reason = format!("发送数据报失败: {}", e);
                                            if let Err(se) = sender.send(ConnectionEvent::Error(id_clone_write.clone(), reason)).await {
                                                error!("[UDP客户端] 发送 Error 事件失败: {:?}", se);
                                            }
                                            if let Err(se) = sender.send(ConnectionEvent::Disconnected(id_clone_write.clone())).await {
                                                error!("[UDP客户端] 发送 Disconnected 事件失败: {:?}", se);
                                            }
                                        }
                                        break;
                                    }
                                    if let Some(c) = &net_counters_clone_write {
                                        c.add_sent(1);
                                        c.add_sent_bytes(payload.len() as u64);
                                    }
                                },
                                Err(_) => {
                                    debug!("UDP消息发送通道已关闭");
                                    break;
                                }
                            }
                        }

                        _ = write_cancel_token.cancelled() => {
                            info!("UDP客户端写任务收到取消信号，退出");
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
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        if !self.is_connected {
            return Pin::from(Box::new(async move { Ok(()) }));
        }

        self.is_connected = false;
        self.cancel_token.cancel();

        Pin::from(Box::new(async move { Ok(()) }))
    }
}

impl Drop for UdpClient {
    fn drop(&mut self) {
        // 绕过 disconnect() 直接 drop 时兜底终止收/发任务，避免孤儿 task 持有 socket
        self.cancel_token.cancel();
    }
}

/// UDP服务器实现
pub struct UdpServer {
    config: ServerConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    clients: Arc<RwLock<HashMap<SocketAddr, Sender<WireMessage>>>>,
    message_processor: Arc<dyn MessageProcessor>,
    net_counters: Option<NetCounters>,
    /// 父取消令牌: 仅作"实例总闸"(Drop 兜底/紧急全停)。
    /// CancellationToken 一旦取消不可恢复, 因此正常 stop() 不取消它 ——
    /// 否则同一实例再次 start() 派生的子令牌出生即取消, 服务端静默失效。
    stop_token: CancellationToken,
    /// 本轮运行的子令牌: start() 生成并记录, stop() 只取消它。
    /// 支持同一实例反复 stop→start 复用。
    current_run_token: Option<CancellationToken>,
    /// 主发送通道，用于手动添加客户端时接入发送链路
    ///
    /// 通道携带 `WireMessage` 而非裸字节：规则要求"原样输出"时必须能绕过连接
    /// trailer（二进制协议正确性的关键，见 `network::events::WireMessage`）。
    main_send_tx: Arc<Mutex<Option<Sender<WireMessage>>>>,
    /// 回复规则集共享状态（服务端走运行时事件下发，决策 D-12）
    reply_rules: Arc<ReplyRulesStore>,
    /// 运行期「结尾追加字符」设置(下拉改动即时生效)
    trailer: TrailerSetting,
}

impl UdpServer {
    pub fn new(
        config: ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<NetCounters>,
        trailer: TrailerSetting,
    ) -> Self {
        UdpServer {
            config,
            event_sender,
            clients: Arc::new(RwLock::new(HashMap::new())),
            message_processor: Arc::new(DefaultMessageProcessor),
            net_counters,
            stop_token: CancellationToken::new(),
            current_run_token: None,
            main_send_tx: Arc::new(Mutex::new(None)),
            reply_rules: ReplyRulesStore::new(),
            trailer,
        }
    }

    /// 手动添加客户端地址（仅UDP有效，不需要真实网络连接）
    /// 本质：在 clients 列表中注册一个地址，创建发送通道接入 socket 发送链路
    pub async fn add_client(&self, addr: SocketAddr) -> Result<Sender<WireMessage>, String> {
        // 检查是否已存在
        {
            let clients = self.clients.read().unwrap();
            if clients.contains_key(&addr) {
                return Err(format!("客户端 {} 已存在", addr));
            }
        }

        // 获取主发送通道
        let main_tx = {
            let guard = self.main_send_tx.lock().await;
            guard.clone()
        };
        let main_tx = match main_tx {
            Some(tx) => tx,
            None => return Err("服务器未启动".to_string()),
        };

        // 创建客户端发送通道
        let (client_tx, client_rx) = smol_unbounded::<WireMessage>();

        // 转发任务：client_rx → main_tx(主发送通道) → socket.send_to。
        // 用 `retarget` 补上目标地址：手动添加的客户端也要能收到消息，
        // 而发往它的待发项本就不带目标（与"已注册客户端"路径行为一致）。
        let main_tx_clone = main_tx.clone();
        let addr_clone = addr;
        tokio::spawn(async move {
            while let Ok(data) = client_rx.recv().await {
                if main_tx_clone
                    .send(retarget(data, addr_clone))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // 添加到 clients 列表
        {
            let mut clients = self.clients.write().unwrap();
            clients.insert(addr, client_tx.clone());
        }

        // 通知 UI 层
        if let Some(sender) = &self.event_sender {
            if let Err(e) = sender
                .send(ConnectionEvent::ServerClientConnected(
                    self.config.id.clone(),
                    addr,
                    client_tx.clone(),
                ))
                .await
            {
                error!("[UDP服务器] 发送 ServerClientConnected 事件失败: {:?}", e);
            }
        }

        info!("[UDP服务器] 手动添加客户端: {}", addr);
        Ok(client_tx)
    }
}

/// UDP 服务端的单数据报处理：走规则引擎。
///
/// 抽象出来的理由与 TCP 侧同一考量：主收包路径与排空路径必须用**同一段决策逻辑**。
#[allow(clippy::too_many_arguments)]
fn process_datagram_server(
    payload: &[u8],
    addr: &SocketAddr,
    processor: &Arc<dyn MessageProcessor>,
    batch: &mut ReceivedBatch,
    counters: &Option<NetCounters>,
    rules: &Arc<ReplyRulesStore>,
    sender: &Sender<WireMessage>,
    connection_id: &str,
) {
    try_udp_rule_reply(
        rules,
        sender,
        batch,
        counters,
        processor,
        connection_id,
        addr,
        payload,
        None,
    );
}

impl NetworkServer for UdpServer {
    fn start(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let net_counters = self.net_counters.clone();
        let reply_rules = self.reply_rules.clone();
        let trailer = self.trailer.clone();

        // 使用现有的clients字段
        let clients = self.clients.clone();
        // 保存主发送通道，用于手动添加客户端
        let main_send_tx = self.main_send_tx.clone();
        // 每次启动生成新的本轮子令牌并记录到 self: stop() 只取消本轮令牌,
        // 父令牌保持未取消, 支持同一实例反复 stop→start
        let stop_token = self.stop_token.child_token();
        self.current_run_token = Some(stop_token.clone());

        Pin::from(Box::new(async move {
            // 绑定地址，支持IPv4和IPv6
            let address =
                if config.listen_address.contains(':') && !config.listen_address.contains('[') {
                    // IPv6地址需要方括号
                    format!("[{}]:{}", config.listen_address, config.listen_port)
                } else {
                    format!("{}:{}", config.listen_address, config.listen_port)
                };

            let socket_addr = match SocketAddr::from_str(&address) {
                Ok(addr) => addr,
                Err(e) => {
                    error!("无效的UDP监听地址格式 '{}': {}", address, e);
                    return Err(format!("无效的监听地址格式: {}", e).into());
                }
            };

            info!("UDP服务器启动在地址: {}", socket_addr);
            debug!("UDP服务器配置: {:?}", config);

            let socket = UdpSocket::bind(socket_addr).await.map_err(|e| {
                format!(
                    "绑定 {} 失败: {}（端口可能已被其他进程或另一个实例占用）",
                    address, e
                )
            })?;
            info!("UDP服务器成功绑定到地址: {}", address);
            debug!("UDP套接字创建成功: {:?}", socket);

            // 使用Arc来共享socket，解决移动问题
            let socket_arc = Arc::new(socket);

            // 发送监听事件到UI线程
            if let Some(sender) = &event_sender {
                if let Err(e) = sender
                    .send(ConnectionEvent::Listening(config.id.clone()))
                    .await
                {
                    error!("[UDP服务器] 发送 Listening 事件失败: {:?}", e);
                }
                // 回复规则集共享状态就绪(UI 运行时下发整表)
                if let Err(e) = sender
                    .send(ConnectionEvent::ReplyRulesStoreReady(
                        config.id.clone(),
                        reply_rules.clone(),
                    ))
                    .await
                {
                    error!("[UDP服务器] 发送 ReplyRulesStoreReady 事件失败: {:?}", e);
                }
            }

            // 创建发送器和接收器
            let (tx, rx) = smol_unbounded::<WireMessage>();

            // 保存主发送通道，供 add_client 使用
            {
                let mut guard = main_send_tx.lock().await;
                *guard = Some(tx.clone());
            }

            let clients_clone = clients.clone();

            // 创建消息接收任务
            let event_sender_clone = event_sender.clone();
            let id_clone = config.id.clone();
            let message_processor_clone = message_processor.clone();
            let net_counters_clone = net_counters.clone();
            let socket_recv = socket_arc.clone();
            let reply_rules_clone = reply_rules.clone();
            let recv_stop_token = stop_token.clone();

            tokio::spawn(async move {
                let mut buffer = vec![0u8; UDP_RECV_BUFFER_SIZE];
                let mut batch = ReceivedBatch::with_capacity(32);
                loop {
                    tokio::select! {
                        result = socket_recv.recv_from(&mut buffer) => {
                        match result {
                        Ok((n, addr)) => {

                            // 检查是否是新客户端(读多写少: RwLock 读锁快速路径)。
                            // 锁均在下方作用域内释放, 不跨越任何 await, 保证 async block 是 Send。
                            let new_client = {
                                let clients_guard = clients_clone.read().unwrap();
                                if !clients_guard.contains_key(&addr) {
                                    // 仅新客户端时才升级为写锁注册
                                    drop(clients_guard);
                                    let mut clients_mut = clients_clone.write().unwrap();
                                    if !clients_mut.contains_key(&addr) {
                                        // 创建客户端发送通道
                                        let (client_tx, client_rx) = smol_unbounded::<WireMessage>();
                                        clients_mut.insert(addr, client_tx.clone());
                                        drop(clients_mut);

                                        // 处理从UI来的消息: 给裸字节补上目标地址
                                        let tx_clone = tx.clone();
                                        let addr_clone = addr;
                                        tokio::spawn(async move {
                                            while let Ok(wire) = client_rx.recv().await {
                                                if tx_clone.send(retarget(wire, addr_clone)).await.is_err() {
                                                    break;
                                                }
                                            }
                                        });
                                        Some((addr, client_tx))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            };
                            // 锁已全部释放, 此处 await 安全
                            if let Some((addr, client_tx)) = new_client {
                                // 发送客户端连接事件到UI线程
                                if let Some(sender) = &event_sender_clone {
                                    if let Err(e) = sender
                                        .send(ConnectionEvent::ServerClientConnected(
                                            id_clone.clone(),
                                            addr,
                                            client_tx,
                                        ))
                                        .await
                                    {
                                        error!(
                                            "[UDP服务器] 发送 ServerClientConnected 事件失败: {:?}",
                                            e
                                        );
                                    }
                                }
                            }

                            // 网络层规则/自动回复: 每个数据报触发一次, Sent 明细聚合入批
                            process_datagram_server(
                                &buffer[..n],
                                &addr,
                                &message_processor_clone,
                                &mut batch,
                                &net_counters_clone,
                                &reply_rules_clone,
                                &tx,
                                &id_clone,
                            );

                            // 非阻塞排空: 聚合突发到达的数据报为单个批次事件(洪泛聚合路径)。
                            // try_recv_from 无可读数据时返回 WouldBlock 自动终止; 达到上界先下发一批, 保证有界
                            while batch.count < UDP_MAX_BATCH_PACKETS {
                                let (n, addr) = match socket_recv.try_recv_from(&mut buffer) {
                                    Ok(v) => v,
                                    Err(_) => break,
                                };
                                {
                                    let clients_guard = clients_clone.read().unwrap();
                                    if !clients_guard.contains_key(&addr) {
                                        drop(clients_guard);
                                        let mut clients_mut = clients_clone.write().unwrap();
                                        if !clients_mut.contains_key(&addr) {
                                            let (client_tx, client_rx) = smol_unbounded::<WireMessage>();
                                            clients_mut.insert(addr, client_tx.clone());
                                            drop(clients_mut);
                                            let tx_clone = tx.clone();
                                            let addr_clone = addr;
                                            tokio::spawn(async move {
                                                while let Ok(wire) = client_rx.recv().await {
                                                    if tx_clone.send(retarget(wire, addr_clone)).await.is_err() {
                                                        break;
                                                    }
                                                }
                                            });
                                            if let Some(sender) = &event_sender_clone {
                                                let _ = sender.try_send(
                                                    ConnectionEvent::ServerClientConnected(
                                                        id_clone.clone(),
                                                        addr,
                                                        client_tx,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                                process_datagram_server(
                                    &buffer[..n],
                                    &addr,
                                    &message_processor_clone,
                                    &mut batch,
                                    &net_counters_clone,
                                    &reply_rules_clone,
                                    &tx,
                                    &id_clone,
                                );
                            }

                            // 批量事件推送(一次通道操作取代每包一次)
                            if let Some(sender) = &event_sender_clone {
                                let batch = std::mem::take(&mut batch);
                                if let Err(e) = sender.try_send(ConnectionEvent::MessagesReceived(id_clone.clone(), batch)) {
                                    error!("[UDP服务器] 发送 MessagesReceived 事件失败: {:?}", e);
                                }
                            } else {
                                batch = ReceivedBatch::with_capacity(32);
                            }
                        }
                        Err(e) => {
                            // 瞬态错误(WouldBlock/TimedOut)可重试, 不终止服务
                            if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
                                continue;
                            }
                            // 致命错误: 读任务退出后服务端将收不到任何数据报, 若不上报事件,
                            // UI 仍显示"监听中"而实际已成僵尸(复现基线 B1)。先报原因再报断开。
                            error!("UDP服务器读取消息时发生错误: {:?}", e);
                            if let Some(sender) = &event_sender_clone {
                                let reason = format!("接收数据报失败, 服务端已停止: {}", e);
                                if let Err(se) = sender.send(ConnectionEvent::Error(id_clone.clone(), reason)).await {
                                    error!("[UDP服务器] 发送 Error 事件失败: {:?}", se);
                                }
                                if let Err(se) = sender.send(ConnectionEvent::Disconnected(id_clone.clone())).await {
                                    error!("[UDP服务器] 发送 Disconnected 事件失败: {:?}", se);
                                }
                            }
                            break;
                        }
                        }
                        }
                        _ = recv_stop_token.cancelled() => {
                            info!("[UDP服务器] 接收任务收到停止信号，退出");
                            break;
                        }
                    }
                }
            });

            // 创建消息发送任务
            let socket_write = socket_arc;
            let net_counters_clone_write = net_counters.clone();
            // 运行期结尾追加字符设置: 每包读取当前值, 下拉改动即时生效; 广播与定向均经此任务回发
            let trailer_write = trailer.clone();
            let write_stop_token = stop_token;
            // 超限拦截需要上报 Warning 事件
            let event_sender_clone_write = event_sender.clone();
            let id_clone_write = config.id.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        data = rx.recv() => {
                            match data {
                                Ok(wire) => {
                                    // `finalized` 表示字节已按规则编码方式处理好（如"原样输出"），
                                    // 此时不得再追加连接 trailer —— 否则 Modbus 应答的 CRC 后面会多出字节。
                                    // UDP 服务端的每一个待发项都必须带目标（注册客户端时补上，
                                    // 规则应答则携带真实来源），无目标即丢弃并告警。
                                    let Some(target) = wire.target() else {
                                        warn!("[UDP服务器] 待发项缺少目标地址, 已丢弃");
                                        continue;
                                    };
                                    let payload = wire_bytes(&wire, trailer_write.clone());
                                    if payload.len() > UDP_DATAGRAM_MAX {
                                        // 与客户端写任务同一策略: 超限提前拦截, 提示后连接保持
                                        warn!(
                                            "[UDP服务器] 消息 {} 字节超过 UDP 单包上限 {} 字节, 已跳过",
                                            payload.len(),
                                            UDP_DATAGRAM_MAX
                                        );
                                        if let Some(sender) = &event_sender_clone_write {
                                            let _ = sender.try_send(ConnectionEvent::Warning(
                                                id_clone_write.clone(),
                                                format!(
                                                    "消息 {} 字节超过 UDP 单包上限 {} 字节, 未发送",
                                                    payload.len(),
                                                    UDP_DATAGRAM_MAX
                                                ),
                                            ));
                                        }
                                        continue;
                                    }
                                    if let Err(e) = socket_write.send_to(&payload, target).await {
                                        error!("UDP服务器发送消息时发生错误: {:?}", e);
                                    } else if let Some(c) = &net_counters_clone_write {
                                        // 洪泛下逐包日志会疯狂写文件且格式化字节开销大, 发送路径不再逐包记录
                                        c.add_sent(1);
                                        c.add_sent_bytes(payload.len() as u64);
                                    }
                                }
                                Err(_) => {
                                    debug!("[UDP服务器] 发送通道已关闭，发送任务退出");
                                    break;
                                }
                            }
                        }
                        _ = write_stop_token.cancelled() => {
                            info!("[UDP服务器] 发送任务收到停止信号，退出");
                            break;
                        }
                    }
                }
            });

            Ok(())
        }))
    }

    fn stop(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let event_sender = self.event_sender.clone();
        let server_id = self.config.id.clone();
        let clients = self.clients.clone();
        let main_send_tx = self.main_send_tx.clone();

        // 只取消本轮子令牌(收/发 task 经 select! 感知,退出后释放 socket 的 Arc 引用,端口随之关闭)。
        // 不取消父令牌: 父令牌取消后不可恢复, 会让同一实例的下一次 start() 出生即失效。
        if let Some(run_token) = self.current_run_token.take() {
            run_token.cancel();
        }
        debug!("UDP服务器已发送停止信号");

        Pin::from(Box::new(async move {
            // 清空主发送通道
            {
                let mut guard = main_send_tx.lock().await;
                *guard = None;
            }

            // 清空客户端列表(作用域内借用到 await 之前释放, 避免非 Send 的 RwLockWriteGuard 跨越 await)
            {
                let mut clients_guard = clients.write().unwrap();
                clients_guard.clear();
            }

            // 发送断开连接事件
            if let Some(sender) = &event_sender {
                if let Err(e) = sender.send(ConnectionEvent::Disconnected(server_id)).await {
                    error!("[UDP服务器] 发送 Disconnected 事件失败: {:?}", e);
                }
            }

            info!("UDP服务器已停止");
            Ok(())
        }))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Drop for UdpServer {
    fn drop(&mut self) {
        // 绕过 stop() 直接 drop 时兜底终止收/发任务,避免孤儿 task 持有 socket
        self.stop_token.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reply::model::{MatchNode, ReplyPayload, ReplyRule, ReplyRulesConfig, RuleCodec};

    fn addr() -> SocketAddr {
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

    /// 取一个当前空闲的 UDP 端口(探测后释放, 存在极小竞态, 测试可接受)
    fn free_udp_port() -> u16 {
        std::net::UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn udp_server_config(port: u16) -> ServerConfig {
        ServerConfig {
            id: "srv".to_string(),
            protocol: crate::config::connection::ConnectionType::Udp,
            listen_address: "127.0.0.1".to_string(),
            listen_port: port,
            decoder_config: crate::config::connection::DecoderConfig::default(),
            message_input_mode: "text".to_string(),
            send_trailer: crate::config::connection::TrailerKind::default(),
        }
    }

    /// 回归测试(B1): >1024B(此处 8KB)数据报必须完整到达服务端, 不截断、读循环不断开。
    /// 旧实现读缓冲区仅 1024 字节: Windows 下触发 WSAEMSGSIZE(10040) 使读循环把
    /// 可恢复的截断当致命错误退出, 服务端成为"显示监听中但收不到包"的僵尸。
    #[tokio::test]
    async fn test_udp_server_receives_large_datagram() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;

        let port = free_udp_port();
        let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut server = UdpServer::new(udp_server_config(port), Some(tx), None, TrailerSetting::default());
        server.start().await.expect("服务端应启动成功");

        let payload = vec![0xABu8; 8192];
        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.connect(("127.0.0.1", port)).await.unwrap();
        client.send(&payload).await.unwrap();

        let mut verified = false;
        while let Ok(event) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("等待事件超时")
        {
            match event {
                ConnectionEvent::MessagesReceived(_, batch) => {
                    assert_eq!(batch.count, 1, "应恰好收到 1 个数据报");
                    assert_eq!(batch.bytes, payload.len() as u64, "数据报应完整无截断");
                    let raw = batch
                        .messages
                        .first()
                        .expect("全量模式明细不应为空")
                        .raw_data
                        .to_vec();
                    assert_eq!(raw, payload, "数据报内容应逐字节一致");
                    verified = true;
                    break;
                }
                _ => continue,
            }
        }
        assert!(verified, "5 秒内未收到完整大包的 MessagesReceived 事件");
        server.stop().await.unwrap();
    }

    /// 回归测试(P2-2): 同一 UdpServer 实例 stop→start 复用后必须仍能收包。
    /// 旧实现 stop() 取消父令牌(CancellationToken 取消不可恢复), 同实例再次 start()
    /// 派生的子令牌出生即取消, 服务端静默失效 —— 第二条消息永远收不到。
    #[tokio::test]
    async fn test_udp_server_restart_same_instance() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;

        let port = free_udp_port();
        let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut server = UdpServer::new(udp_server_config(port), Some(tx), None, TrailerSetting::default());
        server.start().await.expect("首次启动应成功");

        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.connect(("127.0.0.1", port)).await.unwrap();

        // 第一轮: 收到 ping1
        client.send(b"ping1").await.unwrap();
        let mut first = false;
        while let Ok(event) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("等待事件超时")
        {
            if let ConnectionEvent::MessagesReceived(_, batch) = event {
                assert_eq!(batch.bytes, 5);
                first = true;
                break;
            }
        }
        assert!(first, "首轮未收到 ping1");

        server.stop().await.unwrap();

        // 同实例重启: stop 只取消了本轮子令牌, 父令牌未动, 新一轮任务应正常存活。
        // 旧端口释放与任务退出存在毫秒级时差, 绑定失败时短暂重试。
        let mut restarted = false;
        for _ in 0..50 {
            if server.start().await.is_ok() {
                restarted = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(restarted, "同实例重启绑定端口失败");

        client.send(b"ping2").await.unwrap();
        let mut second = false;
        while let Ok(event) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("等待事件超时")
        {
            if let ConnectionEvent::MessagesReceived(_, batch) = event {
                assert_eq!(batch.bytes, 5);
                let raw = batch.messages.first().unwrap().raw_data.to_vec();
                assert_eq!(raw, b"ping2".to_vec());
                second = true;
                break;
            }
        }
        assert!(second, "重启后未收到 ping2 —— 令牌复用修复未生效");
        server.stop().await.unwrap();
    }

    /// T-3 ①（UDP 侧）：规则全禁用 → 不产生任何应答
    #[test]
    fn test_master_on_all_rules_disabled_no_reply() {
        let mut disabled = rule_on_len("RULE");
        disabled.enabled = false;
        let store = store(cfg(vec![disabled]));
        assert!(!store.is_enabled(), "无启用规则时快速路径必须关闭");

        let (tx, _rx) = smol_unbounded::<UdpWire>();
        let mut batch = ReceivedBatch::default();
        process_datagram_server(
            b"req",
            &addr(),
            &processor(),
            &mut batch,
            &Some(NetCounters::default()),
            &store,
            &tx,
            "tab",
        );

        assert_eq!(batch.count, 1, "数据报照常进明细");
        assert!(sent_bytes(&batch).is_empty(), "无启用规则时不产生应答");
    }

    /// T-3 ②（UDP 侧）：有启用规则 → 走规则引擎，只回规则应答
    #[test]
    fn test_enabled_rule_replies() {
        let store = store(cfg(vec![rule_on_len("RULE")]));
        assert!(store.is_enabled());

        let (tx, _rx) = smol_unbounded::<UdpWire>();
        let mut batch = ReceivedBatch::default();
        process_datagram_server(
            b"req",
            &addr(),
            &processor(),
            &mut batch,
            &Some(NetCounters::default()),
            &store,
            &tx,
            "tab",
        );

        assert_eq!(batch.count, 1, "数据报照常进明细");
        assert_eq!(
            sent_bytes(&batch),
            vec![b"RULE".to_vec()],
            "有启用规则时旧固定回复不得生效"
        );
        assert_eq!(
            store.hits_snapshot().values().sum::<u64>(),
            1,
            "规则命中计数 +1"
        );
    }
}
