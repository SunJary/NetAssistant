// 「回复规则」管理弹窗
//
// 入口: 连接标签页发送区的「管理规则…」按钮(客户端/服务端都可见)。
// 职责: **本连接**规则集的概览与批量操作 —— 逐条启用/排序/复制/删除/清零。
// 规则严格按连接隔离: 从连接 A 打开只看到 A 的规则。
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
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, Size};

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::reply::model::{
    ReplyRule, Severity, format_matcher_summary, format_reply_summary, validate_rule,
};

use super::{dialog_content_max_height, dialog_height, open_reply_rule_edit_dialog};

/// 「回复规则」管理弹窗状态(打开时创建, 关闭时由 app 置 None)
pub struct ReplyRulesDialogState {
    /// 本弹窗所属的连接 id（= tab_id）。规则严格按连接隔离，列表只读该连接的规则。
    pub tab_id: String,
    /// 连接展示名（如 "127.0.0.1:502"），用于标题/空态文案
    pub tab_label: String,
    /// 已点击过一次「删除」、等待二次确认的规则 id
    ///
    /// 用"按钮自身变确认态"而不是再开一个确认弹窗: 弹窗栈里再叠弹窗会让
    /// 关闭语义(close_dialog 关最上层 vs 关全部)变得难以推理, 而破坏性动作的
    /// 二次确认只需要一次明确的再点击。
    pub confirm_delete: Option<String>,
}

impl ReplyRulesDialogState {
    pub fn new(tab_id: impl Into<String>, tab_label: impl Into<String>) -> Self {
        Self {
            tab_id: tab_id.into(),
            tab_label: tab_label.into(),
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

/// 拖动排序的载荷：只带被拖规则的 id(落点由目标卡片的渲染序号决定)
///
/// 不复用 `DragPanel` 之类的现成类型是因为规则的落点语义是"重排到第 N 位",
/// 与面板/标签页的跨容器搬运无关; 独立类型也让 `can_drop` 的类型过滤更直接。
#[derive(Clone)]
struct RuleDragPayload {
    rule_id: String,
    name: String,
}

/// 拖动时跟随光标的预览(只显示规则名, 避免大面积遮挡列表)
struct RuleDragPreview {
    name: String,
}

impl Render for RuleDragPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(theme.primary)
            .text_color(theme.primary_foreground)
            .text_xs()
            .child(self.name.clone())
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

/// 渲染弹窗主体(每帧从 app 读**本连接**的规则集与命中计数)
fn render_body(app: &Entity<NetAssistantApp>, window: &Window, cx: &App) -> Div {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let tab_id = state
        .reply_rules_dialog
        .as_ref()
        .map(|s| s.tab_id.clone())
        .unwrap_or_default();
    // 严格按连接隔离：只取本连接（tab_id）的规则
    let rules: Vec<ReplyRule> = state.storage.rules_for_connection(&tab_id).to_vec();
    let hits = state.reply_rule_hits();
    let total_hits: u64 = rules
        .iter()
        .map(|r| hits.get(&r.id).copied().unwrap_or(0))
        .sum();
    let per_rule_errors: Vec<Vec<String>> = rules
        .iter()
        .map(|rule| {
            validate_rule(rule)
                .into_iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| i.message)
                .collect()
        })
        .collect();
    let error_count: usize = per_rule_errors.iter().map(|v| v.len()).sum();
    let confirm_delete = state
        .reply_rules_dialog
        .as_ref()
        .and_then(|s| s.confirm_delete.clone());

    let mut body = div().flex().flex_col().gap_3().px_6().pb_4();

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
                        t!("reply_rules.count", n = rules.len()),
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

    // 拖动排序的提示: 只有 ≥2 条时拖动才有意义
    if rules.len() > 1 {
        body = body.child(hint_line(
            t!("reply_rules.drag_hint").to_string(),
            theme.muted_foreground,
        ));
    }

    // ===== 规则列表 =====
    if rules.is_empty() {
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
        for (index, rule) in rules.iter().enumerate() {
            let hit = hits.get(&rule.id).copied().unwrap_or(0);
            body = body.child(render_rule_row(
                app,
                &tab_id,
                rule,
                index,
                hit,
                per_rule_errors[index].clone(),
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
///
/// 布局: 左侧拖动手柄 | 中间摘要(标题/条件/动作/错误) | 右侧上下两块(次数+开关 / 图标操作组)。
/// 开关放右上角是因为它是最常按的单条操作; 破坏性操作(删除)收进右下角并保留二次点击确认。
fn render_rule_row(
    app: &Entity<NetAssistantApp>,
    tab_id: &str,
    rule: &ReplyRule,
    index: usize,
    hits: u64,
    errors: Vec<String>,
    confirming_delete: bool,
    theme: &Theme,
) -> Div {
    let rule_id = rule.id.clone();
    let summary = format_matcher_summary(&rule.matcher).join(" 且 ");
    let action = format_reply_summary(&rule.payload);
    let enabled = rule.enabled;

    // ===== 中间: 标题 + 条件 + 动作 + 错误 =====
    let mut head = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .flex_wrap()
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

    let mut main = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap_1()
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
        main = main.child(hint_line(err.clone(), theme.danger));
    }

    // ===== 右上: 触发次数 + 启用开关 =====
    let top_right = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(t!("reply_rules.row_hits", n = hits).to_string()),
        )
        .child(
            Switch::new(ElementId::named_usize("reply-rule-switch", index))
                .checked(enabled)
                .with_size(Size::Small)
                .on_change({
                    let entity = app.clone();
                    let rule_id = rule_id.clone();
                    let tab_id = tab_id.to_string();
                    move |_next, _window, cx| {
                        entity.update(cx, |app, cx| {
                            // `storage.reply_rules()` 只给不可变引用: 取整表副本改完再整表保存
                            let mut config = app.reply_rules_config();
                            let mut changed = false;
                            if let Some(conn_rules) = config.connections.get_mut(&tab_id) {
                                for rule in conn_rules.iter_mut() {
                                    if rule.id == rule_id {
                                        rule.enabled = !rule.enabled;
                                        changed = true;
                                        break;
                                    }
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

    // ===== 右下: 图标操作组 =====
    // 删除进入确认态后按钮会换成文字, 用 i18n 文案而不是写死中文
    let delete_label = if confirming_delete {
        t!("reply_rules.delete_confirm").to_string()
    } else {
        "✕".to_string()
    };
    let mut ops = div().flex().flex_row().items_center().gap_1();
    // 编辑按钮不走 `row_icon_button`: 它内部已持有 `NetAssistantApp` 的租约,
    // 而 `open_reply_rule_edit_dialog` 需要自己 update 该实体, 嵌套会 double lease panic。
    ops = ops.child(row_icon_button_raw(
        ElementId::named_usize("reply-rule-edit", index),
        "✎".to_string(),
        t!("reply_rules.edit").to_string(),
        false,
        theme,
        {
            let app = app.downgrade();
            let rule_id = rule_id.clone();
            let tab_id = tab_id.to_string();
            move |window, cx| {
                open_reply_rule_edit_dialog(
                    app.clone(),
                    tab_id.clone(),
                    Some(rule_id.clone()),
                    window,
                    cx,
                );
            }
        },
    ));
    ops = ops.child(row_icon_button(
        ElementId::named_usize("reply-rule-dup", index),
        "⧉".to_string(),
        t!("reply_rules.duplicate").to_string(),
        false,
        theme,
        app,
        {
            let rule_id = rule_id.clone();
            let tab_id = tab_id.to_string();
            move |app, cx, _window| {
                if let Some(original) = app
                    .storage
                    .rules_for_connection(&tab_id)
                    .iter()
                    .find(|r| r.id == rule_id)
                    .cloned()
                {
                    let mut copy = original;
                    copy.id = uuid::Uuid::new_v4().to_string();
                    copy.name = format!("{} {}", copy.name, t!("reply_rules.copy_suffix"));
                    app.storage.upsert_reply_rule(&tab_id, copy);
                    // 副本追加到末尾后须重排 priority，否则会与原规则同值、顺序错乱
                    app.storage.renumber_reply_rule_priorities(&tab_id);
                    app.sync_reply_rules_to_network(cx);
                }
            }
        },
    ));
    ops = ops.child(row_icon_button(
        ElementId::named_usize("reply-rule-reset-hits", index),
        "↺".to_string(),
        t!("reply_rules.reset_hits").to_string(),
        false,
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
    ops = ops.child(row_icon_button(
        ElementId::named_usize("reply-rule-delete", index),
        delete_label,
        t!("reply_rules.delete_confirm").to_string(),
        true,
        theme,
        app,
        {
            let rule_id = rule_id.clone();
            let tab_id = tab_id.to_string();
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
                if app.storage.delete_reply_rule(&tab_id, &rule_id) {
                    app.sync_reply_rules_to_network(cx);
                }
            }
        },
    ));

    let right = div()
        .flex_none()
        .flex()
        .flex_col()
        .items_end()
        .gap_2()
        .child(top_right)
        .child(ops);

    // ===== 左侧: 拖动手柄(独占鼠标事件, 卡片其余区域保持点击/选择行为) =====
    let handle_bg = theme.border;
    let handle = div()
        .id(ElementId::named_usize("reply-rule-handle", index))
        .flex_none()
        .px_1()
        .py_0p5()
        .rounded_md()
        .text_sm()
        .text_color(theme.muted_foreground)
        .cursor_grab()
        .hover(move |d| d.bg(handle_bg))
        .child("⠿")
        .tooltip({
            let tip = t!("reply_rules.drag_handle_tooltip").to_string();
            move |window, cx| Tooltip::new(tip.clone()).build(window, cx)
        })
        .on_drag(
            RuleDragPayload {
                rule_id: rule_id.clone(),
                name: rule.name.clone(),
            },
            |payload, _offset, _window, cx| {
                cx.new(|_| RuleDragPreview {
                    name: payload.name.clone(),
                })
            },
        );

    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_2()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(if errors.is_empty() {
            theme.border
        } else {
            theme.danger
        })
        .bg(theme.secondary)
        .child(handle)
        .child(main)
        .child(right)
        // 落点高亮: 只有拖别的规则到本卡片上方时才亮边框
        .can_drop({
            let self_id = rule.id.clone();
            move |dragged, _window, _cx| {
                dragged
                    .downcast_ref::<RuleDragPayload>()
                    .is_some_and(|p| p.rule_id != self_id)
            }
        })
        .drag_over::<RuleDragPayload>(|d, _drag, _window, cx| {
            d.border_color(cx.theme().primary)
        })
        .on_drop({
            let entity = app.clone();
            let drop_tab_id = tab_id.to_string();
            move |drag: &RuleDragPayload, _window, cx| {
                let dragged = drag.rule_id.clone();
                entity.update(cx, |app, cx| {
                    // 落点语义 = "重排到第 index 位"; 原地投放时 move 返回 false, 天然 no-op
                    if app
                        .storage
                        .move_reply_rule(&drop_tab_id, &dragged, index)
                    {
                        app.sync_reply_rules_to_network(cx);
                    }
                    cx.notify();
                });
            }
        })
}

/// 行内图标按钮(自动在 `NetAssistantApp` 的 update 中执行回调)
fn row_icon_button(
    id: ElementId,
    label: String,
    tooltip: String,
    danger: bool,
    theme: &Theme,
    app: &Entity<NetAssistantApp>,
    on_click: impl Fn(&mut NetAssistantApp, &mut Context<NetAssistantApp>, &mut Window) + 'static,
) -> impl IntoElement {
    let entity = app.clone();
    row_icon_button_raw(id, label, tooltip, danger, theme, move |window, cx| {
        entity.update(cx, |app, cx| {
            on_click(app, cx, window);
            cx.notify();
        });
    })
}

/// 行内图标按钮(仅样式 + 点击回调 + tooltip)
///
/// 回调直接拿到 `App`/`Window`, 由调用方自行决定是否 `update` `NetAssistantApp`:
/// 需要 app 可变引用的用 [`row_icon_button`]; 需要自己 update 实体或叠开弹窗的
/// (例如「编辑」) 用本函数, 以免在已持有租约时再次 update 造成 double lease panic。
fn row_icon_button_raw(
    id: ElementId,
    label: String,
    tooltip: String,
    danger: bool,
    theme: &Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let base_fg = if danger { theme.danger } else { theme.foreground };
    let hover_bg = if danger { theme.danger } else { theme.primary };
    let hover_fg = theme.primary_foreground;
    div()
        .id(id)
        .px_1p5()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(base_fg)
        .bg(theme.border)
        .hover(move |d| d.bg(hover_bg).text_color(hover_fg))
        .child(label)
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_click(window, cx);
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
    // 本弹窗所属连接: 只统计/操作该连接的规则
    let (has_rules, tab_id) = app
        .upgrade()
        .map(|e| {
            let s = e.read(cx);
            let tab_id = s
                .reply_rules_dialog
                .as_ref()
                .map(|d| d.tab_id.clone())
                .unwrap_or_default();
            let has_rules = !s.storage.rules_for_connection(&tab_id).is_empty();
            (has_rules, tab_id)
        })
        .unwrap_or_default();

    let app_new = app.clone();
    let new_tab_id = tab_id.clone();
    let new_rule = Button::new("reply-rules-new")
        .primary()
        .label(t!("reply_rules.new_rule").to_string())
        .on_click(move |_, window, cx| {
            open_reply_rule_edit_dialog(
                app_new.clone(),
                new_tab_id.clone(),
                None,
                window,
                cx,
            );
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
        let mut state = ReplyRulesDialogState::new("tab-1", "127.0.0.1:502");
        assert!(!state.on_delete_click("rule-a"), "首次点击不得删除");
        assert_eq!(state.confirm_delete.as_deref(), Some("rule-a"));

        assert!(state.on_delete_click("rule-a"), "二次点击才执行删除");
        assert_eq!(state.confirm_delete, None, "删除后确认态必须清空");
    }

    /// 确认态具有"排他性"：点另一条规则会把确认态转移过去，且当前点击不删除。
    #[test]
    fn test_delete_confirm_moves_to_other_rule() {
        let mut state = ReplyRulesDialogState::new("tab-1", "127.0.0.1:502");
        assert!(!state.on_delete_click("rule-a"));
        assert!(!state.on_delete_click("rule-b"), "点别的规则不得删除该规则");
        assert_eq!(state.confirm_delete.as_deref(), Some("rule-b"));
        // 之前处于确认态的 rule-a 再点一次，只重新进入确认态（不会误删）
        assert!(!state.on_delete_click("rule-a"));
        assert_eq!(state.confirm_delete.as_deref(), Some("rule-a"));
    }
}
