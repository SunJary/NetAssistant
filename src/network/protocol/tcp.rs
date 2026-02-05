use std::collections::HashMap;
use std::sync::Arc;
use log::{debug, error, info};
use std::pin::Pin;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use crate::config::connection::{ClientConfig, ServerConfig};
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::core::message_processor::{MessageProcessor, DefaultMessageProcessor};

/// TCP客户端实现
pub struct TcpClient {
    config: ClientConfig,
    socket: Option<TcpStream>,
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_connected: bool,
}

impl TcpClient {
    pub fn new(
        config: ClientConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Self {
        TcpClient {
            config,
            socket: None,
            event_sender,
            message_processor: Arc::new(DefaultMessageProcessor),
            is_connected: false,
        }
    }
    
    // 内部辅助方法：读取消息
    async fn read_message(&mut self) -> Result<Message, Box<dyn std::error::Error>> {
        let socket = self.socket.as_mut().ok_or("客户端未连接")?;
        let mut buffer = [0; 1024];
        
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            return Err("连接已关闭".into());
        }
        
        let raw_data = buffer[..n].to_vec();
        let message = self.message_processor.process_received_message(raw_data, MessageType::Text);
        
        Ok(message)
    }
    
    // 内部辅助方法：发送消息
    async fn write_message(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let socket = self.socket.as_mut().ok_or("客户端未连接")?;
        socket.write_all(data).await?;
        Ok(())
    }
}

impl NetworkConnection for TcpClient {
    fn connect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        
        Pin::from(Box::new(async move {
            let address = format!("{}:{}", config.server_address, config.server_port);
            info!("TCP客户端连接到地址: {}", address);
            
            let socket = TcpStream::connect(&address).await?;
            info!("TCP客户端连接成功: {}", address);
            
            // 创建发送器和接收器
            let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
            
            // 发送连接成功事件到UI线程
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Connected(config.id.clone()));
                let _ = sender.send(ConnectionEvent::ClientWriteSenderReady(config.id.clone(), tx));
            }
            
            // 分割socket为读取和写入部分
            let (socket_read, socket_write) = tokio::io::split(socket);
            
            // 启动接收消息任务
            let event_sender_clone = event_sender.clone();
            let config_clone = config.clone();
            tokio::spawn(async move {
                let mut socket_read = socket_read;
                let mut buffer = [0; 1024];
                loop {
                    match socket_read.read(&mut buffer).await {
                        Ok(0) => {
                            info!("TCP连接已关闭");
                            if let Some(sender) = &event_sender_clone {
                                let _ = sender.send(ConnectionEvent::Disconnected(config_clone.id.clone()));
                            }
                            break;
                        },
                        Ok(n) => {
                            let raw_data = buffer[..n].to_vec();
                            let message = message_processor.process_received_message(raw_data, MessageType::Text);
                            
                            if let Some(sender) = &event_sender_clone {
                                let _ = sender.send(ConnectionEvent::MessageReceived(config_clone.id.clone(), message));
                            }
                        },
                        Err(e) => {
                            error!("TCP读取错误: {:?}", e);
                            if let Some(sender) = &event_sender_clone {
                                let _ = sender.send(ConnectionEvent::Disconnected(config_clone.id.clone()));
                            }
                            break;
                        },
                    }
                }
            });
            
            // 启动发送消息任务
            tokio::spawn(async move {
                let mut socket_write = socket_write;
                loop {
                    match rx.recv().await {
                        Some(data) => {
                            if let Err(e) = socket_write.write_all(&data).await {
                                error!("TCP写入错误: {:?}", e);
                                break;
                            }
                        },
                        None => {
                            info!("消息发送通道已关闭");
                            break;
                        }
                    }
                }
            });
            
            Ok(())
        }))
    }
    
    fn disconnect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        // 在异步闭包外修改连接状态
        self.is_connected = false;
        
        let event_sender = self.event_sender.clone();
        let config = self.config.clone();
        
        Pin::from(Box::new(async move {
            // 发送断开连接事件
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Disconnected(config.id.clone()));
            }
            
            Ok(())
        }))
    }
    
    fn send_message(&mut self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let data_clone = data;
        Pin::from(Box::new(async move {
            // 注意：我们不再需要write_message方法，因为socket已经被移动到其他任务中
            // 消息应该通过ClientWriteSenderReady事件提供的sender发送
            Ok(())
        }))
    }
    
    fn receive_message(&mut self) -> Pin<Box<dyn Future<Output = Result<Message, Box<dyn std::error::Error>>> + Send>> {
        Pin::from(Box::new(async move {
            // 注意：我们不再需要read_message方法，因为接收任务已经在connect方法中启动
            // 消息应该通过ConnectionEvent::MessageReceived事件接收
            Err("Receive message is handled by event loop".into())
        }))
    }
    
    fn is_connected(&self) -> bool {
        self.is_connected
    }
}

/// TCP服务器实现
use tokio::sync::Mutex;

pub struct TcpServer {
    config: ServerConfig,
    listener: Option<TcpListener>,
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
    clients: Arc<Mutex<HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_running: bool,
    listener_handle: Option<JoinHandle<()>>,
}

impl TcpServer {
    pub fn new(
        config: ServerConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Self {
        TcpServer {
            config,
            listener: None,
            event_sender,
            clients: Arc::new(Mutex::new(HashMap::new())),
            message_processor: Arc::new(DefaultMessageProcessor),
            is_running: false,
            listener_handle: None,
        }
    }
}

impl NetworkServer for TcpServer {
    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let clients = self.clients.clone();
        
        Pin::from(Box::new(async move {
            let address = format!("{}:{}", config.listen_address, config.listen_port);
            info!("TCP服务器启动在地址: {}", address);
            
            let listener = TcpListener::bind(&address).await?;
            info!("TCP服务器开始监听: {}", address);
            
            // 发送监听事件到UI线程
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Listening(config.id.clone()));
            }
            
            // 创建监听任务
            let event_sender_clone = event_sender.clone();
            let id_clone = config.id.clone();
            let message_processor_clone = message_processor.clone();
            let clients_clone = clients;
            
            tokio::spawn(async move {
                while let Ok((mut socket, addr)) = listener.accept().await {
                    debug!("TCP服务器接收到来自 {} 的连接", addr);
                    
                    // 创建客户端连接的发送器和接收器
                    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
                    
                    // 保存客户端连接到共享的clients哈希表
                    let mut clients_guard = clients_clone.lock().await;
                    clients_guard.insert(addr, tx.clone());
                    drop(clients_guard);
                    
                    // 发送客户端连接事件到UI线程
                    if let Some(sender) = &event_sender_clone {
                        let _ = sender.send(ConnectionEvent::ServerClientConnected(
                            id_clone.clone(),
                            addr,
                            tx,
                        ));
                    }
                    
                    // 处理客户端连接
                    let client_addr = addr.clone();
                    let client_id_clone = id_clone.clone();
                    let client_event_sender = event_sender_clone.clone();
                    let client_message_processor = message_processor_clone.clone();
                    let clients_clone_for_disconnect = clients_clone.clone();
                    
                    tokio::spawn(async move {
                        // 读取客户端消息的循环
                        let mut buffer: [u8; 1024] = [0; 1024];
                        loop {
                            tokio::select! {
                            // 从客户端读取消息
                            result = socket.read(&mut buffer) => {
                                    match result {
                                        Ok(0) => {
                                            // 客户端关闭连接
                                            info!("TCP客户端 {} 断开连接", addr);
                                            
                                            // 从共享的clients哈希表中移除断开连接的客户端
                                            let mut clients_guard = clients_clone_for_disconnect.lock().await;
                                            clients_guard.remove(&addr);
                                            drop(clients_guard);
                                            
                                            // 发送客户端断开连接事件到UI线程
                                            if let Some(sender) = &client_event_sender {
                                                let _ = sender.send(ConnectionEvent::ServerClientDisconnected(
                                                    client_id_clone.clone(),
                                                    addr,
                                                ));
                                            }
                                            break;
                                        },
                                        Ok(n) => {
                                            // 处理接收到的消息
                                            let data = buffer[..n].to_vec();
                                            info!("TCP服务器从 {} 收到消息: {:?}", addr, data);
                                            
                                            // 创建消息对象
                                            let message = client_message_processor.process_received_message(
                                                data, 
                                                MessageType::Text
                                            ).with_source(addr.to_string());
                                            
                                            // 发送消息事件到UI线程
                                            if let Some(sender) = &client_event_sender {
                                                let _ = sender.send(ConnectionEvent::MessageReceived(
                                                    client_id_clone.clone(),
                                                    message,
                                                ));
                                            }
                                        },
                                        Err(e) => {
                                            // 处理读取错误
                                            error!("TCP服务器读取来自 {} 的消息时发生错误: {:?}", addr, e);
                                            
                                            // 从共享的clients哈希表中移除断开连接的客户端
                                            let mut clients_guard = clients_clone_for_disconnect.lock().await;
                                            clients_guard.remove(&addr);
                                            drop(clients_guard);
                                            
                                            // 发送错误事件到UI线程
                                            if let Some(sender) = &client_event_sender {
                                                let _ = sender.send(ConnectionEvent::Error(
                                                    client_id_clone.clone(),
                                                    format!("读取消息错误: {:?}", e),
                                                ));
                                            }
                                            break;
                                        },
                                    }
                                },
                                // 发送消息到客户端
                                Some(message) = rx.recv() => {
                                    match socket.write_all(&message).await {
                                        Ok(_) => {
                                            info!("TCP服务器向 {} 发送消息: {:?}", addr, message);
                                        },
                                        Err(e) => {
                                            error!("TCP服务器向 {} 发送消息时发生错误: {:?}", addr, e);
                                            
                                            // 从共享的clients哈希表中移除断开连接的客户端
                                            let mut clients_guard = clients_clone_for_disconnect.lock().await;
                                            clients_guard.remove(&addr);
                                            drop(clients_guard);
                                            
                                            break;
                                        },
                                    }
                                },
                            }
                        }
                    });
                }
            });
            
            Ok(())
        }))
    }
    
    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        Pin::from(Box::new(async move {
            // 简单返回Ok，因为我们不需要发送停止事件
            Ok(())
        }))
    }
    
    fn send_to_client(
        &mut self, 
        client_addr: SocketAddr, 
        data: Vec<u8>
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let data_clone = data;
        let clients = self.clients.clone();
        
        Pin::from(Box::new(async move {
            let clients_guard = clients.lock().await;
            if let Some(sender) = clients_guard.get(&client_addr) {
                sender.send(data_clone)?;
            } else {
                return Err("客户端不存在".into());
            }
            
            Ok(())
        }))
    }
    
    fn is_running(&self) -> bool {
        self.is_running
    }
}
