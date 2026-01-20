use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;

use crate::app::NetAssistantApp;
use crate::ui::connection_panel::ConnectionPanel;
use crate::ui::dialog::new_connection::NewConnectionDialog;
use crate::ui::tab_container::TabContainer;

pub struct MainWindow<'a> {
    app: &'a NetAssistantApp,
}

impl<'a> MainWindow<'a> {
    pub fn new(app: &'a NetAssistantApp, _cx: &mut Context<NetAssistantApp>) -> Self {
        Self { app }
    }

    pub fn render(
        self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(0xf9fafb))
            .child(
                div()
                    .h_12()
                    .bg(gpui::rgb(0xffffff))
                    .border_b_1()
                    .border_color(gpui::rgb(0xe5e7eb))
                    .flex()
                    .items_center()
                    .px_4()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child("NetAssistant"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .child(
                        ConnectionPanel::new(self.app).render(window, cx),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_x_hidden()
                            .child(TabContainer::new(self.app).render(window, cx)),
                    ),
            )
            .when(self.app.show_new_connection, |this_div| {
                this_div.child(NewConnectionDialog::new(self.app).render(window, cx))
            })
            .when(self.app.show_context_menu, |this_div| {
                let menu_x = self.app.context_menu_position.unwrap_or_else(|| px(0.0));
                let menu_y = self.app.context_menu_position_y.unwrap_or_else(|| px(0.0));
                this_div.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_start()
                        .justify_start()
                        .bg(gpui::rgba(0x80000000))
                        .child(
                            div()
                                .absolute()
                                .left(menu_x)
                                .top(menu_y)
                                .bg(gpui::rgb(0xffffff))
                                .rounded_md()
                                .shadow_lg()
                                .w_48()
                                .child(
                                    div()
                                        .px_4()
                                        .py_3()
                                        .text_sm()
                                        .text_color(gpui::rgb(0xef4444))
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(gpui::rgb(0xfef2f2))
                                        })
                                        .child("删除连接")
                                        .on_mouse_down(MouseButton::Left, cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            if let Some(connection_name) = app.context_menu_connection.clone() {
                                                let is_client = app.context_menu_is_client;
                                                
                                                // 生成对应的标签页ID并关闭标签页
                                                let tab_id = if is_client {
                                                    format!("client_{}", connection_name)
                                                } else {
                                                    format!("server_{}", connection_name)
                                                };
                                                app.close_tab(tab_id);
                                                
                                                // 然后删除连接配置
                                                if is_client {
                                                    app.storage.remove_client_connection(&connection_name);
                                                } else {
                                                    app.storage.remove_server_connection(&connection_name);
                                                }
                                            }
                                            app.show_context_menu = false;
                                            app.context_menu_connection = None;
                                            app.context_menu_position = None;
                                            app.context_menu_position_y = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .on_mouse_down(MouseButton::Left, cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                            app.show_context_menu = false;
                            app.context_menu_connection = None;
                            app.context_menu_position = None;
                            app.context_menu_position_y = None;
                            cx.notify();
                        })),
                )
            })
    }
}
