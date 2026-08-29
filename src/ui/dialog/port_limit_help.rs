// 端口上限说明弹窗
//
// 在压测配置弹窗中点击 "端口说明" 按钮时叠加显示。
// 内容按运行时平台 cfg!(target_os) 条件编译, 只展示当前平台相关信息。
// 调优命令行末附 Clipboard 复制按钮, 方便用户直接复制执行。
// 布局复用 stress_config.rs 的两层滚动结构:
//   外层 flex_1() + overflow_hidden(), 内层 size_full() + overflow_y_scrollbar()

use std::borrow::Cow;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::clipboard::Clipboard;
use gpui_component::scroll::ScrollableElement;
use gpui_component::StyledExt;
use gpui_component::Theme;

use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::ui::dialog::stress_config::StressConfigDialogState;

/// 渲染端口说明弹窗 (全屏蒙层, 覆盖在压测配置弹窗之上)
pub fn render_port_limit_help_dialog(
    app: &NetAssistantApp,
    _state: &StressConfigDialogState,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<NetAssistantApp>,
) -> impl IntoElement {
    let win_h = (window.bounds().size.height / px(1.0)) as f32;
    let dialog_height = (win_h * 0.8_f32).max(500.0);

    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba(0x80000000))
        .p_4()
        // 拦截蒙层背景的鼠标按下事件, 防止穿透到下层压测配置弹窗的按钮
        // (点击背景空白不关闭, 避免误操作; 只用关闭按钮关闭)
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .w(px(560.0))
                .h(px(dialog_height))
                .overflow_hidden()
                .flex()
                .flex_col()
                .bg(theme.muted)
                .rounded_lg()
                .shadow_2xl()
                // 标题区
                .child(
                    div()
                        .px_6()
                        .pt_6()
                        .pb_4()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_lg()
                                .font_semibold()
                                .text_color(theme.foreground)
                                .child(t!("port_limit_help.title").to_string()),
                        ),
                )
                // 滚动内容区 (两层结构, 避免 flex 高度分配冲突)
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div()
                                .size_full()
                                .overflow_y_scrollbar()
                                .px_6()
                                .pb_4()
                                .child(render_help_content(app, theme, cx)),
                        ),
                )
                // 底部关闭按钮
                .child(
                    div()
                        .px_6()
                        .pb_6()
                        .pt_2()
                        .border_t_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .w_full()
                                .p_2()
                                .bg(theme.primary)
                                .rounded_md()
                                .cursor_pointer()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.primary_foreground)
                                        .text_center()
                                        .child(t!("port_limit_help.close").to_string()),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                                        // 阻止事件继续传播, 防止穿透到下层压测配置弹窗的按钮
                                        cx.stop_propagation();
                                        if let Some(s) = &mut app.stress_config_dialog {
                                            s.show_port_help = false;
                                        }
                                        cx.notify();
                                    }),
                                ),
                        ),
                ),
        )
}

/// 渲染说明内容 (按平台条件编译)
fn render_help_content(
    app: &NetAssistantApp,
    theme: &Theme,
    cx: &mut Context<NetAssistantApp>,
) -> Div {
    // 端口范围描述: 检测成功显示真实值; 检测中显示"读取中…"; 未检测/失败显示"获取失败"
    // 不再回退 fallback_default 编造数字, 失败时引导用户用下方命令或重新检测
    let range_desc = if app.port_range_detecting {
        t!("port_limit_help.reading_port_config").to_string()
    } else {
        match &app.detected_port_range {
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
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                                // 阻止事件继续传播, 防止穿透到下层压测配置弹窗的按钮
                                cx.stop_propagation();
                                app.trigger_port_range_detect(cx);
                            }),
                        ),
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
