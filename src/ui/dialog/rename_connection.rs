// 连接重命名对话框
//
// 基于 gpui_kit::component::Dialog 实现:
// 通过 window.open_dialog 命令式打开(Root 管理对话框栈), 键盘 Enter 确认 / ESC 取消。
// 名称允许为空(空串 = 清除名称, 恢复仅显示地址)。

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::input::Input;
use gpui_kit::*;
use rust_i18n::t;

use crate::app::NetAssistantApp;

use super::dialog_height;

/// 打开「重命名连接」对话框(命令式, 由 Root 管理层叠)
///
/// 待重命名连接由调用方先写入 `app.rename_connection_id`, 输入框复用
/// `app.rename_connection_input`。
pub fn open_rename_connection_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        let Some(input) = app
            .upgrade()
            .map(|entity| entity.read(cx).rename_connection_input.clone())
        else {
            return dialog;
        };

        dialog
            .title(t!("rename_connection.title").to_string())
            .w(px(320.0))
            .max_h(dialog_height(window))
            // 单输入框表单: 保留 Enter 确认 / ESC 取消
            .keyboard(true)
            .on_ok({
                let app = app.clone();
                move |_, _, cx| {
                    app.update(cx, |app, cx| app.confirm_rename_connection(cx))
                        .unwrap_or(false)
                }
            })
            // X 按钮 / 蒙层关闭时清理重命名态
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.rename_connection_id = None;
                        cx.notify();
                    });
                    true
                }
            })
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new("rename-connection-cancel")
                                .outline()
                                .label(t!("rename_connection.cancel").to_string()),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new("rename-connection-ok")
                                .primary()
                                .label(t!("rename_connection.confirm").to_string()),
                        ),
                    ),
            )
            .content(move |content, _window, _cx| content.child(Input::new(&input)))
    });
}
