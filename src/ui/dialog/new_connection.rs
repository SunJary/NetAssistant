use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::input::Input;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::DecoderConfig;

pub struct NewConnectionDialog<'a> {
    app: &'a NetAssistantApp,
}

impl<'a> NewConnectionDialog<'a> {
    pub fn new(app: &'a NetAssistantApp) -> Self {
        Self { app }
    }

    pub fn render(
        self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_edit = self.app.editing_connection_id.is_some();
        let is_client = self.app.new_connection_is_client;
        let title = if is_edit {
            t!("new_connection.edit_title").to_string()
        } else {
            t!("new_connection.new_title").to_string()
        };
        let show_advanced = self.app.show_connection_advanced;

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x80000000))
            .child(
                div()
                    .w_96()
                    .bg(theme.muted)
                    .rounded_lg()
                    .shadow_2xl()
                    .p_6()
                    // 标题
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .mb_4()
                            .text_color(theme.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            // 主机地址（必填）
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(theme.foreground)
                                            .child(t!("new_connection.host_label").to_string()),
                                    )
                                    .child(Input::new(&self.app.host_input).cleanable(true))
                                    .when(!is_client, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                // TODO: 等待主题增加 disabled.foreground 键后迁移
                                                .text_color(gpui::rgb(0x9ca3af))
                                                .child(t!("new_connection.host_hint").to_string()),
                                        )
                                    }),
                            )
                            // 端口（必填）
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(theme.foreground)
                                            .child(t!("new_connection.port_label").to_string()),
                                    )
                                    .child(Input::new(&self.app.port_input).cleanable(true)),
                            )
                            // 协议（必填，编辑模式下锁定）
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(theme.foreground)
                                            .child(t!("new_connection.protocol_label").to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(self.render_protocol_chip("TCP", cx))
                                            .child(self.render_protocol_chip("UDP", cx)),
                                    ),
                            )
                            // 更多设置折叠区
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .text_color(theme.foreground)
                                            .cursor_pointer()
                                            .child(if show_advanced {
                                                t!("new_connection.more_settings_collapse").to_string()
                                            } else {
                                                t!("new_connection.more_settings_expand").to_string()
                                            })
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                    app.show_connection_advanced = !app.show_connection_advanced;
                                                    cx.notify();
                                                }),
                                            ),
                                    )
                                    .when(show_advanced, |this| {
                                        this.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_3()
                                                .pl_2()
                                                // 消息模式（发送格式）
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_semibold()
                                                                .text_color(theme.foreground)
                                                                .child(t!("new_connection.message_mode_label").to_string()),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap_2()
                                                                .child(self.render_mode_chip("text", &t!("new_connection.mode_text"), cx))
                                                                .child(self.render_mode_chip("hex", &t!("new_connection.mode_hex"), cx)),
                                                        ),
                                                )
                                                // 解码器
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_semibold()
                                                                .text_color(theme.foreground)
                                                                .child(t!("new_connection.decoder_label").to_string()),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap_2()
                                                                .child(self.render_decoder_chip(&t!("new_connection.decoder_bytes"), DecoderConfig::Bytes, cx))
                                                                .child(self.render_decoder_chip(&t!("new_connection.decoder_line_based"), DecoderConfig::LineBased, cx))
                                                                .child(self.render_decoder_chip("JSON", DecoderConfig::Json, cx)),
                                                        ),
                                                ),
                                        )
                                    }),
                            ),
                    )
                    // 取消 / 确定
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .mt_6()
                            .child(
                                div()
                                    .flex_1()
                                    .p_2()
                                    .bg(theme.border)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.foreground)
                                            .child(t!("new_connection.cancel").to_string()),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            app.show_new_connection = false;
                                            app.editing_connection_id = None;
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .p_2()
                                    .bg(theme.primary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.primary_foreground)
                                            .child(if is_edit {
                                                t!("new_connection.save").to_string()
                                            } else {
                                                t!("new_connection.confirm").to_string()
                                            }),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            app.confirm_connection_form(window, cx);
                                        }),
                                    ),
                            ),
                    ),
            )
    }

    /// 渲染协议选择芯片（编辑模式下禁用切换）
    fn render_protocol_chip(&self, protocol: &str, cx: &mut Context<NetAssistantApp>) -> Div {
        let theme = cx.theme().clone();
        let is_edit = self.app.editing_connection_id.is_some();
        let selected = self.app.new_connection_protocol == protocol;
        let protocol_owned = protocol.to_string();
        div()
            .px_3()
            .py_1()
            .when(selected, |div| {
                div.bg(theme.primary).text_color(theme.primary_foreground)
            })
            .when(!selected, |div| {
                div.bg(theme.border).text_color(theme.foreground)
            })
            .rounded_md()
            .when(!is_edit, |div| div.cursor_pointer())
            .when(is_edit, |div| div.opacity(0.6))
            .child(div().text_sm().font_medium().child(protocol_owned.clone()))
            .when(!is_edit, |div| {
                div.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(
                        move |app: &mut NetAssistantApp,
                              _event: &MouseDownEvent,
                              _window: &mut Window,
                              cx: &mut Context<NetAssistantApp>| {
                            app.new_connection_protocol = protocol_owned.clone();
                            cx.notify();
                        },
                    ),
                )
            })
    }

    /// 渲染消息模式选择芯片
    fn render_mode_chip(&self, mode: &str, label: &str, cx: &mut Context<NetAssistantApp>) -> Div {
        let theme = cx.theme().clone();
        let selected = self.app.edit_message_input_mode == mode;
        let mode_owned = mode.to_string();
        div()
            .px_3()
            .py_1()
            .when(selected, |div| {
                div.bg(theme.primary).text_color(theme.primary_foreground)
            })
            .when(!selected, |div| {
                div.bg(theme.border).text_color(theme.foreground)
            })
            .rounded_md()
            .cursor_pointer()
            .child(div().text_sm().font_medium().child(label.to_string()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    move |app: &mut NetAssistantApp,
                          _event: &MouseDownEvent,
                          _window: &mut Window,
                          cx: &mut Context<NetAssistantApp>| {
                        app.edit_message_input_mode = mode_owned.clone();
                        cx.notify();
                    },
                ),
            )
    }

    /// 渲染解码器选择芯片
    fn render_decoder_chip(
        &self,
        label: &str,
        config: DecoderConfig,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        let theme = cx.theme().clone();
        let selected = self.app.edit_decoder_config == config;
        div()
            .px_3()
            .py_1()
            .when(selected, |div| {
                div.bg(theme.primary).text_color(theme.primary_foreground)
            })
            .when(!selected, |div| {
                div.bg(theme.border).text_color(theme.foreground)
            })
            .rounded_md()
            .cursor_pointer()
            .child(div().text_sm().font_medium().child(label.to_string()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    move |app: &mut NetAssistantApp,
                          _event: &MouseDownEvent,
                          _window: &mut Window,
                          cx: &mut Context<NetAssistantApp>| {
                        app.edit_decoder_config = config.clone();
                        cx.notify();
                    },
                ),
            )
    }
}
