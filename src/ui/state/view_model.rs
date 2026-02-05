use gpui::*;
use gpui_component::input::InputState;
use std::collections::HashMap;
use std::net::SocketAddr;
use crate::app::NetAssistantApp;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType};
use crate::core::state::AppState;
use crate::core::tab_manager::TabState;
use crate::message::{Message, MessageType};

/// 视图模型：连接标签页的UI数据
pub struct ConnectionTabViewModel {
    pub id: String,
    pub name: String,
    pub protocol: ConnectionType,
    pub status: ConnectionStatus,
    pub is_connected: bool,
    pub message_input_mode: String,
    pub message_list: Vec<Message>,
    pub client_connections: Vec<String>, // 格式化的客户端地址
    pub selected_client: Option<String>,
    pub error_message: Option<String>,
}

impl From<&TabState> for ConnectionTabViewModel {
    fn from(tab: &TabState) -> Self {
        ConnectionTabViewModel {
            id: tab.id.clone(),
            name: tab.connection_config.name().to_string(),
            protocol: tab.connection_config.protocol(),
            status: tab.connection_status,
            is_connected: tab.is_connected,
            message_input_mode: tab.message_input_mode.clone(),
            message_list: tab.message_list.messages.clone(),
            client_connections: tab.client_connections
                .iter()
                .map(|addr| addr.to_string())
                .collect(),
            selected_client: tab.selected_client.map(|addr| addr.to_string()),
            error_message: tab.error_message.clone(),
        }
    }
}

/// 视图模型：应用程序的UI数据
 pub struct AppViewModel {
    pub active_tab_id: Option<String>,
    pub tabs: Vec<ConnectionTabViewModel>,
    pub is_running: bool,
    pub show_new_connection_dialog: bool,
    pub new_connection_is_client: bool,
    pub new_connection_protocol: ConnectionType,
    pub host_input: Option<Entity<InputState>>,
    pub port_input: Option<Entity<InputState>>,
}

impl AppViewModel {
    pub fn new() -> Self {
        AppViewModel {
            active_tab_id: None,
            tabs: Vec::new(),
            is_running: false,
            show_new_connection_dialog: false,
            new_connection_is_client: true,
            new_connection_protocol: ConnectionType::Tcp,
            host_input: None,
            port_input: None,
        }
    }
    
    /// 从应用状态更新视图模型
    pub fn update_from_state(&mut self, app_state: &AppState) {
        // 更新标签页列表
        self.tabs = app_state.all_tabs()
            .iter()
            .map(|tab| ConnectionTabViewModel::from(*tab))
            .collect();
        
        // 更新活动标签页
        if let Some(active_tab) = app_state.active_tab() {
            self.active_tab_id = Some(active_tab.id.clone());
        } else {
            self.active_tab_id = None;
        }
        
        // 更新运行状态
        self.is_running = app_state.is_running();
    }
}

/// 视图模型管理器
pub struct ViewModelManager {
    pub app_view_model: AppViewModel,
    pub input_entities: HashMap<String, Entity<InputState>>, // 存储所有输入框实体
}

impl ViewModelManager {
    pub fn new() -> Self {
        ViewModelManager {
            app_view_model: AppViewModel::new(),
            input_entities: HashMap::new(),
        }
    }
    
    /// 创建新的输入框实体
    pub fn create_input_entity(&mut self, window: &mut Window, cx: &mut Context<NetAssistantApp>) -> Entity<InputState> {
        let entity = cx.new(|cx| InputState::new(window, cx));
        // 使用实体的原始指针作为唯一标识（简单实现）
        let id = format!("{:p}", &*entity);
        self.input_entities.insert(id, entity.clone());
        entity
    }
    
    /// 获取输入框实体
    pub fn get_input_entity(&self, id: &str) -> Option<&Entity<InputState>> {
        self.input_entities.get(id)
    }
    
    /// 删除输入框实体
    pub fn remove_input_entity(&mut self, id: &str) {
        self.input_entities.remove(id);
    }
    
    /// 转换客户端地址字符串为SocketAddr
    pub fn parse_client_addr(&self, addr_str: &str) -> Option<SocketAddr> {
        addr_str.parse().ok()
    }
    
    /// 切换消息输入模式
    pub fn toggle_input_mode(&mut self, tab_id: &str) {
        if let Some(tab) = self.app_view_model.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.message_input_mode = if tab.message_input_mode == "text" {
                "hex".to_string()
            } else {
                "text".to_string()
            };
        }
    }
    
    /// 选择客户端
    pub fn select_client(&mut self, tab_id: &str, client_addr: Option<&str>) {
        if let Some(tab) = self.app_view_model.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.selected_client = client_addr.map(|s| s.to_string());
        }
    }
    
    /// 显示新连接对话框
    pub fn show_new_connection_dialog(&mut self, is_client: bool, protocol: ConnectionType) {
        self.app_view_model.show_new_connection_dialog = true;
        self.app_view_model.new_connection_is_client = is_client;
        self.app_view_model.new_connection_protocol = protocol;
    }
    
    /// 隐藏新连接对话框
    pub fn hide_new_connection_dialog(&mut self) {
        self.app_view_model.show_new_connection_dialog = false;
    }
}