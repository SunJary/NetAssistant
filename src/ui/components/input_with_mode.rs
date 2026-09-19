use crate::custom_icons::CustomIconName;
use crate::message::{MessageDisplayMode, format_json_text};
use crate::ui::dialog::{dialog_content_max_height, dialog_height};
use crate::utils::hex::{hex_to_text, text_to_hex, validate_hex_input};
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Icon, StyledExt, Theme, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    input::{
        Copy as CopyAction, Cut as CutAction, Input, InputState, Paste as PasteAction,
        SelectAll as SelectAllAction,
    },
    menu::{ContextMenuExt, PopupMenu, PopupMenuItem},
    scroll::ScrollableElement,
    tooltip::Tooltip,
};
use rust_i18n::t;

use super::hex_editor::HexEditorState;
use super::hex_editor::adapter as hex_adapter;

/// 通用输入框组件（支持文本/十六进制模式）
pub struct InputWithMode;

impl InputWithMode {
    /// 渲染通用输入框。
    ///
    /// - `mode == "text"`：多行文本框 + JSON 美化/压缩悬浮按钮（与旧行为一致）
    /// - `mode == "hex"` 且提供 `hex_editor`：十六进制网格编辑器（解析失败回退文本框）
    /// - `mode == "hex"` 未提供 `hex_editor`：保持旧行为（文本框 + 校验边框）
    ///
    /// 同时挂统一右键菜单：文本框含剪切/复制/粘贴/全选，hex 网格无编辑动作；
    /// 转换项只放与当前模式相反的那一项（文本模式给「转换为 Hex」，hex 模式给
    /// 「转换为文本」）；转换结果送只读弹窗，不改动输入框内容。
    ///
    /// 注意：调用方需先完成 `hex_adapter::sync`（本函数只读渲染，不更新实体）。
    pub fn render(
        input_state: &Entity<InputState>,
        hex_editor: Option<&Entity<HexEditorState>>,
        mode: &str,
        theme: &Theme,
        window: &Window,
        cx: &App,
    ) -> impl IntoElement {
        let is_hex_mode = mode == "hex";
        // 内容解析成功才走网格分支（此时无文本框，右键菜单只放转换项）
        let is_grid = is_hex_mode
            && hex_editor
                .map(|editor| editor.read(cx).core.doc.is_some())
                .unwrap_or(false);

        let body: Div = if is_grid {
            let editor = hex_editor.expect("is_grid 为真时必然存在 hex 编辑器");
            div()
                .flex()
                .flex_col()
                .gap_1()
                .w_full()
                .child(hex_adapter::render_inline(
                    editor,
                    input_state,
                    theme,
                    window,
                    cx,
                ))
        } else if mode == "hex" {
            // 网格不可用：回退文本框 + 错误提示（不丢用户内容）；
            // 提供编辑器却走到这里说明解析失败，与旧行为一致地报错
            let is_valid = hex_editor.is_none() && validate_hex_input(&input_state.read(cx).value());
            let mut view = div()
                .flex()
                .flex_col()
                .gap_1()
                .w_full()
                .child(text_input_container(input_state, theme, is_valid, cx));
            if !is_valid {
                view = view.child(error_line(theme));
            }
            view
        } else {
            // ---- 文本模式（与旧行为一致）----
            let pretty_entity = input_state.clone();
            let minify_entity = input_state.clone();
            let container = text_input_container(input_state, theme, true, cx).child(
                div()
                    .absolute()
                    .top_1()
                    .right_1()
                    .flex()
                    .gap_1()
                    // 美化按钮
                    .child(
                        div()
                            .id("json-pretty-btn")
                            .p_1()
                            .text_color(theme.muted_foreground)
                            .opacity(0.4)
                            .hover(|s| s.opacity(1.0))
                            .cursor_pointer()
                            .child(Icon::new(CustomIconName::Braces).size(px(14.0)))
                            .tooltip(|window, cx| {
                                Tooltip::new(t!("input_mode.json_pretty").to_string())
                                    .build(window, cx)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_event: &MouseDownEvent,
                                      window: &mut Window,
                                      cx: &mut App| {
                                    let content = pretty_entity.read(cx).value().to_string();
                                    let formatted = format_json_text(
                                        &content,
                                        MessageDisplayMode::JsonPretty,
                                    );
                                    pretty_entity.update(cx, |input, cx| {
                                        input.set_value(formatted, window, cx);
                                    });
                                },
                            ),
                    )
                    // 压缩按钮
                    .child(
                        div()
                            .id("json-minify-btn")
                            .p_1()
                            .text_color(theme.muted_foreground)
                            .opacity(0.4)
                            .hover(|s| s.opacity(1.0))
                            .cursor_pointer()
                            .child(Icon::new(CustomIconName::Minimize2).size(px(14.0)))
                            .tooltip(|window, cx| {
                                Tooltip::new(t!("input_mode.json_minify").to_string())
                                    .build(window, cx)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_event: &MouseDownEvent,
                                      window: &mut Window,
                                      cx: &mut App| {
                                    let content = minify_entity.read(cx).value().to_string();
                                    let formatted = format_json_text(
                                        &content,
                                        MessageDisplayMode::JsonMinified,
                                    );
                                    minify_entity.update(cx, |input, cx| {
                                        input.set_value(formatted, window, cx);
                                    });
                                },
                            ),
                    ),
            );

            div().flex().flex_col().gap_1().w_full().child(container)
        };

        // 统一右键菜单（4 处调用点自动获得能力；Input 内置原生菜单已在创建处关闭）
        let menu_input = input_state.clone();
        let menu_editor = hex_editor.cloned();
        body.id(("input-with-mode", input_state.entity_id()))
            .context_menu(move |menu, _window, cx| {
                build_context_menu(
                    menu,
                    menu_input.clone(),
                    menu_editor.clone(),
                    is_grid,
                    is_hex_mode,
                    cx,
                )
            })
    }
}

/// 转换方向
#[derive(Clone, Copy)]
enum Direction {
    ToHex,
    ToText,
}

/// 右键菜单的转换源：优先选区（hex 网格选区 / 文本框选区），无选区取全文。
///
/// 菜单打开时才求值，取到的是用户当下的选区，不随渲染帧缓存。
fn convert_source(
    input: &Entity<InputState>,
    hex_editor: Option<&Entity<HexEditorState>>,
    is_grid: bool,
    cx: &App,
) -> String {
    if is_grid {
        if let Some(editor) = hex_editor {
            let state = editor.read(cx);
            return state
                .core
                .selection_value()
                .unwrap_or_else(|| state.core.full_value());
        }
    }
    let selected = input.read(cx).selected_value().to_string();
    if selected.is_empty() {
        input.read(cx).value().to_string()
    } else {
        selected
    }
}

/// 构建输入框右键菜单。
///
/// - 文本框分支：剪切/复制/粘贴/全选（gpui_component 标准编辑动作，靠 `action_context`
///   在确认时先聚焦输入框再派发）+ 分隔线
/// - hex 网格分支：不放标准编辑动作（网格编辑走自身按键）
/// - 转换项按当前模式二选一：文本模式只给「转换为 Hex」，hex 模式只给「转换为文本」
///（当前模式对应的方向在切换时已自动完成，放上去只会是无意义项）
///
/// 「转换为 Hex」恒可用（任何内容都能编码），「转换为文本」要求源内容为合法 hex。
fn build_context_menu(
    menu: PopupMenu,
    input: Entity<InputState>,
    hex_editor: Option<Entity<HexEditorState>>,
    is_grid: bool,
    is_hex_mode: bool,
    cx: &App,
) -> PopupMenu {
    let source = convert_source(&input, hex_editor.as_ref(), is_grid, cx);
    let has_source = !source.is_empty();
    let is_hex_source = validate_hex_input(&source);

    let mut menu = menu;
    if !is_grid {
        menu = menu
            .action_context(input.read(cx).focus_handle(cx))
            .menu(t!("input_mode.cut").to_string(), Box::new(CutAction))
            .menu(t!("input_mode.copy").to_string(), Box::new(CopyAction))
            .menu(t!("input_mode.paste").to_string(), Box::new(PasteAction))
            .separator()
            .menu(
                t!("input_mode.select_all").to_string(),
                Box::new(SelectAllAction),
            )
            .separator();
    }

    if is_hex_mode {
        menu.item(convert_menu_item(
            t!("input_mode.to_text").to_string(),
            Direction::ToText,
            !is_hex_source,
            &input,
            hex_editor,
            is_grid,
        ))
    } else {
        menu.item(convert_menu_item(
            t!("input_mode.to_hex").to_string(),
            Direction::ToHex,
            !has_source,
            &input,
            hex_editor,
            is_grid,
        ))
    }
}

/// 构造「转换」菜单项：点击后把转换结果送进只读结果弹窗（不改动输入框）
fn convert_menu_item(
    label: String,
    direction: Direction,
    disabled: bool,
    input: &Entity<InputState>,
    hex_editor: Option<Entity<HexEditorState>>,
    is_grid: bool,
) -> PopupMenuItem {
    let input = input.clone();
    let title = label.clone();
    PopupMenuItem::new(label)
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            let source = convert_source(&input, hex_editor.as_ref(), is_grid, cx);
            let converted = match direction {
                Direction::ToHex => text_to_hex(&source),
                Direction::ToText => hex_to_text(&source),
            };
            let title = title.clone();
            // 菜单确认后本帧还要关菜单、归还焦点，弹窗延后一帧打开避免同期打架
            window.defer(cx, move |window, cx| {
                open_convert_result_dialog(title, converted, window, cx);
            });
        })
}

/// 打开转换结果对话框：只读展示（多行、可滚动、等宽）+ 复制/关闭按钮。
/// 纯展示，不回写输入框。
fn open_convert_result_dialog(title: String, text: String, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, _cx| {
        let copy_value = text.clone();
        let body_value = text.clone();
        dialog
            .title(title.clone())
            .w(px(480.0))
            .max_h(dialog_height(window))
            // ESC / 蒙层可关闭；复制后自动关闭（无独立「关闭」按钮）
            .keyboard(true)
            .on_ok(|_, _, _| true)
            .footer(
                DialogFooter::new().child(
                    Button::new("convert-result-copy")
                        .primary()
                        .label(t!("input_mode.copy").to_string())
                        .on_click(move |_, window, cx| {
                            let value = copy_value.clone();
                            if !value.is_empty() {
                                cx.write_to_clipboard(ClipboardItem::new_string(value));
                            }
                            window.close_dialog(cx);
                        }),
                ),
            )
            .content(move |content, window, cx| {
                let theme = cx.theme().clone();
                content.child(
                    div().max_h(dialog_content_max_height(window)).child(
                        div()
                            .id("convert-result-body")
                            .overflow_y_scrollbar()
                            .px_6()
                            .pb_4()
                            .child(
                                div()
                                    .w_full()
                                    .px_3()
                                    .py_2()
                                    .bg(theme.border)
                                    .rounded_md()
                                    .font_family("JetBrains Mono")
                                    .text_xs()
                                    .text_color(theme.foreground)
                                    .child(body_value.clone()),
                            ),
                    ),
                )
            })
    });
}

/// 构建文本输入框容器（hex 校验失败时 danger 边框；valid_only 表示仅 valid 时边框着色）
fn text_input_container(
    input_state: &Entity<InputState>,
    theme: &Theme,
    is_valid: bool,
    _cx: &App,
) -> Div {
    div()
        .w_full()
        .min_h_32()
        .relative()
        .bg(theme.background)
        .rounded_md()
        .border_1()
        // 根据验证结果设置边框颜色
        .border_color(if !is_valid {
            theme.danger
        } else {
            theme.border
        })
        .child(
            Input::new(input_state)
                .w_full()
                .h_full()
                .p_2()
                .font_family("JetBrains Mono")
                .bg(theme.background)
                .rounded_md()
                .border_0(),
        )
}

fn error_line(theme: &Theme) -> Div {
    div()
        .text_xs()
        .font_medium()
        .text_color(theme.danger)
        .child(t!("input_mode.hex_invalid").to_string())
}

#[cfg(test)]
mod repro_tests {
    //! 复现：自动回复输入框默认值 "ok" 切到 hex 模式后的完整真实序列
    //! （渲染中创建实体/订阅 → text 模式渲染 → 切 hex + 内容转换 → 继续渲染）
    use gpui::{
        AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, Window, div, px,
    };
    use gpui_component::{ActiveTheme as _, Root, WindowExt as _, input::InputState};
    use rust_i18n::t;

    use super::InputWithMode;
    use crate::ui::components::hex_editor::{HexEditorState, adapter as hex_adapter};

    struct Host {
        tab_id: String,
        is_server: bool,
        message_input: Option<Entity<InputState>>,
        message_editor: Option<Entity<HexEditorState>>,
        auto_reply_input: Option<Entity<InputState>>,
        auto_reply_editor: Option<Entity<HexEditorState>>,
        #[allow(dead_code)]
        subscription: Option<gpui::Subscription>,
        mode: &'static str,
    }

    impl Host {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            Self {
                tab_id: "t1".into(),
                is_server: true,
                message_input: None,
                message_editor: None,
                auto_reply_input: None,
                auto_reply_editor: None,
                subscription: None,
                mode: "text",
            }
        }

        /// 与 NetAssistantApp::render 中的 ensure_auto_reply_input_exists 一致:
        /// 渲染期间创建实体 + 订阅
        fn ensure_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if !self.is_server || self.auto_reply_input.is_some() {
                return;
            }
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .code_editor("json")
                    .line_number(false)
                    .folding(false)
                    .multi_line(true)
            });
            input.update(cx, |input, cx| {
                input.set_value("ok".to_string(), window, cx);
            });
            let hex_editor = cx.new(HexEditorState::new);
            let subscription = cx.subscribe(&input, {
                let tab_id = self.tab_id.clone();
                move |_host, _input, event, _cx| {
                    if matches!(event, gpui_component::input::InputEvent::Change) {
                        log::debug!("[repro] auto reply change {tab_id}");
                    }
                }
            });
            self.auto_reply_input = Some(input);
            self.auto_reply_editor = Some(hex_editor);
            self.subscription = Some(subscription);
            // 消息输入框（含合法 hex 内容, 模拟用户已在文本模式输入）
            let message = cx.new(|cx| {
                InputState::new(window, cx)
                    .code_editor("json")
                    .line_number(false)
                    .folding(false)
                    .multi_line(true)
            });
            message.update(cx, |input, cx| {
                input.set_value("11 22 22 33 44 55 11 22".to_string(), window, cx);
            });
            self.message_input = Some(message);
            self.message_editor = Some(cx.new(HexEditorState::new));
        }

        /// 与 app.rs::convert_input_on_mode_switch 一致: 先转换、hex 目标再规范化
        fn convert_mode(
            &mut self,
            from_mode: &str,
            to_mode: &str,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            let inputs: Vec<Entity<InputState>> = self
                .message_input
                .clone()
                .into_iter()
                .chain(self.auto_reply_input.clone())
                .collect();
            for input in inputs {
                let value = input.read(cx).value().to_string();
                let Some(converted) = crate::utils::hex::convert_value(&value, from_mode, to_mode)
                else {
                    continue;
                };
                let next = if to_mode == "hex" {
                    hex_adapter::normalize_hex_value(&converted).unwrap_or(converted)
                } else {
                    converted
                };
                if next != value {
                    input.update(cx, |input, cx| input.replace_all(next, window, cx));
                }
            }
        }
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.ensure_inputs(window, cx);
            let theme = cx.theme().clone();
            let mut panel = div().flex().flex_col().gap_2().w(px(480.0));
            if let (Some(input), Some(editor)) = (&self.message_input, &self.message_editor) {
                hex_adapter::sync(editor, input, cx);
                panel = panel.child(InputWithMode::render(
                    input,
                    Some(editor),
                    self.mode,
                    &theme,
                    window,
                    cx,
                ));
            }
            if let (Some(input), Some(editor)) = (&self.auto_reply_input, &self.auto_reply_editor) {
                hex_adapter::sync(editor, input, cx);
                panel = panel.child(InputWithMode::render(
                    input,
                    Some(editor),
                    self.mode,
                    &theme,
                    window,
                    cx,
                ));
            }
            let _ = t!("input_mode.hex_invalid").to_string();
            panel
        }
    }

    #[gpui::test]
    fn hex_mode_with_invalid_default_value_ok(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let host = cx.new(|cx| Host::new(window, cx));
            Root::new(host, window, cx)
        });
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        // text 模式渲染几帧
        draw(&mut cx);
        draw(&mut cx);

        // 切换到 hex: 与 chip handler 一致 (mode 更新 + 内容整体转换)
        cx.update(|window, cx| {
            let root = window.root::<Root>().unwrap().unwrap();
            let host = root.read(cx).view().clone().downcast::<Host>().unwrap();
            host.update(cx, |host, cx| {
                host.mode = "hex";
                host.convert_mode("text", "hex", window, cx);
                // 转换型语义: 自动回复默认值 "ok" 被编码为 "6F 6B"，hex 模式下合法，
                // 不再是「非法 hex」而回退文本框 + 红色边框
                let auto_reply = host
                    .auto_reply_input
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .value()
                    .to_string();
                assert_eq!(auto_reply, "6F 6B");
            });
        });
        // hex 模式渲染多帧
        for _ in 0..4 {
            draw(&mut cx);
        }
        // 内容已转换为合法 hex, 必须走网格分支: 若退回文本框会同时出现
        // 「不是合法的 hex」提示(用户报告的「切模式没更新文本只报非法 hex」)
        assert!(
            cx.debug_bounds("hex-row-0").is_some(),
            "切到 hex 且内容合法时应渲染 hex 网格, 而不是回退文本框 + 非法 hex 提示"
        );
    }

    /// 回归：真实 InputWithMode 上右键能弹出菜单，且「转换为 Hex」可被鼠标点中
    /// 并打开只读结果弹窗（用户报告的「右键不好使」）。
    ///
    /// 菜单锚在右键点、项高固定 26px，故按下落偏移逐个试探命中位置，
    /// 避免把菜单项序号写死（项顺序/分隔线变化时用例不该失效）。
    #[gpui::test]
    fn convert_context_menu_click_opens_result_dialog(cx: &mut TestAppContext) {
        use gpui::{MouseButton, MouseDownEvent, point};

        cx.update(gpui_component::init);
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let host = cx.new(|cx| Host::new(window, cx));
            Root::new(host, window, cx)
        });
        let mut draw = |cx: &mut gpui::VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        draw(&mut cx);
        draw(&mut cx);

        let at = point(px(100.0), px(40.0));
        let dialog_open =
            |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| window.has_active_dialog(cx));

        let mut hit_offset = None;
        for offset in (30..210).step_by(6) {
            cx.simulate_event(MouseDownEvent {
                button: MouseButton::Right,
                position: at,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            });
            for _ in 0..3 {
                draw(&mut cx);
            }
            let target = point(at.x + px(20.0), at.y + px(offset as f32));
            cx.simulate_click(target, Default::default());
            for _ in 0..3 {
                draw(&mut cx);
            }
            if dialog_open(&mut cx) {
                hit_offset = Some(offset);
                break;
            }
        }

        assert!(
            hit_offset.is_some(),
            "右键菜单没能打开转换结果弹窗（菜单未渲染或菜单项点不中）"
        );
    }
}
