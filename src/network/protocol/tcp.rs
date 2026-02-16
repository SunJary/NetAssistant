use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use log::{debug, error, info};
use std::pin::Pin;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

use tokio::task::JoinHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bytes::{BytesMut};
use crate::config::connection::{ClientConfig, ServerConfig};
use crate::message::MessageType;
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::core::message_processor::{MessageProcessor, DefaultMessageProcessor};
use crate::network::protocol::decoder::CodecFactory;

/// 处理解码后的数据，转换为消息并发送事件（客户端用）
fn process_decoded_data(
    data: BytesMut,
    processor: &Arc<dyn MessageProcessor>,
    event_sender: &Option<mpsc::UnboundedSender<ConnectionEvent>>,
    connection_id: &str
) {
    let raw_data: Vec<u8> = data.to_vec();
    let message = processor.process_received_message(raw_data, MessageType::Text);
    
    if let Some(sender) = event_sender {
        let _ = sender.send(ConnectionEvent::MessageReceived(connection_id.to_string(), message));
    }
}

/// 处理解码后的数据，转换为消息并发送事件（服务器端用，包含地址信息）
fn process_decoded_data_with_addr(
    data: BytesMut,
    processor: &Arc<dyn MessageProcessor>,
    event_sender: &Option<mpsc::UnboundedSender<ConnectionEvent>>,
    connection_id: &str,
    addr: &str
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
    info!("TCP服务器从 {} 收到消息: {}", addr, message_str);
    
    // 创建消息对象
    let message = processor.process_received_message(raw_data, MessageType::Text).with_source(addr.to_string());
    
    // 发送消息事件到UI线程
    if let Some(sender) = event_sender {
        let _ = sender.send(ConnectionEvent::MessageReceived(connection_id.to_string(), message));
    }
}

/// TCP客户端实现
pub struct TcpClient {
    config: ClientConfig,
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
            event_sender,
            message_processor: Arc::new(DefaultMessageProcessor),
            is_connected: false,
        }
    }
}

impl NetworkConnection for TcpClient {
    fn connect(&mut self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
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
            
            // 创建decoder和encoder
            let (mut socket_read, mut socket_write) = tokio::io::split(socket);
            
            // 启动接收消息任务
            let event_sender_clone = event_sender.clone();
            let config_clone = config.clone();
            let message_processor_clone = message_processor.clone();
            let decoder_config = config.decoder_config.clone();
            tokio::spawn(async move {
                let mut buffer = BytesMut::with_capacity(16384); // 16KB缓冲区
                
                // 使用CodecFactory创建解码器（所有解码器现在都支持force_flush）
                let mut decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&decoder_config);
                
                loop {
                    tokio::select! {
                        // 数据读取事件
                        result = socket_read.read_buf(&mut buffer) => {
                            match result {
                                Ok(0) => {
                                    // 连接关闭
                                    info!("TCP连接已关闭");
                                    break;
                                },
                                Ok(n) => {
                                    debug!("TCP客户端读取了 {} 字节数据", n);
                                    
                                    // 使用decoder解码数据，循环处理所有可用消息
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
                                                // 解码器需要更多数据，退出循环
                                                break;
                                            },
                                            Err(e) => {
                                                error!("TCP解码错误: {:?}", e);
                                                break;
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
                        
                        // 50ms超时事件 - 强制刷新缓冲区
                        _ = tokio::time::sleep(Duration::from_millis(50)) => {
                            // 强制刷新解码器缓冲区
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
                }
                
                // 发送断开连接事件
                if let Some(sender) = &event_sender_clone {
                    let _ = sender.send(ConnectionEvent::Disconnected(config_clone.id.clone()));
                }
            });
            
            // 启动发送消息任务
            let encoder_for_write = CodecFactory::create_encoder(&config.decoder_config);
            tokio::spawn(async move {
                let mut encoder = encoder_for_write;
                loop {
                    match rx.recv().await {
                        Some(data) => {
                            let mut buffer = BytesMut::with_capacity(data.len());
                            let data_bytes = BytesMut::from(data.as_slice());
                            
                            // 使用encoder编码数据
                            if let Err(e) = encoder.encode(data_bytes, &mut buffer) {
                                error!("TCP编码错误: {:?}", e);
                                break;
                            }
                            
                            // 写入数据
                            if let Err(e) = socket_write.write_all(&buffer).await {
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
    
    fn disconnect(&mut self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        // 在异步闭包外修改连接状态
        self.is_connected = false;
        
        let event_sender = self.event_sender.clone();
        let config = self.config.clone();
        
        Pin::from(Box::new(async move {
            // 发送断开连接事件
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Disconnected(config.id.clone()));
            }
            
            // 注意：socket已经被移动到其他任务中
            // 当socket_read/socket_write被drop时，连接会自动关闭
            // 任务会在socket关闭时自动退出循环
            
            Ok(())
        }))
    }
    

}

/// TCP服务器实现
pub struct TcpServer {
    config: ServerConfig,
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
    clients: Arc<Mutex<HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>>>,
    message_processor: Arc<dyn MessageProcessor>,
    is_running: bool,
    listener_handle: Option<JoinHandle<()>>,
    client_handles: Arc<Mutex<HashMap<SocketAddr, JoinHandle<()>>>>,
    listener: Option<Arc<TcpListener>>,
}

/// 实现Drop trait，确保资源被正确释放
impl Drop for TcpServer {
    fn drop(&mut self) {
        // 当服务器实例被销毁时，取消监听任务
        if let Some(handle) = self.listener_handle.take() {
            handle.abort();
            info!("TCP服务器监听任务已取消");
        }
    }
}

impl TcpServer {
    pub fn new(
        config: ServerConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Self {
        TcpServer {
            config,
            event_sender,
            clients: Arc::new(Mutex::new(HashMap::new())),
            message_processor: Arc::new(DefaultMessageProcessor),
            is_running: false,
            listener_handle: None,
            client_handles: Arc::new(Mutex::new(HashMap::new())),
            listener: None,
        }
    }
}

impl NetworkServer for TcpServer {
    fn start(&mut self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>> {
        // 如果服务器已经在运行，直接返回
        if self.is_running {
            info!("TCP服务器已经在运行中");
            return Pin::from(Box::new(async move { Ok(()) }));
        }
        
        // 绑定地址
        let address = format!("{}:{}", self.config.listen_address, self.config.listen_port);
        info!("TCP服务器启动在地址: {}", address);
        
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
        
        // 启动一个任务来创建listener并启动监听
        tokio::spawn(async move {
            // 绑定地址
            match TcpListener::bind(&address).await {
                Ok(listener) => {
                    info!("TCP服务器开始监听: {}", address);
                    
                    // 发送监听事件到UI线程
                    if let Some(sender) = &event_sender {
                        let _ = sender.send(ConnectionEvent::Listening(config.id.clone()));
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
                        async move { 
                            loop {
                                match listener_clone.accept().await {
                                    Ok((socket, addr)) => {
                                        debug!("TCP服务器接收到来自 {} 的连接", addr);
                                        
                                        // 创建客户端连接的发送器和接收器
                                        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
                                        
                                        // 保存客户端连接到共享的clients哈希表
                                        let mut clients_guard: tokio::sync::MutexGuard<'_, HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>> = clients.lock().await;
                                        clients_guard.insert(addr, tx.clone());
                                        drop(clients_guard);
                                        
                                        // 发送客户端连接事件到UI线程
                                        if let Some(sender) = &event_sender {
                                            let _ = sender.send(ConnectionEvent::ServerClientConnected(
                                                config.id.clone(),
                                                addr,
                                                tx,
                                            ));
                                        }
                                        
                                        // 处理客户端连接
                                        let client_id_clone = config.id.clone();
                                        let client_event_sender = event_sender.clone();
                                        let client_message_processor = message_processor.clone();
                                        let clients_clone_for_disconnect = clients.clone();
                                        let config_clone_for_client = config.clone();
                                        let client_handles_clone_for_client = client_handles.clone();
                                        
                                        // 创建客户端连接的任务句柄
                                        let client_task = tokio::spawn(async move { 
                                            // 创建decoder和encoder
                                            let (mut socket_read, mut socket_write) = tokio::io::split(socket);
                                            
                                            // 根据配置创建具体的解码器
                                            let decoder_config = config_clone_for_client.decoder_config.clone();
                                            let encoder = CodecFactory::create_encoder(&config_clone_for_client.decoder_config);
                                            
                                            // 启动接收消息循环
                                            let recv_fut = async { 
                                                let mut buffer = BytesMut::with_capacity(16384); // 16KB缓冲区
                                                
                                                // 使用CodecFactory创建解码器（所有解码器现在都支持force_flush）
                                                let mut decoder = crate::network::protocol::decoder::CodecFactory::create_decoder(&decoder_config);
                                                
                                                loop {
                                                    tokio::select! {
                                                        // 数据读取事件
                                                        result = socket_read.read_buf(&mut buffer) => {
                                                            match result {
                                                                Ok(0) => {
                                                                    // 客户端关闭连接
                                                                    info!("TCP客户端 {} 断开连接", addr);
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
                                                                },
                                                                Err(e) => {
                                                                    error!("TCP服务器读取来自 {} 的消息时发生错误: {:?}", addr, e);
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        
                                                        // 50ms超时事件 - 强制刷新缓冲区
                                                        _ = tokio::time::sleep(Duration::from_millis(50)) => {
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
                                                        Some(message) => {
                                                            let mut buffer = BytesMut::with_capacity(message.len());
                                                            let data_bytes = BytesMut::from(message.as_slice());
                                                            
                                                            // 使用encoder编码数据
                                                            if let Err(e) = encoder.encode(data_bytes, &mut buffer) {
                                                                error!("TCP服务器编码消息时发生错误: {:?}", e);
                                                                break;
                                                            }
                                                            
                                                            // 写入数据
                                                            if let Err(e) = socket_write.write_all(&buffer).await {
                                                                error!("TCP服务器向 {} 发送消息时发生错误: {:?}", addr, e);
                                                                break;
                                                            }
                                                            
                                                            // 尝试将消息转换为文本，如果失败则显示十六进制
                                                            let send_message_str = match String::from_utf8(message.clone()) {
                                                                Ok(s) => s,
                                                                Err(_) => {
                                                                    // 转换为十六进制
                                                                    let hex: Vec<String> = message.iter().map(|b| format!("{:02x}", b)).collect();
                                                                    hex.join(" ")
                                                                }
                                                            };
                                                            info!("TCP服务器向 {} 发送消息: {}", addr, send_message_str);
                                                        },
                                                        None => {
                                                            info!("TCP服务器发送消息通道已关闭");
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
                                            let mut clients_guard: tokio::sync::MutexGuard<'_, HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>> = clients_clone_for_disconnect.lock().await;
                                            clients_guard.remove(&addr);
                                            drop(clients_guard);
                                            
                                            // 从客户端任务句柄表中移除
                                            let mut handles_guard: tokio::sync::MutexGuard<'_, HashMap<SocketAddr, JoinHandle<()>>> = client_handles_clone_for_client.lock().await;
                                            handles_guard.remove(&addr);
                                            drop(handles_guard);
                                            
                                            // 发送客户端断开连接事件到UI线程
                                            if let Some(sender) = &client_event_sender {
                                                let _ = sender.send(ConnectionEvent::ServerClientDisconnected(
                                                    client_id_clone.clone(),
                                                    addr,
                                                ));
                                            }
                                        });
                                        
                                        // 保存客户端任务句柄到client_handles
                                        let client_handles_clone = client_handles.clone();
                                        let mut handles_guard: tokio::sync::MutexGuard<'_, HashMap<SocketAddr, JoinHandle<()>>> = client_handles_clone.lock().await;
                                        handles_guard.insert(addr, client_task);
                                        drop(handles_guard);
                                    },
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
                },
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
                            let _ = tx.send((Arc::new(TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|_| panic!("无法绑定任何TCP地址"))), tokio::spawn(async {})));
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
                },
                Err(e) => {
                    error!("TCP服务器无法从通道接收listener和task handle: {:?}", e);
                    // 更新状态为停止
                    self.is_running = false;
                    Err(Box::new(e) as Box<dyn std::error::Error>)
                }
            }
        }))
    }
    
    fn stop(&mut self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let event_sender = self.event_sender.clone();
        let server_id = self.config.id.clone();
        let clients = self.clients.clone();
        let client_handles = self.client_handles.clone();
        
        // 如果服务器已经停止，直接返回
        if !self.is_running {
            info!("TCP服务器已经停止");
            return Pin::from(Box::new(async move {
                Ok(())
            }));
        }
        
        // 取消监听任务
        if let Some(handle) = self.listener_handle.take() {
            handle.abort();
            info!("TCP服务器监听任务已取消");
        }
        
        // 关闭监听套接字
        if let Some(_listener) = self.listener.take() {
            // 当我们从self.listener中取出listener并drop它时，会自动关闭监听套接字
            // 这将导致所有正在进行的accept()调用返回错误，从而停止接收新连接
            info!("TCP服务器监听套接字已关闭");
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
            let mut handles_guard: tokio::sync::MutexGuard<'_, HashMap<SocketAddr, JoinHandle<()>>> = client_handles.lock().await;
            let handles = std::mem::take(&mut *handles_guard);
            drop(handles_guard);
            
            for (addr, handle) in handles {
                handle.abort();
                debug!("TCP服务器已取消客户端 {} 的连接任务", addr);
            }
            
            // 发送断开连接事件到UI线程
            if let Some(sender) = &event_sender {
                let _ = sender.send(ConnectionEvent::Disconnected(server_id));
            }
            
            info!("TCP服务器已停止");
            info!("TCP服务器已停止监听端口");
            Ok(())
        }))
    }
    

}
