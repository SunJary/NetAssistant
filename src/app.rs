use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use log::{debug, error, info, warn};
use rust_i18n::t;

use crate::config;
use crate::config::app_stats::AppStats;
use crate::config::connection::{
    AutoReplyConfig, ConnectionConfig, ConnectionStatus, ConnectionType, DecoderConfig,
};
use crate::config::storage::ConfigStorage;
use crate::export::{self, ExportFormat};
use crate::log_writer::LogWriter;
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::{ConnectionEvent, NetCounters};
use crate::stress::engine::StressTestEngine;
use crate::stress::port_range::EphemeralPortRange;
use crate::stress::{StressEvent, StressStats, StressTestConfig, TabViewMode};
use crate::utils::hex::convert_value;

use crate::ui::components::hex_editor::HexEditorState;
use crate::ui::connection_tab::ConnectionTabState;
use crate::ui::dialog::{
    DecoderSelectionDialogState, StressConfigDialogState, open_new_connection_dialog,
    open_stress_config_dialog,
};
use crate::ui::main_window::MainWindow;

use indexmap::IndexMap;
use smol::channel::{Receiver, Sender, unbounded as smol_unbounded};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct NetAssistantApp {
    // 配置存储
    pub storage: ConfigStorage,

    // 客户端连接相关状态
    pub client_expanded: bool,
    pub new_connection_is_client: bool,
    pub host_input: Entity<InputState>,
    pub port_input: Entity<InputState>,
    /// 本地绑定地址输入(仅客户端, 留空=自动选择网卡)
    pub local_address_input: Entity<InputState>,
    /// 本地绑定端口输入(仅客户端, 留空=自动分配临时端口)
    pub local_port_input: Entity<InputState>,
    pub new_connection_protocol: String,

    // 连接编辑对话框状态（新建/编辑共用）
    // None=新建模式, Some(id)=编辑模式
    pub editing_connection_id: Option<String>,
    pub edit_message_input_mode: String,
    pub edit_decoder_config: DecoderConfig,
    pub show_connection_advanced: bool,

    // 解码器选择对话框状态(打开时创建, 关闭时置 None)
    pub decoder_selection_dialog: Option<DecoderSelectionDialogState>,

    // 服务端连接相关状态
    pub server_expanded: bool,

    // Tab页状态（每个标签页独立管理自己的网络连接）
    pub active_tab: String,
    pub connection_tabs: IndexMap<String, ConnectionTabState>,
    pub tab_multiline: bool,

    // 自动回复输入框状态（每个标签页一个）
    pub auto_reply_inputs: HashMap<String, Entity<InputState>>,
    // 自动回复输入框（hex 模式）的十六进制编辑器状态（与 auto_reply_inputs 同生命周期）
    pub auto_reply_hex_editors: HashMap<String, Entity<HexEditorState>>,
    // 自动回复输入框变更订阅(保持订阅存活; 内容变化时同步到网络层)
    pub auto_reply_input_subscriptions: HashMap<String, Subscription>,

    // 连接事件通道（用于通知UI更新）- 使用smol channel与GPUI兼容
    pub connection_event_sender: Option<Sender<ConnectionEvent>>,
    pub connection_event_receiver: Option<Receiver<ConnectionEvent>>,

    // 网络层精确计数器(每 tab 一份, 随连接创建; UI 每拍读快照覆盖显示计数)
    pub net_counters: HashMap<String, NetCounters>,

    // 压测事件通道（引擎→UI，同 smol channel 模式）
    pub stress_event_sender: Option<Sender<StressEvent>>,
    pub stress_event_receiver: Option<Receiver<StressEvent>>,

    // 压测配置弹窗状态(打开时创建, 关闭时置 None)
    pub stress_config_dialog: Option<StressConfigDialogState>,

    // 本机临时端口范围检测结果 (懒检测 + 手动重新检测, 全局共享)
    // None + !detecting: 尚未检测 或 检测失败 (UI 应提示用户手动获取而非回退默认值)
    // Some: 已检测的真实系统配置
    pub detected_port_range: Option<EphemeralPortRange>,
    // 是否已尝试检测端口范围 (true=已尝试, 无论成功失败; 防止 render 时疯狂 spawn netsh)
    pub port_range_detected: bool,
    // 是否正在检测中 (trigger_port_range_detect 置 true, 异步完成置 false)
    // 用于区分 "检测中" (detected=true && range=None && detecting=true) 与 "检测失败" (detecting=false)
    pub port_range_detecting: bool,

    // 网络连接管理器
    pub network_manager: std::sync::Arc<
        tokio::sync::Mutex<crate::network::connection::manager::NetworkConnectionManager>,
    >,

    // 写入发送器映射（无锁设计，每个标签页独立管理）- 使用smol channel
    pub client_write_senders: HashMap<String, Sender<Vec<u8>>>,
    pub server_clients: HashMap<String, HashMap<SocketAddr, Sender<Vec<u8>>>>,
    // 解码器控制发送器映射（用于运行时下发解码器配置，无需重连）
    pub decoder_control_senders: HashMap<String, Sender<DecoderConfig>>,
    pub server_decoder_controls: HashMap<String, HashMap<SocketAddr, Sender<DecoderConfig>>>,
    // 服务端自动回复共享状态(UI 下发启用开关与回复内容 → 网络层每条消息读取)
    pub server_auto_reply_states: HashMap<String, Arc<AutoReplyConfig>>,

    // 右键菜单状态
    pub show_context_menu: bool,
    pub context_menu_connection: Option<String>,
    pub context_menu_is_client: bool,
    pub context_menu_position: Option<Pixels>,
    pub context_menu_position_y: Option<Pixels>,

    // 语言切换下拉菜单状态
    pub show_language_menu: bool,

    // 添加客户端对话框状态（UDP服务端专用）
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

    // 根元素焦点句柄: 保证未聚焦任何输入框时, 全局快捷键也能命中主窗口 on_key_down
    pub root_focus: FocusHandle,
    // 消息输入框 Ctrl+Enter 发送订阅(每个标签页一份, 随关闭清理)
    pub message_input_enter_subscriptions: HashMap<String, Subscription>,
}

impl NetAssistantApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let storage = ConfigStorage::new().expect("无法创建配置存储");

        // 使用window创建InputState实体
        let host_input = cx.new(|cx| InputState::new(window, cx));
        let port_input = cx.new(|cx| InputState::new(window, cx));
        let local_address_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("new_connection.local_address_placeholder").to_string())
        });
        let local_port_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("new_connection.local_port_placeholder").to_string())
        });

        // 初始化空的连接标签页状态（不预先创建）
        let connection_tabs = IndexMap::new();
        let active_tab = String::new();

        // 创建连接事件通道 - 使用smol channel与GPUI兼容
        let (connection_event_sender, connection_event_receiver) =
            smol_unbounded::<ConnectionEvent>();

        // 创建压测事件通道（引擎→UI，同 smol channel 模式）
        let (stress_event_sender, stress_event_receiver) = smol_unbounded::<StressEvent>();

        // 初始化网络连接管理器
        let network_manager = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::network::connection::manager::NetworkConnectionManager::new(),
        ));

        // 初始化写入发送器映射
        let client_write_senders = HashMap::new();
        let server_clients = HashMap::new();
        // 初始化解码器控制发送器映射
        let decoder_control_senders = HashMap::new();
        let server_decoder_controls = HashMap::new();

        // 从配置加载侧边栏宽度和折叠状态
        let sidebar_width = storage.load_sidebar_width().map(|w| gpui::px(w as f32));
        let sidebar_collapsed = storage.load_sidebar_collapsed().unwrap_or(false);

        let mut app = Self {
            storage,
            client_expanded: true,
            new_connection_is_client: true,
            host_input,
            port_input,
            local_address_input,
            local_port_input,
            new_connection_protocol: String::from("TCP"),
            // 初始化连接编辑对话框状态
            editing_connection_id: None,
            edit_message_input_mode: String::from("text"),
            edit_decoder_config: DecoderConfig::default(),
            show_connection_advanced: false,
            // 初始化解码器选择对话框状态
            decoder_selection_dialog: None,
            server_expanded: true,
            active_tab,
            connection_tabs,
            tab_multiline: false,
            auto_reply_inputs: HashMap::new(),
            auto_reply_hex_editors: HashMap::new(),
            auto_reply_input_subscriptions: HashMap::new(),
            connection_event_sender: Some(connection_event_sender),
            connection_event_receiver: Some(connection_event_receiver),
            net_counters: HashMap::new(),
            stress_event_sender: Some(stress_event_sender),
            stress_event_receiver: Some(stress_event_receiver),
            stress_config_dialog: None,
            detected_port_range: None,
            port_range_detected: false,
            port_range_detecting: false,
            network_manager,
            client_write_senders,
            server_clients,
            decoder_control_senders,
            server_decoder_controls,
            server_auto_reply_states: HashMap::new(),
            show_context_menu: false,
            context_menu_connection: None,
            context_menu_is_client: false,
            context_menu_position: None,
            context_menu_position_y: None,
            // 语言切换下拉菜单
            show_language_menu: false,
            // 添加客户端对话框状态（UDP服务端专用）
            add_client_dialog_error: None,
            // 初始化侧边栏布局状态
            sidebar_width,
            sidebar_resizing: false,
            sidebar_collapsed,
            // 最后更新时间
            last_update_time: Instant::now(),
            // 初始化消息容器宽度
            message_container_width: None,
            // 初始化收藏功能状态
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
            root_focus: cx.focus_handle(),
            message_input_enter_subscriptions: HashMap::new(),
        };

        // 启动时聚焦根元素, 保证未点击任何输入框时快捷键也全局生效
        app.root_focus.focus(window, cx);

        // 创建专门的异步任务来处理连接事件
        // 驱动源: GPUI BackgroundExecutor::timer (Windows ThreadPoolTimer)
        //   - 不依赖 smol::Timer 的全局 reactor (项目未启 smol runtime, 不可靠)
        //   - 不依赖 smol channel recv().await 的 waker (跨 tokio/GPUI runtime 时 wake 路径丢失)
        // timer 到期 → 唤醒 cx.spawn future 的 waker → dispatch_on_main_thread + PostMessageW
        //   → GetMessageW 唤醒主循环 → poll future → try_recv 排空 → app.update 批量处理。
        // 50ms 节拍 (10Hz): 消息显示延迟上限 50ms, 可接受; try_recv 排空为微秒级, 无 CPU 压力。
        let weak_app = cx.entity().clone().downgrade();
        let event_receiver = app.connection_event_receiver.take();

        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            let receiver = match event_receiver {
                Some(receiver) => receiver,
                None => return,
            };
            let bg_executor_conn = async_app.background_executor().clone();
            loop {
                // 16ms 节拍对齐帧率(约 60fps): 让同一帧内到达的网络事件合并成一次渲染
                bg_executor_conn.timer(Duration::from_millis(16)).await;
                // 排空优先: 一次 try_recv 循环把可用事件全部取出, 去掉旧的 500 条上限,
                // 通道积压在整批渲染时一次性消化, 不产生"结束后仍在回放积压"的问题
                let mut batch: Vec<ConnectionEvent> = Vec::with_capacity(128);
                while let Ok(event) = receiver.try_recv() {
                    batch.push(event);
                    if batch.len() >= 100_000 {
                        break;
                    }
                }
                let has_events = !batch.is_empty();
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                let _ = app.update(async_app, |app, cx| {
                    // 每拍同步网络层精确计数到显示(洪泛下 UI 侧批次累加可能漏计,且手动发送不产生连接事件,
                    // 故必须周期同步才能反映真实的发送/接收总数)。仅计数变化时才触发重绘。
                    let counters_changed = app.sync_net_counters_to_ui();
                    if has_events {
                        app.handle_connection_events_batch(batch, cx);
                    } else if counters_changed {
                        cx.notify();
                    }
                });
            }
        })
        .detach();

        // 创建压测事件泵任务（引擎→UI）
        // 驱动源: GPUI BackgroundExecutor::timer, 走 Windows ThreadPoolTimer, 可靠。
        // 250ms 节拍对齐 aggregator 的 SNAPSHOT_INTERVAL, 每快照刷一次 (4Hz)。
        // 关键: timer 不依赖 vsync/窗口可见, 最小化/遮挡时仍能唤醒主循环 poll future。
        let weak_app_stress = cx.entity().clone().downgrade();
        let stress_receiver = app.stress_event_receiver.take();
        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            let receiver = match stress_receiver {
                Some(receiver) => receiver,
                None => return,
            };
            let bg_executor_stress = async_app.background_executor().clone();
            loop {
                bg_executor_stress.timer(Duration::from_millis(250)).await;
                let mut batch: Vec<StressEvent> = Vec::with_capacity(8);
                while let Ok(event) = receiver.try_recv() {
                    batch.push(event);
                    if batch.len() >= 64 {
                        break;
                    }
                }
                if batch.is_empty() {
                    if weak_app_stress.upgrade().is_none() {
                        return;
                    }
                    continue;
                }
                if let Some(app) = weak_app_stress.upgrade() {
                    let _ = app.update(async_app, |app, cx| {
                        for event in batch {
                            app.handle_stress_event(event, cx);
                        }
                    });
                } else {
                    return;
                }
            }
        })
        .detach();

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
            let version_handle = tokio_handle
                .spawn(async move { crate::update_checker::check_latest_version().await });
            let star_handle =
                tokio_handle.spawn(async { crate::update_checker::fetch_star_count().await });

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

    /// 模式切换时转换输入框内容（转换型语义：文本 ↔ Hex 双向互转）。
    ///
    /// - `text → hex`：内容按 UTF-8 逐字节编码为 hex（`${...}` 变量原样保留），
    ///   再按既有规范格式化为「两位一组、空格分隔」
    /// - `hex → text`：内容为合法 hex 时解码回字符（不可打印字节用 `\xNN` 转义）；
    ///   内容非法时不动内容（与既有「不擅自改动用户内容」一致）
    ///
    /// 覆盖消息输入框与自动回复输入框；`from_mode == to_mode` 时不做任何事。
    pub fn convert_input_on_mode_switch(
        &mut self,
        tab_id: &str,
        from_mode: &str,
        to_mode: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from_mode == to_mode {
            return;
        }
        let inputs: Vec<Entity<InputState>> = self
            .connection_tabs
            .get(tab_id)
            .and_then(|tab| tab.message_input.clone())
            .into_iter()
            .chain(self.auto_reply_inputs.get(tab_id).cloned())
            .collect();
        for input in inputs {
            let value = input.read(cx).value().to_string();
            let Some(converted) = convert_value(&value, from_mode, to_mode) else {
                continue;
            };
            let next = if to_mode == "hex" {
                crate::ui::components::hex_editor::adapter::normalize_hex_value(&converted)
                    .unwrap_or(converted)
            } else {
                converted
            };
            if next != value {
                input.update(cx, |input, cx| input.replace_all(next, window, cx));
            }
        }
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
        self.ensure_message_input_enter_subscription(&tab_id, cx);
    }

    /// 订阅消息输入框的 Ctrl+Enter 事件, 复用发送逻辑完成发送。
    ///
    /// 说明: 焦点在消息输入框时, Ctrl+Enter 会被 gpui-component Input 内部的
    /// 多行换行绑定拦截, 不会冒泡到主窗口的 on_key_down。故在此订阅
    /// `InputEvent::PressEnter { secondary: true }`(即 Ctrl/Cmd+Enter)。
    fn ensure_message_input_enter_subscription(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        if self.message_input_enter_subscriptions.contains_key(tab_id) {
            return;
        }
        let Some(message_input) = self
            .connection_tabs
            .get(tab_id)
            .and_then(|t| t.message_input.clone())
        else {
            return;
        };
        let sub_tab_id = tab_id.to_string();
        let app_handle = cx.entity().clone();
        let subscription = cx.subscribe(&message_input, {
            move |app, _input, event, cx| {
                if !matches!(
                    event,
                    InputEvent::PressEnter {
                        secondary: true,
                        ..
                    }
                ) {
                    return;
                }
                let Some(window_handle) = cx.active_window() else {
                    return;
                };
                let Some(input_entity) = app
                    .connection_tabs
                    .get(&sub_tab_id)
                    .and_then(|t| t.message_input.clone())
                else {
                    return;
                };
                // 去掉 Input 多行模式按下 Ctrl+Enter 自动插入的换行符, 避免发送多余空行
                let raw = input_entity.read(cx).text().to_string();
                let cleaned = raw.trim_end_matches(&['\n', '\r'][..]).to_string();
                if cleaned.trim().is_empty() {
                    return;
                }
                // 当前处于 app 实体更新(订阅回调)期间, 不能同步重入 app 实体。
                // 用 cx.defer 推迟到本次更新结束、实体解锁后再真正发送。
                let app_handle = app_handle.clone();
                let sub_tab_id = sub_tab_id.clone();
                let input_entity = input_entity.clone();
                cx.defer(move |cx: &mut App| {
                    let _ = window_handle.update(cx, |_view, window, cx| {
                        // 先回写清理后的内容, 再发送(发送内部按 auto_clear 决定是否清空)
                        let _ = input_entity
                            .update(cx, |input, cx| input.set_value(cleaned, window, cx));
                        let _ = app_handle.update(cx, |app, cx| {
                            app.send_message_from_tab(&sub_tab_id, window, cx);
                        });
                    });
                });
            }
        });
        self.message_input_enter_subscriptions
            .insert(tab_id.to_string(), subscription);
    }

    pub fn ensure_auto_reply_input_exists(
        &mut self,
        tab_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.auto_reply_inputs.contains_key(&tab_id) {
            // 默认回复内容随当前模式生成: 文本模式下 "ok", hex 模式下为 "ok" 的编码,
            // 避免默认值在 hex 模式被判为非法 hex
            let default_reply = if self
                .connection_tabs
                .get(&tab_id)
                .map(|tab| tab.message_input_mode == "hex")
                .unwrap_or(false)
            {
                "6F 6B"
            } else {
                "ok"
            };
            let auto_reply_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .code_editor("json")
                    .line_number(false)
                    .folding(false)
                    // .rows(5)
                    .multi_line(true)
                    // 关闭 Input 内置的原生右键菜单: 由 InputWithMode 统一挂「转换为 Hex/文本」绘制菜单
                    .context_menu(false)
                    .placeholder(t!("app_ui.auto_reply_placeholder").to_string())
            });
            auto_reply_input.update(cx, |input, cx| {
                input.set_value(default_reply.to_string(), window, cx);
            });
            // 自动回复框面板较窄: 每行 5 字节
            let hex_editor = cx.new(|cx| {
                crate::ui::components::hex_editor::HexEditorState::with_inline_bytes_per_row(
                    cx,
                    crate::ui::components::hex_editor::adapter::INLINE_BYTES_PER_ROW_AUTO_REPLY,
                )
            });
            // 订阅输入内容变化: 实时将用户配置的回复内容同步到网络层(严格按用户输入, 不改内容)
            let tab_id_for_sub = tab_id.clone();
            let subscription = cx.subscribe(&auto_reply_input, {
                move |app, _input, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        app.sync_auto_reply_to_network(&tab_id_for_sub, cx);
                    }
                }
            });
            self.auto_reply_inputs
                .insert(tab_id.clone(), auto_reply_input);
            self.auto_reply_hex_editors
                .insert(tab_id.clone(), hex_editor);
            self.auto_reply_input_subscriptions
                .insert(tab_id, subscription);
        }
    }

    /// 打开「新建连接」对话框（重置为新建模式）
    pub fn open_new_connection(
        &mut self,
        is_client: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_connection_id = None;
        self.new_connection_is_client = is_client;
        self.new_connection_protocol = String::from("TCP");
        self.show_connection_advanced = false;
        self.edit_message_input_mode = String::from("text");
        self.edit_decoder_config = DecoderConfig::default();

        let default_host = if is_client { "127.0.0.1" } else { "0.0.0.0" };
        self.host_input.update(cx, |i, cx| {
            i.set_value(default_host.to_string(), window, cx)
        });
        self.port_input
            .update(cx, |i, cx| i.set_value(String::new(), window, cx));
        // 本地绑定默认留空 = 系统自动
        self.local_address_input
            .update(cx, |i, cx| i.set_value(String::new(), window, cx));
        self.local_port_input
            .update(cx, |i, cx| i.set_value(String::new(), window, cx));
        // 占位符跟随当前语言(placeholder 在 InputState 创建时固化, 语言切换后需刷新)
        self.local_address_input.update(cx, |i, cx| {
            i.set_placeholder(
                t!("new_connection.local_address_placeholder").to_string(),
                window,
                cx,
            )
        });
        self.local_port_input.update(cx, |i, cx| {
            i.set_placeholder(
                t!("new_connection.local_port_placeholder").to_string(),
                window,
                cx,
            )
        });

        // 命令式打开对话框(由 Root 管理层叠)
        open_new_connection_dialog(cx.entity().downgrade(), window, cx);
    }

    /// 打开「编辑连接」对话框（从现有配置回填）
    pub fn open_edit_connection(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        let (address, port, message_input_mode, decoder, local_address, local_port) = match &config
        {
            ConnectionConfig::Client(c) => (
                c.server_address.clone(),
                c.server_port,
                c.message_input_mode.clone(),
                c.decoder_config.clone(),
                c.local_address.clone(),
                c.local_port,
            ),
            ConnectionConfig::Server(c) => (
                c.listen_address.clone(),
                c.listen_port,
                c.message_input_mode.clone(),
                c.decoder_config.clone(),
                None,
                None,
            ),
        };

        self.host_input
            .update(cx, |i, cx| i.set_value(address, window, cx));
        self.port_input
            .update(cx, |i, cx| i.set_value(port.to_string(), window, cx));
        // 本地绑定为 None 时回填空串(=自动)
        self.local_address_input.update(cx, |i, cx| {
            i.set_value(local_address.unwrap_or_default(), window, cx)
        });
        self.local_port_input.update(cx, |i, cx| {
            i.set_value(
                local_port.map(|p| p.to_string()).unwrap_or_default(),
                window,
                cx,
            )
        });
        // 占位符跟随当前语言(placeholder 在 InputState 创建时固化, 语言切换后需刷新)
        self.local_address_input.update(cx, |i, cx| {
            i.set_placeholder(
                t!("new_connection.local_address_placeholder").to_string(),
                window,
                cx,
            )
        });
        self.local_port_input.update(cx, |i, cx| {
            i.set_placeholder(
                t!("new_connection.local_port_placeholder").to_string(),
                window,
                cx,
            )
        });
        self.edit_message_input_mode = message_input_mode;
        self.edit_decoder_config = decoder;
        self.show_connection_advanced = true;

        // 命令式打开对话框(由 Root 管理层叠)
        open_new_connection_dialog(cx.entity().downgrade(), window, cx);
    }

    /// 确认连接表单（新建或编辑），返回是否成功（成功后由调用方关闭对话框）
    pub fn confirm_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let host = self.host_input.read(cx).value().to_string();
        let port_str = self.port_input.read(cx).value().to_string();
        if host.is_empty() || port_str.is_empty() {
            return false;
        }
        let Ok(port) = port_str.parse::<u16>() else {
            return false;
        };

        // 本地绑定(仅客户端字段; 服务端忽略): 地址留空=自动选网卡, 端口留空=自动分配。
        // 校验沿用现有惯例: 非法输入静默拒绝(返回 false, 对话框保持打开)
        let local_addr_str = self.local_address_input.read(cx).value().trim().to_string();
        let local_address = if local_addr_str.is_empty() {
            None
        } else {
            match local_addr_str.parse::<std::net::IpAddr>() {
                Ok(_) => Some(local_addr_str),
                Err(_) => return false,
            }
        };
        let local_port_str = self.local_port_input.read(cx).value().trim().to_string();
        let local_port = if local_port_str.is_empty() {
            None
        } else {
            match local_port_str.parse::<u16>() {
                Ok(p) => Some(p),
                Err(_) => return false,
            }
        };

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
                        c.local_address = local_address.clone();
                        c.local_port = local_port;
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
                // 同步已打开的标签页; 模式变更时按「转换型语义」整体互转输入内容
                // (否则切到 hex 后原文本会被判为非法 hex)
                let to_mode = updated_config.message_input_mode().to_string();
                let from_mode = self
                    .connection_tabs
                    .get(&edit_id)
                    .map(|tab_state| tab_state.message_input_mode.clone())
                    .unwrap_or_else(|| to_mode.clone());
                if let Some(tab_state) = self.connection_tabs.get_mut(&edit_id) {
                    tab_state.connection_config = updated_config.clone();
                    tab_state.message_input_mode = to_mode.clone();
                }
                self.convert_input_on_mode_switch(&edit_id, &from_mode, &to_mode, window, cx);
                // 转换走的是 replace_all(不发 Change 事件), 需手动把自动回复同步到网络层
                self.sync_auto_reply_to_network(&edit_id, cx);
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
                    c.local_address = local_address.clone();
                    c.local_port = local_port;
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
        cx.notify();
        true
    }

    pub fn close_tab(&mut self, tab_id: String, _cx: &mut Context<Self>) {
        debug!("[关闭标签页] 开始关闭标签页: {}", tab_id);

        // 角色判定必须在移除 tab 状态之前: 决定后续走客户端断开还是服务端停止
        let is_client = self
            .connection_tabs
            .get(&tab_id)
            .map(|tab_state| tab_state.connection_config.is_client());

        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            tab_state.disconnect();
        }

        if self.connection_tabs.shift_remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除标签页状态: {}", tab_id);
        }

        if self.auto_reply_inputs.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除自动回复输入框: {}", tab_id);
        }
        self.auto_reply_hex_editors.remove(&tab_id);
        if self
            .auto_reply_input_subscriptions
            .remove(&tab_id)
            .is_some()
        {
            debug!("[关闭标签页] 移除自动回复输入订阅: {}", tab_id);
        }
        self.message_input_enter_subscriptions.remove(&tab_id);
        if self.server_auto_reply_states.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除服务端自动回复共享状态: {}", tab_id);
        }

        // 清理客户端连接发送器
        if self.client_write_senders.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除客户端连接发送器: {}", tab_id);
        }

        // 清理服务端客户端连接
        if self.server_clients.remove(&tab_id).is_some() {
            debug!("[关闭标签页] 移除服务端客户端连接: {}", tab_id);
        }

        // 清理网络层计数器(避免残留条目被后续同步读取到已关闭的 tab)
        self.net_counters.remove(&tab_id);

        // 清理解码器控制通道(此前仅由 Disconnected 事件分支清理, 关闭标签页不走该分支)
        self.decoder_control_senders.remove(&tab_id);
        self.server_decoder_controls.remove(&tab_id);

        // 断开网络层: socket 与监听端口由 NetworkConnectionManager 持有, 不经过这里则
        // 端口不释放、读任务继续把消息投递给已关闭(或被重开复用同一 config.id)的 tab_id
        if let Some(is_client) = is_client {
            let network_manager_arc = self.network_manager.clone();
            let tab_id_clone = tab_id.clone();
            tokio::spawn(async move {
                let mut network_manager = network_manager_arc.lock().await;
                let result = if is_client {
                    network_manager.disconnect_client(&tab_id_clone).await
                } else {
                    network_manager.stop_server(&tab_id_clone).await
                };
                if let Err(e) = result {
                    error!(
                        "关闭标签页时断开网络连接失败: tab={}, {:?}",
                        tab_id_clone, e
                    );
                }
            });
        }

        debug!("[关闭标签页] 标签页 {} 已关闭", tab_id);
    }

    // ============== 快捷键相关方法 ==============

    /// 返回按插入顺序排列的标签页 id 列表(序号即快捷键 Ctrl+1..9 的定位)
    pub fn tab_ids_in_order(&self) -> Vec<String> {
        self.connection_tabs.keys().cloned().collect()
    }

    /// 按偏移量循环切换标签页(step>0 向后, step<0 向前)
    pub fn switch_tab(&mut self, step: isize, cx: &mut Context<Self>) {
        let ids = self.tab_ids_in_order();
        if ids.is_empty() {
            return;
        }
        let current = ids
            .iter()
            .position(|id| *id == self.active_tab)
            .unwrap_or(0);
        let len = ids.len() as isize;
        let mut next = (current as isize + step) % len;
        if next < 0 {
            next += len;
        }
        self.active_tab = ids[next as usize].clone();
        cx.notify();
    }

    /// 直接跳转到序号(从 1 计)对应的标签页, 越界或无标签时不生效
    pub fn activate_tab_by_index(&mut self, index_1_based: usize, cx: &mut Context<Self>) {
        let ids = self.tab_ids_in_order();
        if index_1_based == 0 || index_1_based > ids.len() {
            return;
        }
        self.active_tab = ids[index_1_based - 1].clone();
        cx.notify();
    }

    /// 关闭当前激活的标签页(快捷键 Ctrl+W), 复用时保留 close_tab 的清理与关闭后自动切换语义
    pub fn close_active_tab(&mut self, cx: &mut Context<Self>) {
        if self.active_tab.is_empty() {
            return;
        }
        let tab_id = self.active_tab.clone();
        self.close_tab(tab_id.clone(), cx);
        // 关闭后自动切换到剩余第一个标签页
        if let Some(first_tab_id) = self.connection_tabs.keys().next() {
            self.active_tab = (*first_tab_id).to_string();
        } else {
            self.active_tab = String::new();
        }
        cx.notify();
    }

    // ============== 压测相关方法 ==============

    /// 处理压测事件(由压测事件泵调用)
    /// 按 event 携带的 tab_id 路由, 而非 active_tab —— 用户切走 tab 后事件仍能投递到发起压测的 tab。
    pub fn handle_stress_event(&mut self, event: StressEvent, cx: &mut Context<Self>) {
        match event {
            StressEvent::StatsSnapshot { tab_id, stats } => {
                if let Some(tab) = self.connection_tabs.get_mut(&tab_id) {
                    tab.stress_stats = stats;
                    cx.notify();
                }
            }
            StressEvent::Finished { tab_id, report } => {
                if let Some(tab) = self.connection_tabs.get_mut(&tab_id) {
                    tab.stress_report = Some(report);
                    tab.stress_engine = None;
                    info!("[压测] 已完成，报告已存档 (tab={})", tab_id);
                    cx.notify();
                }
            }
            StressEvent::Error { tab_id, msg } => {
                if let Some(tab) = self.connection_tabs.get_mut(&tab_id) {
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

    /// 触发本机临时端口范围检测 (异步, 不阻塞 UI)
    ///
    /// 无条件 spawn 后台检测任务, 适合以下场景:
    /// - 用户点击"重新检测"按钮时 (端口说明弹窗 / 压测配置弹窗警告行内联按钮)
    ///
    /// render 时的按需检测 (ensure_port_range_detected) 用 port_range_detected 守卫防 storm,
    /// 不会走到这里; 这里是主动触发, 绕过守卫。
    pub fn trigger_port_range_detect(&mut self, cx: &mut Context<Self>) {
        // 立即置 true, 防止 ensure_port_range_detected 在 detect 完成前重复 spawn
        self.port_range_detected = true;
        self.port_range_detecting = true;
        let weak_app = cx.entity().downgrade();
        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            let detected = smol::unblock(|| EphemeralPortRange::detect()).await;
            if let Some(app) = weak_app.upgrade() {
                let _ = app.update(async_app, |app: &mut NetAssistantApp, cx| {
                    app.detected_port_range = detected;
                    app.port_range_detecting = false;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// 打开压测配置弹窗(回填目标 + 已保存配置)
    pub fn open_stress_config(
        &mut self,
        tab_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = match self.build_stress_config_for_tab(&tab_id) {
            Some(c) => c,
            None => {
                error!("[压测] 未找到标签页: {}", tab_id);
                return;
            }
        };
        self.stress_config_dialog = Some(StressConfigDialogState::new(tab_id, config, window, cx));
        // 端口范围检测改为懒触发: 仅当用户填的并发数超过系统默认端口数时,
        // 由 render_port_warning -> ensure_port_range_detected 兜底检测。
        // 用户也可在端口说明弹窗点"重新检测"手动触发。
        // 命令式打开对话框(由 Root 管理层叠)
        open_stress_config_dialog(cx.entity().downgrade(), window, cx);
    }

    /// 启动压测
    pub fn start_stress(
        &mut self,
        tab_id: String,
        config: StressTestConfig,
        cx: &mut Context<Self>,
    ) {
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
            self.storage
                .save_stress_profile(&connection_id, config.clone());
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
            let engine = StressTestEngine::start(config, tab_id.clone(), sender);
            tab.stress_engine = Some(Arc::new(Mutex::new(Some(engine))));
            info!("[压测] 引擎已启动 (tab={})", tab_id);
        }

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
                .add_filter(t!("app_ui.filter_csv").to_string(), &["csv"])
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
    pub fn switch_tab_view_mode(
        &mut self,
        tab_id: String,
        view_mode: TabViewMode,
        cx: &mut Context<Self>,
    ) {
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
            let client_config = if let ConnectionConfig::Client(client_config) =
                tab_state.connection_config.clone()
            {
                client_config
            } else {
                return;
            };

            let network_manager_arc = self.network_manager.clone();
            let client_config_clone = client_config.clone();
            let connection_event_sender_clone = self.connection_event_sender.clone();
            let tab_id_for_error = tab_id.clone();
            // 每 tab 一份网络层精确计数器: 重复连接时复用(计数在断开后仍累计需由 UI 层在断开时置零处理)
            let counters = self.net_counters.entry(tab_id.clone()).or_default().clone();

            tokio::spawn(async move {
                let mut network_manager = network_manager_arc.lock().await;
                if let Err(e) = network_manager
                    .create_and_connect_client_with_counters(
                        &client_config_clone,
                        connection_event_sender_clone.clone(),
                        Some(counters),
                    )
                    .await
                {
                    error!("客户端连接失败: {:?}", e);
                    // 复位乐观设置的连接状态并在 UI 提示(如目标端口不可达/连接被拒绝)
                    if let Some(sender) = &connection_event_sender_clone {
                        let _ = sender.try_send(ConnectionEvent::Error(
                            tab_id_for_error,
                            format!("连接失败: {}", e),
                        ));
                    }
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
                let tab_id_for_error = tab_id.clone();
                // 每 tab 一份网络层精确计数器
                let counters = self.net_counters.entry(tab_id.clone()).or_default().clone();

                tokio::spawn(async move {
                    let mut network_manager = network_manager_arc.lock().await;
                    if let Err(e) = network_manager
                        .create_and_start_server_with_counters(
                            &server_config_clone,
                            connection_event_sender_clone.clone(),
                            Some(counters),
                        )
                        .await
                    {
                        error!("服务端启动失败: {:?}", e);
                        // 复位乐观设置的连接状态并在 UI 提示(如端口被其他实例/进程占用)
                        if let Some(sender) = &connection_event_sender_clone {
                            let _ = sender.try_send(ConnectionEvent::Error(
                                tab_id_for_error,
                                format!("启动失败: {}", e),
                            ));
                        }
                    }
                });
            }
        }
    }

    /// 从指定标签页的消息输入框读取内容并发送，供"发送按钮"和"Ctrl+Enter 快捷键"共用。
    /// 逻辑与原先发送按钮的 on_mouse_down 闭包保持一致（含 hex 校验、连接状态校验、
    /// 自动清空、周期发送、错误提示）。
    pub fn send_message_from_tab(
        &mut self,
        tab_id: &str,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) {
        // 首先获取所有需要的值，避免后续的借用冲突
        let mut message_input_clone = None;
        let mut content = String::new();
        let mut tab_message_input_mode = String::new();
        let mut auto_clear_input = false;
        let mut periodic_send_enabled = false;
        let mut connection_config = None;
        let mut interval_ms: u64 = 1000;

        // 获取当前标签页的状态
        if let Some(tab_state) = self.connection_tabs.get_mut(tab_id) {
            // 获取消息输入内容
            if let Some(message_input) = &tab_state.message_input {
                content = message_input.read(cx).text().to_string();
                message_input_clone = Some(message_input.clone());

                // 读取周期发送间隔值
                let interval_str =
                    if let Some(periodic_interval_input) = &tab_state.periodic_interval_input {
                        periodic_interval_input.read(cx).text().to_string()
                    } else {
                        "1000".to_string()
                    };
                interval_ms = interval_str.parse::<u32>().map(u64::from).unwrap_or(1000);

                // 存储其他需要的值
                tab_message_input_mode = tab_state.message_input_mode.clone();
                auto_clear_input = tab_state.auto_clear_input;
                periodic_send_enabled = tab_state.periodic_send_enabled;
                connection_config = Some(tab_state.connection_config.clone());

                // 在发送前再次验证十六进制输入是否有效
                let is_hex_valid = if tab_message_input_mode == "hex" {
                    let hex_content = message_input.read(cx).text().to_string();
                    crate::utils::hex::validate_hex_input(&hex_content)
                } else {
                    true
                };
                if !is_hex_valid {
                    debug!("[发送] 十六进制输入格式错误，不发送");
                    return;
                }
            }
        } else {
            // Tab not found
            error!("[发送] 发送失败: 标签页不存在");
            return;
        }

        // 检查消息内容是否为空
        if content.trim().is_empty() {
            debug!("[发送] 消息内容为空，不发送");
            return;
        }

        // 确保获取到了所有必要的值
        if let Some(connection_config) = connection_config {
            // Check connection status before sending
            let can_send = if connection_config.is_client() {
                if let Some(tab_state) = self.connection_tabs.get(tab_id) {
                    tab_state.is_connected
                } else {
                    false
                }
            } else {
                // Server mode: check if there are connected clients
                self.server_clients
                    .get(tab_id)
                    .map_or(false, |clients| !clients.is_empty())
            };

            if can_send {
                // 发送消息
                if tab_message_input_mode == "hex" {
                    let bytes = crate::utils::hex::hex_to_bytes(&content);
                    self.send_message_bytes(tab_id.to_string(), bytes, content.clone());
                } else {
                    self.send_message(tab_id.to_string(), content.clone());
                }

                // Clear input ONLY on successful send initiation and if auto_clear_input is true
                if auto_clear_input {
                    if let Some(message_input) = message_input_clone {
                        message_input.update(cx, |input: &mut InputState, cx| {
                            input.set_value("", window, cx);
                        });
                    }
                }

                // 启动周期发送（如果启用）
                if periodic_send_enabled {
                    let tab_id_periodic = tab_id.to_string();
                    let content_periodic = content.clone();
                    let message_input_mode_periodic = tab_message_input_mode.clone();
                    self.start_periodic_send(
                        tab_id_periodic,
                        interval_ms,
                        content_periodic,
                        message_input_mode_periodic,
                        cx,
                    );
                }

                // 清除错误消息
                if let Some(tab_state) = self.connection_tabs.get_mut(tab_id) {
                    tab_state.error_message = None;
                }
            } else {
                // Send failed due to connection issue
                warn!("[发送] 发送失败: 连接未建立或无客户端连接");
                if let Some(tab_state) = self.connection_tabs.get_mut(tab_id) {
                    tab_state.error_message = Some(if connection_config.is_client() {
                        t!("app_ui.send_not_connected").to_string()
                    } else {
                        t!("connection_tab.error_no_client_connections").to_string()
                    });
                }
                cx.notify();
                // DO NOT clear input on connection failure
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
        let tab_info = self.connection_tabs.get(&tab_id).map(|tab_state| {
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
        let is_connected_result = self
            .connection_tabs
            .get(&tab_id)
            .map(|tab| tab.is_connected);

        if is_connected_result.is_none() {
            error!("[send_message] 未找到标签页: {}", tab_id);
            return;
        }

        let is_connected = is_connected_result.unwrap();

        if !is_connected {
            if let Some(sender) = sender {
                let _ = sender.try_send(ConnectionEvent::Error(
                    tab_id_clone,
                    t!("app_ui.send_not_connected").to_string(),
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
                            t!("app_ui.send_failed").to_string(),
                        ));
                    }
                } else {
                    debug!("[send_message] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender
                            .try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message] 客户端写入发送器不可用");
                if let Some(sender) = sender {
                    let _ = sender.try_send(ConnectionEvent::Error(
                        tab_id_clone,
                        t!("app_ui.send_client_write_unavailable").to_string(),
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
                            warn!(
                                "[send_message] 发送给客户端 {} 失败（客户端可能已断开）",
                                target_addr
                            );
                        } else {
                            debug!("[send_message] 定向发送成功");
                            if let Some(sender) = sender {
                                let message =
                                    Message::new(MessageDirection::Sent, bytes, message_type)
                                        .with_source(target_addr.to_string());
                                let _ = sender.try_send(ConnectionEvent::MessageReceived(
                                    tab_id_clone,
                                    message,
                                ));
                            }
                        }
                    } else {
                        warn!("[send_message] 客户端 {} 不存在或已断开", target_addr);
                    }
                } else {
                    // 广播给所有客户端（并行发送）
                    debug!(
                        "[send_message] 服务端模式，广播给所有客户端，共 {} 个",
                        clients.len()
                    );
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
                        let _ = sender
                            .try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
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
        let tab_info = self.connection_tabs.get(&tab_id).map(|tab_state| {
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
        let is_connected_result = self
            .connection_tabs
            .get(&tab_id)
            .map(|tab| tab.is_connected);

        if is_connected_result.is_none() {
            error!("[send_message_bytes] 未找到标签页: {}", tab_id);
            return;
        }

        let is_connected = is_connected_result.unwrap();

        if !is_connected {
            if let Some(sender) = sender {
                let _ = sender.try_send(ConnectionEvent::Error(
                    tab_id_clone,
                    t!("app_ui.send_not_connected").to_string(),
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
                            t!("app_ui.send_failed").to_string(),
                        ));
                    }
                } else {
                    debug!("[send_message_bytes] 发送成功");
                    if let Some(sender) = sender {
                        let message = Message::new(MessageDirection::Sent, bytes, message_type);
                        let _ = sender
                            .try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                error!("[send_message_bytes] 客户端写入发送器不可用");
                if let Some(sender) = sender {
                    let _ = sender.try_send(ConnectionEvent::Error(
                        tab_id_clone,
                        t!("app_ui.send_client_write_unavailable").to_string(),
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
                    debug!(
                        "[send_message_bytes] 服务端模式，定向发送给: {}",
                        target_addr
                    );
                    if let Some(write_sender) = clients.get(&target_addr) {
                        if write_sender.try_send(bytes.clone()).is_err() {
                            // 单个客户端发送失败不应影响整个服务端，仅记录日志
                            warn!(
                                "[send_message_bytes] 发送给客户端 {} 失败（客户端可能已断开）",
                                target_addr
                            );
                        } else {
                            debug!("[send_message_bytes] 定向发送成功");
                            if let Some(sender) = sender {
                                let message =
                                    Message::new(MessageDirection::Sent, bytes, message_type)
                                        .with_source(target_addr.to_string());
                                let _ = sender.try_send(ConnectionEvent::MessageReceived(
                                    tab_id_clone,
                                    message,
                                ));
                            }
                        }
                    } else {
                        warn!("[send_message_bytes] 客户端 {} 不存在或已断开", target_addr);
                    }
                } else {
                    // 广播给所有客户端（并行发送）
                    debug!(
                        "[send_message_bytes] 服务端模式，广播给所有客户端，共 {} 个",
                        clients.len()
                    );
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
                        let _ = sender
                            .try_send(ConnectionEvent::MessageReceived(tab_id_clone, message));
                    }
                }
            } else {
                warn!("[send_message_bytes] 服务器客户端映射不可用");
            }
        }
    }

    /// 向UDP服务端手动添加客户端地址
    pub fn add_client_to_server(
        &mut self,
        tab_id: String,
        addr_str: String,
        cx: &mut Context<Self>,
    ) {
        let addr: std::net::SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(_) => {
                error!("[add_client_to_server] 无效的地址格式: {}", addr_str);
                if let Some(sender) = &self.connection_event_sender {
                    let _ = sender.try_send(ConnectionEvent::Error(
                        tab_id,
                        t!("app_ui.invalid_address_format", addr = addr_str).to_string(),
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
                            t!("app_ui.add_client_failed", error = e).to_string(),
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
        let address_label = self
            .connection_tabs
            .get(&tab_id)
            .map(|t| t.connection_config.address_label())
            .unwrap_or_else(|| "export".to_string());

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let default_filename = format!("{}_{}.txt", address_label, timestamp);

        // 在异步任务中弹出文件对话框并保存
        tokio::spawn(async move {
            let file_path = rfd::AsyncFileDialog::new()
                .set_file_name(&default_filename)
                .add_filter(t!("app_ui.filter_text").to_string(), &["txt"])
                .add_filter(t!("app_ui.filter_json").to_string(), &["json"])
                .add_filter(t!("app_ui.filter_csv").to_string(), &["csv"])
                .save_file()
                .await;

            if let Some(file_path) = file_path {
                let path = file_path.path();

                // 根据扩展名确定格式，默认为 txt
                let format = ExportFormat::from_extension(path).unwrap_or(ExportFormat::Txt);

                match export::format_messages(&messages, format) {
                    Ok(content) => match std::fs::write(path, content) {
                        Ok(_) => {
                            debug!("[导出] 消息记录已导出到: {:?}", path);
                        }
                        Err(e) => {
                            error!("[导出] 写入文件失败: {:?}", e);
                        }
                    },
                    Err(e) => {
                        error!("[导出] 格式化消息失败: {}", e);
                    }
                }
            }
        });
    }

    /// 切换消息显示模式（原始/美化/压缩）
    ///
    /// 惰性计算: 仅切换标记并触发重绘,格式化由渲染可见项时按需填充
    /// display_cache 完成,1 万条消息下切换为 O(1) 而非 O(全部消息)。
    pub fn toggle_message_display_mode(&mut self, tab_id: String, cx: &mut Context<Self>) {
        if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
            let new_mode = tab_state.message_display_mode.next();
            tab_state.message_display_mode = new_mode;
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
                let log_path = tab_state
                    .custom_log_path
                    .as_ref()
                    .map(|p| std::path::PathBuf::from(p))
                    .unwrap_or_else(|| {
                        LogWriter::default_log_path(&tab_state.connection_config.address_label())
                    });

                // 确保日志目录存在（同步创建，很快）
                if let Some(parent) = log_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                let log_path_display = log_path.display().to_string();
                let log_path_for_writer = log_path.clone();

                // 使用 cx.spawn 异步打开文件并更新状态
                let tab_id_clone = tab_id.clone();
                cx.spawn(
                    async move |this, cx| match LogWriter::open(log_path_for_writer).await {
                        Ok(log_writer) => {
                            let writer_arc =
                                std::sync::Arc::new(tokio::sync::Mutex::new(log_writer));
                            let _ = this.update(cx, |app, cx| {
                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_clone)
                                {
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
                    },
                )
                .detach();

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
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(dir).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                }
            }
        }
    }

    /// 修改日志保存路径
    pub fn change_log_path(&mut self, tab_id: String, cx: &mut Context<Self>) {
        // 先关闭当前日志
        let was_enabled = self
            .connection_tabs
            .get(&tab_id)
            .map(|t| t.log_enabled)
            .unwrap_or(false);

        if was_enabled {
            self.toggle_log(tab_id.clone(), cx);
        }

        let address_label = self
            .connection_tabs
            .get(&tab_id)
            .map(|t| t.connection_config.address_label())
            .unwrap_or_else(|| "log".to_string());

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let default_filename = format!("{}_{}.log", address_label, timestamp);

        let tab_id_clone = tab_id.clone();
        cx.spawn(async move |this, cx| {
            let file_path = rfd::AsyncFileDialog::new()
                .set_file_name(&default_filename)
                .add_filter(t!("app_ui.filter_log").to_string(), &["log"])
                .add_filter(t!("app_ui.filter_all").to_string(), &["*"])
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
        })
        .detach();
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

    /// 用户点击「给个 Star」按钮：跳转 repo 页面 + 关闭提示并持久化（用户已点 Star，不再骚扰）
    pub fn accept_star_prompt(&mut self, cx: &mut Context<Self>) {
        cx.open_url("https://github.com/SunJary/NetAssistant");
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.stats.dismiss_star_prompt(&today);
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

    /// 切换界面语言：设置运行时 locale、同步组件库内置文案，并持久化到配置
    /// 切换后 cx.notify() 触发整棵视图树重渲染，t! 宏取词即生效
    pub fn set_language(&mut self, language: &str, cx: &mut Context<Self>) {
        rust_i18n::set_locale(language);
        gpui_component::set_locale(language);
        self.storage.save_language(language);
        self.show_language_menu = false;
        info!("界面语言已切换为: {}", language);
        cx.notify();
    }

    /// 批量处理连接事件: 聚合同 tab 的 MessageReceived 一次添加,其他事件走单条处理。
    /// 整批仅触发一次 cx.notify(),大幅降低高并发消息洪泛下的 UI 开销。
    pub fn handle_connection_events_batch(
        &mut self,
        events: Vec<ConnectionEvent>,
        cx: &mut Context<Self>,
    ) {
        let mut need_notify = false;
        // 聚合同 tab 的消息,延迟批量添加
        let mut message_batch: Vec<Message> = Vec::new();
        let mut batch_tab_id: Option<String> = None;

        for event in events {
            match event {
                ConnectionEvent::MessageReceived(tab_id, mut message) => {
                    // tab 切换时先 flush 已收集的消息
                    if batch_tab_id.as_ref() != Some(&tab_id) {
                        self.flush_message_batch(
                            batch_tab_id.take(),
                            std::mem::take(&mut message_batch),
                            &mut need_notify,
                        );
                    }
                    batch_tab_id = Some(tab_id.clone());

                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        // 设置消息类型
                        message.set_message_type(if tab_state.message_input_mode == "text" {
                            MessageType::Text
                        } else {
                            MessageType::Hex
                        });
                        message_batch.push(message);
                    }
                }
                ConnectionEvent::MessagesReceived(tab_id, mut batch) => {
                    // 与单条消息同处理: tab 切换时先 flush 已收集的单条消息批
                    if batch_tab_id.as_ref() != Some(&tab_id) {
                        self.flush_message_batch(
                            batch_tab_id.take(),
                            std::mem::take(&mut message_batch),
                            &mut need_notify,
                        );
                    }
                    batch_tab_id = Some(tab_id.clone());

                    if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                        // 按 tab 输入模式统一标注消息类型(与单条 MessageReceived 路径一致)
                        let mode = if tab_state.message_input_mode == "text" {
                            MessageType::Text
                        } else {
                            MessageType::Hex
                        };
                        for m in batch.messages.iter_mut() {
                            m.set_message_type(mode);
                        }
                        // 自动回复的 Sent 明细聚合在同一批, 合并进列表展示
                        for m in batch.sent_messages.iter_mut() {
                            m.set_message_type(mode);
                        }
                        // batch 里的 Vec 通过 extend 移入 message_batch, 无额外拷贝
                        message_batch.extend(batch.messages);
                        message_batch.extend(batch.sent_messages);
                    }
                }
                // 其他事件: 先 flush 待处理消息批, 再走单条处理
                other => {
                    self.flush_message_batch(
                        batch_tab_id.take(),
                        std::mem::take(&mut message_batch),
                        &mut need_notify,
                    );
                    self.handle_single_connection_event(other, cx);
                    need_notify = true;
                }
            }
        }

        // flush 末尾消息批
        self.flush_message_batch(
            batch_tab_id.take(),
            std::mem::take(&mut message_batch),
            &mut need_notify,
        );

        if need_notify {
            cx.notify();
        }
    }

    /// 每拍将网络层精确计数器快照同步到各 tab 的显示计数。
    ///
    /// 返回是否有任一 tab 计数发生变化(由调用方决定是否触发重绘)。
    fn sync_net_counters_to_ui(&mut self) -> bool {
        let mut changed = false;
        for (tab_id, counters) in &self.net_counters {
            if let Some(tab_state) = self.connection_tabs.get_mut(tab_id) {
                let (received, sent, _rb, _sb) = counters.snapshot();
                if tab_state.message_list.total_received != received as usize
                    || tab_state.message_list.total_sent != sent as usize
                {
                    tab_state.message_list.sync_totals(received, sent);
                    changed = true;
                }
            }
        }
        changed
    }

    /// 清空消息时调用: 网络层计数一并归零, 避免下一拍同步把显示计数覆盖回原值。
    pub fn reset_net_counters(&mut self, tab_id: &str) {
        if let Some(counters) = self.net_counters.get(tab_id) {
            counters.reset();
        }
    }

    /// flush 一批待处理消息到对应 tab
    fn flush_message_batch(
        &mut self,
        tab_id: Option<String>,
        messages: Vec<Message>,
        need_notify: &mut bool,
    ) {
        if messages.is_empty() {
            return;
        }
        if let Some(tab_id) = tab_id {
            if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                tab_state.add_messages_batch(messages);
                // 仅活动 tab 触发重绘：非活动 tab 的消息列表未挂载，notify 无意义；
                // 列表状态已通过 splice 同步，切回时自然渲染最新数据。
                // 压测场景下被压测 tab 常处于后台，此举可显著减少无效重绘。
                if tab_id == self.active_tab {
                    *need_notify = true;
                }
            }
        }
    }

    /// 将 UI 层自动回复配置(开关 + 内容)同步到网络层共享状态。
    ///
    /// 内容严格按用户输入转换: 文本模式 = UTF-8 字节; 十六进制模式 = 解析后的字节。
    /// 不额外修改内容、不添加换行符; 编码由用户配置的 encoder 负责。
    /// 服务端未就绪(未启动)时无目标, 待 ServerAutoReplyStateReady 到达后再同步。
    pub fn sync_auto_reply_to_network(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        let Some(auto_reply_state) = self.server_auto_reply_states.get(tab_id).cloned() else {
            return;
        };
        let Some(tab_state) = self.connection_tabs.get(tab_id) else {
            return;
        };
        let enabled = tab_state.auto_reply_enabled;
        let bytes = match self.auto_reply_inputs.get(tab_id) {
            Some(input) => {
                let text = input.read(cx).text().to_string();
                if tab_state.message_input_mode == "hex" {
                    crate::utils::hex::hex_to_bytes(&text)
                } else {
                    text.into_bytes()
                }
            }
            None => Vec::new(),
        };
        auto_reply_state.set(enabled, bytes);
    }

    /// 运行时下发解码器配置到在线连接(客户端或服务端所有已连接客户端)，无需重连。
    /// 仅对 TCP 生效(UDP 为数据报，无分帧解码器)。
    pub fn apply_decoder_config_to_connection(&mut self, tab_id: &str, config: &DecoderConfig) {
        // 客户端: 下发给单条读任务
        if let Some(sender) = self.decoder_control_senders.get(tab_id) {
            debug!(
                "[apply_decoder_config] 下发解码器配置到客户端 {}: {:?}",
                tab_id, config
            );
            if let Err(e) = sender.try_send(config.clone()) {
                error!(
                    "[apply_decoder_config] 下发解码器配置失败(客户端 {}): {:?}",
                    tab_id, e
                );
            }
        }
        // 服务端: 下发给该 tab 下所有已连接客户端的读任务
        if let Some(map) = self.server_decoder_controls.get(tab_id) {
            for sender in map.values() {
                let _ = sender.try_send(config.clone());
            }
            debug!(
                "[apply_decoder_config] 下发解码器配置到服务端 {} 的 {} 个客户端",
                tab_id,
                map.len()
            );
        }
    }

    pub fn handle_single_connection_event(
        &mut self,
        event: ConnectionEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            // 批量接收事件仅在 handle_connection_events_batch 处理, 不应到达单条路径
            ConnectionEvent::MessagesReceived(_, _) => {}
            ConnectionEvent::Connected(tab_id, local_addr) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    tab_state.is_connected = true;
                    tab_state.connection_status = ConnectionStatus::Connected;
                    // 记录实际生效的本地端点(UDP 自动分配的端口只有这里能拿到)
                    tab_state.local_endpoint = Some(local_addr.to_string());
                    tab_state.error_message = None;
                    cx.notify();
                }
            }
            ConnectionEvent::Disconnected(tab_id) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    tab_state.is_connected = false;
                    tab_state.connection_status = ConnectionStatus::Disconnected;
                    tab_state.local_endpoint = None;
                    cx.notify();
                }
                self.client_write_senders.remove(&tab_id);
                self.server_clients.remove(&tab_id);
                self.decoder_control_senders.remove(&tab_id);
                self.server_decoder_controls.remove(&tab_id);
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
                    tab_state.local_endpoint = None;
                    cx.notify();
                }
                // 清理连接信息，确保下次发送时直接失败。
                // 注意: 解码器控制通道(decoder_control_senders/server_decoder_controls)不在此清理——
                // 发送方向出错不代表读方向已结束, 读任务仍需持有其控制通道; 丢弃 sender 会让该连接
                // 从此收不到运行时解码器下发(历史上还会因该分支恒就绪而让读任务忙转独占运行时)。
                // 这些条目由重连覆盖或 close_tab 统一清理。
                self.client_write_senders.remove(&tab_id);
                self.server_clients.remove(&tab_id);
            }
            ConnectionEvent::ClientWriteSenderReady(tab_id, write_sender) => {
                // 标签页已关闭: 丢弃孤儿连接的回填事件, 避免已清理的 map 被重新填满
                if !self.connection_tabs.contains_key(&tab_id) {
                    debug!(
                        "[handle_connection_events] 忽略已关闭标签页的事件: tab_id={}",
                        tab_id
                    );
                    return;
                }
                debug!(
                    "[handle_connection_events] 客户端写入发送器就绪: {}",
                    tab_id
                );
                self.client_write_senders.insert(tab_id, write_sender);
            }
            ConnectionEvent::DecoderControlSenderReady(tab_id, control_sender) => {
                // 标签页已关闭: 丢弃孤儿连接的回填事件, 避免已清理的 map 被重新填满
                if !self.connection_tabs.contains_key(&tab_id) {
                    debug!(
                        "[handle_connection_events] 忽略已关闭标签页的事件: tab_id={}",
                        tab_id
                    );
                    return;
                }
                debug!(
                    "[handle_connection_events] 客户端解码器控制发送器就绪: {}",
                    tab_id
                );
                self.decoder_control_senders.insert(tab_id, control_sender);
            }
            ConnectionEvent::ServerDecoderControlSenderReady(tab_id, addr, control_sender) => {
                // 标签页已关闭: 丢弃孤儿连接的回填事件, 避免已清理的 map 被重新填满
                if !self.connection_tabs.contains_key(&tab_id) {
                    debug!(
                        "[handle_connection_events] 忽略已关闭标签页的事件: tab_id={}",
                        tab_id
                    );
                    return;
                }
                debug!(
                    "[handle_connection_events] 服务端解码器控制发送器就绪: tab_id={}, addr={}",
                    tab_id, addr
                );
                if !self.server_decoder_controls.contains_key(&tab_id) {
                    self.server_decoder_controls
                        .insert(tab_id.clone(), HashMap::new());
                }
                if let Some(map) = self.server_decoder_controls.get_mut(&tab_id) {
                    map.insert(addr, control_sender);
                }
            }
            ConnectionEvent::ServerClientConnected(tab_id, addr, write_sender) => {
                // 标签页已关闭: 丢弃孤儿连接的回填事件, 避免已清理的 map 被重新填满
                if !self.connection_tabs.contains_key(&tab_id) {
                    debug!(
                        "[handle_connection_events] 忽略已关闭标签页的事件: tab_id={}",
                        tab_id
                    );
                    return;
                }
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
                if let Some(map) = self.server_decoder_controls.get_mut(&tab_id) {
                    map.remove(&addr);
                }
                // 更新 ConnectionTabState 中的客户端连接列表
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    if let Some(idx) = tab_state.client_connections.iter().position(|&c| c == addr)
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
            ConnectionEvent::ServerAutoReplyStateReady(tab_id, auto_reply_state) => {
                // 服务端共享状态就绪: 保存句柄, 并将 UI 当前配置推送到网络层
                self.server_auto_reply_states
                    .insert(tab_id.clone(), auto_reply_state);
                self.sync_auto_reply_to_network(&tab_id, cx);
                cx.notify();
            }
            ConnectionEvent::MessageReceived(tab_id, message) => {
                if let Some(tab_state) = self.connection_tabs.get_mut(&tab_id) {
                    let mut message = message.clone();
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

            if self.connection_tabs.shift_remove(&tab_id).is_some() {
                debug!("[关闭标签页] 移除标签页状态: {}", tab_id);
            }

            if self.auto_reply_inputs.remove(&tab_id).is_some() {
                debug!("[关闭标签页] 移除自动回复输入框: {}", tab_id);
            }
            self.auto_reply_hex_editors.remove(&tab_id);

            if self
                .auto_reply_input_subscriptions
                .remove(&tab_id)
                .is_some()
            {
                debug!("[关闭标签页] 移除自动回复输入订阅: {}", tab_id);
            }
            self.message_input_enter_subscriptions.remove(&tab_id);

            if self.server_auto_reply_states.remove(&tab_id).is_some() {
                debug!("[关闭标签页] 移除服务端自动回复共享状态: {}", tab_id);
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
