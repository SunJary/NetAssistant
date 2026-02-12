use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;
use gpui_component::{Icon, IconName};
use gpui_component::ActiveTheme as _;
use crate::custom_icons::CustomIconName;

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
        let theme = cx.theme().clone();
        
        // 提取客户端连接信息（ID、IP、端口、类型）
        let client_info: Vec<(String, String, u16, String)> = self
            .app
            .storage
            .client_connections()
            .iter()
            .map(|c| {
                if let ConnectionConfig::Client(client) = c {
                    (
                        client.id.clone(),
                        client.server_address.clone(),
                        client.server_port,
                        client.protocol.to_string(),
                    )
                } else {
                    (String::new(), String::new(), 0, String::new())
                }
            })
            .collect();

        // 提取服务端连接信息（ID、IP、端口、类型）
        let server_info: Vec<(String, String, u16, String)> = self
            .app
            .storage
            .server_connections()
            .iter()
            .map(|c| {
                if let ConnectionConfig::Server(server) = c {
                    (
                        server.id.clone(),
                        server.listen_address.clone(),
                        server.listen_port,
                        server.protocol.to_string(),
                    )
                } else {
                    (String::new(), String::new(), 0, String::new())
                }
            })
            .collect();

        div()
            .w_full()
            .h_full()
            .px_2()
            .flex()
            .flex_col()
            .bg(theme.background)
            .border_r_1()
            .border_color(theme.border)
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
                )
                .mt_4(), // 添加上边距，增加与客户端连接标题的间距
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
        items: Vec<(String, String, u16, String)>,
        new_button_id: &'static str,
        is_client: bool,
    ) -> Div {
        let theme = cx.theme().clone();
        let mut content_div = div().flex().flex_col().gap_2().id(content_id).pl_3();

        for (conn_id, host, port, protocol) in items.iter() {
            let conn_id_clone1 = conn_id.clone();
            let conn_id_clone2 = conn_id.clone();
            let _host_clone = host.clone();
            let _port_clone = *port;
            let _protocol_clone = protocol.clone();
            let is_client_clone = is_client;
            let display_text = format!("{}:{} [{}]", host, port, protocol);

            content_div = content_div.child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .bg(theme.secondary)
                    .rounded_md()
                    .hover(|style| style.bg(theme.border))
                    .child(display_text)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(
                            move |app: &mut NetAssistantApp,
                                  _event: &MouseDownEvent,
                                  window: &mut Window,
                                  cx: &mut Context<NetAssistantApp>| {
                                // 直接使用连接配置的原始ID作为标签页ID
                                let tab_id = conn_id_clone1.to_string();

                                let connection_config = if is_client_clone {
                                    let client_configs = app.storage.client_connections();
                                    if let Some(config) = client_configs.iter().find(|c| {
                                        if let ConnectionConfig::Client(client) = c {
                                            client.id == conn_id_clone1
                                        } else {
                                            false
                                        }
                                    }) {
                                        (*config).clone()
                                    } else {
                                        return;
                                    }
                                } else {
                                    let server_configs = app.storage.server_connections();
                                    if let Some(config) = server_configs.iter().find(|c| {
                                        if let ConnectionConfig::Server(server) = c {
                                            server.id == conn_id_clone1
                                        } else {
                                            false
                                        }
                                    }) {
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
                                    Some(conn_id_clone2.clone());
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

        // 构建新建连接按钮（仅图标）
        let new_connection_button = div()
            .id(new_button_id)
            .p_1()
            .mr_2() // 添加右边距，增加两个图标之间的间距
            .cursor_pointer()
            .child(
                Icon::new(CustomIconName::FilePlusCorner)
                    .text_color(theme.primary) // 设置图标颜色为主题的主色调（蓝色）
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    move |app: &mut NetAssistantApp,
                          _event: &MouseDownEvent,
                          window: &mut Window,
                          cx: &mut Context<NetAssistantApp>| {
                        // 阻止事件传播，防止点击穿透到展开事件
                        cx.stop_propagation();
                        
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
            );

        div()
            .flex()
            .flex_col()
            .child(
                // 手风琴标题（可点击）
                div()
                    .id(id)
                    .px_3()
                    .py_2()
                    .text_sm()
                    .font_medium()
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .bg(theme.secondary)
                    .rounded_md()
                    .hover(|style| style.bg(theme.border))
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb_2()
                    // 将标题和新建连接按钮组合在一起
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(title)
                            .child(
                                div()
                                    .w_2() // 使用固定宽度的间隔元素，提供更大的间距
                            )
                            .child(new_connection_button)
                    )
                    // 展开/折叠按钮放在右侧
                    .child(
                        if is_expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        }
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
