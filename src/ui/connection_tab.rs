use crate::ui::components::input_with_mode::InputWithMode;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui::{ElementId, Pixels, ScrollStrategy, Size, px, size};
use gpui_component::{ActiveTheme as _};
use gpui_component::PixelsExt;
use gpui_component::StyledExt;
use gpui_component::{
    Theme, VirtualListScrollHandle,
    clipboard::Clipboard,
    input::{Input, InputState},
    scroll::{ScrollableElement, Scrollbar},
    v_virtual_list,
};

use log::{debug, error, info, warn};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use textwrap::wrap;
use tokio::task::JoinHandle;

use crate::app::NetAssistantApp;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType};
use crate::message::{Message, MessageDirection, MessageListState};
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
    pub scroll_handle: VirtualListScrollHandle,
    pub item_sizes: RefCell<Rc<Vec<Size<Pixels>>>>,
    pub auto_scroll_enabled: bool,
    pub client_connections: Vec<SocketAddr>,
    pub selected_client: Option<SocketAddr>,

    // 每个标签页独立的功能
    pub message_input: Option<Entity<InputState>>,
    pub message_input_mode: String,
    pub auto_clear_input: bool,
    pub periodic_send_enabled: bool,
    pub periodic_interval_input: Option<Entity<InputState>>,
    // 使用 Arc<Mutex> 包装以支持克隆
    pub periodic_send_timer: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,

    // 服务端和客户端的控制句柄
    pub server_handle: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,
    pub client_handle: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,
}

impl ConnectionTabState {
    pub fn new(
        connection_config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        Self {
            connection_config,
            connection_status: ConnectionStatus::NotConnected,
            message_list: MessageListState::new(),
            is_connected: false,
            error_message: None,
            auto_reply_enabled: false,
            scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: RefCell::new(Rc::new(Vec::new())),
            auto_scroll_enabled: true,
            client_connections: Vec::new(),
            selected_client: None,

            // 初始化每个标签页独立的功能
            message_input: Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .placeholder("输入消息内容...")
            })),
            message_input_mode: String::from("text"),
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
        }
    }

    pub fn name(&self) -> &str {
        self.connection_config.name()
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

    pub fn add_message_with_width(&mut self, message: Message, _width: Pixels) {
        self.message_list.add_message(message);

        if self.auto_scroll_enabled {
            let message_count = self.message_list.messages.len();
            if message_count > 0 {
                self.scroll_handle
                    .scroll_to_item(message_count - 1, ScrollStrategy::Bottom);
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.is_connected = false;
        self.connection_status = ConnectionStatus::Disconnected;
        self.client_connections.clear();

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
    }

    /// 清空所有消息的高度缓存，以便在窗口大小变化时重新计算
    pub fn clear_message_height_cache(&mut self) {
        self.message_list.messages.iter_mut().for_each(|message| {
            message.message_height.set(None);
            message.bubble_width.set(None);
        });
        *self.item_sizes.borrow_mut() = Rc::new(Vec::new());
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
        mode: &str,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        InputWithMode::render(input_state, mode, theme, cx)
    }

    pub fn render(
        self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_client = self.tab_state.connection_config.is_client();

        div()
            .flex()
            .flex_row()
            .flex_1()
            .bg(theme.background)
            .child(self.render_connection_info(window, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(self.render_message_area(window, cx))
                    .when(is_client, |div| div.child(self.render_send_area(cx))),
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
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(self.tab_state.name().to_string()),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
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
                                        if is_client { "断开" } else { "停止" }
                                    } else {
                                        if is_client { "连接" } else { "启动" }
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
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child("协议:"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(gpui::rgb(0x111827))
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
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child("地址:"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(gpui::rgb(0x111827))
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
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child("状态:"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .when(self.tab_state.is_connected, |div| {
                                        div.text_color(gpui::rgb(0x22c55e))
                                    })
                                    .when(!self.tab_state.is_connected, |div| {
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
                                        .text_color(gpui::rgb(0x6b7280))
                                        .child("解码器:"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(gpui::rgb(0x111827))
                                        .child(self.tab_state.decoder()),
                                )
                                // 只在断开连接时显示编辑按钮
                                .when(!self.tab_state.is_connected, |div_builder| {
                                    div_builder.child(
                                        div()
                                            .text_xs()
                                            .px_1()
                                            .py_0()
                                            .bg(gpui::rgb(0x3b82f6))
                                            .text_color(gpui::rgb(0xffffff))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .child(div().text_xs().font_medium().child("编辑"))
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_clone = tab_id.clone();
                                                move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                    // 打开解码器选择对话框
                                                    debug!("Edit decoder clicked for tab: {}", tab_id_clone);
                                                    let tab_state = app.connection_tabs.get(&tab_id_clone).unwrap();
                                                    let current_config = match &tab_state.connection_config {
                                                        ConnectionConfig::Client(config) => config.decoder_config.clone(),
                                                        ConnectionConfig::Server(config) => config.decoder_config.clone(),
                                                    };
                                                    
                                                    app.show_decoder_selection = true;
                                                    app.decoder_selection_tab_id = Some(tab_id_clone.clone());
                                                    app.decoder_selection_config = Some(current_config);
                                                    cx.notify();
                                                }
                                            }))
                                    )
                                }),
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
                                    .bg(gpui::rgb(0x3b82f6)),
                            )
                            .child(
                                div()
                                    .text_xs() // 使用gpui支持的最小字体
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child(format!("发送: {}", self.tab_state.message_list.total_sent)),
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
                                    .bg(gpui::rgb(0x10b981)),
                            )
                            .child(
                                div()
                                    .text_xs() // 使用gpui支持的最小字体
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child(format!("接收: {}", self.tab_state.message_list.total_received)),
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
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child(format!("总计: {}", self.tab_state.message_list.total_messages())),
                            ),
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
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child("消息模式:"),
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
                                                div.bg(gpui::rgb(0x3b82f6))
                                                    .text_color(gpui::rgb(0xffffff))
                                            })
                                            .when(self.tab_state.message_input_mode != "text", |div| {
                                                div.bg(gpui::rgb(0xe5e7eb))
                                                    .text_color(gpui::rgb(0x6b7280))
                                            })
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|style| style.bg(gpui::rgb(0xd1d5db)))
                                            .child(div().text_xs().font_medium().child("文本"))
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_text = tab_id.clone();
                                                move |app, _event, _window, cx| {
                                                    app.connection_tabs.get_mut(&tab_id_text).unwrap().message_input_mode = String::from("text");
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .when(self.tab_state.message_input_mode == "hex", |div| {
                                                div.bg(gpui::rgb(0x3b82f6))
                                                    .text_color(gpui::rgb(0xffffff))
                                            })
                                            .when(self.tab_state.message_input_mode != "hex", |div| {
                                                div.bg(gpui::rgb(0xe5e7eb))
                                                    .text_color(gpui::rgb(0x6b7280))
                                            })
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|style| {
                                                style.bg(gpui::rgb(0xd1d5db))
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_medium()
                                                    .child("十六进制"),
                                            )
                                            .on_mouse_down(MouseButton::Left, cx.listener({
                                                let tab_id_hex = tab_id.clone();
                                                move |app, _event, window, cx| {
                                                    app.connection_tabs.get_mut(&tab_id_hex).unwrap().message_input_mode = String::from("hex");
                                                    app.sanitize_hex_input(window, cx);
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
                        .text_color(gpui::rgb(0xef4444))
                        .child(error_msg.to_string()),
                )
            })
    }

    /// 渲染自动回复配置区域
    fn render_auto_reply_config(
        &self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab_id = self.tab_id.clone();
        let tab_id_for_toggle = tab_id.clone();
        let auto_reply_enabled = self.tab_state.auto_reply_enabled;

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
                            .child("自动回复"),
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
                            .when(auto_reply_enabled, |this| {
                                this.bg(gpui::rgb(0x3b82f6))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(gpui::rgb(0xffffff))
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
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("启用自动回复"),
                    ),
            )
            .when(auto_reply_enabled, |this| {

                if let Some(input_state) = self.app.auto_reply_inputs.get(&tab_id) {
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("回复内容:"),
                            )
                            .child(
                                self.render_input_with_mode(input_state, &self.tab_state.message_input_mode, &theme, cx),
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
                                    .child("客户端连接"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex_1()
                            .bg(theme.background)
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .w_full()
                                    .h_full()
                                    .overflow_y_scrollbar()
                                    .child(
                                        if self.tab_state.client_connections.is_empty() {
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .h_full()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(theme.muted_foreground)
                                                        .child("暂无客户端连接"),
                                                )
                                        } else {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .p_2()
                                                .gap_1()
                                                .children(
                                                    self.tab_state.client_connections.iter().map(|addr| {
                                                        let addr_clone = addr.clone();
                                                        let tab_id_clone = tab_id.clone();
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .p_2()
                                                            .bg(if Some(addr) == self.tab_state.selected_client.as_ref() {
                                                                gpui::rgb(0x22c55e)
                                                            } else {
                                                                theme.secondary.to_rgb()
                                                            })
                                                            .rounded_md()
                                                            .hover(|style| {
                                                                style.bg(theme.border.to_rgb())
                                                            })
                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_clone) {
                                                                    // 切换选中状态：如果已经选中则取消选中，否则选中
                                                                    tab_state.selected_client = if tab_state.selected_client.as_ref() == Some(&addr_clone) {
                                                                        None
                                                                    } else {
                                                                        Some(addr_clone)
                                                                    };
                                                                    cx.notify();
                                                                }
                                                            }))
                                                            .child(
                                                                div()
                                                                    .w_2()
                                                                    .h_2()
                                                                    .rounded_full()
                                                                    .bg(gpui::rgb(0x22c55e)),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(theme.foreground)
                                                                    .child(addr.to_string()),
                                                            )
                                                    })
                                                )
                                        }
                                    ),
                            ),
                    )
            )
    }

    /// 渲染报文记录区域（聊天样式）- 使用虚拟列表优化性能
    fn render_message_area(&self, window: &mut Window, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let messages = &self.tab_state.message_list.messages;
        let _tab_id = self.tab_id.clone();

        // 根据选中的客户端查看消息
        let filtered_messages: Vec<&Message> = messages
            .iter()
            .filter(|m| {
                // 如果没有选中客户端，显示所有消息
                // 如果选中了客户端，只显示该客户端的消息
                self.tab_state
                    .selected_client
                    .as_ref()
                    .map_or(true, |selected| {
                        m.source.as_ref() == Some(&selected.to_string())
                    })
            })
            .collect();

        let is_empty = filtered_messages.is_empty();
        let tab_id = self.tab_id.clone();

        // 获取消息容器宽度（如果可用），否则使用默认宽度
        let container_width = if let Some(width) = self.app.message_container_width {
            width
        } else {
            px(800.0)
        };
        
        // 计算消息气泡的宽度（使用容器宽度的50%，更合理的比例）
        let bubble_width = container_width * 0.6;

        // 为虚拟列表计算消息的高度，使用缓存优化性能
        let item_sizes = if !is_empty {
            let cached = self.tab_state.item_sizes.borrow();
            debug!("缓存检查: cached.is_empty()={}, cached.len()={}, filtered_messages.len()={}", 
                   cached.is_empty(), cached.len(), filtered_messages.len());
            
            if !cached.is_empty() && cached.len() == filtered_messages.len() {
                debug!("使用缓存，跳过计算");
                Some(cached.clone())
            } else if !cached.is_empty() && cached.len() < filtered_messages.len() {
                debug!("增量计算: 缓存={}, 消息={}, 新增={}", cached.len(), filtered_messages.len(), filtered_messages.len() - cached.len());
                
                let cached_clone = cached.clone();
                drop(cached);
                
                let new_items: Vec<Size<Pixels>> = filtered_messages[cached_clone.len()..]
                    .iter()
                    .map(|m| {
                        let message_content = m.get_content_by_type();
                        let bubble_width_f32 = bubble_width.as_f32();
                        
                        let complete_message_height = if let Some(cached_height) = m.message_height.get() {
                            if let Some(cached_width) = m.bubble_width.get() {
                                if (cached_width - bubble_width_f32).abs() < 10.0 {
                                    px(cached_height)
                                } else {
                                    let height = self.app.text_measurement.calculate_text_height(window, &message_content, bubble_width, Some(px(14.0)));
                                    m.message_height.set(Some(height.as_f32()));
                                    m.bubble_width.set(Some(bubble_width_f32));
                                    height
                                }
                            } else {
                                let height = self.app.text_measurement.calculate_text_height(window, &message_content, bubble_width, Some(px(14.0)));
                                m.message_height.set(Some(height.as_f32()));
                                m.bubble_width.set(Some(bubble_width_f32));
                                height
                            }
                        } else {
                            let height = self.app.text_measurement.calculate_text_height(window, &message_content, bubble_width, Some(px(14.0)));
                            m.message_height.set(Some(height.as_f32()));
                            m.bubble_width.set(Some(bubble_width_f32));
                            height
                        };
                        
                        size(bubble_width, complete_message_height)
                    })
                    .collect();
                
                let mut combined = (*cached_clone).clone();
                combined.extend(new_items);
                let new_sizes = Rc::new(combined);
                
                *self.tab_state.item_sizes.borrow_mut() = new_sizes.clone();
                Some(new_sizes)
            } else {
                drop(cached);
                debug!("缓存失效，重新计算所有消息高度");
                
                let new_sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
                    filtered_messages
                        .iter()
                        .map(|m| {
                            let message_content = m.get_content_by_type();
                            let bubble_width_f32 = bubble_width.as_f32();
                            
                            let complete_message_height = if let Some(cached_height) = m.message_height.get() {
                                if let Some(cached_width) = m.bubble_width.get() {
                                    if (cached_width - bubble_width_f32).abs() < 10.0 {
                                        px(cached_height)
                                    } else {
                                        let height = self.app.text_measurement.calculate_text_height(window, &message_content, bubble_width, Some(px(14.0)));
                                        m.message_height.set(Some(height.as_f32()));
                                        m.bubble_width.set(Some(bubble_width_f32));
                                        height
                                    }
                                } else {
                                    let height = self.app.text_measurement.calculate_text_height(window, &message_content, bubble_width, Some(px(14.0)));
                                    m.message_height.set(Some(height.as_f32()));
                                    m.bubble_width.set(Some(bubble_width_f32));
                                    height
                                }
                            } else {
                                let height = self.app.text_measurement.calculate_text_height(window, &message_content, bubble_width, Some(px(14.0)));
                                m.message_height.set(Some(height.as_f32()));
                                m.bubble_width.set(Some(bubble_width_f32));
                                height
                            };
                            
                            size(bubble_width, complete_message_height)
                        })
                        .collect(),
                );
                
                *self.tab_state.item_sizes.borrow_mut() = new_sizes.clone();
                Some(new_sizes)
            }
        } else {
            None
        };

        let filtered_messages_clone: Vec<Message> =
            filtered_messages.into_iter().cloned().collect();
        let scroll_handle = self.tab_state.scroll_handle.clone();

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
                            .text_color(gpui::rgb(0x6b7280))
                            .child("消息记录"),
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
                            .child("清空")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |app, _event, _window, cx| {
                                    app.connection_tabs.get_mut(&tab_id).map(|tab_state| {
                                        tab_state.message_list.clear_messages();
                                        *tab_state.item_sizes.borrow_mut() = Rc::new(Vec::new());
                                        cx.notify();
                                    });
                                }),
                            ),
                    ),
            )
            .child(if is_empty {
                // 无消息记录时显示
                div().flex().items_center().justify_center().flex_1().child(
                    div()
                        .text_sm()
                        .text_color(gpui::rgb(0x9ca3af))
                        .child("暂无消息记录"),
                )
            } else {
                // 有消息记录时显示虚拟列表
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .h_full()
                    // 消息区域
                    .child(
                        div().flex().flex_col().flex_1().h_full().child(
                            v_virtual_list(
                                cx.entity().clone(),
                                "message-list",
                                item_sizes.unwrap(),
                                move |_view, visible_range, _, _cx| {
                                    visible_range
                                        .map(|ix| {
                                            if let Some(message) = filtered_messages_clone.get(ix) {
                                                let is_sent =
                                                    message.direction == MessageDirection::Sent;

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
                                                                        div.text_color(gpui::rgb(
                                                                            0x3b82f6,
                                                                        ))
                                                                    })
                                                                    .when(!is_sent, |div| {
                                                                        div.text_color(gpui::rgb(
                                                                            0x10b981,
                                                                        ))
                                                                    })
                                                                    .child(if is_sent {
                                                                        "发送"
                                                                    } else {
                                                                        "接收"
                                                                    }),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(gpui::rgb(0x9ca3af))
                                                                    .child(
                                                                        message.timestamp.clone(),
                                                                    ),
                                                            )
                                                            .when(
                                                                message.source.is_some(),
                                                                |this_div| {
                                                                    if let Some(source) =
                                                                        &message.source
                                                                    {
                                                                        this_div.child(
                                                                            div()
                                                                                .text_xs()
                                                                                .text_color(
                                                                                    gpui::rgb(
                                                                                        0x6b7280,
                                                                                    ),
                                                                                )
                                                                                .child(format!(
                                                                                    "({})",
                                                                                    source
                                                                                )),
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
                                                                        div.bg(gpui::rgb(0x3b82f6))
                                                                    })
                                                                    .when(!is_sent, |div| {
                                                                        div.bg(gpui::rgb(0xf3f4f6))
                                                                    })
                                                                    .child(
                                                                        div()
                                                            .text_sm()
                                                            .whitespace_normal() // 添加自动换行
                                                            .when(is_sent, |div| {
                                                                div.text_color(gpui::rgb(
                                                                    0xffffff,
                                                                ))
                                                            })
                                                            .when(!is_sent, |div| {
                                                                div.text_color(gpui::rgb(
                                                                    0x111827,
                                                                ))
                                                            })
                                                            .child(
                                                                message
                                                                    .get_content_by_type(),
                                                            ),
                                                                    ),
                                                            )
                                                            .child(
                                                                div()
                                                                    .opacity(0.2)
                                                                    .hover(|div| {
                                                                        div.opacity(1.0)
                                                                    })
                                                                    .child(
                                                                        Clipboard::new(ElementId::named_usize("copy-message", ix))
                                                                            .value(message.get_content_by_type())
                                                                            .on_copied(|value, _, _| {
                                                                                debug!("Copied message content: {}", value);
                                                                            })
                                                                    )
                                                            )
                                                    )
                                            } else {
                                                div()
                                            }
                                        })
                                        .collect()
                                },
                            )
                            .track_scroll(&scroll_handle),
                        ),
                    )
                    // 滚动条区域
                    .child(
                        div()
                            .w_6()
                            .h_full()
                            .flex()
                            .justify_center()
                            .child(Scrollbar::vertical(&scroll_handle)),
                    )
            })
    }

    /// 渲染发送区域
    fn render_send_area(&self, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab_id = self.tab_id.clone();
        let _tab_id_text = tab_id.clone();
        let _tab_id_hex = tab_id.clone();
        let tab_id_periodic = tab_id.clone();
        let tab_id_auto_clear = tab_id.clone();
        let tab_id_send = tab_id.clone();

        div()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        self.render_input_with_mode(
                            self.tab_state.message_input.as_ref().unwrap(),
                            &self.tab_state.message_input_mode,
                            &theme,
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .justify_between() // 两端对齐，发送按钮在右侧
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
                                        .child("清空"),
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
                                            .border_color(gpui::rgb(0xd1d5db))
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .when(self.tab_state.auto_clear_input, |this| {
                                                this.bg(gpui::rgb(0x3b82f6))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(gpui::rgb(0xffffff))
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
                                            .text_color(gpui::rgb(0x6b7280))
                                            .child("自动清空"),
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
                                            .border_color(gpui::rgb(0xd1d5db))
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .when(self.tab_state.periodic_send_enabled, |this| {
                                                this.bg(gpui::rgb(0x3b82f6))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(gpui::rgb(0xffffff))
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
                                            .text_color(gpui::rgb(0x6b7280))
                                            .child("周期发送"),
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
                                                        "连接未建立".to_string()
                                                    } else {
                                                        "无客户端连接".to_string()
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
                                            .child("发送"),
                                    ),
                            ),
                    ),
            )
    }
}
