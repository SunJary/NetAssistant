use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::{ActiveTheme, StyledExt, Icon, IconName, input::{Input, InputState}, scroll::ScrollableElement};

use crate::app::NetAssistantApp;
use crate::message::FavoriteItem;

pub struct FavoriteListPanel<'a> {
    app: &'a NetAssistantApp,
    search_input: Entity<InputState>,
}

impl<'a> FavoriteListPanel<'a> {
    pub fn new(app: &'a NetAssistantApp, search_input: Entity<InputState>) -> Self {
        Self { app, search_input }
    }

    pub fn render(
        self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let search_input = self.search_input.clone();
        let tab_id = self.app.favorite_list_tab_id.clone().unwrap_or_default();
        let pos_x = self.app.favorite_list_position.unwrap_or(px(0.0));
        let pos_y = self.app.favorite_list_position_y.unwrap_or(px(0.0));
        let search_keyword = search_input.read(cx).value().to_string().to_lowercase();

        let mut favorites: Vec<FavoriteItem> = self.app.storage.get_favorites_ref(&tab_id)
            .iter()
            .filter(|item| {
                if search_keyword.is_empty() {
                    true
                } else {
                    item.remark.to_lowercase().contains(&search_keyword)
                        || item.content.to_lowercase().contains(&search_keyword)
                }
            })
            .cloned()
            .collect();

        favorites.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        div()
            .absolute()
            .inset_0()
            .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
                app.show_favorite_list = false;
                cx.notify();
            }))
            .child(
                div()
                    .absolute()
                    .left(pos_x - px(320.0))
                    .top(pos_y - px(300.0) - px(4.0))
                    .w(px(320.0))
                    .h(px(300.0))
                    .overflow_hidden()
                    .bg(theme.muted)
                    .rounded_lg()
                    .shadow_2xl()
                    .border(px(1.0))
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {})
                    .on_key_down(cx.listener(|app, event: &KeyDownEvent, _window, cx| {
                        if event.keystroke.key.as_str() == "escape" {
                            app.show_favorite_list = false;
                            cx.notify();
                        }
                    }))
                    .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child("收藏列表")
                    )
                    .child(
                        div()
                            .px_2()
                            .py(px(1.0))
                            .rounded_md()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .hover(|s| s.bg(theme.secondary))
                            .child("关闭")
                            .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
                                app.show_favorite_list = false;
                                cx.notify();
                            }))
                    )
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Search).size(px(12.0)))
                            .child(
                                div()
                                    .flex_1()
                                    .child(
                                        Input::new(&search_input)
                                    )
                            )
                    )
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .px_3()
                    .pb_3()
                    .when(favorites.is_empty(), |el| {
                        el.child(
                            div()
                                .py_6()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(if search_keyword.is_empty() { "暂无收藏" } else { "未找到匹配的收藏" })
                        )
                    })
                    .children(favorites.into_iter().map(|item| {
                        let item_message_type = item.message_type;
                        let item_id = item.id.clone();
                        let tab_id_for_click = tab_id.clone();
                        let tab_id_for_delete = tab_id.clone();
                        let item_content_for_click = item.content.clone();
                        let item_content_for_delete = item.content.clone();
                        let item_remark = item.remark.clone();
                        let item_created_at = item.created_at.clone();
                        let full_content = item.content.clone();

                        div()
                            .mt_1()
                            .p_2()
                            .rounded_md()
                            .bg(theme.background)
                            .border(px(1.0))
                            .border_color(theme.border)
                            .cursor_pointer()
                            .hover(|s| s.border_color(theme.primary))
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .px_1()
                                            .py(px(1.0))
                                            .rounded(px(3.0))
                                            .bg(theme.primary)
                                            .text_xs()
                                            .font_medium()
                                            .text_color(theme.primary_foreground)
                                            .max_w(px(60.0))
                                            .whitespace_nowrap()
                                            .overflow_x_hidden()
                                            .child(item_remark.clone())
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .whitespace_normal()
                                            .overflow_hidden()
                                            .max_h(px(36.0))
                                            .hover(|s| s.max_h(px(600.0)))
                                            .child(full_content.clone())
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .px_1()
                                            .rounded(px(2.0))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme.secondary))
                                            .child(
                                                Icon::new(IconName::Close)
                                                    .size(px(10.0))
                                                    .text_color(theme.muted_foreground)
                                            )
                                            .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                                cx.stop_propagation();
                                                app.storage.remove_favorite(&tab_id_for_delete, &item_id);
                                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_for_delete) {
                                                    tab_state.favorited_contents.remove(&item_content_for_delete);
                                                }
                                                cx.notify();
                                            }))
                                    )
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .px_1()
                                            .rounded(px(2.0))
                                            .bg(theme.secondary)
                                            .text_color(theme.muted_foreground)
                                            .child(format!("{:?}", item_message_type))
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(item_created_at)
                                    )
                            )
                            .on_mouse_down(MouseButton::Left, cx.listener(move |app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id_for_click) {
                                    tab_state.message_input_mode = match item_message_type {
                                        crate::message::MessageType::Text => String::from("text"),
                                        crate::message::MessageType::Hex => String::from("hex"),
                                    };
                                    if let Some(input) = tab_state.message_input.as_ref() {
                                        input.update(cx, |state, inner_cx| {
                                            state.set_value(&item_content_for_click, window, inner_cx);
                                        });
                                    }
                                }
                                app.show_favorite_list = false;
                                cx.notify();
                            }))
                    }))
            )
            )
    }
}
