use crate::ui::components::hex_editor::{adapter as hex_adapter, HexEditorState};
use crate::ui::components::input_with_mode::InputWithMode;
use crate::ui::dialog::{
    DecoderSelectionDialogState, open_add_client_dialog, open_decoder_selection_dialog,
    open_favorite_remark_dialog,
};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt};
use gpui_component::{
    Icon, IconName, Theme,
    clipboard::Clipboard,
    input::{Input, InputState},
    scroll::{Scrollbar, ScrollbarShow},
    tooltip::Tooltip,
};

use log::{debug, error, info, warn};
use rust_i18n::t;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

use crate::app::NetAssistantApp;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType};
use crate::custom_icons::CustomIconName;
use crate::log_writer::LogWriter;
use crate::message::{Message, MessageDirection, MessageDisplayMode, MessageListState};
use crate::stress::engine::StressTestEngine;
use crate::stress::{StressReport, StressStats, StressTestConfig, TabViewMode};
use crate::ui::stress_panel::StressPanel;
use crate::utils::hex::hex_to_bytes;

/// 连接标签页状态
#[derive(Clone)]
pub struct ConnectionTabState {
    pub connection_config: ConnectionConfig,
    pub connection_status: ConnectionStatus,
    pub message_list: MessageListState,
    pub is_connected: bool,
    pub error_message: Option<String>,
    pub auto_reply_enabled: bool,
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
    pub message_input: Option<Entity<InputState>>,
    /// 发送输入框（hex 模式）的十六进制编辑器状态，与 message_input 同步创建
    pub message_hex_editor: Option<Entity<HexEditorState>>,
    pub message_input_mode: String,
    pub auto_clear_input: bool,
    pub periodic_send_enabled: bool,
    pub periodic_interval_input: Option<Entity<InputState>>,
    // 使用 Arc<Mutex> 包装以支持克隆
    pub periodic_send_timer: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,

    // 服务端和客户端的控制句柄
    pub server_handle: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,
    pub client_handle: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,

    pub favorited_contents: HashSet<String>,

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
}

impl ConnectionTabState {
    pub fn new(
        connection_config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        // 从连接配置中恢复发送消息输入模式
        let message_input_mode = connection_config.message_input_mode().to_string();
        Self {
            connection_config,
            connection_status: ConnectionStatus::NotConnected,
            message_list: MessageListState::new(),
            is_connected: false,
            error_message: None,
            auto_reply_enabled: false,
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
                InputState::new(window, cx)
                    .code_editor("json")
                    .line_number(false)
                    .folding(false)
                    .multi_line(true)
                    .placeholder(t!("connection_tab.message_input_placeholder"))
            })),
            // 消息发送框: 每行 16 字节
            message_hex_editor: Some(cx.new(|cx| {
                HexEditorState::with_inline_bytes_per_row(cx, hex_adapter::INLINE_BYTES_PER_ROW_WIDE)
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
            periodic_send_timer: None,

            // 初始化服务端和客户端的控制句柄
            server_handle: None,
            client_handle: None,

            favorited_contents: HashSet::new(),

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

        // 非原始显示模式下，重算新消息的内容以保持一致的格式化
        if self.message_display_mode != MessageDisplayMode::Normal {
            let messages = std::sync::Arc::make_mut(&mut self.message_list.messages);
            if let Some(last) = messages.last_mut() {
                last.recompute_content_for_display(self.message_display_mode);
            }
        }

        // 更新 GPUI 列表状态：先移除头部丢弃项，再在尾部追加新项
        if dropped > 0 {
            self.message_list_state.splice(0..dropped, 0);
        }
        let insert_pos = old_count - dropped;
        self.message_list_state.splice(insert_pos..insert_pos, 1);

        if self.auto_scroll_enabled && new_count > 0 {
            self.message_list_state.scroll_to(gpui::ListOffset {
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
        let dropped = self.message_list.add_messages_batch(messages);
        let new_count = self.message_list.messages.len();

        // 非原始显示模式下，重算新增消息的内容
        if self.message_display_mode != MessageDisplayMode::Normal {
            let start = old_count.saturating_sub(dropped);
            let vec = std::sync::Arc::make_mut(&mut self.message_list.messages);
            for msg in vec[start..].iter_mut() {
                msg.recompute_content_for_display(self.message_display_mode);
            }
        }

        // 批量更新列表状态：先移除丢弃项，再追加新增项（一次 splice）
        if dropped > 0 {
            self.message_list_state.splice(0..dropped, 0);
        }
        let added = new_count - (old_count - dropped);
        let insert_pos = old_count - dropped;
        if added > 0 {
            self.message_list_state
                .splice(insert_pos..insert_pos, added);
        }

        // 批末单次滚动
        if self.auto_scroll_enabled && new_count > 0 {
            self.message_list_state.scroll_to(gpui::ListOffset {
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

        // 停止周期发送任务
        if let Some(timer_arc) = &self.periodic_send_timer {
            if let Ok(mut timer) = timer_arc.lock() {
                if let Some(timer_handle) = timer.take() {
                    timer_handle.abort();
                    info!("[ConnectionTabState] 周期发送任务已取消");
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
        input_state: &Entity<InputState>,
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

        div()
            .flex()
            .flex_row()
            .flex_1()
            // min_h_0: 解除 min-content 钉死, 保证压测面板滚动容器拿到受限高度
            .min_h_0()
            .bg(theme.background)
            .child(self.render_connection_info(window, cx))
            .child(self.render_right_panel(window, cx))
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
                                        div.text_color(gpui::rgb(0x9ca3af))
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
                                    .bg(gpui::rgb(0x9ca3af)),
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
                                            .text_color(gpui::rgb(0x9ca3af))
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
                                                move |app, _event, _window, cx| {
                                                    let updated = app.connection_tabs.get_mut(&tab_id_text).map(|tab_state| {
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
                                                    // 输入模式影响自动回复内容解析, 同步到网络层
                                                    app.sync_auto_reply_to_network(&tab_id_text, cx);
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
                                                    let updated = app.connection_tabs.get_mut(&tab_id_hex).map(|tab_state| {
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
                                                    app.sanitize_hex_input(&tab_id_hex, window, cx);
                                                    // 切到 hex 后聚焦编辑器: 光标立刻可见, 可直接键入
                                                    if let Some(tab_state) = app.connection_tabs.get(&tab_id_hex) {
                                                        if let Some(editor) = tab_state.message_hex_editor.as_ref() {
                                                            let focus = editor.read(cx).focus.clone();
                                                            focus.focus(window, cx);
                                                        }
                                                    }
                                                    // 输入模式影响自动回复内容解析, 同步到网络层
                                                    app.sync_auto_reply_to_network(&tab_id_hex, cx);
                                                    cx.notify();
                                                }
                                            })),
                                    ),
                            ),
                    ),
            )
            .when(!is_client, |this| {
                this.child(self.render_auto_reply_config(window, cx))
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

    /// 渲染自动回复配置区域
    fn render_auto_reply_config(
        &self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab_id = self.tab_id.clone();
        let tab_id_for_toggle = tab_id.clone();
        let auto_reply_enabled = self.tab_state.auto_reply_enabled;
        let is_connected = self.tab_state.is_connected;
        let is_udp_server = self.tab_state.connection_config.protocol()
            == crate::config::connection::ConnectionType::Udp;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_h_0() // 允许 flex 子项收缩
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
                            .child(t!("connection_tab.auto_reply").to_string()),
                    )
                    .child(
                        div()
                            .w_4()
                            .h_4()
                            .border_1()
                            .border_color(theme.border)
                            .rounded(px(4.))
                            .cursor_pointer()
                            .when(auto_reply_enabled, |this| {
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
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, window, cx| {
                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_for_toggle) {
                                    tab_state.auto_reply_enabled = !tab_state.auto_reply_enabled;
                                    if tab_state.auto_reply_enabled {
                                        app.ensure_auto_reply_input_exists(tab_id_for_toggle.clone(), window, cx);
                                    }
                                }
                                // 开关变化实时同步到网络层(网络层每条消息读取)
                                app.sync_auto_reply_to_network(&tab_id_for_toggle, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(t!("connection_tab.enable_auto_reply").to_string()),
                    ),
            )
            .when(auto_reply_enabled, |this| {

                if let Some(input_state) = self.app.auto_reply_inputs.get(&tab_id) {
                    let hex_editor = self.app.auto_reply_hex_editors.get(&tab_id);
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("connection_tab.reply_content_label").to_string()),
                            )
                            .child(
                                self.render_input_with_mode(
                                    input_state,
                                    hex_editor,
                                    &self.tab_state.message_input_mode,
                                    &theme,
                                    window,
                                    cx,
                                ),
                            ),
                    )
                } else {
                    this
                }
            })
            .child(
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
                                                        .scrollbar_show(ScrollbarShow::Always),
                                                ),
                                        )
                                        .into_any()
                                },
                            ),
                    )
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
                                                move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                    if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                                        tab_state.auto_scroll_enabled = !tab_state.auto_scroll_enabled;
                                                        cx.notify();
                                                    }
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
                                                    cx.notify();
                                                });
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
                        .text_color(gpui::rgb(0x9ca3af))
                        .child(t!("connection_tab.no_messages").to_string()),
                )
                .into_any()
            } else {
                let messages = self.tab_state.message_list.messages.clone();
                let selected_client = self.tab_state.selected_client.clone();
                let scrollbar_state = self.tab_state.message_list_state.clone();
                let tab_id_for_list = tab_id.clone();
                let app_entity = cx.entity().clone();
                let favorited_contents = self.tab_state.favorited_contents.clone();
               
                div()
                    .relative()
                    .w_full()
                    .flex_1()
                    .child(
                        div()
                            .pr_8()
                            .size_full()
                            .child(
                                list(
                                    self.tab_state.message_list_state.clone(),
                                    move |ix, _window, _cx| {
                                        if let Some(message) = messages.get(ix) {
                                            let is_sent = message.direction == MessageDirection::Sent;
                                            let is_favorited = favorited_contents.contains(message.get_content_by_type());
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
                                                                .text_color(gpui::rgb(0x9ca3af))
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
                                                                        .child(message.get_content_by_type().to_string()),
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
                                                                                .value(message.get_content_by_type().to_string())
                                                                                .on_copied(|value, _, _| {
                                                                                    debug!("Copied message content: {}", value);
                                                                                })
                                                                        )
                                                                )
                                                                .child({
                                                                    let tab_id_fav = tab_id_for_list.clone();
                                                                    let content = message.get_content_by_type().to_string();
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
                                                                                    if let Some(fav) = app.storage.find_favorite_by_content(&tab_id_fav, &content) {
                                                                                        app.storage.remove_favorite(&tab_id_fav, &fav.id);
                                                                                        if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_fav) {
                                                                                            tab_state.favorited_contents.remove(&content);
                                                                                        }
                                                                                        cx.notify();
                                                                                    }
                                                                                } else {
                                                                                    app.favorite_remark_content = Some(content.clone());
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
                                    .scrollbar_show(ScrollbarShow::Always),
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
                                                    message_input.update(cx, |input: &mut InputState, cx| {
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
                                                    if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_auto_clear) {
                                                        tab_state.auto_clear_input = !tab_state.auto_clear_input;
                                                        // 互斥逻辑：勾选自动清除时禁用周期发送
                                                        if tab_state.auto_clear_input {
                                                            tab_state.periodic_send_enabled = false;
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
                                                        } else {
                                                            // 禁用周期发送时停止定时器
                                                            if let Some(timer_arc) = tab_state.periodic_send_timer.take() {
                                                                if let Ok(mut timer) = timer_arc.lock() {
                                                                    if let Some(timer_handle) = timer.take() {
                                                                        timer_handle.abort();
                                                                    }
                                                                }
                                                            }
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
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, window, cx| {
                                        let tab_id_send = tab_id_send.clone();
                                        debug!("[发送按钮] 点击事件触发，tab_id: {}", tab_id_send);

                                        // 首先获取所有需要的值，避免后续的借用冲突
                                        let mut message_input_clone = None;
                                        let mut content = String::new();
                                        let mut message_input_mode = String::new();
                                        let mut auto_clear_input = false;
                                        let mut periodic_send_enabled = false;
                                        let mut connection_config = None;
                                        let mut interval_ms = 1000;

                                        // 获取当前标签页的状态
                                        if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_send) {
                                            // 获取消息输入内容
                                            if let Some(message_input) = &tab_state.message_input {
                                                content = message_input.read(cx).text().to_string();
                                                message_input_clone = Some(message_input.clone());
                                                debug!("[发送按钮] 消息内容: '{}', 长度: {}, 模式: {}", content, content.len(), tab_state.message_input_mode);

                                                // 读取周期发送间隔值
                                                let interval_str = if let Some(periodic_interval_input) = &tab_state.periodic_interval_input {
                                                    periodic_interval_input.read(cx).text().to_string()
                                                } else {
                                                    "1000".to_string()
                                                };
                                                interval_ms = interval_str.parse::<u32>().unwrap_or(1000);
                                                debug!("[发送按钮] 周期发送间隔: {}ms", interval_ms);

                                                // 存储其他需要的值
                                                message_input_mode = tab_state.message_input_mode.clone();
                                                auto_clear_input = tab_state.auto_clear_input;
                                                periodic_send_enabled = tab_state.periodic_send_enabled;
                                                connection_config = Some(tab_state.connection_config.clone());

                                                // 在发送前再次验证十六进制输入是否有效
                                                let is_hex_valid = if message_input_mode == "hex" {
                                                    let content = message_input.read(cx).text().to_string();
                                                    crate::utils::hex::validate_hex_input(&content)
                                                } else {
                                                    true
                                                };
                                                if !is_hex_valid {
                                                    debug!("[发送按钮] 十六进制输入格式错误，不发送");
                                                    return;
                                                }
                                            }
                                        } else {
                                            // Tab not found
                                            error!("[发送按钮] 发送失败: 标签页不存在");
                                            return;
                                        }

                                        // 检查消息内容是否为空
                                        if content.trim().is_empty() {
                                            debug!("[发送按钮] 消息内容为空，不发送");
                                            return;
                                        }

                                        // 确保获取到了所有必要的值
                                        if let Some(connection_config) = connection_config {
                                            // Check connection status before sending
                                            let can_send = if connection_config.is_client() {
                                                if let Some(tab_state) = app.connection_tabs.get(&tab_id_send) {
                                                    tab_state.is_connected
                                                } else {
                                                    false
                                                }
                                            } else {
                                                // Server mode: check if there are connected clients
                                                app.server_clients.get(&tab_id_send).map_or(false, |clients| !clients.is_empty())
                                            };

                                            if can_send {
                                                // 发送消息
                                                if message_input_mode == "hex" {
                                                    let bytes = hex_to_bytes(&content);
                                                    app.send_message_bytes(tab_id_send.clone(), bytes, content.clone());
                                                } else {
                                                    app.send_message(tab_id_send.clone(), content.clone());
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
                                                    let tab_id_periodic = tab_id_send.clone();
                                                    let content_periodic = content.clone();
                                                    let message_input_mode_periodic = message_input_mode.clone();
                                                    app.start_periodic_send(tab_id_periodic, interval_ms.into(), content_periodic, message_input_mode_periodic, cx);
                                                }

                                                // 清除错误消息
                                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_send) {
                                                    tab_state.error_message = None;
                                                }
                                            } else {
                                                // Send failed due to connection issue
                                                warn!("[发送按钮] 发送失败: 连接未建立或无客户端连接");
                                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_send) {
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
