use gpui::*;
use gpui_component::input::InputState;
use log::{debug, error, info};


use crate::config;
use crate::config::connection::{ConnectionConfig, ConnectionStatus};
use crate::config::storage::ConfigStorage;
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::ConnectionEvent;
use crate::theme_event_handler::{ThemeEventHandler, apply_theme};
use crate::ui::connection_tab::ConnectionTabState;
use crate::ui::main_window::MainWindow;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

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

    // 解码器选择对话框状态
    pub show_decoder_selection: bool,
    pub decoder_selection_tab_id: Option<String>,
    pub decoder_selection_config: Option<crate::config::connection::DecoderConfig>,

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

    // 网络连接管理器
    pub network_manager: std::sync::Arc<tokio::sync::Mutex<crate::network::connection::manager::NetworkConnectionManager>>,
    
    // 消息处理器
    pub message_processor: std::sync::Arc<crate::core::message_processor::DefaultMessageProcessor>,

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

        // 初始化网络连接管理器
        let network_manager = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::network::connection::manager::NetworkConnectionManager::new()
        ));

        // 初始化写入发送器映射
        let client_write_senders = HashMap::new();
        let server_clients = HashMap::new();
        
        // 初始化消息处理器
        let message_processor = std::sync::Arc::new(
            crate::core::message_processor::DefaultMessageProcessor::new()
        );

        Self {
            storage,
            client_expanded: true,
            show_new_connection: false,
            new_connection_is_client: true,
            host_input,
            port_input,
            new_connection_protocol: String::from("TCP"),
            // 初始化解码器选择对话框状态
            show_decoder_selection: false,
            decoder_selection_tab_id: None,
            decoder_selection_config: None,
            server_expanded: true,
            active_tab,
            connection_tabs,
            tab_multiline: false,
            auto_reply_inputs: HashMap::new(),
            connection_event_sender: Some(connection_event_sender),
            connection_event_receiver: Some(connection_event_receiver),
            network_manager,
            message_processor,
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
                    self.disconnect_server(tab_id);
                }
            } else {
                    // 建立连接
                    if tab_state.connection_config.is_client() {
                        self.connect_to_server(tab_id);
                    } else {
                        self.start_server(tab_id);
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


    pub fn disconnect_client(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let network_manager_arc = self.network_manager.clone();

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }

        tokio::spawn(async move {
                            // 断开网络连接
                            let mut network_manager = network_manager_arc.lock().await;
                            if let Err(e) = network_manager.disconnect_client(&tab_id_clone).await {
                error!("断开客户端连接失败: {:?}", e);
            }
            
            // 发送断开连接事件
            if let Some(sender) = sender {
                let _ = sender.send(ConnectionEvent::Disconnected(tab_id_clone));
            }
        });
    }

    /// 服务端断开连接
    pub fn disconnect_server(&mut self, tab_id: String) {
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let network_manager_arc = self.network_manager.clone();

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }
        
        self.server_clients.remove(&tab_id);

        tokio::spawn(async move {
            let mut network_manager = network_manager_arc.lock().await;
            if let Err(e) = network_manager.stop_server(&tab_id_clone).await {
                error!("停止服务器失败: {:?}", e);
            }
            
            if let Some(sender) = sender {
                let _ = sender.send(ConnectionEvent::Disconnected(tab_id_clone));
            }
        });
    }

    /// 客户端连接到服务端
    pub fn connect_to_server(&mut self, tab_id: String) {
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            let client_config = if let ConnectionConfig::Client(client_config) = tab_state.connection_config.clone() {
                client_config
            } else {
                return;
            };
            
            let network_manager_arc = self.network_manager.clone();
            let client_config_clone = client_config.clone();
            let connection_event_sender_clone = self.connection_event_sender.clone();
            
            tokio::spawn(async move {
                let mut network_manager = network_manager_arc.lock().await;
                if let Err(e) = network_manager.create_and_connect_client(&client_config_clone, connection_event_sender_clone).await {
                    error!("客户端连接失败: {:?}", e);
                }
            });
        }
    }

    /// 服务端启动
    pub fn start_server(&mut self, tab_id: String) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            // 立即更新UI状态为正在启动
            tab_state.is_connected = true;
            tab_state.connection_status = ConnectionStatus::Connecting;
            
            if let ConnectionConfig::Server(server_config) = &tab_state.connection_config {
                let network_manager_arc = self.network_manager.clone();
                let server_config_clone = server_config.clone();
                let connection_event_sender_clone = self.connection_event_sender.clone();
                
                tokio::spawn(async move {
                    let mut network_manager = network_manager_arc.lock().await;
                    if let Err(e) = network_manager.create_and_start_server(&server_config_clone, connection_event_sender_clone).await {
                        error!("服务端启动失败: {:?}", e);
                    }
                });
            }
        }
    }

    pub fn send_message(&mut self, tab_id: String, content: String) {
        info!(
            "[send_message] 开始，tab_id: {}, content: '{}'",
            tab_id, content
        );
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let content_clone = content.clone();
        
        // 保存message_type用于后续事件发送
        let message_type_result = self.connection_tabs.get(&tab_id)
            .map(|tab_state| {
                if tab_state.message_input_mode == "text" {
                    MessageType::Text
                } else {
                    MessageType::Hex
                }
            });
        
        if message_type_result.is_none() {
            error!("[send_message] 未找到标签页: {}", tab_id);
            return;
        }
        
        let message_type = message_type_result.unwrap();
        
        // 在闭包外部获取必要的信息
        let is_connected_result = self.connection_tabs.get(&tab_id).map(|tab| tab.is_connected);
        let is_client_result = self.connection_tabs.get(&tab_id)
            .map(|tab| tab.connection_config.is_client());
        
        if is_connected_result.is_none() || is_client_result.is_none() {
            error!("[send_message] 未找到标签页: {}", tab_id);
            return;
        }
        
        let is_connected = is_connected_result.unwrap();
        let is_client = is_client_result.unwrap();
        
        if !is_connected {
            if let Some(sender) = sender {
                let _ = sender.send(ConnectionEvent::Error(
                    tab_id_clone,
                    "连接未建立".to_string(),
                ));
            }
            return;
        }
        
        // 直接使用client_write_senders和server_clients来发送消息
        let bytes = content_clone.into_bytes();
        
        if is_client {
            // 客户端模式：发送给服务器
            debug!("[send_message] 客户端模式，发送给服务器");
            
            if let Some(write_sender) = self.client_write_senders.get(&tab_id) {
                if let Err(e) = write_sender.send(bytes.clone()) {
                    error!("[send_message] 无法发送消息到服务器: {}", e);
                    if let Some(sender) = sender {
                        let _ = sender.send(ConnectionEvent::Error(
                            tab_id_clone,
                            e.to_string(),
                        ));
                    }
                } else {
                    debug!("[send_message] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message] 客户端写入发送器不可用");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(
                        tab_id_clone,
                        "客户端写入发送器不可用".to_string(),
                    ));
                }
            }
        } else {
            // 服务器模式：广播给所有客户端
            debug!("[send_message] 服务端模式，广播给所有客户端");
            
            if let Some(clients) = self.server_clients.get(&tab_id) {
                if clients.is_empty() {
                    error!("[send_message] 没有可用的客户端连接");
                    if let Some(sender) = sender {
                        let _ = sender.send(ConnectionEvent::Error(
                            tab_id_clone,
                            "没有可用的客户端连接".to_string(),
                        ));
                    }
                } else {
                    for (_, write_sender) in clients {
                        if let Err(e) = write_sender.send(bytes.clone()) {
                            error!("[send_message] 发送给客户端失败: {}", e);
                        }
                    }
                    
                    debug!("[send_message] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message] 服务器客户端映射不可用");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(
                        tab_id_clone,
                        "服务器客户端映射不可用".to_string(),
                    ));
                }
            }
        }
    }

    pub fn send_message_bytes(&mut self, tab_id: String, bytes: Vec<u8>, hex_input: String) {
        info!(
            "[send_message_bytes] 开始，tab_id: {}, bytes: {:?}, hex_input: '{}'",
            tab_id, bytes, hex_input
        );
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        
        // 保存message_type用于后续事件发送
        let message_type_result = self.connection_tabs.get(&tab_id)
            .map(|tab_state| {
                if tab_state.message_input_mode == "text" {
                    MessageType::Text
                } else {
                    MessageType::Hex
                }
            });
        
        if message_type_result.is_none() {
            error!("[send_message_bytes] 未找到标签页: {}", tab_id);
            return;
        }
        
        let message_type = message_type_result.unwrap();
        
        // 在闭包外部获取必要的信息
        let is_connected_result = self.connection_tabs.get(&tab_id).map(|tab| tab.is_connected);
        let is_client_result = self.connection_tabs.get(&tab_id)
            .map(|tab| tab.connection_config.is_client());
        
        if is_connected_result.is_none() || is_client_result.is_none() {
            error!("[send_message_bytes] 未找到标签页: {}", tab_id);
            return;
        }
        
        let is_connected = is_connected_result.unwrap();
        let is_client = is_client_result.unwrap();
        
        if !is_connected {
            if let Some(sender) = sender {
                let _ = sender.send(ConnectionEvent::Error(
                    tab_id_clone,
                    "连接未建立".to_string(),
                ));
            }
            return;
        }
        
        // 直接使用client_write_senders和server_clients来发送消息
        if is_client {
            // 客户端模式：发送给服务器
            debug!("[send_message_bytes] 客户端模式，发送给服务器");
            
            if let Some(write_sender) = self.client_write_senders.get(&tab_id) {
                if let Err(e) = write_sender.send(bytes.clone()) {
                    error!("[send_message_bytes] 无法发送消息到服务器: {}", e);
                    if let Some(sender) = sender {
                        let _ = sender.send(ConnectionEvent::Error(
                            tab_id_clone,
                            e.to_string(),
                        ));
                    }
                } else {
                    debug!("[send_message_bytes] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message_bytes] 客户端写入发送器不可用");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(
                        tab_id_clone,
                        "客户端写入发送器不可用".to_string(),
                    ));
                }
            }
        } else {
            // 服务器模式：广播给所有客户端
            debug!("[send_message_bytes] 服务端模式，广播给所有客户端");
            
            if let Some(clients) = self.server_clients.get(&tab_id) {
                if clients.is_empty() {
                    error!("[send_message_bytes] 没有可用的客户端连接");
                    if let Some(sender) = sender {
                        let _ = sender.send(ConnectionEvent::Error(
                            tab_id_clone,
                            "没有可用的客户端连接".to_string(),
                        ));
                    }
                } else {
                    for (_, write_sender) in clients {
                        if let Err(e) = write_sender.send(bytes.clone()) {
                            error!("[send_message_bytes] 发送给客户端失败: {}", e);
                        }
                    }
                    
                    debug!("[send_message_bytes] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message_bytes] 服务器客户端映射不可用");
                if let Some(sender) = sender {
                    let _ = sender.send(ConnectionEvent::Error(
                        tab_id_clone,
                        "服务器客户端映射不可用".to_string(),
                    ));
                }
            }
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
        let content_clone = content.clone();
        
        // 获取标签页信息
        let tab_state_result = self.connection_tabs.get(&tab_id);
        
        if tab_state_result.is_none() {
            error!("[send_message_to_client] 未找到标签页: {}", tab_id);
            return;
        }
        
        let tab_state = tab_state_result.unwrap();
        let message_type = if tab_state.message_input_mode == "text" {
            MessageType::Text
        } else {
            MessageType::Hex
        };
        
        // 检查连接状态
        if !tab_state.is_connected && !tab_state.connection_config.is_server() {
            error!("[send_message_to_client] 连接未建立");
            if let Some(sender) = sender {
                let _ = sender.send(ConnectionEvent::Error(
                    tab_id_clone,
                    "连接未建立".to_string(),
                ));
            }
            return;
        }
        
        // 客户端模式：直接发送给服务器
        if tab_state.connection_config.is_client() {
            debug!("[send_message_to_client] 客户端模式，直接发送给服务器");
            if tab_state.message_input_mode == "hex" {
                // 十六进制模式：解析十六进制内容并发送字节数组
                let bytes = crate::utils::hex::hex_to_bytes(&content_clone);
                self.send_message_bytes(tab_id, bytes, content_clone);
            } else {
                // 文本模式：直接发送文本内容
                self.send_message(tab_id, content);
            }
            return;
        }
        
        // 服务器模式：发送给指定客户端
        debug!("[send_message_to_client] 服务端模式");
        
        if let Some(source_str) = source {
            // 解析客户端地址
            match source_str.parse::<std::net::SocketAddr>() {
                Ok(addr) => {
                    info!("[send_message_to_client] 发送给指定客户端: {}", addr);
                    let bytes = if tab_state.message_input_mode == "hex" {
                        // 十六进制模式：解析十六进制内容
                        crate::utils::hex::hex_to_bytes(&content_clone)
                    } else {
                        // 文本模式：直接转换为字节
                        content_clone.into_bytes()
                    };
                    
                    // 直接使用server_clients发送消息给指定客户端
                    if let Some(clients) = self.server_clients.get(&tab_id) {
                        if let Some(write_sender) = clients.get(&addr) {
                            if let Err(e) = write_sender.send(bytes.clone()) {
                                error!("[send_message_to_client] 发送失败: {}", e);
                                if let Some(sender) = sender {
                                    let _ = sender.send(ConnectionEvent::Error(
                                        tab_id_clone,
                                        e.to_string(),
                                    ));
                                }
                            } else {
                                debug!("[send_message_to_client] 发送成功");
                                if let Some(sender) = sender {
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        message_type,
                                    )
                                    .with_source(source_str);
                                    let _ = sender.send(ConnectionEvent::MessageReceived(
                                        tab_id_clone,
                                        message,
                                    ));
                                }
                            }
                        } else {
                            error!("[send_message_to_client] 客户端 {} 不存在", addr);
                            if let Some(sender) = sender {
                                let _ = sender.send(ConnectionEvent::Error(
                                    tab_id_clone,
                                    format!("客户端 {} 不存在", addr),
                                ));
                            }
                        }
                    } else {
                        error!("[send_message_to_client] 服务器客户端映射不可用");
                        if let Some(sender) = sender {
                            let _ = sender.send(ConnectionEvent::Error(
                                tab_id_clone,
                                "服务器客户端映射不可用".to_string(),
                            ));
                        }
                    }
                },
                Err(_) => {
                    error!("[send_message_to_client] 无效的客户端地址: {}", source_str);
                },
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

    pub fn update_tab_decoder_config(&mut self, tab_id: String, new_config: crate::config::connection::DecoderConfig, cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            match &mut tab_state.connection_config {
                ConnectionConfig::Client(config) => {
                    config.decoder_config = new_config;
                },
                ConnectionConfig::Server(config) => {
                    config.decoder_config = new_config;
                },
            }
            cx.notify();
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
