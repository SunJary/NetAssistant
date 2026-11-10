// 「复制规则到其他连接」目标选择弹窗
//
// 由规则管理弹窗的「复制到其他连接…」按钮叠开（见 reply_rules.rs）。
// 只列**已保存的 TCP/UDP 连接**并排除来源连接：规则引擎未接入 MQTT，
// 复制过去也不会生效，因此不展示。
//
// 选择结果按 connection_id 持久化：目标连接即使当前未打开，规则也会在
// 打开后立即参与求值。

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::{ConnectionConfig, ConnectionType};

use super::{dialog_content_max_height, dialog_height};

/// 复制目标选择弹窗状态（打开时创建，关闭时置 None）
pub struct ReplyRulesCopyDialogState {
    /// 来源连接 id（不展示在目标列表中）
    pub source_tab_id: String,
    /// 待复制的规则 id（来自来源连接）
    pub rule_ids: Vec<String>,
    /// 当前选中的目标连接 id
    pub target_id: Option<String>,
}

impl ReplyRulesCopyDialogState {
    pub fn new(source_tab_id: impl Into<String>, rule_ids: Vec<String>) -> Self {
        Self {
            source_tab_id: source_tab_id.into(),
            rule_ids,
            target_id: None,
        }
    }

    /// 待复制条数
    pub fn rule_count(&self) -> usize {
        self.rule_ids.len()
    }

    /// 是否已选择目标（未选择时「复制」置灰）
    pub fn can_confirm(&self) -> bool {
        self.target_id.is_some() && !self.rule_ids.is_empty()
    }
}

/// 打开目标选择弹窗（命令式，由 Root 管理层叠）
pub fn open_reply_rule_copy_dialog(
    app: WeakEntity<NetAssistantApp>,
    source_tab_id: String,
    rule_ids: Vec<String>,
    window: &mut Window,
    cx: &mut App,
) {
    if rule_ids.is_empty() {
        return;
    }
    let _ = app.update(cx, |app, cx| {
        app.reply_rules_copy_dialog = Some(ReplyRulesCopyDialogState::new(
            source_tab_id.clone(),
            rule_ids.clone(),
        ));
        cx.notify();
    });

    window.open_dialog(cx, move |dialog, window, cx| {
        dialog
            .title(t!("reply_rules_copy.title").to_string())
            .w(px(560.0))
            .max_h(dialog_height(window))
            .keyboard(false)
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.reply_rules_copy_dialog = None;
                        cx.notify();
                    });
                    true
                }
            })
            .footer(render_footer(&app, cx))
            .content({
                let app = app.clone();
                move |content, window, cx| {
                    let Some(entity) = app.upgrade() else {
                        return content;
                    };
                    content.child(
                        div().max_h(dialog_content_max_height(window)).child(
                            div()
                                .id("reply-rules-copy-scroll")
                                .overflow_y_scrollbar()
                                .child(render_body(&entity, window, cx)),
                        ),
                    )
                }
            })
    });
}

/// 渲染目标连接列表
fn render_body(app: &Entity<NetAssistantApp>, _window: &Window, cx: &App) -> Div {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let Some(dialog) = state.reply_rules_copy_dialog.as_ref() else {
        return div();
    };
    let source = dialog.source_tab_id.as_str();
    let selected_target = dialog.target_id.clone();
    let count = dialog.rule_count();

    // 目标候选：已保存的 TCP/UDP 连接，排除来源连接与 MQTT
    let mut targets: Vec<&ConnectionConfig> = state
        .storage
        .client_connections()
        .into_iter()
        .chain(state.storage.server_connections())
        .filter(|config| config.id() != source && config.protocol() != ConnectionType::Mqtt)
        .collect();
    targets.sort_by(|a, b| {
        a.protocol()
            .to_string()
            .cmp(&b.protocol().to_string())
            .then_with(|| a.address_label().cmp(&b.address_label()))
    });

    let mut body = div().flex().flex_col().gap_3().px_6().pb_4().child(
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .whitespace_normal()
            .child(t!("reply_rules_copy.desc", n = count).to_string()),
    );

    if targets.is_empty() {
        body = body.child(
            div()
                .p_4()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("reply_rules_copy.empty").to_string()),
                ),
        );
    } else {
        let mut list = div().flex().flex_col().gap_2();
        for config in targets {
            let target_id = config.id().to_string();
            let is_selected = selected_target.as_deref() == Some(target_id.as_str());
            let role = if config.is_client() {
                t!("reply_rules_copy.role_client").to_string()
            } else {
                t!("reply_rules_copy.role_server").to_string()
            };
            let existing = state.storage.rules_for_connection(&target_id).len();
            let title = match config {
                ConnectionConfig::Client(c) => {
                    format!("{} {}:{}", c.protocol, c.server_address, c.server_port)
                }
                ConnectionConfig::Server(s) => {
                    format!("{} {}:{}", s.protocol, s.listen_address, s.listen_port)
                }
            };
            let entity = app.clone();
            let click_target = target_id.clone();
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .cursor_pointer()
                    .border_color(if is_selected {
                        theme.primary
                    } else {
                        theme.border
                    })
                    .bg(if is_selected {
                        theme.primary.opacity(0.12)
                    } else {
                        theme.secondary
                    })
                    .child(
                        div()
                            .w_3()
                            .h_3()
                            .flex_none()
                            .rounded_full()
                            .border_1()
                            .border_color(if is_selected {
                                theme.primary
                            } else {
                                theme.muted_foreground
                            })
                            .when(is_selected, |d| d.bg(theme.primary)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .font_family("JetBrains Mono")
                                    .text_color(theme.foreground)
                                    .child(title),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!(
                                    "{} · {}",
                                    role,
                                    t!("reply_rules_copy.existing", n = existing)
                                ),
                            )),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        let target_id = click_target.clone();
                        entity.update(cx, |app, cx| {
                            if let Some(dialog) = app.reply_rules_copy_dialog.as_mut() {
                                dialog.target_id = Some(target_id);
                            }
                            cx.notify();
                        });
                    }),
            );
        }
        body = body.child(list);
    }

    body.child(
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .whitespace_normal()
            .child(t!("reply_rules_copy.mqtt_note").to_string()),
    )
}

/// 底部：取消 / 复制
fn render_footer(app: &WeakEntity<NetAssistantApp>, cx: &App) -> DialogFooter {
    let can_confirm = app
        .upgrade()
        .and_then(|entity| {
            entity
                .read(cx)
                .reply_rules_copy_dialog
                .as_ref()
                .map(|s| s.can_confirm())
        })
        .unwrap_or(false);
    let count = app
        .upgrade()
        .and_then(|entity| {
            entity
                .read(cx)
                .reply_rules_copy_dialog
                .as_ref()
                .map(|s| s.rule_count())
        })
        .unwrap_or(0);

    let cancel = Button::new("reply-rules-copy-cancel")
        .outline()
        .label(t!("reply_rules_copy.cancel").to_string())
        .on_click({
            let app = app.clone();
            move |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    app.reply_rules_copy_dialog = None;
                    cx.notify();
                });
                window.close_dialog(cx);
            }
        });

    let confirm = Button::new("reply-rules-copy-confirm")
        .primary()
        .label(t!("reply_rules_copy.confirm", n = count).to_string())
        .on_click({
            let app = app.clone();
            move |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    if let Some(dialog) = app.reply_rules_copy_dialog.take()
                        && let Some(target_id) = dialog.target_id.clone()
                    {
                        app.copy_reply_rules_to(
                            &dialog.source_tab_id,
                            &target_id,
                            &dialog.rule_ids,
                            cx,
                        );
                    }
                    cx.notify();
                });
                window.close_dialog(cx);
            }
        });

    let confirm = if can_confirm {
        confirm
    } else {
        confirm.disabled(true)
    };

    DialogFooter::new().child(cancel).child(confirm)
}
