use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;

use crate::app::NetAssistantApp;
use crate::config::connection::ConnectionConfig;

pub struct ConnectionPanel<'a> {
    app: &'a NetAssistantApp,
}

impl<'a> ConnectionPanel<'a> {
    pub fn new(app: &'a NetAssistantApp) -> Self {
        Self { app }
    }

    pub fn render(
        self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        // 提取客户端连接信息（IP、端口、类型）
        let client_info: Vec<(String, u16, String)> = self
            .app
            .storage
            .client_connections()
            .iter()
            .map(|c| {
                if let ConnectionConfig::Client(client) = c {
                    (
                        client.server_address.clone(),
                        client.server_port,
                        client.protocol.to_string(),
                    )
                } else {
                    (String::new(), 0, String::new())
                }
            })
            .collect();

        // 提取服务端连接信息（IP、端口、类型）
        let server_info: Vec<(String, u16, String)> = self
            .app
            .storage
            .server_connections()
            .iter()
            .map(|c| {
                if let ConnectionConfig::Server(server) = c {
                    (
                        server.listen_address.clone(),
                        server.listen_port,
                        server.protocol.to_string(),
                    )
                } else {
                    (String::new(), 0, String::new())
                }
            })
            .collect();

        div()
            .w_64()
            .h_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(0xffffff))
            .border_r_1()
            .border_color(gpui::rgb(0xe5e7eb))
            .p_2()
            .child(
                // 客户端连接手风琴项
                self.render_accordion_item(
                    window,
                    cx,
                    "client-connections",
                    "client-connections-content",
                    "客户端连接",
                    self.app.client_expanded,
                    client_info,
                    "client-new-button",
                    true, // is_client
                ),
            )
            .child(
                // 服务端连接手风琴项
                self.render_accordion_item(
                    window,
                    cx,
                    "server-connections",
                    "server-connections-content",
                    "服务端连接",
                    self.app.server_expanded,
                    server_info,
                    "server-new-button",
                    false, // is_client
                ),
            )
    }

    fn render_accordion_item(
        &self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
        id: &'static str,
        content_id: &'static str,
        title: &'static str,
        is_expanded: bool,
        items: Vec<(String, u16, String)>,
        new_button_id: &'static str,
        is_client: bool,
    ) -> Div {
        let mut content_div = div().flex().flex_col().gap_2().id(content_id).pl_3();

        for (index, (host, port, protocol)) in items.iter().enumerate() {
            let host_clone = host.clone();
            let port_clone = *port;
            let _protocol_clone = protocol.clone();
            let is_client_clone = is_client;
            let index_clone = index;
            let display_text = format!("{}:{} [{}]", host, port, protocol);

            content_div = content_div.child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(gpui::rgb(0x6b7280))
                    .cursor_pointer()
                    .bg(gpui::rgb(0xf3f4f6))
                    .rounded_md()
                    .hover(|style| style.bg(gpui::rgb(0xe5e7eb)))
                    .child(display_text)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(
                            move |app: &mut NetAssistantApp,
                                  _event: &MouseDownEvent,
                                  window: &mut Window,
                                  cx: &mut Context<NetAssistantApp>| {
                                let tab_id = if is_client_clone {
                                    format!("client_{}", index_clone)
                                } else {
                                    format!("server_{}", index_clone)
                                };

                                let connection_config = if is_client_clone {
                                    let client_configs = app.storage.client_connections();
                                    if let Some(config) = client_configs.get(index_clone) {
                                        (*config).clone()
                                    } else {
                                        return;
                                    }
                                } else {
                                    let server_configs = app.storage.server_connections();
                                    if let Some(config) = server_configs.get(index_clone) {
                                        (*config).clone()
                                    } else {
                                        return;
                                    }
                                };

                                app.ensure_tab_exists(
                                    tab_id.clone(),
                                    connection_config,
                                    window,
                                    cx,
                                );
                                app.active_tab = tab_id;
                                cx.notify();
                            },
                        ),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(
                            move |app: &mut NetAssistantApp,
                                  event: &MouseDownEvent,
                                  _window: &mut Window,
                                  cx: &mut Context<NetAssistantApp>| {
                                app.show_context_menu = true;
                                app.context_menu_connection =
                                    Some(format!("{}:{}", host_clone, port_clone));
                                app.context_menu_is_client = is_client_clone;
                                app.context_menu_position = Some(event.position.x);
                                app.context_menu_position_y = Some(event.position.y);
                                cx.notify();
                            },
                        ),
                    ),
            );
        }

        let _app_ptr = self.app as *const NetAssistantApp;
        let is_client_clone = is_client;
        content_div = content_div.child(
            div()
                .id(new_button_id)
                .px_3()
                .py_2()
                .text_sm()
                .text_color(gpui::rgb(0x3b82f6))
                .font_medium()
                .cursor_pointer()
                .bg(gpui::rgb(0xf0f9ff))
                .rounded_md()
                .hover(|style| style.bg(gpui::rgb(0xe0f2fe)))
                .child("+ 新建连接")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(
                        move |app: &mut NetAssistantApp,
                              _event: &MouseDownEvent,
                              window: &mut Window,
                              cx: &mut Context<NetAssistantApp>| {
                            app.show_new_connection = true;
                            app.new_connection_is_client = is_client_clone;

                            let default_host = if is_client_clone {
                                "127.0.0.1"
                            } else {
                                "0.0.0.0"
                            };

                            app.host_input.update(cx, |input, cx| {
                                input.set_value(default_host.to_string(), window, cx);
                                cx.notify();
                            });
                        },
                    ),
                ),
        );

        div()
            .flex()
            .flex_col()
            .mb_2()
            .child(
                // 手风琴标题（可点击）
                div()
                    .id(id)
                    .px_3()
                    .py_2()
                    .text_sm()
                    .font_medium()
                    .text_color(gpui::rgb(0x374151))
                    .cursor_pointer()
                    .bg(gpui::rgb(0xf9fafb))
                    .rounded_md()
                    .hover(|style| style.bg(gpui::rgb(0xf3f4f6)))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(title)
                    .child(
                        div()
                            .text_sm()
                            .text_color(gpui::rgb(0x6b7280))
                            .child(if is_expanded { "▼" } else { "▶" })
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _event, _window, _cx| {
                            if is_client {
                                app.client_expanded = !app.client_expanded;
                            } else {
                                app.server_expanded = !app.server_expanded;
                            }
                        }),
                    ),
            )
            .when(is_expanded, |div| div.child(content_div))
    }
}
