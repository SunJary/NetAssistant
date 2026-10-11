use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Icon;
use gpui_kit::component::IconName;
use gpui_kit::component::StyledExt;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;

use crate::app::{NetAssistantApp, TOOLBOX_TAB_ID};
use crate::custom_icons::CustomIconName;
use crate::ui::connection_tab::ConnectionTab;
use crate::ui::toolbox_panel::ToolboxPanel;

/// 标签页信息
#[derive(Debug, Clone)]
pub struct TabInfo {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

pub struct TabContainer<'a> {
    app: &'a NetAssistantApp,
}

impl<'a> TabContainer<'a> {
    pub fn new(app: &'a NetAssistantApp) -> Self {
        Self { app }
    }

    pub fn render(
        self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tabs = self.get_tabs();

        div()
            .flex()
            .flex_col()
            .flex_1()
            // min_h_0: 解除 flex 项自动最小尺寸(min-content)对高度的钉死,
            // 否则内容高于窗口时本层被撑开, 下层滚动容器拿不到受限高度
            .min_h_0()
            .bg(theme.background)
            .child(self.render_tab_header(&tabs, cx))
            .child(self.render_tab_content(window, cx))
    }

    /// 获取所有标签页（只显示已创建的连接标签页；工具箱入口为 Tab 栏右侧固定按钮）
    fn get_tabs(&self) -> Vec<TabInfo> {
        let mut tabs = Vec::new();

        for (tab_id, tab_state) in &self.app.connection_tabs {
            let address = tab_state.address();
            let protocol = tab_state.protocol();
            let connection_type = if tab_state.connection_config.is_client() {
                "C"
            } else {
                "S"
            };
            let name = format!("{} [{}-{}]", address, connection_type, protocol);
            let tab = TabInfo {
                id: (*tab_id).to_string(),
                name,
                is_active: self.app.active_tab == *tab_id,
            };
            tabs.push(tab);
        }

        tabs
    }

    /// 渲染标签页头部
    fn render_tab_header(
        &self,
        tabs: &[TabInfo],
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_tab_multiline = self.app.tab_multiline;

        let header_div = div()
            .flex()
            .gap_1()
            .p_1()
            .bg(theme.secondary)
            .border_b_1()
            .border_color(theme.border)
            .min_h(px(32.))
            .w_full();

        // 构建标签页容器，使用flex_1占据主要空间，并添加flex_shrink_0防止被压缩
        let mut tabs_container = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_1()
            .whitespace_nowrap()
            .overflow_hidden();

        if is_tab_multiline {
            tabs_container = tabs_container.flex_wrap();
        }

        // 添加所有标签页
        for (index, tab) in tabs.iter().enumerate() {
            let tab_id = tab.id.clone();
            let is_active = tab.is_active;
            let tab_name = tab.name.clone();

            tabs_container = tabs_container.child(
                div()
                    .id(("tab", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .when(is_active, |div| {
                        div.bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .border_1()
                            .border_color(theme.primary)
                            .border_b_0()
                    })
                    .when(!is_active, |div| {
                        div.bg(theme.secondary)
                            .text_color(theme.muted_foreground)
                            .hover(|style| {
                                style.bg(theme.border)
                            })
                    })
                    .on_mouse_down(MouseButton::Left, {
                        let tab_id_clone = tab_id.clone();
                        cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                            app.active_tab = tab_id_clone.clone();
                            cx.notify();
                        })
                    })
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .max_w(px(150.))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(tab_name),
                    )
                    .child(
                        div()
                            .id(("close-tab", index))
                            .text_xs()
                            .when(is_active, |div| {
                                div.text_color(theme.primary_foreground)
                            })
                            .when(!is_active, |div| {
                                // TODO: 等待主题增加 disabled.foreground 键后迁移
                                div.text_color(gpui_kit::rgb(0x9ca3af))
                            })
                            .hover(|style| {
                                style.text_color(theme.danger)
                            })
                            .cursor_pointer()
                            .child("×")
                            .on_mouse_down(MouseButton::Left, {
                                let tab_id_clone = tab_id.clone();
                                cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                    // 统一关闭入口：工具箱标签页与连接标签页各自处理关闭与激活态修正
                                    app.close_tab_and_fix_active(tab_id_clone.clone(), cx);
                                })
                            }),
                    ),
            );
        }

        // 构建完整的头部，右侧固定放两个按钮：工具箱入口（开关切换）与展开/折叠切换
        let toolbox_active = self.app.active_tab == TOOLBOX_TAB_ID;
        header_div
            .child(tabs_container)
            .child(
                div()
                    .id("toolbox-entry")
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .h_8()
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.border)
                    .when(toolbox_active, |btn| {
                        btn.bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .border_color(theme.primary)
                    })
                    .when(!toolbox_active, |btn| {
                        btn.bg(theme.secondary)
                            .text_color(theme.muted_foreground)
                            .hover(|style| style.bg(theme.border))
                    })
                    .tooltip(|window, cx| {
                        Tooltip::new(t!("toolbox.title").to_string()).build(window, cx)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(
                            move |app: &mut NetAssistantApp,
                                  _event: &MouseDownEvent,
                                  _window: &mut Window,
                                  cx: &mut Context<NetAssistantApp>| {
                                // 开关语义：未激活则打开工具箱，已激活则关闭回到连接标签页
                                app.toggle_toolbox(cx);
                            },
                        ),
                    )
                    .child(Icon::new(CustomIconName::Wrench).size(px(14.0))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0() // 只防止被压缩，不设置固定宽度
                    .h_8() // 设置固定高度
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .bg(theme.secondary)
                    .border_1()
                    .border_color(theme.border)
                    .hover(|style| style.bg(theme.border))
                    .on_mouse_down(MouseButton::Left, {
                        cx.listener(
                            move |app: &mut NetAssistantApp,
                                  _event: &MouseDownEvent,
                                  _window: &mut Window,
                                  cx: &mut Context<NetAssistantApp>| {
                                app.tab_multiline = !app.tab_multiline;
                                cx.notify();
                            },
                        )
                    })
                    .child(if is_tab_multiline {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    }),
            )
    }

    /// 渲染标签页内容区域
    fn render_tab_content(
        &self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        // 工具箱标签页：特殊 id，不走 connection_tabs
        if self.app.active_tab == TOOLBOX_TAB_ID {
            return div().flex().flex_col().flex_1().min_h_0().child(
                ToolboxPanel::new(self.app).render(cx),
            );
        }

        if let Some((tab_id, tab_state)) =
            self.app.connection_tabs.get_key_value(&self.app.active_tab)
        {
            div().flex().flex_col().flex_1().min_h_0().child(
                ConnectionTab::new(self.app, (*tab_id).clone(), tab_state).render(window, cx),
            )
        } else {
            div().flex().flex_col().flex_1().min_h_0().child(
                div().flex().items_center().justify_center().flex_1().child(
                    div()
                        .text_sm()
                        // TODO: 等待主题增加 disabled.foreground 键后迁移
                        .text_color(gpui_kit::rgb(0x9ca3af))
                        .child(t!("tab_container.no_connection").to_string()),
                ),
            )
        }
    }
}
