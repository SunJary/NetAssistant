use crate::custom_icons::CustomIconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::StyledExt;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::{Icon, IconName};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use rust_i18n::t;

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
                    t!("connection_panel.client").to_string(),
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
                    t!("connection_panel.server").to_string(),
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
        title: String,
        is_expanded: bool,
        items: Vec<(String, String, u16, String)>,
        new_button_id: &'static str,
        is_client: bool,
    ) -> Div {
        let theme = cx.theme().clone();
        let mut content_div = div().flex().flex_col().gap_2().id(content_id).pl_3();

        for (conn_id, host, port, protocol) in items.iter() {
            let conn_id_clone1 = conn_id.clone();
            let _host_clone = host.clone();
            let _port_clone = *port;
            let _protocol_clone = protocol.clone();
            let is_client_clone = is_client;
            let display_text = format!("{}:{} [{}]", host, port, protocol);
            // 右键菜单项闭包用: 行渲染期捕获（框架菜单只在打开那一刻惰性构建）
            let menu_app_weak = cx.entity().downgrade();
            let menu_conn_id = conn_id.clone();
            let menu_is_client = is_client;

            content_div = content_div.child(
                div()
                    .id(format!("conn-row-{}", conn_id))
                    .w_full()
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
                    // 右键菜单: 编辑连接 / 删除连接(由框架 ContextMenuExt 承载, 自动收敛到窗口内)
                    .context_menu(move |menu, _window, _cx| {
                        let edit_app = menu_app_weak.clone();
                        let edit_conn_id = menu_conn_id.clone();
                        let delete_app = menu_app_weak.clone();
                        let delete_conn_id = menu_conn_id.clone();
                        menu
                            // 编辑连接
                            .item(
                                PopupMenuItem::new(t!("context_menu.edit").to_string()).on_click(
                                    move |_, window, cx| {
                                        let Some(app) = edit_app.upgrade() else {
                                            return;
                                        };
                                        let conn_id = edit_conn_id.clone();
                                        app.update(cx, |app, cx| {
                                            app.open_edit_connection(conn_id, window, cx);
                                        });
                                    },
                                ),
                            )
                            .separator()
                            // 删除连接（直接执行, 不二次确认）
                            .item(
                                PopupMenuItem::new(t!("context_menu.delete").to_string())
                                    .on_click(move |_, _window, cx| {
                                        let Some(app) = delete_app.upgrade() else {
                                            return;
                                        };
                                        let conn_id = delete_conn_id.clone();
                                        let is_client = menu_is_client;
                                        app.update(cx, |app, cx| {
                                            // 直接使用连接配置的原始ID作为标签页ID
                                            app.close_tab(conn_id.clone(), cx);
                                            // 然后删除连接配置
                                            if is_client {
                                                app.storage.remove_client_connection(&conn_id);
                                            } else {
                                                app.storage.remove_server_connection(&conn_id);
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                    }),
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
                Icon::new(CustomIconName::FilePlusCorner).text_color(theme.primary), // 设置图标颜色为主题的主色调（蓝色）
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

                        app.open_new_connection(is_client_clone, window, cx);
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
                                div().w_2(), // 使用固定宽度的间隔元素，提供更大的间距
                            )
                            .child(new_connection_button),
                    )
                    // 展开/折叠按钮放在右侧
                    .child(if is_expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
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
