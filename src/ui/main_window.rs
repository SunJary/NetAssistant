use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;
use gpui_component::IconName;
use gpui_component::ActiveTheme;
use gpui_component::scroll::ScrollableElement;
use gpui_component::TitleBar;
use gpui_component::tooltip::Tooltip;
use crate::app::NetAssistantApp;
use crate::theme_event_handler::{ThemeEventHandler, apply_theme};
use crate::ui::connection_panel::ConnectionPanel;
use crate::ui::dialog::{NewConnectionDialog, DecoderSelectionDialog, FavoriteRemarkDialog, FavoriteListPanel, AddClientDialog, StressConfigDialog};
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
        let update_available = self.app.update_available;
        let latest_version = self.app.latest_version.clone();
        let star_count = self.app.star_count;
        
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    if app.show_favorite_list {
                        app.show_favorite_list = false;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, _window, cx| {
                if app.sidebar_resizing {
                    let mouse_x = event.position.x;
                    app.resize_sidebar(mouse_x, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
                if app.sidebar_resizing {
                    app.end_sidebar_resize(cx);
                }
            }))
            .child(
                TitleBar::new()
                    .on_close_window(|_, window: &mut Window, _cx| {
                        window.remove_window();
                    })
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(format!("NetAssistant {}", env!("APP_VERSION"))),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .gap_2()
                            // GitHub 图标 + 红点（star 数和引导放在 tooltip 里）
                            .child(
                                div()
                                    .relative()
                                    .w_8()
                                    .h_8()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .rounded_md()
                                    .hover(|style| style.bg(theme.border))
                                    .child(IconName::Github)
                                    .when(update_available, |this_div| {
                                        this_div.child(
                                            div()
                                                .absolute()
                                                .top(px(2.0))
                                                .right(px(2.0))
                                                .w_2()
                                                .h_2()
                                                .rounded_full()
                                                .bg(gpui::rgb(0xE5484D)),
                                        )
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |app, _event, _window, cx| {
                                            let url = if app.update_available {
                                                "https://github.com/SunJary/NetAssistant/releases/latest"
                                            } else {
                                                "https://github.com/SunJary/NetAssistant"
                                            };
                                            cx.open_url(url);
                                        }),
                                    )
                                    .id("github-link")
                                    .tooltip(move |window, cx| {
                                        // tooltip 始终展示星数 + 引导，有更新时附加版本提示
                                        let star_text = match star_count {
                                            Some(n) => format!("已有 {} 位用户 Star，觉得有用的话也来Star下吧", n),
                                            None => "如果本项目对你有帮助，欢迎来 GitHub Star 一下".to_string(),
                                        };
                                        let text = if update_available {
                                            let update_msg = latest_version
                                                .as_ref()
                                                .map(|v| format!("发现新版本 {}，点击查看", v))
                                                .unwrap_or_else(|| "发现新版本，点击查看".to_string());
                                            match star_count {
                                                Some(_) => format!("{}\n{}", update_msg, star_text),
                                                None => update_msg,
                                            }
                                        } else {
                                            star_text
                                        };
                                        Tooltip::new(text).build(window, cx)
                                    }),
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
                    .when(!self.app.sidebar_collapsed, |this_div| {
                        this_div
                            // 左侧连接面板
                            .child(div()
                                // 使用动态宽度，如果没有设置则使用默认的200px
                                .w(self.app.sidebar_width.unwrap_or_else(|| px(200.0)))
                                .h_full()
                                .overflow_y_scrollbar()
                                .child(ConnectionPanel::new(self.app).render(window, cx)))
                            // 调整手柄
                            .child(div()
                                .w_2()
                                .h_full()
                                .bg(theme.border)
                                .cursor_col_resize()
                                .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _, cx| {
                                    // 开始调整大小
                                    app.start_sidebar_resize(cx);
                                }))
                                .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, _window, cx| {
                                    // 只有在调整大小状态下才处理移动事件
                                    if app.sidebar_resizing {
                                        let mouse_x = event.position.x;
                                        app.resize_sidebar(mouse_x, cx);
                                    }
                                })))
                    })
                    .when(self.app.sidebar_collapsed, |this_div| {
                        this_div
                            // 折叠状态下只显示展开按钮
                            .child(div()
                                .w_10()
                                .h_full()
                                .bg(theme.border)
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, cx.listener(|app, _, _, cx| {
                                    // 展开侧边栏
                                    app.toggle_sidebar(cx);
                                }))
                                .child(IconName::ChevronRight))
                    })
                    // 右侧内容区域
                    .child(div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .overflow_x_hidden()
                        .child(TabContainer::new(self.app).render(window, cx))),

            )
            .when(self.app.show_new_connection, |this_div| {
                this_div.child(NewConnectionDialog::new(self.app).render(window, cx))
            })
            .when(self.app.show_decoder_selection, |this_div| {
                this_div.child(DecoderSelectionDialog::new(self.app).render(window, cx))
            })
            .when(self.app.show_add_client_dialog, |this_div| {
                if let Some(input) = self.app.add_client_dialog_input.clone() {
                    let error = self.app.add_client_dialog_error.clone();
                    this_div.child(AddClientDialog::new(self.app, input, error).render(window, cx))
                } else {
                    this_div
                }
            })
            .when(self.app.show_favorite_remark, |this_div| {
                this_div.child(FavoriteRemarkDialog::new(self.app, self.app.favorite_remark_input.clone()).render(window, cx))
            })
            .when(self.app.show_favorite_list, |this_div| {
                this_div.child(FavoriteListPanel::new(self.app, self.app.favorite_list_search_input.clone()).render(window, cx))
            })
            .when(self.app.stress_config_dialog.is_some(), |this_div| {
                this_div.child(StressConfigDialog::render(self.app, window, cx))
            })
            .when(self.app.show_star_prompt, |this_div| {
                this_div.child(
                    div()
                        .absolute()
                        .top(px(48.0))
                        .right(px(16.0))
                        .w_72()
                        .p_4()
                        .rounded_md()
                        .bg(theme.background)
                        .shadow_lg()
                        .border_1()
                        .border_color(theme.border)
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(theme.foreground)
                                .child("喜欢 NetAssistant？"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("给项目加个 Star 支持一下吧！"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                // 给个 Star
                                .child(
                                    div()
                                        .flex_1()
                                        .py_2()
                                        .text_center()
                                        .text_xs()
                                        .text_color(gpui::white())
                                        .bg(gpui::rgb(0x238636))
                                        .rounded_md()
                                        .cursor_pointer()
                                        .child("⭐ 给个 Star")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|app, _event, _window, cx| {
                                                app.accept_star_prompt(cx);
                                            }),
                                        ),
                                )
                                // 近期不再提示
                                .child(
                                    div()
                                        .flex_1()
                                        .py_2()
                                        .text_center()
                                        .text_xs()
                                        .text_color(theme.foreground)
                                        .border_1()
                                        .border_color(theme.border)
                                        .rounded_md()
                                        .cursor_pointer()
                                        .child("近期不再提示")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|app, _event, _window, cx| {
                                                app.dismiss_star_prompt(cx);
                                            }),
                                        ),
                                ),
                        ),
                )
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
                        .occlude()
                        .child(
                            div()
                                .absolute()
                                .left(menu_x)
                                .top(menu_y)
                                .bg(theme.background)
                                .rounded_md()
                                .shadow_lg()
                                .w_48()
                                .flex()
                                .flex_col()
                                // 编辑连接
                                .child(
                                    div()
                                        .id("ctx-menu-edit")
                                        .w_full()
                                        .px_4()
                                        .py_3()
                                        .text_sm()
                                        .text_color(theme.foreground)
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(theme.border)
                                        })
                                        .child("编辑连接")
                                        .on_mouse_down(MouseButton::Left, cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                            if let Some(connection_id) = app.context_menu_connection.clone() {
                                                app.show_context_menu = false;
                                                app.context_menu_connection = None;
                                                app.context_menu_position = None;
                                                app.context_menu_position_y = None;
                                                app.open_edit_connection(connection_id, window, cx);
                                            }
                                        })),
                                )
                                // 删除连接
                                .child(
                                    div()
                                        .id("ctx-menu-delete")
                                        .w_full()
                                        .px_4()
                                        .py_3()
                                        .text_sm()
                                        .text_color(theme.danger)
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(theme.border)
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
            // 自动显示的更新提示（10 秒后消失，放在最后确保不被遮挡）
            .when(self.app.show_update_tooltip, |this_div| {
                let version_text = self
                    .app
                    .latest_version
                    .as_ref()
                    .map(|v| format!("发现新版本 {}，点击查看", v))
                    .unwrap_or_else(|| "发现新版本，点击查看".to_string());
                this_div.child(
                    div()
                        .absolute()
                        .top(px(40.0))
                        .right(px(16.0))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(theme.background)
                        .shadow_lg()
                        .border_1()
                        .border_color(theme.border)
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(version_text),
                )
            })
    }
}
