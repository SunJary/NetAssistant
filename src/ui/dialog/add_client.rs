use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, input::{Input, InputState}};

use crate::app::NetAssistantApp;

pub struct AddClientDialog {
    input: Entity<InputState>,
    error: Option<String>,
}

impl AddClientDialog {
    pub fn new(_app: &NetAssistantApp, input: Entity<InputState>, error: Option<String>) -> Self {
        Self { input, error }
    }

    /// 验证地址格式：必须是 IP:端口
    fn validate_address(addr: &str) -> Result<(), &'static str> {
        if addr.trim().is_empty() {
            return Err("地址不能为空");
        }
        if !addr.contains(':') {
            return Err("格式错误，需要 IP:端口（如 192.168.1.100:8080）");
        }
        if addr.parse::<std::net::SocketAddr>().is_err() {
            return Err("无效的地址格式，需要 IP:端口（如 192.168.1.100:8080）");
        }
        Ok(())
    }

    pub fn render(
        self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let input = self.input.clone();
        let input_for_key = input.clone();

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x80000000))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        app.show_add_client_dialog = false;
                        app.add_client_dialog_error = None;
                        cx.notify();
                    }
                    "enter" => {
                        let addr_str = input_for_key.read(cx).value().to_string();
                        match Self::validate_address(&addr_str) {
                            Ok(()) => {
                                app.add_client_dialog_error = None;
                                let tab_id = app.add_client_dialog_tab_id.clone();
                                app.add_client_to_server(tab_id, addr_str.trim().to_string(), cx);
                                app.show_add_client_dialog = false;
                                cx.notify();
                            }
                            Err(msg) => {
                                app.add_client_dialog_error = Some(msg.to_string());
                                cx.notify();
                            }
                        }
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .w_80()
                    .bg(theme.muted)
                    .rounded_lg()
                    .shadow_2xl()
                    .p_5()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {})
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .mb_3()
                            .text_color(theme.foreground)
                            .child("添加客户端")
                    )
                    .child(
                        div()
                            .text_xs()
                            .mb_2()
                            .text_color(theme.muted_foreground)
                            .child("输入客户端地址，如 192.168.1.100:8080")
                    )
                    .child(
                        div()
                            .mb_3()
                            .child(
                                Input::new(&input)
                            )
                    )
                    // 验证错误提示
                    .when_some(self.error.clone(), |el, err| {
                        el.child(
                            div()
                                .mb_2()
                                .text_xs()
                                .text_color(gpui::rgb(0xef4444))
                                .child(err)
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .hover(|s| s.bg(theme.secondary))
                                    .child("取消")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
                                        app.show_add_client_dialog = false;
                                        app.add_client_dialog_error = None;
                                        cx.notify();
                                    }))
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_sm()
                                    .bg(theme.primary)
                                    .text_color(theme.background)
                                    .hover(|s| s.opacity(0.8))
                                    .child("确定")
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                        let addr_str = input.read(cx).value().to_string();
                                        match Self::validate_address(&addr_str) {
                                            Ok(()) => {
                                                app.add_client_dialog_error = None;
                                                let tab_id = app.add_client_dialog_tab_id.clone();
                                                app.add_client_to_server(tab_id, addr_str.trim().to_string(), cx);
                                                app.show_add_client_dialog = false;
                                                cx.notify();
                                            }
                                            Err(msg) => {
                                                app.add_client_dialog_error = Some(msg.to_string());
                                                cx.notify();
                                            }
                                        }
                                    }))
                            )
                    )
            )
    }
}
