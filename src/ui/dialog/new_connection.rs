// 新建/编辑连接对话框
//
// 基于 gpui_component::Dialog 实现:
// 通过 window.open_dialog 命令式打开(Root 管理对话框栈), 内容区用动态 content 模式
// 每帧从 NetAssistantApp 读取最新状态(编辑态标题、协议/消息模式/解码器 chips、
// 「更多设置」折叠区随状态刷新), 内容过高时中间滚动, 标题与底部按钮固定。

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::Theme;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::DialogFooter;
use gpui_component::input::Input;
use gpui_component::scroll::ScrollableElement as _;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::DecoderConfig;

use super::{dialog_content_max_height, dialog_height};

/// 打开「新建/编辑连接」对话框(命令式, 由 Root 管理层叠)
///
/// 表单状态(host/port 输入实体、编辑态、协议等)都保存在 NetAssistantApp 上,
/// 由 `open_new_connection` / `open_edit_connection` 先重置好再打开。
pub fn open_new_connection_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        // 新建/编辑双态标题与确认按钮文案(每帧读取, 跟随状态)
        let (title, ok_label) = {
            let Some(entity) = app.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let is_edit = app.editing_connection_id.is_some();
            let title = if is_edit {
                t!("new_connection.edit_title").to_string()
            } else {
                t!("new_connection.new_title").to_string()
            };
            let ok_label = if is_edit {
                t!("new_connection.save").to_string()
            } else {
                t!("new_connection.confirm").to_string()
            };
            (title, ok_label)
        };

        dialog
            .title(title)
            .w(px(384.0))
            .max_h(dialog_height(window))
            // ESC 取消 / Enter 确认: Input 对 Enter 的处理是 propagate,
            // 输入框内按键会继续传播到 Dialog 动作, 经 tests/dialog_layout.rs 验证无误触
            .keyboard(true)
            .on_ok({
                let app = app.clone();
                move |_, window, cx| {
                    app.update(cx, |app, cx| app.confirm_connection_form(window, cx))
                        .unwrap_or(false)
                }
            })
            // X 按钮 / 蒙层关闭时同步清理编辑态
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.editing_connection_id = None;
                        cx.notify();
                    });
                    true
                }
            })
            .footer(render_footer(&app, ok_label, cx))
            .content({
                let app = app.clone();
                move |content, window, cx| {
                    let Some(entity) = app.upgrade() else {
                        return content;
                    };
                    let theme = cx.theme().clone();
                    content.child(
                        // 滚动结构(经 tests/dialog_layout.rs 无头测试验证):
                        // max_h 在外层普通 div 钳制可视区, 内层滚动容器不限高让内容自然撑高,
                        // 否则被跟踪元素自身高度=可视区高度, 滚动机制认为未溢出, 滚轮不响应。
                        div().max_h(dialog_content_max_height(window)).child(
                            div()
                                .overflow_y_scrollbar()
                                .child(render_form(&entity, &theme, cx)),
                        ),
                    )
                }
            })
    });
}

/// 渲染表单主体
fn render_form(app: &Entity<NetAssistantApp>, theme: &Theme, cx: &App) -> Div {
    let state = app.read(cx);
    let is_client = state.new_connection_is_client;
    let is_edit = state.editing_connection_id.is_some();
    let show_advanced = state.show_connection_advanced;

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
                .child(Input::new(&state.host_input).cleanable(true))
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
                .child(Input::new(&state.port_input).cleanable(true)),
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
                        .child(render_protocol_chip(app, "TCP", is_edit, theme, cx))
                        .child(render_protocol_chip(app, "UDP", is_edit, theme, cx)),
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
                        .on_mouse_down(MouseButton::Left, {
                            let entity = app.clone();
                            move |_, _, cx| {
                                entity.update(cx, |app, cx| {
                                    app.show_connection_advanced = !app.show_connection_advanced;
                                    cx.notify();
                                });
                            }
                        }),
                )
                .when(show_advanced, |this| {
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .pl_2()
                            // 本地绑定(仅客户端; 留空=自动选网卡与临时端口)
                            .when(is_client, |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(theme.foreground)
                                                .child(
                                                    t!("new_connection.local_address_label")
                                                        .to_string(),
                                                ),
                                        )
                                        .child(
                                            Input::new(&state.local_address_input).cleanable(true),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(theme.foreground)
                                                .child(
                                                    t!("new_connection.local_port_label")
                                                        .to_string(),
                                                ),
                                        )
                                        .child(Input::new(&state.local_port_input).cleanable(true))
                                        // 何时需要填写的说明(样式与 host_hint 一致)
                                        .child(
                                            div()
                                                .text_xs()
                                                // TODO: 等待主题增加 disabled.foreground 键后迁移
                                                .text_color(gpui::rgb(0x9ca3af))
                                                .child(
                                                    t!("new_connection.local_bind_hint")
                                                        .to_string(),
                                                ),
                                        ),
                                )
                            })
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
                                            .child(
                                                t!("new_connection.message_mode_label").to_string(),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(render_mode_chip(
                                                app,
                                                "text",
                                                &t!("new_connection.mode_text"),
                                                theme,
                                                cx,
                                            ))
                                            .child(render_mode_chip(
                                                app,
                                                "hex",
                                                &t!("new_connection.mode_hex"),
                                                theme,
                                                cx,
                                            )),
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
                                            .child(render_decoder_chip(
                                                app,
                                                &t!("new_connection.decoder_bytes"),
                                                DecoderConfig::Bytes,
                                                theme,
                                                cx,
                                            ))
                                            .child(render_decoder_chip(
                                                app,
                                                &t!("new_connection.decoder_line_based"),
                                                DecoderConfig::LineBased,
                                                theme,
                                                cx,
                                            ))
                                            .child(render_decoder_chip(
                                                app,
                                                "JSON",
                                                DecoderConfig::Json,
                                                theme,
                                                cx,
                                            )),
                                    ),
                            ),
                    )
                }),
        )
}

/// 渲染协议选择芯片（编辑模式下禁用切换）
fn render_protocol_chip(
    app: &Entity<NetAssistantApp>,
    protocol: &str,
    is_edit: bool,
    theme: &Theme,
    cx: &App,
) -> Div {
    let selected = app.read(cx).new_connection_protocol == protocol;
    let protocol_owned = protocol.to_string();
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .when(selected, |div| {
            div.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!selected, |div| {
            div.bg(theme.border).text_color(theme.foreground)
        })
        .when(!is_edit, |div| div.cursor_pointer())
        .when(is_edit, |div| div.opacity(0.6))
        .child(div().text_sm().font_medium().child(protocol_owned.clone()))
        .when(!is_edit, |div| {
            div.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                entity.update(cx, |app, cx| {
                    app.new_connection_protocol = protocol_owned.clone();
                    cx.notify();
                });
            })
        })
}

/// 渲染消息模式选择芯片
fn render_mode_chip(
    app: &Entity<NetAssistantApp>,
    mode: &str,
    label: &str,
    theme: &Theme,
    cx: &App,
) -> Div {
    let selected = app.read(cx).edit_message_input_mode == mode;
    let mode_owned = mode.to_string();
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |div| {
            div.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!selected, |div| {
            div.bg(theme.border).text_color(theme.foreground)
        })
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                app.edit_message_input_mode = mode_owned.clone();
                cx.notify();
            });
        })
}

/// 渲染解码器选择芯片
fn render_decoder_chip(
    app: &Entity<NetAssistantApp>,
    label: &str,
    config: DecoderConfig,
    theme: &Theme,
    cx: &App,
) -> Div {
    let selected = app.read(cx).edit_decoder_config == config;
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |div| {
            div.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!selected, |div| {
            div.bg(theme.border).text_color(theme.foreground)
        })
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                app.edit_decoder_config = config.clone();
                cx.notify();
            });
        })
}

/// 渲染底部操作按钮
fn render_footer(app: &WeakEntity<NetAssistantApp>, ok_label: String, _cx: &App) -> DialogFooter {
    let app_ok = app.clone();
    let app_cancel = app.clone();
    DialogFooter::new()
        // 取消
        .child(
            Button::new("new-connection-cancel")
                .outline()
                .label(t!("new_connection.cancel").to_string())
                .on_click(move |_, window, cx| {
                    let _ = app_cancel.update(cx, |app, cx| {
                        app.editing_connection_id = None;
                        cx.notify();
                    });
                    window.close_dialog(cx);
                }),
        )
        // 确定 / 保存
        .child(
            Button::new("new-connection-ok")
                .primary()
                .label(ok_label)
                .on_click(move |_, window, cx| {
                    let confirmed = app_ok
                        .update(cx, |app, cx| app.confirm_connection_form(window, cx))
                        .unwrap_or(false);
                    if confirmed {
                        window.close_dialog(cx);
                    }
                }),
        )
}
