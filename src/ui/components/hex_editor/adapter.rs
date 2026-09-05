//! 十六进制编辑器 —— 项目适配层
//!
//! 所有项目绑定点集中于此：gpui_component 的 Input/Theme/Icon、rust-i18n 文案、
//! 展开对话框与文件导入。core/widget 保持可提取。
//! 设计与布局规格见 plans/plan-hex-editor.md。

use std::sync::Arc;

use gpui::*;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    input::InputState,
    tooltip::Tooltip,
};
use rust_i18n::t;

use crate::custom_icons::CustomIconName;
use crate::ui::dialog::{dialog_content_max_height, dialog_height};
use crate::utils::hex::validate_hex_input;

use super::{
    core::{self, Cell},
    widget::{HexEditorState, HexEditorStyle, HexViewConfig, WriteBack, apply_action, render_grid},
};

/// 内联紧凑视图总高（与原 Input 的 min_h_32 一致，三处调用点布局不回归）
const INLINE_HEIGHT: f32 = 128.0;
/// 内联每行字节数 —— 消息发送框/压测 payload：16 字节一行
/// （行宽 ≈ offset 52px + 16×20px ≈ 372px）
pub const INLINE_BYTES_PER_ROW_WIDE: usize = 16;
/// 内联每行字节数 —— 自动回复框：5 字节一行（面板窄）
pub const INLINE_BYTES_PER_ROW_AUTO_REPLY: usize = 5;
/// 内联网格可见行数：网格区约 100px / 行高 20px
const INLINE_MAX_ROWS: usize = 5;
const EXPANDED_BYTES_PER_ROW: usize = 16;

/// 值 → 编辑器状态同步（渲染前调用）。
/// 仅当输入框值与上次同步值不同（文本模式编辑过、模式切换、外部 set_value）
/// 才重新解析；编辑器自身写回的值已标记 parsed_from，不会触发重解析。
pub fn sync(editor: &Entity<HexEditorState>, input: &Entity<InputState>, cx: &mut App) {
    let value = input.read(cx).value().to_string();
    if editor.read(cx).parsed_from.as_deref() != Some(value.as_str()) {
        editor.update(cx, |state, cx| {
            state.parsed_from = Some(value.clone());
            state.core = core::State::from_value(&value);
            cx.notify();
        });
    }
}

/// 内容写回回调：core 的编辑结果写回 InputState。
/// 使用 replace_all（而非 set_value）：保留撤销历史，且会发出 InputEvent::Change，
/// 自动回复等既有订阅链路无需改动。
fn write_back(input: &Entity<InputState>) -> WriteBack {
    let input = input.clone();
    Arc::new(move |value, window, cx| {
        input.update(cx, |input, cx| {
            input.replace_all(value, window, cx);
        });
    })
}

fn style_from_theme(theme: &gpui_component::Theme) -> HexEditorStyle {
    // 未聚焦光标: 中性灰(HxD/VSCode 惯例)——主色高亮易与"聚焦/选中"状态混淆,
    // 半透明灰在浅色/深色主题下都可辨识且不抢视觉焦点
    let mut unfocused_cursor = theme.muted_foreground;
    unfocused_cursor.a *= 0.5;
    HexEditorStyle {
        text: theme.foreground,
        muted: theme.muted_foreground,
        border: theme.border,
        cursor_bg: theme.primary,
        cursor_bg_unfocused: unfocused_cursor,
        cursor_text: theme.primary_foreground,
        selection: theme.secondary_hover,
        token_bg: theme.secondary,
        token_text: theme.foreground,
    }
}

/// 内联紧凑编辑器（替换 InputWithMode 的 hex 分支）。
/// 调用前保证已 `sync`（由调用方完成）且输入值可解析（解析失败走回退文本框路径）。
pub fn render_inline(
    editor: &Entity<HexEditorState>,
    input: &Entity<InputState>,
    theme: &gpui_component::Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let style = style_from_theme(theme);
    let is_valid = validate_hex_input(&input.read(cx).value());
    let (full, half, tokens) = editor.read(cx).core.counts();
    let bytes_label = bytes_label(full + half, tokens);
    // 每行字节数按接入点差异化（消息/压测 16、自动回复 5），创建时写在实例上
    let bytes_per_row = editor.read(cx).inline_bytes_per_row;

    let config = HexViewConfig {
        bytes_per_row,
        offset_digits: 4,
        // 指令调试场景下内联 ASCII 基本是乱码噪声(文本协议直接用文本模式),
        // 内联去掉该列换取每行更多字节; 展开编辑器保留三栏供核对魔法数/内嵌字符串
        show_ascii: false,
        ascii_editable: false,
        max_rows: Some(INLINE_MAX_ROWS),
        view_key: SharedString::from(format!("hex-inline-{}", editor.entity_id())),
        empty_hint: Some(t!("hex_editor.empty_hint").to_string().into()),
        overflow_notice: Some(
            t!("hex_editor.overflow", rows = INLINE_MAX_ROWS)
                .to_string()
                .into(),
        ),
        truncated_notice: Some(
            t!("hex_editor.truncated", n = core::MAX_CELLS)
                .to_string()
                .into(),
        ),
    };
    let grid = render_grid(editor, &config, &style, window, cx, write_back(input));

    div()
        .w_full()
        .h(px(INLINE_HEIGHT))
        .flex()
        .flex_col()
        .bg(theme.background)
        .rounded_md()
        .border_1()
        .border_color(if is_valid { theme.border } else { theme.danger })
        // 工具栏：字节计数 + 格式化 + 展开
        .child(
            div()
                .h(px(22.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .px_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(bytes_label),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("hex-format-btn")
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .opacity(0.6)
                                .hover(|s| s.opacity(1.0).text_color(theme.foreground))
                                .cursor_pointer()
                                .child(t!("hex_editor.format").to_string())
                                .tooltip(|window, cx| {
                                    Tooltip::new(t!("hex_editor.format").to_string())
                                        .build(window, cx)
                                })
                                .on_mouse_down(MouseButton::Left, {
                                    let input = input.clone();
                                    move |_, window, cx| {
                                        format_value(&input, window, cx);
                                    }
                                }),
                        )
                        .child(
                            div()
                                .id("hex-expand-btn")
                                .p_1()
                                .rounded_sm()
                                .text_color(theme.muted_foreground)
                                .opacity(0.6)
                                .hover(|s| s.opacity(1.0).text_color(theme.foreground))
                                .cursor_pointer()
                                .child(Icon::new(IconName::Maximize).size(px(14.0)))
                                .tooltip(|window, cx| {
                                    Tooltip::new(t!("hex_editor.expand").to_string())
                                        .build(window, cx)
                                })
                                .on_mouse_down(MouseButton::Left, {
                                    let input = input.clone();
                                    let editor = editor.clone();
                                    move |_, window, cx| {
                                        open_expand_dialog(
                                            input.clone(),
                                            editor.clone(),
                                            window,
                                            cx,
                                        );
                                    }
                                }),
                        ),
                ),
        )
        // 网格区：外层定高容器分配高度，内层滚动（滚动结构铁律）
        .child(div().flex_1().min_h_0().px_1().pb_1().child(grid))
}

fn bytes_label(full: usize, tokens: usize) -> String {
    if tokens > 0 {
        t!("hex_editor.bytes_with_vars", n = full, vars = tokens).to_string()
    } else {
        t!("hex_editor.bytes", n = full).to_string()
    }
}

/// 格式化重排：按 core 序列化规范统一空格分组（不动字节内容）
fn format_value(input: &Entity<InputState>, window: &mut Window, cx: &mut App) {
    let value = input.read(cx).value().to_string();
    if let Some(normalized) = normalize_hex_value(&value) {
        input.update(cx, |input, cx| {
            input.replace_all(normalized, window, cx);
        });
    }
}

/// 规范化 hex 值（模式切换时清理用）；解析失败返回 None（不动内容）
pub fn normalize_hex_value(value: &str) -> Option<String> {
    let doc = core::parse(value).ok()?;
    let normalized = core::serialize(&doc.cells);
    if normalized == value {
        None
    } else {
        Some(normalized)
    }
}

/// 展开大编辑器对话框。
/// 自包含：直接持有 input/editor 实体，不占用 App 状态；
/// 编辑实时写回（与内联同一实例），取消时恢复快照。
pub fn open_expand_dialog(
    input: Entity<InputState>,
    editor: Entity<HexEditorState>,
    window: &mut Window,
    cx: &mut App,
) {
    let snapshot = input.read(cx).value().to_string();
    // 打开后由 content 闭包一次性聚焦网格: 光标实心可见, 可直接键入
    editor.update(cx, |state, _| state.wants_focus = true);
    window.open_dialog(cx, move |dialog, window, _cx| {
        let input = input.clone();
        let editor = editor.clone();
        let snapshot = snapshot.clone();
        dialog
            .title(t!("hex_editor.expand_title").to_string())
            .w(px(720.0))
            .max_h(dialog_height(window))
            // 键盘关闭关闭：编辑器需要自收按键，避免 Esc/Enter 冲突
            .keyboard(false)
            .on_cancel({
                let input = input.clone();
                let snapshot_for_cancel = snapshot.clone();
                move |_, window, cx| {
                    // X / 蒙层关闭视为取消：恢复快照
                    input.update(cx, |input, cx| {
                        input.replace_all(snapshot_for_cancel.clone(), window, cx);
                    });
                    true
                }
            })
            .footer(expand_footer(input.clone(), snapshot))
            .content(move |content, window, cx| {
                let theme = cx.theme().clone();
                sync(&editor, &input, cx);
                // 消 wants_focus: 网格已挂载, 聚焦后光标实心、键立即可用
                if editor.read(cx).wants_focus {
                    editor.update(cx, |state, _| state.wants_focus = false);
                    let focus = editor.read(cx).focus.clone();
                    focus.focus(window, cx);
                }
                let style = style_from_theme(&theme);
                let (full, half, tokens) = editor.read(cx).core.counts();
                let cursor_offset = editor.read(cx).core.cursor_offset();
                let sel_len = editor.read(cx).core.selection_len();
                let mode_label = if editor.read(cx).core.insert_mode {
                    t!("hex_editor.ins").to_string()
                } else {
                    t!("hex_editor.ovr").to_string()
                };

                let config = HexViewConfig {
                    bytes_per_row: EXPANDED_BYTES_PER_ROW,
                    offset_digits: 8,
                    show_ascii: true,
                    ascii_editable: true,
                    max_rows: None,
                    view_key: SharedString::from("hex-expanded"),
                    empty_hint: Some(t!("hex_editor.empty_hint").to_string().into()),
                    overflow_notice: None,
                    truncated_notice: Some(
                        t!("hex_editor.truncated", n = core::MAX_CELLS)
                            .to_string()
                            .into(),
                    ),
                };
                let grid = render_grid(&editor, &config, &style, window, cx, write_back(&input));

                content.child(
                    div()
                        .max_h(dialog_content_max_height(window))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .px_6()
                        .pb_4()
                        // 工具栏：清空 / 导入文件 / 复制全部
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap_1()
                                .child(
                                    dialog_tool_button(
                                        "hex-clear",
                                        IconName::Delete.into(),
                                        t!("hex_editor.clear").to_string(),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        {
                                            let input = input.clone();
                                            let editor = editor.clone();
                                            move |_, window, cx| {
                                                apply_action(
                                                    &editor,
                                                    core::Action::Clear,
                                                    &write_back(&input),
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    ),
                                )
                                .child(
                                    dialog_tool_button(
                                        "hex-import",
                                        IconName::FolderOpen.into(),
                                        t!("hex_editor.import").to_string(),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        {
                                            let input = input.clone();
                                            let editor = editor.clone();
                                            let window_handle = window.window_handle();
                                            move |_, _window, cx| {
                                                start_import(&input, &editor, window_handle, cx);
                                            }
                                        },
                                    ),
                                )
                                .child(
                                    dialog_tool_button(
                                        "hex-copy-all",
                                        IconName::Copy.into(),
                                        t!("hex_editor.copy_all").to_string(),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        {
                                            let editor = editor.clone();
                                            move |_, _, cx| {
                                                let text = editor.read(cx).core.full_value();
                                                if !text.is_empty() {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(text),
                                                    );
                                                }
                                            }
                                        },
                                    ),
                                ),
                        )
                        // 网格：外层 flex 分配高度（max_h 由父容器钳制），内层滚动
                        .child(div().flex_1().min_h(px(240.0)).child(grid))
                        // 状态栏
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_between()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(
                                    div().flex().items_center().gap_2().child(
                                        div().child(match cursor_offset {
                                            Some(offset) => t!(
                                                "hex_editor.status_offset",
                                                offset = format!("{:04X}", offset)
                                            )
                                            .to_string(),
                                            None => String::new(),
                                        }),
                                    ),
                                )
                                .child(div().flex().items_center().gap_2().child(div().child(
                                    if sel_len > 0 {
                                        t!("hex_editor.status_selected", n = sel_len).to_string()
                                    } else {
                                        bytes_label(full + half, tokens)
                                    },
                                )))
                                .child(div().child(mode_label)),
                        ),
                )
            })
    });
}

fn expand_footer(input: Entity<InputState>, snapshot: String) -> DialogFooter {
    DialogFooter::new()
        .child(
            Button::new("hex-expand-cancel")
                .outline()
                .label(t!("hex_editor.cancel").to_string())
                .on_click(move |_, window, cx| {
                    // 取消：恢复快照（编辑已实时写回，需回滚）
                    input.update(cx, |input, cx| {
                        input.replace_all(snapshot.clone(), window, cx);
                    });
                    window.close_dialog(cx);
                }),
        )
        .child(
            Button::new("hex-expand-ok")
                .primary()
                .label(t!("hex_editor.confirm").to_string())
                .on_click(|_, window, cx| {
                    // 编辑已实时写回，确定仅关闭
                    window.close_dialog(cx);
                }),
        )
}

fn dialog_tool_button(id: &'static str, icon: CustomIconName, label: String) -> Stateful<Div> {
    // 简单的图标+文字工具按钮
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_1()
        .hover(|s| s.opacity(0.75))
        .child(Icon::new(icon).size(px(14.0)))
        .child(div().text_xs().child(label))
}

/// 从文件导入：优先按 hex 文本容错解析；无法解析则按二进制逐字节编码。
/// 文件对话框异步弹出，读完后经 update_window 写回（GPUI 实体不可跨线程）。
fn start_import(
    input: &Entity<InputState>,
    editor: &Entity<HexEditorState>,
    window_handle: AnyWindowHandle,
    cx: &mut App,
) {
    let input = input.clone();
    let editor = editor.clone();
    cx.spawn(async move |cx| {
        let Some(file) = rfd::AsyncFileDialog::new().pick_file().await else {
            return;
        };
        let bytes = file.read().await;
        let cells = file_to_hex_cells(&bytes);
        if cells.is_empty() {
            return;
        }
        let _ = cx.update_window(window_handle, |_view, window, cx| {
            apply_action(
                &editor,
                core::Action::Paste(cells),
                &write_back(&input),
                window,
                cx,
            );
        });
    })
    .detach();
}

fn file_to_hex_cells(bytes: &[u8]) -> Vec<Cell> {
    let text = String::from_utf8_lossy(bytes);
    if let Ok(cells) = core::parse_tolerant(&text) {
        if !cells.is_empty() {
            return cells;
        }
    }
    bytes
        .iter()
        .map(|b| {
            let s = format!("{:02x}", b);
            let mut chars = s.chars();
            Cell::Byte {
                hi: chars.next().unwrap(),
                lo: chars.next().unwrap(),
            }
        })
        .collect()
}
