use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;
use gpui_component::input::Input;
use gpui_component::ActiveTheme as _;

use crate::app::NetAssistantApp;
use crate::config::connection::{ConnectionConfig, ConnectionType};

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
        let theme = cx.theme().clone();
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
                                            .text_color(theme.foreground)
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
                                            .text_color(theme.foreground)
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
                                            .text_color(theme.foreground)
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
                                                        div.bg(theme.primary)
                                                            .text_color(theme.background)
                                                    })
                                                    .when(self.app.new_connection_protocol != "TCP", |div| {
                                                        div.bg(theme.border)
                                                            .text_color(theme.foreground)
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
                                                        div.bg(theme.primary)
                                                            .text_color(theme.background)
                                                    })
                                                    .when(self.app.new_connection_protocol != "UDP", |div| {
                                                        div.bg(theme.border)
                                                            .text_color(theme.foreground)
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
                                    .bg(theme.border)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.foreground)
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
                                    .bg(theme.primary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.background)
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
                                        let connection_config = if app.new_connection_is_client {
                                            // 创建客户端连接配置（自动生成ID）
                                            let config = ConnectionConfig::new_client(
                                                String::new(),
                                                host,
                                                port,
                                                connection_type,
                                            );
                                            
                                            // 添加到配置存储
                                            app.storage.add_connection(config.clone());
                                            config
                                        } else {
                                            // 创建服务端连接配置（自动生成ID）
                                            let config = ConnectionConfig::new_server(
                                                String::new(),
                                                host,
                                                port,
                                                connection_type,
                                            );
                                            
                                            // 添加到配置存储
                                            app.storage.add_connection(config.clone());
                                            config
                                        };
                                        
                                        // 使用连接配置中的ID作为标签页ID
                                        let new_tab_id = connection_config.id().to_string();
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
