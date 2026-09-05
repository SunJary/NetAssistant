//! 压测面板滚动 - 无头布局测试
//!
//! 复刻 main_window → tab_container → connection_tab → stress_panel 的完整
//! 嵌套链, 用 debug_bounds 逐层测量高度, 并模拟滚轮验证滚动是否生效。
//! 目的: 定位高度约束链断点 (滚动失效 = 某层被内容撑开, scroll_max 恒为 0)。

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, ScrollDelta, ScrollWheelEvent,
    Size, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use gpui_component::scroll::ScrollableElement;

const WINDOW_SIZE: (f32, f32) = (1000., 500.);

/// 与线上结构逐层一致的复刻 (高度均为固定值, 便于精确断言)
struct StressChainTest;

impl Render for StressChainTest {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // main_window 根
        div()
            .size_full()
            .flex()
            .flex_col()
            // 标题栏
            .child(div().h(px(32.)).flex_shrink_0())
            // main_window 内容行: flex + flex_1 + overflow_hidden
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .debug_selector(|| "content-row".to_string())
                    // 侧边栏 + 调整手柄
                    .child(div().w(px(200.)).h_full())
                    .child(div().w(px(16.)).h_full())
                    // 右侧内容区
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_x_hidden()
                            .debug_selector(|| "right-side".to_string())
                            // TabContainer 根 (min_h_0: 解除 min-content 钉死, 允许收缩)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .debug_selector(|| "tabc-root".to_string())
                                    // tab 头
                                    .child(div().h(px(28.)).flex_shrink_0())
                                    // tab 内容区 (min_h_0: 同上)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_h_0()
                                            .debug_selector(|| "tab-content".to_string())
                                            // ConnectionTab 根 (min_h_0: 解除钉死)
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .flex_1()
                                                    .min_h_0()
                                                    .debug_selector(|| "tab-row".to_string())
                                                    // 左侧连接信息
                                                    .child(div().w(px(200.)).h_full())
                                                    // render_right_panel (min_h_0: 高度轴同样解除钉死)
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .min_h_0()
                                                            .debug_selector(|| {
                                                                "right-panel".to_string()
                                                            })
                                                            // 调试/压测 tab 切换栏
                                                            .child(
                                                                div().h(px(32.)).flex_shrink_0(),
                                                            )
                                                            // StressPanel 根
                                                            .child(
                                                                div()
                                                                    .flex()
                                                                    .flex_col()
                                                                    .flex_1()
                                                                    .min_h_0()
                                                                    .w_full()
                                                                    .debug_selector(|| {
                                                                        "stress-root".to_string()
                                                                    })
                                                                    // 控制条
                                                                    .child(
                                                                        div()
                                                                            .h(px(36.))
                                                                            .flex_shrink_0(),
                                                                    )
                                                                    // 状态条
                                                                    .child(
                                                                        div()
                                                                            .h(px(28.))
                                                                            .flex_shrink_0(),
                                                                    )
                                                                    // 指标内容区: 外层 flex_1 + overflow_hidden
                                                                    .child(
                                                                        div()
                                                                            .flex_1()
                                                                            .overflow_hidden()
                                                                            .debug_selector(
                                                                                || "outer"
                                                                                    .to_string(),
                                                                            )
                                                                            // 内层滚动容器
                                                                            .child(
                                                                                div()
                                                                                    .size_full()
                                                                                    .overflow_y_scrollbar()
                                                                                    .flex()
                                                                                    .flex_col()
                                                                                    .gap(px(16.))
                                                                                    .children((0..8).map(|i| {
                                                                                        div()
                                                                                            .h(px(100.))
                                                                                            .flex_shrink_0()
                                                                                            .when(i == 7, |d| {
                                                                                                d.debug_selector(|| "last-card".to_string())
                                                                                            })
                                                                                    })),
                                                                            ),
                                                                    ),
                                                            ),
                                                    ),
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}

use gpui::prelude::FluentBuilder as _;

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn scroll(cx: &mut VisualTestContext, x: gpui::Pixels, y: gpui::Pixels, dx: f32, dy: f32) {
    cx.simulate_event(ScrollWheelEvent {
        position: point(x, y),
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        ..Default::default()
    });
    draw(cx);
}

#[gpui::test]
fn stress_chain_layout_and_scroll(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|_, _| StressChainTest);
    let cx: &mut VisualTestContext = cx;
    cx.simulate_resize(Size::new(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)));
    draw(cx);

    // 逐层打印高度
    for name in [
        "content-row",
        "right-side",
        "tabc-root",
        "tab-content",
        "tab-row",
        "right-panel",
        "stress-root",
        "outer",
    ] {
        println!("{name:>12}: {:?}", cx.debug_bounds(name));
    }

    let outer = cx.debug_bounds("outer").expect("outer bounds");
    // 链条固定高度: 500 - 32(标题) - 28(tab头) - 32(视图tab) - 36(控制条) - 28(状态条)
    let expected_outer_h = WINDOW_SIZE.1 - 32. - 28. - 32. - 36. - 28.;
    assert!(
        (outer.size.height - px(expected_outer_h)).abs() < px(1.),
        "outer 应获得确定高度 {expected_outer_h}, 实际 {:?} — 若远大于此值说明高度约束链断裂",
        outer.size.height
    );

    // 内容 8×100 + 7×16 = 912 > outer, 应溢出
    let last = cx.debug_bounds("last-card").expect("last-card bounds");
    println!("outer: {outer:?}\nlast-card: {last:?}");
    assert!(
        last.bottom() > outer.bottom(),
        "初始状态内容应溢出容器 (last.bottom={} > outer.bottom={})",
        last.bottom(),
        outer.bottom()
    );

    // 模拟滚轮: 内容应上移
    let center = outer.center();
    let before = cx.debug_bounds("last-card").unwrap().origin.y;
    scroll(cx, center.x, center.y, 0., -300.);
    let after = cx.debug_bounds("last-card").unwrap().origin.y;
    println!("last-card y: before={before:.1} after={after:.1}");
    assert!(
        after < before - px(100.),
        "滚轮下滚应使内容上移 (before={before:.1}, after={after:.1}) — 未移动说明滚动未生效"
    );
}
