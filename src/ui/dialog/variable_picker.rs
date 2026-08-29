// 「插入变量」浮层组件
//
// 在压测配置弹窗中点击「插入变量」时展开:
// 一行一个变量, 显示变量名 + 简短说明, 点击后在输入框光标处插入变量并关闭。
//
// 采用 gpui-component combobox 同款 deferred+anchored 模式:
// - deferred 在独立合成层渲染, 不会被滚动区/兄弟节点覆盖
// - anchored 依据按钮的窗口坐标定位, 并自动吸附窗口边缘防止越界
// - on_mouse_down_out 在面板外点击时关闭

use std::borrow::Cow;

use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::Theme;
use gpui_component::StyledExt as _;
use rust_i18n::t;

use crate::app::NetAssistantApp;

/// 变量定义: 名称、简短说明、点击后插入的文本
struct VariableDef {
    name: &'static str,
    description: Cow<'static, str>,
    insert_text: &'static str,
}

/// 支持的变量列表(顺序即展示顺序)
fn variables() -> Vec<VariableDef> {
    vec![
        VariableDef {
            name: "${seq}",
            description: t!("variable_picker.desc_seq"),
            insert_text: "${seq}",
        },
        VariableDef {
            name: "${worker_id}",
            description: t!("variable_picker.desc_worker_id"),
            insert_text: "${worker_id}",
        },
        VariableDef {
            name: "${counter}",
            description: t!("variable_picker.desc_counter"),
            insert_text: "${counter}",
        },
        VariableDef {
            name: "${timestamp}",
            description: t!("variable_picker.desc_timestamp"),
            insert_text: "${timestamp}",
        },
        VariableDef {
            name: "${uuid}",
            description: t!("variable_picker.desc_uuid"),
            insert_text: "${uuid}",
        },
        VariableDef {
            name: "${random:min:max}",
            description: t!("variable_picker.desc_random"),
            insert_text: "${random:1:100}",
        },
    ]
}

/// 渲染「插入变量」浮层
///
/// `button_bounds` 为「插入变量」按钮在窗口坐标系中的 bounds (由 on_prepaint 提供),
/// 面板锚定在按钮右下角, 向下展开; 若越界则由 anchored 自动吸附窗口边缘。
pub fn render_variable_picker(
    app: &Entity<NetAssistantApp>,
    button_bounds: Bounds<Pixels>,
    theme: &Theme,
    _cx: &App,
) -> impl IntoElement {
    // 点击面板外任意区域关闭浮层
    let dismiss_entity = app.clone();
    let dismiss_handler: Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static> =
        Box::new(move |_, _, cx| {
            dismiss_entity.update(cx, |app, cx| {
                if let Some(s) = &mut app.stress_config_dialog {
                    s.show_variable_picker = false;
                }
                cx.notify();
            });
        });

    let entity = app.clone();
    let popup = anchored()
        .anchor(Anchor::TopRight)
        .position(button_bounds.bottom_right())
        .offset(point(px(0.0), px(4.0)))
        .snap_to_window_with_margin(px(8.0))
        .child(
            div()
                .occlude()
                .w(px(320.0))
                .h(px(320.0))
                .flex()
                .flex_col()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .rounded_md()
                .shadow_lg()
                .overflow_hidden()
                .on_mouse_down_out(dismiss_handler)
                // 标题 (固定)
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(theme.border)
                        .text_xs()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(t!("variable_picker.title").to_string()),
                )
                // 变量列表 (两层结构: 外层分配剩余高度, 内层滚动)
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div()
                                .size_full()
                                .overflow_y_scrollbar()
                                .children(variables().iter().map(|var_def| {
                                    render_variable_row(&entity, var_def, theme)
                                })),
                        ),
                )
                // 底部提示 (固定)
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("variable_picker.hint").to_string()),
                ),
        );

    // deferred 在独立合成层渲染, with_priority(1) 高于普通内容
    deferred(popup).with_priority(1)
}

/// 渲染单行变量(变量名 + 说明), 点击插入到光标处并关闭浮层
fn render_variable_row(
    app: &Entity<NetAssistantApp>,
    var_def: &VariableDef,
    theme: &Theme,
) -> Div {
    let insert_text = var_def.insert_text;
    let entity = app.clone();
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .px_3()
        .py_1p5()
        .cursor_pointer()
        .hover(|d| d.bg(theme.border))
        .child(
            div()
                .text_xs()
                .font_medium()
                .font_family("JetBrains Mono")
                .text_color(theme.primary)
                .child(var_def.name.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(var_def.description.to_string()),
        )
        .on_mouse_down(
            MouseButton::Left,
            move |_event, window: &mut Window, cx: &mut App| {
                entity.update(cx, |app, cx| {
                    if let Some(s) = &mut app.stress_config_dialog {
                        // 在输入框当前光标处插入变量
                        s.payload_input.update(cx, |input, cx| {
                            input.insert(insert_text.to_string(), window, cx);
                        });
                        s.show_variable_picker = false;
                    }
                    cx.notify();
                });
            },
        )
}
