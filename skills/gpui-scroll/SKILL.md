---
name: "gpui-scroll"
description: "GPUI scrolling patterns and pitfalls (overflow_y_scrollbar). Invoke when implementing scrollable areas, fixing scroll issues (no scrollbar, can't scroll to bottom, content cut off), or using overflow_y_scrollbar/Scrollbar in GPUI framework."
---

# GPUI Scrolling Patterns & Pitfalls

本项目多次踩坑的 GPUI 滚动问题汇总。**实现滚动前必读**,尤其是"陷阱"章节。

## 核心原理

**`overflow_y_scrollbar()` 需要一条完整的高度约束链才能生效。** 如果容器高度是弹性的(flex_1 无固定高度父级),容器会撑开到内容高度,滚动永不触发。

```
窗口 (有高度)
  └─ 祖先 (absolute inset_0 → 继承窗口尺寸, 或 h()/h_full)
      └─ 弹窗/面板 (h() 固定高度)
          └─ 滚动容器 (flex_1 分配剩余高度)
              └─ Scrollable root (size_full 填满滚动容器)
```

**任何一环断裂,flex_1 就分配不到高度,overflow_y_scrollbar 不生效。**

## ⚠️ 关键陷阱:Scrollable root 的 size_full 与 flex_grow 冲突

这是本项目反复踩的坑,**必须理解**。

### Scrollable 的实际源码机制

`overflow_y_scrollbar()` 来自 `gpui-component` 的 `scroll/scrollable.rs`。它**不是**简单加 `overflow_y: scroll`,而是**重新包装**元素:

```rust
// scrollable.rs RenderOnce 实现 (简化)
fn render(mut self, window, cx) -> impl IntoElement {
    let root_style = root_style_from(&mut self.element);  // 1. 提取元素的 size/flex_grow/flex_shrink 等

    let content = self.element
        .id(content_id)
        .flex_none()                                        // 2. content 被设为 flex_none
        .h_auto().min_h_full();                             //    h_auto + min_h_full (不会被压缩!)

    let scroll_area = div()
        .id(area_id)
        .size_full()                                        // 3. scroll-area: size_full
        .flex().flex_col()
        .track_scroll(&scroll_handle)
        .overflow_y_scroll()
        .child(content);                                    //    content 作为 flex_none 子元素

    div()
        .id(root_id)
        .size_full()                                        // 4. root: size_full (!!!)
        .refine_style(&root_style)                          // 5. 把元素的 flex_grow/flex_shrink 挂到 root
        .relative()
        .child(scroll_area)
        .child(render_scrollbar(...))
}
```

### 关键行为表

| 行为 | 说明 |
|------|------|
| root 有 `size_full` | root div 强制 `height: 100%` |
| root 继承元素的 `flex_grow` | 如果你写 `flex_1()`,root 会有 `flex_grow: 1` |
| **size_full 与 flex_grow 冲突** | `height: 100%` 和 `flex_grow: 1` 同时存在,在 flex_col 父级中高度计算异常 |
| content 是 `flex_none + h_auto + min_h_full` | content 不会被 flex 压缩,这是好事 |
| scroll-area 是 `flex_col` | content 作为 flex_none 子元素,自然高度撑开 scrollHeight |

### 冲突的后果

当 `overflow_y_scrollbar` 直接挂在 `flex_1` 元素上时:

```rust
// ⚠️ 危险模式
div()
    .flex_1()                    // 元素 flex_grow: 1
    .overflow_y_scrollbar()      // root 继承 flex_grow: 1, 同时 root 有 size_full
    .child(content)
```

渲染后:
```
<父级 flex_col>
  <root: size_full + flex_grow:1>   ← height:100% 和 flex_grow:1 冲突!
    <scroll-area: size_full + flex_col>
      <content: flex_none + h_auto + min_h_full>
```

**现象**:滚动条可能出现,但**滚不到底**,后面内容被挡住。因为 root 的高度计算异常,导致 scroll-area 的可视高度与实际不符。

## 正确模式:两层包裹结构

**这是本项目验证可用的模式**,来自 gpui-component 源码测试用例 `MaxHeightParentTest`。

```rust
// ✅ 正确:两层包裹
div()
    .h(px(dialog_height))        // 或父级有确定高度
    .overflow_hidden()           // 外层裁剪
    .flex().flex_col()
    .child(header)               // 固定标题 (不滚动)
    .child(
        div()
            .flex_1()             // 外层: flex_1 分配剩余高度
            .overflow_hidden()   // 外层: 裁剪
            .child(
                div()
                    .size_full()              // 内层: 填满外层
                    .overflow_y_scrollbar()   // 内层: 滚动 (root 的 size_full 不再冲突)
                    .px_6().pb_4()            // padding 挂内层
                    .child(content)
            )
    )
    .child(footer)               // 固定底部 (不滚动)
```

### 为什么两层结构能工作?

- 外层 `flex_1 + overflow_hidden`:在 flex_col 中正确分配剩余高度,不带 `overflow_y_scrollbar` 所以无 root 冲突
- 内层 `size_full + overflow_y_scrollbar`:root 的 `size_full` 与外层分配的高度一致(size_full = 100% of 外层),不与 flex_grow 冲突(内层无 flex_1)

### 完整弹窗示例 (本项目 stress_config.rs)

```rust
// Layer 1: 遮罩 (全屏 flex 居中)
div()
    .absolute().inset_0()
    .flex().items_center().justify_center()
    .bg(rgba(0x80000000))
    .p_4()
    .child(
        // Layer 2: 弹窗本体 (固定高度 + 三段式)
        div()
            .w(px(520.0))
            .h(px(dialog_height))     // 关键1: 固定高度
            .overflow_hidden()        // 关键2: 裁剪溢出
            .flex().flex_col()        // 关键3: 垂直三段
            .bg(theme.muted).rounded_lg().shadow_2xl()
            // Layer 3a: 标题 (固定, 不滚动)
            .child(div().px_6().pt_6().pb_4().child("标题"))
            // Layer 3b: 滚动内容区 (两层包裹!)
            .child(
                div()
                    .flex_1().overflow_hidden()      // 外层
                    .child(
                        div()
                            .size_full()              // 内层
                            .overflow_y_scrollbar()
                            .px_6().pb_4()
                            .child(/* 内容块 */)
                            .child(/* 更多内容 */)
                    )
            )
            // Layer 3c: 底部按钮 (固定, 不滚动)
            .child(
                div()
                    .px_6().pb_6().pt_2()
                    .border_t_1().border_color(theme.border)
                    .child(render_actions)
            )
    )
```

## 子元素布局:block + margin vs flex_col + gap

经过 Scrollable 包装后,content 是 scroll-area(flex_col)的 `flex_none` 子元素。**content 内部的布局是自由的**。

- **content 内部用 `flex_col + gap`**:✅ 可用(content 是 flex_none,不会被压缩)
- **content 内部用 block + mt_4 margin**:✅ 也可用

本项目 stress_config.rs 用 `flex_col + gap_1` 在 content 内部组织"目标/报文"等子块,工作正常。**关键不是 content 内部布局,而是滚动容器本身不要直接挂 flex_1 + overflow_y_scrollbar**。

## 常见错误模式与失败现象

### 错误 1: flex_1 + overflow_y_scrollbar 同一元素

```rust
// ❌ 错误: size_full 与 flex_grow 冲突, 滚不到底
div()
    .flex_1()
    .overflow_y_scrollbar()   // root 继承 flex_grow, 同时有 size_full
    .child(content)
```

**现象**:滚动条出现,但滚不到底,后面内容被挡住。

**修复**:改用两层包裹(见上文)。

### 错误 2: 缺少高度约束链

```rust
// ❌ 错误: 父级无高度, flex_1 分配到 0
div()
    .flex().flex_col()
    .child(
        div().flex_1().overflow_y_scrollbar().child(content)
    )
```

**现象**:滚动条不出现,或容器高度为 0。

**修复**:父级加 `h()` 或 `h_full()`,确保高度链完整。

### 错误 3: 祖先缺 overflow_hidden

```rust
// ❌ 错误: 内容视觉溢出弹窗
div()
    .h(px(300.0))
    .flex().flex_col()
    .child(
        div().flex_1().overflow_y_scrollbar().child(content)
    )
```

**现象**:内容溢出弹窗边缘,视觉泄漏。

**修复**:祖先加 `overflow_hidden()`。

### 错误 4: 用 max_h 而非 h

```rust
// ❌ 错误: max_h 允许容器更小, 滚动可能不触发
div().max_h(px(300.0)).overflow_y_scrollbar()

// ✅ 正确: h 固定高度, 内容超出时滚动
div().h(px(300.0)).overflow_y_scrollbar()
```

### 错误 5: 高度计算值异常

```rust
// ⚠️ 注意: window.bounds() 在首次渲染时可能返回异常值
let dialog_height = (window.bounds().size.height / px(1.0)) as f32 * 0.8;
// 建议加最小值保护
let dialog_height = (win_h * 0.8).max(500.0);
```

## 简单滚动区模式 (无需 flex_1)

如果不需要"固定标题 + 滚动内容 + 固定底部"的三段式,直接固定高度滚动:

```rust
// ✅ 简单模式: 固定高度直接滚动
div()
    .h(px(300.0))
    .overflow_y_scrollbar()
    .child(content)
```

此模式无 flex_grow 冲突,直接可用。

## 全高侧边栏模式

```rust
// ✅ 侧边栏: h_full + overflow_y_scrollbar
div()
    .w(px(200.0))
    .h_full()                  // 父级必须有确定高度
    .overflow_y_scrollbar()
    .child(content)
```

**关键**:父级必须有确定高度。若父级是 `flex_1`,确保祖父级有固定或确定高度。

## 调试方法

### 1. 打印高度值确认约束链

```rust
let win_h = (window.bounds().size.height / px(1.0)) as f32;
log::info!("[debug] win_h={}, dialog_height={}", win_h, dialog_height);
```

如果 `win_h=0` 或异常小,说明 `window.bounds()` 在该渲染时机未就绪,加最小值保护。

### 2. 检查滚动条是否出现

- 无滚动条 → 高度约束链断裂,flex_1 没分配到高度
- 有滚动条但滚不到底 → size_full 与 flex_grow 冲突,改两层包裹

### 3. 验证顺序

1. 确认弹窗/容器有固定 `h()` 值(非 0)
2. 确认祖先有 `overflow_hidden`
3. 确认滚动容器用两层包裹(外层 flex_1+overflow_hidden,内层 size_full+overflow_y_scrollbar)
4. 确认内容块不被 flex_shrink 压缩(content 内部用 flex_col+gap 或 block+margin 都可)

## 必需导入

```rust
use gpui_component::scroll::ScrollableElement;              // overflow_y_scrollbar()
use gpui_component::scroll::{Scrollbar, ScrollbarShow};     // 自定义 Scrollbar
```

## 快速参考表

| 场景 | 高度 | Overflow | Scrollbar | 备注 |
|------|------|----------|-----------|------|
| 简单滚动框 | `h(px(N))` | — | `overflow_y_scrollbar()` | 无冲突,直接用 |
| 标题+滚动+底部三段式 | 外层 `h()` + `overflow_hidden` | 外层 hidden | **两层包裹**(外层 flex_1+hidden, 内层 size_full+scrollbar) | ⚠️ 关键模式 |
| 全高侧边栏 | `h_full()` | — | `overflow_y_scrollbar()` | 父级需有高度 |
| 文本裁剪 | `max_w()` | `overflow_x_hidden()` | — | 不滚动 |

## 源码参考

- `gpui-component` Scrollable 实现: `~/.cargo/git/checkouts/gpui-component-*/crates/ui/src/scroll/scrollable.rs`
- 关键测试用例: `MaxHeightParentTest` (两层包裹模式的权威参考)
- 本项目可用实现:
  - `src/ui/dialog/stress_config.rs` - 弹窗三段式 + 两层包裹滚动
  - `src/ui/dialog/favorite_list.rs` - 固定高度弹窗 + flex_1 滚动
  - `src/ui/main_window.rs` - 全高侧边栏滚动
