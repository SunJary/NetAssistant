//! gpui-fast 子树复用机制的机制级验证。
//!
//! 背景：gpui-fast 按实体脏标记复用未变化的渲染子树（连旧帧的事件监听器一起）；
//! 普通 gpui 的测试 `window.draw()` 无条件全量重绘，会掩盖「改实体不 notify」的契约违反。
//! 本测试直接观测 render 计数，锁定 gpui-fast 的复用语义：
//!
//! - 改实体字段但不 `cx.notify()` → render 不重跑（子树复用）
//! - `cx.notify()` 后 → render 必须重跑
//!
//! 注意：第 1 条断言只在 gpui-fast 下成立；若将来切换回普通 gpui（每次 draw 全量重绘），
//! 该断言应随实现调整。
use gpui_kit::{
    AppContext as _, Context, IntoElement, Render, TestAppContext, VisualTestContext, Window, div,
};
use std::cell::Cell;
use std::rc::Rc;

struct RenderCounter {
    flag: bool,
    frames: Rc<Cell<usize>>,
}

impl Render for RenderCounter {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.frames.set(self.frames.get() + 1);
        div()
    }
}

#[gpui_kit::test]
fn subtree_reused_until_entity_notified(cx: &mut TestAppContext) {
    cx.update(gpui_kit::component::init);
    let frames = Rc::new(Cell::new(0));
    let (_, mut cx) = cx.add_window_view({
        let frames = frames.clone();
        move |window, cx| {
            let counter = cx.new(|_cx| RenderCounter {
                flag: false,
                frames,
            });
            gpui_kit::component::Root::new(counter, window, cx)
        }
    });
    let draw = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    };
    let counter = cx.update(|window, cx| {
        let root = window
            .root::<gpui_kit::component::Root>()
            .unwrap()
            .unwrap();
        root.read(cx)
            .view()
            .clone()
            .downcast::<RenderCounter>()
            .unwrap()
    });

    draw(&mut cx);
    assert_eq!(frames.get(), 1, "首帧应恰好 render 一次");

    // 改实体但不 notify：gpui-fast 复用子树，render 不重跑。
    // （普通 gpui 下此处会重跑 render，断言将失败——见文件头说明）
    counter.update(cx, |v, _cx| v.flag = !v.flag);
    draw(&mut cx);
    assert_eq!(
        frames.get(),
        1,
        "未 cx.notify() 时 gpui-fast 应复用子树，render 不得重跑"
    );

    // cx.notify() 后必须重跑 render——GPUI 契约本体
    counter.update(cx, |v, cx| {
        v.flag = !v.flag;
        cx.notify();
    });
    draw(&mut cx);
    assert_eq!(frames.get(), 2, "cx.notify() 后必须重跑 render");
}
