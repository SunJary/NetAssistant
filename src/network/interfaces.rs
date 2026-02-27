use std::future::Future;
use std::pin::Pin;
use smol::channel::Sender;
use crate::network::events::ConnectionEvent;

/// 网络连接接口
pub trait NetworkConnection: Send {
    /// 建立连接
    fn connect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
    
    /// 断开连接
    fn disconnect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
}

/// 网络服务器接口
pub trait NetworkServer: Send {
    /// 启动服务器
    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>>;
    
    /// 停止服务器
    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
}

/// 网络工厂接口
pub trait NetworkFactory {
    /// 创建客户端连接
    fn create_client(
        config: &crate::config::connection::ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>
    ) -> Box<dyn NetworkConnection> where Self: Sized;
    
    /// 创建服务器
    fn create_server(
        config: &crate::config::connection::ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>
    ) -> Box<dyn NetworkServer> where Self: Sized;
}
