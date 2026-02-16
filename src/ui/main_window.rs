use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;
use gpui_component::IconName;
use gpui_component::ActiveTheme;
use gpui_component::scroll::ScrollableElement;
use gpui_component::tooltip::Tooltip;
use crate::app::NetAssistantApp;
use crate::theme_event_handler::{ThemeEventHandler, apply_theme};
use crate::ui::connection_panel::ConnectionPanel;
use crate::ui::dialog::{NewConnectionDialog, DecoderSelectionDialog};
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
        let theme = cx.theme().clone();
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(
                div()
                    .h_12()
                    .bg(theme.background)
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child("NetAssistant"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w_8()
                                    .h_8()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .rounded_md()
                                    .hover(|style| style.bg(theme.border))
                                    .child(IconName::GitHub)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_app, _event, window, cx| {
                                            cx.open_url("https://github.com/SunJary/NetAssistant/");
                                        }),
                                    )
                                    .id("github-link")
                                    .tooltip(|window, cx| {
                                        Tooltip::new("来 GitHub 看看我们的项目吧").build(window, cx)
                                    })
                            )
                            .child(
                                div()
                                    .w_8()
                                    .h_8()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .rounded_md()
                                    .hover(|style| style.bg(theme.border))
                                    .child(
                                        if cx.global::<ThemeEventHandler>().is_dark_mode() {
                                            IconName::Sun
                                        } else {
                                            IconName::Moon
                                        }
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_app, _event, _window, cx| {
                                            cx.global_mut::<ThemeEventHandler>().toggle_theme();
                                            let is_dark = cx.global::<ThemeEventHandler>().is_dark_mode();
                                            apply_theme(is_dark, cx);
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_64()
                            .h_full()
                            .overflow_y_scrollbar()
                            .child(ConnectionPanel::new(self.app).render(window, cx)),
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
            .when(self.app.show_decoder_selection, |this_div| {
                this_div.child(DecoderSelectionDialog::new(self.app).render(window, cx))
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
                                .bg(theme.background)
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
                                                
                                                // 直接使用连接配置的原始ID作为标签页ID
                                                let tab_id = connection_name.clone();
                                                app.close_tab(tab_id, cx);
                                                
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
