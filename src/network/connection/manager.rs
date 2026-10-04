use crate::config::connection::{ClientConfig, ConnectionType, ServerConfig, TrailerSetting};
use crate::network::events::ConnectionEvent;
use crate::network::interfaces::{NetworkConnection, NetworkFactory, NetworkServer};
use crate::network::protocol::tcp::{TcpClient, TcpServer};
use crate::network::protocol::udp::{UdpClient, UdpServer};
use crate::reply::ReplyRulesStore;
use smol::channel::Sender;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

/// 默认的网络工厂实现
pub struct DefaultNetworkFactory;
impl NetworkFactory for DefaultNetworkFactory {
    fn create_client(
        config: &ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<crate::network::events::NetCounters>,
        trailer: TrailerSetting,
        reply_rules: Arc<ReplyRulesStore>,
    ) -> Box<dyn NetworkConnection> {
        match config.protocol {
            ConnectionType::Tcp => Box::new(TcpClient::new(
                config.clone(),
                event_sender,
                net_counters,
                trailer,
                reply_rules,
            )),
            ConnectionType::Udp => Box::new(UdpClient::new(
                config.clone(),
                event_sender,
                net_counters,
                trailer,
                reply_rules,
            )),
        }
    }

    fn create_server(
        config: &ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<crate::network::events::NetCounters>,
        trailer: TrailerSetting,
    ) -> Box<dyn NetworkServer> {
        match config.protocol {
            ConnectionType::Tcp => Box::new(TcpServer::new(
                config.clone(),
                event_sender,
                net_counters,
                trailer,
            )),
            ConnectionType::Udp => Box::new(UdpServer::new(
                config.clone(),
                event_sender,
                net_counters,
                trailer,
            )),
        }
    }
}

/// 网络连接管理器
pub struct NetworkConnectionManager {
    clients: HashMap<String, Box<dyn NetworkConnection>>,
    servers: HashMap<String, Box<dyn NetworkServer>>,
    /// 客户端侧回复规则共享状态（进程级唯一，所有客户端连接共享同一份规则集）
    ///
    /// 规则是**全局知识资产**（决策 D-2），不属于某个 tab，因此这里持一份并在
    /// 每次建连时注入给客户端。
    reply_rules: Arc<ReplyRulesStore>,
}

impl NetworkConnectionManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            servers: HashMap::new(),
            reply_rules: ReplyRulesStore::new(),
        }
    }

    /// 注入 UI 侧的规则集（与 UI 共享同一份 store，改动即时生效）
    pub fn with_reply_rules(mut self, store: Arc<ReplyRulesStore>) -> Self {
        self.reply_rules = store;
        self
    }

    /// 创建并启动客户端连接(不注入计数器, 用于回归测试)
    #[allow(dead_code)]
    pub async fn create_and_connect_client(
        &mut self,
        config: &ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.create_and_connect_client_with_counters(
            config,
            event_sender,
            None,
            TrailerSetting::default(),
        )
        .await
    }

    /// 创建并启动客户端连接(携带网络层精确计数器)
    pub async fn create_and_connect_client_with_counters(
        &mut self,
        config: &ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<crate::network::events::NetCounters>,
        trailer: TrailerSetting,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 如果连接已存在，则先断开
        if self.clients.contains_key(&config.id) {
            self.disconnect_client(&config.id).await?;
        }

        // 创建客户端连接（注入共享的回复规则集）
        let mut client = DefaultNetworkFactory::create_client(
            config,
            event_sender,
            net_counters,
            trailer,
            self.reply_rules.clone(),
        );

        // 连接到服务器(失败直接返回错误,由 UI 层提示;吞掉会导致 tab 永远停在"连接中")
        client.connect().await?;

        // 保存客户端连接
        self.clients.insert(config.id.clone(), client);

        Ok(())
    }

    /// 创建并启动服务端(不注入计数器, 用于回归测试)
    #[allow(dead_code)]
    pub async fn create_and_start_server(
        &mut self,
        config: &ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.create_and_start_server_with_counters(
            config,
            event_sender,
            None,
            TrailerSetting::default(),
        )
        .await
    }

    /// 创建并启动服务器(携带网络层精确计数器)
    pub async fn create_and_start_server_with_counters(
        &mut self,
        config: &ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<crate::network::events::NetCounters>,
        trailer: TrailerSetting,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 如果服务器已存在，则先停止
        if self.servers.contains_key(&config.id) {
            self.stop_server(&config.id).await?;
        }

        // 创建服务器
        let server =
            DefaultNetworkFactory::create_server(config, event_sender, net_counters, trailer);

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
    ) -> Result<Sender<crate::network::events::WireMessage>, String> {
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

    /// 回归测试: TCP 服务端端口被占用时必须返回错误。
    ///
    /// 此前 TCP bind 失败会静默回退绑定 `127.0.0.1:0` 随机端口,start() 仍返回
    /// Ok,UI 停在"连接中"且无法收发;回退也失败时甚至 panic。
    #[tokio::test]
    async fn test_start_tcp_server_port_conflict_returns_error() {
        // 先占用一个 TCP 端口并保持存活
        let holder = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = holder.local_addr().unwrap().port();

        let mut manager = NetworkConnectionManager::new();
        let config = ServerConfig {
            protocol: ConnectionType::Tcp,
            listen_address: "127.0.0.1".to_string(),
            listen_port: port,
            ..Default::default()
        };

        let result = manager.create_and_start_server(&config, None).await;
        assert!(
            result.is_err(),
            "TCP端口被占用时启动服务端应返回错误,实际: {:?}",
            result
        );
    }

    /// 回归测试: TCP 服务端正常启动路径(listener 真实绑定、可被连接、stop 后端口释放)。
    /// 锁定 start() 重构后的行为: bind 在 future 内执行,成功后 accept 任务就绪。
    #[tokio::test]
    async fn test_start_tcp_server_success_and_stop_releases_port() {
        // 取一个空闲端口
        let port = {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().port()
        };

        let mut manager = NetworkConnectionManager::new();
        let config = ServerConfig {
            protocol: ConnectionType::Tcp,
            listen_address: "127.0.0.1".to_string(),
            listen_port: port,
            ..Default::default()
        };

        manager
            .create_and_start_server(&config, None)
            .await
            .expect("空闲端口启动 TCP 服务端应成功");

        // listener 已真实绑定: 原始 TCP 连接应能接入(accept 任务已在收)
        let _conn = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("应能连接到已启动的 TCP 服务端");

        // 停止后端口应释放: 重新绑定同一端口应成功。
        // 注: stop() 的 handle.abort() 是异步生效的,accept 任务持有的 listener Arc
        // 要等调度器下次运行时才随任务一起 drop,这里让出一个调度点。
        manager.stop_server(&config.id).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let rebind = tokio::net::TcpListener::bind(("127.0.0.1", port)).await;
        assert!(
            rebind.is_ok(),
            "stop 后端口应被释放,实际: {:?}",
            rebind.err()
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

    /// 端到端回归: TCP 服务端广播消息, 客户端 tab 必须通过事件通道收到。
    ///
    /// 完整复现用户场景: 服务端 tab(Bytes 解码器)广播 'ee' 给所有已连接客户端,
    /// 同实例客户端 tab(Bytes 解码器)应收到 MessagesReceived 事件。
    /// 验证链路: 服务端 clients 通道 → 服务端发送任务(encode+write_all)
    /// → 客户端读任务(decode) → MessagesReceived 事件。
    #[tokio::test]
    async fn test_tcp_server_broadcast_client_receives() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;

        let port = {
            let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            probe.local_addr().unwrap().port()
        };

        let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut manager = NetworkConnectionManager::new();

        let server_config = ServerConfig {
            protocol: ConnectionType::Tcp,
            listen_address: "127.0.0.1".to_string(),
            listen_port: port,
            ..Default::default()
        };
        manager
            .create_and_start_server(&server_config, Some(tx.clone()))
            .await
            .expect("服务端应启动成功");

        let client_config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Tcp,
            server_address: "127.0.0.1".to_string(),
            server_port: port,
            ..Default::default()
        };
        manager
            .create_and_connect_client(&client_config, Some(tx))
            .await
            .expect("客户端应连接成功");

        // 等待服务端 accept: ServerClientConnected 携带该客户端的写入发送器
        let mut write_sender = None;
        loop {
            let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("等待 ServerClientConnected 超时")
                .expect("事件通道不应关闭");
            match event {
                ConnectionEvent::ServerClientConnected(_, _, sender) => {
                    write_sender = Some(sender);
                    break;
                }
                _ => continue,
            }
        }

        // 模拟 app.rs 广播路径: 向写入发送器投递消息, 由服务端发送任务写出
        write_sender
            .unwrap()
            .send(crate::network::events::WireMessage::inherit(b"ee".to_vec()))
            .await
            .expect("投递到服务端发送通道应成功");

        // 客户端必须在超时前收到 MessagesReceived, 且路由到客户端 tab。
        // 注: 网络层读路径自批量事件重构后只投递 MessagesReceived(整批), 不再逐条投递
        // MessageReceived —— 后者仅由 UI 侧本地回显使用。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, rx.recv())
                .await
                .expect("等待客户端 MessagesReceived 超时")
                .expect("事件通道不应关闭");
            if let ConnectionEvent::MessagesReceived(tab_id, batch) = event {
                assert_eq!(tab_id, client_config.id, "消息应路由到客户端 tab");
                let message = batch.messages.first().expect("应保留消息明细");
                assert_eq!(
                    &*message.raw_data,
                    &b"ee"[..],
                    "客户端收到的字节应与服务端发送一致"
                );
                assert_eq!(
                    message.direction,
                    crate::message::MessageDirection::Received
                );
                return;
            }
        }
    }

    /// 回归测试: 解码器控制通道被丢弃后, 读任务必须继续正常收包, 不得忙转独占运行时。
    ///
    /// 此前 `decoder_control_rx.recv()` 在发送端被 drop 后恒为就绪的 Err(async-channel
    /// 语义: 通道为空且无发送端 → 立即返回 Err), 而分支既不 break 也不 await → select!
    /// 每轮命中且全程无让出点 → 单次 poll 永不返回 Pending → 运行时被独占: 读任务收不到
    /// 后续数据, 表现为"已连接但永远收不到消息"。
    ///
    /// 场景放在独立线程的 current_thread 运行时里执行, 由本线程用 `recv_timeout` 看门狗判定
    /// 结果。两点都是必需的:
    /// - 必须 current_thread: 忙转的危害正是"poll 永不返回 Pending → 同运行时的其他任务全被
    ///   饿死"(测试主体本身也会被饿死), 多线程运行时下主体跑在别的线程上, 反而测不出来;
    /// - 超时必须在被测运行时之外: 忙转会把该运行时的定时器一并饿死(时间驱动需工作线程 park
    ///   才能推进, 实测 multi_thread 亦如此), 否则回归时只会无声卡死而非明确失败。
    #[test]
    fn test_tcp_client_read_task_survives_closed_decoder_control_channel() {
        use crate::network::events::ConnectionEvent;
        use std::sync::mpsc;
        use std::time::Duration;

        let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("创建测试运行时失败");

            // catch_unwind: 断言失败要以原始信息回报, 否则只会表现为看门狗超时
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rt.block_on(async {
                    // 服务端: 自行持有连接, 稍后主动下发数据
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let server_addr = listener.local_addr().unwrap();

                    let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
                    let mut manager = NetworkConnectionManager::new();
                    let config = crate::config::connection::ClientConfig {
                        protocol: ConnectionType::Tcp,
                        server_address: "127.0.0.1".to_string(),
                        server_port: server_addr.port(),
                        ..Default::default()
                    };
                    manager
                        .create_and_connect_client(&config, Some(tx))
                        .await
                        .expect("客户端应连接成功");

                    let (mut server_conn, _) = listener.accept().await.expect("服务端应能 accept");

                    // 排空事件: 收到 DecoderControlSenderReady 后立刻丢弃该事件, 其中携带的
                    // sender 随之释放 → 客户端解码器控制通道关闭(正是此前触发忙转的状态)
                    loop {
                        let event = rx.recv().await.expect("事件通道不应关闭");
                        if matches!(event, ConnectionEvent::DecoderControlSenderReady(..)) {
                            break;
                        }
                    }

                    // 控制通道关闭后服务端仍下发数据: 读任务必须继续解码并投递事件
                    tokio::io::AsyncWriteExt::write_all(&mut server_conn, b"ee")
                        .await
                        .expect("服务端写入应成功");

                    loop {
                        let event = rx.recv().await.expect("事件通道不应关闭");
                        if let ConnectionEvent::MessagesReceived(tab_id, batch) = event {
                            if tab_id == config.id {
                                assert!(batch.count >= 1, "应至少收到 1 条消息");
                                // TCP 允许把 'ee' 拆成多次 read, 故按字节拼接比较而非按消息条数
                                let received: Vec<u8> = batch
                                    .messages
                                    .iter()
                                    .flat_map(|m| m.raw_data.iter().copied())
                                    .collect();
                                assert_eq!(received, b"ee", "客户端收到的字节应与服务端发送一致");
                                return;
                            }
                        }
                    }
                })
            }));

            let _ = done_tx.send(match outcome {
                Ok(()) => Ok(()),
                Err(payload) => Err(payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "场景 panic(无字符串载荷)".to_string())),
            });
        });

        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("解码器控制通道关闭后读任务应继续收包: 超时说明读任务忙转或未投递事件")
            .expect("场景断言失败");
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

    // ========================================================================
    // 回复规则端到端（真实 TCP 链路，验证"预演 = 真实行为"）
    // ========================================================================

    /// 构造某连接下只含单条规则的规则集，并开启该连接的总闸
    fn rules_with(
        connection_id: &str,
        rule: crate::reply::model::ReplyRule,
    ) -> Arc<crate::reply::ReplyRulesStore> {
        let store = crate::reply::ReplyRulesStore::new();
        let mut cfg = crate::reply::model::ReplyRulesConfig::default();
        cfg.connections
            .insert(connection_id.to_string(), vec![rule]);
        store.replace(&cfg);
        store.set_connection_gates([(connection_id.to_string(), true)].into_iter().collect());
        store
    }

    fn reply_rule_on_len(expected_reply: &str) -> crate::reply::model::ReplyRule {
        crate::reply::model::ReplyRule {
            matcher: crate::reply::model::MatchNode::Length { min: 1, max: 64 },
            payload: crate::reply::model::ReplyPayload {
                text: expected_reply.to_string(),
                hex_mode: true,
                codec: crate::reply::model::RuleCodec::Raw,
            },
            ..crate::reply::model::ReplyRule::new("应答", 10)
        }
    }

    /// 端到端回归（**本轮最重要的一条**）：规则未启用时，链路行为与升级前逐字节一致。
    ///
    /// 断言口径：服务端广播 'ee' → 客户端收到的字节、方向、来源都必须与既有实现相同。
    /// 这条测试是"没有破坏任何既有功能"的唯一可靠证据。
    #[tokio::test]
    async fn test_tcp_echo_regression_without_rules() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;

        let port = {
            let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            probe.local_addr().unwrap().port()
        };

        let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut manager = NetworkConnectionManager::new();

        let server_config = ServerConfig {
            protocol: ConnectionType::Tcp,
            listen_address: "127.0.0.1".to_string(),
            listen_port: port,
            ..Default::default()
        };
        manager
            .create_and_start_server(&server_config, Some(tx.clone()))
            .await
            .expect("服务端应启动成功");

        let client_config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Tcp,
            server_address: "127.0.0.1".to_string(),
            server_port: port,
            ..Default::default()
        };
        manager
            .create_and_connect_client(&client_config, Some(tx))
            .await
            .expect("客户端应连接成功");

        // 等 accept 拿到写入发送器（服务端侧）
        let mut server_write = None;
        loop {
            let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("等待 ServerClientConnected 超时")
                .expect("事件通道不应关闭");
            if let ConnectionEvent::ServerClientConnected(_, _, sender) = event {
                server_write = Some(sender);
                break;
            }
        }

        server_write
            .unwrap()
            .send(crate::network::events::WireMessage::inherit(b"ee".to_vec()))
            .await
            .expect("投递到服务端发送通道应成功");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, rx.recv())
                .await
                .expect("等待客户端 MessagesReceived 超时")
                .expect("事件通道不应关闭");
            if let ConnectionEvent::MessagesReceived(tab_id, batch) = event {
                if tab_id == client_config.id {
                    assert_eq!(batch.count, 1, "未启用规则时 1 次 read 应恰好计 1 条");
                    assert_eq!(batch.bytes, 2, "字节计数不得因规则引擎而变化");
                    assert!(batch.sent_messages.is_empty(), "未启用规则时不得产生应答");
                    assert_eq!(&*batch.messages[0].raw_data, &b"ee"[..]);
                    return;
                }
            }
        }
    }

    /// 端到端：TCP **客户端**侧的规则应答（F-04 的核心场景）。
    ///
    /// 场景：NetAssistant 作客户端连上一个"真实服务端"，服务端发来一帧，
    /// 客户端按规则自动回一条应答（模拟真实服务端的行为）。
    #[tokio::test]
    async fn test_tcp_client_side_rule_reply() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // 客户端配置（其 id 即规则所属连接）
        let client_config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Tcp,
            server_address: "127.0.0.1".to_string(),
            server_port: server_addr.port(),
            ..Default::default()
        };
        // 客户端侧规则：收到 Modbus 读保持寄存器请求 → 回 CRC16 小端应答
        let rules = rules_with(
            &client_config.id,
            reply_rule_on_len("01 03 00 00 00 02 ${crc16modbus:0:6:le}"),
        );

        let (tx, rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut manager = NetworkConnectionManager::new().with_reply_rules(rules.clone());
        manager
            .create_and_connect_client(&client_config, Some(tx))
            .await
            .expect("客户端应连接成功");

        let (mut server_conn, _) = listener.accept().await.expect("服务端应能 accept");

        // 服务端发来一帧请求：6 字节（规则只按长度匹配，内容是任意的）
        server_conn
            .write_all(&[0x01, 0x03, 0x00, 0x00, 0x00, 0x02])
            .await
            .expect("服务端写入应成功");

        // 服务端必须读到客户端按规则生成的应答（含自动填充的 CRC）
        let mut buf = vec![0u8; 8];
        let n = tokio::time::timeout(Duration::from_secs(5), server_conn.read(&mut buf))
            .await
            .expect("等待客户端规则应答超时")
            .expect("读取应成功");
        assert_eq!(
            &buf[..n],
            &[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B],
            "客户端应答必须带自动填充的小端 CRC（Raw 编码，不得追加 CRLF）"
        );

        // 命中计数必须被累加（UI「命中 N」的数据来源）
        assert_eq!(rules.hits_snapshot().values().sum::<u64>(), 1);

        // 应答方向的消息也必须进入展示明细（Sent 方向聚合在本批）
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, rx.recv())
                .await
                .expect("等待客户端 MessagesReceived 超时")
                .expect("事件通道不应关闭");
            if let ConnectionEvent::MessagesReceived(tab_id, batch) = event {
                if tab_id == client_config.id && !batch.sent_messages.is_empty() {
                    assert_eq!(
                        &*batch.sent_messages[0].raw_data,
                        &[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B][..],
                        "Sent 明细里应能看到规则生成的应答"
                    );
                    return;
                }
            }
        }
    }

    /// 端到端：UDP **客户端**侧的规则应答必须回到数据报的真实来源
    /// （广播发现场景：回复来自设备自己的 IP，而不是发送目标地址）
    #[tokio::test]
    async fn test_udp_client_side_rule_reply_to_real_source() {
        use crate::network::events::ConnectionEvent;
        use std::time::Duration;

        // 客户端固定本地端口，作为"被仿真的服务端"地址
        let client_local_port = {
            let probe = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            probe.local_addr().unwrap().port()
        };
        // 真实设备（模拟）：另一个 socket 向客户端发数据报
        let device = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let device_addr = device.local_addr().unwrap();

        let client_config = crate::config::connection::ClientConfig {
            protocol: ConnectionType::Udp,
            server_address: "127.0.0.1".to_string(),
            // 指向一个不会被真正使用的端口：应答必须回到 device_addr 而非这里
            server_port: 9,
            local_address: Some("127.0.0.1".to_string()),
            local_port: Some(client_local_port),
            ..Default::default()
        };
        let rules = rules_with(&client_config.id, reply_rule_on_len("4F 4B")); // "OK"

        let (tx, _rx) = smol::channel::unbounded::<ConnectionEvent>();
        let mut manager = NetworkConnectionManager::new().with_reply_rules(rules.clone());
        manager
            .create_and_connect_client(&client_config, Some(tx))
            .await
            .expect("UDP 客户端应启动成功");

        // 等客户端 socket 真正绑定（Connected 事件即可视为就绪，这里用固定端口重试发送）
        let target = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            client_local_port,
        );
        let mut got = Vec::new();
        for _ in 0..20 {
            device.send_to(b"AT", target).await.expect("设备发送应成功");
            let mut buf = [0u8; 64];
            if let Ok(Ok((n, from))) =
                tokio::time::timeout(Duration::from_millis(150), device.recv_from(&mut buf)).await
            {
                got = buf[..n].to_vec();
                assert_eq!(from.port(), client_local_port, "应答应来自客户端的本地端口");
                break;
            }
        }
        assert_eq!(got, b"OK", "UDP 客户端规则应答必须回到数据报的真实来源");
        assert_eq!(rules.hits_snapshot().values().sum::<u64>(), 1);
        assert!(
            device_addr.ip().is_loopback(),
            "设备地址仅用于确认回包方向（占位断言，避免未使用告警）"
        );
    }
}
