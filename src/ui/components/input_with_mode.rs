use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    input::{Input, InputState},
    Theme,
    StyledExt,
};
use crate::utils::hex::validate_hex_input;
use crate::app::NetAssistantApp;

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
            
        // 输入框容器
        container = container.child(
            div()
                .w_full()
                .min_h_32()
                .bg(theme.background)
                .rounded_md()
                .border_1()
                // 根据验证结果设置边框颜色
                .border_color(if !is_valid && mode == "hex" {
                    gpui::rgb(0xef4444) // 红色边框表示无效
                } else {
                    theme.border.to_rgb() // 转换为Rgb类型以匹配
                })
                .child(
                    Input::new(input_state)
                        .w_full()
                        .h_full()
                        .p_2()
                        .bg(theme.background)
                        .rounded_md()
                        .border_0()
                )
        );

        // 在输入框下方显示错误信息
        if !is_valid && mode == "hex" {
            container = container.child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(gpui::rgb(0xef4444))
                    .child("十六进制输入格式错误，包含非法字符或长度为奇数")
            );
        }

        container
    }
}
