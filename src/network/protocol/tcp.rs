use crate::config::connection::{AutoReplyConfig, ClientConfig, DecoderConfig, ServerConfig};
use crate::core::message_processor::{DefaultMessageProcessor, MessageProcessor};
use crate::message::MessageType;
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::network::protocol::decoder::CodecFactory;
use bytes::BytesMut;
use log::{debug, error, info};
use smol::channel::{Sender, unbounded as smol_unbounded};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// 半包数据强制 flush 的延迟
///
/// 行/长度前缀等分帧解码器的残留半包,在该延迟后作为一条消息强制落地,
/// 避免对端半包后静默时数据长时间滞留。
const FLUSH_DELAY: Duration = Duration::from_millis(50);

/// 处理解码后的数据，转换为消息并发送事件（客户端用）
fn process_decoded_data(
    data: BytesMut,
    processor: &Arc<dyn MessageProcessor>,
    event_sender: &Option<Sender<ConnectionEvent>>,
    connection_id: &str,
) {
    let raw_data: Vec<u8> = data.to_vec();
    let message = processor.process_received_message(raw_data, MessageType::Text);

    if let Some(sender) = event_sender {
        if let Err(e) = sender.try_send(ConnectionEvent::MessageReceived(
            connection_id.to_string(),
            message,
        )) {
            error!("[TCP] 发送 MessageReceived 事件失败: {:?}", e);
        }
    }
}

/// 处理解码后的数据，转换为消息并发送事件（服务器端用，包含地址信息）
fn process_decoded_data_with_addr(
    data: BytesMut,
    processor: &Arc<dyn MessageProcessor>,
    event_sender: &Option<Sender<ConnectionEvent>>,
    connection_id: &str,
    addr: &str,
) {
    let raw_data: Vec<u8> = data.to_vec();

    // 尝试将数据转换为文本，如果失败则显示十六进制
    let message_str = match String::from_utf8(raw_data.clone()) {
        Ok(s) => s,
        Err(_) => {
            // 转换为十六进制
            let hex: Vec<String> = raw_data.iter().map(|b| format!("{:02x}", b)).collect();
            hex.join(" ")
        }
    };
    debug!("TCP服务器从 {} 收到消息: {}", addr, message_str);

    // 创建消息对象
    let message = processor
        .process_received_message(raw_data, MessageType::Text)
        .with_source(addr.to_string());

    // 发送消息事件到UI线程
    if let Some(sender) = event_sender {
        if let Err(e) = sender.try_send(ConnectionEvent::MessageReceived(
            connection_id.to_string(),
            message,
        )) {
            error!("[TCP服务器] 发送 MessageReceived 事件失败: {:?}", e);
        }
    }
}

/// 网络层自动回复: 每条解码出的完整消息触发一次回复。
///
/// 回复内容为用户配置的原始字节(`AutoReplyConfig::content`), 通过该连接的发送通道
/// 投递后由用户配置的 encoder 编码发送——不额外修改内容、不擅自添加换行符。
/// 仅在启用且内容非空时发送; 未启用走 `is_enabled()` 无锁快速路径, 零开销。
fn try_auto_reply(
    auto_reply_state: &Arc<AutoReplyConfig>,
    client_tx: &Sender<Vec<u8>>,
    event_sender: &Option<Sender<ConnectionEvent>>,
    connection_id: &str,
    source: &SocketAddr,
) {
    if !auto_reply_state.is_enabled() {
        return;
    }
    let content = auto_reply_state.content();
    if content.is_empty() {
        return;
    }
    // 投递到该连接的发送通道, 由 send_fut 经 encoder 编码后写出
    if client_tx.try_send(content.clone()).is_err() {
        return;
    }
    // 在 UI 消息列表中展示回复(Sent 方向)
    if let Some(sender) = event_sender {
        let _ = sender.try_send(ConnectionEvent::auto_reply_sent(
            connection_id,
            content,
            &source.to_string(),
        ));
    }
}

/// TCP客户端实现
pub struct TcpClient {
    config: ClientConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_connected: bool,
    cancel_token: CancellationToken,
}

impl TcpClient {
    pub fn new(config: ClientConfig, event_sender: Option<Sender<ConnectionEvent>>) -> Self {
        TcpClient {
            config,
            event_sender,
            message_processor: Arc::new(DefaultMessageProcessor),
            is_connected: false,
            cancel_token: CancellationToken::new(),
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
        let cancel_token = self.cancel_token.clone();

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

            let socket = TcpStream::connect(socket_addr).await?;
            info!("TCP客户端连接成功: {}", socket_addr);

            // 创建发送器和接收器
            let (tx, rx) = smol_unbounded::<Vec<u8>>();
            // 创建解码器控制通道(用于运行时下发解码器配置, 无需重连)
            let (decoder_control_tx, decoder_control_rx) = smol_unbounded::<DecoderConfig>();

            // 发送连接成功事件到UI线程
            if let Some(sender) = &event_sender {
                debug!("[TCP客户端] 发送 Connected 事件");
                if let Err(e) = sender
                    .send(ConnectionEvent::Connected(config.id.clone()))
                    .await
                {
                    error!("[TCP客户端] 发送 Connected 事件失败: {:?}", e);
                }
                debug!("[TCP客户端] 发送 ClientWriteSenderReady 事件");
                if let Err(e) = sender
                    .send(ConnectionEvent::ClientWriteSenderReady(
                        config.id.clone(),
                        tx,
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
            let decoder_config = config.decoder_config.clone();
            let read_cancel_token = cancel_token.clone();
            let decoder_control_rx = decoder_control_rx;
            tokio::spawn(async move {
                let mut buffer = BytesMut::with_capacity(16384);

                let mut decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(
                    &decoder_config,
                );

                // 半包 flush 截止时间: 收到数据后安排 FLUSH_DELAY 后强制 flush 一次。
                // 关键: 截止时间是绝对时刻且不随后续收包顺延 —— 旧实现每轮循环重建
                // sleep,持续有数据时 flush 分支被无限推迟(饥饿),半包永不落地。
                // 截止时间到后该分支保持就绪,select 随机命中或在下一次收包路径内联触发,
                // 保证最终执行;force_flush 无待处理数据时返回 None,空转无害。
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

                                    loop {
                                        match decoder.decode(&mut buffer) {
                                            Ok(Some(data)) => {
                                                let data: BytesMut = data;
                                                process_decoded_data(
                                                    data,
                                                    &message_processor_clone,
                                                    &event_sender_clone,
                                                    &config_clone.id
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

                                    // 首次出现待 flush 的机会时安排截止时间(不随收包顺延)
                                    if flush_deadline.is_none() {
                                        flush_deadline = Some(tokio::time::Instant::now() + FLUSH_DELAY);
                                    }
                                    // 截止时间已过: 在收包路径内联触发,避免依赖 select 随机调度
                                    if let Some(deadline) = flush_deadline {
                                        if tokio::time::Instant::now() >= deadline {
                                            flush_deadline = None;
                                            if let Some(data) = decoder.force_flush() {
                                                let data: BytesMut = data;
                                                process_decoded_data(
                                                    data,
                                                    &message_processor_clone,
                                                    &event_sender_clone,
                                                    &config_clone.id
                                                );
                                            }
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("TCP读取错误: {:?}", e);
                                    break;
                                }
                            }
                        }

                        _ = tokio::time::sleep_until(flush_deadline.unwrap()), if flush_deadline.is_some() => {
                            flush_deadline = None;
                            if let Some(data) = decoder.force_flush() {
                                let data: BytesMut = data;
                                process_decoded_data(
                                    data,
                                    &message_processor_clone,
                                    &event_sender_clone,
                                    &config_clone.id
                                );
                            }
                        }

                        // 运行时下发解码器配置: 先刷新旧解码器待处理数据, 再替换为新解码器
                        new_config = decoder_control_rx.recv() => {
                            if let Ok(new_config) = new_config {
                                debug!("[TCP客户端] 收到运行时解码器配置更新: {:?}", new_config);
                                if let Some(data) = decoder.force_flush() {
                                    let data: BytesMut = data;
                                    process_decoded_data(
                                        data,
                                        &message_processor_clone,
                                        &event_sender_clone,
                                        &config_clone.id
                                    );
                                }
                                decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&new_config);
                                info!("[TCP客户端] 解码器已运行时更新");
                            }
                        }

                        _ = read_cancel_token.cancelled() => {
                            info!("TCP客户端读任务收到取消信号，退出");
                            break;
                        }
                    }
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
            let encoder_for_write = CodecFactory::create_encoder(&config.decoder_config);
            let write_cancel_token = cancel_token.clone();
            tokio::spawn(async move {
                let mut encoder = encoder_for_write;
                loop {
                    tokio::select! {
                        data = rx.recv() => {
                            match data {
                                Ok(data) => {
                                    let mut buffer = BytesMut::with_capacity(data.len());
                                    let data_bytes = BytesMut::from(data.as_slice());

                                    if let Err(e) = encoder.encode(data_bytes, &mut buffer) {
                                        error!("TCP编码错误: {:?}", e);
                                        break;
                                    }

                                    if let Err(e) = socket_write.write_all(&buffer).await {
                                        error!("TCP写入错误: {:?}", e);
                                        break;
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

/// TCP服务器实现
pub struct TcpServer {
    config: ServerConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    clients: Arc<Mutex<HashMap<SocketAddr, Sender<Vec<u8>>>>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_running: bool,
    listener_handle: Option<JoinHandle<()>>,
    client_handles: Arc<Mutex<HashMap<SocketAddr, JoinHandle<()>>>>,
    listener: Option<Arc<TcpListener>>,
    /// 自动回复共享状态(UI 下发 → 网络层每条消息读取)
    auto_reply_state: Arc<AutoReplyConfig>,
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
    pub fn new(config: ServerConfig, event_sender: Option<Sender<ConnectionEvent>>) -> Self {
        TcpServer {
            config,
            event_sender,
            clients: Arc::new(Mutex::new(HashMap::new())),
            message_processor: Arc::new(DefaultMessageProcessor),
            is_running: false,
            listener_handle: None,
            client_handles: Arc::new(Mutex::new(HashMap::new())),
            listener: None,
            auto_reply_state: Arc::new(AutoReplyConfig::new()),
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

        // 更新状态为运行中
        self.is_running = true;

        // 创建oneshot通道，用于在异步任务中传递listener和task handle
        let (tx, rx) = tokio::sync::oneshot::channel::<(Arc<TcpListener>, JoinHandle<()>)>();

        // 保存需要在异步块中使用的字段的克隆
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let clients = self.clients.clone();
        let client_handles = self.client_handles.clone();
        let auto_reply_state = self.auto_reply_state.clone();

        // 启动一个任务来创建listener并启动监听
        tokio::spawn(async move {
            // 绑定地址
            match TcpListener::bind(socket_addr).await {
                Ok(listener) => {
                    info!("TCP服务器开始监听: {}", address);

                    // 发送监听事件到UI线程
                    if let Some(sender) = &event_sender {
                        if let Err(e) = sender
                            .send(ConnectionEvent::Listening(config.id.clone()))
                            .await
                        {
                            error!("[TCP服务器] 发送 Listening 事件失败: {:?}", e);
                        }
                        // 自动回复共享状态就绪(UI 下发启用开关与回复内容)
                        if let Err(e) = sender
                            .send(ConnectionEvent::ServerAutoReplyStateReady(
                                config.id.clone(),
                                auto_reply_state.clone(),
                            ))
                            .await
                        {
                            error!(
                                "[TCP服务器] 发送 ServerAutoReplyStateReady 事件失败: {:?}",
                                e
                            );
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
                        let clients = clients.clone();
                        let client_handles = client_handles.clone();
                        let auto_reply_state = auto_reply_state.clone();
                        async move {
                            loop {
                                match listener_clone.accept().await {
                                    Ok((socket, addr)) => {
                                        debug!("TCP服务器接收到来自 {} 的连接", addr);

                                        // 创建客户端连接的发送器和接收器
                                        let (tx, rx) = smol_unbounded::<Vec<u8>>();
                                        // 供网络层自动回复投递回复字节(经该连接 encoder 编码)
                                        let client_tx_auto_reply = tx.clone();
                                        // 创建解码器控制通道(用于运行时下发解码器配置, 无需重连)
                                        let (decoder_control_tx, decoder_control_rx) =
                                            smol_unbounded::<DecoderConfig>();

                                        // 保存客户端连接到共享的clients哈希表
                                        let mut clients_guard: tokio::sync::MutexGuard<
                                            '_,
                                            HashMap<SocketAddr, Sender<Vec<u8>>>,
                                        > = clients.lock().await;
                                        clients_guard.insert(addr, tx.clone());
                                        drop(clients_guard);

                                        // 发送客户端连接事件到UI线程
                                        if let Some(sender) = &event_sender {
                                            if let Err(e) = sender
                                                .send(ConnectionEvent::ServerClientConnected(
                                                    config.id.clone(),
                                                    addr,
                                                    tx,
                                                ))
                                                .await
                                            {
                                                error!(
                                                    "[TCP服务器] 发送 ServerClientConnected 事件失败: {:?}",
                                                    e
                                                );
                                            }
                                            debug!(
                                                "[TCP服务器] 发送 ServerDecoderControlSenderReady 事件"
                                            );
                                            if let Err(e) = sender.send(ConnectionEvent::ServerDecoderControlSenderReady(
                                                config.id.clone(),
                                                addr,
                                                decoder_control_tx,
                                            )).await {
                                                error!("[TCP服务器] 发送 ServerDecoderControlSenderReady 事件失败: {:?}", e);
                                            }
                                        }

                                        // 处理客户端连接
                                        let client_id_clone = config.id.clone();
                                        let client_event_sender = event_sender.clone();
                                        let client_message_processor = message_processor.clone();
                                        let clients_clone_for_disconnect = clients.clone();
                                        let config_clone_for_client = config.clone();
                                        let client_handles_clone_for_client =
                                            client_handles.clone();
                                        let auto_reply_state_for_client = auto_reply_state.clone();

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
                                            );
                                            let decoder_control_rx = decoder_control_rx;
                                            let auto_reply_state = auto_reply_state_for_client;
                                            let client_tx_auto_reply = client_tx_auto_reply;

                                            // 启动接收消息循环
                                            let recv_fut = async {
                                                let mut buffer = BytesMut::with_capacity(16384); // 16KB缓冲区

                                                // 使用CodecFactory创建解码器（所有解码器现在都支持force_flush）
                                                let mut decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&decoder_config);

                                                // 半包 flush 截止时间(与客户端读循环同一策略):
                                                // 绝对时刻、不随收包顺延,截止后经内联或 select 分支保证执行
                                                let mut flush_deadline: Option<
                                                    tokio::time::Instant,
                                                > = None;

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

                                                                    // 使用decoder解码数据，循环处理所有可用消息
                                                                    loop {
                                                                        match decoder.decode(&mut buffer) {
                                                                            Ok(Some(data)) => {
                                                                                // 处理接收到的消息
                                                                                let data: BytesMut = data;
                                                                                process_decoded_data_with_addr(
                                                                                    data,
                                                                                    &client_message_processor,
                                                                                    &client_event_sender,
                                                                                    &client_id_clone,
                                                                                    &addr.to_string()
                                                                                );
                                                                                // 网络层自动回复(每条解码出的完整消息触发一次)
                                                                                try_auto_reply(
                                                                                    &auto_reply_state,
                                                                                    &client_tx_auto_reply,
                                                                                    &client_event_sender,
                                                                                    &client_id_clone,
                                                                                    &addr,
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

                                                                    // 首次安排 flush 截止时间(不随收包顺延),截止后在收包路径内联触发
                                                                    if flush_deadline.is_none() {
                                                                        flush_deadline = Some(tokio::time::Instant::now() + FLUSH_DELAY);
                                                                    }
                                                                    if let Some(deadline) = flush_deadline {
                                                                        if tokio::time::Instant::now() >= deadline {
                                                                            flush_deadline = None;
                                                                            if let Some(data) = decoder.force_flush() {
                                                                                let data: BytesMut = data;
                                                                                process_decoded_data_with_addr(
                                                                                    data,
                                                                                    &client_message_processor,
                                                                                    &client_event_sender,
                                                                                    &client_id_clone,
                                                                                    &addr.to_string()
                                                                                );
                                                                                try_auto_reply(
                                                                                    &auto_reply_state,
                                                                                    &client_tx_auto_reply,
                                                                                    &client_event_sender,
                                                                                    &client_id_clone,
                                                                                    &addr,
                                                                                );
                                                                            }
                                                                        }
                                                                    }
                                                                },
                                                                Err(e) => {
                                                                    error!("TCP服务器读取来自 {} 的消息时发生错误: {:?}", addr, e);
                                                                    break;
                                                                }
                                                            }
                                                        }

                                                        // flush 截止时间到 - 强制刷新解码器缓冲区
                                                        _ = tokio::time::sleep_until(flush_deadline.unwrap()), if flush_deadline.is_some() => {
                                                            flush_deadline = None;
                                                            // 强制刷新解码器缓冲区
                                                            if let Some(data) = decoder.force_flush() {
                                                                let data: BytesMut = data;
                                                                process_decoded_data_with_addr(
                                                                    data,
                                                                    &client_message_processor,
                                                                    &client_event_sender,
                                                                    &client_id_clone,
                                                                    &addr.to_string()
                                                                );
                                                                try_auto_reply(
                                                                    &auto_reply_state,
                                                                    &client_tx_auto_reply,
                                                                    &client_event_sender,
                                                                    &client_id_clone,
                                                                    &addr,
                                                                );
                                                            }
                                                        }

                                                        // 运行时下发解码器配置: 先刷新旧解码器待处理数据, 再替换为新解码器
                                                        new_config = decoder_control_rx.recv() => {
                                                            if let Ok(new_config) = new_config {
                                                                debug!("[TCP服务器] 客户端 {} 收到运行时解码器配置更新: {:?}", addr, new_config);
                                                                if let Some(data) = decoder.force_flush() {
                                                                    let data: BytesMut = data;
                                                                    process_decoded_data_with_addr(
                                                                        data,
                                                                        &client_message_processor,
                                                                        &client_event_sender,
                                                                        &client_id_clone,
                                                                        &addr.to_string()
                                                                    );
                                                                    try_auto_reply(
                                                                        &auto_reply_state,
                                                                        &client_tx_auto_reply,
                                                                        &client_event_sender,
                                                                        &client_id_clone,
                                                                        &addr,
                                                                    );
                                                                }
                                                                decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&new_config);
                                                                info!("[TCP服务器] 客户端 {} 解码器已运行时更新", addr);
                                                            }
                                                        }
                                                    }
                                                }
                                            };

                                            // 启动发送消息循环
                                            let send_fut = async {
                                                let mut encoder = encoder;
                                                loop {
                                                    match rx.recv().await {
                                                        Ok(message) => {
                                                            let mut buffer =
                                                                BytesMut::with_capacity(
                                                                    message.len(),
                                                                );
                                                            let data_bytes =
                                                                BytesMut::from(message.as_slice());

                                                            // 使用encoder编码数据
                                                            if let Err(e) = encoder
                                                                .encode(data_bytes, &mut buffer)
                                                            {
                                                                error!(
                                                                    "TCP服务器编码消息时发生错误: {:?}",
                                                                    e
                                                                );
                                                                break;
                                                            }

                                                            // 写入数据
                                                            if let Err(e) = socket_write
                                                                .write_all(&buffer)
                                                                .await
                                                            {
                                                                error!(
                                                                    "TCP服务器向 {} 发送消息时发生错误: {:?}",
                                                                    addr, e
                                                                );
                                                                break;
                                                            }

                                                            // 尝试将消息转换为文本，如果失败则显示十六进制
                                                            let send_message_str =
                                                                match String::from_utf8(
                                                                    message.clone(),
                                                                ) {
                                                                    Ok(s) => s,
                                                                    Err(_) => {
                                                                        // 转换为十六进制
                                                                        let hex: Vec<String> =
                                                                            message
                                                                                .iter()
                                                                                .map(|b| {
                                                                                    format!(
                                                                                        "{:02x}",
                                                                                        b
                                                                                    )
                                                                                })
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
                                                HashMap<SocketAddr, Sender<Vec<u8>>>,
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
                                                    .send(
                                                        ConnectionEvent::ServerClientDisconnected(
                                                            client_id_clone.clone(),
                                                            addr,
                                                        ),
                                                    )
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

                    // 发送listener和task handle到通道
                    if let Err(e) = tx.send((listener_arc, listener_task)) {
                        error!("TCP服务器无法发送listener和task handle到通道: {:?}", e);
                    }
                }
                Err(e) => {
                    error!("TCP服务器绑定地址失败: {:?}", e);
                    // 尝试绑定回退地址，如果失败则发送错误
                    match TcpListener::bind("127.0.0.1:0").await {
                        Ok(listener) => {
                            let _ = tx.send((Arc::new(listener), tokio::spawn(async {})));
                        }
                        Err(fallback_error) => {
                            error!("TCP服务器回退地址绑定也失败: {:?}", fallback_error);
                            // 发送空结果表示完全失败
                            let _ = tx.send((
                                Arc::new(
                                    TcpListener::bind("127.0.0.1:0")
                                        .await
                                        .unwrap_or_else(|_| panic!("无法绑定任何TCP地址")),
                                ),
                                tokio::spawn(async {}),
                            ));
                        }
                    }
                }
            }
        });

        // 返回一个future，该future会等待通道中的listener和task handle，并将它们保存到self中
        Pin::from(Box::new(async move {
            // 等待通道中的listener和task handle
            match rx.await {
                Ok((listener_arc, listener_task)) => {
                    // 保存listener和listener_handle到self中
                    self.listener = Some(listener_arc);
                    self.listener_handle = Some(listener_task);
                    Ok(())
                }
                Err(e) => {
                    error!("TCP服务器无法从通道接收listener和task handle: {:?}", e);
                    // 更新状态为停止
                    self.is_running = false;
                    Err(Box::new(e) as Box<dyn std::error::Error>)
                }
            }
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
