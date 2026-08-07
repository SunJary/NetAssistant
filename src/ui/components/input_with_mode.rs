use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    input::{Input, InputState},
    Icon,
    Theme,
    StyledExt,
    tooltip::Tooltip,
};
use crate::utils::hex::validate_hex_input;
use crate::app::NetAssistantApp;
use crate::custom_icons::CustomIconName;
use crate::message::{format_json_text, MessageDisplayMode};

/// 通用输入框组件（支持文本/十六进制模式）
pub struct InputWithMode;

impl InputWithMode {
    /// 渲染通用输入框
    pub fn render
    (
        input_state: &Entity<InputState>,
        mode: &str,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        // 检查输入是否有效
        let is_valid = if mode == "hex" {
            // 获取输入内容并验证
            let content = input_state.read(cx).value().to_string();
            validate_hex_input(&content)
        } else {
            true
        };

        let mut container = div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full();

        // 构建输入框容器（relative 用于悬浮按钮定位）
        let mut input_container = div()
            .w_full()
            .min_h_32()
            .relative()
            .bg(theme.background)
            .rounded_md()
            .border_1()
            // 根据验证结果设置边框颜色
            .border_color(if !is_valid && mode == "hex" {
                theme.danger // 红色边框表示无效
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
                    .border_0()
            );

        // 仅文本模式下显示 JSON 格式化悬浮按钮
        if mode == "text" {
            let pretty_entity = input_state.clone();
            let minify_entity = input_state.clone();
            input_container = input_container.child(
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
                                Tooltip::new("JSON 美化").build(window, cx)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                    let content = pretty_entity.read(cx).value().to_string();
                                    let formatted = format_json_text(&content, MessageDisplayMode::JsonPretty);
                                    pretty_entity.update(cx, |input, cx| {
                                        input.set_value(formatted, window, cx);
                                    });
                                })
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
                                Tooltip::new("JSON 压缩").build(window, cx)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                    let content = minify_entity.read(cx).value().to_string();
                                    let formatted = format_json_text(&content, MessageDisplayMode::JsonMinified);
                                    minify_entity.update(cx, |input, cx| {
                                        input.set_value(formatted, window, cx);
                                    });
                                })
                            )
                    )
            );
        }

        container = container.child(input_container);

        // 在输入框下方显示错误信息
        if !is_valid && mode == "hex" {
            container = container.child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(theme.danger)
                    .child("十六进制输入格式错误，包含非法字符或长度为奇数")
            );
        }

        container
    }
}
