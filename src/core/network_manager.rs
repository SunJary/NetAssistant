use crate::config::connection::{ConnectionConfig, ConnectionType, ClientConfig, ServerConfig};
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use tokio::sync::mpsc;
use std::net::SocketAddr;

#[derive(Clone)]
pub struct NetworkConnectionManager {
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
}

impl NetworkConnectionManager {
    pub fn new(event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>) -> Self {
        NetworkConnectionManager {
            event_sender,
        }
    }
    
    /// 建立TCP客户端连接
    pub async fn connect_tcp_client(&self, tab_id: String, client_config: ClientConfig) -> Result<(), Box<dyn std::error::Error>> {
        use crate::network::protocol::tcp::TcpClient;
        
        let mut tcp_client = TcpClient::new(
            client_config,
            self.event_sender.clone()
        );
        
        // 连接TCP客户端
        tcp_client.connect().await?;
        
        Ok(())
    }
    
    /// 建立UDP客户端连接
    pub async fn connect_udp_client(&self, tab_id: String, client_config: ClientConfig) -> Result<(), Box<dyn std::error::Error>> {
        use crate::network::protocol::udp::UdpClient;
        
        let mut udp_client = UdpClient::new(
            client_config,
            self.event_sender.clone()
        );
        
        // 连接UDP客户端
        udp_client.connect().await?;
        
        Ok(())
    }
    
    /// 启动TCP服务端
    pub async fn start_tcp_server(&self, tab_id: String, server_config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
        use crate::network::protocol::tcp::TcpServer;
        
        let mut tcp_server = TcpServer::new(
            server_config,
            self.event_sender.clone()
        );
        
        // 启动TCP服务端
        tcp_server.start().await?;
        
        Ok(())
    }
    
    /// 启动UDP服务端
    pub async fn start_udp_server(&self, tab_id: String, server_config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
        use crate::network::protocol::udp::UdpServer;
        
        let mut udp_server = UdpServer::new(
            server_config,
            self.event_sender.clone()
        );
        
        // 启动UDP服务端
        udp_server.start().await?;
        
        Ok(())
    }
}
