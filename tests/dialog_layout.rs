use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Root, WindowExt, v_flex};
/// 对话框滚动布局的无头测试：复现 Dialog 组件内「auto 高度面板 + max_h 封顶 + 滚动容器」
/// 的精确结构，验证内容超高时滚动容器被 clamp 且可滚动。
use gpui_kit::{
    AppContext as _, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, ScrollDelta, ScrollWheelEvent, Styled as _, TestAppContext, VisualTestContext, Window,
    div, point, px,
};

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn scroll(cx: &mut VisualTestContext, x: f32, y: f32, dx: f32, dy: f32) {
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(x), px(y)),
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        ..Default::default()
    });
    draw(cx);
}

fn row(selector: String, height: f32) -> Div {
    div()
        .h(px(height))
        .flex_shrink_0()
        .debug_selector(move || selector.clone())
}

/// 复现迁移后对话框的完整结构：
/// Dialog 面板(auto + max_h=480) / title / DialogContent / 滚动容器(max_h=352) / footer
struct DialogHost {
    opened: bool,
}

impl DialogHost {
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, |dialog, window, _cx| {
            let content_cap = px(352.);
            dialog
                .title("Scroll Test")
                .max_h(px(480.))
                // 与生产配置一致: 点击蒙层关闭弹窗
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new().child(
                        div()
                            .h(px(28.))
                            .w(px(60.))
                            .debug_selector(|| "footer-btn".to_string())
                            .child("OK"),
                    ),
                )
                .content(move |content, _window, _cx| {
                    content.child(
                        // 正确结构: max_h 在外层普通 div(钳制可视区), 滚动容器自身不限高
                        // (内容自然撑高 -> 滚动区内容 > 可视区 -> 滚轮生效)
                        div().max_h(content_cap).child(
                            div()
                                .overflow_y_scrollbar()
                                .debug_selector(|| "content-wrap".to_string())
                                .child(
                                    v_flex().children(
                                        (0..20)
                                            .map(|i| row(format!("row-{}", i), 40.))
                                            .collect::<Vec<_>>(),
                                    ),
                                ),
                        ),
                    )
                })
                .on_cancel(|_, _, _| true)
        });
    }
}

impl Render for DialogHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.opened {
            self.opened = true;
            self.open(window, cx);
        }
        // gpui-base::Root 在 0.7.0 中自动挂载 Dialog/Sheet/Tooltip 等浮层,
        // 内部视图无需手动渲染 dialog_layer
        div().size_full()
    }
}

#[gpui_kit::test]
fn dialog_content_with_max_h_clamps_and_scrolls(cx: &mut TestAppContext) {
    cx.update(gpui_kit::component::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let host = cx.new(|cx| DialogHost { opened: false });
        Root::new(host, window, cx)
    });
    let mut cx = cx;
    // 第一帧打开对话框, 多画几帧让 Dialog 层完成渲染
    draw(&mut cx);
    draw(&mut cx);
    draw(&mut cx);

    // 诊断输出: 各层实际 bounds
    for key in ["content-wrap", "row-0", "row-19", "footer-btn"] {
        if let Some(b) = cx.debug_bounds(key) {
            eprintln!(
                "DIAG {key}: origin=({},{}) size={}x{}",
                b.origin.x, b.origin.y, b.size.width, b.size.height
            );
        } else {
            eprintln!("DIAG {key}: <no bounds>");
        }
    }

    let row0 = cx.debug_bounds("row-0").expect("row-0 bounds");
    let row19 = cx.debug_bounds("row-19").expect("row-19 bounds");
    let footer = cx.debug_bounds("footer-btn").expect("footer bounds");

    // 面板被 max_h=480 封顶：footer 仍在窗口内（y < 480 + dialog top 偏移）
    assert!(
        footer.size.height > px(0.),
        "footer should be visible, bounds={footer:?}"
    );

    // 20 行 x 40px = 800px 内容，滚动容器应被 clamp 到 352，
    // row-19 应被裁剪到可视区外（其布局位置应在滚动容器起点 + 352 之外），
    // 即 row-19.top - row-0.top = 760 > 352，且 row-19 实际不可见（超容器）。
    let content_span = row19.origin.y - row0.origin.y;
    assert!(
        (content_span - px(760.)).abs() < px(2.),
        "rows keep layout positions, span={content_span:?}"
    );

    // 滚动后 row-0 上移（说明滚动生效）；滚动点取内容区内部（行 x=753 起）
    scroll(&mut cx, 900., 300., 0., -100.);
    let row0_after = cx.debug_bounds("row-0").expect("row-0 after scroll");
    assert!(
        row0_after.origin.y < row0.origin.y,
        "content should scroll, before={row0:?} after={row0_after:?}"
    );
}

/// ESC 关闭 / 点击蒙层关闭的交互行为测试
#[gpui_kit::test]
fn dialog_esc_and_overlay_click_both_close(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent};

    cx.update(gpui_kit::component::init);
    let (_, mut cx) = cx.add_window_view(|window, cx| {
        let host = cx.new(|cx| DialogHost { opened: false });
        Root::new(host, window, cx)
    });
    let host = cx.update(|window, cx| {
        let root = window
            .root::<Root>()
            .unwrap_or_else(|| panic!("no root view"))
            .unwrap_or_else(|| panic!("root type mismatch"));
        root.read(cx)
            .view()
            .clone()
            .downcast::<DialogHost>()
            .unwrap()
    });
    draw(&mut cx);
    draw(&mut cx);
    draw(&mut cx);

    let is_open = |cx: &mut VisualTestContext| cx.update(|window, cx| window.has_active_dialog(cx));
    assert!(is_open(&mut cx), "dialog should be open");

    // 点击面板外的蒙层: 关闭弹窗(overlay_closable 默认 true)
    cx.simulate_event(MouseDownEvent {
        position: point(px(60.), px(700.)),
        button: MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(&mut cx);
    assert!(!is_open(&mut cx), "overlay click should close the dialog");

    // 重开对话框, 验证 ESC 关闭(keyboard 默认开启, 打开时自动聚焦)
    cx.update(|_, cx| host.update(cx, |h, _| h.opened = false));
    draw(&mut cx);
    assert!(is_open(&mut cx), "dialog should reopen");
    cx.simulate_keystrokes("escape");
    draw(&mut cx);
    assert!(!is_open(&mut cx), "ESC should close the dialog");
}

/// T-2：「回复规则」管理弹窗的结构复现 —— 固定顶部区（总开关）+ 规则列表滚动区 + 底部按钮条。
///
/// 规则条数现实中可达数十条，本用例用 30 行验证两件事：
/// 1. 长列表被 `max_h` 钳制且**可滚动**（不会把底部按钮挤出窗口）；
/// 2. 底部按钮始终留在窗口内（布局不溢出）。
struct ReplyRulesDialogHost {
    opened: bool,
}

impl ReplyRulesDialogHost {
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, |dialog, _window, _cx| {
            let content_cap = px(352.);
            dialog
                .title("Reply Rules")
                .w(px(760.))
                .max_h(px(480.))
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new().child(
                        div()
                            .h(px(28.))
                            .w(px(60.))
                            .debug_selector(|| "rr-footer-ok".to_string())
                            .child("OK"),
                    ),
                )
                .content(move |content, _window, _cx| {
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            // 固定顶部区（总开关/提示）
                            .child(
                                div()
                                    .h(px(32.))
                                    .flex_shrink_0()
                                    .debug_selector(|| "rr-top".to_string())
                                    .child("master switch"),
                            )
                            // 规则列表：max_h 在外层普通 div 钳制可视区，内层滚动容器不限高
                            .child(
                                div().max_h(content_cap).child(
                                    div()
                                        .overflow_y_scrollbar()
                                        .debug_selector(|| "rr-list".to_string())
                                        .child(
                                            v_flex().children(
                                                (0..30)
                                                    .map(|i| row(format!("rr-row-{}", i), 40.))
                                                    .collect::<Vec<_>>(),
                                            ),
                                        ),
                                ),
                            ),
                    )
                })
                .on_cancel(|_, _, _| true)
        });
    }
}

impl Render for ReplyRulesDialogHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.opened {
            self.opened = true;
            self.open(window, cx);
        }
        div().size_full()
    }
}

#[gpui_kit::test]
fn reply_rules_dialog_long_list_clamps_scrolls_and_keeps_footer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::component::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let host = cx.new(|cx| ReplyRulesDialogHost { opened: false });
        Root::new(host, window, cx)
    });
    let mut cx = cx;
    draw(&mut cx);
    draw(&mut cx);
    draw(&mut cx);

    let row0 = cx.debug_bounds("rr-row-0").expect("rr-row-0 bounds");
    let row29 = cx.debug_bounds("rr-row-29").expect("rr-row-29 bounds");
    let footer = cx.debug_bounds("rr-footer-ok").expect("footer bounds");
    let top = cx.debug_bounds("rr-top").expect("top bounds");

    // 布局位置保留（30 行 x 40px，跨度 29*40 = 1160），说明列表区确实被钳成可视高度
    let content_span = row29.origin.y - row0.origin.y;
    assert!(
        (content_span - px(1160.)).abs() < px(2.),
        "rows keep layout positions, span={content_span:?}"
    );

    // 底部按钮条在窗口内（不溢出）
    let win_h = cx.update(|window, _| window.bounds().size.height);
    assert!(
        footer.origin.y + footer.size.height <= win_h,
        "footer must stay inside the window: footer={footer:?} win_h={win_h:?}"
    );
    // 顶部固定区与列表区不重叠
    assert!(
        row0.origin.y >= top.origin.y + top.size.height - px(1.),
        "list must start below the fixed top section"
    );

    // 滚动生效：row-0 上移
    scroll(&mut cx, 900., 300., 0., -100.);
    let row0_after = cx.debug_bounds("rr-row-0").expect("rr-row-0 after scroll");
    assert!(
        row0_after.origin.y < row0.origin.y,
        "list should scroll, before={row0:?} after={row0_after:?}"
    );
}
