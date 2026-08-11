// 压测监控面板
//
// 同步读 ConnectionTabState.stress_stats / stress_report 渲染。
// 所有控制按钮通过 cx.listener dispatch 到 app 方法, 无 async。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::Theme;

use crate::app::NetAssistantApp;
use crate::ui::connection_tab::ConnectionTabState;

/// 压测监控面板
pub struct StressPanel<'a> {
    tab_id: String,
    tab_state: &'a ConnectionTabState,
}

impl<'a> StressPanel<'a> {
    pub fn new(tab_id: String, tab_state: &'a ConnectionTabState) -> Self {
        Self { tab_id, tab_state }
    }

    pub fn render(
        self,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let stats = &self.tab_state.stress_stats;
        let report = &self.tab_state.stress_report;
        let is_running = self.tab_state.stress_engine.is_some() && report.is_none();
        let is_finished = report.is_some();
        // 延迟卡片仅在往返(ping-pong)模式下有意义: 吞吐模式不等待响应, 不采集 RTT
        let show_latency = self
            .tab_state
            .stress_config_snapshot
            .as_ref()
            .map(|c| c.is_ping_pong())
            .unwrap_or(true);

        div()
            .flex()
            .flex_col()
            .h_full()
            .w_full()
            .bg(theme.background)
            // 顶部控制条
            .child(self.render_control_bar(&theme, is_running, is_finished, cx))
            // 状态指示
            .child(self.render_status_bar(&theme, is_running, is_finished, stats))
            // 指标网格
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_4()
                    // QPS 大字
                    .child(self.render_qps_card(&theme, stats))
                    // 发包统计
                    .child(self.render_stats_grid(&theme, stats))
                    // 延迟卡片(仅 ping-pong)
                    .when(show_latency, |d| {
                        d.child(self.render_latency_cards(&theme, stats))
                    })
                    // 连接/字节统计
                    .child(self.render_connection_stats(&theme, stats))
                    // 失败分类(仅在有失败时显示)
                    .when_some(self.render_failure_breakdown(&theme, stats), |d, fb| {
                        d.child(fb)
                    }),
            )
    }

    /// 顶部控制条
    fn render_control_bar(
        &self,
        theme: &Theme,
        is_running: bool,
        is_finished: bool,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child("压测监控"),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    // 开始/停止
                    .when(!is_running, |this| {
                        let tid = self.tab_id.clone();
                        this.child(
                            div()
                                .id("stress-start")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(theme.primary)
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.9))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.primary_foreground)
                                        .child("配置并开始"),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |app: &mut NetAssistantApp, _, window, cx| {
                                        app.open_stress_config(tid.clone(), window, cx);
                                    }),
                                ),
                        )
                    })
                    .when(is_running, |this| {
                        let tid = self.tab_id.clone();
                        this.child(
                            div()
                                .id("stress-stop")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(theme.danger)
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.9))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.primary_foreground)
                                        .child("停止"),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                                        app.stop_stress(tid.clone(), cx);
                                    }),
                                ),
                        )
                    })
                    // 导出 CSV
                    .when(is_finished, |this| {
                        let tid = self.tab_id.clone();
                        this.child(
                            div()
                                .id("stress-export")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(theme.accent)
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.9))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.foreground)
                                        .child("导出CSV"),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                                        app.export_stress_report(tid.clone(), cx);
                                    }),
                                ),
                        )
                    }),
            )
    }

    /// 状态指示条
    fn render_status_bar(
        &self,
        theme: &Theme,
        is_running: bool,
        is_finished: bool,
        stats: &crate::stress::StressStats,
    ) -> Div {
        let (status_text, status_color) = if is_running {
            ("运行中", theme.success)
        } else if is_finished {
            ("已完成", theme.primary)
        } else {
            ("未开始", theme.muted_foreground)
        };
        let elapsed = format_elapsed(stats.elapsed_ms);

        div()
            .flex()
            .items_center()
            .gap_4()
            .px_4()
            .py_1p5()
            .bg(theme.muted)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().w_2().h_2().rounded_full().bg(status_color))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.foreground)
                            .child(status_text),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("耗时: {}", elapsed)),
            )
    }

    /// QPS 大字卡片
    fn render_qps_card(&self, theme: &Theme, stats: &crate::stress::StressStats) -> Div {
        div()
            .p_4()
            .rounded_lg()
            .bg(theme.muted)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("当前 QPS"),
            )
            .child(
                div()
                    .text_3xl()
                    .font_bold()
                    .text_color(theme.primary)
                    .child(format!("{:.0}", stats.current_qps)),
            )
    }

    /// 发包统计网格
    fn render_stats_grid(&self, theme: &Theme, stats: &crate::stress::StressStats) -> Div {
        div()
            .flex()
            .gap_3()
            .child(self.render_stat_cell(
                theme,
                "总发送",
                &stats.total_sent.to_string(),
                theme.foreground,
            ))
            .child(self.render_stat_cell(
                theme,
                "成功",
                &stats.total_success.to_string(),
                theme.success,
            ))
            .child(self.render_stat_cell(
                theme,
                "失败",
                &stats.total_failure.to_string(),
                theme.danger,
            ))
            .child(self.render_stat_cell(
                theme,
                "活跃连接",
                &stats.active_connections.to_string(),
                theme.foreground,
            ))
    }

    /// 单个统计单元
    fn render_stat_cell(&self, theme: &Theme, label: &str, value: &str, color: Hsla) -> Div {
        div()
            .flex_1()
            .p_3()
            .rounded_md()
            .bg(theme.muted)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .text_color(color)
                    .child(value.to_string()),
            )
    }

    /// 延迟卡片(p50/p95/p99/avg/max)
    fn render_latency_cards(&self, theme: &Theme, stats: &crate::stress::StressStats) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child("延迟 (ms)"),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(self.render_latency_cell(theme, "p50", stats.latency_p50_us))
                    .child(self.render_latency_cell(theme, "p95", stats.latency_p95_us))
                    .child(self.render_latency_cell(theme, "p99", stats.latency_p99_us))
                    .child(self.render_latency_cell(theme, "avg", stats.latency_avg_us))
                    .child(self.render_latency_cell(theme, "max", stats.latency_max_us)),
            )
    }

    /// 单个延迟单元
    fn render_latency_cell(&self, theme: &Theme, label: &str, us: Option<u64>) -> Div {
        let value = us
            .map(|v| format!("{:.2}", v as f64 / 1000.0))
            .unwrap_or_else(|| "-".to_string());
        div()
            .flex_1()
            .p_2()
            .rounded_md()
            .bg(theme.muted)
            .flex()
            .flex_col()
            .gap_0p5()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(theme.foreground)
                    .child(value),
            )
    }

    /// 连接/字节统计
    fn render_connection_stats(&self, theme: &Theme, stats: &crate::stress::StressStats) -> Div {
        div()
            .flex()
            .gap_3()
            .child(self.render_stat_cell(
                theme,
                "断连",
                &stats.disconnects.to_string(),
                theme.danger,
            ))
            .child(self.render_stat_cell(
                theme,
                "重连",
                &stats.reconnects.to_string(),
                theme.primary,
            ))
            .child(self.render_stat_cell(
                theme,
                "发送字节",
                &format_bytes(stats.bytes_sent),
                theme.foreground,
            ))
            .child(self.render_stat_cell(
                theme,
                "接收字节",
                &format_bytes(stats.bytes_received),
                theme.foreground,
            ))
    }

    /// 失败分类统计(仅在有失败时显示)
    fn render_failure_breakdown(
        &self,
        theme: &Theme,
        stats: &crate::stress::StressStats,
    ) -> Option<Div> {
        let f = &stats.failures;
        if stats.total_failure == 0 {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                // 标题行:说明下方是失败原因分类
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("失败原因分类"),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .flex_wrap()
                        .child(self.render_stat_cell(
                            theme,
                            "连接失败",
                            &f.connect_failed.to_string(),
                            theme.danger,
                        ))
                        .child(self.render_stat_cell(
                            theme,
                            "发送失败",
                            &f.send_failed.to_string(),
                            theme.danger,
                        ))
                        .child(self.render_stat_cell(
                            theme,
                            "接收超时",
                            &f.recv_timeout.to_string(),
                            theme.danger,
                        ))
                        .child(self.render_stat_cell(
                            theme,
                            "对端关闭",
                            &f.peer_closed.to_string(),
                            theme.danger,
                        ))
                        .child(self.render_stat_cell(
                            theme,
                            "校验失败",
                            &f.validate_failed.to_string(),
                            theme.danger,
                        )),
                ),
        )
    }
}

/// 格式化耗时(ms → mm:ss)
fn format_elapsed(ms: u64) -> String {
    let secs = ms / 1000;
    let m = secs / 60;
    let s = secs % 60;
    format!("{:02}:{:02}", m, s)
}

/// 格式化字节数
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
