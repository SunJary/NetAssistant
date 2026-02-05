use std::collections::HashMap;
use std::net::SocketAddr;
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
    network_factory: DefaultNetworkFactory,
}

impl NetworkConnectionManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            servers: HashMap::new(),
            network_factory: DefaultNetworkFactory,
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
        let mut server = DefaultNetworkFactory::create_server(config, event_sender);
        
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
    
    /// 向客户端发送消息
    pub async fn send_message_to_client(
        &mut self,
        client_id: &str,
        data: Vec<u8>
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(client) = self.clients.get_mut(client_id) {
            client.send_message(data).await?;
        } else {
            return Err(format!("客户端不存在: {}", client_id).into());
        }
        
        Ok(())
    }
    
    /// 向服务器客户端发送消息
    pub async fn send_message_to_server_client(
        &mut self,
        server_id: &str,
        client_addr: SocketAddr,
        data: Vec<u8>
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(server) = self.servers.get_mut(server_id) {
            server.send_to_client(client_addr, data).await?;
        } else {
            return Err(format!("服务器不存在: {}", server_id).into());
        }
        
        Ok(())
    }
    
    /// 检查客户端是否已连接
    pub fn is_client_connected(&self, client_id: &str) -> bool {
        if let Some(client) = self.clients.get(client_id) {
            client.is_connected()
        } else {
            false
        }
    }
    
    /// 检查服务器是否正在运行
    pub fn is_server_running(&self, server_id: &str) -> bool {
        if let Some(server) = self.servers.get(server_id) {
            server.is_running()
        } else {
            false
        }
    }
    
    /// 获取所有客户端连接的ID
    pub fn get_all_client_ids(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }
    
    /// 获取所有服务器的ID
    pub fn get_all_server_ids(&self) -> Vec<String> {
        self.servers.keys().cloned().collect()
    }
    
    /// 获取服务端所有连接的客户端地址
    pub fn get_server_client_addresses(&self, server_id: &str) -> Vec<SocketAddr> {
        // 注意：这个方法需要NetworkServer trait提供获取客户端地址的方法
        // 目前NetworkServer trait没有这个方法，所以返回空列表
        // 后续需要扩展NetworkServer trait来支持这个功能
        Vec::new()
    }
    
    /// 断开所有客户端连接
    pub async fn disconnect_all_clients(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let client_ids: Vec<String> = self.clients.keys().cloned().collect();
        
        for client_id in client_ids {
            self.disconnect_client(&client_id).await?;
        }
        
        Ok(())
    }
    
    /// 停止所有服务器
    pub async fn stop_all_servers(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let server_ids: Vec<String> = self.servers.keys().cloned().collect();
        
        for server_id in server_ids {
            self.stop_server(&server_id).await?;
        }
        
        Ok(())
    }
}
