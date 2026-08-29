use std::borrow::Cow;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, StyledExt,
    input::{Input, InputState},
};
use rust_i18n::t;

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
    fn validate_address(addr: &str) -> Result<(), Cow<'static, str>> {
        if addr.trim().is_empty() {
            return Err(t!("add_client.address_empty"));
        }
        if !addr.contains(':') {
            return Err(t!("add_client.invalid_format"));
        }
        if addr.parse::<std::net::SocketAddr>().is_err() {
            return Err(t!("add_client.invalid_address_format"));
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
                            .child(t!("add_client.title").to_string()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .mb_2()
                            .text_color(theme.muted_foreground)
                            .child(t!("add_client.address_hint").to_string()),
                    )
                    .child(div().mb_3().child(Input::new(&input)))
                    // 验证错误提示
                    .when_some(self.error.clone(), |el, err| {
                        el.child(div().mb_2().text_xs().text_color(theme.danger).child(err))
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
                                    .child(t!("add_client.cancel").to_string())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app, _event, _window, cx| {
                                            app.show_add_client_dialog = false;
                                            app.add_client_dialog_error = None;
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_sm()
                                    .bg(theme.primary)
                                    .text_color(theme.primary_foreground)
                                    .hover(|s| s.bg(theme.primary_hover))
                                    .child(t!("add_client.confirm").to_string())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |app, _event, _window, cx| {
                                            let addr_str = input.read(cx).value().to_string();
                                            match Self::validate_address(&addr_str) {
                                                Ok(()) => {
                                                    app.add_client_dialog_error = None;
                                                    let tab_id =
                                                        app.add_client_dialog_tab_id.clone();
                                                    app.add_client_to_server(
                                                        tab_id,
                                                        addr_str.trim().to_string(),
                                                        cx,
                                                    );
                                                    app.show_add_client_dialog = false;
                                                    cx.notify();
                                                }
                                                Err(msg) => {
                                                    app.add_client_dialog_error =
                                                        Some(msg.to_string());
                                                    cx.notify();
                                                }
                                            }
                                        }),
                                    ),
                            ),
                    ),
            )
    }
}
