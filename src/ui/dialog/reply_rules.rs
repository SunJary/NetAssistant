// 「回复规则」管理弹窗
//
// 入口: 连接标签页发送区的「管理规则…」按钮(客户端/服务端都可见)。
// 职责: 规则集的**总览与批量操作** —— 总开关、逐条启用/排序/复制/删除/清零。
// 单条规则的具体编辑(条件树、动作、内联试跑)在 `reply_rule_edit.rs`。
//
// 打开/关闭沿用 gpui_component 命令式对话框惯例(见 timed_task.rs / stress_config.rs):
// 状态挂在 `app.reply_rules_dialog`, 内容闭包每帧从 app 读取最新状态。
//
// **列表虚拟化的取舍**: 规则数量级是"人手工维护的协议知识"(现实中数十条),
// 而不是消息明细那样的万级流数据; 因此本轮用普通 `overflow_y_scrollbar` 全量渲染,
// 不引入 `list()` 虚拟化。若将来规则数稳定超过 ~200 条再改为虚拟化。
//
// 滚动结构遵循 `dialog_content_max_height` 的约定:
// 外层普通 div `max_h` 钳制可视区, 内层 div 挂 `overflow_y_scrollbar` 且自身不限高。

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::Theme;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, Size};

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::reply::model::{
    ReplyRule, Severity, format_matcher_summary, format_reply_summary, validate_rules_config,
};

use super::{dialog_content_max_height, dialog_height, open_reply_rule_edit_dialog};

/// 「回复规则」管理弹窗状态(打开时创建, 关闭时由 app 置 None)
pub struct ReplyRulesDialogState {
    /// 已点击过一次「删除」、等待二次确认的规则 id
    ///
    /// 用"按钮自身变确认态"而不是再开一个确认弹窗: 弹窗栈里再叠弹窗会让
    /// 关闭语义(close_dialog 关最上层 vs 关全部)变得难以推理, 而破坏性动作的
    /// 二次确认只需要一次明确的再点击。
    pub confirm_delete: Option<String>,
}

impl ReplyRulesDialogState {
    pub fn new() -> Self {
        Self {
            confirm_delete: None,
        }
    }

    /// 点一次「删除」按钮：返回 `true` 表示**本次点击应真正执行删除**。
    ///
    /// 状态机（破坏性动作的二次确认）：
    /// - 首次点击（该规则尚未处于确认态）→ 进入确认态，返回 `false`（不删）；
    /// - 再点同一条规则 → 退出确认态，返回 `true`（执行删除）；
    /// - 点的是**另一条**规则 → 确认态转移到新规则，返回 `false`（不删）。
    ///
    /// 抽成方法而不是内联在渲染闭包里，是为了让这段状态机可被单测覆盖
    /// （闭包需要完整 `NetAssistantApp` 实体，无法直接测试）。
    pub fn on_delete_click(&mut self, rule_id: &str) -> bool {
        if self.confirm_delete.as_deref() == Some(rule_id) {
            self.confirm_delete = None;
            true
        } else {
            self.confirm_delete = Some(rule_id.to_string());
            false
        }
    }
}

impl Default for ReplyRulesDialogState {
    fn default() -> Self {
        Self::new()
    }
}

/// 打开管理弹窗(命令式, 由 Root 管理层叠)
pub fn open_reply_rules_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        dialog
            .title(t!("reply_rules.title").to_string())
            .w(px(760.0))
            .max_h(dialog_height(window))
            .keyboard(false)
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.reply_rules_dialog = None;
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
                    content.child(render_body(&entity, window, cx))
                }
            })
    });
}

/// 渲染弹窗主体(每帧从 app 读最新规则集与命中计数)
fn render_body(app: &Entity<NetAssistantApp>, window: &Window, cx: &App) -> Div {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let config = state.reply_rules_config();
    let hits = state.reply_rule_hits();
    let total_hits: u64 = hits.values().sum();
    let issues = validate_rules_config(&config);
    let error_count = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    let confirm_delete = state
        .reply_rules_dialog
        .as_ref()
        .and_then(|s| s.confirm_delete.clone());

    let mut body = div().flex().flex_col().gap_3().px_6().pb_4();

    // ===== 顶部: 总开关 =====
    let master_switch_entity = app.clone();
    body = body.child(
        div().flex().flex_row().items_center().gap_3().child(
            Switch::new("reply-rules-master-switch")
                .checked(config.enabled)
                .with_size(Size::Small)
                .label(t!("reply_rules.enabled").to_string())
                .on_change(move |_next, _window, cx| {
                    master_switch_entity.update(cx, |app, cx| {
                        let next = !app.storage.reply_rules().enabled;
                        app.storage.set_reply_rules_enabled(next);
                        app.sync_reply_rules_to_network(cx);
                        cx.notify();
                    });
                }),
        ),
    );

    // 未启用时的中性提示(不是错误: 未启用=功能完全不参与网络路径)
    if !config.enabled {
        body = body.child(hint_line(
            t!("reply_rules.disabled_hint").to_string(),
            theme.muted_foreground,
        ));
    } else if !config.rules.is_empty() && !state.has_enabled_reply_rules() {
        // 总开关开着但每条规则都被单独禁用: 提示比"看起来什么都没发生"有用得多
        body = body.child(hint_line(
            t!("reply_rules.no_enabled_rule").to_string(),
            theme.muted_foreground,
        ));
    }

    // ===== 规则集概览 =====
    body = body.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(t!("reply_rules.list_title").to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{} · {}",
                        t!("reply_rules.count", n = config.rules.len()),
                        t!("reply_rules.hits", n = total_hits)
                    )),
            )
            .when(error_count > 0, |d| {
                d.child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(theme.danger)
                        .child(t!("reply_rules.invalid_count", n = error_count).to_string()),
                )
            }),
    );

    // ===== 规则列表 =====
    if config.rules.is_empty() {
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
                        .child(t!("reply_rules.empty").to_string()),
                ),
        );
    } else {
        for (index, rule) in config.rules.iter().enumerate() {
            let hit = hits.get(&rule.id).copied().unwrap_or(0);
            let rule_errors: Vec<String> = issues
                .iter()
                .filter(|i| {
                    i.severity == Severity::Error
                        && i.path.starts_with(&format!("rules[{}]", index))
                })
                .map(|i| i.message.clone())
                .collect();
            body = body.child(render_rule_row(
                app,
                rule,
                index,
                config.rules.len(),
                hit,
                rule_errors,
                confirm_delete.as_deref() == Some(rule.id.as_str()),
                &theme,
            ));
        }
    }

    div().max_h(dialog_content_max_height(window)).child(
        div()
            .id("reply-rules-scroll")
            .overflow_y_scrollbar()
            .child(body),
    )
}

/// 单条规则卡片
#[allow(clippy::too_many_arguments)]
fn render_rule_row(
    app: &Entity<NetAssistantApp>,
    rule: &ReplyRule,
    index: usize,
    total: usize,
    hits: u64,
    errors: Vec<String>,
    confirming_delete: bool,
    theme: &Theme,
) -> Div {
    let rule_id = rule.id.clone();
    let summary = format_matcher_summary(&rule.matcher).join(" 且 ");
    let action = format_reply_summary(&rule.payload);
    let enabled = rule.enabled;

    let mut head = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_family("JetBrains Mono")
                .text_color(theme.muted_foreground)
                .child(format!("{}", index + 1)),
        )
        .child(
            div()
                .text_sm()
                .font_semibold()
                .text_color(if enabled {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .child(rule.name.clone()),
        );

    if !rule.enabled {
        head = head.child(
            div()
                .px_1p5()
                .rounded_md()
                .bg(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(t!("reply_rules.badge_disabled").to_string()),
        );
    }
    // 「任意消息」徽章: 空 All 组合(匹配全部报文)在列表里更易辨识
    if rule.matcher.is_match_all() {
        head = head.child(
            div()
                .px_1p5()
                .rounded_md()
                .bg(theme.primary.opacity(0.12))
                .text_xs()
                .text_color(theme.primary)
                .child(t!("reply_rules.badge_any_message").to_string()),
        );
    }
    for tag in &rule.tags {
        head = head.child(
            div()
                .px_1p5()
                .rounded_md()
                .bg(theme.primary.opacity(0.12))
                .text_xs()
                .text_color(theme.primary)
                .child(tag.clone()),
        );
    }
    head = head.child(
        div()
            .ml_auto()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(t!("reply_rules.row_hits", n = hits).to_string()),
    );

    let mut row = div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(if errors.is_empty() {
            theme.border
        } else {
            theme.danger
        })
        .bg(theme.secondary)
        .child(head)
        .child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "{}: {}",
                    t!("reply_rules.label_condition"),
                    summary
                )),
        )
        .child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(theme.muted_foreground)
                .child(format!("{}: {}", t!("reply_rules.label_action"), action)),
        );

    for err in &errors {
        row = row.child(hint_line(err.clone(), theme.danger));
    }

    // ===== 行内操作 =====
    let mut ops = div().flex().flex_row().items_center().gap_1();
    // 编辑按钮不走 `row_button`: 它内部已持有 `NetAssistantApp` 的租约,
    // 而 `open_reply_rule_edit_dialog` 需要自己 update 该实体, 嵌套会 double lease panic。
    ops = ops.child(row_button_raw(
        ElementId::named_usize("reply-rule-edit", index),
        t!("reply_rules.edit").to_string(),
        theme,
        {
            let app = app.downgrade();
            let rule_id = rule_id.clone();
            move |window, cx| {
                open_reply_rule_edit_dialog(app.clone(), Some(rule_id.clone()), window, cx);
            }
        },
    ));
    ops = ops.child(row_button(
        ElementId::named_usize("reply-rule-dup", index),
        t!("reply_rules.duplicate").to_string(),
        theme,
        app,
        {
            let rule_id = rule_id.clone();
            move |app, cx, _window| {
                if let Some(original) = app
                    .storage
                    .reply_rules()
                    .rules
                    .iter()
                    .find(|r| r.id == rule_id)
                    .cloned()
                {
                    let mut copy = original;
                    copy.id = uuid::Uuid::new_v4().to_string();
                    copy.name = format!("{} {}", copy.name, t!("reply_rules.copy_suffix"));
                    app.storage.upsert_reply_rule(copy);
                    app.sync_reply_rules_to_network(cx);
                }
            }
        },
    ));
    ops = ops.child(
        Switch::new(ElementId::named_usize("reply-rule-switch", index))
            .checked(enabled)
            .with_size(Size::Small)
            .on_change({
                let entity = app.clone();
                let rule_id = rule_id.clone();
                move |_next, _window, cx| {
                    entity.update(cx, |app, cx| {
                        // `storage.reply_rules()` 只给不可变引用: 取整表副本改完再整表保存
                        let mut config = app.reply_rules_config();
                        let mut changed = false;
                        for rule in config.rules.iter_mut() {
                            if rule.id == rule_id {
                                rule.enabled = !rule.enabled;
                                changed = true;
                                break;
                            }
                        }
                        if changed {
                            app.storage.save_reply_rules(config);
                            app.sync_reply_rules_to_network(cx);
                        }
                        cx.notify();
                    });
                }
            }),
    );
    ops = ops.child(row_button(
        ElementId::named_usize("reply-rule-up", index),
        "↑".to_string(),
        theme,
        app,
        {
            let rule_id = rule_id.clone();
            move |app, cx, _window| {
                if index > 0 && app.storage.move_reply_rule(&rule_id, index - 1) {
                    app.sync_reply_rules_to_network(cx);
                }
            }
        },
    ));
    ops = ops.child(row_button(
        ElementId::named_usize("reply-rule-down", index),
        "↓".to_string(),
        theme,
        app,
        {
            let rule_id = rule_id.clone();
            move |app, cx, _window| {
                if index + 1 < total && app.storage.move_reply_rule(&rule_id, index + 1) {
                    app.sync_reply_rules_to_network(cx);
                }
            }
        },
    ));
    ops = ops.child(row_button(
        ElementId::named_usize("reply-rule-reset-hits", index),
        t!("reply_rules.reset_hits").to_string(),
        theme,
        app,
        {
            let rule_id = rule_id.clone();
            move |app, cx, _window| {
                app.reply_rules_store.reset_hits_of(&rule_id);
                for store in app.server_reply_rules_stores.values() {
                    store.reset_hits_of(&rule_id);
                }
                cx.notify();
            }
        },
    ));
    // 删除: 破坏性动作需二次确认(按钮自身变确认态)
    ops = ops.child(row_button(
        ElementId::named_usize("reply-rule-delete", index),
        if confirming_delete {
            t!("reply_rules.delete_confirm").to_string()
        } else {
            t!("reply_rules.delete").to_string()
        },
        theme,
        app,
        {
            let rule_id = rule_id.clone();
            move |app, cx, _window| {
                // 删除不可撤销，一律二次确认（按钮自身变确认态）
                let confirmed = app
                    .reply_rules_dialog
                    .as_mut()
                    .map(|s| s.on_delete_click(&rule_id))
                    .unwrap_or(false);
                if !confirmed {
                    cx.notify();
                    return;
                }
                if app.storage.delete_reply_rule(&rule_id) {
                    app.sync_reply_rules_to_network(cx);
                }
            }
        },
    ));
    row = row.child(ops);

    row
}

/// 行内小按钮(仅样式 + 点击回调)
///
/// 回调直接拿到 `App`/`Window`, 由调用方自行决定是否 `update` `NetAssistantApp`:
/// 需要 app 可变引用的用 [`row_button`]; 需要自己 update 实体或叠开弹窗的
/// (例如「编辑」) 用本函数, 以免在已持有租约时再次 update 造成 double lease panic。
fn row_button_raw(
    id: ElementId,
    label: String,
    theme: &Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = theme.foreground;
    let hover_bg = theme.primary;
    let hover_fg = theme.primary_foreground;
    div()
        .id(id)
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .cursor_pointer()
        .text_color(color)
        .bg(theme.border)
        .hover(move |d| d.bg(hover_bg).text_color(hover_fg))
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_click(window, cx);
        })
}

/// 行内小按钮(自动在 `NetAssistantApp` 的 update 中执行回调)
///
/// 回调签名带 `window` 是因为部分动作需要 `window`(例如叠开弹窗)。
fn row_button(
    id: ElementId,
    label: String,
    theme: &Theme,
    app: &Entity<NetAssistantApp>,
    on_click: impl Fn(&mut NetAssistantApp, &mut Context<NetAssistantApp>, &mut Window) + 'static,
) -> impl IntoElement {
    let entity = app.clone();
    row_button_raw(id, label, theme, move |window, cx| {
        entity.update(cx, |app, cx| {
            on_click(app, cx, window);
            cx.notify();
        });
    })
}

/// 提示行(中性 / 错误 / 成功都用它, 只换颜色)
fn hint_line(text: String, color: gpui_kit::Hsla) -> Div {
    div()
        .text_xs()
        .whitespace_normal()
        .text_color(color)
        .child(text)
}

/// 底部操作按钮: 新建规则 / 全部清零 / 关闭
fn render_footer(app: &WeakEntity<NetAssistantApp>, cx: &App) -> DialogFooter {
    let has_rules = app
        .upgrade()
        .map(|e| !e.read(cx).storage.reply_rules().rules.is_empty())
        .unwrap_or(false);

    let app_new = app.clone();
    let new_rule = Button::new("reply-rules-new")
        .primary()
        .label(t!("reply_rules.new_rule").to_string())
        .on_click(move |_, window, cx| {
            open_reply_rule_edit_dialog(app_new.clone(), None, window, cx);
        });

    let app_reset = app.clone();
    let reset_all = Button::new("reply-rules-reset-all")
        .outline()
        .label(t!("reply_rules.reset_all").to_string())
        .on_click(move |_, _, cx| {
            let _ = app_reset.update(cx, |app, cx| {
                app.reply_rules_store.reset_hits();
                for store in app.server_reply_rules_stores.values() {
                    store.reset_hits();
                }
                cx.notify();
            });
        });
    // 没有规则时"全部清零"无可清零的对象, 置灰而非隐藏(位置稳定, 不跳动)
    let reset_all = if has_rules {
        reset_all
    } else {
        reset_all.disabled(true)
    };

    let app_close = app.clone();
    let close = Button::new("reply-rules-close")
        .outline()
        .label(t!("reply_rules.close").to_string())
        .on_click(move |_, window, cx| {
            let _ = app_close.update(cx, |app, cx| {
                app.reply_rules_dialog = None;
                cx.notify();
            });
            window.close_dialog(cx);
        });

    let mut footer = DialogFooter::new().child(new_rule).child(reset_all);
    footer = footer.child(close);
    footer
}

#[cfg(test)]
mod tests {
    use super::ReplyRulesDialogState;

    /// T-2：删除二次确认状态机 —— 第一次点击只进入确认态，第二次才真正删除。
    #[test]
    fn test_delete_requires_second_click() {
        let mut state = ReplyRulesDialogState::new();
        assert!(!state.on_delete_click("rule-a"), "首次点击不得删除");
        assert_eq!(state.confirm_delete.as_deref(), Some("rule-a"));

        assert!(state.on_delete_click("rule-a"), "二次点击才执行删除");
        assert_eq!(state.confirm_delete, None, "删除后确认态必须清空");
    }

    /// 确认态具有"排他性"：点另一条规则会把确认态转移过去，且当前点击不删除。
    #[test]
    fn test_delete_confirm_moves_to_other_rule() {
        let mut state = ReplyRulesDialogState::new();
        assert!(!state.on_delete_click("rule-a"));
        assert!(!state.on_delete_click("rule-b"), "点别的规则不得删除该规则");
        assert_eq!(state.confirm_delete.as_deref(), Some("rule-b"));
        // 之前处于确认态的 rule-a 再点一次，只重新进入确认态（不会误删）
        assert!(!state.on_delete_click("rule-a"));
        assert_eq!(state.confirm_delete.as_deref(), Some("rule-a"));
    }
}
