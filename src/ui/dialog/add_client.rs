// 添加客户端对话框
//
// 基于 gpui_component::Dialog 实现:
// 通过 window.open_dialog 命令式打开(Root 管理对话框栈), 键盘 Enter 确认 / ESC 取消
// 由 Dialog 的 on_ok/on_cancel 处理, 校验错误提示动态显示在输入框下方。

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::ActiveTheme as _;
use gpui_component::WindowExt as _;
use gpui_component::input::{Input, InputState};
use rust_i18n::t;

use crate::app::NetAssistantApp;

use super::{dialog_content_max_height, dialog_height};

/// 打开「添加客户端」对话框(命令式, 由 Root 管理层叠)
///
/// `input` 为调用方新建的地址输入实体; 校验错误保存在 `app.add_client_dialog_error`,
/// 内容闭包每帧读取, 动态刷新错误行。
pub fn open_add_client_dialog(
    app: WeakEntity<NetAssistantApp>,
    tab_id: String,
    input: Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, _cx| {
        dialog
            .title(t!("add_client.title").to_string())
            .w(px(320.0))
            .max_h(dialog_height(window))
            // 单输入框表单: 保留 Enter 确认 / ESC 取消(与迁移前行为一致)
            .keyboard(true)
            .on_ok({
                let app = app.clone();
                let input = input.clone();
                let tab_id = tab_id.clone();
                move |_, _, cx| confirm_add_client(&app, &input, &tab_id, cx)
            })
            // X 按钮 / 蒙层关闭时同步清理错误提示
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.add_client_dialog_error = None;
                        cx.notify();
                    });
                    true
                }
            })
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new("add-client-cancel")
                                .outline()
                                .label(t!("add_client.cancel").to_string()),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new("add-client-ok")
                                .primary()
                                .label(t!("add_client.confirm").to_string()),
                        ),
                    ),
            )
            .content({
                let app = app.clone();
                let input = input.clone();
                move |content, window, cx| {
                    let theme = cx.theme().clone();
                    let error = app
                        .upgrade()
                        .and_then(|entity| entity.read(cx).add_client_dialog_error.clone());
                    // 滚动结构(经 tests/dialog_layout.rs 无头测试验证): max_h 在外层钳制可视区,
                    // 内层滚动容器不限高; 极矮窗口下错误提示也可滚动
                    content.child(
                        div().max_h(dialog_content_max_height(window)).child(
                            div()
                                .overflow_y_scrollbar()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(t!("add_client.address_hint").to_string()),
                                )
                                .child(Input::new(&input))
                                // 验证错误提示
                                .when_some(error, |el, err| {
                                    el.child(div().text_xs().text_color(theme.danger).child(err))
                                }),
                        ),
                    )
                }
            })
    });
}

/// 校验并提交客户端地址, 返回是否成功(成功后由调用方关闭对话框)
fn confirm_add_client(
    app: &WeakEntity<NetAssistantApp>,
    input: &Entity<InputState>,
    tab_id: &str,
    cx: &mut App,
) -> bool {
    app.update(cx, |app, cx| {
        let addr_str = input.read(cx).value().to_string();
        match validate_address(&addr_str) {
            Ok(()) => {
                app.add_client_dialog_error = None;
                app.add_client_to_server(tab_id.to_string(), addr_str.trim().to_string(), cx);
                cx.notify();
                true
            }
            Err(msg) => {
                app.add_client_dialog_error = Some(msg.to_string());
                cx.notify();
                false
            }
        }
    })
    .unwrap_or(false)
}

/// 验证地址格式：必须是 IP:端口
fn validate_address(addr: &str) -> Result<(), std::borrow::Cow<'static, str>> {
    if addr.trim().is_empty() {
        return Err(t!("add_client.address_empty"));
    }
    if !addr.contains(':') {
        return Err(t!("add_client.invalid_format"));
    }
    if addr.parse::<std::net::SocketAddr>().is_err() {
        return Err(t!("add_client.invalid_address_format"));
    }
    Ok(())
}
