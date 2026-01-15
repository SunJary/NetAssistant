use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;
use gpui_component::input::Input;

use crate::app::NetAssistantApp;
use crate::config::connection::{ClientConfig, ConnectionConfig, ConnectionType, ServerConfig};

pub struct NewConnectionDialog<'a> {
    app: &'a NetAssistantApp,
}

impl<'a> NewConnectionDialog<'a> {
    pub fn new(app: &'a NetAssistantApp) -> Self {
        Self { app }
    }

    pub fn render(
        self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
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
                    .bg(gpui::rgb(0xffffff))
                    .rounded_lg()
                    .shadow_2xl()
                    .p_6()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .mb_4()
                            .child("新建连接"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .child("主机地址"),
                                    )
                                    .child(Input::new(&self.app.host_input)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .child("端口"),
                                    )
                                    .child(Input::new(&self.app.port_input)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .child("协议"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .px_3()
                                                    .py_1()
                                                    .cursor_pointer()
                                                    .when(self.app.new_connection_protocol == "TCP", |div| {
                                                        div.bg(gpui::rgb(0x3b82f6))
                                                            .text_color(gpui::rgb(0xffffff))
                                                    })
                                                    .when(self.app.new_connection_protocol != "TCP", |div| {
                                                        div.bg(gpui::rgb(0xe5e7eb))
                                                            .text_color(gpui::rgb(0x374151))
                                                    })
                                                    .rounded_md()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_medium()
                                                            .child("TCP"),
                                                    )
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                        app.new_connection_protocol = String::from("TCP");
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .px_3()
                                                    .py_1()
                                                    .cursor_pointer()
                                                    .when(self.app.new_connection_protocol == "UDP", |div| {
                                                        div.bg(gpui::rgb(0x3b82f6))
                                                            .text_color(gpui::rgb(0xffffff))
                                                    })
                                                    .when(self.app.new_connection_protocol != "UDP", |div| {
                                                        div.bg(gpui::rgb(0xe5e7eb))
                                                            .text_color(gpui::rgb(0x374151))
                                                    })
                                                    .rounded_md()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_medium()
                                                            .child("UDP"),
                                                    )
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                                        app.new_connection_protocol = String::from("UDP");
                                                        cx.notify();
                                                    })),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .mt_6()
                            .child(
                                div()
                                    .flex_1()
                                    .p_2()
                                    .bg(gpui::rgb(0x9ca3af))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(gpui::rgb(0xffffff))
                                            .child("取消"),
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                        app.show_new_connection = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .p_2()
                                    .bg(gpui::rgb(0x3b82f6))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(gpui::rgb(0xffffff))
                                            .child("确定"),
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                        // 从InputState读取值
                                        let host = app.host_input.read(cx).value().to_string();
                                        let port_str = app.port_input.read(cx).value().to_string();

                                        // 验证必填字段
                                        if host.is_empty() || port_str.is_empty() {
                                            return;
                                        }

                                        // 解析端口
                                        let port: u16 = match port_str.parse() {
                                            Ok(p) => p,
                                            Err(_) => return,
                                        };

                                        // 根据协议类型创建连接配置
                                        let connection_type = if app.new_connection_protocol == "TCP" {
                                            ConnectionType::Tcp
                                        } else {
                                            ConnectionType::Udp
                                        };

                                        // 根据new_connection_is_client创建客户端或服务端连接
                                        let new_tab_id = if app.new_connection_is_client {
                                            // 创建客户端连接配置
                                            let config = ClientConfig::new(
                                                String::new(),
                                                host,
                                                port,
                                                connection_type,
                                            );

                                            // 添加到配置存储
                                            app.storage.add_connection(ConnectionConfig::Client(config));

                                            // 获取新添加的客户端索引
                                            let client_configs = app.storage.client_connections();
                                            let index = client_configs.len() - 1;
                                            format!("client_{}", index)
                                        } else {
                                            // 创建服务端连接配置
                                            let config = ServerConfig::new(
                                                String::new(),
                                                host,
                                                port,
                                                connection_type,
                                            );

                                            // 添加到配置存储
                                            app.storage.add_connection(ConnectionConfig::Server(config));

                                            // 获取新添加的服务端索引
                                            let server_configs = app.storage.server_connections();
                                            let index = server_configs.len() - 1;
                                            format!("server_{}", index)
                                        };

                                        // 获取新添加的连接配置
                                        let connection_config = if app.new_connection_is_client {
                                            let client_configs = app.storage.client_connections();
                                            if let Some(config) = client_configs.last() {
                                                (*config).clone()
                                            } else {
                                                return;
                                            }
                                        } else {
                                            let server_configs = app.storage.server_connections();
                                            if let Some(config) = server_configs.last() {
                                                (*config).clone()
                                            } else {
                                                return;
                                            }
                                        };
                                                                                // 确保标签页存在并切换到该标签页
                                        app.ensure_tab_exists(new_tab_id.clone(), connection_config, window, cx);
                                        app.active_tab = new_tab_id;
                                        // 重置协议
                                        app.new_connection_protocol = String::from("TCP");

                                        // 关闭对话框
                                        app.show_new_connection = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }
}
