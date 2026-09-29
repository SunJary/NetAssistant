// 发送任务面板(逐行发送)
//
// 由两部分组成:
// - 发送区工具行右侧的图标按钮(list-checks): 用颜色/角标汇总该 tab 的任务状态
// - 点击后展开的浮层面板: 任务卡片列表(进度、轮次、暂停/继续/停止/删除、行清单展开)
//
// 浮层沿用 variable_picker.rs 同款 deferred + anchored 模式(而非 gpui_component Popover):
// 面板内容需要按每帧最新状态渲染, 且操作要直接落到 app 的发送任务方法上,
// 受控 Popover 的内容闭包拿不到 `Context<NetAssistantApp>`, 故不适用。

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ElementExt as _, Icon, StyledExt as _, Theme};
use indexmap::IndexMap;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::custom_icons::CustomIconName;
use crate::send_task::{SendTaskEntry, TaskStatus};

use super::connection_tab::ConnectionTabState;

/// 展开行清单时固定展示的最大条数(超出只提示总数, 不做虚拟化)
const MAX_EXPANDED_LINES: usize = 128;

/// 任务图标按钮的汇总外观(由该 tab 的全部任务推导)
struct TaskButtonVisual {
    icon_color: Hsla,
    /// 运行中任务数(>0 时显示角标)
    badge: Option<usize>,
    /// 是否存在失败任务(>=1 时显示红色小圆点)
    failed: bool,
}

impl TaskButtonVisual {
    fn from_tasks(tasks: &IndexMap<String, SendTaskEntry>, theme: &Theme) -> Self {
        // 面板完整过滤 hidden(周期发送): 卡片 / 计数 / 着色 / 空态一概不含
        let visible = tasks.values().filter(|e| !e.state.config.hidden);
        let running = visible
            .clone()
            .filter(|e| e.state.status == TaskStatus::Running)
            .count();
        let paused = visible
            .clone()
            .filter(|e| matches!(e.state.status, TaskStatus::Idle | TaskStatus::Paused))
            .count();
        let failed = visible
            .clone()
            .any(|e| matches!(e.state.status, TaskStatus::Failed(_)));

        let icon_color = if running > 0 {
            theme.primary
        } else if failed {
            theme.danger
        } else if paused > 0 {
            theme.warning
        } else {
            theme.muted_foreground
        };

        Self {
            icon_color,
            badge: if running > 0 { Some(running) } else { None },
            failed: failed && running == 0,
        }
    }
}

/// 发送任务面板组件(借用该 tab 的状态, 渲染按钮与浮层)
pub struct SendTaskPanel<'a> {
    tab_id: String,
    tab_state: &'a ConnectionTabState,
}

impl<'a> SendTaskPanel<'a> {
    pub fn new(tab_id: String, tab_state: &'a ConnectionTabState) -> Self {
        Self { tab_id, tab_state }
    }

    /// 渲染工具行里的任务图标按钮(状态色 + 角标 + tooltip)
    pub fn render_button(&self, theme: &Theme, cx: &mut Context<NetAssistantApp>) -> Div {
        let visual = TaskButtonVisual::from_tasks(&self.tab_state.send_tasks, theme);
        let open = self.tab_state.send_task_panel_open;

        // on_prepaint 需挂在 Div 上(ElementExt 仅对 ParentElement 实现),
        // 必须在 .id()/.tooltip() 转为 Stateful 之前调用
        let prepaint_entity = cx.entity().clone();
        let prepaint_tab_id = self.tab_id.clone();
        let prepaint_handler: Box<dyn Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static> =
            Box::new(move |bounds, _window, cx| {
                prepaint_entity.update(cx, |app, _| {
                    if let Some(tab_state) = app.connection_tabs.get_mut(&prepaint_tab_id) {
                        tab_state.send_task_button_bounds = Some(bounds);
                    }
                });
            });

        let toggle_tab_id = self.tab_id.clone();
        let hover_bg = theme.secondary_hover;
        div()
            .relative()
            .child(
                div()
                    .on_prepaint(prepaint_handler)
                    .id("send-task-btn")
                    .p_1()
                    .rounded_md()
                    .bg(if open {
                        theme.secondary_hover
                    } else {
                        theme.secondary
                    })
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .child(
                        Icon::new(CustomIconName::ListChecks)
                            .size(px(14.0))
                            .text_color(visual.icon_color),
                    )
                    .tooltip(|window, cx| {
                        Tooltip::new(t!("send_task.tooltip").to_string()).build(window, cx)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _event, _window, cx| {
                            if let Some(tab_state) = app.connection_tabs.get_mut(&toggle_tab_id) {
                                tab_state.send_task_panel_open = !tab_state.send_task_panel_open;
                            }
                            cx.notify();
                        }),
                    ),
            )
            // 运行中任务数角标
            .when_some(visual.badge, |d, count| {
                d.child(
                    div()
                        .absolute()
                        .top(px(-3.0))
                        .right(px(-3.0))
                        .min_w(px(14.0))
                        .h(px(14.0))
                        .px_0p5()
                        .rounded_full()
                        .bg(theme.primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(theme.primary_foreground)
                        .child(count.to_string()),
                )
            })
            // 失败小圆点
            .when(visual.failed, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(-1.0))
                        .right(px(-1.0))
                        .w(px(7.0))
                        .h(px(7.0))
                        .rounded_full()
                        .bg(theme.danger),
                )
            })
    }

    /// 渲染任务浮层面板; 返回 None 表示未展开或按钮坐标尚未就绪
    pub fn render_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Option<AnyElement> {
        if !self.tab_state.send_task_panel_open {
            return None;
        }
        let bounds = self.tab_state.send_task_button_bounds?;

        // 点击面板外: 仅收起面板, 不销毁任务
        let dismiss_entity = cx.entity().clone();
        let dismiss_tab_id = self.tab_id.clone();
        let dismiss_handler: Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static> =
            Box::new(move |_event, _window, cx| {
                dismiss_entity.update(cx, |app, cx| {
                    if let Some(tab_state) = app.connection_tabs.get_mut(&dismiss_tab_id) {
                        tab_state.send_task_panel_open = false;
                    }
                    cx.notify();
                });
            });

        // 过滤 hidden(周期发送): 不进面板, 也不影响空态
        let visible: Vec<(&String, &SendTaskEntry)> = self
            .tab_state
            .send_tasks
            .iter()
            .filter(|(_, e)| !e.state.config.hidden)
            .collect();

        let body: AnyElement = if visible.is_empty() {
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .p_6()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(t!("send_task.empty").to_string())
                .into_any_element()
        } else {
            let cards: Vec<AnyElement> = visible
                .iter()
                .map(|(task_id, entry)| self.render_card(task_id.as_str(), entry, theme, cx))
                .collect();
            // 两层结构: 外层分配剩余高度, 内层滚动
            div()
                .flex_1()
                .overflow_hidden()
                .child(div().size_full().overflow_y_scrollbar().children(cards))
                .into_any_element()
        };

        // 标题栏「＋ 添加到定时任务」: 每连接仅 1 个, 已存在时置灰 + tooltip
        let timed_exists = self
            .tab_state
            .send_tasks
            .values()
            .any(|e| e.state.config.is_timed());
        let add_tab_id = self.tab_id.clone();
        let add_label = t!("send_task.add_timed").to_string();
        let add_tooltip = if timed_exists {
            t!("send_task.add_timed_exists").to_string()
        } else {
            add_label.clone()
        };
        let hover_bg = theme.secondary_hover;
        let add_button = div()
            .id("add-timed-task")
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .rounded_md()
            .text_xs()
            .when(timed_exists, |d| d.text_color(theme.muted_foreground))
            .when(!timed_exists, |d| {
                d.cursor_pointer()
                    .bg(theme.secondary)
                    .text_color(theme.secondary_foreground)
                    .hover(move |s| s.bg(hover_bg))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _event, window, cx| {
                            app.open_timed_task_dialog_for_tab(add_tab_id.clone(), window, cx);
                        }),
                    )
            })
            .child(
                Icon::new(CustomIconName::IconName(gpui_kit::component::IconName::Plus))
                    .size(px(12.0)),
            )
            .child(add_label)
            .tooltip(move |window, cx| Tooltip::new(add_tooltip.clone()).build(window, cx));

        let popup = anchored()
            .anchor(Anchor::TopRight)
            .position(bounds.bottom_right())
            .offset(point(px(0.0), px(4.0)))
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .occlude()
                    .w(px(400.0))
                    .max_h(px(440.0))
                    .flex()
                    .flex_col()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down_out(dismiss_handler)
                    // 标题(固定) + 「＋ 添加到定时任务」
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child(t!("send_task.title").to_string()),
                            )
                            .child(add_button),
                    )
                    .child(body),
            );

        Some(deferred(popup).with_priority(1).into_any_element())
    }

    /// 渲染单个任务卡片
    fn render_card(
        &self,
        task_id: &str,
        entry: &SendTaskEntry,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> AnyElement {
        let state = &entry.state;
        let config = &state.config;
        let expanded = self.tab_state.send_task_expanded.as_deref() == Some(task_id);
        let is_running = state.status == TaskStatus::Running;
        // 引擎是否仍在运行: 终态(Stopped/Finished/Failed)的 runner 已退出,
        // 此时「暂停/继续」与「停止」都不会有任何效果, 故不渲染, 只留删除与展开
        let is_alive = matches!(
            state.status,
            TaskStatus::Idle | TaskStatus::Running | TaskStatus::Paused
        );

        // 操作按钮: 暂停/继续、停止、删除、展开行清单
        // 定时任务(心跳): 暂停/继续映射持久化 enabled, ✎ 编辑, 无「停止」(与暂停语义重复), 无行清单
        let is_timed = config.is_timed();
        let toggle_tab_id = self.tab_id.clone();
        let toggle_task_id = task_id.to_string();
        let stop_tab_id = self.tab_id.clone();
        let stop_task_id = task_id.to_string();
        let delete_tab_id = self.tab_id.clone();
        let delete_task_id = task_id.to_string();
        let edit_tab_id = self.tab_id.clone();
        let expand_tab_id = self.tab_id.clone();
        let expand_task_id = task_id.to_string();

        let mut actions = div().flex().items_center().gap_1().flex_shrink_0();
        if is_timed {
            actions = actions
                .child(icon_button(
                    format!("task-toggle-{task_id}"),
                    if is_running {
                        CustomIconName::CirclePause
                    } else {
                        CustomIconName::CirclePlay
                    },
                    if is_running {
                        t!("send_task.pause").to_string()
                    } else {
                        t!("send_task.resume").to_string()
                    },
                    theme,
                    cx.listener(move |app, _event, _window, cx| {
                        if is_running {
                            app.pause_timed_task(&toggle_tab_id, cx);
                        } else {
                            app.resume_timed_task(&toggle_tab_id, cx);
                        }
                    }),
                ))
                .child(icon_button(
                    format!("task-edit-{task_id}"),
                    CustomIconName::Pencil,
                    t!("send_task.edit").to_string(),
                    theme,
                    cx.listener(move |app, _event, window, cx| {
                        app.open_timed_task_dialog_for_tab(edit_tab_id.clone(), window, cx);
                    }),
                ));
        } else if is_alive {
            actions = actions
                .child(icon_button(
                    format!("task-toggle-{task_id}"),
                    if is_running {
                        CustomIconName::CirclePause
                    } else {
                        CustomIconName::CirclePlay
                    },
                    if is_running {
                        t!("send_task.pause").to_string()
                    } else {
                        t!("send_task.resume").to_string()
                    },
                    theme,
                    cx.listener(move |app, _event, _window, cx| {
                        if is_running {
                            app.pause_send_task(&toggle_tab_id, &toggle_task_id, None, cx);
                        } else {
                            app.resume_send_task(&toggle_tab_id, &toggle_task_id, cx);
                        }
                    }),
                ))
                .child(icon_button(
                    format!("task-stop-{task_id}"),
                    CustomIconName::CircleStop,
                    t!("send_task.stop").to_string(),
                    theme,
                    cx.listener(move |app, _event, _window, cx| {
                        app.stop_send_task(&stop_tab_id, &stop_task_id, cx);
                    }),
                ));
        }
        let mut actions = actions.child(icon_button(
            format!("task-delete-{task_id}"),
            CustomIconName::Trash2,
            t!("send_task.delete").to_string(),
            theme,
            cx.listener(move |app, _event, _window, cx| {
                if is_timed {
                    app.delete_timed_task(&delete_tab_id, cx);
                } else {
                    app.delete_send_task(&delete_tab_id, &delete_task_id, cx);
                }
            }),
        ));
        if !is_timed {
            actions = actions.child(icon_button(
                format!("task-expand-{task_id}"),
                if expanded {
                    CustomIconName::IconName(gpui_kit::component::IconName::ChevronUp)
                } else {
                    CustomIconName::IconName(gpui_kit::component::IconName::ChevronDown)
                },
                if expanded {
                    t!("send_task.collapse_lines").to_string()
                } else {
                    t!("send_task.expand_lines").to_string()
                },
                theme,
                cx.listener(move |app, _event, _window, cx| {
                    if let Some(tab_state) = app.connection_tabs.get_mut(&expand_tab_id) {
                        tab_state.send_task_expanded =
                            if tab_state.send_task_expanded.as_deref() == Some(&expand_task_id) {
                                None
                            } else {
                                Some(expand_task_id.clone())
                            };
                    }
                    cx.notify();
                }),
            ));
        }
        let actions = actions;

        // 轮次: 无限循环 / 最多 n 轮 / 仅一轮
        let round_text = if config.loop_enabled {
            match config.max_rounds {
                Some(max) => format!("{} / {}", t!("send_task.round", n = state.round), max),
                None => format!(
                    "{} / {}",
                    t!("send_task.round", n = state.round),
                    t!("send_task.round_infinite")
                ),
            }
        } else {
            t!("send_task.round", n = state.round).to_string()
        };

        let mut card = div()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .text_ellipsis()
                                    .child(config.name.clone()),
                            )
                            .child(status_badge(&state.status, theme)),
                    )
                    .child(actions),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .when(is_timed, |d| {
                        // 心跳: 「自动启动」徽标 + 第 n 次 + 间隔
                        d.child(auto_start_badge(theme))
                            .child(t!("send_task.timed_round", n = state.round).to_string())
                    })
                    .when(!is_timed, |d| {
                        // 逐行: i / N + 轮次
                        d.child(format!("{} / {}", state.sent_items, state.total_items))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    // 循环发送标记
                                    .when(config.loop_enabled, |d| {
                                        d.child(Icon::new(CustomIconName::Repeat).size(px(10.0)))
                                    })
                                    .child(round_text),
                            )
                    })
                    .child(
                        t!("send_task.interval", ms = config.interval.get()).to_string(),
                    ),
            );

        // 暂停原因 / 失败原因
        if let Some(reason) = state.pause_reason.as_ref() {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(theme.warning)
                    .child(reason.clone()),
            );
        }
        if let TaskStatus::Failed(message) = &state.status {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(theme.danger)
                    .child(message.clone()),
            );
        }

        // 行清单展开: 固定展示前 MAX_EXPANDED_LINES 条 + 底部总数提示
        if expanded {
            let items = config.items();
            let shown = items.len().min(MAX_EXPANDED_LINES);
            card = card
                .child(
                    div()
                        .pt_1()
                        .border_t_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("send_task.lines_preview_title", n = shown).to_string()),
                )
                .children(items.iter().take(MAX_EXPANDED_LINES).map(|item| {
                    div()
                        .text_xs()
                        .font_family("JetBrains Mono")
                        .text_color(theme.foreground)
                        .child(format!("{}. {}", item.index + 1, item.raw))
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("send_task.items", n = items.len()).to_string()),
                );
        }

        card.into_any_element()
    }
}

/// 「自动启动」徽标(定时任务/心跳专属)
fn auto_start_badge(theme: &Theme) -> Div {
    div()
        .flex_shrink_0()
        .px_1p5()
        .py_0p5()
        .rounded(px(4.0))
        .bg(theme.primary.opacity(0.12))
        .text_xs()
        .text_color(theme.primary)
        .child(t!("send_task.auto_start").to_string())
}

/// 状态徽章(文字 + 半透明底色)
fn status_badge(status: &TaskStatus, theme: &Theme) -> Div {
    let (text, color) = match status {
        TaskStatus::Idle => (
            t!("send_task.status_idle").to_string(),
            theme.muted_foreground,
        ),
        TaskStatus::Running => (t!("send_task.status_running").to_string(), theme.success),
        TaskStatus::Paused => (t!("send_task.status_paused").to_string(), theme.warning),
        TaskStatus::Finished => (t!("send_task.status_finished").to_string(), theme.primary),
        TaskStatus::Stopped => (
            t!("send_task.status_stopped").to_string(),
            theme.muted_foreground,
        ),
        TaskStatus::Failed(_) => (t!("send_task.status_failed").to_string(), theme.danger),
    };

    div()
        .flex_shrink_0()
        .px_1p5()
        .py_0p5()
        .rounded(px(4.0))
        .bg(color.opacity(0.12))
        .text_xs()
        .text_color(color)
        .child(text)
}

/// 卡片操作图标按钮(等宽方形, hover 变色)
fn icon_button(
    id: String,
    icon: CustomIconName,
    tooltip: String,
    theme: &Theme,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let hover_bg = theme.secondary;
    let hover_fg = theme.foreground;
    div()
        .id(id)
        .p_1()
        .rounded(px(4.0))
        .text_color(theme.muted_foreground)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
        .child(Icon::new(icon).size(px(14.0)))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, handler)
}
