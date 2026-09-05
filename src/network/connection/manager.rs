use crate::config::connection::{ClientConfig, ConnectionType, ServerConfig};
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkFactory, NetworkServer};
use crate::network::protocol::tcp::{TcpClient, TcpServer};
use crate::network::protocol::udp::{UdpClient, UdpServer};
use smol::channel::Sender;
use std::collections::HashMap;
use std::net::SocketAddr;

/// 默认的网络工厂实现
pub struct DefaultNetworkFactory;
impl NetworkFactory for DefaultNetworkFactory {
    fn create_client(
        config: &ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
    ) -> Box<dyn NetworkConnection> {
        match config.protocol {
            ConnectionType::Tcp => Box::new(TcpClient::new(config.clone(), event_sender)),
            ConnectionType::Udp => Box::new(UdpClient::new(config.clone(), event_sender)),
        }
    }

    fn create_server(
        config: &ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
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
        event_sender: Option<Sender<ConnectionEvent>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 如果连接已存在，则先断开
        if self.clients.contains_key(&config.id) {
            self.disconnect_client(&config.id).await?;
        }

        // 创建客户端连接
        let mut client = DefaultNetworkFactory::create_client(config, event_sender);

        // 连接到服务器(失败直接返回错误,由 UI 层提示;吞掉会导致 tab 永远停在"连接中")
        client.connect().await?;

        // 保存客户端连接
        self.clients.insert(config.id.clone(), client);

        Ok(())
    }

    /// 创建并启动服务器
    pub async fn create_and_start_server(
        &mut self,
        config: &ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 如果服务器已存在，则先停止
        if self.servers.contains_key(&config.id) {
            self.stop_server(&config.id).await?;
        }

        // 创建服务器
        let server = DefaultNetworkFactory::create_server(config, event_sender);

        // 保存服务器到映射中
        self.servers.insert(config.id.clone(), server);

        // 从映射中获取服务器并启动(绑定失败等错误直接返回,不再吞掉——
        // 否则端口被其他实例占用时 UI 无任何提示,表现为"在监听但收不到消息")
        let start_result = if let Some(server) = self.servers.get_mut(&config.id) {
            server.start().await
        } else {
            Ok(())
        };
        if start_result.is_err() {
            // 启动失败的服务端没有运行中的任务,直接移除避免残留
            self.servers.remove(&config.id);
        }
        start_result
    }

    /// 断开客户端连接
    pub async fn disconnect_client(
        &mut self,
        client_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut client) = self.clients.remove(client_id) {
            let _ = client.disconnect().await;
        }

        Ok(())
    }

    /// 停止服务器
    pub async fn stop_server(&mut self, server_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut server) = self.servers.remove(server_id) {
            let _ = server.stop().await;
        }

        Ok(())
    }

    /// 手动向UDP服务端添加客户端地址
    /// 仅对UDP协议有效，TCP服务端不支持此操作
    pub async fn add_udp_client(
        &self,
        server_id: &str,
        addr: SocketAddr,
    ) -> Result<Sender<Vec<u8>>, String> {
        let server = self
            .servers
            .get(server_id)
            .ok_or_else(|| format!("服务器 {} 不存在", server_id))?;

        let any_ref = (**server).as_any();

        if let Some(udp_server) = any_ref.downcast_ref::<UdpServer>() {
            udp_server.add_client(addr).await
        } else {
            Err("仅UDP服务端支持手动添加客户端".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection::ConnectionType;

    /// 回归测试: 端口被占用时 create_and_start_server 必须返回错误。
    ///
    /// 此前 start() 的错误被 `let _ =` 吞掉,UI 无任何提示——多实例监听同一
    /// UDP 端口时表现为"两个实例都显示在监听,但都收不到消息"。
    #[tokio::test]
    async fn test_start_server_port_conflict_returns_error() {
        // 先占用一个随机端口
        let holder = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = holder.local_addr().unwrap().port();

        let mut manager = NetworkConnectionManager::new();
        let config = ServerConfig {
            protocol: ConnectionType::Udp,
            listen_address: "127.0.0.1".to_string(),
            listen_port: port,
            ..Default::default()
        };

        let result = manager.create_and_start_server(&config, None).await;
        assert!(
            result.is_err(),
            "端口被占用时启动服务端应返回错误,实际: {:?}",
            result
        );
    }

    /// 回归测试: 客户端连接失败必须返回错误(此前同样被 `let _ =` 吞掉)。
    #[tokio::test]
    async fn test_connect_client_failure_returns_error() {
        // 取一个当前无人监听的 TCP 端口: 先 bind 再 drop
        let port = {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().port()
        };

        let mut manager = NetworkConnectionManager::new();
        let config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Tcp,
            server_address: "127.0.0.1".to_string(),
            server_port: port,
            ..Default::default()
        };
        let result = manager.create_and_connect_client(&config, None).await;
        assert!(result.is_err(), "连接被拒绝时应返回错误,实际: {:?}", result);
    }
}
