use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use log::{debug, error, info};
use tokio::sync::Mutex;
use std::pin::Pin;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use crate::config::connection::{ClientConfig, ServerConfig};
use crate::message::MessageType;
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::core::message_processor::{MessageProcessor, DefaultMessageProcessor};

/// UDP客户端实现
pub struct UdpClient {
    config: ClientConfig,
    server_addr: SocketAddr,
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_connected: bool,
}

impl UdpClient {
    pub fn new(
        config: ClientConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Self {
        let server_addr = format!("{}:{}", config.server_address, config.server_port)
            .parse::<SocketAddr>()
            .expect("无效的UDP服务器地址");
            
        UdpClient {
            config,
            server_addr,
            event_sender,
            message_processor: Arc::new(DefaultMessageProcessor),
            is_connected: false,
        }
    }
}

impl NetworkConnection for UdpClient {
    fn connect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        // 如果已经连接，直接返回
        if self.is_connected {
            return Pin::from(Box::new(async move {
                Ok(())
            }));
        }
        
        let config = self.config.clone();
        let server_addr = self.server_addr;
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        
        // 创建一个Arc<Mutex<bool>>来共享连接状态
        let is_connected_flag = Arc::new(std::sync::Mutex::new(true));
        
        // 更新连接状态
        self.is_connected = true;
        
        Pin::from(Box::new(async move {
            info!("UDP客户端连接到地址: {}", server_addr);
            
            // 绑定到本地随机端口
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            let local_addr = socket.local_addr()
                .map_err(|e| {
                    error!("获取UDP套接字本地地址失败: {:?}", e);
                    e
                })?;
            info!("UDP客户端绑定到本地端口: {:?}", local_addr);
            
            // 尝试发送一个空的测试数据包，检查是否能够到达服务端
            // 注意：UDP是无连接的，这个测试可能不会失败，即使服务端不存在
            // 我们只能检查是否有网络错误
            // if let Err(e) = socket.send_to(&[], &server_addr).await {
            //     error!("UDP客户端无法发送测试数据到服务端: {:?}", e);
            //     return Err(format!("无法连接到UDP服务端: {}", e).into());
            // }
            
            // 创建发送器和接收器
            let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
            
            // 发送连接成功事件到UI线程
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Connected(config.id.clone()));
                let _ = sender.send(ConnectionEvent::ClientWriteSenderReady(config.id.clone(), tx));
            }
            
            // 将socket包装在Arc中以便共享
            let shared_socket = Arc::new(socket);
            let socket_read = shared_socket.clone();
            let socket_write = shared_socket.clone();
            
            // 创建消息接收任务
            let event_sender_clone = event_sender.clone();
            let id_clone = config.id.clone();
            let message_processor_clone = message_processor.clone();
            let is_connected_flag_read = is_connected_flag.clone();
            
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                loop {
                    // 检查连接状态
                    let is_connected = match is_connected_flag_read.lock() {
                        Ok(guard) => *guard,
                        Err(e) => {
                            error!("获取UDP连接状态锁失败: {:?}", e);
                            break;
                        }
                    };
                    if !is_connected {
                        break;
                    }
                    
                    match socket_read.recv_from(&mut buffer).await {
                        Ok((n, addr)) => {
                            // 只处理来自目标服务器的消息
                            if addr == server_addr {
                                let raw_data = buffer[..n].to_vec();
                                let message = message_processor_clone.process_received_message(raw_data, MessageType::Text);
                                
                                if let Some(sender) = &event_sender_clone {
                                    let _ = sender.send(ConnectionEvent::MessageReceived(id_clone.clone(), message));
                                }
                            }
                        },
                        Err(e) => {
                            error!("UDP读取错误: {:?}", e);
                            // 发送断开连接事件
                            if let Some(sender) = &event_sender_clone {
                                let _ = sender.send(ConnectionEvent::Disconnected(id_clone.clone()));
                            }
                            break;
                        },
                    }
                }
            });
            
            // 创建消息发送任务
            let event_sender_clone_write = event_sender.clone();
            let id_clone_write = config.id.clone();
            let is_connected_flag_write = is_connected_flag.clone();
            
            tokio::spawn(async move {
                while let Some(data) = rx.recv().await {
                    // 检查连接状态
                    let is_connected = match is_connected_flag_write.lock() {
                        Ok(guard) => *guard,
                        Err(e) => {
                            error!("获取UDP连接状态锁失败: {:?}", e);
                            break;
                        }
                    };
                    if !is_connected {
                        break;
                    }
                    
                    if let Err(e) = socket_write.send_to(&data, &server_addr).await {
                        error!("UDP发送错误: {:?}", e);
                        // 发送断开连接事件
                        if let Some(sender) = &event_sender_clone_write {
                            let _ = sender.send(ConnectionEvent::Disconnected(id_clone_write.clone()));
                        }
                        break;
                    }
                }
            });
            
            Ok(())
        }))
    }
    
    fn disconnect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        // 如果没有连接，直接返回
        if !self.is_connected {
            return Pin::from(Box::new(async move {
                Ok(())
            }));
        }
        
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        
        // 更新连接状态
        self.is_connected = false;
        
        Pin::from(Box::new(async move {
            // 发送断开连接事件
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Disconnected(config.id.clone()));
            }
            
            // 注意：UDP套接字会在socket_read/socket_write被drop时自动关闭
            // 任务会在socket关闭时自动退出循环
            
            Ok(())
        }))
    }
    

}

/// UDP服务器实现
pub struct UdpServer {
    config: ServerConfig,
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
    clients: Arc<Mutex<HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_running: bool,
    read_handle: Option<JoinHandle<()>>,
    write_handle: Option<JoinHandle<()>>,
}

impl UdpServer {
    pub fn new(
        config: ServerConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Self {
        UdpServer {
            config,
            event_sender,
            clients: Arc::new(Mutex::new(HashMap::new())),
            message_processor: Arc::new(DefaultMessageProcessor),
            is_running: false,
            read_handle: None,
            write_handle: None,
        }
    }
}

impl NetworkServer for UdpServer {
    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        
        // 使用现有的clients字段
        let clients = self.clients.clone();
        
        Pin::from(Box::new(async move {
            let address = format!("{}:{}", config.listen_address, config.listen_port);
            info!("UDP服务器启动在地址: {}", address);
            debug!("UDP服务器配置: {:?}", config);
            
            let socket = UdpSocket::bind(&address).await?;
            info!("UDP服务器成功绑定到地址: {}", address);
            debug!("UDP套接字创建成功: {:?}", socket);
            
            // 使用Arc来共享socket，解决移动问题
            let socket_arc = Arc::new(socket);
            
            // 发送监听事件到UI线程
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Listening(config.id.clone()));
            }
            
            // 创建发送器和接收器
            let (tx, mut rx) = mpsc::unbounded_channel::<(SocketAddr, Vec<u8>)>();
            
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
                                let (client_tx, mut client_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                                
                                // 保存客户端信息
                                clients_guard.insert(addr, client_tx.clone());
                                drop(clients_guard);
                                
                                // 处理从UI来的消息
                                let tx_clone = tx.clone();
                                let addr_clone = addr.clone();
                                tokio::spawn(async move {
                                    while let Some(data) = client_rx.recv().await {
                                        if tx_clone.send((addr_clone, data)).is_err() {
                                            break;
                                        }
                                    }
                                });
                                
                                // 发送客户端连接事件到UI线程
                                if let Some(sender) = &event_sender_clone {
                                    let _ = sender.send(ConnectionEvent::ServerClientConnected(
                                        id_clone.clone(),
                                        addr,
                                        client_tx,
                                    ));
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
                                let _ = sender.send(ConnectionEvent::MessageReceived(
                                    id_clone.clone(),
                                    message,
                                ));
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
                loop {
                    if let Some((addr, message)) = rx.recv().await {
                        if let Err(e) = socket_write.send_to(&message, addr).await {
                            error!("UDP服务器发送消息时发生错误: {:?}", e);
                        } else {
                            info!("UDP服务器向 {} 发送消息: {:?}", addr, message);
                        }
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
            // 清空客户端列表
            let mut clients_guard = clients.lock().await;
            clients_guard.clear();
            drop(clients_guard);
            
            // 发送断开连接事件
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Disconnected(server_id));
            }
            
            info!("UDP服务器已停止");
            Ok(())
        }))
    }
    

}
