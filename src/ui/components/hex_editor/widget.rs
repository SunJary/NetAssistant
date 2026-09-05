//! 十六进制编辑器 —— GPUI 渲染层
//!
//! 仅依赖 gpui 与 core；不 import gpui_component / 项目内类型，
//! 主题色经 [`HexEditorStyle`] 注入。布局规格见 plans/plan-hex-editor.md。

use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::core::{self, Action, Cell, MoveDir};

/// 每行高度
const ROW_HEIGHT: f32 = 20.0;
/// 单个 nibble 字符格宽度（JetBrains Mono 12px 约 7.2px，留余量）
const NIBBLE_WIDTH: f32 = 8.0;
/// 字节组之间的间隔
const BYTE_GAP: f32 = 4.0;

/// 内联视图默认每行字节数（仅测试用默认构造；生产接入点一律显式指定字节数）
#[cfg(test)]
pub const DEFAULT_INLINE_BYTES_PER_ROW: usize = 16;

/// 编辑器组件状态（GPUI Entity 包装）
pub struct HexEditorState {
    pub core: core::State,
    /// 上次同步的输入框值；值变化时才重新解析（缓存，防每帧重解析）
    pub parsed_from: Option<String>,
    pub focus: FocusHandle,
    /// 一次性聚焦请求：打开对话框等挂载时机不可控的场景，
    /// 由持有 &mut Window 的渲染闭包（如对话框 content）消费后置回 false
    pub wants_focus: bool,
    /// 内联紧凑视图每行字节数（同时是上下导航的步长），按接入点在创建时设定
    pub inline_bytes_per_row: usize,
}

impl HexEditorState {
    /// 默认构造（仅测试用；生产接入点请用 [`Self::with_inline_bytes_per_row`] 显式指定）
    #[cfg(test)]
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::with_inline_bytes_per_row(cx, DEFAULT_INLINE_BYTES_PER_ROW)
    }

    /// 指定内联视图每行字节数创建（自动回复等窄面板接入点用）
    pub fn with_inline_bytes_per_row(cx: &mut Context<Self>, bytes_per_row: usize) -> Self {
        Self {
            core: core::State::default(),
            parsed_from: None,
            focus: cx.focus_handle().tab_stop(true),
            wants_focus: false,
            inline_bytes_per_row: bytes_per_row.max(1),
        }
    }
}

/// 内容写回回调（由适配层注入，避免 widget 层依赖 gpui_component）
pub type WriteBack = Arc<dyn Fn(String, &mut Window, &mut App) + 'static>;

/// 主题色（适配层从 Theme 映射，widget 不感知具体主题类型）
#[derive(Debug, Clone)]
pub struct HexEditorStyle {
    pub text: Hsla,
    pub muted: Hsla,
    pub border: Hsla,
    /// 聚焦时的光标底色（实心高亮）
    pub cursor_bg: Hsla,
    /// 未聚焦时的光标底色（主色半透明, 保证"这里可输入"可见）
    pub cursor_bg_unfocused: Hsla,
    pub cursor_text: Hsla,
    pub selection: Hsla,
    pub token_bg: Hsla,
    pub token_text: Hsla,
}

/// 视图配置：内联紧凑视图与展开编辑器共用同一渲染核心，仅参数不同
#[derive(Debug, Clone)]
pub struct HexViewConfig {
    /// 每行字节数（内联 8 / 展开 16），也是上下移动的步长
    pub bytes_per_row: usize,
    /// 偏移列位数（内联 4 / 展开 8）
    pub offset_digits: usize,
    pub show_ascii: bool,
    /// 展开编辑器为 true：键入可打印字符直接设置字节
    pub ascii_editable: bool,
    /// 最多渲染行数（内联限制行数用），超出部分显示 overflow_notice
    pub max_rows: Option<usize>,
    /// 滚动容器元素 id（每个实例需唯一）
    pub view_key: SharedString,
    pub empty_hint: Option<SharedString>,
    pub overflow_notice: Option<SharedString>,
    pub truncated_notice: Option<SharedString>,
}

/// 渲染十六进制网格。
///
/// 返回的元素自身**不限制高度**（滚动结构铁律：高度约束由外层普通 div 提供，
/// 本函数返回外层键盘焦点容器 + 内层滚动容器，外层由调用方用 flex_1/min_h_0 或定高包裹）。
pub fn render_grid(
    editor: &Entity<HexEditorState>,
    config: &HexViewConfig,
    style: &HexEditorStyle,
    window: &Window,
    cx: &App,
    on_write: WriteBack,
) -> Div {
    let focused = editor.read(cx).focus.is_focused(window);
    let state = editor.read(cx);
    let Some(doc) = state.core.doc.as_ref() else {
        // 解析失败不会走到这里（适配层已回退文本框），兜底渲染空容器
        return div();
    };

    let cells = &doc.cells;
    let len = cells.len();
    // 光标常显: 从未点击时默认定位到首字节(键入/导航同样从该处生效),
    // 让"这里可以输入"一眼可见
    let (cursor_cell, cursor_nibble) = match state.core.cursor {
        Some(c) => (Some(c.cell), c.nibble),
        None => (Some(0), 0),
    };
    let selection = state.core.selection;
    let stride = config.bytes_per_row.max(1);
    let total_rows = len.div_ceil(stride);
    let mut rendered_rows = config
        .max_rows
        .map(|m| m.min(total_rows))
        .unwrap_or(total_rows);
    // 光标在虚拟末尾(内容尾部的空位)且恰好压在行边界时, 该位置属于新的一行:
    // 补渲染一行, 保证末尾光标格可见
    let cursor_at_row_boundary_end = cursor_cell == Some(len) && len % stride == 0 && len > 0;
    if cursor_at_row_boundary_end {
        rendered_rows += 1;
    }
    // 内容行是否因 max_rows 被裁剪(决定是否显示"展开查看全部"提示)
    let clipped = rendered_rows < total_rows;

    let mut scroll = div()
        .id(config.view_key.clone())
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .font_family("JetBrains Mono")
        .text_xs();

    if let Some(notice) = config.truncated_notice.as_ref() {
        if doc.truncated {
            scroll = scroll.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(style.muted)
                    .child(notice.clone()),
            );
        }
    }

    if len == 0 {
        // 空内容：渲染「幻影光标格」——光标块本身可见可点，"这里可以输入"一眼可辨；
        // 键入/点击都从 (0,0) 落位（core 对 len==0 的 resolved_cursor/click 语义一致）
        let offset_width = px(config.offset_digits as f32 * NIBBLE_WIDTH + 8.0);
        let mut empty_row = div()
            .h(px(ROW_HEIGHT))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .debug_selector(|| "hex-phantom-cursor".to_string())
            .child(
                div()
                    .w(offset_width)
                    .flex_none()
                    .text_color(style.muted)
                    .px_1()
                    .mr_2()
                    .border_r_1()
                    .border_color(style.border)
                    .child(format!("{:01$X}", 0, config.offset_digits)),
            )
            .child(
                div()
                    .w(px(NIBBLE_WIDTH))
                    .h(px(ROW_HEIGHT))
                    .flex_none()
                    .when(focused, |d| d.bg(style.cursor_bg))
                    .when(!focused, |d| d.bg(style.cursor_bg_unfocused))
                    .on_mouse_down(MouseButton::Left, {
                        let editor = editor.clone();
                        let on_write = on_write.clone();
                        move |_, window, cx| {
                            click_cell(&editor, 0, 0, &on_write, window, cx);
                        }
                    }),
            );
        if let Some(hint) = config.empty_hint.as_ref() {
            empty_row = empty_row.child(
                div()
                    .ml_2()
                    .text_xs()
                    .text_color(style.muted)
                    .child(hint.clone()),
            );
        }
        scroll = scroll.child(empty_row);
    } else {
        for row in 0..rendered_rows {
            scroll = scroll.child(render_row(
                row * stride,
                cells,
                config,
                style,
                focused,
                cursor_cell,
                cursor_nibble,
                selection,
                editor,
                &on_write,
            ));
        }
        if clipped {
            if let Some(notice) = config.overflow_notice.as_ref() {
                scroll = scroll.child(
                    div()
                        .h(px(ROW_HEIGHT))
                        .flex()
                        .items_center()
                        .px_2()
                        .text_xs()
                        .text_color(style.muted)
                        .child(notice.clone()),
                );
            }
        }
    }

    // 外层：键盘焦点容器。on_key_down 仅在焦点位于编辑器内时触发。
    let container_focus = editor.read(cx).focus.clone();
    let ascii_editable = config.ascii_editable;
    div()
        .track_focus(&container_focus)
        .on_key_down({
            let editor = editor.clone();
            let on_write = on_write.clone();
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                handle_key(
                    &editor,
                    event,
                    &on_write,
                    ascii_editable,
                    stride,
                    window,
                    cx,
                );
            }
        })
        // 点击空白处也能聚焦（单元格点击各自处理聚焦与定位）
        .on_mouse_down(MouseButton::Left, {
            let focus = editor.read(cx).focus.clone();
            move |_, window, cx| focus.focus(window, cx)
        })
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .min_h_0()
        .child(scroll)
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    start: usize,
    cells: &[Cell],
    config: &HexViewConfig,
    style: &HexEditorStyle,
    focused: bool,
    cursor_cell: Option<usize>,
    cursor_nibble: usize,
    selection: Option<(usize, usize)>,
    editor: &Entity<HexEditorState>,
    on_write: &WriteBack,
) -> Div {
    let stride = config.bytes_per_row;
    let offset_text = format!("{:01$X}", start, config.offset_digits);
    let offset_width = px(config.offset_digits as f32 * NIBBLE_WIDTH + 8.0);

    let mut row = div()
        .h(px(ROW_HEIGHT))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .debug_selector(move || format!("hex-row-{}", start))
        .child(
            // 偏移列: 行首字节序号(标尺), 用分隔线与数据区隔开避免误读为内容前缀
            div()
                .w(offset_width)
                .flex_none()
                .text_color(style.muted)
                .px_1()
                .mr_2()
                .border_r_1()
                .border_color(style.border)
                .child(offset_text),
        );

    // hex 区
    let mut hex_area = div().flex().flex_row().items_center();
    for slot in 0..stride {
        let idx = start + slot;
        if idx >= cells.len() {
            // 空槽占位，保证行对齐。内容末尾的空位即"虚拟末尾"光标位置：
            // 渲染光标块(否则导航到末尾时光标消失)，点击定位到末尾、键入即追加
            let on_end = cursor_cell == Some(idx) && cursor_nibble == 0;
            let editor = editor.clone();
            let on_write = on_write.clone();
            hex_area = hex_area.child(
                div()
                    .w(px(NIBBLE_WIDTH * 2.0 + BYTE_GAP))
                    .h(px(ROW_HEIGHT))
                    .flex_none()
                    .debug_selector(move || format!("hex-cell-{}-{}", idx, 0))
                    .when(on_end && focused, |d| d.bg(style.cursor_bg))
                    .when(on_end && !focused, |d| d.bg(style.cursor_bg_unfocused))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        click_cell(&editor, idx, 0, &on_write, window, cx);
                    }),
            );
            continue;
        }
        match &cells[idx] {
            Cell::Byte { hi, lo } => {
                let selected = is_selected(selection, idx);
                let hi_cursor = cursor_cell == Some(idx) && cursor_nibble == 0;
                let lo_cursor = cursor_cell == Some(idx) && cursor_nibble == 1;
                let lo_char = if *lo == core::HALF_EMPTY {
                    None
                } else {
                    Some(*lo)
                };
                // 字节组之间的间隙：点击定位到下一格(末尾空位则落到虚拟末尾)
                let gap_editor = editor.clone();
                let gap_on_write = on_write.clone();
                hex_area = hex_area
                    .child(byte_span(
                        Some(*hi),
                        false,
                        hi_cursor,
                        selected,
                        focused,
                        style,
                        editor,
                        on_write,
                        idx,
                        0,
                    ))
                    .child(byte_span(
                        lo_char,
                        lo_char.is_none(),
                        lo_cursor,
                        selected,
                        focused,
                        style,
                        editor,
                        on_write,
                        idx,
                        1,
                    ))
                    .child(
                        div()
                            .w(px(BYTE_GAP))
                            .h(px(ROW_HEIGHT))
                            .flex_none()
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                click_cell(&gap_editor, idx + 1, 0, &gap_on_write, window, cx);
                            }),
                    );
            }
            Cell::Token(text) => {
                let selected = is_selected(selection, idx);
                let on_token = cursor_cell == Some(idx);
                let mut token = div()
                    .flex_none()
                    .mr(px(BYTE_GAP))
                    .px_1()
                    .rounded_sm()
                    .max_w(px(150.0))
                    .truncate()
                    .when(on_token && focused, |d| {
                        d.bg(style.cursor_bg).text_color(style.cursor_text)
                    })
                    .when(on_token && !focused, |d| d.bg(style.cursor_bg_unfocused))
                    .when(!on_token && selected, |d| d.bg(style.selection))
                    .text_color(if on_token && focused {
                        style.cursor_text
                    } else {
                        style.token_text
                    })
                    .bg(if (on_token && !focused) || selected {
                        gpui::transparent_black()
                    } else {
                        style.token_bg
                    })
                    .child(text.clone());
                token = token
                    .on_mouse_down(MouseButton::Left, {
                        let editor = editor.clone();
                        let on_write = on_write.clone();
                        move |_, window, cx| {
                            click_cell(&editor, idx, 0, &on_write, window, cx);
                        }
                    })
                    .on_mouse_move(drag_handler(editor, on_write, idx));
                hex_area = hex_area.child(token);
            }
        }
    }
    row = row.child(hex_area);

    // ASCII 区
    if config.show_ascii {
        let mut ascii_area = div()
            .flex()
            .flex_row()
            .items_center()
            .ml_2()
            .pl_2()
            .border_l_1()
            .border_color(style.border);
        for slot in 0..stride {
            let idx = start + slot;
            if idx >= cells.len() {
                ascii_area = ascii_area.child(div().w(px(NIBBLE_WIDTH)).flex_none());
                continue;
            }
            let ch = cells[idx].to_ascii_char();
            let selected = is_selected(selection, idx);
            let on_cell = cursor_cell == Some(idx);
            let mut span = div()
                .w(px(NIBBLE_WIDTH))
                .flex_none()
                .h(px(ROW_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .text_color(style.muted)
                .when(selected, |d| d.bg(style.selection))
                .when(on_cell && focused, |d| {
                    d.bg(style.cursor_bg).text_color(style.cursor_text)
                })
                .when(on_cell && !focused, |d| d.bg(style.cursor_bg_unfocused))
                .child(match ch {
                    Some(c) => c.to_string(),
                    None => "·".to_string(),
                });
            span = span
                .on_mouse_down(MouseButton::Left, {
                    let editor = editor.clone();
                    let on_write = on_write.clone();
                    move |_, window, cx| {
                        click_cell(&editor, idx, 0, &on_write, window, cx);
                    }
                })
                .on_mouse_move(drag_handler(editor, on_write, idx));
            ascii_area = ascii_area.child(span);
        }
        row = row.child(ascii_area);
    }

    row
}

/// 单个 nibble 字符格
#[allow(clippy::too_many_arguments)]
fn byte_span(
    ch: Option<char>,
    half_slot: bool,
    cursor: bool,
    selected: bool,
    focused: bool,
    style: &HexEditorStyle,
    editor: &Entity<HexEditorState>,
    on_write: &WriteBack,
    cell: usize,
    nibble: usize,
) -> Div {
    let text = match (ch, half_slot) {
        (Some(c), _) => c.to_string(),
        (None, true) => "‹".to_string(), // 半字节空槽：提示此处缺一个 nibble
        (None, false) => String::new(),
    };
    // 基础文字色先写,光标态在后面覆盖(反白生效);非光标态的空槽提示符用 muted 弱化
    let base_color = if half_slot && !cursor {
        style.muted
    } else {
        style.text
    };
    let mut span = div()
        .w(px(NIBBLE_WIDTH))
        .flex_none()
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .debug_selector(move || format!("hex-cell-{}-{}", cell, nibble))
        .text_color(base_color)
        .when(cursor && focused, |d| {
            d.bg(style.cursor_bg).text_color(style.cursor_text)
        })
        .when(cursor && !focused, |d| d.bg(style.cursor_bg_unfocused))
        .when(!cursor && selected, |d| d.bg(style.selection))
        .child(text);
    span = span
        .on_mouse_down(MouseButton::Left, {
            let editor = editor.clone();
            let on_write = on_write.clone();
            move |_, window, cx| {
                click_cell(&editor, cell, nibble, &on_write, window, cx);
            }
        })
        .on_mouse_move(drag_handler(editor, on_write, cell));
    span
}

fn drag_handler(
    editor: &Entity<HexEditorState>,
    on_write: &WriteBack,
    cell: usize,
) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static {
    let editor = editor.clone();
    let _ = on_write;
    move |event: &MouseMoveEvent, _window, cx| {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        // 光标已在目标格则跳过，避免悬停时的无效重绘
        if editor.read(cx).core.cursor.map(|c| c.cell) == Some(cell) {
            return;
        }
        editor.update(cx, |state, cx| {
            state.core.apply(Action::DragTo(cell));
            cx.notify();
        });
    }
}

fn click_cell(
    editor: &Entity<HexEditorState>,
    cell: usize,
    nibble: usize,
    _on_write: &WriteBack,
    window: &mut Window,
    cx: &mut App,
) {
    let focus = editor.read(cx).focus.clone();
    focus.focus(window, cx);
    editor.update(cx, |state, cx| {
        state.core.apply(Action::Click { cell, nibble });
        cx.notify();
    });
}

fn is_selected(selection: Option<(usize, usize)>, idx: usize) -> bool {
    selection.map(|(s, e)| idx >= s && idx < e).unwrap_or(false)
}

fn handle_key(
    editor: &Entity<HexEditorState>,
    event: &KeyDownEvent,
    on_write: &WriteBack,
    ascii_editable: bool,
    stride: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let key = event.keystroke.key.as_str();
    let modifiers = event.keystroke.modifiers;
    let platform = modifiers.platform || modifiers.control;

    // 复制选区（无选区时复制全部？保持标准行为：无选区不复制）
    if platform && !modifiers.shift && key == "c" {
        if let Some(text) = editor.read(cx).core.selection_value() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.stop_propagation();
        return;
    }
    // 粘贴（容错解析）
    if platform && !modifiers.shift && key == "v" {
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                match core::parse_tolerant(&text) {
                    Ok(cells) if !cells.is_empty() => {
                        apply_action(editor, Action::Paste(cells), on_write, window, cx);
                    }
                    _ => {}
                }
            }
        }
        cx.stop_propagation();
        return;
    }
    if let Some(action) = key_to_action(
        key,
        event.keystroke.key_char.as_deref(),
        &modifiers,
        stride,
        ascii_editable,
    ) {
        apply_action(editor, action, on_write, window, cx);
        cx.stop_propagation();
    }
}

/// 应用动作：core 变更 → 经 on_write 写回输入框 → 标记 parsed_from。
/// parsed_from 必须与写回值一致，否则下一次渲染 sync 会重解析并丢失光标。
pub fn apply_action(
    editor: &Entity<HexEditorState>,
    action: Action,
    on_write: &WriteBack,
    window: &mut Window,
    cx: &mut App,
) {
    editor.update(cx, |state, cx| {
        if let Some(value) = state.core.apply(action) {
            on_write(value.clone(), window, cx);
            state.parsed_from = Some(value);
        }
        cx.notify();
    });
}

fn key_to_action(
    key: &str,
    key_char: Option<&str>,
    m: &Modifiers,
    stride: usize,
    ascii_editable: bool,
) -> Option<Action> {
    let extend = m.shift;
    let dir = match key {
        "left" => Some(MoveDir::Left),
        "right" => Some(MoveDir::Right),
        "up" => Some(MoveDir::Up { stride }),
        "down" => Some(MoveDir::Down { stride }),
        "home" => Some(MoveDir::Home { stride }),
        "end" => Some(MoveDir::End { stride }),
        _ => None,
    };
    if let Some(dir) = dir {
        return Some(Action::Move { dir, extend });
    }
    match key {
        "backspace" => return Some(Action::Backspace),
        "delete" => return Some(Action::Delete),
        "insert" => return Some(Action::InsertToggle),
        // 空格: 前进到下一格(hex 网格中无空字符可键入, 用作光标行走)
        "space" => {
            return Some(Action::Move {
                dir: MoveDir::Right,
                extend,
            });
        }
        "a" if m.platform || m.control => return Some(Action::SelectAll),
        _ => {}
    }
    // 单字符键：hex 数字优先，其次（展开编辑器中）可打印 ASCII 字符
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_hexdigit() {
            return Some(Action::Digit(c));
        }
        if ascii_editable && c != ' ' && (0x20..=0x7e).contains(&(c as u32)) {
            return Some(Action::Ascii(c));
        }
    }
    // key 未命中时回退 key_char（非 ASCII 键盘布局 / IME 提交等路径，字符在 key_char 中）；
    // ctrl/platform/alt 组合不参与，避免吞掉快捷键
    if m.control || m.platform || m.alt {
        return None;
    }
    let c = key_char.and_then(|s| s.chars().next())?;
    if c.is_ascii_hexdigit() {
        return Some(Action::Digit(c));
    }
    if ascii_editable && c != ' ' && (0x20..=0x7e).contains(&(c as u32)) {
        return Some(Action::Ascii(c));
    }
    None
}

#[cfg(test)]
mod visual_tests {
    //! 无头可视化测试：网格布局/滚动/点击定位/键入改写
    use std::sync::Arc;

    use gpui::{
        AppContext as _, Context, Entity, IntoElement, MouseButton, MouseDownEvent,
        ParentElement as _, Render, ScrollDelta, ScrollWheelEvent, Styled as _, TestAppContext,
        VisualTestContext, Window, div, point, px, white,
    };
    use gpui_component::Root;

    use super::{HexEditorState, HexEditorStyle, HexViewConfig, render_grid};
    use crate::ui::components::hex_editor::core::{self, Cursor};

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn test_value(bytes: usize) -> String {
        (0..bytes)
            .map(|i| format!("{:02x}", i))
            .collect::<Vec<_>>()
            .join(" ")
    }

    struct HexHost {
        editor: Entity<HexEditorState>,
        /// false = 不渲染网格(模拟"切换到 hex 之前"), 用于测试真实切换顺序
        show_grid: bool,
        /// 宿主容器高度(px)
        height: f32,
    }

    impl Render for HexHost {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            if !self.show_grid {
                return div().w(px(480.0)).h(px(100.0));
            }
            let config = HexViewConfig {
                bytes_per_row: 8,
                offset_digits: 4,
                show_ascii: true,
                ascii_editable: false,
                max_rows: None,
                view_key: "hex-test".into(),
                empty_hint: None,
                overflow_notice: None,
                truncated_notice: None,
            };
            let style = HexEditorStyle {
                text: white(),
                muted: white(),
                border: white(),
                cursor_bg: white(),
                cursor_bg_unfocused: white(),
                cursor_text: white(),
                selection: white(),
                token_bg: white(),
                token_text: white(),
            };
            let grid = render_grid(
                &self.editor,
                &config,
                &style,
                _window,
                cx,
                Arc::new(|_value, _window, _cx| {}),
            );
            // 外层定高容器：8 字节/行 x 20px 行高，仅可见部分行(高度可配置)
            div().w(px(480.0)).h(px(self.height)).child(grid)
        }
    }

    fn seed(cx: &mut TestAppContext, bytes: usize) -> Entity<HexEditorState> {
        let editor = cx.new(|cx| HexEditorState::new(cx));
        editor.update(cx, |state, _cx| {
            let value = test_value(bytes);
            state.core = core::State::from_value(&value);
            state.parsed_from = Some(value);
        });
        editor
    }

    #[gpui::test]
    fn grid_rows_layout_and_scroll(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 64); // 8 行
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: true,
                height: 100.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);
        draw(&mut cx);

        let row0 = cx.debug_bounds("hex-row-0").expect("row-0 bounds");
        // debug_selector 以行起始 cell 索引命名: 8 字节/行 → 第 8 行为 "hex-row-56"
        let row56 = cx.debug_bounds("hex-row-56").expect("row-56 bounds");
        let span = row56.origin.y - row0.origin.y;
        assert!(
            (span - px(140.0)).abs() < px(2.0),
            "7 rows span, got {span:?}"
        );

        // 滚轮滚动后内容上移
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(240.0), px(50.0)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
            ..Default::default()
        });
        draw(&mut cx);
        let row0_after = cx.debug_bounds("hex-row-0").expect("row-0 after scroll");
        assert!(
            row0_after.origin.y < row0.origin.y,
            "wheel scroll should move content, before={row0:?} after={row0_after:?}"
        );
    }

    #[gpui::test]
    fn click_cell_positions_cursor_with_nibble_precision(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 16);
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: true,
                height: 100.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);
        draw(&mut cx);

        // 点击第一个字节的高 nibble
        let bounds = cx.debug_bounds("hex-cell-0-0").expect("cell 0-0 bounds");
        cx.simulate_event(MouseDownEvent {
            position: bounds.center(),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        draw(&mut cx);
        cx.update(|_window, cx| {
            editor.update(cx, |state, _| {
                assert_eq!(
                    state.core.cursor,
                    Some(Cursor { cell: 0, nibble: 0 }),
                    "click high nibble"
                );
            });
        });

        // 点击第三个字节的低 nibble
        let bounds = cx.debug_bounds("hex-cell-2-1").expect("cell 2-1 bounds");
        cx.simulate_event(MouseDownEvent {
            position: bounds.center(),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        draw(&mut cx);
        cx.update(|_window, cx| {
            editor.update(cx, |state, _| {
                assert_eq!(
                    state.core.cursor,
                    Some(Cursor { cell: 2, nibble: 1 }),
                    "click low nibble"
                );
            });
        });
    }

    /// 模拟真实切换流程: chip 处理器直接 focus(元素未点击) → 立即键入数字。
    /// 回归: 自动聚焦后键入必须生效(用户反馈"输入没反应"的路径)。
    #[gpui::test]
    fn auto_focus_then_typing_without_click(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 4);
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: true,
                height: 100.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);
        draw(&mut cx);

        // 与 connection_tab chip 处理器一致: 直接 focus 网格的 focus handle
        cx.update(|window, cx| {
            let focus = editor.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        draw(&mut cx);

        // 焦点应落在网格上
        cx.update(|window, _cx| {
            assert!(
                editor.read(_cx).focus.is_focused(window),
                "grid focus handle should be focused after explicit focus()"
            );
        });

        // 不点击, 直接键入
        cx.simulate_keystrokes("5");
        draw(&mut cx);
        cx.update(|_window, cx| {
            editor.update(cx, |state, _| {
                assert_eq!(
                    state.core.cursor,
                    Some(Cursor { cell: 0, nibble: 1 }),
                    "typing without click should overwrite first high nibble"
                );
                assert!(
                    state.core.full_value().starts_with("50 01"),
                    "digit should land, got {}",
                    state.core.full_value()
                );
            });
        });
    }

    /// 真实切换顺序: 网格尚未挂载(text 模式) → focus → 网格挂载(hex 模式) → 键入。
    /// 回归: "切换后未点击就输入"的完整链路。
    #[gpui::test]
    fn focus_before_mount_then_type(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 4);
        let (host, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: false,
                height: 100.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);

        // chip 处理器: 网格未挂载时 focus
        cx.update(|window, cx| {
            let focus = editor.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        // 切换模式 → 网格挂载 (从 Root 层取 Host 实体, 与 dialog_layout 测试一致)
        let host = cx.update(|window, cx| {
            let root = window.root::<Root>().unwrap().unwrap();
            root.read(cx).view().clone().downcast::<HexHost>().unwrap()
        });
        cx.update(|_, cx| host.update(cx, |h, _| h.show_grid = true));
        draw(&mut cx);
        draw(&mut cx);

        cx.update(|window, _cx| {
            assert!(
                editor.read(_cx).focus.is_focused(window),
                "focus should survive mount of the tracked element"
            );
        });
        cx.simulate_keystrokes("a");
        draw(&mut cx);
        cx.update(|_window, cx| {
            editor.update(cx, |state, _| {
                assert_eq!(
                    state.core.cursor,
                    Some(Cursor { cell: 0, nibble: 1 }),
                    "first typed digit should land at byte 0 high nibble"
                );
                assert!(
                    state.core.full_value().starts_with("a0 01"),
                    "got {}",
                    state.core.full_value()
                );
            });
        });
    }

    /// 空内容必须渲染「幻影光标格」: 光标可见可点, 键入从 (0,0) 追加。
    /// 回归: 空状态下"没有光标"的用户反馈。
    #[gpui::test]
    fn empty_doc_shows_phantom_cursor_and_typing_lands(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 0); // 空内容
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: true,
                height: 100.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);
        draw(&mut cx);

        // 幻影光标行在场
        cx.debug_bounds("hex-phantom-cursor")
            .expect("phantom cursor row should render for empty doc");

        // 模拟切换场景: 未点击直接聚焦后键入, 从首格追加
        cx.update(|window, cx| {
            let focus = editor.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        draw(&mut cx);
        cx.simulate_keystrokes("5");
        draw(&mut cx);
        cx.update(|_window, cx| {
            editor.update(cx, |state, _| {
                assert_eq!(
                    state.core.cursor,
                    Some(Cursor { cell: 0, nibble: 1 }),
                    "typing into empty doc should append first nibble"
                );
                assert_eq!(state.core.full_value(), "5", "value should be appended");
            });
        });
    }

    /// 导航到内容末尾(虚拟末尾)时光标格必须可见, 点击末尾空位后键入追加。
    /// 回归: "字节之间的白色空格处不显示光标"。
    #[gpui::test]
    fn virtual_end_cursor_visible_and_typing_appends(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 3); // 3 字节, 每行 8: 末尾空位在首行 slot 3
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: true,
                height: 100.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);
        draw(&mut cx);

        // 聚焦后 End 键导航到虚拟末尾
        cx.update(|window, cx| {
            let focus = editor.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        draw(&mut cx);
        cx.simulate_keystrokes("end");
        draw(&mut cx);
        cx.update(|_window, cx| {
            assert_eq!(
                editor.read(cx).core.cursor,
                Some(Cursor { cell: 3, nibble: 0 }),
                "End should land on virtual end"
            );
        });
        // 末尾空位渲染了光标格(否则光标消失)
        let end_bounds = cx
            .debug_bounds("hex-cell-3-0")
            .expect("virtual end cursor slot should render");

        // 点击末尾空位后键入 → 追加
        cx.simulate_event(MouseDownEvent {
            position: end_bounds.center(),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        draw(&mut cx);
        cx.simulate_keystrokes("a");
        draw(&mut cx);
        cx.update(|_window, cx| {
            let state = editor.read(cx);
            assert!(
                state.core.full_value().starts_with("00 01 02 a"),
                "typing after clicking end slot should append, got {}",
                state.core.full_value()
            );
        });
    }

    /// 内容恰好填满整行时, 虚拟末尾光标属于新的一行: 补渲染一行保证光标可见。
    #[gpui::test]
    fn virtual_end_cursor_renders_extra_row_at_row_boundary(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 8); // 恰好填满一行(8 字节/行)
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: true,
                height: 400.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);
        draw(&mut cx);

        cx.update(|window, cx| {
            let focus = editor.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        draw(&mut cx);
        cx.simulate_keystrokes("end");
        draw(&mut cx);
        cx.update(|_window, cx| {
            assert_eq!(
                editor.read(cx).core.cursor,
                Some(Cursor { cell: 8, nibble: 0 }),
                "End on full row should land on next-row virtual end"
            );
        });
        cx.debug_bounds("hex-cell-8-0")
            .expect("end cursor slot on the extra row should render");
    }

    #[gpui::test]
    fn typing_hex_digit_overwrites_and_advances(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let editor = seed(cx, 4);
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let editor = editor.clone();
            let host = cx.new(|_| HexHost {
                editor,
                show_grid: true,
                height: 100.0,
            });
            gpui_component::Root::new(host, window, cx)
        });
        draw(&mut cx);
        draw(&mut cx);

        // 点击第一个字节高 nibble 后键入 'f'
        let bounds = cx.debug_bounds("hex-cell-0-0").expect("cell 0-0 bounds");
        cx.simulate_event(MouseDownEvent {
            position: bounds.center(),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        draw(&mut cx);
        cx.simulate_keystrokes("f");
        draw(&mut cx);

        cx.update(|_window, cx| {
            editor.update(cx, |state, _| {
                assert_eq!(
                    state.core.cursor,
                    Some(Cursor { cell: 0, nibble: 1 }),
                    "cursor should advance to low nibble"
                );
                let value = state.core.full_value();
                assert!(
                    value.starts_with("f0"),
                    "first byte overwritten, got {value}"
                );
                assert!(
                    state.parsed_from.as_deref() == Some(value.as_str()),
                    "parsed_from synced"
                );
            });
        });

        // 继续键入 'f'：写入低 nibble 并推进到下一字节
        cx.simulate_keystrokes("f");
        draw(&mut cx);
        cx.update(|_window, cx| {
            editor.update(cx, |state, _| {
                assert_eq!(
                    state.core.cursor,
                    Some(Cursor { cell: 1, nibble: 0 }),
                    "cursor should advance to next byte"
                );
                assert!(
                    state.core.full_value().starts_with("ff 01"),
                    "second nibble written"
                );
            });
        });
    }
}
