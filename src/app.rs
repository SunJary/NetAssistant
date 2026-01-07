use gpui::*;
use gpui_component::input::InputState;

use crate::config;
use crate::config::storage::ConfigStorage;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType};
use crate::ui::connection_tab::ConnectionTabState;
use crate::ui::main_window::MainWindow;
use crate::message::{Message, MessageDirection, MessageType, DisplayMode};

use std::collections::HashMap;
use std::net::SocketAddr;
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

    // 发送消息输入框状态
    pub message_input: Entity<InputState>,
    pub message_input_mode: String,
    pub auto_clear_input: bool,

    // 消息显示模式
    pub display_mode: DisplayMode,
}

impl NetAssistantApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let storage = ConfigStorage::new().expect("无法创建配置存储");
        
        // 使用window创建InputState实体
        let host_input = cx.new(|cx| InputState::new(window, cx));
        let port_input = cx.new(|cx| InputState::new(window, cx));
        
        // 创建多行文本输入框
        let message_input = cx.new(|cx| 
            InputState::new(window, cx)
                .code_editor("json")
                .line_number(false)
                // .rows(5)
                .multi_line(true)
                .placeholder("输入消息..."));
        
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
            message_input,
            message_input_mode: String::from("text"),
            auto_clear_input: true,
            display_mode: DisplayMode::Text,
        }
    }

    pub fn ensure_tab_exists(&mut self, tab_id: String, connection_config: config::connection::ConnectionConfig) {
        if !self.connection_tabs.contains_key(&tab_id) {
            self.connection_tabs.insert(
                tab_id.clone(),
                ConnectionTabState::new(connection_config),
            );
        }
    }

    pub fn ensure_auto_reply_input_exists(&mut self, tab_id: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.auto_reply_inputs.contains_key(&tab_id) {
            let auto_reply_input = cx.new(|cx| InputState::new(window, cx)
                .code_editor("json")
                .line_number(false)
                // .rows(5)
                .multi_line(true)
                .placeholder("输入自动回复内容..."));
            auto_reply_input.update(cx, |input, cx| {
                input.set_value("ok".to_string(), window, cx);
            });
            self.auto_reply_inputs.insert(tab_id, auto_reply_input);
        }
    }

    pub fn close_tab(&mut self, tab_id: String) {
        println!("[关闭标签页] 开始关闭标签页: {}", tab_id);
        
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }
        
        if self.connection_tabs.remove(&tab_id).is_some() {
            println!("[关闭标签页] 移除标签页状态: {}", tab_id);
        }
        
        if self.auto_reply_inputs.remove(&tab_id).is_some() {
            println!("[关闭标签页] 移除自动回复输入框: {}", tab_id);
        }
        
        println!("[关闭标签页] 标签页 {} 已关闭", tab_id);
    }

    pub fn sanitize_hex_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.message_input_mode == "hex" {
            let current_text = self.message_input.read(cx).text().to_string();
            let sanitized: String = current_text
                .chars()
                .filter(|c| c.is_ascii_hexdigit() || c.is_ascii_whitespace())
                .collect();
            
            if sanitized != current_text {
                self.message_input.update(cx, |input, cx| {
                    input.set_value(sanitized, window, cx);
                });
            }
        }
    }

    pub fn connect_client(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            if !tab_state.is_connected && tab_state.connection_config.is_client() {
                if let ConnectionConfig::Client(client_config) = &tab_state.connection_config {
                    let address = format!("{}:{}", client_config.server_address, client_config.server_port);
                    let sender = self.connection_event_sender.clone();
                    let tab_id_clone = tab_id.clone();
                    
                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        tab_state.connection_status = ConnectionStatus::Connecting;
                    }
                    
                    tokio::spawn(async move {
                        match tokio::net::TcpStream::connect(&address).await {
                            Ok(stream) => {
                                let peer_addr = stream.peer_addr().ok();
                                println!("[客户端] 连接成功: {:?}", peer_addr);
                                
                                let (mut reader, mut writer) = stream.into_split();
                                let (write_sender, mut write_receiver) = mpsc::unbounded_channel::<Vec<u8>>();
                                
                                let sender_clone = sender.clone();
                                let tab_id_clone2 = tab_id_clone.clone();
                                
                                // 保存write_sender到映射（需要在UI线程中操作）
                                let tab_id_clone_for_sender = tab_id_clone.clone();
                                let write_sender_clone = write_sender.clone();
                                let sender_clone_for_map = sender.clone();
                                tokio::spawn(async move {
                                    if let Some(sender) = sender_clone_for_map {
                                        let _ = sender.send(ConnectionEvent::ClientWriteSenderReady(tab_id_clone_for_sender, write_sender_clone));
                                    }
                                });
                                
                                // 启动接收任务
                                tokio::spawn(async move {
                                    println!("[客户端] 启动接收任务");
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
                                                        let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone2.clone(), message));
                                                
                                                    }
                                                } else {
                                                    println!("[客户端] 接收到0字节，连接已关闭");
                                                    break;
                                                }
                                            }
                                            Err(e) => {
                                                println!("[客户端] 接收数据失败: {}", e);
                                                break;
                                            }
                                        }
                                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                                    }
                                    println!("[客户端] 接收任务结束");
                                });
                                
                                // 启动写入任务
                                let sender_clone2 = sender.clone();
                                let tab_id_clone3 = tab_id_clone.clone();
                                tokio::spawn(async move {
                                    println!("[客户端] 启动写入任务");
                                    while let Some(data) = write_receiver.recv().await {
                                        let result = writer.write_all(&data).await;
                                        if let Err(e) = result {
                                            println!("[客户端] 写入数据失败: {}", e);
                                            if let Some(sender) = sender_clone2.clone() {
                                                let _ = sender.send(ConnectionEvent::Error(tab_id_clone3, e.to_string()));
                                            }
                                            break;
                                        }
                                    }
                                    println!("[客户端] 写入任务结束");
                                });
                                
                                // 通知UI连接成功
                                let sender_clone3 = sender.clone();
                                if let Some(sender) = sender_clone3 {
                                    let _ = sender.send(ConnectionEvent::Connected(tab_id_clone));
                                }
                            }
                            Err(e) => {
                                println!("[客户端] 连接失败: {}", e);
                                let sender_clone4 = sender.clone();
                                if let Some(sender) = sender_clone4 {
                                    let _ = sender.send(ConnectionEvent::Error(tab_id_clone, e.to_string()));
                                }
                            }
                        }
                    });
                }
            }
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

    pub fn start_server(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            if !tab_state.is_connected && tab_state.connection_config.is_server() {
                if let ConnectionConfig::Server(server_config) = &tab_state.connection_config {
                    let server_config_clone = server_config.clone();
                    let sender = self.connection_event_sender.clone();
                    let tab_id_clone = tab_id.clone();
                    
                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        tab_state.connection_status = ConnectionStatus::Connecting;
                    }
                    
                    tokio::spawn(async move {
                        match server_config_clone.protocol {
                            ConnectionType::Tcp => {
                                let address = format!("{}:{}", server_config_clone.listen_address, server_config_clone.listen_port);
                                match tokio::net::TcpListener::bind(&address).await {
                                    Ok(listener) => {
                                        println!("[服务端] TCP监听器启动成功: {}", address);
                                        
                                        if let Some(sender) = sender.clone() {
                                            let _ = sender.send(ConnectionEvent::Listening(tab_id_clone.clone()));
                                        }
                                        
                                        let sender_clone = sender.clone();
                                        let tab_id_clone2 = tab_id_clone.clone();
                                        
                                        loop {
                                            match listener.accept().await {
                                                Ok((stream, addr)) => {
                                                    println!("[服务端] 接受新客户端连接: {}", addr);
                                                    
                                                    let (mut reader, mut writer) = stream.into_split();
                                                    let (write_sender, mut write_receiver) = mpsc::unbounded_channel::<Vec<u8>>();
                                                    
                                                    // 保存write_sender到映射（需要在UI线程中操作）
                                                    let sender_clone_for_map = sender_clone.clone();
                                                    let tab_id_clone_for_map = tab_id_clone2.clone();
                                                    let write_sender_clone = write_sender.clone();
                                                    tokio::spawn(async move {
                                                        if let Some(sender) = sender_clone_for_map {
                                                            let _ = sender.send(ConnectionEvent::ServerClientConnected(tab_id_clone_for_map, addr, write_sender_clone));
                                                        }
                                                    });
                                                    
                                                    let sender_clone2 = sender_clone.clone();
                                                    let tab_id_clone3 = tab_id_clone2.clone();
                                                    
                                                    tokio::spawn(async move {
                                                        println!("[服务端] 开始为客户端 {} 启动消息接收任务", addr);
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
                                                                        ).with_source(addr.to_string());
                                                                        if let Some(sender) = sender_clone2.clone() {
                                                                            let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone3.clone(), message));
                                                                        }
                                                                    } else {
                                                                        println!("[服务端] 客户端 {} 接收到0字节，连接已关闭", addr);
                                                                        let sender_clone4 = sender_clone2.clone();
                                                                        let tab_id_clone4 = tab_id_clone3.clone();
                                                                        if let Some(sender) = sender_clone4 {
                                                                            let _ = sender.send(ConnectionEvent::ServerClientDisconnected(tab_id_clone4, addr));
                                                                        }
                                                                        break;
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    println!("[服务端] 客户端 {} 读取数据失败: {}", addr, e);
                                                                    let sender_clone4 = sender_clone2.clone();
                                                                    let tab_id_clone4 = tab_id_clone3.clone();
                                                                    if let Some(sender) = sender_clone4 {
                                                                        let _ = sender.send(ConnectionEvent::ServerClientDisconnected(tab_id_clone4, addr));
                                                                    }
                                                                    break;
                                                                }
                                                            }
                                                            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                                                        }
                                                        println!("[服务端] 客户端 {} 的消息接收任务结束", addr);
                                                    });
                                                    
                                                    let sender_clone3 = sender_clone.clone();
                                                    let tab_id_clone4 = tab_id_clone2.clone();
                                                    tokio::spawn(async move {
                                                        println!("[服务端] 开始为客户端 {} 启动写入任务", addr);
                                                        while let Some(data) = write_receiver.recv().await {
                                                            let result = writer.write_all(&data).await;
                                                            if let Err(e) = result {
                                                                println!("[服务端] 客户端 {} 写入数据失败: {}", addr, e);
                                                                if let Some(sender) = sender_clone3.clone() {
                                                                    let _ = sender.send(ConnectionEvent::Error(tab_id_clone4, e.to_string()));
                                                                }
                                                                break;
                                                            }
                                                        }
                                                        println!("[服务端] 客户端 {} 的写入任务结束", addr);
                                                    });
                                                }
                                                Err(e) => {
                                                    println!("[服务端] 接受连接失败: {}", e);
                                                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!("[服务端] TCP监听器启动失败: {}", e);
                                        let sender_clone5 = sender.clone();
                                        if let Some(sender) = sender_clone5 {
                                            let _ = sender.send(ConnectionEvent::Error(tab_id_clone, e.to_string()));
                                        }
                                    }
                                }
                            }
                            ConnectionType::Udp => {
                                let address = format!("{}:{}", server_config_clone.listen_address, server_config_clone.listen_port);
                                match tokio::net::UdpSocket::bind(&address).await {
                                    Ok(socket) => {
                                        println!("[服务端] UDP套接字启动成功: {}", address);
                                        
                                        let sender_clone7 = sender.clone();
                                        if let Some(sender) = sender_clone7 {
                                            let _ = sender.send(ConnectionEvent::Listening(tab_id_clone.clone()));
                                        }
                                        
                                        let sender_clone = sender.clone();
                                        let tab_id_clone2 = tab_id_clone.clone();
                                        
                                        tokio::spawn(async move {
                                            loop {
                                                let mut buffer = vec![0u8; 4096];
                                                match socket.recv_from(&mut buffer).await {
                                                    Ok((n, addr)) => {
                                                        if n > 0 {
                                                            buffer.truncate(n);
                                                            let message = Message::new(
                                                                MessageDirection::Received,
                                                                buffer.clone(),
                                                                MessageType::Text,
                                                            ).with_source(addr.to_string());
                                                            if let Some(sender) = sender_clone.clone() {
                                                                let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone2.clone(), message));
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        println!("[服务端] UDP接收失败: {}", e);
                                                        break;
                                                    }
                                                }
                                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        println!("[服务端] UDP套接字启动失败: {}", e);
                                        let sender_clone6 = sender.clone();
                                        if let Some(sender) = sender_clone6 {
                                            let _ = sender.send(ConnectionEvent::Error(tab_id_clone, e.to_string()));
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }
        }
    }

    pub fn stop_server(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }
    }

    pub fn toggle_connection(&mut self, tab_id: String, cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            if tab_state.is_connected {
                if tab_state.connection_config.is_client() {
                    self.disconnect_client(tab_id, cx);
                } else {
                    self.stop_server(tab_id, cx);
                }
            } else {
                if tab_state.connection_config.is_client() {
                    self.connect_client(tab_id, cx);
                } else {
                    self.start_server(tab_id, cx);
                }
            }
        }
    }

    pub fn send_message(&mut self, tab_id: String, content: String, _cx: &mut Context<Self>) {
        println!("[send_message] 开始，tab_id: {}, content: '{}'", tab_id, content);
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let bytes = content.into_bytes();
        
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            println!("[send_message] 找到标签页，is_connected: {}, connection_config: {:?}", 
                tab_state.is_connected, tab_state.connection_config);
            if tab_state.is_connected {
                if tab_state.connection_config.is_client() {
                    println!("[send_message] 客户端模式");
                    if let Some(write_sender) = self.client_write_senders.get(&tab_id).cloned() {
                        let bytes_clone = bytes.clone();
                        tokio::spawn(async move {
                            println!("[send_message] 异步任务开始发送");
                            let result: Result<(), mpsc::error::SendError<Vec<u8>>> = write_sender.send(bytes_clone);
                            if let Err(e) = result {
                                println!("[send_message] 发送失败: {}", e);
                                if let Some(sender) = sender {
                                    let _ = sender.send(ConnectionEvent::Error(tab_id_clone, format!("发送失败: {}", e)));
                                }
                            } else {
                                println!("[send_message] 发送成功");
                                if let Some(sender) = sender {
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        MessageType::Text,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                                }
                            }
                        });
                    } else {
                        println!("[send_message] 未找到TCP写入器");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "TCP写入器未初始化".to_string()));
                        }
                    }
                } else {
                    println!("[send_message] 服务端模式");
                    let clients: Vec<(SocketAddr, mpsc::UnboundedSender<Vec<u8>>)> = self.server_clients
                        .get(&tab_id)
                        .map(|clients| clients.iter().map(|(addr, sender)| (*addr, sender.clone())).collect())
                        .unwrap_or_default();
                    
                    if clients.is_empty() {
                        println!("[send_message] 没有连接的客户端");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "没有连接的客户端".to_string()));
                        }
                    } else {
                        let sender_clone = sender.clone();
                        let tab_id_clone2 = tab_id_clone.clone();
                        tokio::spawn(async move {
                            println!("[send_message] 异步任务开始广播");
                            let mut success_count = 0;
                            for (addr, write_sender) in clients {
                                if let Err(_e) = write_sender.send(bytes.clone()) {
                                    println!("[send_message] 发送给客户端 {} 失败", addr);
                                } else {
                                    success_count += 1;
                                }
                            }

                            if success_count > 0 {
                                println!("[send_message] 广播成功，发送给 {} 个客户端", success_count);
                                if let Some(sender) = sender_clone {
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        MessageType::Text,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone2, message));
                                }
                            }

                        });
                    }
                }
            } else {
                println!("[send_message] 连接未建立");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "连接未建立".to_string()));
                }
            }
        } else {
            println!("[send_message] 未找到标签页: {}", tab_id);
        }
    }

    pub fn send_message_bytes(&mut self, tab_id: String, bytes: Vec<u8>, hex_input: String, _cx: &mut Context<Self>) {
        println!("[send_message_bytes] 开始，tab_id: {}, bytes: {:?}, hex_input: '{}'", tab_id, bytes, hex_input);
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            println!("[send_message_bytes] 找到标签页，is_connected: {}, connection_config: {:?}", 
                tab_state.is_connected, tab_state.connection_config);
            if tab_state.is_connected {
                if tab_state.connection_config.is_client() {
                    println!("[send_message_bytes] 客户端模式");
                    if let Some(write_sender) = self.client_write_senders.get(&tab_id).cloned() {
                        let bytes_clone = bytes.clone();
                        tokio::spawn(async move {
                            println!("[send_message_bytes] 异步任务开始发送");
                            let result: Result<(), mpsc::error::SendError<Vec<u8>>> = write_sender.send(bytes_clone);
                            if let Err(e) = result {
                                println!("[send_message_bytes] 发送失败: {}", e);
                                if let Some(sender) = sender {
                                    let _ = sender.send(ConnectionEvent::Error(tab_id_clone, format!("发送失败: {}", e)));
                                }
                            } else {
                                println!("[send_message_bytes] 发送成功");
                                if let Some(sender) = sender {
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        MessageType::Hex,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                                }
                            }
                        });
                    } else {
                        println!("[send_message_bytes] 未找到TCP写入器");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "TCP写入器未初始化".to_string()));
                        }
                    }
                } else {
                    println!("[send_message_bytes] 服务端模式");
                    let clients: Vec<(SocketAddr, mpsc::UnboundedSender<Vec<u8>>)> = self.server_clients
                        .get(&tab_id)
                        .map(|clients| clients.iter().map(|(addr, sender)| (*addr, sender.clone())).collect())
                        .unwrap_or_default();
                    
                    if clients.is_empty() {
                        println!("[send_message_bytes] 没有连接的客户端");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "没有连接的客户端".to_string()));
                        }
                    } else {
                        let sender_clone = sender.clone();
                        let tab_id_clone2 = tab_id_clone.clone();
                        let bytes_clone = bytes.clone();
                        tokio::spawn(async move {
                            println!("[send_message_bytes] 异步任务开始广播");
                            let mut success_count = 0;
                            for (addr, write_sender) in clients {
                                if let Err(_e) = write_sender.send(bytes_clone.clone()) {
                                    println!("[send_message_bytes] 发送给客户端 {} 失败", addr);
                                } else {
                                    success_count += 1;
                                }
                            }
                            if success_count > 0 {
                                println!("[send_message_bytes] 广播成功，发送给 {} 个客户端", success_count);
                                if let Some(sender) = sender_clone {
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes_clone,
                                        MessageType::Hex,
                                    );
                                    let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone2, message));
                                }
                            }
                        });
                    }
                }
            } else {
                println!("[send_message_bytes] 连接未建立");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "连接未建立".to_string()));
                }
            }
        } else {
            println!("[send_message_bytes] 未找到标签页: {}", tab_id);
        }
    }

    pub fn send_message_to_client(&mut self, tab_id: String, content: String, source: Option<String>, _cx: &mut Context<Self>) {
        println!("[send_message_to_client] 开始，tab_id: {}, content: '{}', source: {:?}", tab_id, content, source);
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let bytes = content.clone().into_bytes();
        
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            println!("[send_message_to_client] 找到标签页，is_connected: {}, connection_config: {:?}", 
                tab_state.is_connected, tab_state.connection_config);
            if tab_state.is_connected {
                if tab_state.connection_config.is_client() {
                    println!("[send_message_to_client] 客户端模式，直接发送给服务器");
                    self.send_message(tab_id, content, _cx);
                } else {
                    println!("[send_message_to_client] 服务端模式");
                    
                    if let Some(source_str) = source {
                        if let Ok(addr) = source_str.parse::<std::net::SocketAddr>() {
                            println!("[send_message_to_client] 发送给指定客户端: {}", addr);
                            if let Some(clients) = self.server_clients.get(&tab_id) {
                                if let Some(write_sender) = clients.get(&addr).cloned() {
                                    let sender_clone = sender.clone();
                                    let tab_id_clone2 = tab_id_clone.clone();
                                    let bytes_clone = bytes.clone();
                                    let source_str_clone = source_str.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = write_sender.send(bytes_clone) {
                                            println!("[send_message_to_client] 发送失败: {}", e);
                                            if let Some(sender) = sender_clone {
                                                let _ = sender.send(ConnectionEvent::Error(tab_id_clone2, e.to_string()));
                                            }
                                        } else {
                                            println!("[send_message_to_client] 发送成功");
                                            if let Some(sender) = sender_clone {
                                                let message = Message::new(
                                                    MessageDirection::Sent,
                                                    bytes,
                                                    MessageType::Text,
                                                ).with_source(source_str_clone);
                                                let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone2, message));
                                            }
                                        }
                                    });
                                } else {
                                    println!("[send_message_to_client] 客户端 {} 没有写入器", addr);
                                }
                            } else {
                                println!("[send_message_to_client] 未找到服务端客户端映射: {}", tab_id);
                            }
                        } else {
                            println!("[send_message_to_client] 无效的客户端地址: {}", source_str);
                        }
                    } else {
                        println!("[send_message_to_client] 没有指定客户端，无法发送自动回复");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "无法确定目标客户端".to_string()));
                        }
                    }
                }
            } else {
                println!("[send_message_to_client] 连接未建立");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(tab_id_clone, "连接未建立".to_string()));
                }
            }
        } else {
            println!("[send_message_to_client] 未找到标签页: {}", tab_id);
        }
    }

    fn handle_connection_events(&mut self, cx: &mut Context<Self>) {
        let mut auto_reply_events: Vec<(String, String, Option<String>)> = Vec::new();
        
        if let Some(ref mut receiver) = self.connection_event_receiver {
            while let Ok(event) = receiver.try_recv() {
                match event {
                    ConnectionEvent::Connected(tab_id) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = true;
                            tab_state.connection_status = ConnectionStatus::Connected;
                            tab_state.error_message = None;
                        }
                    }
                    ConnectionEvent::Disconnected(tab_id) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = false;
                            tab_state.connection_status = ConnectionStatus::Disconnected;
                        }
                        self.client_write_senders.remove(&tab_id);
                        self.server_clients.remove(&tab_id);
                    }
                    ConnectionEvent::Listening(tab_id) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = true;
                            tab_state.connection_status = ConnectionStatus::Listening;
                            tab_state.error_message = None;
                        }
                    }
                    ConnectionEvent::Error(tab_id, error) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.is_connected = false;
                            tab_state.connection_status = ConnectionStatus::Error;
                            tab_state.error_message = Some(error);
                        }
                    }
                    ConnectionEvent::ClientWriteSenderReady(tab_id, write_sender) => {
                        println!("[handle_connection_events] 客户端写入发送器就绪: {}", tab_id);
                        self.client_write_senders.insert(tab_id, write_sender);
                    }
                    ConnectionEvent::ServerClientConnected(tab_id, addr, write_sender) => {
                        println!("[handle_connection_events] 服务端客户端连接: tab_id={}, addr={}", tab_id, addr);
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
                            }
                        }
                    }
                    ConnectionEvent::ServerClientDisconnected(tab_id, addr) => {
                        println!("[handle_connection_events] 服务端客户端断开: tab_id={}, addr={}", tab_id, addr);
                        if let Some(clients) = self.server_clients.get_mut(&tab_id) {
                            clients.remove(&addr);
                        }
                        // 更新 ConnectionTabState 中的客户端连接列表
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.client_connections.retain(|&client_addr| client_addr != addr);
                        }
                    }
                    ConnectionEvent::MessageReceived(tab_id, message) => {
                        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                            tab_state.add_message(message.clone());
                            
                            if tab_state.auto_reply_enabled && message.direction == MessageDirection::Received {
                                if let Some(auto_reply_input) = self.auto_reply_inputs.get(&tab_id) {
                                    let auto_reply_content = auto_reply_input.read(cx).text().to_string();
                                    if !auto_reply_content.trim().is_empty() {
                                        auto_reply_events.push((tab_id, auto_reply_content, message.source.clone()));
                                    }
                                }
                            }
                        }
                        cx.notify();
                    }
                }
            }
        }
        
        if !auto_reply_events.is_empty() {
            for (tab_id, auto_reply_content, source) in auto_reply_events {
                self.send_message_to_client(tab_id, auto_reply_content, source, cx);
            }
        }
    }
}

impl Drop for NetAssistantApp {
    fn drop(&mut self) {
        println!("[应用关闭] 开始关闭所有连接");
        
        let tab_ids: Vec<String> = self.connection_tabs.keys().cloned().collect();
        for tab_id in tab_ids {
            self.close_tab(tab_id);
        }
        
        println!("[应用关闭] 所有连接已关闭");
    }
}

impl Render for NetAssistantApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.handle_connection_events(cx);
        
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
