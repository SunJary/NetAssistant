use gpui::*;
use gpui_component::input::InputState;
use log::{debug, error, info};

use crate::config;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType, ServerConfig};
use crate::config::storage::ConfigStorage;
use crate::message::{Message, MessageDirection, MessageType};
use crate::theme_event_handler::{ThemeEventHandler, apply_theme};
use crate::ui::connection_tab::ConnectionTabState;
use crate::ui::main_window::MainWindow;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Connected(String),
    Disconnected(String),
    Listening(String),
    Error(String, String),
    MessageReceived(String, Message),
    ClientWriteSenderReady(String, mpsc::UnboundedSender<Vec<u8>>),
    ServerClientConnected(String, SocketAddr, mpsc::UnboundedSender<Vec<u8>>),
    ServerClientDisconnected(String, SocketAddr),
    PeriodicSend(String, String),
    PeriodicSendBytes(String, Vec<u8>, String),
}

pub struct NetAssistantApp {
    // 配置存储
    pub storage: ConfigStorage,

    // 客户端连接相关状态
    pub client_expanded: bool,
    pub show_new_connection: bool,
    pub new_connection_is_client: bool,
    pub host_input: Entity<InputState>,
    pub port_input: Entity<InputState>,
    pub new_connection_protocol: String,

    // 服务端连接相关状态
    pub server_expanded: bool,

    // Tab页状态（每个标签页独立管理自己的网络连接）
    pub active_tab: String,
    pub connection_tabs: HashMap<String, ConnectionTabState>,
    pub tab_multiline: bool,

    // 自动回复输入框状态（每个标签页一个）
    pub auto_reply_inputs: HashMap<String, Entity<InputState>>,

    // 连接事件通道（用于通知UI更新）
    pub connection_event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
    pub connection_event_receiver: Option<mpsc::UnboundedReceiver<ConnectionEvent>>,

    // 写入发送器映射（无锁设计，每个标签页独立管理）
    pub client_write_senders: HashMap<String, mpsc::UnboundedSender<Vec<u8>>>,
    pub server_clients: HashMap<String, HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>>,

    // 右键菜单状态
    pub show_context_menu: bool,
    pub context_menu_connection: Option<String>,
    pub context_menu_is_client: bool,
    pub context_menu_position: Option<Pixels>,
    pub context_menu_position_y: Option<Pixels>,
}

impl NetAssistantApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let storage = ConfigStorage::new().expect("无法创建配置存储");

        // 使用window创建InputState实体
        let host_input = cx.new(|cx| InputState::new(window, cx));
        let port_input = cx.new(|cx| InputState::new(window, cx));

        // 初始化空的连接标签页状态（不预先创建）
        let connection_tabs = HashMap::new();
        let active_tab = String::new();

        // 创建连接事件通道
        let (connection_event_sender, connection_event_receiver) = mpsc::unbounded_channel();

        // 初始化写入发送器映射
        let client_write_senders = HashMap::new();
        let server_clients = HashMap::new();

        Self {
            storage,
            client_expanded: true,
            show_new_connection: false,
            new_connection_is_client: true,
            host_input,
            port_input,
            new_connection_protocol: String::from("TCP"),
            server_expanded: true,
            active_tab,
            connection_tabs,
            tab_multiline: false,
            auto_reply_inputs: HashMap::new(),
            connection_event_sender: Some(connection_event_sender),
            connection_event_receiver: Some(connection_event_receiver),
            client_write_senders,
            server_clients,
            show_context_menu: false,
            context_menu_connection: None,
            context_menu_is_client: false,
            context_menu_position: None,
            context_menu_position_y: None,
        }
    }

    pub fn toggle_connection(&mut self, tab_id: String, cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            if tab_state.is_connected {
                // 断开连接
                if tab_state.connection_config.is_client() {
                    self.disconnect_client(tab_id, cx);
                } else {
                    // 服务端断开
                    tab_state.disconnect();
                    self.server_clients.remove(&tab_id);
                }
            } else {
                // 建立连接
                if tab_state.connection_config.is_client() {
                    // 根据协议类型选择连接方法
                    if tab_state.connection_config.protocol() == ConnectionType::Tcp {
                        self.connect_client(tab_id, cx);
                    } else {
                        self.connect_udp_client(tab_id, cx);
                    }
                } else {
                    // 启动服务端
                    if let ConnectionConfig::Server(server_config) = &tab_state.connection_config {
                        let server_config_clone = server_config.clone();
                        let tab_id_clone = tab_id.clone();
                        // 然后调用相应的服务器启动方法
                        if server_config_clone.protocol == ConnectionType::Tcp {
                            self.start_tcp_server(tab_id_clone, &server_config_clone, cx);
                        } else {
                            self.start_udp_server(tab_id_clone, &server_config_clone, cx);
                        }
                    }
                }
            }
        }
    }

    pub fn start_periodic_send(
        &mut self,
        tab_id: String,
        interval_ms: u64,
        content: String,
        message_input_mode: String,
        _cx: &mut Context<Self>,
    ) {
        // 首先停止已有的周期发送任务
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            if let Some(timer_arc) = &tab_state.periodic_send_timer {
                if let Ok(mut timer) = timer_arc.lock() {
                    if let Some(timer_handle) = timer.take() {
                        timer_handle.abort();
                        info!("[周期发送] 已停止旧的周期发送任务");
                    }
                }
            }
        }

        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let content_clone = content.clone();
        let message_input_mode_clone = message_input_mode.clone();

        // 创建周期发送任务
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;

                // 发送消息
                if message_input_mode_clone == "text" {
                    // 这里我们需要一种方式来访问应用实例
                    // 由于我们不能直接访问，我们可以通过事件系统来处理
                    if let Some(sender) = sender.clone() {
                        let _ = sender.send(ConnectionEvent::PeriodicSend(
                            tab_id_clone.clone(),
                            content_clone.clone(),
                        ));
                    }
                } else {
                    // 处理十六进制输入
                    let hex_content = content_clone.clone();
                    let cleaned_hex = hex_content.replace(|c: char| !c.is_ascii_hexdigit(), "");
                    if cleaned_hex.len() % 2 == 0 {
                        if let Ok(bytes) = hex::decode(&cleaned_hex) {
                            if let Some(sender) = sender.clone() {
                                let _ = sender.send(ConnectionEvent::PeriodicSendBytes(
                                    tab_id_clone.clone(),
                                    bytes,
                                    hex_content,
                                ));
                            }
                        }
                    }
                }
            }
        });

        // 存储任务句柄到标签页状态中
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.periodic_send_timer = Some(Arc::new(Mutex::new(Some(task))));
        }
    }

    pub fn start_tcp_server(
        &mut self,
        tab_id: String,
        server_config: &ServerConfig,
        _cx: &mut Context<Self>,
    ) {
        let address = format!(
            "{}:{}",
            server_config.listen_address, server_config.listen_port
        );
        info!("[服务端] 尝试启动: {}", address);

        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.connection_status = ConnectionStatus::Connecting;
        }

        let handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            debug!("[服务端] 异步任务开始，尝试监听: {}", address);
            match tokio::net::TcpListener::bind(&address).await {
                Ok(listener) => {
                    info!("[服务端] 启动成功，监听: {}", address);

                    if let Some(sender) = sender.clone() {
                        let _ = sender.send(ConnectionEvent::Listening(tab_id_clone.clone()));
                    }

                    // 接受连接循环
                    loop {
                        match listener.accept().await {
                            Ok((stream, addr)) => {
                                info!("[服务端] 客户端连接: {}", addr);

                                let (mut reader, mut writer) = stream.into_split();
                                let (write_sender, mut write_receiver) =
                                    mpsc::unbounded_channel::<Vec<u8>>();

                                let sender_clone = sender.clone();
                                let tab_id_clone2 = tab_id_clone.clone();

                                // 通知UI客户端连接
                                let sender_clone_for_connect = sender.clone();
                                let tab_id_clone_for_connect = tab_id_clone.clone();
                                let write_sender_clone = write_sender.clone();
                                tokio::spawn(async move {
                                    if let Some(sender) = sender_clone_for_connect {
                                        let _ =
                                            sender.send(ConnectionEvent::ServerClientConnected(
                                                tab_id_clone_for_connect,
                                                addr,
                                                write_sender_clone,
                                            ));
                                    }
                                });

                                // 启动接收任务
                                tokio::spawn(async move {
                                    let mut buffer = vec![0u8; 4096];
                                    loop {
                                        match reader.read(&mut buffer).await {
                                            Ok(n) if n > 0 => {
                                                buffer.truncate(n);
                                                let message = Message::new(
                                                    MessageDirection::Received,
                                                    buffer.clone(),
                                                    MessageType::Text,
                                                )
                                                .with_source(addr.to_string());

                                                if let Some(sender) = sender_clone.clone() {
                                                    let _ = sender.send(
                                                        ConnectionEvent::MessageReceived(
                                                            tab_id_clone2.clone(),
                                                            message,
                                                        ),
                                                    );
                                                }
                                            }
                                            Ok(_) => {
                                                info!("[服务端] 客户端 {} 连接关闭", addr);
                                                break;
                                            }
                                            Err(e) => {
                                                error!("[服务端] 读取错误: {}", e);
                                                break;
                                            }
                                        }
                                    }

                                    // 通知UI客户端断开
                                    if let Some(sender) = sender_clone {
                                        let _ =
                                            sender.send(ConnectionEvent::ServerClientDisconnected(
                                                tab_id_clone2,
                                                addr,
                                            ));
                                    }
                                });

                                // 启动写入任务
                                tokio::spawn(async move {
                                    while let Some(data) = write_receiver.recv().await {
                                        if let Err(e) = writer.write_all(&data).await {
                                            error!("[服务端] 写入错误: {}", e);
                                            break;
                                        }
                                        // 确保数据立即发送
                                        if let Err(e) = writer.flush().await {
                                            error!("[服务端] 刷新缓冲区失败: {}", e);
                                            break;
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                error!("[服务端] 接受连接错误: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("[服务端] 启动失败: {}", e);
                    if let Some(sender) = sender {
                        let _ = sender.send(ConnectionEvent::Error(tab_id_clone, e.to_string()));
                    }
                }
            }
        });

        // 保存服务端任务的 JoinHandle
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.server_handle =
                Some(std::sync::Arc::new(std::sync::Mutex::new(Some(handle))));
        }
    }

    pub fn start_udp_server(
        &mut self,
        tab_id: String,
        server_config: &ServerConfig,
        _cx: &mut Context<Self>,
    ) {
        let address = format!(
            "{}:{}",
            server_config.listen_address, server_config.listen_port
        );
        info!("[UDP服务端] 尝试启动: {}", address);

        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.connection_status = ConnectionStatus::Connecting;
        }

        let handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            debug!("[UDP服务端] 异步任务开始，尝试监听: {}", address);
            match tokio::net::UdpSocket::bind(&address).await {
                Ok(socket) => {
                    info!("[UDP服务端] 启动成功，监听: {}", address);

                    // 使用 Arc 包装 socket 以支持多任务共享
                    let socket = std::sync::Arc::new(socket);

                    if let Some(sender) = sender.clone() {
                        let _ = sender.send(ConnectionEvent::Listening(tab_id_clone.clone()));
                    }

                    // 保存客户端地址和对应的发送器
                    let mut clients: std::collections::HashMap<
                        std::net::SocketAddr,
                        mpsc::UnboundedSender<Vec<u8>>,
                    > = std::collections::HashMap::new();

                    // 接收数据循环
                    loop {
                        let mut buffer = vec![0u8; 4096];
                        let socket_clone = socket.clone();
                        match socket_clone.recv_from(&mut buffer).await {
                            Ok((n, addr)) => {
                                buffer.truncate(n);
                                debug!("[UDP服务端] 收到来自 {} 的数据: {:?}", addr, buffer);

                                // 检查客户端是否已存在，不存在则创建新的发送器
                                if !clients.contains_key(&addr) {
                                    let (write_sender, mut write_receiver) =
                                        mpsc::unbounded_channel::<Vec<u8>>();
                                    clients.insert(addr, write_sender.clone());

                                    // 通知UI客户端连接
                                    let sender_clone_for_connect = sender.clone();
                                    let tab_id_clone_for_connect = tab_id_clone.clone();
                                    let write_sender_clone = write_sender.clone();
                                    tokio::spawn(async move {
                                        if let Some(sender) = sender_clone_for_connect {
                                            let _ = sender.send(
                                                ConnectionEvent::ServerClientConnected(
                                                    tab_id_clone_for_connect,
                                                    addr,
                                                    write_sender_clone,
                                                ),
                                            );
                                        }
                                    });

                                    // 启动写入任务
                                    let socket_clone = socket.clone();
                                    let addr_clone = addr;
                                    tokio::spawn(async move {
                                        while let Some(data) = write_receiver.recv().await {
                                            if let Err(e) =
                                                socket_clone.send_to(&data, addr_clone).await
                                            {
                                                error!("[UDP服务端] 发送错误: {}", e);
                                                break;
                                            }
                                        }
                                    });
                                }

                                // 处理收到的数据
                                let sender_clone = sender.clone();
                                let tab_id_clone2 = tab_id_clone.clone();
                                let addr_clone = addr;
                                tokio::spawn(async move {
                                    let message = Message::new(
                                        MessageDirection::Received,
                                        buffer,
                                        MessageType::Text,
                                    )
                                    .with_source(addr_clone.to_string());

                                    if let Some(sender) = sender_clone {
                                        let _ = sender.send(ConnectionEvent::MessageReceived(
                                            tab_id_clone2,
                                            message,
                                        ));
                                    }
                                });
                            }
                            Err(e) => {
                                error!("[UDP服务端] 接收错误: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("[UDP服务端] 启动失败: {}", e);
                    if let Some(sender) = sender {
                        let _ = sender.send(ConnectionEvent::Error(tab_id_clone, e.to_string()));
                    }
                }
            }
        });

        // 保存服务端任务的 JoinHandle
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.server_handle =
                Some(std::sync::Arc::new(std::sync::Mutex::new(Some(handle))));
        }
    }

    pub fn sanitize_hex_input(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // 这里可以实现十六进制输入的清理逻辑
        // 由于我们现在使用的是每个标签页独立的输入框，
        // 这个方法可能需要根据具体的标签页来清理
        debug!("[sanitize_hex_input] 清理十六进制输入");
    }

    pub fn ensure_tab_exists(
        &mut self,
        tab_id: String,
        connection_config: config::connection::ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_tabs.contains_key(&tab_id) {
            self.connection_tabs.insert(
                tab_id.clone(),
                ConnectionTabState::new(connection_config, window, cx),
            );
        }
    }

    pub fn ensure_auto_reply_input_exists(
        &mut self,
        tab_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.auto_reply_inputs.contains_key(&tab_id) {
            let auto_reply_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .code_editor("json")
                    .line_number(false)
                    // .rows(5)
                    .multi_line(true)
                    .placeholder("输入自动回复内容...")
            });
            auto_reply_input.update(cx, |input, cx| {
                input.set_value("ok".to_string(), window, cx);
            });
            self.auto_reply_inputs.insert(tab_id, auto_reply_input);
        }
    }

    pub fn close_tab(&mut self, tab_id: String) {
        info!("[关闭标签页] 开始关闭标签页: {}", tab_id);

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }

        if self.connection_tabs.remove(&tab_id).is_some() {
            info!("[关闭标签页] 移除标签页状态: {}", tab_id);
        }

        if self.auto_reply_inputs.remove(&tab_id).is_some() {
            info!("[关闭标签页] 移除自动回复输入框: {}", tab_id);
        }

        // 清理客户端连接发送器
        if self.client_write_senders.remove(&tab_id).is_some() {
            info!("[关闭标签页] 移除客户端连接发送器: {}", tab_id);
        }

        // 清理服务端客户端连接
        if self.server_clients.remove(&tab_id).is_some() {
            info!("[关闭标签页] 移除服务端客户端连接: {}", tab_id);
        }

        info!("[关闭标签页] 标签页 {} 已关闭", tab_id);
    }

    pub fn connect_client(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            if !tab_state.is_connected && tab_state.connection_config.is_client() {
                if let ConnectionConfig::Client(client_config) = &tab_state.connection_config {
                    let address = format!(
                        "{}:{}",
                        client_config.server_address, client_config.server_port
                    );
                    info!("[客户端] 尝试连接到服务器: {}", address);
                    let sender = self.connection_event_sender.clone();
                    let tab_id_clone = tab_id.clone();

                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        tab_state.connection_status = ConnectionStatus::Connecting;
                        info!("[客户端] 连接状态已更新为: Connecting");
                    }

                    let handle = tokio::spawn(async move {
                        debug!("[客户端] 异步任务开始，尝试连接: {}", address);
                        match tokio::net::TcpStream::connect(&address).await {
                            Ok(stream) => {
                                let peer_addr = stream.peer_addr().ok();
                                info!("[客户端] 连接成功: {:?}", peer_addr);

                                let (mut reader, mut writer) = stream.into_split();
                                let (write_sender, mut write_receiver) =
                                    mpsc::unbounded_channel::<Vec<u8>>();

                                let sender_clone = sender.clone();
                                let tab_id_clone2 = tab_id_clone.clone();

                                // 保存write_sender到映射（需要在UI线程中操作）
                                let tab_id_clone_for_sender = tab_id_clone.clone();
                                let write_sender_clone = write_sender.clone();
                                let sender_clone_for_map = sender.clone();
                                // 直接发送事件，不创建新的异步任务，减少延迟
                                if let Some(sender) = sender_clone_for_map {
                                    let _ = sender.send(ConnectionEvent::ClientWriteSenderReady(
                                        tab_id_clone_for_sender,
                                        write_sender_clone,
                                    ));
                                }

                                // 启动接收任务
                                tokio::spawn(async move {
                                    debug!("[客户端] 启动接收任务");
                                    loop {
                                        let mut buffer = vec![0u8; 4096];
                                        let result = reader.read(&mut buffer).await;
                                        match result {
                                            Ok(n) => {
                                                if n > 0 {
                                                    buffer.truncate(n);
                                                    let message = Message::new(
                                                        MessageDirection::Received,
                                                        buffer.clone(),
                                                        MessageType::Text,
                                                    );
                                                    if let Some(sender) = sender_clone.clone() {
                                                        let _ = sender.send(
                                                            ConnectionEvent::MessageReceived(
                                                                tab_id_clone2.clone(),
                                                                message,
                                                            ),
                                                    );
                                                    }
                                                } else {
                                                    info!("[客户端] 接收到0字节，连接已关闭");
                                                    // 通知UI连接已断开
                                                    if let Some(sender) = sender_clone.clone() {
                                                        let _ = sender.send(
                                                            ConnectionEvent::Disconnected(
                                                                tab_id_clone2.clone(),
                                                            ),
                                                        );
                                                    }
                                                    break;
                                                }
                                            }
                                            Err(e) => {
                                                error!("[客户端] 接收数据失败: {}", e);
                                                // 通知UI连接已断开
                                                if let Some(sender) = sender_clone.clone() {
                                                    let _ =
                                                        sender.send(ConnectionEvent::Disconnected(
                                                            tab_id_clone2.clone(),
                                                        ));
                                                }
                                                break;
                                            }
                                        }
                                        tokio::time::sleep(tokio::time::Duration::from_millis(10))
                                            .await;
                                    }
                                    debug!("[客户端] 接收任务结束");
                                });

                                // 启动写入任务
                                let sender_clone2 = sender.clone();
                                let tab_id_clone3 = tab_id_clone.clone();
                                tokio::spawn(async move {
                                    debug!("[客户端] 启动写入任务");
                                    while let Some(data) = write_receiver.recv().await {
                                        let result = writer.write_all(&data).await;
                                        if let Err(e) = result {
                                            error!("[客户端] 写入数据失败: {}", e);
                                            if let Some(sender) = sender_clone2.clone() {
                                                let _ = sender.send(ConnectionEvent::Error(
                                                    tab_id_clone3,
                                                    e.to_string(),
                                                ));
                                            }
                                            break;
                                        }
                                        // 确保数据立即发送
                                        if let Err(e) = writer.flush().await {
                                            error!("[客户端] 刷新缓冲区失败: {}", e);
                                            if let Some(sender) = sender_clone2.clone() {
                                                let _ = sender.send(ConnectionEvent::Error(
                                                    tab_id_clone3,
                            e.to_string(),
                                                ));
                                            }
                                            break;
                                        }
                                    }
                                    debug!("[客户端] 写入任务结束");
                                });

                                // 通知UI连接成功
                                let sender_clone3 = sender.clone();
                                if let Some(sender) = sender_clone3 {
                                    let _ = sender.send(ConnectionEvent::Connected(tab_id_clone));
                                }
                            }
                            Err(e) => {
                                error!("[客户端] 连接失败: {}", e);
                                let sender_clone4 = sender.clone();
                                if let Some(sender) = sender_clone4 {
                                    let _ = sender
                                        .send(ConnectionEvent::Error(tab_id_clone, e.to_string()));
                                }
                            }
                        }
                    });

                    // 保存客户端任务的 JoinHandle
                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        tab_state.client_handle =
                            Some(std::sync::Arc::new(std::sync::Mutex::new(Some(handle))));
                    }
                }
            } else {
                debug!(
                    "[客户端] 连接条件不满足: is_connected={}, is_client={}",
                    tab_state.is_connected,
                    tab_state.connection_config.is_client()
                );
            }
        } else {
            error!("[客户端] 未找到标签页状态: {}", tab_id);
        }
    }

    pub fn connect_udp_client(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            if !tab_state.is_connected && tab_state.connection_config.is_client() {
                if let ConnectionConfig::Client(client_config) = &tab_state.connection_config {
                    let address = format!(
                        "{}:{}",
                        client_config.server_address, client_config.server_port
                    );
                    info!("[UDP客户端] 尝试连接到服务器: {}", address);
                    let sender = self.connection_event_sender.clone();
                    let tab_id_clone = tab_id.clone();

                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        tab_state.connection_status = ConnectionStatus::Connecting;
                        info!("[UDP客户端] 连接状态已更新为: Connecting");
                    }

                    let handle = tokio::spawn(async move {
                        info!("[UDP客户端] 异步任务开始，尝试连接: {}", address);

                        info!("[UDP客户端] 步骤1: 开始创建UDP Socket");
                        match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
                            Ok(socket) => {

                                // UDP是无连接的，所以这里只是创建socket，不需要真正"连接"
                                info!("[UDP客户端] Socket创建成功");

                                // 使用 Arc 包装 socket 以支持多任务共享
                                let socket = std::sync::Arc::new(socket);

                                // 解析服务器地址
                                let server_addr: std::net::SocketAddr = address.parse().unwrap();

                                // 创建发送器
                                let (write_sender, mut write_receiver) =
                                    mpsc::unbounded_channel::<Vec<u8>>();

                                let sender_clone = sender.clone();
                                let tab_id_clone2 = tab_id_clone.clone();
                                let socket_clone = socket.clone();

                                // 保存write_sender到映射（需要在UI线程中操作）
                                let tab_id_clone_for_sender = tab_id_clone.clone();
                                let write_sender_clone = write_sender.clone();
                                let sender_clone_for_map = sender.clone();

                                // 直接发送事件，不创建新的异步任务，减少延迟
                                if let Some(sender) = sender_clone_for_map {
                                    let _ = sender.send(ConnectionEvent::ClientWriteSenderReady(
                                        tab_id_clone_for_sender,
                                        write_sender_clone,
                                    ));
                                }


                                // 启动接收任务
                                tokio::spawn(async move {
                                    info!("[UDP客户端] 接收任务启动");
                                    loop {
                                        let mut buffer = vec![0u8; 4096];
                                        let socket_clone = socket_clone.clone();
                                        let result = socket_clone.recv_from(&mut buffer).await;
                                        match result {
                                            Ok((n, addr)) => {
                                                if n > 0 {
                                                    buffer.truncate(n);
                                                    info!(
                                                        "[UDP客户端] 收到来自 {} 的数据: {:?}",
                                                        addr, buffer
                                                    );
                                                    let message = Message::new(
                                                        MessageDirection::Received,
                                                        buffer.clone(),
                                                        MessageType::Text,
                                                    )
                                                    .with_source(addr.to_string());
                                                    if let Some(sender) = sender_clone.clone() {
                                                        let _ = sender.send(
                                                            ConnectionEvent::MessageReceived(
                                                                tab_id_clone2.clone(),
                                                                message,
                                                            ),
                                                        );
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                error!("[UDP客户端] 接收数据失败: {}", e);
                                                // UDP无连接，不需要通知断开
                                                break;
                                            }
                                        }
                                        tokio::time::sleep(tokio::time::Duration::from_millis(10))
                                            .await;
                                    }
                                    info!("[UDP客户端] 接收任务结束");
                                });

                                // 启动写入任务
                                let sender_clone2 = sender.clone();
                                let tab_id_clone4 = tab_id_clone.clone();
                                tokio::spawn(async move {
                                    info!("[UDP客户端] 写入任务启动");
                                    while let Some(data) = write_receiver.recv().await {
                                        let socket_clone = socket.clone();
                                        let tab_id_clone3 = tab_id_clone4.clone();
                                        let sender_clone3 = sender_clone2.clone();

                                        let result = socket_clone.send_to(&data, server_addr).await;
                                        if let Err(e) = result {
                                            error!("[UDP客户端] 写入数据失败: {}", e);
                                            if let Some(sender) = sender_clone3 {
                                                let _ = sender.send(ConnectionEvent::Error(
                                                    tab_id_clone3,
                                                    e.to_string(),
                                                ));
                                            }
                                            // 对于UDP，写入失败可能是暂时的，不需要断开连接
                                        } else {
                                            info!("[UDP客户端] 数据发送成功");
                                        }
                                    }
                                    info!("[UDP客户端] 写入任务结束");
                                });

                                // 通知UI连接成功
                                let sender_clone3 = sender.clone();
                                let tab_id_clone5 = tab_id_clone.clone();
                                if let Some(sender) = sender_clone3 {
                                    let _ = sender.send(ConnectionEvent::Connected(tab_id_clone5));
                                }
                            }
                            Err(e) => {
                                error!("[UDP客户端] Socket创建失败: {}", e);
                                let sender_clone4 = sender.clone();
                                if let Some(sender) = sender_clone4 {
                                    let _ = sender
                                        .send(ConnectionEvent::Error(tab_id_clone, e.to_string()));
                                }
                            }
                        }
                    });

                    // 保存客户端任务的 JoinHandle
                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        tab_state.client_handle =
                            Some(std::sync::Arc::new(std::sync::Mutex::new(Some(handle))));
                    }
                }
            } else {
                debug!(
                    "[UDP客户端] 连接条件不满足: is_connected={}, is_client={}",
                    tab_state.is_connected,
                    tab_state.connection_config.is_client()
                );
            }
        } else {
            error!("[UDP客户端] 未找到标签页状态: {}", tab_id);
        }
    }

    pub fn disconnect_client(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }

        tokio::spawn(async move {
            if let Some(sender) = sender {
                let _ = sender.send(ConnectionEvent::Disconnected(tab_id_clone));
            }
        });
    }

    pub fn send_message(&mut self, tab_id: String, content: String) {
        info!(
            "[send_message] 开始，tab_id: {}, content: '{}'",
            tab_id, content
        );
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let bytes = content.into_bytes();

        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            debug!(
                "[send_message] 找到标签页，is_connected: {}, connection_config: {:?}",
                tab_state.is_connected, tab_state.connection_config
            );
            if tab_state.is_connected {
                if tab_state.connection_config.is_client() {
                    debug!("[send_message] 客户端模式");
                    if let Some(write_sender) = self.client_write_senders.get(&tab_id).cloned() {
                        let bytes_clone = bytes.clone();
                        let message_input_mode = tab_state.message_input_mode.clone();
                        tokio::spawn(async move {
                            debug!("[send_message] 异步任务开始发送");
                            let result: Result<(), mpsc::error::SendError<Vec<u8>>> =
                                write_sender.send(bytes_clone);
                            if let Err(e) = result {
                                error!("[send_message] 发送失败: {}", e);
                                if let Some(sender) = sender {
                                    let _ = sender.send(ConnectionEvent::Error(
                                        tab_id_clone,
                                        format!("发送失败: {}", e),
                                    ));
                                }
                            } else {
                                debug!("[send_message] 发送成功");
                                if let Some(sender) = sender {
                                    let message_type = if message_input_mode == "text" {
                                        MessageType::Text
                                    } else {
                                        MessageType::Hex
                                    };
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        message_type,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(
                                        tab_id_clone,
                                        message,
                                    ));
                                }
                            }
                        });
                    } else {
                        error!("[send_message] 未找到写入器");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(
                                tab_id_clone,
                                "写入器未初始化".to_string(),
                            ));
                        }
                    }
                } else {
                    debug!("[send_message] 服务端模式");
                    let clients: Vec<(SocketAddr, mpsc::UnboundedSender<Vec<u8>>)> = self
                        .server_clients
                        .get(&tab_id)
                        .map(|clients| {
                            clients
                                .iter()
                                .map(|(addr, sender)| (*addr, sender.clone()))
                                .collect()
                        })
                        .unwrap_or_default();

                    if clients.is_empty() {
                        error!("[send_message] 没有连接的客户端");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(
                                tab_id_clone,
                                "没有连接的客户端".to_string(),
                            ));
                        }
                    } else {
                        let message_input_mode = tab_state.message_input_mode.clone();
                        let sender_clone = sender.clone();
                        let tab_id_clone2 = tab_id_clone.clone();
                        tokio::spawn(async move {
                            debug!("[send_message] 异步任务开始广播");
                            let mut success_count = 0;
                            for (addr, write_sender) in clients {
                                if let Err(_e) = write_sender.send(bytes.clone()) {
                                    error!("[send_message] 发送给客户端 {} 失败", addr);
                                } else {
                                    success_count += 1;
                                }
                            }

                            if success_count > 0 {
                                info!("[send_message] 广播成功，发送给 {} 个客户端", success_count);
                                if let Some(sender) = sender_clone {
                                    let message_type = if message_input_mode == "text" {
                                        MessageType::Text
                                    } else {
                                        MessageType::Hex
                                    };
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        message_type,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(
                                        tab_id_clone2,
                                        message,
                                    ));
                                }
                            }
                        });
                    }
                }
            } else {
                error!("[send_message] 连接未建立");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(
                        tab_id_clone,
                        "连接未建立".to_string(),
                    ));
                }
            }
        } else {
            error!("[send_message] 未找到标签页: {}", tab_id);
        }
    }

    pub fn send_message_bytes(&mut self, tab_id: String, bytes: Vec<u8>, hex_input: String) {
        info!(
            "[send_message_bytes] 开始，tab_id: {}, bytes: {:?}, hex_input: '{}'",
            tab_id, bytes, hex_input
        );
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();

        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            debug!(
                "[send_message_bytes] 找到标签页，is_connected: {}, connection_config: {:?}",
                tab_state.is_connected, tab_state.connection_config
            );
            if tab_state.is_connected {
                if tab_state.connection_config.is_client() {
                    debug!("[send_message_bytes] 客户端模式");
                    if let Some(write_sender) = self.client_write_senders.get(&tab_id).cloned() {
                        let bytes_clone = bytes.clone();
                        let message_input_mode = tab_state.message_input_mode.clone();
                        tokio::spawn(async move {
                            debug!("[send_message_bytes] 异步任务开始发送");
                            let result: Result<(), mpsc::error::SendError<Vec<u8>>> =
                                write_sender.send(bytes_clone);
                            if let Err(e) = result {
                                error!("[send_message_bytes] 发送失败: {}", e);
                                if let Some(sender) = sender {
                                    let _ = sender.send(ConnectionEvent::Error(
                                        tab_id_clone,
                                        format!("发送失败: {}", e),
                                    ));
                                }
                            } else {
                                debug!("[send_message_bytes] 发送成功");
                                if let Some(sender) = sender {
                                    let message_type = if message_input_mode == "text" {
                                        MessageType::Text
                                    } else {
                                        MessageType::Hex
                                    };
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        message_type,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(
                                        tab_id_clone,
                                        message,
                                    ));
                                }
                            }
                        });
                    } else {
                        error!("[send_message_bytes] 未找到写入器");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(
                                tab_id_clone,
                                "写入器未初始化".to_string(),
                            ));
                        }
                    }
                } else {
                    debug!("[send_message_bytes] 服务端模式");
                    let clients: Vec<(SocketAddr, mpsc::UnboundedSender<Vec<u8>>)> = self
                        .server_clients
                        .get(&tab_id)
                        .map(|clients| {
                            clients
                                .iter()
                                .map(|(addr, sender)| (*addr, sender.clone()))
                                .collect()
                        })
                        .unwrap_or_default();

                    if clients.is_empty() {
                        error!("[send_message_bytes] 没有连接的客户端");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(
                                tab_id_clone,
                                "没有连接的客户端".to_string(),
                            ));
                        }
                    } else {
                        let sender_clone = sender.clone();
                        let tab_id_clone2 = tab_id_clone.clone();
                        let message_input_mode = tab_state.message_input_mode.clone();
                        tokio::spawn(async move {
                            debug!("[send_message_bytes] 异步任务开始广播");
                            let mut success_count = 0;
                            for (addr, write_sender) in clients {
                                if let Err(_e) = write_sender.send(bytes.clone()) {
                                    error!("[send_message_bytes] 发送给客户端 {} 失败", addr);
                                } else {
                                    success_count += 1;
                                }
                            }

                            if success_count > 0 {
                                info!(
                                    "[send_message_bytes] 广播成功，发送给 {} 个客户端",
                                    success_count
                                );
                                if let Some(sender) = sender_clone {
                                    let message_type = if message_input_mode == "text" {
                                        MessageType::Text
                                    } else {
                                        MessageType::Hex
                                    };
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        message_type,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(
                                        tab_id_clone2,
                                        message,
                                    ));
                                }
                            }
                        });
                    }
                }
            } else {
                error!("[send_message_bytes] 连接未建立");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(
                        tab_id_clone,
                        "连接未建立".to_string(),
                    ));
                }
            }
        } else {
            error!("[send_message_bytes] 未找到标签页: {}", tab_id);
        }
    }

    pub fn send_message_to_client(
        &mut self,
        tab_id: String,
        content: String,
        source: Option<String>,
        _cx: &mut Context<Self>,
    ) {
        info!(
            "[send_message_to_client] 开始，tab_id: {}, content: '{}', source: {:?}",
            tab_id, content, source
        );
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let bytes = content.clone().into_bytes();

        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            debug!(
                "[send_message_to_client] 找到标签页，is_connected: {}, connection_config: {:?}",
                tab_state.is_connected, tab_state.connection_config
            );
            if tab_state.is_connected {
                if tab_state.connection_config.is_client() {
                    debug!("[send_message_to_client] 客户端模式，直接发送给服务器");
                    self.send_message(tab_id, content);
                } else {
                    debug!("[send_message_to_client] 服务端模式");

                    if let Some(source_str) = source {
                        if let Ok(addr) = source_str.parse::<std::net::SocketAddr>() {
                            info!("[send_message_to_client] 发送给指定客户端: {}", addr);
                            if let Some(clients) = self.server_clients.get(&tab_id) {
                                if let Some(write_sender) = clients.get(&addr).cloned() {
                                    let message_input_mode = tab_state.message_input_mode.clone();
                                    let sender_clone = sender.clone();
                                    let tab_id_clone2 = tab_id_clone.clone();
                                    let bytes_clone = bytes.clone();
                                    let source_str_clone = source_str.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = write_sender.send(bytes_clone) {
                                            error!("[send_message_to_client] 发送失败: {}", e);
                                            if let Some(sender) = sender_clone {
                                                let _ = sender.send(ConnectionEvent::Error(
                                                    tab_id_clone2,
                                                    e.to_string(),
                                                ));
                                            }
                                        } else {
                                            debug!("[send_message_to_client] 发送成功");
                                            if let Some(sender) = sender_clone {
                                                let message_type = if message_input_mode == "text" {
                                                    MessageType::Text
                                                } else {
                                                    MessageType::Hex
                                                };
                                                let message = Message::new(
                                                    MessageDirection::Sent,
                                                    bytes,
                                                    message_type,
                                                )
                                                .with_source(source_str_clone);
                                                let _ =
                                                    sender.send(ConnectionEvent::MessageReceived(
                                                        tab_id_clone2,
                                                        message,
                                                    ));
                                            }
                                        }
                                    });
                                } else {
                                    error!("[send_message_to_client] 客户端 {} 没有写入器", addr);
                                }
                            } else {
                                error!(
                                    "[send_message_to_client] 未找到服务端客户端映射: {}",
                                    tab_id
                                );
                            }
                        } else {
                            error!("[send_message_to_client] 无效的客户端地址: {}", source_str);
                        }
                    } else {
                        error!("[send_message_to_client] 没有指定客户端，无法发送自动回复");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(
                                tab_id_clone,
                                "无法确定目标客户端".to_string(),
                            ));
                        }
                    }
                }
            } else {
                error!("[send_message_to_client] 连接未建立");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(
                        tab_id_clone,
                        "连接未建立".to_string(),
                    ));
                }
            }
        } else {
            error!("[send_message_to_client] 未找到标签页: {}", tab_id);
        }
    }

    pub fn handle_connection_events(&mut self, cx: &mut Context<Self>) {
        let mut auto_reply_events: Vec<(String, String, Option<String>)> = Vec::new();
        let mut periodic_send_events: Vec<(String, String)> = Vec::new();
        let mut periodic_send_bytes_events: Vec<(String, Vec<u8>, String)> = Vec::new();
        let mut need_notify = false;

        if let Some(ref mut receiver) = self.connection_event_receiver {
            while let Ok(event) = receiver.try_recv() {
                match event {
                    ConnectionEvent::Connected(tab_id) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = true;
                            tab_state.connection_status = ConnectionStatus::Connected;
                            tab_state.error_message = None;
                            need_notify = true;
                        }
                    }
                    ConnectionEvent::Disconnected(tab_id) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = false;
                            tab_state.connection_status = ConnectionStatus::Disconnected;
                            need_notify = true;
                        }
                        self.client_write_senders.remove(&tab_id);
                        self.server_clients.remove(&tab_id);
                    }
                    ConnectionEvent::Listening(tab_id) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = true;
                            tab_state.connection_status = ConnectionStatus::Listening;
                            tab_state.error_message = None;
                            need_notify = true;
                        }
                    }
                    ConnectionEvent::Error(tab_id, error) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = false;
                            tab_state.connection_status = ConnectionStatus::Error;
                            tab_state.error_message = Some(error);
                            need_notify = true;
                        }
                        // 清理连接信息，确保下次发送时直接失败
                        self.client_write_senders.remove(&tab_id);
                        self.server_clients.remove(&tab_id);
                    }
                    ConnectionEvent::ClientWriteSenderReady(tab_id, write_sender) => {
                        info!(
                            "[handle_connection_events] 客户端写入发送器就绪: {}",
                            tab_id
                        );
                        self.client_write_senders.insert(tab_id, write_sender);
                    }
                    ConnectionEvent::ServerClientConnected(tab_id, addr, write_sender) => {
                        info!(
                            "[handle_connection_events] 服务端客户端连接: tab_id={}, addr={}",
                            tab_id, addr
                        );
                        if !self.server_clients.contains_key(&tab_id) {
                            self.server_clients.insert(tab_id.clone(), HashMap::new());
                        }
                        if let Some(clients) = self.server_clients.get_mut(&tab_id) {
                            clients.insert(addr, write_sender);
                        }
                        // 更新 ConnectionTabState 中的客户端连接列表
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            if !tab_state.client_connections.contains(&addr) {
                                tab_state.client_connections.push(addr);
                                need_notify = true;
                            }
                        }
                    }
                    ConnectionEvent::ServerClientDisconnected(tab_id, addr) => {
                        info!(
                            "[handle_connection_events] 服务端客户端断开: tab_id={}, addr={}",
                            tab_id, addr
                        );
                        if let Some(clients) = self.server_clients.get_mut(&tab_id) {
                            clients.remove(&addr);
                        }
                        // 更新 ConnectionTabState 中的客户端连接列表
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state
                                .client_connections
                                .retain(|&client_addr| client_addr != addr);
                            need_notify = true;
                        }
                    }
                    ConnectionEvent::MessageReceived(tab_id, message) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            let mut message = message.clone();
                            let message_for_auto_reply = message.clone();
                            if message.direction == MessageDirection::Received {
                                message.message_type = if tab_state.message_input_mode == "text" {
                                    MessageType::Text
                                } else {
                                    MessageType::Hex
                                };
                            }
                            tab_state.add_message(message);
                            need_notify = true;

                            // 只有当消息方向是 Received 且是真正从网络接收到的消息时才触发自动回复
                            // 避免自动回复生成的消息又被当作新消息处理
                            if tab_state.auto_reply_enabled
                                && message_for_auto_reply.direction == MessageDirection::Received
                            {
                                if let Some(auto_reply_input) = self.auto_reply_inputs.get(&tab_id)
                                {
                                    let auto_reply_content =
                                        auto_reply_input.read(cx).text().to_string();
                                    if !auto_reply_content.trim().is_empty() {
                                        auto_reply_events.push((
                                            tab_id,
                                            auto_reply_content,
                                            message_for_auto_reply.source.clone(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    ConnectionEvent::PeriodicSend(tab_id, content) => {
                        // 处理周期发送文本消息
                        periodic_send_events.push((tab_id, content));
                    }
                    ConnectionEvent::PeriodicSendBytes(tab_id, bytes, hex_input) => {
                        // 处理周期发送十六进制消息
                        periodic_send_bytes_events.push((tab_id, bytes, hex_input));
                    }
                }
            }
        }

        // 处理自动回复事件
        if !auto_reply_events.is_empty() {
            for (tab_id, auto_reply_content, source) in auto_reply_events {
                self.send_message_to_client(tab_id, auto_reply_content, source, cx);
            }
        }

        // 处理周期发送事件
        if !periodic_send_events.is_empty() {
            for (tab_id, content) in periodic_send_events {
                self.send_message(tab_id, content);
            }
        }

        if !periodic_send_bytes_events.is_empty() {
            for (tab_id, bytes, hex_input) in periodic_send_bytes_events {
                self.send_message_bytes(tab_id, bytes, hex_input);
            }
        }

        if need_notify {
            cx.notify();
        }
    }
}

impl Drop for NetAssistantApp {
    fn drop(&mut self) {
        info!("[应用关闭] 开始关闭所有连接");

        let tab_ids: Vec<String> = self.connection_tabs.keys().cloned().collect();
        for tab_id in tab_ids {
            self.close_tab(tab_id);
        }

        info!("[应用关闭] 所有连接已关闭");
    }
}

impl Render for NetAssistantApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.handle_connection_events(cx);

        // 处理主题事件
        let need_notify = cx.global_mut::<ThemeEventHandler>().handle_events();
        if need_notify {
            let is_dark = cx.global::<ThemeEventHandler>().is_dark_mode();
            apply_theme(is_dark, cx);
            cx.notify();
        }

        if !self.active_tab.is_empty() {
            if let Some(tab_state) = self.connection_tabs.get(&self.active_tab) {
                if !tab_state.connection_config.is_client() {
                    self.ensure_auto_reply_input_exists(self.active_tab.clone(), window, cx);
                }
            }
        }

        MainWindow::new(self, cx).render(window, cx)
    }
}
