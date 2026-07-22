use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::str::FromStr;
use log::{debug, error, info};
use tokio::sync::Mutex;
use std::pin::Pin;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use smol::channel::{Sender, unbounded as smol_unbounded};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use crate::config::connection::{ClientConfig, ServerConfig};
use crate::message::MessageType;
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::core::message_processor::{MessageProcessor, DefaultMessageProcessor};

/// UDP客户端实现
pub struct UdpClient {
    config: ClientConfig,
    server_addr: SocketAddr,
    event_sender: Option<Sender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_connected: bool,
    cancel_token: CancellationToken,
}

impl UdpClient {
    pub fn new(
        config: ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>
    ) -> Self {
        // 解析地址，支持IPv4和IPv6
        let address = if config.server_address.contains(':') && !config.server_address.contains('[') {
            // IPv6地址需要方括号
            format!("[{}]:{}", config.server_address, config.server_port)
        } else {
            format!("{}:{}", config.server_address, config.server_port)
        };
        
        let server_addr = SocketAddr::from_str(&address)
            .expect(&format!("无效的UDP服务器地址: {}", address));
            
        UdpClient {
            config,
            server_addr,
            event_sender,
            message_processor: Arc::new(DefaultMessageProcessor),
            is_connected: false,
            cancel_token: CancellationToken::new(),
        }
    }
}

impl NetworkConnection for UdpClient {
    fn connect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        if self.is_connected {
            return Pin::from(Box::new(async move {
                Ok(())
            }));
        }
        
        let config = self.config.clone();
        let server_addr = self.server_addr;
        let server_host = self.config.server_address.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let cancel_token = self.cancel_token.clone();
        
        self.is_connected = true;
        
        Pin::from(Box::new(async move {
            info!("UDP客户端连接到地址: {}", server_addr);
            
            let bind_addr = if server_addr.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" };
            let socket = UdpSocket::bind(bind_addr).await?;
            let local_addr = socket.local_addr()
                .map_err(|e| {
                    error!("获取UDP套接字本地地址失败: {:?}", e);
                    e
                })?;
            info!("UDP客户端绑定到本地端口: {:?}", local_addr);
            
            let (tx, rx) = smol_unbounded::<Vec<u8>>();
            
            if let Some(sender) = &event_sender {
                info!("[UDP客户端] 发送 Connected 事件");
                if let Err(e) = sender.send(ConnectionEvent::Connected(config.id.clone())).await {
                    error!("[UDP客户端] 发送 Connected 事件失败: {:?}", e);
                }
                info!("[UDP客户端] 发送 ClientWriteSenderReady 事件");
                if let Err(e) = sender.send(ConnectionEvent::ClientWriteSenderReady(config.id.clone(), tx)).await {
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
            let read_cancel_token = cancel_token.clone();
            let expected_host = server_host;
            
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                loop {
                    tokio::select! {
                        result = socket_read.recv_from(&mut buffer) => {
                            match result {
                                Ok((n, addr)) => {
                                    // 移除源地址过滤，允许接收来自任何地址的回复
                                    // 这对于广播场景很重要：下位机回复来自其真实IP而非广播地址
                                    let raw_data = buffer[..n].to_vec();
                                    let message = message_processor_clone.process_received_message(raw_data, MessageType::Text)
                                        .with_unexpected_source(addr.to_string(), &expected_host);
                                    
                                    info!("UDP客户端从 {} 收到 {} 字节", addr, n);
                                    
                                    if let Some(sender) = &event_sender_clone {
                                        if let Err(e) = sender.send(ConnectionEvent::MessageReceived(id_clone.clone(), message)).await {
                                            error!("[UDP客户端] 发送 MessageReceived 事件失败: {:?}", e);
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("UDP读取错误: {:?}", e);
                                    if let Some(sender) = &event_sender_clone {
                                        if let Err(e) = sender.send(ConnectionEvent::Disconnected(id_clone.clone())).await {
                                            error!("[UDP客户端] 发送 Disconnected 事件失败: {:?}", e);
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
            let write_cancel_token = cancel_token.clone();
            
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        data = rx.recv() => {
                            match data {
                                Ok(data) => {
                                    if let Err(e) = socket_write.send_to(&data, &server_addr).await {
                                        error!("UDP发送错误: {:?}", e);
                                        if let Some(sender) = &event_sender_clone_write {
                                            if let Err(e) = sender.send(ConnectionEvent::Disconnected(id_clone_write.clone())).await {
                                                error!("[UDP客户端] 发送 Disconnected 事件失败: {:?}", e);
                                            }
                                        }
                                        break;
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
    
    fn disconnect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        if !self.is_connected {
            return Pin::from(Box::new(async move {
                Ok(())
            }));
        }
        
        self.is_connected = false;
        self.cancel_token.cancel();
        
        Pin::from(Box::new(async move {
            Ok(())
        }))
    }
    

}

/// UDP服务器实现
pub struct UdpServer {
    config: ServerConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    clients: Arc<Mutex<HashMap<SocketAddr, Sender<Vec<u8>>>>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_running: bool,
    read_handle: Option<JoinHandle<()>>,
    write_handle: Option<JoinHandle<()>>,
    /// 主发送通道，用于手动添加客户端时接入发送链路
    main_send_tx: Arc<Mutex<Option<Sender<(SocketAddr, Vec<u8>)>>>>,
}

impl UdpServer {
    pub fn new(
        config: ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>
    ) -> Self {
        UdpServer {
            config,
            event_sender,
            clients: Arc::new(Mutex::new(HashMap::new())),
            message_processor: Arc::new(DefaultMessageProcessor),
            is_running: false,
            read_handle: None,
            write_handle: None,
            main_send_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// 手动添加客户端地址（仅UDP有效，不需要真实网络连接）
    /// 本质：在 clients 列表中注册一个地址，创建发送通道接入 socket 发送链路
    pub async fn add_client(&self, addr: SocketAddr) -> Result<Sender<Vec<u8>>, String> {
        // 检查是否已存在
        {
            let clients = self.clients.lock().await;
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
        let (client_tx, client_rx) = smol_unbounded::<Vec<u8>>();

        // 转发任务：client_rx → main_tx(主发送通道) → socket.send_to
        let main_tx_clone = main_tx.clone();
        let addr_clone = addr;
        tokio::spawn(async move {
            while let Ok(data) = client_rx.recv().await {
                if main_tx_clone.send((addr_clone, data)).await.is_err() {
                    break;
                }
            }
        });

        // 添加到 clients 列表
        {
            let mut clients = self.clients.lock().await;
            clients.insert(addr, client_tx.clone());
        }

        // 通知 UI 层
        if let Some(sender) = &self.event_sender {
            if let Err(e) = sender.send(ConnectionEvent::ServerClientConnected(
                self.config.id.clone(),
                addr,
                client_tx.clone(),
            )).await {
                error!("[UDP服务器] 发送 ServerClientConnected 事件失败: {:?}", e);
            }
        }

        info!("[UDP服务器] 手动添加客户端: {}", addr);
        Ok(client_tx)
    }
}

impl NetworkServer for UdpServer {
    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        
        // 使用现有的clients字段
        let clients = self.clients.clone();
        // 保存主发送通道，用于手动添加客户端
        let main_send_tx = self.main_send_tx.clone();
        
        Pin::from(Box::new(async move {
            // 绑定地址，支持IPv4和IPv6
            let address = if config.listen_address.contains(':') && !config.listen_address.contains('[') {
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
            
            let socket = UdpSocket::bind(socket_addr).await?;
            info!("UDP服务器成功绑定到地址: {}", address);
            debug!("UDP套接字创建成功: {:?}", socket);
            
            // 使用Arc来共享socket，解决移动问题
            let socket_arc = Arc::new(socket);
            
            // 发送监听事件到UI线程
            if let Some(sender) = &event_sender {
                if let Err(e) = sender.send(ConnectionEvent::Listening(config.id.clone())).await {
                    error!("[UDP服务器] 发送 Listening 事件失败: {:?}", e);
                }
            }
            
            // 创建发送器和接收器
            let (tx, rx) = smol_unbounded::<(SocketAddr, Vec<u8>)>();
            
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
            let socket_recv = socket_arc.clone();
            
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                loop {
                    match socket_recv.recv_from(&mut buffer).await {
                        Ok((n, addr)) => {
                            // 处理接收到的消息
                            let data = buffer[..n].to_vec();
                            info!("UDP服务器从 {} 收到消息: {:?}", addr, data);
                            
                            // 检查是否是新客户端
                            let mut clients_guard = clients_clone.lock().await;
                            let is_new_client = !clients_guard.contains_key(&addr);
                            
                            // 如果是新客户端，添加到客户端列表并发送连接事件
                            if is_new_client {
                                // 创建客户端发送通道
                                let (client_tx, client_rx) = smol_unbounded::<Vec<u8>>();
                                
                                // 保存客户端信息
                                clients_guard.insert(addr, client_tx.clone());
                                drop(clients_guard);
                                
                                // 处理从UI来的消息
                                let tx_clone = tx.clone();
                                let addr_clone = addr.clone();
                                tokio::spawn(async move {
                                    while let Ok(data) = client_rx.recv().await {
                                        if tx_clone.send((addr_clone, data)).await.is_err() {
                                            break;
                                        }
                                    }
                                });
                                
                                // 发送客户端连接事件到UI线程
                                if let Some(sender) = &event_sender_clone {
                                    if let Err(e) = sender.send(ConnectionEvent::ServerClientConnected(
                                        id_clone.clone(),
                                        addr,
                                        client_tx,
                                    )).await {
                                        error!("[UDP服务器] 发送 ServerClientConnected 事件失败: {:?}", e);
                                    }
                                }
                            } else {
                                drop(clients_guard);
                            }
                            
                            // 创建消息对象
                            let mut message = message_processor_clone.process_received_message(
                                data, 
                                MessageType::Text
                            );
                            message = message.with_source(addr.to_string());
                            
                            // 发送消息事件到UI线程
                            if let Some(sender) = &event_sender_clone {
                                if let Err(e) = sender.send(ConnectionEvent::MessageReceived(
                                    id_clone.clone(),
                                    message,
                                )).await {
                                    error!("[UDP服务器] 发送 MessageReceived 事件失败: {:?}", e);
                                }
                            }
                        },
                        Err(e) => {
                            // 处理读取错误
                            error!("UDP服务器读取消息时发生错误: {:?}", e);
                            break;
                        },
                    }
                }
            });
            
            // 创建消息发送任务
            let socket_write = socket_arc;
            tokio::spawn(async move {
                while let Ok((addr, message)) = rx.recv().await {
                    if let Err(e) = socket_write.send_to(&message, addr).await {
                        error!("UDP服务器发送消息时发生错误: {:?}", e);
                    } else {
                        info!("UDP服务器向 {} 发送消息: {:?}", addr, message);
                    }
                }
            });
            
            Ok(())
        }))
    }
    
    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let event_sender = self.event_sender.clone();
        let server_id = self.config.id.clone();
        let clients = self.clients.clone();
        let main_send_tx = self.main_send_tx.clone();
        
        // 取消接收任务
        if let Some(handle) = self.read_handle.take() {
            handle.abort();
            debug!("UDP服务器接收任务已取消");
        }
        
        // 取消发送任务
        if let Some(handle) = self.write_handle.take() {
            handle.abort();
            debug!("UDP服务器发送任务已取消");
        }
        
        // 更新状态为停止
        self.is_running = false;
        
        Pin::from(Box::new(async move {
            // 清空主发送通道
            {
                let mut guard = main_send_tx.lock().await;
                *guard = None;
            }
            
            // 清空客户端列表
            let mut clients_guard = clients.lock().await;
            clients_guard.clear();
            drop(clients_guard);
            
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
