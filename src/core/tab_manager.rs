use std::collections::HashMap;
use std::net::SocketAddr;
use uuid::Uuid;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType};
use crate::message::{Message, MessageListState, MessageDirection, MessageType};
use crate::network::events::ConnectionEvent;

/// 标签页状态（核心逻辑部分）
pub struct TabState {
    pub id: String,
    pub connection_config: ConnectionConfig,
    pub connection_status: ConnectionStatus,
    pub message_list: MessageListState,
    pub is_connected: bool,
    pub error_message: Option<String>,
    pub message_input_mode: String,
    pub auto_clear_input: bool,
    pub periodic_send_enabled: bool,
    pub periodic_interval: u64,
    pub client_connections: Vec<SocketAddr>,
    pub selected_client: Option<SocketAddr>,
}

impl TabState {
    pub fn new(connection_config: ConnectionConfig) -> Self {
        TabState {
            id: connection_config.id().to_string(), // 使用连接配置中的id
            connection_config,
            connection_status: ConnectionStatus::NotConnected,
            message_list: MessageListState::new(),
            is_connected: false,
            error_message: None,
            message_input_mode: "text".to_string(),
            auto_clear_input: true,
            periodic_send_enabled: false,
            periodic_interval: 1000,
            client_connections: Vec::new(),
            selected_client: None,
        }
    }

    
    /// 更新连接状态
    pub fn update_connection_status(&mut self, status: ConnectionStatus) {
        self.connection_status = status;
        self.is_connected = matches!(status, ConnectionStatus::Connected);
    }
    
    /// 添加消息到消息列表
    pub fn add_message(&mut self, message: Message) {
        self.message_list.add_message(message);
    }
    
    /// 设置错误消息
    pub fn set_error_message(&mut self, error: Option<String>) {
        self.error_message = error;
    }
    
    /// 切换消息输入模式
    pub fn toggle_input_mode(&mut self) {
        self.message_input_mode = if self.message_input_mode == "text" {
            "hex".to_string()
        } else {
            "text".to_string()
        };
    }
    
    /// 更新周期发送设置
    pub fn update_periodic_send(&mut self, enabled: bool, interval: u64) {
        self.periodic_send_enabled = enabled;
        self.periodic_interval = interval;
    }
    
    /// 添加客户端连接
    pub fn add_client_connection(&mut self, addr: SocketAddr) {
        if !self.client_connections.contains(&addr) {
            self.client_connections.push(addr);
        }
        
        // 如果没有选中的客户端，则自动选择第一个
        if self.selected_client.is_none() {
            self.selected_client = Some(addr);
        }
    }
    
    /// 移除客户端连接
    pub fn remove_client_connection(&mut self, addr: SocketAddr) {
        self.client_connections.retain(|&a| a != addr);
        
        // 如果移除的是当前选中的客户端，则重新选择一个
        if self.selected_client == Some(addr) {
            self.selected_client = self.client_connections.first().cloned();
        }
    }
    
    /// 选择客户端
    pub fn select_client(&mut self, addr: Option<SocketAddr>) {
        self.selected_client = addr;
    }
}

/// 标签页管理器
pub struct TabManager {
    tabs: HashMap<String, TabState>,
    active_tab_id: Option<String>,
}

impl TabManager {
    pub fn new() -> Self {
        TabManager {
            tabs: HashMap::new(),
            active_tab_id: None,
        }
    }
    
    /// 创建新标签页
    pub fn create_tab(&mut self, connection_config: ConnectionConfig) -> String {
        let tab = TabState::new(connection_config);
        let tab_id = tab.id.clone();
        
        self.tabs.insert(tab_id.clone(), tab);
        
        // 如果是第一个标签页，则设为活动标签页
        if self.active_tab_id.is_none() {
            self.active_tab_id = Some(tab_id.clone());
        }
        
        tab_id
    }
      
    /// 获取活动标签页
    pub fn active_tab(&self) -> Option<&TabState> {
        if let Some(active_id) = &self.active_tab_id {
            self.tabs.get(active_id)
        } else {
            None
        }
    }
    
    /// 获取活动标签页的可变引用
    pub fn active_tab_mut(&mut self) -> Option<&mut TabState> {
        if let Some(active_id) = &self.active_tab_id {
            self.tabs.get_mut(active_id)
        } else {
            None
        }
    }
    
    /// 获取指定标签页
    pub fn get_tab(&self, tab_id: &str) -> Option<&TabState> {
        self.tabs.get(tab_id)
    }
    
    /// 获取指定标签页的可变引用
    pub fn get_tab_mut(&mut self, tab_id: &str) -> Option<&mut TabState> {
        self.tabs.get_mut(tab_id)
    }
    
    /// 获取所有标签页
    pub fn get_all_tabs(&self) -> Vec<&TabState> {
        self.tabs.values().collect()
    }
    
    /// 切换活动标签页
    pub fn set_active_tab(&mut self, tab_id: &str) -> bool {
        if self.tabs.contains_key(tab_id) {
            self.active_tab_id = Some(tab_id.to_string());
            true
        } else {
            false
        }
    }
    
    /// 删除标签页
    pub fn remove_tab(&mut self, tab_id: &str) -> bool {
        if let Some(removed_tab) = self.tabs.remove(tab_id) {
            // 如果删除的是活动标签页，则切换到另一个标签页
            if self.active_tab_id == Some(tab_id.to_string()) {
                if let Some(first_tab) = self.tabs.iter().next() {
                    self.active_tab_id = Some(first_tab.0.clone());
                } else {
                    self.active_tab_id = None;
                }
            }
            
            true
        } else {
            false
        }
    }
    
    /// 获取所有标签页
    pub fn all_tabs(&self) -> Vec<&TabState> {
        self.tabs.values().collect()
    }
    
    /// 获取所有标签页的可变引用
    pub fn all_tabs_mut(&mut self) -> Vec<&mut TabState> {
        self.tabs.values_mut().collect()
    }
    
    /// 处理连接事件
    pub fn handle_connection_event(&mut self, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Connected(tab_id) => {
                if let Some(tab) = self.get_tab_mut(&tab_id) {
                    tab.update_connection_status(ConnectionStatus::Connected);
                    tab.set_error_message(None);
                }
            },
            ConnectionEvent::Disconnected(tab_id) => {
                if let Some(tab) = self.get_tab_mut(&tab_id) {
                    tab.update_connection_status(ConnectionStatus::NotConnected);
                    tab.client_connections.clear();
                    tab.selected_client = None;
                }
            },
            ConnectionEvent::Listening(tab_id) => {
                if let Some(tab) = self.get_tab_mut(&tab_id) {
                    tab.update_connection_status(ConnectionStatus::Connected);
                    tab.set_error_message(None);
                }
            },
            ConnectionEvent::Error(tab_id, error_message) => {
                if let Some(tab) = self.get_tab_mut(&tab_id) {
                    tab.set_error_message(Some(error_message));
                    tab.update_connection_status(ConnectionStatus::Error);
                }
            },
            ConnectionEvent::MessageReceived(tab_id, message) => {
                if let Some(tab) = self.get_tab_mut(&tab_id) {
                    tab.add_message(message);
                }
            },
            ConnectionEvent::ServerClientConnected(tab_id, addr, _) => {
                if let Some(tab) = self.get_tab_mut(&tab_id) {
                    tab.add_client_connection(addr);
                }
            },
            ConnectionEvent::ServerClientDisconnected(tab_id, addr) => {
                if let Some(tab) = self.get_tab_mut(&tab_id) {
                    tab.remove_client_connection(addr);
                }
            },
            ConnectionEvent::PeriodicSend(tab_id, _) | 
            ConnectionEvent::PeriodicSendBytes(tab_id, _, _) |
            ConnectionEvent::ClientWriteSenderReady(tab_id, _) => {
                // 这些事件将在更高层次处理
            },
        }
    }
    
    /// 获取标签页数量
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
    
    /// 检查标签页是否存在
    pub fn tab_exists(&self, tab_id: &str) -> bool {
        self.tabs.contains_key(tab_id)
    }
}
