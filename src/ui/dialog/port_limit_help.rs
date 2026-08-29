// 端口上限说明对话框
//
// 基于 gpui_component::Dialog 实现: 在压测配置对话框之上通过第二个 window.open_dialog
// 叠开(Root 管理层叠, 蒙层/焦点/ESC 由组件管理), 关闭后压测配置保持不变。
// 内容按运行时平台 cfg!(target_os) 条件编译, 只展示当前平台相关信息。
// 调优命令行末附 Clipboard 复制按钮, 方便用户直接复制执行。
// 滚动结构为「外层 max_h 钳制可视区 + 内层 overflow_y_scrollbar」(见 dialog_content_max_height)。

use std::borrow::Cow;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::clipboard::Clipboard;
use gpui_component::dialog::DialogFooter;
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::Theme;
use gpui_component::WindowExt as _;

use rust_i18n::t;

use crate::app::NetAssistantApp;

use super::{dialog_content_max_height, dialog_height};

/// 打开端口说明对话框(命令式, 叠在压测配置对话框之上)
pub fn open_port_limit_help_dialog(app: WeakEntity<NetAssistantApp>, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, _cx| {
        dialog
            .title(t!("port_limit_help.title").to_string())
            .w(px(560.0))
            .max_h(dialog_height(window))
            // 帮助对话框: ESC / 蒙层 / 关闭按钮均可关闭, 无需额外清理
            .keyboard(true)
            .on_ok(|_, _, _| true)
            .footer(
                DialogFooter::new().child(
                    Button::new("port-help-close")
                        .primary()
                        .label(t!("port_limit_help.close").to_string())
                        .on_click(|_, window, cx| {
                            window.close_dialog(cx);
                        }),
                ),
            )
            .content({
                let app = app.clone();
                move |content, window, cx| {
                    let Some(entity) = app.upgrade() else {
                        return content;
                    };
                    let theme = cx.theme().clone();
                    content.child(
                        div().max_h(dialog_content_max_height(window)).child(
                            div()
                                .overflow_y_scrollbar()
                                .px_6()
                                .pb_4()
                                .child(render_help_content(&entity, &theme, cx)),
                        ),
                    )
                }
            })
    });
}

/// 渲染说明内容 (按平台条件编译)
fn render_help_content(app: &Entity<NetAssistantApp>, theme: &Theme, cx: &App) -> Div {
    let app_state = app.read(cx);
    // 端口范围描述: 检测成功显示真实值; 检测中显示"读取中…"; 未检测/失败显示"获取失败"
    // 不再回退 fallback_default 编造数字, 失败时引导用户用下方命令或重新检测
    let range_desc = if app_state.port_range_detecting {
        t!("port_limit_help.reading_port_config").to_string()
    } else {
        match &app_state.detected_port_range {
            Some(r) => t!(
                "port_limit_help.port_range_summary",
                start = r.start,
                end = r.end(),
                count = r.count
            )
            .to_string(),
            None => t!("port_limit_help.detect_failed").to_string(),
        }
    };

    div()
        .flex()
        .flex_col()
        .gap_4()
        // ===== 通用: 当前端口范围 (检测结果或默认值) =====
        .child(section_title(&t!("port_limit_help.section_current_range")))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(range_desc),
                )
                // 重新检测按钮: 用户在系统里改了端口范围后可点击重新检测
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.primary)
                        .cursor_pointer()
                        .hover(|d| d.underline())
                        .child(t!("port_limit_help.re_detect").to_string())
                        .on_mouse_down(MouseButton::Left, {
                            let entity = app.clone();
                            move |_, _, cx| {
                                entity.update(cx, |app, cx| {
                                    app.trigger_port_range_detect(cx);
                                });
                            }
                        }),
                ),
        )
        // ===== 平台相关: 查看命令 =====
        .child(render_platform_view_section(theme))
        // ===== 通用: TIME_WAIT 机制说明 =====
        .child(section_title(&t!("port_limit_help.section_time_wait")))
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(t!("port_limit_help.time_wait_desc_1").to_string())
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(t!("port_limit_help.time_wait_desc_2").to_string())
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(t!("port_limit_help.udp_desc").to_string())
        )
        // ===== 平台相关: 调优命令 =====
        .child(render_platform_tune_section(theme))
        // ===== 通用: 应用兜底机制 =====
        .child(section_title(&t!("port_limit_help.section_over_limit")))
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(t!("port_limit_help.over_limit_desc").to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(t!("port_limit_help.note_permission").to_string()),
        )
}

/// 平台相关: 查看端口范围的命令
fn render_platform_view_section(theme: &Theme) -> Div {
    let (cmd, note) = platform_view_command();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_title(&t!("port_limit_help.section_view_range")))
        .child(render_command_line(cmd, &note, theme, "view-port-range"))
}

/// 平台相关: 调优命令
fn render_platform_tune_section(theme: &Theme) -> Div {
    let cmds = platform_tune_commands();

    let mut container = div().flex().flex_col().gap_2();
    container = container.child(section_title(&t!("port_limit_help.section_tune_range")));

    for (i, (cmd, note)) in cmds.iter().enumerate() {
        container = container.child(render_command_line(
            cmd,
            note,
            theme,
            &format!("tune-port-range-{}", i),
        ));
    }

    container
}

/// 渲染单条命令行 (等宽字体 + 灰色背景 + 复制按钮)
fn render_command_line(cmd: &str, note: &str, theme: &Theme, id: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_3()
                        .py_2()
                        .bg(theme.border)
                        .rounded_md()
                        .text_xs()
                        .child(cmd.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            Clipboard::new(ElementId::Name(format!("port-help-{}", id).into()))
                                .value(cmd.to_string())
                                .on_copied(|value, _, _| {
                                    log::debug!("[端口说明] 已复制命令: {}", value);
                                }),
                        ),
                ),
        )
        .when(!note.is_empty(), |d| {
            d.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(note.to_string()),
            )
        })
}

/// 小节标题
fn section_title(text: &str) -> Div {
    div()
        .text_sm()
        .font_semibold()
        .text_color(gpui::rgba(0xef4444))
        .child(text.to_string())
}

/// 返回当前平台的查看命令 (命令, 权限/说明)
fn platform_view_command() -> (&'static str, Cow<'static, str>) {
    #[cfg(target_os = "windows")]
    {
        (
            "netsh int ipv4 show dynamicport tcp",
            t!("port_limit_help.perm_normal_user_exec"),
        )
    }
    #[cfg(target_os = "linux")]
    {
        (
            "cat /proc/sys/net/ipv4/ip_local_port_range",
            t!("port_limit_help.perm_normal_user_read"),
        )
    }
    #[cfg(target_os = "macos")]
    {
        (
            "sysctl net.inet.ip.portrange.first net.inet.ip.portrange.last",
            t!("port_limit_help.perm_normal_user_exec"),
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        (
            "",
            t!("port_limit_help.platform_not_supported"),
        )
    }
}

/// 返回当前平台的调优命令列表 [(命令, 权限/说明), ...]
fn platform_tune_commands() -> Vec<(&'static str, Cow<'static, str>)> {
    #[cfg(target_os = "windows")]
    {
        vec![
            (
                "netsh int ipv4 set dynamicport tcp start=10000 num=55535",
                t!("port_limit_help.tune_windows_tcp"),
            ),
            (
                "netsh int ipv6 set dynamicport tcp start=10000 num=55535",
                t!("port_limit_help.tune_windows_ipv6"),
            ),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            (
                "sudo sysctl -w net.ipv4.ip_local_port_range=\"10000 65535\"",
                t!("port_limit_help.tune_linux_sysctl"),
            ),
            (
                "echo 'net.ipv4.ip_local_port_range = 10000 65535' | sudo tee -a /etc/sysctl.d/99-port-range.conf",
                t!("port_limit_help.tune_linux_conf"),
            ),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        // macOS 默认 49152-65535, 调优空间有限, 仅在必要时调整
        vec![(
            "sudo sysctl -w net.inet.ip.portrange.first=32768",
            t!("port_limit_help.tune_macos"),
        )]
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        vec![]
    }
}
