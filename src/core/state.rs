use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use log::{info, error};
use tokio::sync::mpsc;
use crate::config::connection::{ClientConfig, ServerConfig, ConnectionType};
use crate::config::storage::ConfigStorage;
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::connection::manager::{NetworkConnectionManager, DefaultNetworkFactory};
use crate::network::events::ConnectionEvent;
use crate::core::message_processor::{MessageProcessor, DefaultMessageProcessor};
use crate::core::tab_manager::{TabManager, TabState};

/// 应用状态
enum AppStateType {
    Initial,
    Running,
    Error(String),
}

/// 应用状态管理器
pub struct AppState {
    state: AppStateType,
    tab_manager: TabManager,
    network_manager: NetworkConnectionManager,
    config_storage: ConfigStorage,
    message_processor: Arc<dyn MessageProcessor>,
    event_receiver: Option<mpsc::UnboundedReceiver<ConnectionEvent>>,
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
}

impl AppState {
    pub fn new(config_storage: ConfigStorage, event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>) -> Self {
        // 如果没有提供事件发送器，创建一个默认的
        let (default_sender, event_receiver) = mpsc::unbounded_channel();
        
        AppState {
            state: AppStateType::Initial,
            tab_manager: TabManager::new(),
            network_manager: NetworkConnectionManager::new(),
            config_storage,
            message_processor: Arc::new(DefaultMessageProcessor),
            event_receiver: Some(event_receiver),
            event_sender: event_sender.or(Some(default_sender)),
        }
    }
    
    /// 初始化应用状态
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 从配置存储加载连接配置
        // 克隆所有连接配置以避免借用冲突
        let connections = self.config_storage.connections();
        let connections_cloned: Vec<_> = connections.into_iter().cloned().collect();
        
        // 为每个连接创建标签页
        for connection in connections_cloned {
            self.create_tab(connection);
        }
        
        self.state = AppStateType::Running;
        Ok(())
    }
    
    /// 创建新标签页
    pub fn create_tab(&mut self, connection_config: crate::config::connection::ConnectionConfig) -> String {
        let tab_id = self.tab_manager.create_tab(connection_config.clone());
        
        // 添加到配置存储
        if !self.config_storage.contains_connection(&connection_config) {
            self.config_storage.add_connection(connection_config);
        }
        
        tab_id
    }
    
    /// 检查标签页是否存在
    pub fn has_tab(&self, tab_id: &str) -> bool {
        self.tab_manager.get_tab(tab_id).is_some()
    }
    
    /// 建立连接
    pub async fn connect(&mut self, tab_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("[状态层] 尝试连接: tab_id={}, 标签页数量={}", tab_id, self.tab_manager.get_all_tabs().len());
        
        // 打印所有标签页的ID
        for tab in self.tab_manager.get_all_tabs() {
            info!("[状态层] 存在的标签页: id={}", tab.id);
        }
        
        let tab = self.tab_manager.get_tab(tab_id).ok_or(format!("标签页不存在: {}", tab_id))?;
        
        // 验证连接配置
        if let Err(err) = tab.connection_config.validate() {
            return Err(err.into());
        }
        
        match &tab.connection_config {
            crate::config::connection::ConnectionConfig::Client(client_config) => {
                // 建立客户端连接
                self.network_manager.create_and_connect_client(
                    client_config,
                    self.event_sender.clone()
                ).await?;
            },
            crate::config::connection::ConnectionConfig::Server(server_config) => {
                // 启动服务器
                self.network_manager.create_and_start_server(
                    server_config,
                    self.event_sender.clone()
                ).await?;
            },
        }
        
        Ok(())
    }
    
    /// 断开连接
    pub async fn disconnect(&mut self, tab_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let tab = self.tab_manager.get_tab(tab_id).ok_or("标签页不存在")?;
        
        // 根据连接类型获取连接ID
        let id = match &tab.connection_config {
            crate::config::connection::ConnectionConfig::Client(client_config) => &client_config.id,
            crate::config::connection::ConnectionConfig::Server(server_config) => &server_config.id,
        };
        
        // 根据连接类型执行不同的断开操作
        if tab.connection_config.is_client() {
            self.network_manager.disconnect_client(id).await?;
        } else {
            self.network_manager.stop_server(id).await?;
        }
        
        Ok(())
    }
    
    /// 发送消息
    pub async fn send_message(
        &mut self,
        tab_id: &str,
        content: &str,
        message_type: MessageType
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 先获取连接配置以确定连接ID
        let connection_id = {
            let tab = self.tab_manager.get_tab(tab_id).ok_or("标签页不存在")?;
            match &tab.connection_config {
                crate::config::connection::ConnectionConfig::Client(client_config) => client_config.id.clone(),
                crate::config::connection::ConnectionConfig::Server(server_config) => server_config.id.clone(),
            }
        };
        
        // 处理要发送的消息
        let raw_data = self.message_processor.process_send_message(content, message_type);
        
        // 创建发送的消息对象
        let message = Message::new(
            MessageDirection::Sent,
            raw_data.clone(),
            message_type
        );
        
        // 添加到消息列表
        self.tab_manager.get_tab_mut(tab_id).unwrap().add_message(message);
        
        // 使用之前获取的连接ID
        let id = &connection_id;
        
        // 重新获取标签页以检查连接类型
        let tab = self.tab_manager.get_tab(tab_id).ok_or("标签页不存在")?;
        
        // 根据连接类型执行不同的发送操作
        if tab.connection_config.is_client() {
            // 发送消息到服务器
            self.network_manager.send_message_to_client(id, raw_data).await?;
        } else {
            // 发送消息到客户端
            if let Some(selected_client) = tab.selected_client {
                self.network_manager.send_message_to_server_client(id, selected_client, raw_data).await?;
            } else {
                return Err("未选择客户端".into());
            }
        }
        
        Ok(())
    }
    
    /// 处理网络事件
    pub async fn process_events(&mut self) {
        if let Some(mut receiver) = self.event_receiver.take() {
            self.event_receiver = None;
            
            tokio::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    // 这里可以添加更多的事件处理逻辑
                    // 目前，事件将由TabManager处理
                }
            });
        }
    }
    
    /// 获取活动标签页
    pub fn active_tab(&self) -> Option<&TabState> {
        self.tab_manager.active_tab()
    }

    /// 检查应用是否正在运行
    pub fn is_running(&self) -> bool {
        matches!(self.state, AppStateType::Running)
    }
    
    /// 获取活动标签页的可变引用
    pub fn active_tab_mut(&mut self) -> Option<&mut TabState> {
        self.tab_manager.active_tab_mut()
    }
    
    /// 获取指定标签页
    pub fn get_tab(&self, tab_id: &str) -> Option<&TabState> {
        self.tab_manager.get_tab(tab_id)
    }
    
    /// 获取指定标签页的可变引用
    pub fn get_tab_mut(&mut self, tab_id: &str) -> Option<&mut TabState> {
        self.tab_manager.get_tab_mut(tab_id)
    }
    
    /// 获取事件发送器
    pub fn get_event_sender(&self) -> Option<mpsc::UnboundedSender<ConnectionEvent>> {
        self.event_sender.clone()
    }
    
    /// 获取事件接收器
    pub fn get_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<ConnectionEvent>> {
        self.event_receiver.take()
    }
    
    /// 设置活动标签页
    pub fn set_active_tab(&mut self, tab_id: &str) -> bool {
        self.tab_manager.set_active_tab(tab_id)
    }
    
    /// 删除标签页
    pub async fn remove_tab(&mut self, tab_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 提前获取并克隆连接配置
        let connection_config = {
            let tab = self.tab_manager.get_tab(tab_id).ok_or("标签页不存在")?;
            tab.connection_config.clone()
        };
        
        // 先断开连接
        self.disconnect(tab_id).await?;
        
        // 从标签页管理器中删除标签页
        self.tab_manager.remove_tab(tab_id);
        
        // 从配置存储中删除连接配置
        self.config_storage.retain_connections(
            |conn| match (conn, &connection_config) {
                (crate::config::connection::ConnectionConfig::Client(client), 
                 crate::config::connection::ConnectionConfig::Client(tab_client)) => {
                    client.id != tab_client.id
                },
                (crate::config::connection::ConnectionConfig::Server(server), 
                 crate::config::connection::ConnectionConfig::Server(tab_server)) => {
                    server.id != tab_server.id
                },
                _ => true,
            }
        );
        
        // 保存配置
        self.config_storage.save()?;
        
        Ok(())
    }
    
    /// 获取所有标签页
    pub fn all_tabs(&self) -> Vec<&TabState> {
        self.tab_manager.all_tabs()
    }
    
    /// 获取配置存储
    pub fn config_storage(&self) -> &ConfigStorage {
        &self.config_storage
    }
    
    /// 获取配置存储的可变引用
    pub fn config_storage_mut(&mut self) -> &mut ConfigStorage {
        &mut self.config_storage
    }
    
    /// 获取消息处理器
    pub fn message_processor(&self) -> &dyn MessageProcessor {
        self.message_processor.as_ref()
    }
    
    /// 保存配置
    pub fn save_config(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.config_storage.save()?;
        Ok(())
    }
}