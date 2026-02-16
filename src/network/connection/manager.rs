use std::collections::HashMap;
use tokio::sync::mpsc;
use crate::config::connection::{ClientConfig, ServerConfig, ConnectionType};
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkServer, NetworkFactory};
use crate::network::protocol::tcp::{TcpClient, TcpServer};
use crate::network::protocol::udp::{UdpClient, UdpServer};

/// 默认的网络工厂实现
pub struct DefaultNetworkFactory;
impl NetworkFactory for DefaultNetworkFactory {
    fn create_client(
        config: &ClientConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Box<dyn NetworkConnection> {
        match config.protocol {
            ConnectionType::Tcp => Box::new(TcpClient::new(config.clone(), event_sender)),
            ConnectionType::Udp => Box::new(UdpClient::new(config.clone(), event_sender)),
        }
    }
    
    fn create_server(
        config: &ServerConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Box<dyn NetworkServer> {
        match config.protocol {
            ConnectionType::Tcp => Box::new(TcpServer::new(config.clone(), event_sender)),
            ConnectionType::Udp => Box::new(UdpServer::new(config.clone(), event_sender)),
        }
    }
}

/// 网络连接管理器
pub struct NetworkConnectionManager {
    clients: HashMap<String, Box<dyn NetworkConnection>>,
    servers: HashMap<String, Box<dyn NetworkServer>>,
}

impl NetworkConnectionManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            servers: HashMap::new(),
        }
    }
    
    /// 创建并启动客户端连接
    pub async fn create_and_connect_client(
        &mut self,
        config: &ClientConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 如果连接已存在，则先断开
        if self.clients.contains_key(&config.id) {
            self.disconnect_client(&config.id).await?;
        }
        
        // 创建客户端连接
        let mut client = DefaultNetworkFactory::create_client(config, event_sender);
        
        // 连接到服务器
        let _ = client.connect().await;
        
        // 保存客户端连接
        self.clients.insert(config.id.clone(), client);
        
        Ok(())
    }
    
    /// 创建并启动服务器
    pub async fn create_and_start_server(
        &mut self,
        config: &ServerConfig,
        event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 如果服务器已存在，则先停止
        if self.servers.contains_key(&config.id) {
            self.stop_server(&config.id).await?;
        }
        
        // 创建服务器
        let server = DefaultNetworkFactory::create_server(config, event_sender);
        
        // 保存服务器到映射中
        self.servers.insert(config.id.clone(), server);
        
        // 从映射中获取服务器并启动
        if let Some(server) = self.servers.get_mut(&config.id) {
            let _ = server.start().await;
        }
        
        Ok(())
    }
    
    /// 断开客户端连接
    pub async fn disconnect_client(
        &mut self,
        client_id: &str
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut client) = self.clients.remove(client_id) {
            let _ = client.disconnect().await;
        }
        
        Ok(())
    }
    
    /// 停止服务器
    pub async fn stop_server(
        &mut self,
        server_id: &str
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut server) = self.servers.remove(server_id) {
            let _ = server.stop().await;
        }
        
        Ok(())
    }
    

}
