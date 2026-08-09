use gpui::*;
use gpui_component::input::InputState;
use log::{debug, error, info, warn};

use crate::config;
use crate::config::app_stats::AppStats;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType, DecoderConfig};
use crate::config::storage::ConfigStorage;
use crate::export::{self, ExportFormat};
use crate::log_writer::LogWriter;
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::ConnectionEvent;
use crate::stress::engine::StressTestEngine;
use crate::stress::{StressEvent, StressStats, StressTestConfig, TabViewMode};

use crate::ui::connection_tab::ConnectionTabState;
use crate::ui::dialog::StressConfigDialogState;
use crate::ui::main_window::MainWindow;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use smol::channel::{Sender, Receiver, unbounded as smol_unbounded};

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

    // 连接编辑对话框状态（新建/编辑共用）
    // None=新建模式, Some(id)=编辑模式
    pub editing_connection_id: Option<String>,
    pub edit_message_input_mode: String,
    pub edit_decoder_config: DecoderConfig,
    pub show_connection_advanced: bool,

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

    // 连接事件通道（用于通知UI更新）- 使用smol channel与GPUI兼容
    pub connection_event_sender: Option<Sender<ConnectionEvent>>,
    pub connection_event_receiver: Option<Receiver<ConnectionEvent>>,

    // 压测事件通道（引擎→UI，同 smol channel 模式）
    pub stress_event_sender: Option<Sender<StressEvent>>,
    pub stress_event_receiver: Option<Receiver<StressEvent>>,

    // 压测配置弹窗状态(打开时创建, 关闭时置 None)
    pub stress_config_dialog: Option<StressConfigDialogState>,

    // 网络连接管理器
    pub network_manager: std::sync::Arc<tokio::sync::Mutex<crate::network::connection::manager::NetworkConnectionManager>>,
    

    // 写入发送器映射（无锁设计，每个标签页独立管理）- 使用smol channel
    pub client_write_senders: HashMap<String, Sender<Vec<u8>>>,
    pub server_clients: HashMap<String, HashMap<SocketAddr, Sender<Vec<u8>>>>,

    // 右键菜单状态
    pub show_context_menu: bool,
    pub context_menu_connection: Option<String>,
    pub context_menu_is_client: bool,
    pub context_menu_position: Option<Pixels>,
    pub context_menu_position_y: Option<Pixels>,
    
    // 添加客户端对话框状态（UDP服务端专用）
    pub show_add_client_dialog: bool,
    pub add_client_dialog_tab_id: String,
    pub add_client_dialog_input: Option<Entity<InputState>>,
    pub add_client_dialog_error: Option<String>,
    
    // 侧边栏布局状态
    pub sidebar_width: Option<Pixels>,
    pub sidebar_resizing: bool,
    pub sidebar_collapsed: bool,
    
    // 性能优化：限制UI更新频率
    pub last_update_time: Instant,
    
    // 消息容器尺寸信息（用于计算消息气泡宽度）
    pub message_container_width: Option<Pixels>,

    // 收藏功能状态
    pub show_favorite_remark: bool,
    pub favorite_remark_content: Option<String>,
    pub favorite_remark_message_type: Option<MessageType>,
    pub favorite_remark_tab_id: Option<String>,
    pub favorite_remark_input: Entity<InputState>,

    pub show_favorite_list: bool,
    pub favorite_list_tab_id: Option<String>,
    pub favorite_list_search_input: Entity<InputState>,
    pub favorite_list_position: Option<Pixels>,
    pub favorite_list_position_y: Option<Pixels>,

    // 版本更新检查状态
    pub update_available: bool,
    pub latest_version: Option<String>,
    pub show_update_tooltip: bool,

    // GitHub Star 数
    pub star_count: Option<u32>,

    // Star 引导提示
    pub show_star_prompt: bool,

    // 统计存储
    pub stats: AppStats,
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

        // 创建连接事件通道 - 使用smol channel与GPUI兼容
        let (connection_event_sender, connection_event_receiver) = smol_unbounded::<ConnectionEvent>();

        // 创建压测事件通道（引擎→UI，同 smol channel 模式）
        let (stress_event_sender, stress_event_receiver) = smol_unbounded::<StressEvent>();

        // 初始化网络连接管理器
        let network_manager = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::network::connection::manager::NetworkConnectionManager::new()
        ));

        // 初始化写入发送器映射
        let client_write_senders = HashMap::new();
        let server_clients = HashMap::new();

        // 从配置加载侧边栏宽度和折叠状态
        let sidebar_width = storage.load_sidebar_width().map(|w| gpui::px(w as f32));
        let sidebar_collapsed = storage.load_sidebar_collapsed().unwrap_or(false);

        let mut app = Self {
            storage,
            client_expanded: true,
            show_new_connection: false,
            new_connection_is_client: true,
            host_input,
            port_input,
            new_connection_protocol: String::from("TCP"),
            // 初始化连接编辑对话框状态
            editing_connection_id: None,
            edit_message_input_mode: String::from("text"),
            edit_decoder_config: DecoderConfig::default(),
            show_connection_advanced: false,
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
            stress_event_sender: Some(stress_event_sender),
            stress_event_receiver: Some(stress_event_receiver),
            stress_config_dialog: None,
            network_manager,
            client_write_senders,
            server_clients,
            show_context_menu: false,
            context_menu_connection: None,
            context_menu_is_client: false,
            context_menu_position: None,
            context_menu_position_y: None,
            // 添加客户端对话框状态（UDP服务端专用）
            show_add_client_dialog: false,
            add_client_dialog_tab_id: String::new(),
            add_client_dialog_input: None,
            add_client_dialog_error: None,
            // 初始化侧边栏布局状态
            sidebar_width,
            sidebar_resizing: false,
            sidebar_collapsed,
            // 初始化最后更新时间
            last_update_time: Instant::now(),
            // 初始化消息容器宽度
            message_container_width: None,
            // 初始化收藏功能状态
            show_favorite_remark: false,
            favorite_remark_content: None,
            favorite_remark_message_type: None,
            favorite_remark_tab_id: None,
            favorite_remark_input: cx.new(|cx| InputState::new(window, cx)),
            show_favorite_list: false,
            favorite_list_tab_id: None,
            favorite_list_search_input: cx.new(|cx| InputState::new(window, cx)),
            favorite_list_position: None,
            favorite_list_position_y: None,
            // 版本更新检查状态
            update_available: false,
            latest_version: None,
            show_update_tooltip: false,
            // GitHub Star 数
            star_count: None,
            // Star 引导提示
            show_star_prompt: false,
            // 统计存储
            stats: AppStats::default(),
        };

        // 创建专门的异步任务来处理连接事件
        let weak_app = cx.entity().clone().downgrade();
        let event_receiver = app.connection_event_receiver.take();
        
        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            let receiver = if let Some(receiver) = event_receiver {
                receiver
            } else {
                return;
            };
            
            // 异步处理连接事件
            while let Ok(event) = receiver.recv().await {
                // 尝试获取应用实例并更新状态
                if let Some(app) = weak_app.upgrade() {
                    let _ = app.update(async_app, |app, cx| {
                        app.handle_single_connection_event(event, cx);
                    });
                } else {
                }
            }
        }).detach();

        // 创建压测事件泵任务（引擎→UI，同 smol channel 模式）
        let weak_app_stress = cx.entity().clone().downgrade();
        let stress_receiver = app.stress_event_receiver.take();
        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            let receiver = if let Some(receiver) = stress_receiver {
                receiver
            } else {
                return;
            };
            while let Ok(event) = receiver.recv().await {
                if let Some(app) = weak_app_stress.upgrade() {
                    let _ = app.update(async_app, |app, cx| {
                        app.handle_stress_event(event, cx);
                    });
                }
            }
        }).detach();

        // 主题事件处理已由GPUI窗口的observe_window_appearance处理，不再需要定期检查

        // 加载统计信息，记录打开天数，判断是否显示 Star 提示
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        app.stats = AppStats::load();
        app.stats.record_open_day(&today);
        app.show_star_prompt = app.stats.should_show_star_prompt(&today);
        app.star_count = app.stats.cached_star_count;

        // 后台检查版本 + 获取 star 数（在 tokio 运行时上发起 HTTP 请求）
        let weak_app_update = cx.entity().clone().downgrade();
        let current_version = env!("APP_VERSION").to_string();
        let tokio_handle = tokio::runtime::Handle::current();

        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            // 在 tokio 运行时上并行发起两个 HTTP 请求
            let version_handle = tokio_handle.spawn(async move {
                crate::update_checker::check_latest_version().await
            });
            let star_handle = tokio_handle.spawn(async {
                crate::update_checker::fetch_star_count().await
            });

            // 等待结果（两个请求并行执行）
            let latest_version = version_handle.await.ok().flatten();
            let star_count = star_handle.await.ok().flatten();

            let mut has_update = false;

            if let Some(app) = weak_app_update.upgrade() {
                let _ = app.update(async_app, |app, cx| {
                    // 更新 star 数并写入缓存
                    if let Some(stars) = star_count {
                        app.star_count = Some(stars);
                        app.stats.cached_star_count = Some(stars);
                        app.stats.save();
                    }

                    // 版本检查（开发版日期号也检查，直接认为需要更新）
                    if let Some(latest) = &latest_version {
                        if crate::update_checker::should_show_update(&current_version, latest) {
                            app.update_available = true;
                            app.latest_version = Some(latest.clone());
                            app.show_update_tooltip = true;
                            has_update = true;
                        }
                    }

                    cx.notify();
                });
            }

            // 10 秒后自动隐藏 tooltip
            if has_update {
                smol::Timer::after(std::time::Duration::from_secs(10)).await;
                if let Some(app) = weak_app_update.upgrade() {
                    let _ = app.update(async_app, |app, cx| {
                        app.show_update_tooltip = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();

        app
    }

    pub fn toggle_connection(&mut self, tab_id: String, cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            if tab_state.is_connected {
                // 断开连接
                if tab_state.connection_config.is_client() {
                    self.disconnect_client(tab_id, cx);
                } else {
                    self.disconnect_server(tab_id, cx);
                }
            } else {
                    // 建立连接
                    if tab_state.connection_config.is_client() {
                        self.connect_to_server(tab_id);
                    } else {
                        self.start_server(tab_id, cx);
                    }
                }
        }
        cx.notify();
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
                        debug!("[周期发送] 已停止旧的周期发送任务");
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
                        let _ = sender.try_send(ConnectionEvent::PeriodicSend(
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
                                let _ = sender.try_send(ConnectionEvent::PeriodicSendBytes(
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
                    .folding(false)
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

    /// 打开「新建连接」对话框（重置为新建模式）
    pub fn open_new_connection(&mut self, is_client: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_connection_id = None;
        self.new_connection_is_client = is_client;
        self.new_connection_protocol = String::from("TCP");
        self.show_connection_advanced = false;
        self.edit_message_input_mode = String::from("text");
        self.edit_decoder_config = DecoderConfig::default();

        let default_host = if is_client { "127.0.0.1" } else { "0.0.0.0" };
        self.host_input.update(cx, |i, cx| i.set_value(default_host.to_string(), window, cx));
        self.port_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));

        self.show_new_connection = true;
        cx.notify();
    }

    /// 打开「编辑连接」对话框（从现有配置回填）
    pub fn open_edit_connection(&mut self, connection_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let config = self
            .storage
            .client_connections()
            .iter()
            .chain(self.storage.server_connections().iter())
            .find(|c| c.id() == connection_id)
            .map(|c| (*c).clone());

        let Some(config) = config else { return };

        self.editing_connection_id = Some(connection_id);
        self.new_connection_is_client = config.is_client();
        self.new_connection_protocol = match config.protocol() {
            ConnectionType::Tcp => "TCP".to_string(),
            ConnectionType::Udp => "UDP".to_string(),
        };

        let (address, port, message_input_mode, decoder) = match &config {
            ConnectionConfig::Client(c) => (
                c.server_address.clone(),
                c.server_port,
                c.message_input_mode.clone(),
                c.decoder_config.clone(),
            ),
            ConnectionConfig::Server(c) => (
                c.listen_address.clone(),
                c.listen_port,
                c.message_input_mode.clone(),
                c.decoder_config.clone(),
            ),
        };

        self.host_input.update(cx, |i, cx| i.set_value(address, window, cx));
        self.port_input.update(cx, |i, cx| i.set_value(port.to_string(), window, cx));
        self.edit_message_input_mode = message_input_mode;
        self.edit_decoder_config = decoder;
        self.show_connection_advanced = true;

        self.show_new_connection = true;
        cx.notify();
    }

    /// 确认连接表单（新建或编辑）
    pub fn confirm_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = self.host_input.read(cx).value().to_string();
        let port_str = self.port_input.read(cx).value().to_string();
        if host.is_empty() || port_str.is_empty() {
            return;
        }
        let Ok(port) = port_str.parse::<u16>() else { return };

        let message_input_mode = self.edit_message_input_mode.clone();
        let decoder_config = self.edit_decoder_config.clone();
        let connection_type = if self.new_connection_protocol == "TCP" {
            ConnectionType::Tcp
        } else {
            ConnectionType::Udp
        };
        let is_client = self.new_connection_is_client;

        if let Some(edit_id) = self.editing_connection_id.take() {
            // 编辑模式：保留 id / 类型 / 协议，更新其余字段
            let existing = self
                .storage
                .client_connections()
                .iter()
                .chain(self.storage.server_connections().iter())
                .find(|c| c.id() == edit_id)
                .map(|c| (*c).clone());

            if let Some(mut existing) = existing {
                match &mut existing {
                    ConnectionConfig::Client(c) => {
                        c.server_address = host;
                        c.server_port = port;
                        c.message_input_mode = message_input_mode;
                        c.decoder_config = decoder_config;
                    }
                    ConnectionConfig::Server(c) => {
                        c.listen_address = host;
                        c.listen_port = port;
                        c.message_input_mode = message_input_mode;
                        c.decoder_config = decoder_config;
                    }
                }
                let updated_config = existing.clone();
                self.storage.update_connection(updated_config.clone());
                // 同步已打开的标签页
                if let Some(tab_state) = self.connection_tabs.get_mut(&edit_id) {
                    tab_state.connection_config = updated_config.clone();
                    tab_state.message_input_mode = updated_config.message_input_mode().to_string();
                }
            }
        } else {
            // 新建模式
            let mut config = if is_client {
                ConnectionConfig::new_client(host, port, connection_type)
            } else {
                ConnectionConfig::new_server(host, port, connection_type)
            };
            match &mut config {
                ConnectionConfig::Client(c) => {
                    c.message_input_mode = message_input_mode;
                    c.decoder_config = decoder_config;
                }
                ConnectionConfig::Server(c) => {
                    c.message_input_mode = message_input_mode;
                    c.decoder_config = decoder_config;
                }
            }
            self.storage.add_connection(config.clone());

            let new_tab_id = config.id().to_string();
            self.ensure_tab_exists(new_tab_id.clone(), config, window, cx);
            self.active_tab = new_tab_id;
        }

        // 重置协议为默认
        self.new_connection_protocol = String::from("TCP");
        // 关闭对话框
        self.show_new_connection = false;
        cx.notify();
    }

    pub fn close_tab(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        debug!("[关闭标签页] 开始关闭标签页: {}", tab_id);

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }

        if self.connection_tabs.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除标签页状态: {}", tab_id);
        }

        if self.auto_reply_inputs.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除自动回复输入框: {}", tab_id);
        }

        // 清理客户端连接发送器
        if self.client_write_senders.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除客户端连接发送器: {}", tab_id);
        }

        // 清理服务端客户端连接
        if self.server_clients.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除服务端客户端连接: {}", tab_id);
        }

        debug!("[关闭标签页] 标签页 {} 已关闭", tab_id);
    }


    // ============== 压测相关方法 ==============

    /// 处理压测事件(由压测事件泵调用)
    pub fn handle_stress_event(&mut self, event: StressEvent, cx: &mut Context<Self>) {
        match event {
            StressEvent::StatsSnapshot(stats) => {
                if let Some(tab) = self.connection_tabs.get_mut(&self.active_tab) {
                    tab.stress_stats = stats;
                    cx.notify();
                }
            }
            StressEvent::Finished(report) => {
                if let Some(tab) = self.connection_tabs.get_mut(&self.active_tab) {
                    tab.stress_report = Some(report);
                    tab.stress_engine = None;
                    info!("[压测] 已完成，报告已存档");
                    cx.notify();
                }
            }
            StressEvent::Error(msg) => {
                if let Some(tab) = self.connection_tabs.get_mut(&self.active_tab) {
                    tab.error_message = Some(msg);
                    cx.notify();
                }
            }
        }
    }

    /// 从连接配置回填压测目标(地址/端口/协议)，优先用已保存的压测配置
    pub fn build_stress_config_for_tab(&self, tab_id: &str) -> Option<StressTestConfig> {
        let tab = self.connection_tabs.get(tab_id)?;
        let (address, port, protocol) = match &tab.connection_config {
            ConnectionConfig::Client(c) => (c.server_address.clone(), c.server_port, c.protocol),
            ConnectionConfig::Server(c) => {
                // 服务端监听 0.0.0.0 时回填 127.0.0.1 作为压测目标
                let addr = if c.listen_address == "0.0.0.0" || c.listen_address.is_empty() {
                    "127.0.0.1".to_string()
                } else {
                    c.listen_address.clone()
                };
                (addr, c.listen_port, c.protocol)
            }
        };
        let mut config = self
            .storage
            .get_stress_profile(tab.connection_config.id())
            .cloned()
            .unwrap_or_else(|| StressTestConfig::for_target(address.clone(), port, protocol));
        // 确保目标地址/端口/协议与当前连接一致
        config.target_address = address;
        config.target_port = port;
        config.protocol = protocol;
        Some(config)
    }

    /// 打开压测配置弹窗(回填目标 + 已保存配置)
    pub fn open_stress_config(&mut self, tab_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let config = match self.build_stress_config_for_tab(&tab_id) {
            Some(c) => c,
            None => {
                error!("[压测] 未找到标签页: {}", tab_id);
                return;
            }
        };
        self.stress_config_dialog = Some(StressConfigDialogState::new(tab_id, config, window, cx));
        cx.notify();
    }

    /// 启动压测
    pub fn start_stress(&mut self, tab_id: String, config: StressTestConfig, cx: &mut Context<Self>) {
        let sender = match &self.stress_event_sender {
            Some(s) => s.clone(),
            None => {
                error!("[压测] 事件通道未初始化");
                return;
            }
        };

        // 持久化压测配置(按 connection_id)
        let connection_id = self
            .connection_tabs
            .get(&tab_id)
            .map(|t| t.connection_config.id().to_string())
            .unwrap_or_default();
        if !connection_id.is_empty() {
            self.storage.save_stress_profile(&connection_id, config.clone());
        }

        if let Some(tab) = self.connection_tabs.get_mut(&tab_id) {
            // 若已有引擎在运行，先停止
            if let Some(engine_arc) = &tab.stress_engine {
                if let Ok(mut guard) = engine_arc.lock() {
                    if let Some(mut engine) = guard.take() {
                        engine.stop();
                    }
                }
            }
            // 重置状态
            tab.stress_stats = StressStats::default();
            tab.stress_report = None;
            tab.error_message = None;
            tab.stress_config_snapshot = Some(config.clone());
            tab.view_mode = TabViewMode::Stress;

            // 启动引擎
            let engine = StressTestEngine::start(config, sender);
            tab.stress_engine = Some(Arc::new(Mutex::new(Some(engine))));
            info!("[压测] 引擎已启动 (tab={})", tab_id);
        }

        // 渲染节拍: smol::Timer 定期唤醒 GPUI 事件循环
        // 引擎的 tokio 任务通过 smol channel 发送事件，但跨 runtime 时 GPUI 事件循环可能不被唤醒。
        // 此定时器运行在 GPUI runtime 上，每 250ms 触发 cx.notify() 确保 UI 刷新。
        let weak_app_render = cx.entity().downgrade();
        let render_tab_id = tab_id.clone();
        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(250)).await;
                let keep_going = if let Some(app) = weak_app_render.upgrade() {
                    app.update(async_app, |app, cx| {
                        if let Some(tab) = app.connection_tabs.get(&render_tab_id) {
                            if tab.stress_engine.is_some() && tab.stress_report.is_none() {
                                cx.notify();
                                return true;
                            }
                        }
                        false
                    })
                } else {
                    false
                };
                if !keep_going {
                    break;
                }
            }
        }).detach();

        cx.notify();
    }

    /// 停止压测
    pub fn stop_stress(&mut self, tab_id: String, cx: &mut Context<Self>) {
        if let Some(tab) = self.connection_tabs.get_mut(&tab_id) {
            if let Some(engine_arc) = &tab.stress_engine {
                if let Ok(mut guard) = engine_arc.lock() {
                    if let Some(mut engine) = guard.take() {
                        engine.stop();
                        info!("[压测] 引擎已停止 (tab={})", tab_id);
                    }
                }
            }
            // 清除外层 Option，让 UI 正确反映已停止状态
            tab.stress_engine = None;
        }
        cx.notify();
    }

    /// 导出压测 CSV 报告
    pub fn export_stress_report(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        let report = match self
            .connection_tabs
            .get(&tab_id)
            .and_then(|t| t.stress_report.clone())
        {
            Some(r) => r,
            None => {
                debug!("[压测导出] 无可导出的报告");
                return;
            }
        };

        let address_label = self
            .connection_tabs
            .get(&tab_id)
            .map(|t| t.connection_config.address_label())
            .unwrap_or_else(|| "stress".to_string());
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let default_filename = format!("{}_stress_{}.csv", address_label, timestamp);

        tokio::spawn(async move {
            let file_path = rfd::AsyncFileDialog::new()
                .set_file_name(&default_filename)
                .add_filter("CSV 文件", &["csv"])
                .save_file()
                .await;

            if let Some(file_path) = file_path {
                let path = file_path.path();
                let content = crate::stress::report::format_stress_csv(&report);
                match std::fs::write(path, content) {
                    Ok(_) => debug!("[压测导出] 报告已导出到: {:?}", path),
                    Err(e) => error!("[压测导出] 写入文件失败: {:?}", e),
                }
            }
        });
    }

    /// 切换 Tab 视图模式(调试/压测)
    pub fn switch_tab_view_mode(&mut self, tab_id: String, view_mode: TabViewMode, cx: &mut Context<Self>) {
        if let Some(tab) = self.connection_tabs.get_mut(&tab_id) {
            tab.view_mode = view_mode;
            cx.notify();
        }
    }

    pub fn disconnect_client(&mut self, tab_id: String, cx: &mut Context<Self>) {
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let network_manager_arc = self.network_manager.clone();

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }

        cx.notify();
        tokio::spawn(async move {
                            // 断开网络连接
                            let mut network_manager = network_manager_arc.lock().await;
                            if let Err(e) = network_manager.disconnect_client(&tab_id_clone).await {
                error!("断开客户端连接失败: {:?}", e);
            }
            
            // 发送断开连接事件
            if let Some(sender) = sender {
                let _ = sender.try_send(ConnectionEvent::Disconnected(tab_id_clone));
            }
        });
    }

    /// 服务端断开连接
    pub fn disconnect_server(&mut self, tab_id: String, cx: &mut Context<Self>) {
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let network_manager_arc = self.network_manager.clone();

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }
        
        self.server_clients.remove(&tab_id);

        cx.notify();
        tokio::spawn(async move {
            let mut network_manager = network_manager_arc.lock().await;
            if let Err(e) = network_manager.stop_server(&tab_id_clone).await {
                error!("停止服务器失败: {:?}", e);
            }
            
            if let Some(sender) = sender {
                let _ = sender.try_send(ConnectionEvent::Disconnected(tab_id_clone));
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
    pub fn start_server(&mut self, tab_id: String, _cx: &mut Context<Self>) {
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
        debug!(
            "[send_message] 开始，tab_id: {}, content: '{}'",
            tab_id, content
        );
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        let content_clone = content.clone();
        
        // 保存message_type用于后续事件发送
        let tab_info = self.connection_tabs.get(&tab_id)
            .map(|tab_state| {
                let message_type = if tab_state.message_input_mode == "text" {
                    MessageType::Text
                } else {
                    MessageType::Hex
                };
                let is_client = tab_state.connection_config.is_client();
                let selected_client = tab_state.selected_client;
                (message_type, is_client, selected_client)
            });
        
        if tab_info.is_none() {
            error!("[send_message] 未找到标签页: {}", tab_id);
            return;
        }
        
        let (message_type, is_client, selected_client) = tab_info.unwrap();
        
        // 在闭包外部获取必要的信息
        let is_connected_result = self.connection_tabs.get(&tab_id).map(|tab| tab.is_connected);
        
        if is_connected_result.is_none() {
            error!("[send_message] 未找到标签页: {}", tab_id);
            return;
        }
        
        let is_connected = is_connected_result.unwrap();
        
        if !is_connected {
            if let Some(sender) = sender {
                let _ = sender.try_send(ConnectionEvent::Error(
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
                if write_sender.try_send(bytes.clone()).is_err() {
                    error!("[send_message] 无法发送消息到服务器");
                    if let Some(sender) = sender {
                        let _ = sender.try_send(ConnectionEvent::Error(
                            tab_id_clone,
                            "发送消息失败".to_string(),
                        ));
                    }
                } else {
                    debug!("[send_message] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message] 客户端写入发送器不可用");
                if let Some(sender) = sender {
                    let _ = sender.try_send(ConnectionEvent::Error(
                        tab_id_clone,
                        "客户端写入发送器不可用".to_string(),
                    ));
                }
            }
        } else {
            // 服务器模式：根据selected_client决定定向发送还是广播
            if let Some(clients) = self.server_clients.get(&tab_id) {
                if clients.is_empty() {
                    // 服务端没有客户端连接，不应标记整个服务端为错误
                    warn!("[send_message] 没有可用的客户端连接");
                } else if let Some(target_addr) = selected_client {
                    // 定向发送给选中的客户端
                    debug!("[send_message] 服务端模式，定向发送给: {}", target_addr);
                    if let Some(write_sender) = clients.get(&target_addr) {
                        if write_sender.try_send(bytes.clone()).is_err() {
                            // 单个客户端发送失败不应影响整个服务端，仅记录日志
                            // TCP/UDP 层会通过 ServerClientDisconnected 事件清理该客户端
                            warn!("[send_message] 发送给客户端 {} 失败（客户端可能已断开）", target_addr);
                        } else {
                            debug!("[send_message] 定向发送成功");
                            if let Some(sender) = sender {
                                let message = Message::new(MessageDirection::Sent, bytes, message_type)
                                    .with_source(target_addr.to_string());
                                let _ = sender.try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                            }
                        }
                    } else {
                        warn!("[send_message] 客户端 {} 不存在或已断开", target_addr);
                    }
                } else {
                    // 广播给所有客户端（并行发送）
                    debug!("[send_message] 服务端模式，广播给所有客户端，共 {} 个", clients.len());
                    let bytes_arc = std::sync::Arc::new(bytes.clone());
                    
                    for (addr, write_sender) in clients.iter() {
                        let sender_clone = write_sender.clone();
                        let bytes_clone = bytes_arc.clone();
                        let addr_str = addr.to_string();
                        tokio::spawn(async move {
                            if sender_clone.send((*bytes_clone).clone()).await.is_err() {
                                error!("[send_message] 广播发送给客户端 {} 失败", addr_str);
                            }
                        });
                    }
                    
                    debug!("[send_message] 广播发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                warn!("[send_message] 服务器客户端映射不可用");
            }
        }
    }

    pub fn send_message_bytes(&mut self, tab_id: String, bytes: Vec<u8>, hex_input: String) {
        debug!(
            "[send_message_bytes] 开始，tab_id: {}, bytes: {:?}, hex_input: '{}'",
            tab_id, bytes, hex_input
        );
        let sender = self.connection_event_sender.clone();
        let tab_id_clone = tab_id.clone();
        
        // 保存message_type和selected_client用于后续事件发送
        let tab_info = self.connection_tabs.get(&tab_id)
            .map(|tab_state| {
                let message_type = if tab_state.message_input_mode == "text" {
                    MessageType::Text
                } else {
                    MessageType::Hex
                };
                let is_client = tab_state.connection_config.is_client();
                let selected_client = tab_state.selected_client;
                (message_type, is_client, selected_client)
            });
        
        if tab_info.is_none() {
            error!("[send_message_bytes] 未找到标签页: {}", tab_id);
            return;
        }
        
        let (message_type, is_client, selected_client) = tab_info.unwrap();
        
        // 在闭包外部获取必要的信息
        let is_connected_result = self.connection_tabs.get(&tab_id).map(|tab| tab.is_connected);
        
        if is_connected_result.is_none() {
            error!("[send_message_bytes] 未找到标签页: {}", tab_id);
            return;
        }
        
        let is_connected = is_connected_result.unwrap();
        
        if !is_connected {
            if let Some(sender) = sender {
                let _ = sender.try_send(ConnectionEvent::Error(
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
                if write_sender.try_send(bytes.clone()).is_err() {
                    error!("[send_message_bytes] 无法发送消息到服务器");
                    if let Some(sender) = sender {
                        let _ = sender.try_send(ConnectionEvent::Error(
                            tab_id_clone,
                            "发送消息失败".to_string(),
                        ));
                    }
                } else {
                    debug!("[send_message_bytes] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message_bytes] 客户端写入发送器不可用");
                if let Some(sender) = sender {
                    let _ = sender.try_send(ConnectionEvent::Error(
                        tab_id_clone,
                        "客户端写入发送器不可用".to_string(),
                    ));
                }
            }
        } else {
            // 服务器模式：根据selected_client决定定向发送还是广播
            if let Some(clients) = self.server_clients.get(&tab_id) {
                if clients.is_empty() {
                    warn!("[send_message_bytes] 没有可用的客户端连接");
                } else if let Some(target_addr) = selected_client {
                    // 定向发送给选中的客户端
                    debug!("[send_message_bytes] 服务端模式，定向发送给: {}", target_addr);
                    if let Some(write_sender) = clients.get(&target_addr) {
                        if write_sender.try_send(bytes.clone()).is_err() {
                            // 单个客户端发送失败不应影响整个服务端，仅记录日志
                            warn!("[send_message_bytes] 发送给客户端 {} 失败（客户端可能已断开）", target_addr);
                        } else {
                            debug!("[send_message_bytes] 定向发送成功");
                            if let Some(sender) = sender {
                                let message = Message::new(MessageDirection::Sent, bytes, message_type)
                                    .with_source(target_addr.to_string());
                                let _ = sender.try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                            }
                        }
                    } else {
                        warn!("[send_message_bytes] 客户端 {} 不存在或已断开", target_addr);
                    }
                } else {
                    // 广播给所有客户端（并行发送）
                    debug!("[send_message_bytes] 服务端模式，广播给所有客户端，共 {} 个", clients.len());
                    let bytes_arc = std::sync::Arc::new(bytes.clone());
                    
                    for (addr, write_sender) in clients.iter() {
                        let sender_clone = write_sender.clone();
                        let bytes_clone = bytes_arc.clone();
                        let addr_str = addr.to_string();
                        tokio::spawn(async move {
                            if sender_clone.send((*bytes_clone).clone()).await.is_err() {
                                error!("[send_message_bytes] 广播发送给客户端 {} 失败", addr_str);
                            }
                        });
                    }
                    
                    debug!("[send_message_bytes] 广播发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender.try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                warn!("[send_message_bytes] 服务器客户端映射不可用");
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
        debug!(
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
                let _ = sender.try_send(ConnectionEvent::Error(
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
                    debug!("[send_message_to_client] 发送给指定客户端: {}", addr);
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
                            if write_sender.try_send(bytes.clone()).is_err() {
                                // 单个客户端发送失败不应影响整个服务端，仅记录日志
                                warn!("[send_message_to_client] 发送给客户端 {} 失败（客户端可能已断开）", addr);
                            } else {
                                debug!("[send_message_to_client] 发送成功");
                                if let Some(sender) = sender {
                                    let message = Message::new(
                                        MessageDirection::Sent,
                                        bytes,
                                        message_type,
                                    )
                                    .with_source(source_str);
                                    let _ = sender.try_send(ConnectionEvent::MessageReceived(
                                        tab_id_clone,
                                        message,
                                    ));
                                }
                            }
                        } else {
                            warn!("[send_message_to_client] 客户端 {} 不存在", addr);
                        }
                    } else {
                        warn!("[send_message_to_client] 服务器客户端映射不可用");
                    }
                },
                Err(_) => {
                    error!("[send_message_to_client] 无效的客户端地址: {}", source_str);
                },
            }
        } else {
            warn!("[send_message_to_client] 没有指定客户端，无法发送自动回复");
        }
    }

    /// 向UDP服务端手动添加客户端地址
    pub fn add_client_to_server(&mut self, tab_id: String, addr_str: String, cx: &mut Context<Self>) {
        let addr: std::net::SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(_) => {
                error!("[add_client_to_server] 无效的地址格式: {}", addr_str);
                if let Some(sender) = &self.connection_event_sender {
                    let _ = sender.try_send(ConnectionEvent::Error(
                        tab_id,
                        format!("无效的地址格式: {}", addr_str),
                    ));
                }
                return;
            }
        };

        let manager = self.network_manager.clone();
        let event_sender = self.connection_event_sender.clone();

        tokio::spawn(async move {
            let mgr = manager.lock().await;
            match mgr.add_udp_client(&tab_id, addr).await {
                Ok(_) => {
                    info!("[add_client_to_server] 成功添加客户端: {}", addr);
                }
                Err(e) => {
                    error!("[add_client_to_server] 添加客户端失败: {}", e);
                    if let Some(sender) = &event_sender {
                        let _ = sender.try_send(ConnectionEvent::Error(
                            tab_id,
                            format!("添加客户端失败: {}", e),
                        ));
                    }
                }
            }
        });

        cx.notify();
    }


    /// 导出指定标签页的通信记录
    pub fn export_messages(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        // 获取消息列表的克隆，避免长期借用
        let messages = match self.connection_tabs.get(&tab_id) {
            Some(tab_state) => tab_state.message_list.messages.clone(),
            None => {
                error!("[导出] 未找到标签页: {}", tab_id);
                return;
            }
        };

        if messages.is_empty() {
            debug!("[导出] 没有可导出的消息记录");
            return;
        }

        // 获取连接地址标识用于默认文件名（如 TCP_127.0.0.1_8080）
        let address_label = self.connection_tabs.get(&tab_id)
            .map(|t| t.connection_config.address_label())
            .unwrap_or_else(|| "export".to_string());

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let default_filename = format!("{}_{}.txt", address_label, timestamp);

        // 在异步任务中弹出文件对话框并保存
        tokio::spawn(async move {
            let file_path = rfd::AsyncFileDialog::new()
                .set_file_name(&default_filename)
                .add_filter("纯文本文件", &["txt"])
                .add_filter("JSON 文件", &["json"])
                .add_filter("CSV 文件", &["csv"])
                .save_file()
                .await;

            if let Some(file_path) = file_path {
                let path = file_path.path();

                // 根据扩展名确定格式，默认为 txt
                let format = ExportFormat::from_extension(path)
                    .unwrap_or(ExportFormat::Txt);

                match export::format_messages(&messages, format) {
                    Ok(content) => {
                        match std::fs::write(path, content) {
                            Ok(_) => {
                                debug!("[导出] 消息记录已导出到: {:?}", path);
                            }
                            Err(e) => {
                                error!("[导出] 写入文件失败: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("[导出] 格式化消息失败: {}", e);
                    }
                }
            }
        });
    }

    /// 切换消息显示模式（原始/美化/压缩），并重算所有消息的 cached_content
    pub fn toggle_message_display_mode(&mut self, tab_id: String, cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            let new_mode = tab_state.message_display_mode.next();
            tab_state.message_display_mode = new_mode;
            // 遍历所有消息，从 raw_data 重新计算 cached_content
            for message in &mut tab_state.message_list.messages {
                message.recompute_content_for_display(new_mode);
            }
            debug!("[消息显示模式] 标签页 {} 切换为: {:?}", tab_id, new_mode);
            cx.notify();
        }
    }

    /// 切换日志记录开关
    pub fn toggle_log(&mut self, tab_id: String, cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            if tab_state.log_enabled {
                // 关闭日志记录
                if let Some(log_writer) = tab_state.log_writer.take() {
                    tokio::spawn(async move {
                        let mut writer = log_writer.lock().await;
                        writer.close().await;
                    });
                }
                tab_state.log_enabled = false;
                tab_state.log_file_path = None;
                debug!("[日志记录] 已关闭: {}", tab_id);
            } else {
                // 开启日志记录：优先使用自定义路径
                let log_path = tab_state.custom_log_path.as_ref()
                    .map(|p| std::path::PathBuf::from(p))
                    .unwrap_or_else(|| LogWriter::default_log_path(&tab_state.connection_config.address_label()));

                // 确保日志目录存在（同步创建，很快）
                if let Some(parent) = log_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                let log_path_display = log_path.display().to_string();
                let log_path_for_writer = log_path.clone();

                // 使用 cx.spawn 异步打开文件并更新状态
                let tab_id_clone = tab_id.clone();
                cx.spawn(async move |this, cx| {
                    match LogWriter::open(log_path_for_writer).await {
                        Ok(log_writer) => {
                            let writer_arc = std::sync::Arc::new(tokio::sync::Mutex::new(log_writer));
                            let _ = this.update(cx, |app, cx| {
                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_clone) {
                                    tab_state.log_writer = Some(writer_arc);
                                    tab_state.log_enabled = true;
                                    cx.notify();
                                }
                            });
                            debug!("[日志记录] 已开启: {:?}", log_path);
                        }
                        Err(e) => {
                            error!("[日志记录] 打开日志文件失败: {:?}", e);
                        }
                    }
                }).detach();

                // 先设置路径显示（文件在后台异步打开）
                tab_state.log_file_path = Some(log_path_display);
                debug!("[日志记录] 正在开启: {}", tab_id);
            }
            cx.notify();
        }
    }

    /// 打开日志文件所在目录
    pub fn open_log_directory(&self, tab_id: String) {
        if let Some(tab_state) = self.connection_tabs.get(&tab_id) {
            if let Some(path) = &tab_state.log_file_path {
                let path = std::path::Path::new(path);
                let dir = if path.is_file() || !path.exists() {
                    path.parent().unwrap_or(path)
                } else {
                    path
                };

                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("explorer")
                        .arg(dir)
                        .spawn();
                }
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg(dir)
                        .spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(dir)
                        .spawn();
                }
            }
        }
    }

    /// 修改日志保存路径
    pub fn change_log_path(&mut self, tab_id: String, cx: &mut Context<Self>) {
        // 先关闭当前日志
        let was_enabled = self.connection_tabs.get(&tab_id)
            .map(|t| t.log_enabled)
            .unwrap_or(false);

        if was_enabled {
            self.toggle_log(tab_id.clone(), cx);
        }

        let address_label = self.connection_tabs.get(&tab_id)
            .map(|t| t.connection_config.address_label())
            .unwrap_or_else(|| "log".to_string());

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let default_filename = format!("{}_{}.log", address_label, timestamp);

        let tab_id_clone = tab_id.clone();
        cx.spawn(async move |this, cx| {
            let file_path = rfd::AsyncFileDialog::new()
                .set_file_name(&default_filename)
                .add_filter("日志文件", &["log"])
                .add_filter("所有文件", &["*"])
                .save_file()
                .await;

            if let Some(file_path) = file_path {
                let path = file_path.path().display().to_string();
                let _ = this.update(cx, |app, cx| {
                    if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_clone) {
                        tab_state.custom_log_path = Some(path);
                        // 自动以新路径开启日志记录
                        app.toggle_log(tab_id_clone.clone(), cx);
                    }
                });
            }
        }).detach();
    }

    // 侧边栏调整大小相关方法
    pub fn start_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        self.sidebar_resizing = true;
        // 如果侧边栏已折叠，则先展开它
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
            // 设置一个默认宽度作为展开后的初始宽度
            if self.sidebar_width.is_none() {
                self.sidebar_width = Some(px(200.0));
            }
        }
        cx.notify();
    }
    
    pub fn resize_sidebar(&mut self, new_width: Pixels, cx: &mut Context<Self>) {
        // 只有在调整大小状态下才允许改变宽度
        if self.sidebar_resizing {
            // 检查是否需要更新（限制更新频率约60fps）
            let now = Instant::now();
            if now.duration_since(self.last_update_time) < Duration::from_millis(16) {
                return; // 跳过此次更新
            }
            
            // 设置侧边栏宽度的最小和最大值限制
            let min_width = px(150.0);
            let max_width = px(300.0);
            let collapse_threshold = px(150.0);
            
            // 如果新宽度小于折叠阈值，自动折叠侧边栏
            if new_width < collapse_threshold {
                self.sidebar_collapsed = true;
            } else {
                // 限制新宽度在合理范围内
                let clamped_width = new_width.max(min_width).min(max_width);
                self.sidebar_width = Some(clamped_width);
                self.sidebar_collapsed = false;
            }
            
            // 更新最后更新时间
            self.last_update_time = now;
            cx.notify();
        }
    }
    
    pub fn end_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        self.sidebar_resizing = false;
        // 保存当前侧边栏宽度和折叠状态到配置
        if let Some(width) = self.sidebar_width {
            let width_f32 = width / gpui::px(1.0);
            self.storage.save_sidebar_width(width_f32 as f64);
        }
        self.storage.save_sidebar_collapsed(self.sidebar_collapsed);
        cx.notify();
    }
    
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        // 保存折叠状态到配置
        self.storage.save_sidebar_collapsed(self.sidebar_collapsed);
        cx.notify();
    }

    /// 用户点击「给个 Star」按钮：跳转 repo 页面 + 关闭提示（不触发 snooze）
    pub fn accept_star_prompt(&mut self, cx: &mut Context<Self>) {
        cx.open_url("https://github.com/SunJary/NetAssistant");
        self.show_star_prompt = false;
        cx.notify();
    }

    /// 用户点击「近期不再提示」：触发 snooze（递增关闭次数 + 记录日期）
    pub fn dismiss_star_prompt(&mut self, cx: &mut Context<Self>) {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.stats.dismiss_star_prompt(&today);
        self.show_star_prompt = false;
        cx.notify();
    }

    pub fn handle_single_connection_event(&mut self, event: ConnectionEvent, cx: &mut Context<Self>) {
        match event {
            ConnectionEvent::Connected(tab_id) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    tab_state.is_connected = true;
                    tab_state.connection_status = ConnectionStatus::Connected;
                    tab_state.error_message = None;
                    cx.notify();
                }
            }
            ConnectionEvent::Disconnected(tab_id) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    tab_state.is_connected = false;
                    tab_state.connection_status = ConnectionStatus::Disconnected;
                    cx.notify();
                }
                self.client_write_senders.remove(&tab_id);
                self.server_clients.remove(&tab_id);
            }
            ConnectionEvent::Listening(tab_id) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    tab_state.is_connected = true;
                    tab_state.connection_status = ConnectionStatus::Listening;
                    tab_state.error_message = None;
                    cx.notify();
                }
            }
            ConnectionEvent::Error(tab_id, error) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    tab_state.is_connected = false;
                    tab_state.connection_status = ConnectionStatus::Error;
                    tab_state.error_message = Some(error);
                    cx.notify();
                }
                // 清理连接信息，确保下次发送时直接失败
                self.client_write_senders.remove(&tab_id);
                self.server_clients.remove(&tab_id);
            }
            ConnectionEvent::ClientWriteSenderReady(tab_id, write_sender) => {
                debug!(
                    "[handle_connection_events] 客户端写入发送器就绪: {}",
                    tab_id
                );
                self.client_write_senders.insert(tab_id, write_sender);
            }
            ConnectionEvent::ServerClientConnected(tab_id, addr, write_sender) => {
                debug!(
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
                        let old_len = tab_state.client_connections.len();
                        tab_state.client_connections.push(addr);
                        tab_state.client_list_state.splice(old_len..old_len, 1);
                        cx.notify();
                    }
                }
            }
            ConnectionEvent::ServerClientDisconnected(tab_id, addr) => {
                debug!(
                    "[handle_connection_events] 服务端客户端断开: tab_id={}, addr={}",
                    tab_id, addr
                );
                if let Some(clients) = self.server_clients.get_mut(&tab_id) {
                    clients.remove(&addr);
                }
                // 更新 ConnectionTabState 中的客户端连接列表
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    if let Some(idx) =
                        tab_state.client_connections.iter().position(|&c| c == addr)
                    {
                        tab_state.client_connections.remove(idx);
                        tab_state.client_list_state.splice(idx..idx + 1, 0);
                    }
                    if tab_state.selected_client.as_ref() == Some(&addr) {
                        tab_state.selected_client = None;
                    }
                    cx.notify();
                }
            }
            ConnectionEvent::MessageReceived(tab_id, message) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    let mut message = message.clone();
                    let message_for_auto_reply = message.clone();
                    // 设置消息类型（对接收和发送的消息都设置）
                    message.set_message_type(if tab_state.message_input_mode == "text" {
                        MessageType::Text
                    } else {
                        MessageType::Hex
                    });
                    // 使用 GPUI list 自动测量高度，无需手动计算宽度
                    tab_state.add_message(message);
                    // 消息接收是关键事件，立即触发UI更新
                    cx.notify();

                    // 只有当消息方向是 Received 且是真正从网络接收到的消息时才触发自动回复
                    // 避免自动回复生成的消息又被当作新消息处理
                    if tab_state.auto_reply_enabled
                        && message_for_auto_reply.direction == MessageDirection::Received
                    {
                        if let Some(auto_reply_input) = self.auto_reply_inputs.get(&tab_id)
                        {
                            let auto_reply_content = auto_reply_input.read(cx).text().to_string();
                            if !auto_reply_content.trim().is_empty() {
                                self.send_message_to_client(tab_id, auto_reply_content, message_for_auto_reply.source.clone(), cx);
                            }
                        }
                    }
                }
            }
            ConnectionEvent::PeriodicSend(tab_id, content) => {
                // 处理周期发送文本消息
                self.send_message(tab_id, content);
            }
            ConnectionEvent::PeriodicSendBytes(tab_id, bytes, hex_input) => {
                // 处理周期发送十六进制消息
                self.send_message_bytes(tab_id, bytes, hex_input);
            }
        }
    }

}

impl Drop for NetAssistantApp {
    fn drop(&mut self) {
        debug!("[应用关闭] 开始关闭所有连接");

        let tab_ids: Vec<String> = self.connection_tabs.keys().cloned().collect();
        for tab_id in tab_ids {
            // 在drop中无法使用cx.notify()，但close_tab的主要功能是断开连接，即使没有UI更新也没关系
            // 重新定义一个内部方法来处理关闭连接但不更新UI的逻辑
            if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                tab_state.disconnect();
            }
            
            if self.connection_tabs.remove(&tab_id).is_some() {
                debug!("[关闭标签页] 移除标签页状态: {}", tab_id);
            }
            
            if self.auto_reply_inputs.remove(&tab_id).is_some() {
                debug!("[关闭标签页] 移除自动回复输入框: {}", tab_id);
            }
            
            // 清理客户端连接发送器
            if self.client_write_senders.remove(&tab_id).is_some() {
                debug!("[关闭标签页] 移除客户端连接发送器: {}", tab_id);
            }
            
            // 清理服务端客户端连接
            if self.server_clients.remove(&tab_id).is_some() {
                debug!("[关闭标签页] 移除服务端客户端连接: {}", tab_id);
            }
        }

        debug!("[应用关闭] 所有连接已关闭");
    }
}

impl Render for NetAssistantApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
