use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::StyledExt;
use gpui_component::{
    v_virtual_list, VirtualListScrollHandle, input::{Input, InputState},
};
use std::rc::Rc;
use gpui::{px, size, ScrollStrategy, Size, Pixels};

use crate::app::NetAssistantApp;
use crate::config::connection::{ConnectionConfig, ConnectionStatus, ConnectionType};
use crate::message::{Message, MessageDirection, MessageListState, DisplayMode};

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex.replace(" ", "").replace("\n", "").replace("\r", "").replace("\t", "");
    let mut bytes = Vec::new();
    
    for i in (0..hex.len()).step_by(2) {
        if i + 1 < hex.len() {
            let byte_str = &hex[i..i+2];
            if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
                bytes.push(byte);
            }
        }
    }
    
    bytes
}

fn validate_hex_input(input: &str) -> bool {
    let cleaned = input.replace(" ", "").replace("\n", "").replace("\r", "").replace("\t", "");
    if cleaned.is_empty() {
        return true;
    }
    cleaned.chars().all(|c| c.is_ascii_hexdigit())
}

/// 连接标签页状态
#[derive(Clone)]
pub struct ConnectionTabState {
    pub connection_config: ConnectionConfig,
    pub connection_status: ConnectionStatus,
    pub message_list: MessageListState,
    pub is_connected: bool,
    pub error_message: Option<String>,
    pub auto_reply_enabled: bool,
    pub auto_reply_content: String,
    pub scroll_handle: VirtualListScrollHandle,
    pub item_sizes: Rc<Vec<Size<Pixels>>>,
    pub auto_scroll_enabled: bool,
}

impl ConnectionTabState {
    pub fn new(connection_config: ConnectionConfig) -> Self {
        Self {
            connection_config,
            connection_status: ConnectionStatus::Disconnected,
            message_list: MessageListState::new(),
            is_connected: false,
            error_message: None,
            auto_reply_enabled: false,
            auto_reply_content: String::new(),
            scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            auto_scroll_enabled: true,
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

    pub fn calculate_message_height(message: &Message) -> Size<Pixels> {
        let outer_gap = px(4.);
        let header_height = px(20.);
        let content_font_height = px(20.);
        let content_padding_top = px(12.);
        let content_padding_bottom = px(12.);
        
        let content_str = String::from_utf8_lossy(&message.raw_data);
        let content_lines = content_str.lines().count().max(1);
        let content_height = content_font_height * content_lines as f32;
        
        let total_height = outer_gap + header_height + content_padding_top + content_height + content_padding_bottom;
        size(px(300.), total_height)
    }

    pub fn add_message(&mut self, message: Message) {
        self.message_list.add_message(message.clone());
        let new_height = Self::calculate_message_height(&message);
        let mut sizes = self.item_sizes.as_ref().to_vec();
        sizes.push(new_height);
        self.item_sizes = Rc::new(sizes);
        
        if self.auto_scroll_enabled {
            let message_count = self.message_list.messages.len();
            if message_count > 0 {
                self.scroll_handle.scroll_to_item(message_count - 1, ScrollStrategy::Bottom);
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.is_connected = false;
        self.connection_status = ConnectionStatus::Disconnected;
    }
}

/// 连接标签页组件
pub struct ConnectionTab<'a> {
    app: &'a NetAssistantApp,
    tab_id: String,
    tab_state: &'a ConnectionTabState,
}

impl<'a> ConnectionTab<'a> {
    pub fn new(app: &'a NetAssistantApp, tab_id: String, tab_state: &'a ConnectionTabState) -> Self {
        Self { 
            app, 
            tab_id, 
            tab_state,
        }
    }

    pub fn render(self, window: &mut Window, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let is_client = self.tab_state.connection_config.is_client();
        
        div()
            .flex()
            .flex_row()
            .flex_1()
            .bg(gpui::rgb(0xffffff))
            .child(
                self.render_connection_info(window, cx),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(
                        self.render_message_area(cx),
                    )
                    .when(is_client, |div| {
                        div.child(self.render_send_area(cx))
                    }),
            )
    }

    /// 渲染连接信息区域（左侧面板）
    fn render_connection_info(&self, window: &mut Window, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let tab_id = self.tab_id.clone();
        let is_connected = self.tab_state.is_connected;
        let is_client = self.tab_state.connection_config.is_client();
        
        div()
            .flex()
            .flex_col()
            .w_64()
            .p_4()
            .gap_3()
            .border_r_1()
            .border_color(gpui::rgb(0xe5e7eb))
            .bg(gpui::rgb(0xf9fafb))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(gpui::rgb(0x111827))
                            .child(self.tab_state.name().to_string()),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_connected, |div| {
                                div.bg(gpui::rgb(0xef4444))
                                    .hover(|style| style.bg(gpui::rgb(0xdc2626)))
                            })
                            .when(!is_connected, |div| {
                                div.bg(gpui::rgb(0x22c55e))
                                    .hover(|style| style.bg(gpui::rgb(0x16a34a)))
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(gpui::rgb(0xffffff))
                                    .child(if is_connected { 
                                        if is_client { "断开" } else { "停止" } 
                                    } else { 
                                        if is_client { "连接" } else { "启动" } 
                                    }),
                            )
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                app.toggle_connection(tab_id.clone(), cx);
                            })),
                    ),
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
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w_2()
                                    .h_2()
                                    .rounded_full()
                                    .bg(gpui::rgb(0x3b82f6)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child(format!("发送: {}", self.tab_state.message_list.total_sent)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w_2()
                                    .h_2()
                                    .rounded_full()
                                    .bg(gpui::rgb(0x10b981)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child(format!("接收: {}", self.tab_state.message_list.total_received)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
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
                    ),
            )
            .when(!is_client, |this| {
                this.child(self.render_auto_reply_config(window, cx))
            })
    }

    /// 渲染自动回复配置区域
    fn render_auto_reply_config(&self, window: &mut Window, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let tab_id = self.tab_id.clone();
        let tab_id_for_toggle = tab_id.clone();
        let auto_reply_enabled = self.tab_state.auto_reply_enabled;
        
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
                            .font_semibold()
                            .text_color(gpui::rgb(0x111827))
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
                            .border_color(gpui::rgb(0xd1d5db))
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
                            .text_color(gpui::rgb(0x6b7280))
                            .child("启用自动回复"),
                    ),
            )
            .when(auto_reply_enabled, |this| {
                let tab_id_clone = tab_id.clone();
                
                if let Some(input_state) = self.app.auto_reply_inputs.get(&tab_id_clone) {
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(gpui::rgb(0x6b7280))
                                    .child("回复内容:"),
                            )
                            .child(
                                div()
                                    .h_32()
                                    .w_full()
                                    .bg(gpui::rgb(0xffffff))
                                    .rounded_md()
                                    .border_1()
                                    .border_color(gpui::rgb(0xe5e7eb))
                                    .child(
                                        Input::new(input_state)
                                            .w_full()
                                            .h_full()
                                            .p_2()
                                            .bg(gpui::rgb(0xffffff))
                                            .rounded_md()
                                            .border_0(),
                                    ),
                            ),
                    )
                } else {
                    this
                }
            })
    }

    /// 渲染报文记录区域（聊天样式）- 使用虚拟列表优化性能
    fn render_message_area(&self, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let messages = &self.tab_state.message_list.messages;
        let display_mode = self.app.display_mode;
        
        if messages.is_empty() {
            return div()
                .flex()
                .flex_col()
                .flex_1()
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
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .when(display_mode == DisplayMode::Text, |div| {
                                            div.bg(gpui::rgb(0x3b82f6))
                                                .text_color(gpui::rgb(0xffffff))
                                        })
                                        .when(display_mode != DisplayMode::Text, |div| {
                                            div.bg(gpui::rgb(0xf3f4f6))
                                                .text_color(gpui::rgb(0x6b7280))
                                        })
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(gpui::rgb(0xe5e7eb))
                                        })
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_medium()
                                                .child("文本"),
                                        )
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                            app.display_mode = DisplayMode::Text;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .when(display_mode == DisplayMode::Hex, |div| {
                                            div.bg(gpui::rgb(0x3b82f6))
                                                .text_color(gpui::rgb(0xffffff))
                                        })
                                        .when(display_mode != DisplayMode::Hex, |div| {
                                            div.bg(gpui::rgb(0xf3f4f6))
                                                .text_color(gpui::rgb(0x6b7280))
                                        })
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(gpui::rgb(0xe5e7eb))
                                        })
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_medium()
                                                .child("十六进制"),
                                        )
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                            app.display_mode = DisplayMode::Hex;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_1()
                        .child(
                            div()
                                .text_sm()
                                .text_color(gpui::rgb(0x9ca3af))
                                .child("暂无消息记录"),
                        ),
                );
        }
        
        let messages_clone = messages.clone();
        let scroll_handle = self.tab_state.scroll_handle.clone();
        let item_sizes = self.tab_state.item_sizes.clone();
        let display_mode_clone = display_mode;
        
        div()
            .flex_1()
            .flex()
            .flex_col()
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
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .when(display_mode == DisplayMode::Text, |div| {
                                        div.bg(gpui::rgb(0x3b82f6))
                                            .text_color(gpui::rgb(0xffffff))
                                    })
                                    .when(display_mode != DisplayMode::Text, |div| {
                                        div.bg(gpui::rgb(0xf3f4f6))
                                            .text_color(gpui::rgb(0x6b7280))
                                    })
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(gpui::rgb(0xe5e7eb))
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_medium()
                                            .child("文本"),
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                        app.display_mode = DisplayMode::Text;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .when(display_mode == DisplayMode::Hex, |div| {
                                        div.bg(gpui::rgb(0x3b82f6))
                                            .text_color(gpui::rgb(0xffffff))
                                    })
                                    .when(display_mode != DisplayMode::Hex, |div| {
                                        div.bg(gpui::rgb(0xf3f4f6))
                                            .text_color(gpui::rgb(0x6b7280))
                                    })
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(gpui::rgb(0xe5e7eb))
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_medium()
                                            .child("十六进制"),
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                        app.display_mode = DisplayMode::Hex;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                v_virtual_list(
                    cx.entity().clone(),
                    "message-list",
                    item_sizes,
                    move |_view, visible_range, _, cx| {
                        visible_range
                            .map(|ix| {
                                if let Some(message) = messages_clone.get(ix) {
                                    let is_sent = message.direction == MessageDirection::Sent;
                                    
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .w_full()
                                        .when(is_sent, |div| {
                                            div.items_end()
                                        })
                                        .when(!is_sent, |div| {
                                            div.items_start()
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
                                                            div.text_color(gpui::rgb(0x3b82f6))
                                                        })
                                                        .when(!is_sent, |div| {
                                                            div.text_color(gpui::rgb(0x10b981))
                                                        })
                                                        .child(if is_sent { "发送" } else { "接收" }),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(gpui::rgb(0x9ca3af))
                                                        .child(message.timestamp.clone()),
                                                )
                                                .when(message.source.is_some(), |this_div| {
                                                    if let Some(source) = &message.source {
                                                        this_div.child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(gpui::rgb(0x6b7280))
                                                                .child(format!("({})", source)),
                                                        )
                                                    } else {
                                                        this_div
                                                    }
                                                }),
                                        )
                                        .child(
                                            div()
                                                .max_w_80()
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
                                                        .when(is_sent, |div| {
                                                            div.text_color(gpui::rgb(0xffffff))
                                                        })
                                                        .when(!is_sent, |div| {
                                                            div.text_color(gpui::rgb(0x111827))
                                                        })
                                                        .child(message.get_display_content(display_mode_clone)),
                                                ),
                                        )
                                } else {
                                    div()
                                }
                            })
                            .collect()
                    },
                )
                .track_scroll(&scroll_handle)
            )
    }

    /// 渲染发送区域
    fn render_send_area(&self, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let tab_id = self.tab_id.clone();
        
        div()
            .h_48()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .border_t_1()
            .border_color(gpui::rgb(0xe5e7eb))
            .bg(gpui::rgb(0xffffff))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::rgb(0x6b7280))
                            .child("发送模式:"),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .when(self.app.message_input_mode == "text", |div| {
                                div.bg(gpui::rgb(0x3b82f6))
                                    .text_color(gpui::rgb(0xffffff))
                            })
                            .when(self.app.message_input_mode != "text", |div| {
                                div.bg(gpui::rgb(0xf3f4f6))
                                    .text_color(gpui::rgb(0x6b7280))
                            })
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| {
                                style.bg(gpui::rgb(0xe5e7eb))
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .child("文本"),
                            )
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                app.message_input_mode = String::from("text");
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .when(self.app.message_input_mode == "hex", |div| {
                                div.bg(gpui::rgb(0x3b82f6))
                                    .text_color(gpui::rgb(0xffffff))
                            })
                            .when(self.app.message_input_mode != "hex", |div| {
                                div.bg(gpui::rgb(0xf3f4f6))
                                    .text_color(gpui::rgb(0x6b7280))
                            })
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| {
                                style.bg(gpui::rgb(0xe5e7eb))
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .child("十六进制"),
                            )
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, window, cx| {
                                app.message_input_mode = String::from("hex");
                                app.sanitize_hex_input(window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .child(
                        Input::new(&self.app.message_input)
                            .w_full()
                            .h_full()
                            .p_3()
                            .bg(gpui::rgb(0xf9fafb))
                            .rounded_md()
                            .border_1()
                            .border_color(gpui::rgb(0xe5e7eb)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
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
                                            .px_3()
                                            .py_1()
                                            .bg(gpui::rgb(0xf3f4f6))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|style| {
                                                style.bg(gpui::rgb(0xe5e7eb))
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_medium()
                                                    .text_color(gpui::rgb(0x6b7280))
                                                    .child("清空"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .px_3()
                                            .py_1()
                                            .bg(gpui::rgb(0xf3f4f6))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|style| {
                                                style.bg(gpui::rgb(0xe5e7eb))
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_medium()
                                                    .text_color(gpui::rgb(0x6b7280))
                                                    .child("导出"),
                                            ),
                                    ),
                            )
                            .when(self.tab_state.error_message.is_some(), |this| {
                                let error_msg = self.tab_state.error_message.as_deref().unwrap_or("");
                                this.child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(gpui::rgb(0xef4444))
                                        .child(error_msg.to_string()),
                                )
                            }),
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
                                    .bg(gpui::rgb(0x3b82f6))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(gpui::rgb(0x2563eb))
                                    })
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, event, window, cx| {
                                        println!("[发送按钮] 点击事件触发，tab_id: {}", tab_id);
                                        let content = app.message_input.read(cx).text().to_string();
                                        println!("[发送按钮] 消息内容: '{}', 长度: {}, 模式: {}", content, content.len(), app.message_input_mode);
                                        
                                        if app.message_input_mode == "hex" {
                                            if !validate_hex_input(&content) {
                                                println!("[发送按钮] 十六进制输入包含非法字符");
                                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                                    tab_state.error_message = Some("十六进制输入包含非法字符".to_string());
                                                }
                                                cx.notify();
                                                return;
                                            }
                                            
                                            if !content.trim().is_empty() {
                                                println!("[发送按钮] 调用 send_message_bytes");
                                                let bytes = hex_to_bytes(&content);
                                                app.send_message_bytes(tab_id.clone(), bytes, content, cx);
                                                app.message_input.update(cx, |input: &mut InputState, cx| {
                                                    input.set_value("", window, cx);
                                                });
                                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                                    tab_state.error_message = None;
                                                }
                                            } else {
                                                println!("[发送按钮] 消息内容为空，不发送");
                                            }
                                        } else {
                                            if !content.is_empty() {
                                                println!("[发送按钮] 调用 send_message");
                                                app.send_message(tab_id.clone(), content, cx);
                                                app.message_input.update(cx, |input: &mut InputState, cx| {
                                                    input.set_value("", window, cx);
                                                });
                                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                                    tab_state.error_message = None;
                                                }
                                            } else {
                                                println!("[发送按钮] 消息内容为空，不发送");
                                            }
                                        }
                                    }))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(gpui::rgb(0xffffff))
                                            .child("发送"),
                                    ),
                            ),
                    ),
            )
    }
}
