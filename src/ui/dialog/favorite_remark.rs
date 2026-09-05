// 收藏备注对话框
//
// 基于 gpui_component::Dialog 实现:
// 通过 window.open_dialog 命令式打开(Root 管理对话框栈), 键盘 Enter 确认 / ESC 取消
// 由 Dialog 的 on_ok/on_cancel 处理。

use gpui::*;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_component::input::{Input, InputState};
use rust_i18n::t;
use std::sync::Arc;

use crate::app::NetAssistantApp;
use crate::message::FavoriteItem;

use super::dialog_height;

/// 打开「收藏备注」对话框(命令式, 由 Root 管理层叠)
///
/// 待收藏内容由调用方先写入 `app.favorite_remark_content` / `favorite_remark_message_type` /
/// `favorite_remark_tab_id`, 备注输入框复用 `app.favorite_remark_input`。
pub fn open_favorite_remark_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        let Some(input) = app
            .upgrade()
            .map(|entity| entity.read(cx).favorite_remark_input.clone())
        else {
            return dialog;
        };

        dialog
            .title(t!("favorite_remark.title").to_string())
            .w(px(320.0))
            .max_h(dialog_height(window))
            // 单输入框表单: 保留 Enter 确认 / ESC 取消(与迁移前行为一致)
            .keyboard(true)
            .on_ok({
                let app = app.clone();
                let input = input.clone();
                move |_, _, cx| confirm_favorite_remark(&app, &input, cx)
            })
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new("favorite-remark-cancel")
                                .outline()
                                .label(t!("favorite_remark.cancel").to_string()),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new("favorite-remark-ok")
                                .primary()
                                .label(t!("favorite_remark.confirm").to_string()),
                        ),
                    ),
            )
            .content(move |content, _window, _cx| content.child(Input::new(&input)))
    });
}

/// 保存收藏备注, 返回是否成功(成功后由调用方关闭对话框)
fn confirm_favorite_remark(
    app: &WeakEntity<NetAssistantApp>,
    input: &Entity<InputState>,
    cx: &mut App,
) -> bool {
    app.update(cx, |app, cx| {
        let remark = input.read(cx).value().to_string();
        if remark.trim().is_empty() {
            return false;
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
                Arc::make_mut(&mut tab_state.favorited_contents).insert(content_for_cache);
            }
        }

        cx.notify();
        true
    })
    .unwrap_or(false)
}
