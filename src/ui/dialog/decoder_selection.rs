use gpui::*;
use gpui_component::{StyledExt, ActiveTheme};

use crate::app::NetAssistantApp;
use crate::config::connection::{DecoderConfig, LengthDelimitedConfig};

pub struct DecoderSelectionDialog<'a> {
    app: &'a NetAssistantApp,
}

impl<'a> DecoderSelectionDialog<'a> {
    pub fn new(app: &'a NetAssistantApp) -> Self {
        Self { app }
    }

    pub fn render(
        self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current_config = self.app.decoder_selection_config.clone().unwrap_or_default();
        
        // 解码器配置更新方法
        let update_decoder_config = move |app: &mut NetAssistantApp, new_config: DecoderConfig| {
            // 更新配置到连接
            if let Some(tab_id) = &app.decoder_selection_tab_id {
                if let Some(tab_state) = app.connection_tabs.get_mut(tab_id) {
                    match &mut tab_state.connection_config {
                        crate::config::connection::ConnectionConfig::Client(config) => {
                            config.decoder_config = new_config.clone();
                        }
                        crate::config::connection::ConnectionConfig::Server(config) => {
                            config.decoder_config = new_config.clone();
                        }
                    }
                    
                    // 保存到JSON配置
                    app.storage.update_connection(tab_state.connection_config.clone());
                }
            }
        };
        

        
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x80000000))
            .child(
                div()
                    .w_96()
                    .bg(theme.muted)
                    .rounded_lg()
                    .shadow_2xl()
                    .p_6()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .mb_4()
                            .text_color(theme.foreground)
                            .child("选择解码器")
                    )
                    // 原始数据解码器选项
                    .child(
                        div()
                            .border(px(1.))
                            .rounded_lg()
                            .p_4()
                            .bg(if current_config == DecoderConfig::Bytes {
                                theme.primary
                            } else {
                                theme.background
                            })
                            .border_color(if current_config == DecoderConfig::Bytes {
                                theme.primary
                            } else {
                                theme.border
                            })
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                // 使用统一方法更新解码器配置并保存到JSON
                                let new_config = DecoderConfig::Bytes;
                                app.decoder_selection_config = Some(new_config.clone());
                                update_decoder_config(app, new_config);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .font_semibold()
                                                    .text_color(if current_config == DecoderConfig::Bytes {
                                                        theme.background
                                                    } else {
                                                        theme.foreground
                                                    })
                                                    .child("原始数据")
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(if current_config == DecoderConfig::Bytes {
                                                        theme.background
                                                    } else {
                                                        theme.muted_foreground
                                                    })
                                                    .child("不进行任何解码处理")
                                            )
                                    )
                                    .child(
                                        if current_config == DecoderConfig::Bytes {
                                            div()
                                                .w(px(20.))
                                                .h(px(20.))
                                                .rounded_full()
                                                .bg(theme.background)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .w(px(8.))
                                                        .h(px(8.))
                                                        .rounded_full()
                                                        .bg(theme.primary)
                                                )
                                        } else {
                                            div()
                                                .w(px(20.))
                                                .h(px(20.))
                                                .rounded_full()
                                                .border(px(2.))
                                                .border_color(theme.border)
                                        }
                                    )
                            )
                    )
                    // 换行符分隔解码器选项
                    .child(
                        div()
                            .mt_4()
                            .border(px(1.))
                            .rounded_lg()
                            .p_4()
                            .bg(if current_config == DecoderConfig::LineBased {
                                theme.primary
                            } else {
                                theme.background
                            })
                            .border_color(if current_config == DecoderConfig::LineBased {
                                theme.primary
                            } else {
                                theme.border
                            })
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                // 使用统一方法更新解码器配置并保存到JSON
                                let new_config = DecoderConfig::LineBased;
                                app.decoder_selection_config = Some(new_config.clone());
                                update_decoder_config(app, new_config);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .font_semibold()
                                                    .text_color(if current_config == DecoderConfig::LineBased {
                                                        theme.background
                                                    } else {
                                                        theme.foreground
                                                    })
                                                    .child("换行符分隔")
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(if current_config == DecoderConfig::LineBased {
                                                        theme.background
                                                    } else {
                                                        theme.muted_foreground
                                                    })
                                                    .child("按换行符分割消息")
                                            )
                                    )
                                    .child(
                                        if current_config == DecoderConfig::LineBased {
                                            div()
                                                .w(px(20.))
                                                .h(px(20.))
                                                .rounded_full()
                                                .bg(theme.background)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .w(px(8.))
                                                        .h(px(8.))
                                                        .rounded_full()
                                                        .bg(theme.primary)
                                                )
                                        } else {
                                            div()
                                                .w(px(20.))
                                                .h(px(20.))
                                                .rounded_full()
                                                .border(px(2.))
                                                .border_color(theme.border)
                                        }
                                    )
                            )
                    )
                    // 长度前缀解码器选项（暂时隐藏）
                    .child(
                        if false { // 设置为false来隐藏此选项
                            div()
                                .mt_4()
                                .border(px(1.))
                                .rounded_lg()
                                .p_4()
                                .bg(if matches!(current_config, DecoderConfig::LengthDelimited(_)) {
                                    theme.primary
                                } else {
                                    theme.background
                                })
                                .border_color(if matches!(current_config, DecoderConfig::LengthDelimited(_)) {
                                    theme.primary
                                } else {
                                    theme.border
                                })
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                    // 使用统一方法更新解码器配置并保存到JSON
                                    let new_config = DecoderConfig::LengthDelimited(LengthDelimitedConfig::default());
                                    app.decoder_selection_config = Some(new_config.clone());
                                    update_decoder_config(app, new_config);
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .font_semibold()
                                                        .text_color(if matches!(current_config, DecoderConfig::LengthDelimited(_)) {
                                                            theme.background
                                                        } else {
                                                            theme.foreground
                                                        })
                                                        .child("长度前缀")
                                                )
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(if matches!(current_config, DecoderConfig::LengthDelimited(_)) {
                                                            theme.background
                                                        } else {
                                                            theme.muted_foreground
                                                        })
                                                        .child("消息前带有固定长度的前缀")
                                                )
                                        )
                                        .child(
                                            if matches!(current_config, DecoderConfig::LengthDelimited(_)) {
                                                div()
                                                    .w(px(20.))
                                                    .h(px(20.))
                                                    .rounded_full()
                                                    .bg(theme.background)
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .w(px(8.))
                                                            .h(px(8.))
                                                            .rounded_full()
                                                            .bg(theme.primary)
                                                    )
                                            } else {
                                                div()
                                                    .w(px(20.))
                                                    .h(px(20.))
                                                    .rounded_full()
                                                    .border(px(2.))
                                                    .border_color(theme.border)
                                            }
                                        )
                                )
                        } else {
                            div().hidden() // 返回一个隐藏的空div
                        }
                    )
                    // JSON解码器选项
                    .child(
                        div()
                            .mt_4()
                            .border(px(1.))
                            .rounded_lg()
                            .p_4()
                            .bg(if current_config == DecoderConfig::Json {
                                theme.primary
                            } else {
                                theme.background
                            })
                            .border_color(if current_config == DecoderConfig::Json {
                                theme.primary
                            } else {
                                theme.border
                            })
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                // 使用统一方法更新解码器配置并保存到JSON
                                let new_config = DecoderConfig::Json;
                                app.decoder_selection_config = Some(new_config.clone());
                                update_decoder_config(app, new_config);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .font_semibold()
                                                    .text_color(if current_config == DecoderConfig::Json {
                                                        theme.background
                                                    } else {
                                                        theme.foreground
                                                    })
                                                    .child("JSON")
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(if current_config == DecoderConfig::Json {
                                                        theme.background
                                                    } else {
                                                        theme.muted_foreground
                                                    })
                                                    .child("解析JSON格式消息")
                                            )
                                    )
                                    .child(
                                        if current_config == DecoderConfig::Json {
                                            div()
                                                .w(px(20.))
                                                .h(px(20.))
                                                .rounded_full()
                                                .bg(theme.background)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .w(px(8.))
                                                        .h(px(8.))
                                                        .rounded_full()
                                                        .bg(theme.primary)
                                                )
                                        } else {
                                            div()
                                                .w(px(20.))
                                                .h(px(20.))
                                                .rounded_full()
                                                .border(px(2.))
                                                .border_color(theme.border)
                                        }
                                    )
                            )
                    )
                    // 关闭按钮
                    .child(
                        div()
                            .mt_6()
                            .flex()
                            .child(
                                div()
                                    .flex_1()
                                    .p_2()
                                    .bg(theme.primary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.background)
                                            .child("关闭")
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                        // 直接关闭对话框
                                        app.show_decoder_selection = false;
                                        app.decoder_selection_tab_id = None;
                                        app.decoder_selection_config = None;
                                        cx.notify();
                                    }))
                            )
                    )
            )
    }
}

