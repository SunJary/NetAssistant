use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::{ActiveTheme as _, Root};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Context, Div, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestAppContext, Window, div, px,
};

/// 复现工具箱输出行结构，验证 Clipboard 图标按钮是否真实渲染（bounds 非零）。
/// 背景：工具箱行内 Clipboard 图标在真实应用中不可见，而消息聊天同组件正常。
struct Host;

fn row(id: &'static str, theme_fg: gpui_kit::Hsla) -> gpui_kit::Stateful<Div> {
    // 与 toolbox_panel::render_output_row 同构：id 化行容器 + 名称 + 值 + Clipboard
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(
            div()
                .w_24()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme_fg)
                .child("MD5"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child("365d167f44e1d0937ddcf4d3d3724fd0"),
        )
        .child(
            Clipboard::new("hash-copy")
                .value("365d167f44e1d0937ddcf4d3d3724fd0")
                .tooltip("Copy"),
        )
}

/// 对照组：无 id 化祖先（与消息聊天消息行同构）
fn plain_row() -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().flex_1().child("plain"))
        .child(Clipboard::new("plain-copy").value("x").tooltip("Copy"))
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .child(row("hash-row", fg))
            .child(plain_row())
    }
}

fn diag(cx: &mut gpui_kit::VisualTestContext, id: &str) {
    cx.update(|window, _cx| {
        match window.try_find(ElementId::Name(id.into())) {
            Some(snap) => {
                let b = snap.bounds();
                eprintln!(
                    "DIAG {id}: visible={} size={}x{} origin=({},{})",
                    snap.visible(),
                    b.size.width,
                    b.size.height,
                    b.origin.x,
                    b.origin.y
                );
                assert!(
                    b.size.width > px(0.) && b.size.height > px(0.) && snap.visible(),
                    "{id} 应渲染为可见元素，bounds={b:?}"
                );
            }
            None => panic!("{id} 未在任何帧中注册（元素根本没渲染）"),
        }
    });
}

#[gpui_kit::test]
fn clipboard_icon_renders_in_toolbox_like_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::component::init);
    let (_, mut cx) = cx.add_window_view(|window, cx| {
        let host = cx.new(|_| Host);
        Root::new(host, window, cx)
    });
    // 多画几帧：Clipboard 的 keyed state 首帧注册后按钮才挂 on_click
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.render_frame(cx);
        });
    }
    diag(&mut cx, "plain-copy");
    diag(&mut cx, "hash-copy");
}
