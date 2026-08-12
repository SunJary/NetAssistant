// 端口上限说明弹窗
//
// 在压测配置弹窗中点击 "端口说明" 按钮时叠加显示。
// 内容按运行时平台 cfg!(target_os) 条件编译, 只展示当前平台相关信息。
// 调优命令行末附 Clipboard 复制按钮, 方便用户直接复制执行。
// 布局复用 stress_config.rs 的两层滚动结构:
//   外层 flex_1() + overflow_hidden(), 内层 size_full() + overflow_y_scrollbar()

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::clipboard::Clipboard;
use gpui_component::scroll::ScrollableElement;
use gpui_component::StyledExt;
use gpui_component::Theme;

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
                                .child("临时端口与压测并发上限说明"),
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
                                        .child("关闭"),
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
        "正在读取系统端口配置…".to_string()
    } else {
        match &app.detected_port_range {
            Some(r) => format!("{}-{} (共约 {} 个端口)", r.start, r.end(), r.count),
            None => "获取失败, 请点击「重新检测」或使用下方命令手动查看".to_string(),
        }
    };

    div()
        .flex()
        .flex_col()
        .gap_4()
        // ===== 通用: 当前端口范围 (检测结果或默认值) =====
        .child(section_title("当前临时端口范围"))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
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
                        .child("重新检测")
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
        .child(section_title("TCP 短连接与 TIME_WAIT"))
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child("TCP 连接由四元组 (源IP, 源端口, 目标IP, 目标端口) 唯一标识。压测同一目标时，源IP / 目标IP / 目标端口三者固定，只有源端口可变。短连接关闭后，该四元组进入 TIME_WAIT 状态 (60-120秒)，期间此端口不能被同一程序用于连接同一目标。")
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child("稳态下短连接占用的端口数 ≈ 并发数 × (TIME_WAIT时长 / 发包间隔)。发包间隔越短，端口消耗放大越严重，建议改用长连接模式或降低并发。")
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child("UDP 不走 TCP 连接，无 TIME_WAIT 问题，端口消耗仅与并发数相关。")
        )
        // ===== 平台相关: 调优命令 =====
        .child(render_platform_tune_section(theme))
        // ===== 通用: 应用兜底机制 =====
        .child(section_title("超过端口上限会怎样"))
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child("并发超过临时端口范围时，新建连接会报 AddrNotAvailable (Linux/macOS) 或 WSAEADDRINUSE (Windows) 错误，压测的连接失败率会骤升。NetAssistant 会在压测结束后的 stress_failure_*.log 中给出诊断建议。"),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("注: 应用自身的端口范围检测全程只需普通用户权限，不涉及提权。调优命令需用户自行以管理员身份执行。"),
        )
}

/// 平台相关: 查看端口范围的命令
fn render_platform_view_section(theme: &Theme) -> Div {
    let (cmd, note) = platform_view_command();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_title("查看当前端口范围"))
        .child(render_command_line(cmd, note, theme, "view-port-range"))
}

/// 平台相关: 调优命令
fn render_platform_tune_section(theme: &Theme) -> Div {
    let cmds = platform_tune_commands();

    let mut container = div().flex().flex_col().gap_2();
    container = container.child(section_title("调大端口范围"));

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
fn platform_view_command() -> (&'static str, &'static str) {
    #[cfg(target_os = "windows")]
    {
        ("netsh int ipv4 show dynamicport tcp", "普通用户可执行")
    }
    #[cfg(target_os = "linux")]
    {
        ("cat /proc/sys/net/ipv4/ip_local_port_range", "普通用户可读")
    }
    #[cfg(target_os = "macos")]
    {
        (
            "sysctl net.inet.ip.portrange.first net.inet.ip.portrange.last",
            "普通用户可执行",
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        ("", "当前平台不支持自动检测")
    }
}

/// 返回当前平台的调优命令列表 [(命令, 权限/说明), ...]
fn platform_tune_commands() -> Vec<(&'static str, &'static str)> {
    #[cfg(target_os = "windows")]
    {
        vec![
            (
                "netsh int ipv4 set dynamicport tcp start=10000 num=55535",
                "需管理员 CMD; 将 TCP 临时端口扩展为 10000-65535",
            ),
            (
                "netsh int ipv6 set dynamicport tcp start=10000 num=55535",
                "需管理员 CMD; IPv6 也一并调大 (推荐)",
            ),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            (
                "sudo sysctl -w net.ipv4.ip_local_port_range=\"10000 65535\"",
                "需 sudo; 将临时端口扩展为 10000-65535",
            ),
            (
                "echo 'net.ipv4.ip_local_port_range = 10000 65535' | sudo tee -a /etc/sysctl.d/99-port-range.conf",
                "需 sudo; 写入配置文件, 重启后生效",
            ),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        // macOS 默认 49152-65535, 调优空间有限, 仅在必要时调整
        vec![(
            "sudo sysctl -w net.inet.ip.portrange.first=32768",
            "需 sudo; 将起始端口从 49152 降到 32768, 扩展约 16384 个端口",
        )]
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        vec![]
    }
}
