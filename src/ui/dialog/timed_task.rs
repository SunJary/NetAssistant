// 定时任务(心跳)—— 添加 / 编辑对话框
//
// 入口: 发送任务面板标题栏「＋ 添加到定时任务」; 卡片 ✎ 复用同一对话框编辑。
// 每连接仅 1 个定时任务: 已存在时入口置灰。
// 内容 = 单条消息(可含 `${...}` 变量), 按固定间隔无限循环发送;
// 持久化在 `storage.timed_tasks`(按 connection_id), 连接后自动启动、断线暂停、重连恢复。
//
// 打开/关闭沿用 gpui_component 命令式对话框惯例(见 import_file.rs / stress_config.rs)。
// 状态挂在 `app.timed_task_dialog`, 内容闭包每帧从 app 读取最新状态。

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::StyledExt;
use gpui_kit::component::Theme;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{EditorState, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::TrailerKind;
use crate::send_task::TimedTaskProfile;
use crate::ui::components::hex_editor::{HexEditorState, adapter as hex_adapter};
use crate::ui::components::input_with_mode::InputWithMode;
use crate::utils::hex::{convert_value, validate_hex_input};

use super::dialog_height;
use super::import_file::trailer_hint_text;

/// 「定时任务」对话框状态(打开时创建, 取消/确定后由 app 置 None)
pub struct TimedTaskDialogState {
    pub tab_id: String,
    /// 编辑已有任务(标题与按钮文案不同)
    pub editing: bool,
    /// 持久化的启用态; 新建恒为 true
    pub enabled: bool,
    /// 跟随连接 message_input_mode(创建时), 可在对话框内切换
    pub hex_mode: bool,
    pub message_input: Entity<EditorState>,
    /// HEX 模式的网格编辑器视图状态(仅视图; 真源是 `message_input` 的 hex 文本)
    pub message_hex_editor: Entity<HexEditorState>,
    pub interval_input: Entity<InputState>,
    /// 校验错误提示(Some 时禁止确定)
    pub error: Option<String>,
}

impl TimedTaskDialogState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tab_id: String,
        editing: bool,
        enabled: bool,
        hex_mode: bool,
        message: String,
        interval_ms: u64,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        let message_input = cx.new(|cx| {
            EditorState::new(window, cx)
                // 多行 JSON 高亮编辑器(EditorMode 默认多行, 与其他多行输入框一致)
                .language("json")
                .line_number(false)
                .folding(false)
                // 关闭 Input 内置原生右键菜单: 由 InputWithMode 统一挂「转换为 Hex/文本」
                .context_menu(false)
                .placeholder(t!("timed_task.message_placeholder"))
        });
        message_input.update(cx, |input, cx| {
            input.set_value(message, window, cx);
        });
        let interval_input = cx.new(|cx| InputState::new(window, cx));
        interval_input.update(cx, |input, cx| {
            input.set_value(interval_ms.to_string(), window, cx);
        });
        // HEX 网格编辑器(与消息发送框同规格: 16 字节/行)
        let message_hex_editor = cx.new(|cx| {
            HexEditorState::with_inline_bytes_per_row(cx, hex_adapter::INLINE_BYTES_PER_ROW_WIDE)
        });
        Self {
            tab_id,
            editing,
            enabled,
            hex_mode,
            message_input,
            message_hex_editor,
            interval_input,
            error: None,
        }
    }

    pub fn message(&self, cx: &App) -> String {
        self.message_input.read(cx).text().to_string()
    }

    /// 间隔(ms); 空/非法/为 0 时返回 None
    pub fn interval_ms(&self, cx: &App) -> Option<u64> {
        match self.interval_input.read(cx).value().trim().parse::<u64>() {
            Ok(ms) if ms > 0 => Some(ms),
            _ => None,
        }
    }

    /// 是否可确认: 消息非空 + 间隔合法 + (hex 模式)内容为合法 HEX
    pub fn can_confirm(&self, cx: &App) -> bool {
        let message = self.message(cx);
        if message.trim().is_empty() || self.interval_ms(cx).is_none() {
            return false;
        }
        if self.hex_mode && !validate_hex_input(&message) {
            return false;
        }
        true
    }

    /// 构建持久化 profile(保留 `enabled`)
    pub fn to_profile(&self, cx: &App) -> TimedTaskProfile {
        TimedTaskProfile {
            enabled: self.enabled,
            message: self.message(cx),
            hex_mode: self.hex_mode,
            interval_ms: self.interval_ms(cx).unwrap_or(30_000),
        }
    }
}

/// 打开对话框(命令式, 由 Root 管理层叠)
pub fn open_timed_task_dialog(app: WeakEntity<NetAssistantApp>, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, cx| {
        let editing = app
            .upgrade()
            .and_then(|e| e.read(cx).timed_task_dialog.as_ref().map(|s| s.editing))
            .unwrap_or(false);
        let title = if editing {
            t!("timed_task.edit_title").to_string()
        } else {
            t!("timed_task.add_title").to_string()
        };
        dialog
            .title(title)
            .w(px(520.0))
            .max_h(dialog_height(window))
            .keyboard(false)
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.timed_task_dialog = None;
                        cx.notify();
                    });
                    true
                }
            })
            .footer(render_footer(&app, cx))
            .content({
                let app = app.clone();
                move |content, window, cx| {
                    let Some(entity) = app.upgrade() else {
                        return content;
                    };
                    // 渲染前同步 hex 编辑器: 先 clone 出实体(释放对 cx 的不可变借用),
                    // 再以 &mut App 按最新输入值重解析(sync 需要 &mut App)
                    let synced = entity
                        .read(cx)
                        .timed_task_dialog
                        .as_ref()
                        .map(|s| (s.message_input.clone(), s.message_hex_editor.clone()));
                    if let Some((input, editor)) = synced {
                        hex_adapter::sync(&editor, &input, cx);
                    }
                    let body = render_body(&entity, window, cx);
                    content.child(body)
                }
            })
    });
}

/// 渲染对话框主体
fn render_body(app: &Entity<NetAssistantApp>, window: &Window, cx: &App) -> Div {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let Some(s) = state.timed_task_dialog.as_ref() else {
        return div();
    };

    let trailer_kind = state
        .connection_tabs
        .get(&s.tab_id)
        .map(|t| t.send_trailer_setting.get())
        .unwrap_or(TrailerKind::None);

    let mut body = div()
        .flex()
        .flex_col()
        .gap_3()
        .px_6()
        .pb_4()
        // 消息
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(t!("timed_task.message").to_string()),
                )
                // 文本 / HEX 双模式: 复用 InputWithMode(hex 模式为十六进制网格;
                // 调用前已由 content 闭包完成 hex_adapter::sync)
                .child(InputWithMode::render(
                    &s.message_input,
                    Some(&s.message_hex_editor),
                    if s.hex_mode { "hex" } else { "text" },
                    &theme,
                    window,
                    cx,
                )),
        )
        // 模式(文本 / HEX)
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(t!("timed_task.mode").to_string()),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(mode_chip(app, s.hex_mode, false, &theme))
                        .child(mode_chip(app, s.hex_mode, true, &theme)),
                ),
        )
        // 间隔(ms)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(t!("timed_task.interval").to_string()),
                )
                .child(
                    div()
                        .w_24()
                        .h_7()
                        .bg(theme.background)
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            Input::new(&s.interval_input)
                                .w_full()
                                .h_full()
                                .bg(theme.background)
                                .rounded_md()
                                .border_0()
                                .text_center(),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("ms".to_string()),
                ),
        );

    // 结尾追加(只读, 跟随发送区连接级配置)
    body = body.child(
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(trailer_hint_text(trailer_kind)),
    );

    // 错误提示
    if let Some(err) = s.error.clone() {
        body = body.child(
            div()
                .p_2()
                .rounded_md()
                .bg(theme.danger.opacity(0.12))
                .border_1()
                .border_color(theme.danger)
                .child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(theme.foreground)
                        .child(err),
                ),
        );
    }

    div().max_h(super::dialog_content_max_height(window)).child(
        div()
            .id("timed-task-scroll")
            .overflow_y_scrollbar()
            .child(body),
    )
}

/// 模式分段按钮(文本 / HEX)
fn mode_chip(app: &Entity<NetAssistantApp>, current: bool, hex: bool, theme: &Theme) -> Div {
    let selected = current == hex;
    let entity = app.clone();
    let label = if hex {
        t!("timed_task.mode_hex").to_string()
    } else {
        t!("timed_task.mode_text").to_string()
    };
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |d| {
            d.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!selected, |d| {
            d.bg(theme.border).text_color(theme.foreground)
        })
        .child(div().text_sm().font_medium().child(label))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            entity.update(cx, |app, cx| {
                let Some(s) = app.timed_task_dialog.as_ref() else {
                    return;
                };
                if s.hex_mode == hex {
                    return;
                }
                let input = s.message_input.clone();
                let value = input.read(cx).value().to_string();
                let converted = convert_value(
                    &value,
                    if hex { "text" } else { "hex" },
                    if hex { "hex" } else { "text" },
                );
                // hex → text 且内容非法时不切换(不擅自改动用户内容)
                if converted.is_none() && !hex {
                    return;
                }
                if let Some(s) = app.timed_task_dialog.as_mut() {
                    s.hex_mode = hex;
                    s.error = None;
                }
                if let Some(next) = converted {
                    let next = if hex {
                        crate::ui::components::hex_editor::adapter::normalize_hex_value(&next)
                            .unwrap_or(next)
                    } else {
                        next
                    };
                    input.update(cx, |input, cx| input.replace_all(next, window, cx));
                }
                cx.notify();
            });
        })
}

/// 渲染底部操作按钮
fn render_footer(app: &WeakEntity<NetAssistantApp>, cx: &App) -> DialogFooter {
    let can_confirm = app
        .upgrade()
        .and_then(|entity| {
            entity
                .read(cx)
                .timed_task_dialog
                .as_ref()
                .map(|s| s.can_confirm(cx))
        })
        .unwrap_or(false);

    let app_cancel = app.clone();
    let footer = DialogFooter::new().child(
        Button::new("timed-task-cancel")
            .outline()
            .label(t!("timed_task.cancel").to_string())
            .on_click(move |_, window, cx| {
                let _ = app_cancel.update(cx, |app, cx| {
                    app.timed_task_dialog = None;
                    cx.notify();
                });
                window.close_dialog(cx);
            }),
    );

    let app_save = app.clone();
    let save = Button::new("timed-task-save")
        .primary()
        .label(t!("timed_task.save").to_string());
    let save = if can_confirm {
        save.on_click(move |_, window, cx| {
            let _ = app_save.update(cx, |app, cx| app.save_timed_task(cx));
            window.close_dialog(cx);
        })
    } else {
        save.disabled(true)
    };
    footer.child(save)
}