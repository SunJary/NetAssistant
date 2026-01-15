use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::StyledExt;

use crate::app::NetAssistantApp;
use crate::ui::connection_tab::ConnectionTab;

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
        let tabs = self.get_tabs();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .bg(gpui::rgb(0xffffff))
            .child(self.render_tab_header(&tabs, cx))
            .child(self.render_tab_content(window, cx))
    }

    /// 获取所有标签页（只显示已创建的标签页）
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
        let mut header_div = div()
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .bg(gpui::rgb(0xf3f4f6))
            .border_b_1()
            .border_color(gpui::rgb(0xe5e7eb));

        for (index, tab) in tabs.iter().enumerate() {
            let tab_id = tab.id.clone();
            let is_active = tab.is_active;
            let tab_name = tab.name.clone();

            header_div = header_div.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .hover(|style| {
                        style.bg(gpui::rgb(0xe5e7eb))
                    })
                    .when(is_active, |div| {
                        div.bg(gpui::rgb(0xffffff))
                            .border_1()
                            .border_color(gpui::rgb(0xe5e7eb))
                            .border_b_0()
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
                            .when(is_active, |div| {
                                div.text_color(gpui::rgb(0x3b82f6))
                            })
                            .when(!is_active, |div| {
                                div.text_color(gpui::rgb(0x6b7280))
                            })
                            .child(tab_name),
                    )
                    .child(
                        div()
                            .id(("close-tab", index))
                            .text_xs()
                            .text_color(gpui::rgb(0x9ca3af))
                            .hover(|style| {
                                style.text_color(gpui::rgb(0xef4444))
                            })
                            .cursor_pointer()
                            .child("×")
                            .on_mouse_down(MouseButton::Left, {
                                let tab_id_clone = tab_id.clone();
                                cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                    app.close_tab(tab_id_clone.clone());

                                    if app.active_tab == tab_id_clone {
                                        if let Some(first_tab_id) = app.connection_tabs.keys().next() {
                                            app.active_tab = (*first_tab_id).to_string();
                                        } else {
                                            app.active_tab = String::new();
                                        }
                                    }
                                    cx.notify();
                                })
                            }),
                    ),
            );
        }

        header_div
    }

    /// 渲染标签页内容区域
    fn render_tab_content(
        &self,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        if let Some((tab_id, tab_state)) =
            self.app.connection_tabs.get_key_value(&self.app.active_tab)
        {
            div().flex().flex_col().flex_1().child(
                ConnectionTab::new(self.app, (*tab_id).clone(), tab_state).render(window, cx),
            )
        } else {
            div().flex().flex_col().flex_1().child(
                div().flex().items_center().justify_center().flex_1().child(
                    div()
                        .text_sm()
                        .text_color(gpui::rgb(0x9ca3af))
                        .child("请先创建连接"),
                ),
            )
        }
    }
}
