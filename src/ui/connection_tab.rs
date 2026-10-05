use crate::ui::components::hex_editor::{HexEditorState, adapter as hex_adapter};
use crate::ui::components::input_with_mode::InputWithMode;
use crate::ui::dialog::variable_picker::{
    VariableItem, VariablePickerTarget, message_variable_items, render_variable_picker,
};
use crate::ui::dialog::{
    DecoderSelectionDialogState, ReplyRulesDialogState, open_add_client_dialog,
    open_decoder_selection_dialog, open_favorite_remark_dialog, open_reply_rules_dialog,
};
use gpui_kit::component::ElementExt as _;
use gpui_kit::component::{ActiveTheme as _, Sizable, StyledExt};
use gpui_kit::component::{
    Icon, IconName, Size, Theme,
    clipboard::Clipboard,
    input::{EditorState, Input, InputState},
    scroll::{Scrollbar, ScrollbarMode},
    switch::Switch,
    tooltip::Tooltip,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use log::{debug, info};
use rust_i18n::t;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

use crate::app::NetAssistantApp;
use crate::config::connection::{
    ConnectionConfig, ConnectionStatus, ConnectionType, TrailerKind, TrailerSetting,
};
use crate::custom_icons::CustomIconName;
use crate::log_writer::LogWriter;
use crate::message::{
    DEFAULT_KEEP_LAST, Message, MessageDirection, MessageDisplayMode, MessageListState,
};
use crate::send_task::{IntervalHandle, PeriodicSource, SendTaskEntry};
use crate::stress::engine::StressTestEngine;
use crate::stress::{StressReport, StressStats, StressTestConfig, TabViewMode};
use crate::ui::send_task_panel::SendTaskPanel;
use crate::ui::stress_panel::StressPanel;
use indexmap::IndexMap;

/// 消息搜索匹配重算的最小间隔。
///
/// 重算挂在事件泵 16ms 节拍上，再叠加这一层限流：压测洪泛下最多 4 次/秒，
/// 避免每拍都做一次 O(N) 全量扫描。
pub const SEARCH_RECALC_MIN_INTERVAL: Duration = Duration::from_millis(250);

/// 连接标签页状态
#[derive(Clone)]
pub struct ConnectionTabState {
    pub connection_config: ConnectionConfig,
    pub connection_status: ConnectionStatus,
    pub message_list: MessageListState,
    pub is_connected: bool,
    pub error_message: Option<String>,
    /// 实际生效的本地端点(IP:端口, 连接成功后由 Connected 事件上报; UDP 自动分配端口也可见)
    pub local_endpoint: Option<String>,
    pub auto_scroll_enabled: bool,
    pub client_connections: Vec<SocketAddr>,
    pub selected_client: Option<SocketAddr>,

    // GPUI List 状态
    pub message_list_state: ListState,
    // GPUI List 状态 - 客户端列表（虚拟化渲染，避免大量客户端时卡顿）
    pub client_list_state: ListState,

    // 消息显示模式（原始/美化/压缩）
    pub message_display_mode: MessageDisplayMode,

    // 每个标签页独立的功能
    pub message_input: Option<Entity<EditorState>>,
    /// 发送输入框（hex 模式）的十六进制编辑器状态，与 message_input 同步创建
    pub message_hex_editor: Option<Entity<HexEditorState>>,
    pub message_input_mode: String,
    pub auto_clear_input: bool,
    pub periodic_send_enabled: bool,
    pub periodic_interval_input: Option<Entity<InputState>>,
    /// 周期发送的实时载荷(主线程写, 引擎每轮读; 无需重建任务)
    pub periodic_source: Arc<PeriodicSource>,
    /// 周期发送间隔句柄(输入变化即写入并唤醒引擎, 立刻生效)
    pub periodic_interval_handle: IntervalHandle,

    /// 「保留最后 N 条」输入框（0 = 不限制）。关闭自动滚动时禁用并强制为 0。
    pub keep_last_input: Entity<InputState>,
    /// 关闭自动滚动前的保留条数，用于重新开启时恢复（含 0）。
    pub keep_last_backup: usize,

    // ===== 消息变量（${...}）插入 =====
    /// 变量浮层当前目标（同一时刻只允许一个浮层）
    pub variable_picker_target: Option<VariablePickerTarget>,
    /// 普通消息区「插入变量」按钮的窗口坐标（on_prepaint 更新，供浮层锚定）
    pub message_var_button_bounds: Option<Bounds<Pixels>>,
    /// 手动发送与周期发送共享的递增序号（${seq}）
    pub message_seq: Arc<AtomicU64>,

    // ===== 消息搜索（每个标签页独立） =====
    /// 搜索浮层是否展开
    pub search_open: bool,
    /// 搜索浮层输入框（每标签一个）
    pub search_input: Entity<InputState>,
    /// 当前生效的查询词（快照自 InputEvent::Change，避免渲染时读 Entity）
    pub search_query: String,
    /// 命中的 Message.id 列表（有序，与消息在列表中的先后一致）。
    ///
    /// 缓存 id 而非下标：淘汰走 drain(0..dropped) 从头部删，下标必然前移失效，
    /// 而 id 是 UUID，跨淘汰稳定。
    pub search_match_ids: Vec<String>,
    /// 当前命中序号（0-based；展示为 cursor + 1）
    pub search_cursor: usize,
    /// 匹配集合需要重算（消息新增/淘汰/清空/过滤变化时置位，由事件泵节流消费）
    pub search_dirty: bool,
    /// 上次重算时刻，用于事件泵限流
    pub search_last_recalc: Instant,

    // 服务端和客户端的控制句柄
    pub server_handle: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,
    pub client_handle: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,

    /// 收藏内容集合。Arc 包装使渲染路径 clone 为 O(1) 引用计数,
    /// 避免每次重渲染深拷贝整个集合;修改走 Arc::make_mut。
    pub favorited_contents: Arc<HashSet<String>>,

    // 日志记录相关
    pub log_enabled: bool,
    pub log_file_path: Option<String>,
    pub custom_log_path: Option<String>,
    pub log_writer: Option<Arc<tokio::sync::Mutex<LogWriter>>>,

    // 压测相关状态
    /// Tab 视图模式: 调试 / 压测
    pub view_mode: TabViewMode,
    /// 压测引擎句柄(因 ConnectionTabState: Clone, 用 Arc<Mutex<Option<...>>>)
    pub stress_engine: Option<Arc<Mutex<Option<StressTestEngine>>>>,
    /// 最新统计快照(由事件泵写入, render 同步读)
    pub stress_stats: StressStats,
    /// 最终报告(压测结束后存档)
    pub stress_report: Option<StressReport>,
    /// 当前运行的压测配置快照
    pub stress_config_snapshot: Option<StressTestConfig>,

    // ===== 发送任务(逐行发送) =====
    /// 该 tab 的发送任务列表(引擎句柄 + 运行期状态); 事件泵写入进度
    pub send_tasks: IndexMap<String, SendTaskEntry>,
    /// 发送任务面板(Popover)是否展开
    pub send_task_panel_open: bool,
    /// 展开行清单的任务 id(同一时刻最多一个)
    pub send_task_expanded: Option<String>,
    /// 发送任务图标按钮的窗口坐标(on_prepaint 更新, 供面板锚定)
    pub send_task_button_bounds: Option<Bounds<Pixels>>,

    /// 运行期「结尾追加字符」设置: 下拉改动即时生效(持久化真源为 connection_config.send_trailer)
    pub send_trailer_setting: TrailerSetting,
}

impl ConnectionTabState {
    pub fn new(
        connection_config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        // 从连接配置中恢复发送消息输入模式
        let message_input_mode = connection_config.message_input_mode().to_string();
        // 运行期结尾追加设置的初始值取自持久化配置(下拉改动即时生效, 无需重连)
        let send_trailer = connection_config.send_trailer();
        // 手动发送与周期发送共享的递增序号
        let message_seq = Arc::new(AtomicU64::new(0));
        // 周期发送的实时载荷: 初始为空, 内容/模式由发送框订阅实时写入
        let periodic_source = Arc::new(PeriodicSource::new(
            message_seq.clone(),
            String::new(),
            message_input_mode == "hex",
        ));
        Self {
            connection_config,
            connection_status: ConnectionStatus::NotConnected,
            message_list: MessageListState::new(),
            is_connected: false,
            error_message: None,
            local_endpoint: None,
            auto_scroll_enabled: true,
            client_connections: Vec::new(),
            selected_client: None,

            // GPUI List 状态
            // 说明: 不使用 measure_all()。该模式会在首次挂载或宽度变化时渲染并测量
            // 全部条目, 压测场景下消息列表满 1 万条时会导致切 tab 卡顿数秒。
            // 使用默认虚拟化按需测量, 仅渲染可见项。
            message_list_state: ListState::new(0, ListAlignment::Top, px(100.)),
            client_list_state: ListState::new(0, ListAlignment::Top, px(32.)),

            // 消息显示模式默认为原始
            message_display_mode: MessageDisplayMode::Normal,

            // 初始化每个标签页独立的功能
            message_input: Some(cx.new(|cx| {
                EditorState::new(window, cx)
                    .language("json")
                    .line_number(false)
                    .folding(false)
                    // 关闭 Input 内置的原生右键菜单: 由 InputWithMode 统一挂「转换为 Hex/文本」绘制菜单
                    .context_menu(false)
                    .placeholder(t!("connection_tab.message_input_placeholder"))
            })),
            // 消息发送框: 每行 16 字节
            message_hex_editor: Some(cx.new(|cx| {
                HexEditorState::with_inline_bytes_per_row(
                    cx,
                    hex_adapter::INLINE_BYTES_PER_ROW_WIDE,
                )
            })),
            message_input_mode,
            auto_clear_input: true,
            periodic_send_enabled: false,
            periodic_interval_input: {
                let input = cx.new(|cx| InputState::new(window, cx));
                // 设置周期发送的默认值为1000
                input.update(cx, |input, cx| {
                    input.set_value("1000".to_string(), window, cx);
                });
                Some(input)
            },
            periodic_source,
            periodic_interval_handle: IntervalHandle::new(1000),

            keep_last_input: {
                let input = cx.new(|cx| InputState::new(window, cx));
                input.update(cx, |input, cx| {
                    input.set_value(DEFAULT_KEEP_LAST.to_string(), window, cx);
                });
                input
            },
            keep_last_backup: DEFAULT_KEEP_LAST,

            // 消息变量插入（默认收起）
            variable_picker_target: None,
            message_var_button_bounds: None,
            message_seq,

            // 消息搜索（默认收起，无查询词 → 零扫描）
            search_open: false,
            search_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(t!("connection_tab.search_placeholder").to_string())
            }),
            search_query: String::new(),
            search_match_ids: Vec::new(),
            search_cursor: 0,
            search_dirty: false,
            search_last_recalc: Instant::now(),

            // 初始化服务端和客户端的控制句柄
            server_handle: None,
            client_handle: None,

            favorited_contents: Arc::new(HashSet::new()),

            // 初始化日志记录
            log_enabled: false,
            log_file_path: None,
            custom_log_path: None,
            log_writer: None,

            // 初始化压测状态
            view_mode: TabViewMode::Debug,
            stress_engine: None,
            stress_stats: StressStats::default(),
            stress_report: None,
            stress_config_snapshot: None,

            // 发送任务(初始为空)
            send_tasks: IndexMap::new(),
            send_task_panel_open: false,
            send_task_expanded: None,
            send_task_button_bounds: None,
            send_trailer_setting: TrailerSetting::new(send_trailer),
        }
    }

    pub fn protocol(&self) -> &str {
        match self.connection_config.protocol() {
            ConnectionType::Tcp => "TCP",
            ConnectionType::Udp => "UDP",
        }
    }

    pub fn address(&self) -> String {
        match &self.connection_config {
            ConnectionConfig::Client(config) => {
                format!("{}:{}", config.server_address, config.server_port)
            }
            ConnectionConfig::Server(config) => {
                format!("{}:{}", config.listen_address, config.listen_port)
            }
        }
    }

    pub fn decoder(&self) -> String {
        match &self.connection_config {
            ConnectionConfig::Client(config) => {
                format!("{}", config.decoder_config)
            }
            ConnectionConfig::Server(config) => {
                format!("{}", config.decoder_config)
            }
        }
    }

    pub fn add_message(&mut self, message: Message) {
        // 日志记录：异步写入文件
        if self.log_enabled {
            if let Some(log_writer) = &self.log_writer {
                let writer = log_writer.clone();
                let msg = message.clone();
                tokio::spawn(async move {
                    let writer = writer.lock().await;
                    writer.write_message(&msg).await;
                });
            }
        }

        let old_count = self.message_list.messages.len();
        let dropped = self.message_list.add_message(message);
        let new_count = self.message_list.messages.len();
        // 命中集合可能变化：只置脏标记，重算交给事件泵限流消费（此处不做 O(N) 扫描）
        self.search_dirty = true;

        // 先补尾部、再删头部：每一步都作用在当时的索引空间上
        self.message_list_state.splice(old_count..old_count, 1);
        if dropped > 0 {
            // 不变式：淘汰只发生在自动滚动开启时，随后的 scroll_to 会覆盖锚点
            self.message_list_state.splice(0..dropped, 0);
        }

        if self.auto_scroll_enabled && new_count > 0 {
            self.message_list_state.scroll_to(gpui_kit::ListOffset {
                item_ix: new_count,
                offset_in_item: px(0.),
            });
        }
    }

    /// 批量添加消息：仅触发一次列表状态更新 + 一次滚动，用于高并发消息洪泛场景。
    /// 日志记录同样批量触发，避免逐条 spawn。
    pub fn add_messages_batch(&mut self, messages: Vec<Message>) {
        if messages.is_empty() {
            return;
        }

        // 日志记录：批量异步写入
        if self.log_enabled {
            if let Some(log_writer) = &self.log_writer {
                let writer = log_writer.clone();
                let msgs = messages.clone();
                tokio::spawn(async move {
                    let writer = writer.lock().await;
                    for msg in &msgs {
                        writer.write_message(msg).await;
                    }
                });
            }
        }

        let old_count = self.message_list.messages.len();
        let added = messages.len();
        let dropped = self.message_list.add_messages_batch(messages);
        let new_count = self.message_list.messages.len();
        // 命中集合可能变化：只置脏标记，重算交给事件泵限流消费（此处不做 O(N) 扫描）
        self.search_dirty = true;

        // 先补尾部、再删头部：尾部 splice 必须无条件执行，
        // 否则 GPUI 的 item_count 会与 Vec 长度错位
        if added > 0 {
            self.message_list_state.splice(old_count..old_count, added);
        }
        if dropped > 0 {
            // 不变式：淘汰只发生在自动滚动开启时，随后的 scroll_to 会覆盖锚点
            self.message_list_state.splice(0..dropped, 0);
        }

        // 批末单次滚动
        if self.auto_scroll_enabled && new_count > 0 {
            self.message_list_state.scroll_to(gpui_kit::ListOffset {
                item_ix: new_count,
                offset_in_item: px(0.),
            });
        }
    }

    pub fn disconnect(&mut self) {
        self.is_connected = false;
        self.connection_status = ConnectionStatus::Disconnected;
        self.client_connections.clear();
        self.client_list_state.reset(0);
        self.selected_client = None;

        // 关闭日志文件（同步等待 close/flush，确保日志数据不丢失）
        // 使用 block_in_place + block_on 同步执行，避免 fire-and-forget spawn 在进程退出时没机会执行
        if let Some(log_writer) = self.log_writer.take() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    // 1 秒超时，避免文件系统异常时卡住退出流程
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), async {
                        let mut writer = log_writer.lock().await;
                        writer.close().await;
                    })
                    .await;
                });
            });
        }
        self.log_enabled = false;

        // 停止服务端任务
        if let Some(handle) = &self.server_handle {
            if let Ok(mut guard) = handle.lock() {
                if let Some(join_handle) = guard.take() {
                    // 尝试取消服务端任务
                    join_handle.abort();
                    info!("[ConnectionTabState] 服务端任务已取消");
                }
            }
        }

        // 停止客户端任务
        if let Some(handle) = &self.client_handle {
            if let Ok(mut guard) = handle.lock() {
                if let Some(join_handle) = guard.take() {
                    // 尝试取消客户端任务
                    join_handle.abort();
                    info!("[ConnectionTabState] 客户端任务已取消");
                }
            }
        }

        // 停止压测引擎(协作取消 + abort 兜底)
        if let Some(engine_arc) = &self.stress_engine {
            if let Ok(mut engine_guard) = engine_arc.lock() {
                if let Some(mut engine) = engine_guard.take() {
                    engine.stop();
                    info!("[ConnectionTabState] 压测引擎已停止");
                }
            }
        }
        self.stress_engine = None;

        // 断开连接时暂停发送任务(保留, 可重连后继续), 不删除。
        // 引擎对外部断开无感(它只感知写通道失败), 这里主动暂停并附原因。
        for entry in self.send_tasks.values() {
            if let Ok(guard) = entry.engine.lock() {
                if let Some(engine) = guard.as_ref() {
                    engine.pause(Some(t!("send_task.reason_disconnected").to_string()));
                }
            }
        }
    }
}

/// 连接标签页组件
pub struct ConnectionTab<'a> {
    app: &'a NetAssistantApp,
    tab_id: String,
    tab_state: &'a ConnectionTabState,
}

impl<'a> ConnectionTab<'a> {
    pub fn new(
        app: &'a NetAssistantApp,
        tab_id: String,
        tab_state: &'a ConnectionTabState,
    ) -> Self {
        Self {
            app,
            tab_id,
            tab_state,
        }
    }

    /// 渲染通用输入框组件（支持文本/十六进制模式）
    fn render_input_with_mode(
        &self,
        input_state: &Entity<EditorState>,
        hex_editor: Option<&Entity<HexEditorState>>,
        mode: &str,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        // hex 编辑器状态同步(值变化时才重解析; 渲染层只读不更新)
        if let Some(editor) = hex_editor {
            hex_adapter::sync(editor, input_state, cx);
        }
        InputWithMode::render(input_state, hex_editor, mode, theme, window, cx)
    }

    pub fn render(
        self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        // 浮层先于根节点构建: 消费 self 前取好 target 与按钮坐标
        let variable_picker = self.render_variable_picker_overlay(&theme, cx);
        let send_task_overlay = self.render_send_task_overlay(&theme, cx);

        div()
            .flex()
            .flex_row()
            .flex_1()
            // min_h_0: 解除 min-content 钉死, 保证压测面板滚动容器拿到受限高度
            .min_h_0()
            .bg(theme.background)
            .child(self.render_connection_info(window, cx))
            .child(self.render_right_panel(window, cx))
            // 变量浮层: deferred 独立合成层, 挂在根节点避免被滚动区裁剪
            .children(variable_picker)
            // 发送任务面板: 同样挂在根节点, 锚定任务图标按钮
            .children(send_task_overlay)
    }

    /// 渲染发送任务面板浮层(展开时才返回 Some)
    fn render_send_task_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Option<AnyElement> {
        // 与变量浮层同理: 切到压测视图时发送区不可见, 不渲染锚在旧坐标的浮层
        if self.tab_state.view_mode != TabViewMode::Debug {
            return None;
        }
        SendTaskPanel::new(self.tab_id.clone(), self.tab_state).render_overlay(theme, cx)
    }

    /// 渲染「插入变量」浮层(消息区与自动回复区共用一个, 按 target 选择锚点与插入目标)
    ///
    /// 返回 None 表示当前无浮层(target 未开启, 或按钮尚未 on_prepaint 拿到坐标)。
    fn render_variable_picker_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Option<AnyElement> {
        // 浮层挂在 ConnectionTab 根节点, 不随调试内容卸载:
        // 切到压测视图时消息区/自动回复区均不可见, 若继续渲染会残留一个锚在旧坐标的浮层
        if self.tab_state.view_mode != TabViewMode::Debug {
            return None;
        }
        let target = self.tab_state.variable_picker_target?;
        let bounds = match target {
            VariablePickerTarget::Message => self.tab_state.message_var_button_bounds?,
        };

        let tab_id = self.tab_id.clone();
        // 点击面板外: 仅收起浮层
        let dismiss_entity = cx.entity().clone();
        let dismiss_tab_id = tab_id.clone();
        // 点击某行变量: 插入到对应输入框的光标处并收起
        let pick_entity = cx.entity().clone();

        Some(
            render_variable_picker(
                message_variable_items(),
                bounds,
                theme,
                Box::new(
                    move |_event: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
                        dismiss_entity.update(cx, |app, cx| {
                            if let Some(tab_state) = app.connection_tabs.get_mut(&dismiss_tab_id) {
                                tab_state.variable_picker_target = None;
                            }
                            cx.notify();
                        });
                    },
                ),
                Box::new(
                    move |item: &VariableItem, window: &mut Window, cx: &mut App| {
                        pick_entity.update(cx, |app, cx| {
                            app.insert_message_variable(&tab_id, item.insert_text, window, cx);
                        });
                    },
                ),
            )
            .into_any_element(),
        )
    }

    /// 右侧面板: 顶部 调试/压测 tab 切换 + 内容区
    fn render_right_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let is_client = self.tab_state.connection_config.is_client();
        let view_mode = self.tab_state.view_mode;
        let tab_id = self.tab_id.clone();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            // 顶部 tab 切换栏(仅客户端显示压测 tab)
            .when(is_client, |d| {
                d.child(self.render_view_mode_tabs(&theme, view_mode, tab_id.clone(), cx))
            })
            // 内容区
            .child(if view_mode == TabViewMode::Stress {
                StressPanel::new(tab_id.clone(), self.tab_state)
                    .render(window, cx)
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_message_area(window, cx))
                    .child(self.render_send_area(window, cx))
                    .into_any_element()
            })
            .into_any_element()
    }

    /// 渲染 调试/压测 视图模式 tab 切换栏
    fn render_view_mode_tabs(
        &self,
        theme: &Theme,
        view_mode: TabViewMode,
        tab_id: String,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        let is_debug = view_mode == TabViewMode::Debug;
        let is_stress = view_mode == TabViewMode::Stress;
        let tab_id_debug = tab_id.clone();
        let tab_id_stress = tab_id.clone();

        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            // 调试 tab
            .child(
                div()
                    .id("tab-debug")
                    .px_4()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_debug, |d| d.bg(theme.primary).hover(|s| s.opacity(0.9)))
                    .when(!is_debug, |d| d.hover(|s| s.bg(theme.border)))
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(if is_debug {
                                theme.primary_foreground
                            } else {
                                theme.muted_foreground
                            })
                            .child(t!("connection_tab.tab_debug").to_string()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                            app.switch_tab_view_mode(tab_id_debug.clone(), TabViewMode::Debug, cx);
                        }),
                    ),
            )
            // 压测 tab
            .child(
                div()
                    .id("tab-stress")
                    .px_4()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_stress, |d| d.bg(theme.primary).hover(|s| s.opacity(0.9)))
                    .when(!is_stress, |d| d.hover(|s| s.bg(theme.border)))
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(if is_stress {
                                theme.primary_foreground
                            } else {
                                theme.muted_foreground
                            })
                            .child(t!("connection_tab.tab_stress").to_string()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                            app.switch_tab_view_mode(
                                tab_id_stress.clone(),
                                TabViewMode::Stress,
                                cx,
                            );
                        }),
                    ),
            )
    }

    /// 渲染连接信息区域（左侧面板）
    fn render_connection_info(
        &self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab_id = self.tab_id.clone();

        let is_connected = self.tab_state.is_connected;
        let is_client = self.tab_state.connection_config.is_client();

        div()
            .flex()
            .flex_col()
            .h_full() // 显式撑满 flex row 高度
            .min_w_40() // 最小宽度
            .w_1_4()   // 默认宽度为父容器的1/4
            .max_w_64() // 最大宽度
            .p_2()     // 减少内边距
            .gap_2()   // 减少元素间距
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(self.tab_state.address()),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .flex_shrink_0()
                            .when(is_connected, |div| {
                                div.bg(theme.danger)
                                    .hover(|style| style.bg(theme.danger_hover))
                            })
                            .when(!is_connected, |div| {
                                div.bg(theme.success)
                                    .hover(|style| style.bg(theme.success_hover))
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(if is_connected { theme.danger_foreground } else { theme.success_foreground })
                                    .child(if is_connected {
                                        if is_client {
                                            t!("connection_tab.disconnect").to_string()
                                        } else {
                                            t!("connection_tab.stop").to_string()
                                        }
                                    } else {
                                        if is_client {
                                            t!("connection_tab.connect").to_string()
                                        } else {
                                            t!("connection_tab.start").to_string()
                                        }
                                    }),
                            )
                            .on_mouse_down(MouseButton::Left, cx.listener({
                                let tab_id_clone = tab_id.clone();
                                move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                    app.toggle_connection(tab_id_clone.clone(), cx);
                                }
                            }))
                    )
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .child(t!("connection_tab.protocol_label").to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .child(self.tab_state.protocol().to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("connection_tab.address_label").to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.foreground)
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(self.tab_state.address()),
                            ),
                    )
                    // 实际生效的本地端点(仅客户端; 服务端监听地址本身就是本地地址, 不重复显示)
                    .when(is_client, |div_builder| {
                        div_builder.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(t!("connection_tab.local_address_label").to_string()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(theme.foreground)
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(
                                            self.tab_state
                                                .local_endpoint
                                                .clone()
                                                .unwrap_or_else(|| "—".to_string()),
                                        ),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("connection_tab.status_label").to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .min_w_0()
                                    .when(self.tab_state.is_connected, |div| {
                                        div.text_color(theme.success)
                                    })
                                    .when(!self.tab_state.is_connected, |div| {
                                        // TODO: 等待主题增加 disabled.foreground 键后迁移
                                        div.text_color(gpui_kit::rgb(0x9ca3af))
                                    })
                                    .child(format!("{}", self.tab_state.connection_status)),
                            ),
                    )
                    // 只在TCP协议下显示解码器信息
                    .when(self.tab_state.connection_config.protocol() == ConnectionType::Tcp, |div_builder| {
                        div_builder.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .child(t!("connection_tab.decoder_label").to_string()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(theme.foreground)
                                        .min_w_0()
                                        .child(self.tab_state.decoder()),
                                )
                                // 编辑解码器配置(运行中也可修改, 即时下发到在线连接)
                                .child(
                                    div()
                                        .text_xs()
                                        .px_1()
                                        .py_0()
                                        .bg(theme.primary)
                                        .text_color(theme.primary_foreground)
                                        .rounded_md()
                                        .cursor_pointer()
                                        .child(div().text_xs().font_medium().child(t!("connection_tab.edit").to_string()))
                                        .on_mouse_down(MouseButton::Left, cx.listener({
                                            let tab_id_clone = tab_id.clone();
                                            move |app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                // 打开解码器选择对话框
                                                debug!("Edit decoder clicked for tab: {}", tab_id_clone);
                                                let tab_state = app.connection_tabs.get(&tab_id_clone).unwrap();
                                                let current_config = match &tab_state.connection_config {
                                                    ConnectionConfig::Client(config) => config.decoder_config.clone(),
                                                    ConnectionConfig::Server(config) => config.decoder_config.clone(),
                                                };

                                                app.decoder_selection_dialog = Some(
                                                    DecoderSelectionDialogState::new(
                                                        tab_id_clone.clone(),
                                                        current_config,
                                                        window,
                                                        cx,
                                                    ),
                                                );
                                                // 命令式打开对话框(由 Root 管理层叠)
                                                open_decoder_selection_dialog(
                                                    cx.entity().downgrade(),
                                                    window,
                                                    cx,
                                                );
                                                cx.notify();
                                            }
                                        }))
                                ),
                        )
                    }),
            )
            // 统计信息区域 - 在极窄窗口下会自动换行并调整样式
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2() // 减少间距
                    .p_1()   // 增加内边距以提高可读性
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1() // 减少间距
                            .child(
                                div()
                                    .w_2()
                                    .h_2()
                                    .rounded_full()
                                    .bg(theme.primary),
                            )
                            .child(
                                div()
                                    .text_xs() // 使用gpui支持的最小字体
                                    .text_color(theme.muted_foreground)
                                    .child(t!("connection_tab.sent_stat", count = self.tab_state.message_list.total_sent).to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1() // 减少间距
                            .child(
                                div()
                                    .w_2()
                                    .h_2()
                                    .rounded_full()
                                    .bg(theme.green),
                            )
                            .child(
                                div()
                                    .text_xs() // 使用gpui支持的最小字体
                                    .text_color(theme.muted_foreground)
                                    .child(t!("connection_tab.received_stat", count = self.tab_state.message_list.total_received).to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1() // 减少间距
                            .child(
                                div()
                                    .w_2()
                                    .h_2()
                                    .rounded_full()
                                    .bg(gpui_kit::rgb(0x9ca3af)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("connection_tab.total_stat", count = self.tab_state.message_list.total_messages()).to_string()),
                            ),
                    )
                    // 日志记录开关
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .mt_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w_4()
                                            .h_4()
                                            .border_1()
                                            .border_color(theme.border)
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .when(self.tab_state.log_enabled, |this| {
                                                this.bg(theme.primary)
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.primary_foreground)
                                                            .font_bold()
                                                            .child("✓"),
                                                    )
                                            })
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_log = tab_id.clone();
                                                move |app, _event, _window, cx| {
                                                    app.toggle_log(tab_id_log.clone(), cx);
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("connection_tab.log_record").to_string()),
                                    )
                                    // 修改路径按钮
                                    .child(
                                        div()
                                            .cursor_pointer()
                                            .text_color(gpui_kit::rgb(0x9ca3af))
                                            .hover(|style| style.text_color(theme.muted_foreground))
                                            .child(Icon::new(CustomIconName::Pencil).size(px(12.0)))
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_path = tab_id.clone();
                                                move |app, _event, _window, cx| {
                                                    app.change_log_path(tab_id_path.clone(), cx);
                                                }
                                            })),
                                    ),
                            )
                            // 日志文件路径：可点击打开目录
                            .when(self.tab_state.log_file_path.is_some(), |this| {
                                let display_name = self.tab_state.log_file_path.as_ref().map(|path| {
                                    std::path::Path::new(path)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or(path)
                                        .to_string()
                                }).unwrap_or_default();
                                this.child(
                                    div()
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(theme.primary)
                                        .hover(|style| style.text_color(theme.primary_hover))
                                        .max_w(px(150.0))
                                        .overflow_x_hidden()
                                        .whitespace_nowrap()
                                        .child(display_name)
                                        .on_mouse_down(MouseButton::Left, cx.listener({
                                            let tab_id_dir = tab_id.clone();
                                            move |app, _event, _window, _cx| {
                                                app.open_log_directory(tab_id_dir.clone());
                                            }
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .mt_2()
                            .flex()
                            .flex_wrap() // 允许自动换行
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("connection_tab.message_mode_label").to_string()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .when(self.tab_state.message_input_mode == "text", |div| {
                                                div.bg(theme.primary)
                                                    .text_color(theme.primary_foreground)
                                                    .hover(|style| style.bg(theme.primary_hover))
                                            })
                                            .when(self.tab_state.message_input_mode != "text", |div| {
                                                div.bg(theme.secondary)
                                                    .text_color(theme.muted_foreground)
                                                    .hover(|style| style.bg(theme.border))
                                            })
                                            .rounded_md()
                                            .cursor_pointer()
                                            .child(div().text_xs().font_medium().child(t!("connection_tab.mode_text").to_string()))
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_text = tab_id.clone();
                                                move |app, _event, window, cx| {
                                                    let mut from_mode = String::new();
                                                    let updated = app.connection_tabs.get_mut(&tab_id_text).map(|tab_state| {
                                                        from_mode = tab_state.message_input_mode.clone();
                                                        tab_state.message_input_mode = String::from("text");
                                                        match &mut tab_state.connection_config {
                                                            ConnectionConfig::Client(c) => c.message_input_mode = "text".to_string(),
                                                            ConnectionConfig::Server(c) => c.message_input_mode = "text".to_string(),
                                                        }
                                                        tab_state.connection_config.clone()
                                                    });
                                                    if let Some(cfg) = updated {
                                                        app.storage.update_connection(cfg);
                                                    }
                                                    // 转换型语义: 切模式时把输入内容整体互转(hex → 文本)
                                                    app.convert_input_on_mode_switch(&tab_id_text, &from_mode, "text", window, cx);
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .when(self.tab_state.message_input_mode == "hex", |div| {
                                                div.bg(theme.primary)
                                                    .text_color(theme.primary_foreground)
                                                    .hover(|style| style.bg(theme.primary_hover))
                                            })
                                            .when(self.tab_state.message_input_mode != "hex", |div| {
                                                div.bg(theme.secondary)
                                                    .text_color(theme.muted_foreground)
                                                    .hover(|style| style.bg(theme.border))
                                            })
                                            .rounded_md()
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_medium()
                                                    .child(t!("connection_tab.mode_hex").to_string()),
                                            )
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_hex = tab_id.clone();
                                                move |app, _event, window, cx| {
                                                    let mut from_mode = String::new();
                                                    let updated = app.connection_tabs.get_mut(&tab_id_hex).map(|tab_state| {
                                                        from_mode = tab_state.message_input_mode.clone();
                                                        tab_state.message_input_mode = String::from("hex");
                                                        match &mut tab_state.connection_config {
                                                            ConnectionConfig::Client(c) => c.message_input_mode = "hex".to_string(),
                                                            ConnectionConfig::Server(c) => c.message_input_mode = "hex".to_string(),
                                                        }
                                                        tab_state.connection_config.clone()
                                                    });
                                                    if let Some(cfg) = updated {
                                                        app.storage.update_connection(cfg);
                                                    }
                                                    // 转换型语义: 切模式时把输入内容整体互转(文本 → hex)并规范化
                                                    app.convert_input_on_mode_switch(&tab_id_hex, &from_mode, "hex", window, cx);
                                                    // 切到 hex 后聚焦编辑器: 光标立刻可见, 可直接键入
                                                    if let Some(tab_state) = app.connection_tabs.get(&tab_id_hex) {
                                                        if let Some(editor) = tab_state.message_hex_editor.as_ref() {
                                                            let focus = editor.read(cx).focus.clone();
                                                            focus.focus(window, cx);
                                                        }
                                                    }
                                                    cx.notify();
                                                }
                                            })),
                                    ),
                            ),
                    ),
            )
            // 回复规则入口: 客户端与服务端都显示(F-04)。
            .child(self.render_reply_rules_section(window, cx))
            // 服务端专属: 客户端连接列表
            .when(!is_client, |this| {
                this.child(self.render_client_connections(window, cx))
            })
            // 连接相关错误信息显示
            .when(self.tab_state.error_message.is_some(), |this| {
                let error_msg = self.tab_state.error_message.as_deref().unwrap_or("");
                this.child(
                    div()
                        .mt_3()
                        .text_xs()
                        .font_medium()
                        .text_color(theme.danger)
                        .child(error_msg.to_string()),
                )
            })
    }

    /// 渲染「自动回复」入口区(客户端与服务端都可见 —— F-04)。
    ///
    /// 单行入口: 标题 + 开关 + 状态 + 「管理规则」按钮; 具体规则在弹窗里维护。
    fn render_reply_rules_section(
        &self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        // 严格按连接隔离: 计数与活跃判定都只看本连接(tab_id)的规则
        let enabled_rules = self.app.reply_rules_store.enabled_rule_count_for(&self.tab_id);
        let rules_active = self.app.reply_rules_active(&self.tab_id);
        let gate_on = self.app.reply_connection_enabled(&self.tab_id);

        // 三态文案: 总闸开且有活跃规则→已启用 · N 条规则; 总闸开但无启用规则→提示;
        // 总闸关→未启用
        let status_text = if gate_on && rules_active {
            t!("connection_tab.auto_reply_active", n = enabled_rules).to_string()
        } else if gate_on {
            t!("reply_rules.no_enabled_rule").to_string()
        } else {
            t!("connection_tab.reply_rules_off").to_string()
        };

        // 连接级开关: 决定该连接是否运行自动回复(标题即开关标签, 不再重复文案)
        let switch_entity = cx.entity().clone();
        let switch_tab_id = self.tab_id.clone();
        // 「管理规则」弹窗的所属连接: 规则严格隔离, 弹窗只展示/编辑该连接的规则
        let dialog_tab_id = self.tab_id.clone();

        div()
            .flex()
            .items_start() // 按钮始终贴首行顶部: 状态文案变长时只让左侧内容换行, 按钮不掉到下一行
            .gap_2()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                // 左侧: 标题 + 开关 + 状态; 侧栏偏窄时在内部换行, 不挤压右侧按钮
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .flex_shrink_0()
                            .child(t!("connection_tab.auto_reply").to_string()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(
                                Switch::new(format!("reply-gate-switch-{}", self.tab_id))
                                    .checked(gate_on)
                                    .with_size(Size::Small)
                                    .on_change(move |next, _window, cx| {
                                        let tab_id = switch_tab_id.clone();
                                        switch_entity.update(cx, |app, cx| {
                                            app.storage
                                                .set_reply_connection_enabled(&tab_id, *next);
                                            app.sync_reply_rules_to_network(cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .flex_shrink_0()
                            .child(status_text),
                    ),
            )
            // 「管理规则」: 打开规则管理弹窗(客户端/服务端通用)
            .child(
                div()
                    .id("manage-reply-rules-btn")
                    .flex_shrink_0()
                    .px_1p5()
                    .py_0p5()
                    .rounded_md()
                    .text_xs()
                    .font_medium()
                    .cursor_pointer()
                    .text_color(theme.primary)
                    .bg(theme.primary.opacity(0.06))
                    .hover(|this| {
                        this.text_color(gpui_kit::white()).bg(theme.primary)
                    })
                    .tooltip(|window, cx| {
                        Tooltip::new(
                            t!("connection_tab.manage_reply_rules_tooltip").to_string(),
                        )
                        .build(window, cx)
                    })
                    .child(t!("connection_tab.manage_reply_rules").to_string())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(
                            move |app: &mut NetAssistantApp,
                                  _event: &MouseDownEvent,
                                  window: &mut Window,
                                  cx: &mut Context<NetAssistantApp>| {
                                // 弹窗状态必须在这里建好: 删除的二次确认态存在该状态里,
                                // 为空时删除按钮永远只走到「未确认」分支, 点了没反应。
                                // 此处直接改字段而非 app.update, 避免在已持有租约时重入 update。
                                app.reply_rules_dialog = Some(ReplyRulesDialogState::new(
                                    dialog_tab_id.clone(),
                                ));
                                open_reply_rules_dialog(
                                    cx.entity().downgrade(),
                                    window,
                                    cx,
                                );
                                cx.notify();
                            },
                        ),
                    ),
            )
    }

    /// 渲染服务端「客户端连接」列表(仅服务端 tab 有内容)
    fn render_client_connections(
        &self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab_id = self.tab_id.clone();
        let is_connected = self.tab_state.is_connected;
        let is_udp_server = self.tab_state.connection_config.protocol()
            == crate::config::connection::ConnectionType::Udp;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(t!("connection_tab.client_connections").to_string()),
                    )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!(
                                        "({})",
                                        self.tab_state.client_connections.len()
                                    )),
                            )
                            // 添加客户端按钮（仅UDP服务端显示）
                            .when(is_udp_server && is_connected, |this| {
                                let tab_id_for_add = tab_id.clone();
                                this.child(
                                    div()
                                        .id("add-client-btn")
                                        .ml_auto()
                                        .cursor_pointer()
                                        .hover(|style| style.opacity(0.7))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(t!("connection_tab.add_client_tooltip").to_string()).build(window, cx)
                                        })
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |_app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            let input = cx.new(|cx| {
                                                InputState::new(window, cx)
                                            });
                                            // 命令式打开对话框(由 Root 管理层叠)
                                            open_add_client_dialog(
                                                cx.entity().downgrade(),
                                                tab_id_for_add.clone(),
                                                input,
                                                window,
                                                cx,
                                            );
                                            cx.notify();
                                        }))
                                        .child(
                                            Icon::new(IconName::Plus).size(px(12.0)),
                                        )
                                )
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .bg(theme.background)
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                if self.tab_state.client_connections.is_empty() {
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .flex_1()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child(t!("connection_tab.no_client_connections").to_string()),
                                        )
                                        .into_any()
                                } else {
                                    let client_connections = self.tab_state.client_connections.clone();
                                    let selected_client = self.tab_state.selected_client.clone();
                                    let scrollbar_state = self.tab_state.client_list_state.clone();
                                    let app_entity = cx.entity().clone();
                                    let tab_id_for_clients = tab_id.clone();

                                    div()
                                        .relative()
                                        .w_full()
                                        .flex_1()
                                        .child(
                                            div()
                                                .pr_1()
                                                .pl_1()
                                                .size_full()
                                                .child(
                                                    list(
                                                        self.tab_state.client_list_state.clone(),
                                                        move |ix, _window, _cx| {
                                                            let addr = match client_connections.get(ix) {
                                                                Some(a) => a.clone(),
                                                                None => return div().into_any(),
                                                            };
                                                            let is_selected = Some(&addr) == selected_client.as_ref();
                                                            let addr_for_click = addr.clone();
                                                            let tab_id_clone = tab_id_for_clients.clone();
                                                            let entity = app_entity.clone();

                                                            div()
                                                                .id(ElementId::named_usize("client-item", ix))
                                                                .w_full()
                                                                .py_1()
                                                                .on_mouse_down(
                                                                    MouseButton::Left,
                                                                    move |_event: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
                                                                        entity.update(cx, |app, cx| {
                                                                            if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_clone) {
                                                                                // 切换选中状态：如果已经选中则取消选中，否则选中
                                                                                tab_state.selected_client = if tab_state.selected_client.as_ref() == Some(&addr_for_click) {
                                                                                    None
                                                                                } else {
                                                                                    Some(addr_for_click)
                                                                                };
                                                                                // 可见集合变了，命中集合需要重算（由事件泵限流消费）
                                                                                tab_state.search_dirty = true;
                                                                                cx.notify();
                                                                            }
                                                                        });
                                                                    },
                                                                )
                                                                .child(
                                                                    div()
                                                                        .flex_1()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap_2()
                                                                        .p_2()
                                                                        .bg(if is_selected {
                                                                            theme.success
                                                                        } else {
                                                                            theme.secondary
                                                                        })
                                                                        .rounded_md()
                                                                        .hover(|style| {
                                                                            style.bg(theme.border)
                                                                        })
                                                                        .child(
                                                                            div()
                                                                                .w_2()
                                                                                .h_2()
                                                                                .rounded_full()
                                                                                .bg(theme.success),
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .text_xs()
                                                                                .text_color(theme.foreground)
                                                                                .child(addr.to_string()),
                                                                        ),
                                                                )
                                                                .into_any()
                                                        },
                                                    )
                                                    .size_full(),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .right_0()
                                                .bottom_0()
                                                .w(px(12.0))
                                                .child(
                                                    Scrollbar::vertical(&scrollbar_state)
                                                        .mode(ScrollbarMode::Always),
                                                ),
                                        )
                                        .into_any()
                                },
                            ),
                    )
    }

    /// 渲染报文记录区域（聊天样式）- 使用 GPUI list 组件
    fn render_message_area(
        &self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab_id = self.tab_id.clone();
        let is_empty = self.tab_state.message_list.messages.is_empty();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .p_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb_2()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .text_color(theme.muted_foreground)
                            .child(t!("connection_tab.message_record").to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .w_4()
                                            .h_4()
                                            .border_1()
                                            .border_color(theme.border)
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .when(self.tab_state.auto_scroll_enabled, |this| {
                                                this.bg(theme.primary)
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.primary_foreground)
                                                            .font_bold()
                                                            .child("✓"),
                                                    )
                                            })
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id = tab_id.clone();
                                                move |app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                    let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) else { return };

                                                    if tab_state.auto_scroll_enabled {
                                                        // 开 → 关：备份当前 N，置 0（不淘汰），输入框置 0 并禁用
                                                        tab_state.auto_scroll_enabled = false;
                                                        tab_state.keep_last_backup = tab_state.message_list.keep_last;
                                                        // 0 不删任何消息，无需 splice
                                                        tab_state.message_list.set_keep_last(0);
                                                        tab_state.keep_last_input.update(cx, |input, cx| {
                                                            input.set_value("0", window, cx);
                                                        });
                                                    } else {
                                                        // 关 → 开：逐字恢复备份值（含 0），立即裁剪，滚到底部
                                                        tab_state.auto_scroll_enabled = true;
                                                        let restore = tab_state.keep_last_backup;
                                                        let dropped = tab_state.message_list.set_keep_last(restore);
                                                        if dropped > 0 {
                                                            tab_state.message_list_state.splice(0..dropped, 0);
                                                            // 头部淘汰会让命中集合失效，置脏交给事件泵限流重算
                                                            tab_state.search_dirty = true;
                                                        }
                                                        tab_state.keep_last_input.update(cx, |input, cx| {
                                                            input.set_value(restore.to_string(), window, cx);
                                                        });
                                                        let new_count = tab_state.message_list.messages.len();
                                                        if new_count > 0 {
                                                            tab_state.message_list_state.scroll_to(gpui_kit::ListOffset {
                                                                item_ix: new_count,
                                                                offset_in_item: px(0.),
                                                            });
                                                        }
                                                    }
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("connection_tab.auto_scroll").to_string()),
                                    ),
                            )
                            // 保留最后 N 条（与自动滚动联动：关闭自动滚动时不淘汰，固定为 0）
                            .child({
                                let disabled = !self.tab_state.auto_scroll_enabled;
                                div()
                                    .id(format!("keep-last-{}", tab_id))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("connection_tab.keep_last").to_string()),
                                    )
                                    .child(
                                        div()
                                            .w_20()
                                            .min_w_16()
                                            .h_7()
                                            .bg(theme.secondary)
                                            .rounded_md()
                                            .border_1()
                                            .border_color(theme.border)
                                            .when(disabled, |d| d.opacity(0.5))
                                            .child(
                                                Input::new(&self.tab_state.keep_last_input)
                                                    .disabled(disabled)
                                                    .w_full()
                                                    .h_full()
                                                    .bg(theme.secondary)
                                                    .rounded_md()
                                                    .border_0()
                                                    .text_center(),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("connection_tab.keep_last_unit").to_string()),
                                    )
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(if disabled {
                                            t!("connection_tab.keep_last_disabled_tooltip")
                                                .to_string()
                                        } else {
                                            t!("connection_tab.keep_last_tooltip").to_string()
                                        })
                                        .build(window, cx)
                                    })
                            })
                            // 搜索消息：点击开合浮层（Ctrl+F 亦可唤起）
                            .child({
                                let search_open = self.tab_state.search_open;
                                let tab_id_search = tab_id.clone();
                                div()
                                    .id(format!("msg-search-{}", tab_id))
                                    .w_6()
                                    .h_6()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(2.0))
                                    .bg(if search_open {
                                        theme.primary
                                    } else {
                                        theme.secondary
                                    })
                                    .when(!is_empty, |el| {
                                        el.cursor_pointer()
                                            .hover(|style| style.bg(theme.secondary_hover))
                                    })
                                    .when(is_empty, |el| el.opacity(0.4))
                                    .child(
                                        Icon::new(IconName::Search).size(px(18.0)).text_color(
                                            if search_open {
                                                theme.primary_foreground
                                            } else {
                                                theme.secondary_foreground
                                            },
                                        ),
                                    )
                                    .tooltip(|window, cx| {
                                        Tooltip::new(
                                            t!("connection_tab.search_tooltip").to_string(),
                                        )
                                        .build(window, cx)
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |app, _event, window, cx| {
                                                app.toggle_search(&tab_id_search, window, cx);
                                            },
                                        ),
                                    )
                            })
                            // 消息显示模式切换按钮（原始/美化/压缩）
                            .child(
                                div()
                                    .id("msg-format-toggle")
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.secondary_foreground)
                                    .bg(theme.secondary)
                                    .border(px(1.0))
                                    .border_color(theme.secondary)
                                    .rounded(px(2.0))
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .hover(|style| {
                                        style.bg(theme.secondary_hover)
                                            .border_color(theme.secondary_hover)
                                    })
                                    .child(t!("connection_tab.display_format", label = self.tab_state.message_display_mode.label()).to_string())
                                    .tooltip(|window, cx| {
                                        Tooltip::new(t!("connection_tab.format_toggle_tooltip").to_string()).build(window, cx)
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener({
                                            let tab_id_format = tab_id.clone();
                                            move |app, _event, _window, cx| {
                                                app.toggle_message_display_mode(tab_id_format.clone(), cx);
                                            }
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.secondary_foreground)
                                    .bg(theme.secondary)
                                    .border(px(1.0))
                                    .border_color(theme.secondary)
                                    .rounded(px(2.0))
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .hover(|style| {
                                        style.bg(theme.secondary_hover)
                                            .border_color(theme.secondary_hover)
                                    })
                                    .child(t!("connection_tab.export").to_string())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener({
                                            let tab_id_export = tab_id.clone();
                                            move |app, _event, _window, cx| {
                                                app.export_messages(tab_id_export.clone(), cx);
                                            }
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.secondary_foreground)
                                    .bg(theme.secondary)
                                    .border(px(1.0))
                                    .border_color(theme.secondary)
                                    .rounded(px(2.0))
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .hover(|style| {
                                        style.bg(theme.secondary_hover)
                                            .border_color(theme.secondary_hover)
                                    })
                                    .child(t!("connection_tab.clear").to_string())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener({
                                            let tab_id_clear = tab_id.clone();
                                            move |app, _event, _window, cx| {
                                                app.connection_tabs.get_mut(&tab_id_clear).map(|tab_state| {
                                                    tab_state.message_list.clear_messages();
                                                    tab_state.message_list_state.reset(0);
                                                    // 消息清空后命中集合必然失效：收起浮层并清空匹配
                                                    tab_state.search_open = false;
                                                    tab_state.search_match_ids.clear();
                                                    tab_state.search_cursor = 0;
                                                    tab_state.search_dirty = false;
                                                });
                                                // 网络层计数一并归零(否则下一拍同步会覆盖回原值)
                                                app.reset_net_counters(&tab_id_clear);
                                                cx.notify();
                                            }
                                        }),
                                    ),
                            ),
                    ),
            )
            .child(if is_empty {
                div().flex().items_center().justify_center().flex_1().child(
                    div()
                        .text_sm()
                        .text_color(gpui_kit::rgb(0x9ca3af))
                        .child(t!("connection_tab.no_messages").to_string()),
                )
                .into_any()
            } else {
                let selected_client = self.tab_state.selected_client.clone();
                let scrollbar_state = self.tab_state.message_list_state.clone();
                let tab_id_for_list = tab_id.clone();
                let app_entity = cx.entity().clone();
                let favorited_contents = self.tab_state.favorited_contents.clone();
                let display_mode = self.tab_state.message_display_mode;

                div()
                    .relative()
                    .w_full()
                    .flex_1()
                    // 搜索浮层：非模态，悬浮在消息区右上角（不加全屏遮罩，可边看消息边搜索）
                    .when(self.tab_state.search_open, |this| {
                        let total = self.tab_state.search_match_ids.len();
                        // i 用 1-based 展示，无命中显示 0/0
                        let current = if total == 0 {
                            0
                        } else {
                            self.tab_state.search_cursor + 1
                        };
                        let has_match = total > 0;
                        let tab_id_prev = tab_id.clone();
                        let tab_id_next = tab_id.clone();
                        let tab_id_close = tab_id.clone();
                        this.child(
                            div()
                                .absolute()
                                .top_0()
                                .right(px(14.0))
                                .occlude()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .shadow_lg()
                                .bg(theme.background)
                                .border_1()
                                .border_color(theme.border)
                                .child(
                                    Icon::new(IconName::Search)
                                        .size(px(12.0))
                                        .text_color(theme.muted_foreground),
                                )
                                .child(
                                    // Small 尺寸 = 24px 高，与浮层其余控件对齐
                                    div().w_40().h_6().child(
                                        Input::new(&self.tab_state.search_input)
                                            .with_size(Size::Small),
                                    ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .when(!has_match, |el| el.opacity(0.5))
                                        .child(format!("{}/{}", current, total)),
                                )
                                .child(
                                    div()
                                        .id(format!("search-prev-{}", tab_id))
                                        .w_5()
                                        .h_5()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(2.0))
                                        .when(has_match, |el| {
                                            el.cursor_pointer()
                                                .hover(|style| style.bg(theme.secondary))
                                        })
                                        .when(!has_match, |el| el.opacity(0.4))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(
                                                t!("connection_tab.search_prev").to_string(),
                                            )
                                            .build(window, cx)
                                        })
                                        .child(
                                            Icon::new(IconName::ChevronUp)
                                                .size(px(14.0))
                                                .text_color(theme.foreground),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |app, _event, window, cx| {
                                                app.jump_to_match(&tab_id_prev, -1, window, cx);
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id(format!("search-next-{}", tab_id))
                                        .w_5()
                                        .h_5()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(2.0))
                                        .when(has_match, |el| {
                                            el.cursor_pointer()
                                                .hover(|style| style.bg(theme.secondary))
                                        })
                                        .when(!has_match, |el| el.opacity(0.4))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(
                                                t!("connection_tab.search_next").to_string(),
                                            )
                                            .build(window, cx)
                                        })
                                        .child(
                                            Icon::new(IconName::ChevronDown)
                                                .size(px(14.0))
                                                .text_color(theme.foreground),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |app, _event, window, cx| {
                                                app.jump_to_match(&tab_id_next, 1, window, cx);
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id(format!("search-close-{}", tab_id))
                                        .w_5()
                                        .h_5()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(2.0))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(theme.secondary))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(
                                                t!("connection_tab.search_close").to_string(),
                                            )
                                            .build(window, cx)
                                        })
                                        .child(
                                            Icon::new(IconName::Close)
                                                .size(px(12.0))
                                                .text_color(theme.muted_foreground),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |app, _event, window, cx| {
                                                app.close_search(&tab_id_close, window, cx);
                                            }),
                                        ),
                                ),
                        )
                    })
                    .child(
                        div()
                            .pr_8()
                            .size_full()
                            // 浮层打开时列表整体下移 40px（浮层高约 34px），避免消息被悬浮的搜索框遮住
                            .when(self.tab_state.search_open, |div| div.pt_10())
                            .child(
                                list(
                                    self.tab_state.message_list_state.clone(),
                                    move |ix, _window, cx| {
                                        // 方案 B:闭包不捕获 Arc<Vec<Message>>,否则渲染元素树会一直持有
                                        // 一个额外引用计数,使事件泵侧 Arc::make_mut 退化为整体深拷贝。
                                        // 此处(列表 prepaint 阶段)App 实体未被借用,可安全 read。
                                        let app = app_entity.read(cx);
                                        let tab = app.connection_tabs.get(&tab_id_for_list);
                                        let message =
                                            tab.and_then(|tab| tab.message_list.messages.get(ix));
                                        // 当前命中项：浮层打开时才有意义（id 而非下标，淘汰不影响）
                                        let current_match_id: Option<&str> = tab
                                            .filter(|tab| tab.search_open)
                                            .and_then(|tab| {
                                                tab.search_match_ids.get(tab.search_cursor)
                                            })
                                            .map(|id| id.as_str());
                                        if let Some(message) = message {
                                            let is_sent = message.direction == MessageDirection::Sent;
                                            let is_current_match =
                                                current_match_id == Some(message.id.as_str());
                                            // 原始内容(未格式化),收藏 key 的基准
                                            let raw_text = message.get_content_by_type();
                                            // 展示/复制按当前显示模式(惰性计算并缓存,仅可见项产生开销)
                                            let display_text: SharedString =
                                                if display_mode == MessageDisplayMode::Normal {
                                                    SharedString::from(raw_text)
                                                } else {
                                                    SharedString::from(
                                                        message.display_content(display_mode),
                                                    )
                                                };
                                            // 收藏 key 固定用原始内容,与显示模式解耦:
                                            // 切换格式化不会让星标熄灭,也不会重复收藏
                                            let favorite_key: SharedString =
                                                if display_mode == MessageDisplayMode::Normal {
                                                    display_text.clone()
                                                } else {
                                                    SharedString::from(raw_text)
                                                };
                                            let is_favorited =
                                                favorited_contents.contains(favorite_key.as_ref());
                                            let should_show = if message.source.is_none() {
                                                true
                                            } else {
                                                selected_client.as_ref().map_or(true, |selected| {
                                                    message.source.as_ref() == Some(&selected.to_string())
                                                })
                                            };

                                            if !should_show {
                                                return div().into_any();
                                            }

                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .w_full()
                                                .when(is_sent, |div| div.items_end())
                                                .when(!is_sent, |div| div.items_start())
                                                // 当前命中项：整行浅灰底（只加 bg 不加内边距，
                                                // 避免命中/未命中切换时行高变化导致列表跳动）
                                                .when(is_current_match, |div| {
                                                    div.bg(theme.muted)
                                                })
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_semibold()
                                                                .when(is_sent, |div| {
                                                                    div.text_color(theme.primary)
                                                                })
                                                                .when(!is_sent, |div| {
                                                                    div.text_color(theme.green)
                                                                })
                                                                .child(if is_sent {
                                                                    t!("connection_tab.sent_label").to_string()
                                                                } else {
                                                                    t!("connection_tab.received_label").to_string()
                                                                }),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(gpui_kit::rgb(0x9ca3af))
                                                                .child(message.timestamp.clone()),
                                                        )
                                                        .when(
                                                            message.source.is_some(),
                                                            |this_div| {
                                                                if let Some(source) = &message.source {
                                                                    let is_unexpected = message.source_unexpected;

                                                                    let source_div = div()
                                                                        .id(ElementId::named_usize("source", ix))
                                                                        .text_xs()
                                                                        .text_color(if is_unexpected {
                                                                            theme.danger
                                                                        } else {
                                                                            theme.muted_foreground
                                                                        });

                                                                    let source_div = if is_unexpected {
                                                                        source_div.tooltip(|window, cx| {
                                                                            Tooltip::new(t!("connection_tab.unexpected_reply_tooltip").to_string()).build(window, cx)
                                                                        })
                                                                    } else {
                                                                        source_div
                                                                    };

                                                                    this_div.child(
                                                                        source_div.child(format!("({})", source)),
                                                                    )
                                                                } else {
                                                                    this_div
                                                                }
                                                            },
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .w_full()
                                                        .when(!is_sent, |div| {
                                                            div.flex_row()
                                                        })
                                                        .when(is_sent, |div| {
                                                            div.flex_row_reverse()
                                                        })
                                                        .child(
                                                            div()
                                                                .max_w_3_5()
                                                                .p_3()
                                                                .rounded_md()
                                                                .when(is_sent, |div| {
                                                                    div.bg(theme.primary)
                                                                })
                                                                .when(!is_sent, |div| {
                                                                    div.bg(theme.secondary)
                                                                })
                                                                .child(
                                                                    div()
                                                                        .text_sm()
                                                                        .font_family("JetBrains Mono")
                                                                        .whitespace_normal()
                                                                        .when(is_sent, |div| {
                                                                            div.text_color(theme.primary_foreground)
                                                                        })
                                                                        .when(!is_sent, |div| {
                                                                            div.text_color(theme.foreground)
                                                                        })
                                                                        .child(display_text.clone()),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap_1()
                                                                .child(
                                                                    div()
                                                                        .opacity(0.2)
                                                                        .hover(|div| {
                                                                            div.opacity(1.0)
                                                                        })
                                                                        .child(
                                                                            Clipboard::new(ElementId::named_usize("copy-message", ix))
                                                                                .value(display_text.clone())
                                                                                .on_copied(|value, _, _| {
                                                                                    debug!("Copied message content: {}", value);
                                                                                })
                                                                        )
                                                                )
                                                                .child({
                                                                    let tab_id_fav = tab_id_for_list.clone();
                                                                    // 收藏与取消收藏都用原始内容作 key(见 favorite_key)
                                                                    let content = favorite_key.clone();
                                                                    let is_fav = is_favorited;
                                                                    let message_type = message.message_type;
                                                                    let entity = app_entity.clone();
                                                                    div()
                                                                        .id(ElementId::named_usize("fav-message", ix))
                                                                        .cursor_pointer()
                                                                        .when(!is_fav, |el| el.opacity(0.2).hover(|el| el.opacity(1.0)))
                                                                        .child(
                                                                            Icon::new(IconName::Star)
                                                                                .size(px(14.0))
                                                                                .when(is_fav, |icon| icon.text_color(theme.yellow))
                                                                        )
                                                                        .on_mouse_down(MouseButton::Left, move |_event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                                                            entity.update(cx, |app, cx| {
                                                                                if is_fav {
                                                                                    if let Some(fav) = app.storage.find_favorite_by_content(&tab_id_fav, content.as_ref()) {
                                                                                        app.storage.remove_favorite(&tab_id_fav, &fav.id);
                                                                                        if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_fav) {
                                                                                            Arc::make_mut(&mut tab_state.favorited_contents).remove(content.as_ref());
                                                                                        }
                                                                                        cx.notify();
                                                                                    }
                                                                                } else {
                                                                                    app.favorite_remark_content = Some(content.to_string());
                                                                                    app.favorite_remark_message_type = Some(message_type);
                                                                                    app.favorite_remark_tab_id = Some(tab_id_fav.clone());
                                                                                    app.favorite_remark_input.update(cx, |state, inner_cx| {
                                                                                        state.set_value("", window, inner_cx);
                                                                                    });
                                                                                    // 命令式打开对话框(由 Root 管理层叠)
                                                                                    open_favorite_remark_dialog(
                                                                                        cx.entity().downgrade(),
                                                                                        window,
                                                                                        cx,
                                                                                    );
                                                                                    cx.notify();
                                                                                }
                                                                            });
                                                                        })
                                                                }),
                                                        ),
                                                )
                                                .into_any()
                                        } else {
                                            div().into_any()
                                        }
                                    },
                                )
                                .size_full(),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(12.0))
                            .child(
                                Scrollbar::vertical(&scrollbar_state)
                                    .mode(ScrollbarMode::Always),
                            ),
                    )
                    .into_any()
            })
    }

    /// 渲染发送区域
    fn render_send_area(
        &self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab_id = self.tab_id.clone();
        let tab_id_periodic = tab_id.clone();
        let tab_id_auto_clear = tab_id.clone();
        let tab_id_send = tab_id.clone();

        let is_client = self.tab_state.connection_config.is_client();
        let selected_client = &self.tab_state.selected_client;

        div()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.background)
            .when(!is_client, |el| {
                let target_text = if let Some(addr) = selected_client {
                    t!("connection_tab.send_to", addr = addr).to_string()
                } else {
                    t!("connection_tab.send_to_all").to_string()
                };
                el.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(target_text),
                )
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        self.render_input_with_mode(
                            self.tab_state.message_input.as_ref().unwrap(),
                            self.tab_state.message_hex_editor.as_ref(),
                            &self.tab_state.message_input_mode,
                            &theme,
                            window,
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap_2()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_wrap() // 允许内部元素自动换行
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("open-file-btn")
                                    .px_3()
                                    .py_1()
                                    .bg(theme.secondary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(theme.secondary_hover)
                                    })
                                    .tooltip(|window, cx| {
                                        Tooltip::new(t!("connection_tab.open_file_tooltip").to_string()).build(window, cx)
                                    })
                                    .on_mouse_down(MouseButton::Left, cx.listener({
                                        let tab_id = tab_id.clone();
                                        move |app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            // 打开「从文件导入」对话框：选文件 → 选编码 → 确定回填发送框
                                            app.open_import_file_dialog_for_tab(tab_id.clone(), window, cx);
                                        }
                                    }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_medium()
                                            .text_color(theme.secondary_foreground)
                                            .child(t!("connection_tab.open_file").to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .bg(theme.secondary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(theme.secondary_hover)
                                    })
                                    .on_mouse_down(MouseButton::Left, cx.listener({
                                        let tab_id = tab_id.clone();
                                        move |app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            // 清空输入框内容
                                            if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                                if let Some(message_input) = &tab_state.message_input {
                                                    message_input.update(cx, |input: &mut EditorState, cx| {
                                                        input.set_value("", window, cx);
                                                    });
                                                }
                                            }
                                        }
                                    }))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(theme.secondary_foreground)
                                        .child(t!("connection_tab.clear").to_string()),
                                ),
                            )
                            .child({
                                // 「插入变量」按钮: 切换浮层显隐, on_prepaint 追踪位置供浮层锚定
                                let prepaint_entity = cx.entity().clone();
                                let prepaint_tab_id = tab_id.clone();
                                let prepaint_handler: Box<
                                    dyn Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static,
                                > = Box::new(move |bounds, _window, cx| {
                                    prepaint_entity.update(cx, |app, _| {
                                        if let Some(tab_state) =
                                            app.connection_tabs.get_mut(&prepaint_tab_id)
                                        {
                                            tab_state.message_var_button_bounds = Some(bounds);
                                        }
                                    });
                                });
                                let toggle_tab_id = tab_id.clone();
                                div()
                                    // on_prepaint 需挂在 Div 上(ElementExt 仅对 ParentElement 实现),
                                    // 必须在 .id()/.tooltip() 转为 Stateful 之前调用
                                    .on_prepaint(prepaint_handler)
                                    .id("insert-var-btn")
                                    .px_3()
                                    .py_1()
                                    .bg(theme.secondary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme.secondary_hover))
                                    .tooltip(|window, cx| {
                                        Tooltip::new(
                                            t!("connection_tab.insert_variable_tooltip").to_string(),
                                        )
                                        .build(window, cx)
                                    })
                                    .on_mouse_down(MouseButton::Left, cx.listener(
                                        move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            if let Some(tab_state) = app.connection_tabs.get_mut(&toggle_tab_id) {
                                                tab_state.variable_picker_target =
                                                    if tab_state.variable_picker_target == Some(VariablePickerTarget::Message) {
                                                        None
                                                    } else {
                                                        Some(VariablePickerTarget::Message)
                                                    };
                                            }
                                            cx.notify();
                                        },
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_medium()
                                            .text_color(theme.secondary_foreground)
                                            .child(t!("connection_tab.insert_variable").to_string()),
                                    )
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w_4()
                                            .h_4()
                                            .border_1()
                                            .border_color(theme.border)
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .when(self.tab_state.auto_clear_input, |this| {
                                                this.bg(theme.primary)
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.primary_foreground)
                                                            .font_bold()
                                                            .child("✓"),
                                                    )
                                            })
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_auto_clear = tab_id_auto_clear.clone();
                                                move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                    // 获取当前标签页的状态
                                                    let mut turned_on = false;
                                                    if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_auto_clear) {
                                                        tab_state.auto_clear_input = !tab_state.auto_clear_input;
                                                        // 互斥逻辑：勾选自动清除时禁用周期发送
                                                        if tab_state.auto_clear_input {
                                                            tab_state.periodic_send_enabled = false;
                                                            turned_on = true;
                                                        }
                                                    }
                                                    // 互斥关闭周期发送时, 同步停止并移除运行中的周期发送任务, 避免后台继续发送
                                                    if turned_on {
                                                        app.stop_periodic_task(&tab_id_auto_clear, cx);
                                                    }
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("connection_tab.auto_clear").to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w_4()
                                            .h_4()
                                            .border_1()
                                            .border_color(theme.border)
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .when(self.tab_state.periodic_send_enabled, |this| {
                                                this.bg(theme.primary)
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.primary_foreground)
                                                            .font_bold()
                                                            .child("✓"),
                                                    )
                                            })
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_periodic = tab_id_periodic.clone();
                                                move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                    // 获取当前标签页的状态
                                                    if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_periodic) {
                                                        tab_state.periodic_send_enabled = !tab_state.periodic_send_enabled;
                                                        // 互斥逻辑：勾选周期发送时禁用自动清除
                                                        if tab_state.periodic_send_enabled {
                                                            tab_state.auto_clear_input = false;
                                                        }
                                                    }
                                                    // 取消勾选: 停止并移除周期发送任务(若在跑)
                                                    let still_enabled = app
                                                        .connection_tabs
                                                        .get(&tab_id_periodic)
                                                        .map(|t| t.periodic_send_enabled)
                                                        .unwrap_or(false);
                                                    if !still_enabled {
                                                        app.stop_periodic_task(&tab_id_periodic, cx);
                                                    }
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("connection_tab.periodic_send").to_string()),
                                    )
                                    // 只有在周期发送选中时才显示时间间隔输入框
                                    .when(self.tab_state.periodic_send_enabled, |builder| {
                                        builder.child(
                                            div()
                                                .w_20()
                                                .min_w_16()
                                                .h_7()
                                                .bg(theme.secondary)
                                                .rounded_md()
                                                .border_1()
                                                .border_color(theme.border)
                                                .child(
                                                    Input::new(self.tab_state.periodic_interval_input.as_ref().unwrap())
                                                        .w_full()
                                                        .h_full()
                                                        .bg(theme.secondary)
                                                        .rounded_md()
                                                        .border_0()
                                                        .text_center(),
                                                ),
                                        )
                                    }),
                            )
                            // 「结尾」下拉: 无 / LF / CRLF 三态循环, 改动即时生效并持久化
                            .child({
                                let tab_id_trailer = tab_id.clone();
                                let label = match self.tab_state.send_trailer_setting.get() {
                                    TrailerKind::None => {
                                        t!("connection_tab.trailer_none").to_string()
                                    }
                                    TrailerKind::Lf => "LF".to_string(),
                                    TrailerKind::CrLf => "CRLF".to_string(),
                                };
                                div()
                                    .id("send-trailer-btn")
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .px_3()
                                    .py_1()
                                    .bg(theme.secondary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme.secondary_hover))
                                    .tooltip(|window, cx| {
                                        Tooltip::new(
                                            t!("connection_tab.trailer_tooltip").to_string(),
                                        )
                                        .build(window, cx)
                                    })
                                    .on_mouse_down(MouseButton::Left, cx.listener(
                                        move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_trailer) {
                                                let next = tab_state.send_trailer_setting.get().next_cycle();
                                                // 即时生效(已建立连接无需重连) + 持久化
                                                tab_state.send_trailer_setting.set(next);
                                                tab_state.connection_config.set_send_trailer(next);
                                                app.storage.update_connection(tab_state.connection_config.clone());
                                            }
                                            cx.notify();
                                        }
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.secondary_foreground)
                                            .child(format!(
                                                "{}: {}",
                                                t!("connection_tab.trailer_label"),
                                                label
                                            )),
                                    )
                                    .child(Icon::new(IconName::ChevronDown).size(px(10.0)))
                            })
                            // 发送任务: 图标按钮(状态色 + 角标) + 逐行发送任务面板
                            .child(
                                SendTaskPanel::new(tab_id.clone(), self.tab_state)
                                    .render_button(&theme, cx),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child({
                                let tab_id_fav = tab_id.clone();
                                div()
                                    .relative()
                                    .child(
                                        div()
                                            .px_3()
                                            .py_2()
                                            .bg(theme.secondary)
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|style| {
                                                style.bg(theme.secondary_hover)
                                            })
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_fav = tab_id_fav.clone();
                                                move |app: &mut NetAssistantApp, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                    app.show_favorite_list = !app.show_favorite_list;
                                                    app.favorite_list_tab_id = Some(tab_id_fav.clone());
                                                    app.favorite_list_position = Some(event.position.x);
                                                    app.favorite_list_position_y = Some(event.position.y);
                                                    app.favorite_list_search_input.update(cx, |state, inner_cx| {
                                                        state.set_value("", window, inner_cx);
                                                    });
                                                    cx.notify();
                                                }
                                            }))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(Icon::new(IconName::Star).size(px(12.0)))
                                                    .child(Icon::new(if self.app.show_favorite_list { IconName::ChevronDown } else { IconName::ChevronUp }).size(px(10.0))),
                                            )
                                    )
                            })
                            .child(
                                div()
                                    .px_4()
                                    .py_2()
                                    .bg(theme.primary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(theme.primary_hover)
                                    })
                                    .id(format!("send-btn-{}", tab_id_send))
                                    .tooltip(|window, cx| {
                                        Tooltip::new(t!("connection_tab.send_shortcut_tooltip").to_string()).build(window, cx)
                                    })
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, window, cx| {
                                        let tab_id_send = tab_id_send.clone();
                                        debug!("[发送按钮] 点击事件触发，tab_id: {}", tab_id_send);
                                        // 发送逻辑与 Ctrl+Enter 快捷键共用同一方法
                                        app.send_message_from_tab(&tab_id_send, window, cx);
                                    }))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(theme.primary_foreground)
                                            .child(t!("connection_tab.send").to_string()),
                                    ),
                            ),
                    ),
            )
    }
}
