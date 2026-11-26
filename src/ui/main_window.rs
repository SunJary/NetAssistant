use crate::app::NetAssistantApp;
use crate::custom_icons::CustomIconName;
use crate::theme_event_handler::{ThemeEventHandler, apply_theme};
use crate::ui::connection_panel::ConnectionPanel;
use crate::ui::dialog::{FavoriteListPanel, open_new_connection_dialog};
use crate::ui::tab_container::TabContainer;
use crate::update::state::UpdatePhase;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Icon;
use gpui_kit::component::IconName;
use gpui_kit::component::StyledExt;
use gpui_kit::component::TitleBar;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
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
        let update_available = self.app.update.is_available() || self.app.update.is_failed();
        let update_ready = self.app.update.is_ready();
        let star_count = self.app.star_count;
        // 标题栏提示文案：按更新阶段区分
        let update_tip: Option<String> = match &self.app.update.phase {
            UpdatePhase::Available { version, .. } => {
                Some(t!("title_bar.update_found", version = version).to_string())
            }
            UpdatePhase::Downloading {
                version,
                received,
                total,
            } => {
                let percent = if *total > 0 {
                    ((*received as f64 / *total as f64) * 100.0) as u8
                } else {
                    0
                };
                Some(
                    t!("update.downloading", version = version, percent = percent).to_string(),
                )
            }
            UpdatePhase::Ready { version, .. } => {
                Some(t!("update.ready_tooltip", version = version).to_string())
            }
            UpdatePhase::Failed { .. } => Some(t!("update.failed_short").to_string()),
            _ => None,
        };
        let update_tip_for_icon = update_tip.clone();

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            // 将根元素加入焦点路径, 保证未聚焦输入框时快捷键仍能命中本 on_key_down
            .track_focus(&self.app.root_focus)
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                // ===== 应用级快捷键分发 =====
                let key = event.keystroke.key.as_str();
                let mods = &event.keystroke.modifiers;
                let is_ctrl = mods.control || mods.platform; // 兼容 Mac Cmd
                let ctrl_shift = mods.shift;
                let ctrl_alt = mods.alt;

                // 已有行为: Escape 关闭收藏夹列表
                if key == "escape" {
                    if app.show_favorite_list {
                        app.show_favorite_list = false;
                        cx.notify();
                        return;
                    }
                    // 搜索浮层打开时先关闭搜索, 并把焦点归还根元素
                    let search_open = app
                        .connection_tabs
                        .get(&app.active_tab)
                        .map(|tab| tab.search_open)
                        .unwrap_or(false);
                    if search_open {
                        let active_tab = app.active_tab.clone();
                        app.close_search(&active_tab, window, cx);
                    }
                    return;
                }

                // 模态守卫: 应用内浮层打开时屏蔽全局快捷键, 避免误触
                let modal_open = app.show_favorite_list || app.show_language_menu;
                if modal_open {
                    return;
                }

                // 说明: 搜索浮层的 Enter(下一个) / Shift+Enter(上一个) 由 search_input 的
                // InputEvent 订阅处理(Enter 会被 Input 的 enter action 消费), 不在此分发。

                // 以下快捷键均需 Ctrl/Cmd 组合
                if !is_ctrl {
                    return;
                }

                // Ctrl+F 打开搜索浮层并聚焦（幂等：已打开则仅重新聚焦，不用于关闭）。
                // 与 Ctrl+Tab 一致：任何焦点状态下都响应，不做输入框屏蔽。
                if key == "f" && !ctrl_shift && !ctrl_alt {
                    let active_tab = app.active_tab.clone();
                    if !active_tab.is_empty() {
                        app.open_search(&active_tab, window, cx);
                    }
                    return;
                }
                // Ctrl+Tab / Ctrl+Shift+Tab 循环切换标签页
                if key == "tab" && !ctrl_alt {
                    let step = if ctrl_shift { -1 } else { 1 };
                    app.switch_tab(step, cx);
                    return;
                }
                // Ctrl+PageDown / Ctrl+PageUp 上/下一个标签页
                if !ctrl_shift && !ctrl_alt {
                    if key == "pagedown" || key == "page_down" {
                        app.switch_tab(1, cx);
                        return;
                    }
                    if key == "pageup" || key == "page_up" {
                        app.switch_tab(-1, cx);
                        return;
                    }
                }
                // Ctrl+1..9 直接跳转到对应标签页
                if !ctrl_shift && !ctrl_alt {
                    if let Ok(digit) = key.parse::<usize>() {
                        if (1..=9).contains(&digit) {
                            app.activate_tab_by_index(digit, cx);
                            return;
                        }
                    }
                }

                // 以下操作的组合不允许 shift/alt
                if ctrl_shift || ctrl_alt {
                    return;
                }

                // Ctrl+Enter 发送当前标签页消息（工具箱标签页不是连接, 不响应发送）
                if key == "enter" {
                    if app.connection_tabs.contains_key(&app.active_tab) {
                        let active_tab = app.active_tab.clone();
                        app.send_message_from_tab(&active_tab, window, cx);
                    }
                    return;
                }
                // Ctrl+W 关闭当前标签页
                if key == "w" {
                    app.close_active_tab(cx);
                    return;
                }
                // Ctrl+N 新建连接
                if key == "n" {
                    open_new_connection_dialog(cx.entity().downgrade(), window, cx);
                    return;
                }
                // Ctrl+K 聚焦当前标签页的消息输入框
                if key == "k" {
                    if let Some(tab_state) = app.connection_tabs.get(&app.active_tab) {
                        if let Some(message_input) = &tab_state.message_input {
                            message_input.update(cx, |input, cx| input.focus(window, cx));
                        }
                    }
                    return;
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
                            // 更新状态图标：仅在有更新时出现，点击打开更新对话框
                            .when(self.app.update.has_update(), |this_div| {
                                this_div.child(
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
                                        .child(Icon::new(CustomIconName::Download))
                                        .when(update_available, |this_div| {
                                            this_div.child(
                                                div()
                                                    .absolute()
                                                    .top(px(2.0))
                                                    .right(px(2.0))
                                                    .w_2()
                                                    .h_2()
                                                    .rounded_full()
                                                    .bg(gpui_kit::rgb(0xE5484D)),
                                            )
                                        })
                                        .when(update_ready, |this_div| {
                                            this_div.child(
                                                div()
                                                    .absolute()
                                                    .top(px(2.0))
                                                    .right(px(2.0))
                                                    .w_2()
                                                    .h_2()
                                                    .rounded_full()
                                                    .bg(gpui_kit::rgb(0x30A46C)),
                                            )
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|app, _event, _window, cx| {
                                                if app.update.has_update() {
                                                    app.update.dialog_open = true;
                                                    cx.notify();
                                                }
                                            }),
                                        )
                                        .id("update-button")
                                        .tooltip(move |window, cx| {
                                            let text = update_tip_for_icon
                                                .clone()
                                                .unwrap_or_else(|| {
                                                    t!("title_bar.star_tooltip_hint").to_string()
                                                });
                                            Tooltip::new(text).build(window, cx)
                                        }),
                                )
                            })
                            // GitHub 图标：固定指向仓库，tooltip 展示 star 数
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
                                    .child(IconName::Github)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|_app, _event, _window, cx| {
                                            cx.open_url(
                                                "https://github.com/SunJary/NetAssistant",
                                            );
                                        }),
                                    )
                                    .id("github-link")
                                    .tooltip(move |window, cx| {
                                        let star_text = match star_count {
                                            Some(n) => t!(
                                                "title_bar.star_tooltip_count",
                                                count = n
                                            )
                                            .to_string(),
                                            None => {
                                                t!("title_bar.star_tooltip_hint").to_string()
                                            }
                                        };
                                        Tooltip::new(star_text).build(window, cx)
                                    }),
                            )
                            // 语言切换按钮：点击展开语言下拉列表
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
                                        .text_color(gpui_kit::white())
                                        .bg(gpui_kit::rgb(0x238636))
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
            // 自动显示的更新提示（5 秒后消失；鼠标移入即隐藏为下方控件让路；
            // 放在最后确保不被遮挡）
            .when(self.app.update.show_tooltip, |this_div| {
                let version_text = update_tip
                    .clone()
                    .unwrap_or_else(|| t!("title_bar.update_found_generic").to_string());
                this_div.child(
                    div()
                        .id("update-tooltip")
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
                        // hover 即隐藏：鼠标移入说明想操作下方区域
                        .on_mouse_move(
                            cx.listener(|app: &mut NetAssistantApp,
                                         _event: &MouseMoveEvent,
                                         _window: &mut Window,
                                         cx: &mut Context<NetAssistantApp>| {
                                if app.update.show_tooltip {
                                    app.update.show_tooltip = false;
                                    cx.notify();
                                }
                            }),
                        )
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
                                .w_48()
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
                                .child(language_menu_item("English", "en", &current_locale, cx))
                                .child(language_menu_item("日本語", "ja", &current_locale, cx))
                                .child(language_menu_item("Deutsch", "de", &current_locale, cx))
                                .child(language_menu_item("Русский", "ru", &current_locale, cx))
                                .child(language_menu_item(
                                    "Português (Brasil)",
                                    "pt-BR",
                                    &current_locale,
                                    cx,
                                ))
                                .child(language_menu_item("Español", "es", &current_locale, cx)),
                        ),
                )
            })
            // 更新对话框（模态，放在最后保证浮在最上层）
            .when(self.app.update.dialog_open, |this_div| {
                this_div.child(render_update_dialog(cx, &self.app.update))
            })
    }
}

/// 更新对话框：按阶段展示进度与操作按钮。
fn render_update_dialog(
    cx: &mut Context<NetAssistantApp>,
    update: &crate::update::state::UpdateState,
) -> impl IntoElement {
    let theme = cx.theme().clone();

    let (title, body, percent) = match &update.phase {
        UpdatePhase::Available { version, size } => {
            let body = if update.manual_only {
                match &update.downloaded_package {
                    Some(path) => t!(
                        "update.manual_downloaded",
                        path = path.display().to_string()
                    )
                    .to_string(),
                    None => t!("update.manual").to_string(),
                }
            } else if *size > 0 {
                t!(
                    "update.background_downloading",
                    size = format!("{:.1}", *size as f64 / 1024.0 / 1024.0)
                )
                .to_string()
            } else {
                t!("update.background_downloading_unknown").to_string()
            };
            (
                t!("update.available", version = version).to_string(),
                body,
                None,
            )
        }
        UpdatePhase::Downloading {
            version,
            received,
            total,
        } => {
            let percent = if *total > 0 {
                ((*received as f64 / *total as f64) * 100.0).clamp(0.0, 100.0) as u8
            } else {
                0
            };
            (
                t!("update.available", version = version).to_string(),
                t!("update.downloading", version = version, percent = percent).to_string(),
                Some(percent),
            )
        }
        UpdatePhase::Ready { version, .. } => (
            t!("update.ready_title", version = version).to_string(),
            t!("update.ready").to_string(),
            None,
        ),
        UpdatePhase::Failed { message } => (
            t!("update.title").to_string(),
            t!("update.failed", message = message).to_string(),
            None,
        ),
        _ => (
            t!("update.title").to_string(),
            t!("update.checking").to_string(),
            None,
        ),
    };

    // 更新说明只指向 Release 页面，文案跟随界面语言，不使用清单里的固定文本
    let notes = t!("update.notes_default").to_string();

    div()
        .absolute()
        .inset_0()
        .occlude()
        .bg(gpui_kit::rgba(0x00000060))
        .flex()
        .justify_center()
        .items_start()
        .pt(px(90.0))
        .child(
            div()
                .w(px(460.0))
                .max_h(px(420.0))
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .bg(theme.background)
                .rounded_lg()
                .shadow_lg()
                .border_1()
                .border_color(theme.border)
                .text_color(theme.foreground)
                .child(
                    div()
                        .text_base()
                        .font_semibold()
                        .child(title),
                )
                .child(div().text_sm().child(body))
                .when_some(percent, |this_div, percent| {
                    this_div.child(
                        div()
                            .w_full()
                            .h(px(6.0))
                            .rounded_full()
                            .bg(theme.border)
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(theme.primary)
                                    .w(gpui_kit::relative(percent as f32 / 100.0)),
                            ),
                    )
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .max_h(px(140.0))
                        .overflow_hidden()
                        .child(notes),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .when(update.is_ready(), |this_div| {
                            this_div
                                .child(
                                    update_dialog_button(t!("update.install_now").to_string(), cx)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|app, _event, _window, cx| {
                                                app.install_update_now(cx);
                                            }),
                                        ),
                                )
                                .child(
                                    update_dialog_button(t!("update.later").to_string(), cx)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|app, _event, _window, cx| {
                                                app.close_update_dialog(cx);
                                            }),
                                        ),
                                )
                        })
                        .when(update.is_available() || update.is_failed(), |this_div| {
                            let manual_ready =
                                update.manual_only && update.downloaded_package.is_some();
                            let manual_downloading = update.manual_only
                                && matches!(update.phase, UpdatePhase::Downloading { .. });

                            let this_div = if manual_ready {
                                this_div.child(
                                    update_dialog_button(
                                        t!("update.open_folder").to_string(),
                                        cx,
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app, _event, _window, _cx| {
                                            app.open_downloaded_package_dir();
                                        }),
                                    ),
                                )
                            } else if update.manual_only && !manual_downloading {
                                this_div.child(
                                    update_dialog_button(
                                        t!("update.manual_download").to_string(),
                                        cx,
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app, _event, _window, cx| {
                                            app.download_update_manually(cx);
                                        }),
                                    ),
                                )
                            } else if manual_downloading {
                                this_div
                            } else {
                                this_div.child(
                                    update_dialog_button(
                                        t!("update.open_page").to_string(),
                                        cx,
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|_app, _event, _window, cx| {
                                            cx.open_url(crate::update::RELEASES_PAGE);
                                        }),
                                    ),
                                )
                            };

                            this_div.child(
                                update_dialog_button(t!("update.later").to_string(), cx)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app, _event, _window, cx| {
                                            app.close_update_dialog(cx);
                                        }),
                                    ),
                            )
                        }),
                ),
        )
}

/// 对话框按钮样式（与标题栏控件保持一致的紧凑风格）。
fn update_dialog_button(label: String, cx: &mut Context<NetAssistantApp>) -> Stateful<Div> {
    let theme = cx.theme().clone();
    div()
        .id(SharedString::from(format!("update-btn-{label}")))
        .px_3()
        .py_1()
        .rounded_md()
        .bg(theme.primary)
        .text_color(theme.primary_foreground)
        .text_sm()
        .cursor_pointer()
        .hover(|style| style.opacity(0.9))
        .child(label)
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
            cx.listener(move |app, _event, window, cx| {
                app.set_language(code, window, cx);
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
