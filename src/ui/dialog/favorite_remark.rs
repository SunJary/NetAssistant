use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, input::{Input, InputState}};

use crate::app::NetAssistantApp;
use crate::message::FavoriteItem;

pub struct FavoriteRemarkDialog {
    remark_input: Entity<InputState>,
}

impl FavoriteRemarkDialog {
    pub fn new(_app: &NetAssistantApp, remark_input: Entity<InputState>) -> Self {
        Self { remark_input }
    }

    pub fn render(
        self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let remark_input = self.remark_input.clone();
        let remark_input_for_key = remark_input.clone();

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x80000000))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        app.show_favorite_remark = false;
                        cx.notify();
                    }
                    "enter" => {
                        let remark = remark_input_for_key.read(cx).value().to_string();
                        if remark.trim().is_empty() {
                            return;
                        }

                        if let (Some(content), Some(message_type), Some(tab_id)) = (
                            app.favorite_remark_content.take(),
                            app.favorite_remark_message_type.take(),
                            app.favorite_remark_tab_id.take(),
                        ) {
                            let content_for_cache = content.clone();
                            let item = FavoriteItem::new(content, message_type, remark.trim().to_string());
                            app.storage.add_favorite(&tab_id, item);
                            if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                tab_state.favorited_contents.insert(content_for_cache);
                            }
                        }

                        app.show_favorite_remark = false;
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .w_80()
                    .bg(theme.muted)
                    .rounded_lg()
                    .shadow_2xl()
                    .p_5()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {})
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .mb_3()
                            .text_color(theme.foreground)
                            .child("添加收藏备注")
                    )
                    .child(
                        div()
                            .mb_3()
                            .child(
                                Input::new(&remark_input)
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .hover(|s| s.bg(theme.secondary))
                                    .child("取消")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
                                        app.show_favorite_remark = false;
                                        cx.notify();
                                    }))
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_sm()
                                    .bg(theme.primary)
                                    .text_color(theme.background)
                                    .hover(|s| s.opacity(0.8))
                                    .child("确定")
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, _event, _window, cx| {
                                        let remark = remark_input.read(cx).value().to_string();
                                        if remark.trim().is_empty() {
                                            return;
                                        }

                                        if let (Some(content), Some(message_type), Some(tab_id)) = (
                                            app.favorite_remark_content.take(),
                                            app.favorite_remark_message_type.take(),
                                            app.favorite_remark_tab_id.take(),
                                        ) {
                                            let content_for_cache = content.clone();
                                            let item = FavoriteItem::new(content, message_type, remark.trim().to_string());
                                            app.storage.add_favorite(&tab_id, item);
                                            if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                                tab_state.favorited_contents.insert(content_for_cache);
                                            }
                                        }

                                        app.show_favorite_remark = false;
                                        cx.notify();
                                    }))
                            )
                    )
            )
    }
}
