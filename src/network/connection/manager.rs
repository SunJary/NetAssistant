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

    /// 功能测试: TCP 客户端配置本地绑定后, 连接必须从指定的本地地址与端口发起。
    #[tokio::test]
    async fn test_tcp_client_local_bind_port() {
        // 服务端
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // 客户端要固定的本地端口(listen socket 直接 drop, 无 TIME_WAIT, 可立即复用)
        let local_port = {
            let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            probe.local_addr().unwrap().port()
        };

        // 服务端先挂起 accept, 避免客户端 connect 后读任务无人对接
        let accept_task = tokio::spawn(async move { listener.accept().await });

        let mut manager = NetworkConnectionManager::new();
        let config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Tcp,
            server_address: server_addr.ip().to_string(),
            server_port: server_addr.port(),
            local_address: Some("127.0.0.1".to_string()),
            local_port: Some(local_port),
            ..Default::default()
        };
        manager
            .create_and_connect_client(&config, None)
            .await
            .expect("配置本地绑定后 TCP 连接应成功");

        let (_, peer) = accept_task.await.unwrap().unwrap();
        assert_eq!(
            peer.port(),
            local_port,
            "服务端看到的客户端源端口应为指定的本地端口"
        );
        assert_eq!(peer.ip().to_string(), "127.0.0.1");
    }

    /// 功能测试: 本地地址族与远端不一致时必须返回明确错误, 而非 panic 或静默失败。
    #[tokio::test]
    async fn test_tcp_client_local_bind_family_mismatch_returns_error() {
        let mut manager = NetworkConnectionManager::new();
        let config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Tcp,
            // IPv4 远端 + IPv6 本地地址 → 族校验先失败, 不会真正发起连接
            server_address: "127.0.0.1".to_string(),
            server_port: 9,
            local_address: Some("::1".to_string()),
            ..Default::default()
        };
        let result = manager.create_and_connect_client(&config, None).await;
        let msg = result.expect_err("地址族不一致应返回错误").to_string();
        assert!(
            msg.contains("地址族不一致"),
            "错误信息应提示地址族不一致, 实际: {}",
            msg
        );
    }

    /// 功能测试: UDP 客户端固定本地端口后, Connected 事件携带的本地端点端口应为配置值。
    #[tokio::test]
    async fn test_udp_client_local_bind_port() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;

        let local_port = {
            let probe = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            probe.local_addr().unwrap().port()
        };

        let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut manager = NetworkConnectionManager::new();
        let config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Udp,
            server_address: "127.0.0.1".to_string(),
            server_port: 9, // discard 端口, UDP 无真实连接, 仅需可达性无关的合法地址
            local_address: Some("127.0.0.1".to_string()),
            local_port: Some(local_port),
            ..Default::default()
        };
        manager
            .create_and_connect_client(&config, Some(tx))
            .await
            .expect("配置本地绑定后 UDP 客户端应启动成功");

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("应在超时前收到事件")
            .expect("事件通道不应关闭");
        match event {
            ConnectionEvent::Connected(_, local_addr) => {
                assert_eq!(local_addr.port(), local_port);
                assert_eq!(local_addr.ip().to_string(), "127.0.0.1");
            }
            other => panic!("首个事件应为 Connected, 实际: {:?}", other),
        }
    }

    /// 兼容测试: 未配置本地绑定时 UDP 客户端保持旧行为(系统自动分配临时端口)。
    #[tokio::test]
    async fn test_udp_client_without_local_bind_gets_ephemeral_port() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;

        let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut manager = NetworkConnectionManager::new();
        let config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Udp,
            server_address: "127.0.0.1".to_string(),
            server_port: 9,
            ..Default::default()
        };
        manager
            .create_and_connect_client(&config, Some(tx))
            .await
            .expect("未配置本地绑定时 UDP 客户端应启动成功");

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("应在超时前收到事件")
            .expect("事件通道不应关闭");
        match event {
            ConnectionEvent::Connected(_, local_addr) => {
                assert!(
                    local_addr.port() > 0,
                    "自动分配的临时端口应大于 0, 实际: {}",
                    local_addr
                );
            }
            other => panic!("首个事件应为 Connected, 实际: {:?}", other),
        }
    }
}
