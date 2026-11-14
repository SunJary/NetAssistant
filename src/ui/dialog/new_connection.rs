// 新建/编辑连接对话框
//
// 基于 gpui_kit::component::Dialog 实现:
// 通过 window.open_dialog 命令式打开(Root 管理对话框栈), 内容区用动态 content 模式
// 每帧从 NetAssistantApp 读取最新状态(编辑态标题、协议/消息模式/解码器 chips、
// 「更多设置」折叠区随状态刷新), 内容过高时中间滚动, 标题与底部按钮固定。

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::StyledExt;
use gpui_kit::component::Theme;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use crate::app::{MqttTlsFile, NetAssistantApp};
use crate::config::connection::{DecoderConfig, MqttVersion, Qos, TlsVerifyMode};

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
    let is_mqtt = state.new_connection_protocol == "MQTT";
    let show_advanced = state.show_connection_advanced;
    // 校验错误提示(内联红字): confirm_connection_form 校验失败时写入, 打开对话框时清空
    let form_error = state.connection_form_error.clone();

    div()
        .flex()
        .flex_col()
        .gap_4()
        .children(form_error.map(|err| {
            div()
                .text_color(theme.danger)
                .text_size(px(12.0))
                .child(err)
        }))
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
                            .text_color(gpui_kit::rgb(0x9ca3af))
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
                        .child(render_protocol_chip(app, "UDP", is_edit, theme, cx))
                        // MQTT 仅客户端语义（D-5）：服务端配置不提供该选项
                        .when(is_client, |this| {
                            this.child(render_protocol_chip(app, "MQTT", is_edit, theme, cx))
                        }),
                ),
        )
        // MQTT 基础区（Client ID / 用户名 / 密码 / KeepAlive）
        .when(is_mqtt, |this| {
            this.child(render_mqtt_base_fields(app, theme, cx))
                .child(render_mqtt_tls_fields(app, theme, cx))
        })
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
                            // 本地绑定(仅客户端; MQTT 无本地绑定语义, 不渲染)
                            .when(is_client && !is_mqtt, |this| {
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
                                                .text_color(gpui_kit::rgb(0x9ca3af))
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
                            // 解码器（MQTT PUBLISH 天然分帧, 隐藏解码器选择; 规划 D-6）
                            .when(!is_mqtt, |this| {
                                this.child(render_decoder_section(app, theme, cx))
                            })
                            // MQTT 专属更多设置（Clean Session / 遗嘱消息; 规划 §8.1）
                            .when(is_mqtt, |this| {
                                this.child(render_mqtt_advanced(app, theme, cx))
                            }),
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
            div.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                entity.update(cx, |app, cx| {
                    app.new_connection_protocol = protocol_owned.clone();
                    // 切到 MQTT 时, 若端口为空则补默认 1883(不覆盖用户已填值)
                    if protocol_owned == "MQTT" && app.port_input.read(cx).value().trim().is_empty()
                    {
                        app.port_input
                            .update(cx, |input, cx| input.set_value("1883", window, cx));
                    }
                    cx.notify();
                });
            })
        })
}

/// 渲染解码器选择区（标签 + 三个芯片）
fn render_decoder_section(app: &Entity<NetAssistantApp>, theme: &Theme, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(field_label(
            theme,
            t!("new_connection.decoder_label").to_string(),
        ))
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
        )
}

/// 渲染 MQTT 基础区（Client ID / 用户名 / 密码 / KeepAlive）
fn render_mqtt_base_fields(app: &Entity<NetAssistantApp>, theme: &Theme, cx: &App) -> Div {
    let state = app.read(cx);
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    theme,
                    t!("new_connection.mqtt_client_id_label").to_string(),
                ))
                .child(Input::new(&state.mqtt_client_id_input).cleanable(true)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    theme,
                    t!("new_connection.mqtt_username_label").to_string(),
                ))
                .child(Input::new(&state.mqtt_username_input).cleanable(true)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    theme,
                    t!("new_connection.mqtt_password_label").to_string(),
                ))
                .child(Input::new(&state.mqtt_password_input).cleanable(true)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    theme,
                    t!("new_connection.mqtt_keep_alive_label").to_string(),
                ))
                .child(Input::new(&state.mqtt_keep_alive_input).cleanable(true)),
        )
        // 协议版本：切换只影响提交内容，表单里已填的 v5 属性保留不丢
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    theme,
                    t!("new_connection.mqtt_protocol_version_label").to_string(),
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(render_mqtt_version_chip(
                            app,
                            "3.1.1",
                            MqttVersion::V311,
                            theme,
                            cx,
                        ))
                        .child(render_mqtt_version_chip(
                            app,
                            "5.0",
                            MqttVersion::V5,
                            theme,
                            cx,
                        )),
                ),
        )
}

/// 渲染 MQTT 协议版本芯片
fn render_mqtt_version_chip(
    app: &Entity<NetAssistantApp>,
    label: &str,
    version: MqttVersion,
    theme: &Theme,
    cx: &App,
) -> Div {
    let is_selected = app.read(cx).edit_mqtt_version == version;
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .when(is_selected, |div| {
            div.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!is_selected, |div| {
            div.bg(theme.border).text_color(theme.foreground)
        })
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                app.edit_mqtt_version = version;
                cx.notify();
            });
        })
}

/// 渲染 MQTT TLS 分区（开关 / 校验模式 / 证书路径 / ALPN）
fn render_mqtt_tls_fields(app: &Entity<NetAssistantApp>, theme: &Theme, cx: &App) -> Div {
    let state = app.read(cx);
    let enabled = state.edit_mqtt_tls_enabled;
    let verify = state.edit_mqtt_tls_verify;

    let app_toggle = app.clone();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            Checkbox::new("mqtt-tls-enable")
                .label(t!("new_connection.mqtt_tls_label").to_string())
                .checked(enabled)
                .on_click(move |checked: &bool, window, cx| {
                    let checked = *checked;
                    app_toggle.update(cx, |app, cx| {
                        app.edit_mqtt_tls_enabled = checked;
                        // 端口联动：只在仍是默认端口时切换，不覆盖用户自定义值
                        let port = app.port_input.read(cx).value().trim().to_string();
                        let target = if checked && port == "1883" {
                            Some("8883")
                        } else if !checked && port == "8883" {
                            Some("1883")
                        } else {
                            None
                        };
                        if let Some(target) = target {
                            app.port_input
                                .update(cx, |input, cx| input.set_value(target, window, cx));
                        }
                        cx.notify();
                    });
                }),
        )
        .when(enabled, |this| {
            this.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .pl_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label(
                                theme,
                                t!("new_connection.mqtt_tls_verify_label").to_string(),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(render_tls_verify_chip(
                                        app,
                                        &t!("new_connection.mqtt_tls_verify_system"),
                                        TlsVerifyMode::SystemRoots,
                                        theme,
                                        cx,
                                        verify,
                                    ))
                                    .child(render_tls_verify_chip(
                                        app,
                                        &t!("new_connection.mqtt_tls_verify_custom_ca"),
                                        TlsVerifyMode::CustomCa,
                                        theme,
                                        cx,
                                        verify,
                                    ))
                                    .child(render_tls_verify_chip(
                                        app,
                                        &t!("new_connection.mqtt_tls_verify_insecure"),
                                        TlsVerifyMode::Insecure,
                                        theme,
                                        cx,
                                        verify,
                                    )),
                            ),
                    )
                    // 自定义 CA 模式才需要填 CA 路径
                    .when(verify == TlsVerifyMode::CustomCa, |this| {
                        this.child(render_tls_file_field(
                            app,
                            theme,
                            "mqtt-tls-pick-ca",
                            t!("new_connection.mqtt_tls_ca_label").to_string(),
                            &state.mqtt_tls_ca_input,
                            MqttTlsFile::Ca,
                        ))
                    })
                    .child(render_tls_file_field(
                        app,
                        theme,
                        "mqtt-tls-pick-cert",
                        t!("new_connection.mqtt_tls_client_cert_label").to_string(),
                        &state.mqtt_tls_cert_input,
                        MqttTlsFile::ClientCert,
                    ))
                    .child(render_tls_file_field(
                        app,
                        theme,
                        "mqtt-tls-pick-key",
                        t!("new_connection.mqtt_tls_client_key_label").to_string(),
                        &state.mqtt_tls_key_input,
                        MqttTlsFile::ClientKey,
                    ))
                    // 跳过校验的安全提示（不阻塞保存）
                    .when(verify == TlsVerifyMode::Insecure, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.warning)
                                .child(t!("new_connection.mqtt_tls_insecure_hint").to_string()),
                        )
                    })
                    // SNI 限制提示：rumqttc 用地址作为证书校验名
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui_kit::rgb(0x9ca3af))
                            .child(t!("new_connection.mqtt_tls_sni_hint").to_string()),
                    ),
            )
        })
}

/// 渲染 TLS 证书路径行（输入框 + 选择文件按钮）
fn render_tls_file_field(
    app: &Entity<NetAssistantApp>,
    theme: &Theme,
    button_id: &'static str,
    label: String,
    input: &Entity<InputState>,
    kind: MqttTlsFile,
) -> Div {
    let entity = app.clone();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(field_label(theme, label))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(div().flex_1().child(Input::new(input).cleanable(true)))
                .child(
                    Button::new(button_id)
                        .outline()
                        .label(t!("new_connection.mqtt_tls_pick_file").to_string())
                        .on_click(move |_, _, cx| {
                            entity.update(cx, |app, cx| app.pick_mqtt_tls_file(kind, cx));
                        }),
                ),
        )
}

/// 渲染 TLS 校验模式芯片
fn render_tls_verify_chip(
    app: &Entity<NetAssistantApp>,
    label: &str,
    mode: TlsVerifyMode,
    theme: &Theme,
    _cx: &App,
    selected: TlsVerifyMode,
) -> Div {
    let is_selected = selected == mode;
    let is_insecure = mode == TlsVerifyMode::Insecure;
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        // 「跳过校验」选中时用警示色描边，避免用户误以为与严格校验等价
        .when(is_selected && is_insecure, |div| {
            div.bg(theme.background)
                .text_color(theme.warning)
                .border_color(theme.warning)
        })
        .when(is_selected && !is_insecure, |div| {
            div.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!is_selected, |div| {
            div.bg(theme.border).text_color(theme.foreground)
        })
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                app.edit_mqtt_tls_verify = mode;
                cx.notify();
            });
        })
}

/// 渲染 MQTT 专属更多设置（Clean Session / 遗嘱消息）
fn render_mqtt_advanced(app: &Entity<NetAssistantApp>, theme: &Theme, cx: &App) -> Div {
    let state = app.read(cx);
    let will_enabled = state.mqtt_will_enabled;
    let clean_session = state.edit_mqtt_clean_session;
    let will_retain = state.mqtt_will_retain;
    let qos = state.edit_mqtt_will_qos;
    let version = state.edit_mqtt_version;

    let app_clean = app.clone();
    let app_will = app.clone();
    let app_retain = app.clone();

    div()
        .flex()
        .flex_col()
        .gap_3()
        // 会话保持
        .child(
            Checkbox::new("mqtt-clean-session")
                .label(t!("new_connection.mqtt_clean_session_label").to_string())
                .checked(clean_session)
                .on_click(move |checked: &bool, _, cx| {
                    let checked = *checked;
                    app_clean.update(cx, |app, cx| {
                        app.edit_mqtt_clean_session = checked;
                        cx.notify();
                    });
                }),
        )
        // MQTT 5 专属连接属性：切回 3.1.1 时保留输入（只是不提交）
        .when(version == MqttVersion::V5, |this| {
            this.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .pl_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label(
                                theme,
                                t!("new_connection.mqtt_session_expiry_label").to_string(),
                            ))
                            .child(Input::new(&state.mqtt_session_expiry_input).cleanable(true))
                            .child(
                                div().text_xs().text_color(gpui_kit::rgb(0x9ca3af)).child(
                                    t!("new_connection.mqtt_session_expiry_hint").to_string(),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label(
                                theme,
                                t!("new_connection.mqtt_receive_maximum_label").to_string(),
                            ))
                            .child(Input::new(&state.mqtt_receive_max_input).cleanable(true))
                            .child(
                                div().text_xs().text_color(gpui_kit::rgb(0x9ca3af)).child(
                                    t!("new_connection.mqtt_receive_maximum_hint").to_string(),
                                ),
                            ),
                    ),
            )
        })
        // 遗嘱消息
        .child(
            Checkbox::new("mqtt-will-enable")
                .label(t!("new_connection.mqtt_will_label").to_string())
                .checked(will_enabled)
                .on_click(move |checked: &bool, _, cx| {
                    let checked = *checked;
                    app_will.update(cx, |app, cx| {
                        app.mqtt_will_enabled = checked;
                        cx.notify();
                    });
                }),
        )
        .when(will_enabled, |this| {
            this.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .pl_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label(
                                theme,
                                t!("new_connection.mqtt_will_topic_label").to_string(),
                            ))
                            .child(Input::new(&state.mqtt_will_topic_input).cleanable(true)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label(
                                theme,
                                t!("new_connection.mqtt_will_payload_label").to_string(),
                            ))
                            .child(Input::new(&state.mqtt_will_payload_input).cleanable(true)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label(
                                theme,
                                t!("new_connection.mqtt_will_qos_label").to_string(),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(render_qos_chip(
                                        app,
                                        &t!("new_connection.qos_0"),
                                        Qos::AtMostOnce,
                                        theme,
                                        cx,
                                        qos,
                                    ))
                                    .child(render_qos_chip(
                                        app,
                                        &t!("new_connection.qos_1"),
                                        Qos::AtLeastOnce,
                                        theme,
                                        cx,
                                        qos,
                                    ))
                                    .child(render_qos_chip(
                                        app,
                                        &t!("new_connection.qos_2"),
                                        Qos::ExactlyOnce,
                                        theme,
                                        cx,
                                        qos,
                                    )),
                            ),
                    )
                    .child(
                        Checkbox::new("mqtt-will-retain")
                            .label(t!("new_connection.mqtt_will_retain_label").to_string())
                            .checked(will_retain)
                            .on_click(move |checked: &bool, _, cx| {
                                let checked = *checked;
                                app_retain.update(cx, |app, cx| {
                                    app.mqtt_will_retain = checked;
                                    cx.notify();
                                });
                            }),
                    ),
            )
        })
        // ALPN 属于低频配置，收进「更多设置」避免干扰主流程
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    theme,
                    t!("new_connection.mqtt_tls_alpn_label").to_string(),
                ))
                .child(Input::new(&state.mqtt_tls_alpn_input).cleanable(true))
                .child(
                    div()
                        .text_xs()
                        .text_color(gpui_kit::rgb(0x9ca3af))
                        .child(t!("new_connection.mqtt_tls_alpn_hint").to_string()),
                ),
        )
}

/// 渲染遗嘱 QoS 选择芯片
fn render_qos_chip(
    app: &Entity<NetAssistantApp>,
    label: &str,
    qos: Qos,
    theme: &Theme,
    _cx: &App,
    selected: Qos,
) -> Div {
    let is_selected = selected == qos;
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .when(is_selected, |div| {
            div.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!is_selected, |div| {
            div.bg(theme.border).text_color(theme.foreground)
        })
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                app.edit_mqtt_will_qos = qos;
                cx.notify();
            });
        })
}

/// 表单字段标签（统一样式）
fn field_label(theme: &Theme, text: String) -> Div {
    div()
        .text_sm()
        .font_semibold()
        .text_color(theme.foreground)
        .child(text)
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
