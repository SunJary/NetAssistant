use crate::app::NetAssistantApp;
use crate::custom_icons::CustomIconName;
use crate::theme_event_handler::{ThemeEventHandler, apply_theme};
use crate::ui::connection_panel::ConnectionPanel;
use crate::ui::dialog::FavoriteListPanel;
use crate::ui::tab_container::TabContainer;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::Icon;
use gpui_component::IconName;
use gpui_component::StyledExt;
use gpui_component::TitleBar;
use gpui_component::scroll::ScrollableElement;
use gpui_component::tooltip::Tooltip;
use rust_i18n::t;

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
                                            Some(n) => {
                                                t!("title_bar.star_tooltip_count", count = n).to_string()
                                            }
                                            None => t!("title_bar.star_tooltip_hint").to_string(),
                                        };
                                        let text = if update_available {
                                            let update_msg = latest_version
                                                .as_ref()
                                                .map(|v| t!("title_bar.update_found", version = v).to_string())
                                                .unwrap_or_else(|| {
                                                    t!("title_bar.update_found_generic").to_string()
                                                });
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
                            // 语言切换按钮：点击展开 中文/English 下拉列表
                            .child(
                                div()
                                    .w_8()
                                    .h_8()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .rounded_md()
                                    .when(self.app.show_language_menu, |this_div| {
                                        this_div.bg(theme.border)
                                    })
                                    .hover(|style| style.bg(theme.border))
                                    .child(Icon::new(CustomIconName::Languages))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app, _event, _window, cx| {
                                            app.show_language_menu = !app.show_language_menu;
                                            cx.notify();
                                        }),
                                    )
                                    .id("language-button")
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(t!("title_bar.language").to_string())
                                            .build(window, cx)
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
            .when(self.app.show_favorite_list, |this_div| {
                this_div.child(FavoriteListPanel::new(self.app, self.app.favorite_list_search_input.clone()).render(window, cx))
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
                                .child(t!("star_prompt.title").to_string()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(t!("star_prompt.body").to_string()),
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
                                        .child(t!("star_prompt.action").to_string())
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
                                        .child(t!("star_prompt.dismiss").to_string())
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
                                        .child(t!("context_menu.edit").to_string())
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
                                        .child(t!("context_menu.delete").to_string())
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
                    .map(|v| t!("title_bar.update_found", version = v).to_string())
                    .unwrap_or_else(|| t!("title_bar.update_found_generic").to_string());
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
            // 语言切换下拉菜单（放在渲染链最后，确保浮在最上层）
            .when(self.app.show_language_menu, |this_div| {
                let current_locale = rust_i18n::locale();
                this_div.child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        // 点击菜单以外任意区域关闭
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|app, _event, _window, cx| {
                                app.show_language_menu = false;
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(44.0))
                                .right(px(12.0))
                                .w_40()
                                .py_1()
                                .bg(theme.background)
                                .rounded_md()
                                .shadow_lg()
                                .border_1()
                                .border_color(theme.border)
                                .flex()
                                .flex_col()
                                // 菜单项显示语言原生名称，不随当前语言变化
                                .child(language_menu_item("中文", "zh-CN", &current_locale, cx))
                                .child(language_menu_item("English", "en", &current_locale, cx)),
                        ),
                )
            })
    }
}

/// 构建语言下拉菜单项：选中项左侧显示对勾，点击切换语言并关闭菜单
fn language_menu_item(
    label: &'static str,
    code: &'static str,
    current_locale: &str,
    cx: &mut Context<NetAssistantApp>,
) -> Stateful<Div> {
    let theme = cx.theme().clone();
    let active = current_locale == code;
    div()
        .id(SharedString::from(format!("lang-item-{}", code)))
        .w_full()
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .gap_1()
        .text_sm()
        .cursor_pointer()
        .text_color(theme.foreground)
        .hover(|style| style.bg(theme.border))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, _event, _window, cx| {
                app.set_language(code, cx);
            }),
        )
        .child(
            // 固定宽度的对勾占位，保证未选中项文字与选中项对齐
            div()
                .w_4()
                .flex()
                .justify_center()
                .when(active, |this_div| {
                    this_div.child(
                        Icon::new(IconName::Check)
                            .size(px(14.0))
                            .text_color(theme.primary),
                    )
                }),
        )
        .child(label)
}
