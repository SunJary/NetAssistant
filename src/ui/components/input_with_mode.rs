use crate::custom_icons::CustomIconName;
use crate::message::{MessageDisplayMode, format_json_text};
use crate::utils::hex::validate_hex_input;
use gpui::*;
use rust_i18n::t;
use gpui_component::{
    Icon, StyledExt, Theme,
    input::{Input, InputState},
    tooltip::Tooltip,
};

use super::hex_editor::adapter as hex_adapter;
use super::hex_editor::HexEditorState;

/// 通用输入框组件（支持文本/十六进制模式）
pub struct InputWithMode;

impl InputWithMode {
    /// 渲染通用输入框。
    ///
    /// - `mode == "text"`：多行文本框 + JSON 美化/压缩悬浮按钮（与旧行为一致）
    /// - `mode == "hex"` 且提供 `hex_editor`：十六进制网格编辑器（解析失败回退文本框）
    /// - `mode == "hex"` 未提供 `hex_editor`：保持旧行为（文本框 + 校验边框）
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
        if mode == "hex" {
            if let Some(editor) = hex_editor {
                if editor.read(cx).core.doc.is_some() {
                    let editor_view = hex_adapter::render_inline(editor, input_state, theme, window, cx);
                    return div().flex().flex_col().gap_1().w_full().child(editor_view);
                }
                // 解析失败 → 回退文本框 + 错误提示（不丢用户内容）
                let container = text_input_container(input_state, theme, false, cx);
                return div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w_full()
                    .child(container)
                    .child(error_line(theme));
            }
            // 未提供编辑器实体：保持旧行为
            let is_valid = validate_hex_input(&input_state.read(cx).value());
            let container = text_input_container(input_state, theme, is_valid, cx);
            let mut view = div().flex().flex_col().gap_1().w_full().child(container);
            if !is_valid {
                view = view.child(error_line(theme));
            }
            return view;
        }

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
                            Tooltip::new(t!("input_mode.json_pretty").to_string()).build(window, cx)
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            move |_event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                let content = pretty_entity.read(cx).value().to_string();
                                let formatted = format_json_text(&content, MessageDisplayMode::JsonPretty);
                                pretty_entity.update(cx, |input, cx| {
                                    input.set_value(formatted, window, cx);
                                });
                            }
                        )
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
                            Tooltip::new(t!("input_mode.json_minify").to_string()).build(window, cx)
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            move |_event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                let content = minify_entity.read(cx).value().to_string();
                                let formatted = format_json_text(&content, MessageDisplayMode::JsonMinified);
                                minify_entity.update(cx, |input, cx| {
                                    input.set_value(formatted, window, cx);
                                });
                            }
                        )
                )
        );

        div().flex().flex_col().gap_1().w_full().child(container)
    }
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
        .border_color(if !is_valid { theme.danger } else { theme.border })
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
    //! （渲染中创建实体/订阅 → text 模式渲染 → 切 hex + sanitize → 继续渲染）
    use gpui::{
        div, px, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
        Styled as _, TestAppContext, Window,
    };
    use gpui_component::{input::InputState, ActiveTheme as _, Root};
    use rust_i18n::t;

    use super::InputWithMode;
    use crate::ui::components::hex_editor::{adapter as hex_adapter, HexEditorState};

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

        /// 与 app.rs::sanitize_hex_input 一致
        fn sanitize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let inputs: Vec<Entity<InputState>> = self
                .message_input
                .clone()
                .into_iter()
                .chain(self.auto_reply_input.clone())
                .collect();
            for input in inputs {
                let value = input.read(cx).value().to_string();
                if let Some(normalized) = hex_adapter::normalize_hex_value(&value) {
                    input.update(cx, |input, cx| input.replace_all(normalized, window, cx));
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
                panel = panel.child(InputWithMode::render(input, Some(editor), self.mode, &theme, window, cx));
            }
            if let (Some(input), Some(editor)) = (&self.auto_reply_input, &self.auto_reply_editor) {
                hex_adapter::sync(editor, input, cx);
                panel = panel.child(InputWithMode::render(input, Some(editor), self.mode, &theme, window, cx));
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

        // 切换到 hex: 与 chip handler 一致 (mode 更新 + sanitize)
        cx.update(|window, cx| {
            let root = window.root::<Root>().unwrap().unwrap();
            let host = root
                .read(cx)
                .view()
                .clone()
                .downcast::<Host>()
                .unwrap();
            host.update(cx, |host, cx| {
                host.mode = "hex";
                host.sanitize(window, cx);
            });
        });
        // hex 模式渲染多帧
        for _ in 0..4 {
            draw(&mut cx);
        }
    }
}
