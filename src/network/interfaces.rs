use std::future::Future;
use std::pin::Pin;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use crate::message::Message;
use crate::network::events::ConnectionEvent;

/// 网络连接接口
pub trait NetworkConnection: Send {
    /// 建立连接
    fn connect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
    
    /// 断开连接
    fn disconnect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
    
    /// 发送消息
    fn send_message(&mut self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
    
    /// 接收消息
    fn receive_message(&mut self) -> Pin<Box<dyn Future<Output = Result<Message, Box<dyn std::error::Error>>> + Send>>;
    
    /// 获取连接状态
    fn is_connected(&self) -> bool;
}

/// 网络服务器接口
pub trait NetworkServer: Send {
    /// 启动服务器
    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>>;
    
    /// 停止服务器
    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
    
    /// 发送消息给指定客户端
    fn send_to_client(
        &mut self, 
        client_addr: SocketAddr, 
        data: Vec<u8>
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
    
    /// 获取所有连接的客户端地址
    fn get_connected_clients(&self) -> Vec<SocketAddr>;
    
    /// 检查服务器是否正在运行
    fn is_running(&self) -> bool;
}

/// 网络工厂接口
pub trait NetworkFactory {
    /// 创建客户端连接
    fn create_client(
        config: &crate::config::connection::ClientConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Box<dyn NetworkConnection> where Self: Sized;
    
    /// 创建服务器
    fn create_server(
        config: &crate::config::connection::ServerConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Box<dyn NetworkServer> where Self: Sized;
}
