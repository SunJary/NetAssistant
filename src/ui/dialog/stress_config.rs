// 压测配置弹窗
//
// 仿 NewConnectionDialog 布局: 协议芯片(只读回填)、目标地址端口、
// 模式/连接模式选择、并发/报文/间隔输入、更多设置折叠区。
// 报文输入复用 InputWithMode(文本/hex)。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::Theme;

use crate::app::NetAssistantApp;
use crate::config::connection::ConnectionType;
use crate::stress::config::{
    ConnectionMode, RampUpConfig, StopCondition, StressMode, StressTestConfig,
};
use crate::stress::port_range::{EphemeralPortRange, STATIC_THRESHOLD};
use crate::ui::components::input_with_mode::InputWithMode;
use crate::ui::dialog::port_limit_help::render_port_limit_help_dialog;

/// 文本模式默认报文
const DEFAULT_TEXT_PAYLOAD: &str = "PING ${seq}";
/// hex 模式默认报文 ("PING" 的十六进制)
const DEFAULT_HEX_PAYLOAD: &str = "50494E47${seq}";

/// 停止条件类型(UI 选择用)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopConditionType {
    Duration,
    Count,
    Either,
    Manual,
}

/// 压测配置弹窗状态(持有所有输入实体, 打开弹窗时创建)
pub struct StressConfigDialogState {
    pub tab_id: String,
    // 只读回填
    pub target_address: String,
    pub target_port: u16,
    pub protocol: ConnectionType,
    // 可编辑输入
    pub concurrency_input: Entity<InputState>,
    pub send_interval_input: Entity<InputState>,
    pub qps_limit_input: Entity<InputState>,
    pub timeout_input: Entity<InputState>,
    pub payload_input: Entity<InputState>,
    pub duration_input: Entity<InputState>,
    pub count_input: Entity<InputState>,
    pub ramp_up_secs_input: Entity<InputState>,
    // 选择状态
    pub stress_mode: StressMode,
    pub connection_mode: ConnectionMode,
    pub message_input_mode: String,
    pub stop_condition_type: StopConditionType,
    pub auto_reconnect: bool,
    pub ramp_up_enabled: bool,
    pub show_advanced: bool,
    /// 是否显示端口说明子弹窗
    pub show_port_help: bool,
}

impl StressConfigDialogState {
    /// 从已有压测配置构造弹窗状态(回填)
    pub fn new(
        tab_id: String,
        config: StressTestConfig,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        let stop_condition_type = match &config.stop_condition {
            StopCondition::Duration(_) => StopConditionType::Duration,
            StopCondition::Count(_) => StopConditionType::Count,
            StopCondition::Either { .. } => StopConditionType::Either,
            StopCondition::Manual => StopConditionType::Manual,
        };
        let (duration_val, count_val) = match &config.stop_condition {
            StopCondition::Duration(s) => (s.to_string(), String::new()),
            StopCondition::Count(n) => (String::new(), n.to_string()),
            StopCondition::Either {
                duration_secs,
                count,
            } => (duration_secs.to_string(), count.to_string()),
            StopCondition::Manual => (String::new(), String::new()),
        };

        Self {
            tab_id,
            target_address: config.target_address.clone(),
            target_port: config.target_port,
            protocol: config.protocol,
            concurrency_input: make_input(&config.concurrency.to_string(), window, cx),
            send_interval_input: make_input(&config.send_interval_ms.to_string(), window, cx),
            qps_limit_input: make_input(
                config
                    .global_qps_limit
                    .map(|q| q.to_string())
                    .unwrap_or_default()
                    .as_str(),
                window,
                cx,
            ),
            timeout_input: make_input(&config.timeout_ms.to_string(), window, cx),
            payload_input: {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .code_editor("json")
                        .line_number(false)
                        .folding(false)
                        .multi_line(true)
                        .placeholder("输入报文内容, 支持 ${seq} ${timestamp} 等变量...")
                });
                input.update(cx, |input, cx| {
                    input.set_value(config.payload_template.clone(), window, cx);
                });
                input
            },
            duration_input: make_input(&duration_val, window, cx),
            count_input: make_input(&count_val, window, cx),
            ramp_up_secs_input: make_input(&config.ramp_up.ramp_up_secs.to_string(), window, cx),
            stress_mode: config.mode,
            connection_mode: config.connection_mode,
            message_input_mode: config.message_input_mode.clone(),
            stop_condition_type,
            auto_reconnect: config.auto_reconnect,
            ramp_up_enabled: config.ramp_up.enabled,
            show_advanced: false,
            show_port_help: false,
        }
    }

    /// 从弹窗状态构建压测配置
    pub fn build_config(&self, cx: &mut Context<NetAssistantApp>) -> StressTestConfig {
        let parse_u64 = |input: &Entity<InputState>| -> u64 {
            input.read(cx).value().trim().parse::<u64>().unwrap_or(0)
        };
        let parse_usize = |input: &Entity<InputState>| -> usize {
            input.read(cx).value().trim().parse::<usize>().unwrap_or(1)
        };
        let parse_opt_u32 = |input: &Entity<InputState>| -> Option<u32> {
            input.read(cx).value().trim().parse::<u32>().ok()
        };

        let stop_condition = match self.stop_condition_type {
            StopConditionType::Duration => StopCondition::Duration(parse_u64(&self.duration_input)),
            StopConditionType::Count => StopCondition::Count(parse_u64(&self.count_input)),
            StopConditionType::Either => StopCondition::Either {
                duration_secs: parse_u64(&self.duration_input),
                count: parse_u64(&self.count_input),
            },
            StopConditionType::Manual => StopCondition::Manual,
        };

        StressTestConfig {
            target_address: self.target_address.clone(),
            target_port: self.target_port,
            protocol: self.protocol,
            mode: self.stress_mode,
            connection_mode: self.connection_mode,
            concurrency: parse_usize(&self.concurrency_input).max(1),
            message_input_mode: self.message_input_mode.clone(),
            payload_template: self.payload_input.read(cx).value().to_string(),
            send_interval_ms: parse_u64(&self.send_interval_input),
            global_qps_limit: parse_opt_u32(&self.qps_limit_input),
            stop_condition,
            ramp_up: RampUpConfig {
                enabled: self.ramp_up_enabled,
                ramp_up_secs: parse_u64(&self.ramp_up_secs_input),
            },
            auto_reconnect: self.auto_reconnect,
            response_validation: None,
            timeout_ms: parse_u64(&self.timeout_input).max(100),
        }
    }
}

/// 压测配置弹窗组件
pub struct StressConfigDialog;

impl StressConfigDialog {
    pub fn render(
        app: &NetAssistantApp,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        // 按需检测: 第一层静态阈值判断 + 第二层运行时检测
        // 仅当 concurrency 超过静态阈值且尚未检测过时, 同步触发 detect()
        Self::ensure_port_range_detected(app, cx);

        let state = match &app.stress_config_dialog {
            Some(s) => s,
            None => return div().into_any_element(),
        };

        // 弹窗高度 = 窗口高度 * 0.8 (留出呼吸空间, 不超过 85%)
        // 设最小值 500px, 防止 window.bounds() 在渲染时返回异常值导致弹窗过小
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
            .child(
                div()
                    .w(px(520.0))
                    .h(px(dialog_height))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .bg(theme.muted)
                    .rounded_lg()
                    .shadow_2xl()
                    // Layer 3a: 标题区 (固定, 不参与滚动)
                    .child(
                        div()
                            .px_6()
                            .pt_6()
                            .pb_4()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child("压测配置"),
                    )
                    // Layer 3b: 滚动内容区 (两层结构, 避免 flex 高度分配冲突)
                    // 外层 flex_1() + overflow_hidden() 分配剩余高度
                    // 内层 size_full() + overflow_y_scrollbar() 负责滚动
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
                                    // 目标(只读回填) - 第一个, 不加间距
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
                                                    .child("目标"),
                                            )
                                            .child(
                                                div().text_sm().text_color(theme.muted_foreground).child(
                                                    format!(
                                                        "{}:{} ({})",
                                                        state.target_address,
                                                        state.target_port,
                                                        match state.protocol {
                                                            ConnectionType::Tcp => "TCP",
                                                            ConnectionType::Udp => "UDP",
                                                        }
                                                    ),
                                                ),
                                            ),
                                    )
                                    // 压测模式
                                    .child(Self::render_mode_selector(state, &theme, cx).mt_4())
                                    // 连接模式
                                    .child(Self::render_connection_mode_selector(state, &theme, cx).mt_4())
                                    // 并发数 + 发包间隔
                                    .child(
                                        div()
                                            .flex()
                                            .gap_4()
                                            .mt_4()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_semibold()
                                                            .text_color(theme.foreground)
                                                            .child("并发客户端数"),
                                                    )
                                                    .child(
                                                        Input::new(&state.concurrency_input)
                                                            .cleanable(true),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_semibold()
                                                            .text_color(theme.foreground)
                                                            .child("发包间隔(ms, 0=不限速)"),
                                                    )
                                                    .child(
                                                        Input::new(&state.send_interval_input)
                                                            .cleanable(true),
                                                    ),
                                            ),
                                    )
                                    // 端口上限警告行 (按需渲染)
                                    .child(Self::render_port_warning(app, state, &theme, cx))
                                    // 报文输入(复用 InputWithMode)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .mt_4()
                                            .child(
                                                div()
                                                    .flex()
                                                    .justify_between()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_semibold()
                                                            .text_color(theme.foreground)
                                                            .child("报文内容"),
                                                    )
                                                    .child(Self::render_payload_mode_chip(
                                                        state, &theme, cx,
                                                    )),
                                            )
                                            .child(InputWithMode::render(
                                                &state.payload_input,
                                                &state.message_input_mode,
                                                &theme,
                                                cx,
                                            )),
                                    )
                                    // 更多设置折叠区
                                    .child(Self::render_advanced(state, &theme, cx)),
                            ),
                    )
                    // Layer 3c: 底部按钮区 (固定, 不参与滚动)
                    .child(
                        div()
                            .px_6()
                            .pb_6()
                            .pt_2()
                            .border_t_1()
                            .border_color(theme.border)
                            .child(Self::render_actions(state, &theme, window, cx)),
                    ),
            )
            // 端口说明子弹窗叠加 (show_port_help=true 时覆盖在压测配置弹窗之上)
            .when(state.show_port_help, |this| {
                this.child(render_port_limit_help_dialog(app, state, &theme, window, cx))
            })
            .into_any_element()
    }

    /// render 时的兜底检测 (防 storm)
    ///
    /// 端口范围由 trigger_port_range_detect 主动触发 (启动时 + 打开弹窗时 + 手动按钮)。
    /// 这里只在"从未检测过" (port_range_detected=false) 且并发超过静态阈值时兜底 spawn 一次,
    /// 防止用户在启动瞬间打开弹窗、且 trigger 尚未完成时拿到空值。
    /// port_range_detected=true 后此函数直接返回, 避免 render 每帧疯狂 spawn netsh。
    fn ensure_port_range_detected(app: &NetAssistantApp, cx: &mut Context<NetAssistantApp>) {
        // 已尝试检测 (无论成功失败) → 不重复, 防 render storm
        if app.port_range_detected {
            return;
        }

        let Some(state) = app.stress_config_dialog.as_ref() else {
            return;
        };
        let concurrency: usize = state
            .concurrency_input
            .read(cx)
            .value()
            .trim()
            .parse::<usize>()
            .unwrap_or(0);

        // 未超过静态阈值: warning 不会显示, 无需触发检测
        if (concurrency as u32) <= STATIC_THRESHOLD {
            return;
        }

        // 超过静态阈值且尚未检测: 兜底 spawn (trigger_port_range_detect 会置 detected=true)
        let weak_app = cx.entity().downgrade();
        cx.spawn(async move |_, async_app: &mut gpui::AsyncApp| {
            let detected = smol::unblock(|| EphemeralPortRange::detect()).await;
            if let Some(app) = weak_app.upgrade() {
                let _ = app.update(async_app, |app: &mut NetAssistantApp, cx| {
                    app.detected_port_range = detected;
                    app.port_range_detected = true;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// 渲染端口上限警告行 (条件渲染)
    ///
    /// 仅当并发超过系统默认端口数 (STATIC_THRESHOLD) 时介入:
    ///   1. 懒触发检测未完成: 暂不显示, 等异步完成 (避免红框闪烁)
    ///   2. 手动"重新检测"进行中: 显示"正在读取…"
    ///   3. 检测失败: 显示"未能读取…", 附「重新检测」+「端口说明」, 不展示编造的默认值
    ///   4. 检测成功且并发 > 建议上限: 显示真实端口数与建议上限
    fn render_port_warning(
        app: &NetAssistantApp,
        state: &StressConfigDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        let concurrency: usize = state
            .concurrency_input
            .read(cx)
            .value()
            .trim()
            .parse::<usize>()
            .unwrap_or(0);
        let interval: u64 = state
            .send_interval_input
            .read(cx)
            .value()
            .trim()
            .parse::<u64>()
            .unwrap_or(0);

        // 并发未超过系统默认端口数: 端口上限不是瓶颈, 不显示
        if (concurrency as u32) <= STATIC_THRESHOLD {
            return div();
        }

        // 手动"重新检测"进行中: 给予反馈 (懒触发的 in-flight 不显示, 避免红框闪烁)
        if app.port_range_detected && app.port_range_detecting {
            return Self::render_warning_box(
                theme,
                "ⓘ 正在读取系统临时端口配置…",
                false,
                cx,
            );
        }
        // 懒触发尚未完成 (detected=false): 暂不显示
        if !app.port_range_detected {
            return div();
        }

        // 检测已完成 (detected && !detecting)
        let port_range = match &app.detected_port_range {
            // 检测失败: 不展示编造的默认值, 引导用户手动获取
            None => {
                return Self::render_warning_box(
                    theme,
                    "⚠ 未能读取系统临时端口配置，建议点击「重新检测」或查看「端口说明」手动获取",
                    true,
                    cx,
                );
            }
            Some(r) => r,
        };

        // 检测成功: 仅当并发超过建议上限时警告
        let suggested_max =
            port_range.suggested_max_concurrency(state.connection_mode, interval, state.protocol);
        if concurrency <= suggested_max {
            return div();
        }

        let warning_text = format!(
            "⚠ 检测到临时端口 {} 个 ({}-{}), 并发超过 {} 可能连接失败",
            port_range.count, port_range.start, port_range.end(), suggested_max,
        );
        Self::render_warning_box(theme, &warning_text, false, cx)
    }

    /// 警告行容器: 文本 + 右下角操作链接 (始终含「端口说明」, 可选「重新检测」)
    fn render_warning_box(
        theme: &Theme,
        text: &str,
        retry: bool,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        let mut actions = div()
            .flex()
            .justify_end()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.primary)
                    .cursor_pointer()
                    .hover(|d| d.underline())
                    .child("端口说明")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                            if let Some(s) = &mut app.stress_config_dialog {
                                s.show_port_help = true;
                            }
                            cx.notify();
                        }),
                    ),
            );
        if retry {
            actions = actions.child(
                div()
                    .text_xs()
                    .text_color(theme.primary)
                    .cursor_pointer()
                    .hover(|d| d.underline())
                    .child("重新检测")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                            app.trigger_port_range_detect(cx);
                        }),
                    ),
            );
        }
        div()
            .mt_2()
            .p_2()
            .rounded_md()
            // 背景用 danger 半透明, 边框用 danger 实色 (主题感知, 亮/暗主题都可见)
            .bg(theme.danger.opacity(0.12))
            .border_1()
            .border_color(theme.danger)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .whitespace_normal()
                    // 文字用主题前景色 (亮色=黑, 暗色=白), 保证在淡红背景上有足够对比度
                    .text_color(theme.foreground)
                    .child(text.to_string()),
            )
            .child(actions)
    }

    /// 渲染压测模式选择(吞吐/往返)
    fn render_mode_selector(
        state: &StressConfigDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child("压测模式"),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(render_stress_mode_chip(
                        state,
                        StressMode::Throughput,
                        "吞吐(只发不等)",
                        theme,
                        cx,
                    ))
                    .child(render_stress_mode_chip(
                        state,
                        StressMode::PingPong,
                        "往返(测RTT)",
                        theme,
                        cx,
                    )),
            )
    }

    /// 渲染连接模式选择(长/短)
    fn render_connection_mode_selector(
        state: &StressConfigDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child("连接模式"),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(render_conn_mode_chip(
                        state,
                        ConnectionMode::Long,
                        "长连接",
                        theme,
                        cx,
                    ))
                    .child(render_conn_mode_chip(
                        state,
                        ConnectionMode::Short,
                        "短连接",
                        theme,
                        cx,
                    )),
            )
    }

    /// 渲染报文模式芯片(文本/十六进制)
    fn render_payload_mode_chip(
        state: &StressConfigDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        let is_text = state.message_input_mode == "text";
        div()
            .flex()
            .gap_1()
            .child(
                div()
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_text, |d| {
                        d.bg(theme.primary).text_color(theme.primary_foreground)
                    })
                    .when(!is_text, |d| {
                        d.bg(theme.border).text_color(theme.foreground)
                    })
                    .child("文本")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp, _, window, cx| {
                            if let Some(s) = &mut app.stress_config_dialog {
                                // 切回文本时，若当前是 hex 默认报文则换回文本默认
                                let current = s.payload_input.read(cx).value().to_string();
                                if current == DEFAULT_HEX_PAYLOAD {
                                    s.payload_input.update(cx, |input, cx| {
                                        input.set_value(
                                            DEFAULT_TEXT_PAYLOAD.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                                s.message_input_mode = "text".to_string();
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .rounded_md()
                    .cursor_pointer()
                    .when(!is_text, |d| {
                        d.bg(theme.primary).text_color(theme.primary_foreground)
                    })
                    .when(is_text, |d| d.bg(theme.border).text_color(theme.foreground))
                    .child("Hex")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp, _, window, cx| {
                            if let Some(s) = &mut app.stress_config_dialog {
                                // 切到 hex 时，若当前是文本默认报文则换为 hex 默认
                                let current = s.payload_input.read(cx).value().to_string();
                                if current == DEFAULT_TEXT_PAYLOAD {
                                    s.payload_input.update(cx, |input, cx| {
                                        input.set_value(
                                            DEFAULT_HEX_PAYLOAD.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                                s.message_input_mode = "hex".to_string();
                            }
                            cx.notify();
                        }),
                    ),
            )
    }

    /// 渲染更多设置折叠区
    fn render_advanced(
        state: &StressConfigDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mt_4()
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .child(if state.show_advanced {
                        "▼ 更多设置"
                    } else {
                        "▶ 更多设置"
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                            if let Some(s) = &mut app.stress_config_dialog {
                                s.show_advanced = !s.show_advanced;
                            }
                            cx.notify();
                        }),
                    ),
            )
            .when(state.show_advanced, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .pl_2()
                        // 全局 QPS 限制 + 超时
                        .child(
                            div()
                                .flex()
                                .gap_4()
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(theme.foreground)
                                                .child("全局QPS限制(留空=不限)"),
                                        )
                                        .child(Input::new(&state.qps_limit_input).cleanable(true)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(theme.foreground)
                                                .child("超时(ms)"),
                                        )
                                        .child(Input::new(&state.timeout_input).cleanable(true)),
                                ),
                        )
                        // 停止条件
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
                                        .child("停止条件"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(render_stop_chip(
                                            state,
                                            StopConditionType::Duration,
                                            "限时",
                                            theme,
                                            cx,
                                        ))
                                        .child(render_stop_chip(
                                            state,
                                            StopConditionType::Count,
                                            "定量",
                                            theme,
                                            cx,
                                        ))
                                        .child(render_stop_chip(
                                            state,
                                            StopConditionType::Either,
                                            "先到先停",
                                            theme,
                                            cx,
                                        ))
                                        .child(render_stop_chip(
                                            state,
                                            StopConditionType::Manual,
                                            "手动",
                                            theme,
                                            cx,
                                        )),
                                )
                                .when(
                                    matches!(
                                        state.stop_condition_type,
                                        StopConditionType::Duration | StopConditionType::Either
                                    ),
                                    |this| {
                                        this.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(theme.muted_foreground)
                                                        .child("限时(秒)"),
                                                )
                                                .child(
                                                    Input::new(&state.duration_input)
                                                        .cleanable(true),
                                                ),
                                        )
                                    },
                                )
                                .when(
                                    matches!(
                                        state.stop_condition_type,
                                        StopConditionType::Count | StopConditionType::Either
                                    ),
                                    |this| {
                                        this.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(theme.muted_foreground)
                                                        .child("定量(总发包数)"),
                                                )
                                                .child(
                                                    Input::new(&state.count_input).cleanable(true),
                                                ),
                                        )
                                    },
                                ),
                        )
                        // 阶梯 ramp-up
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .items_center()
                                        .cursor_pointer()
                                        .child(render_checkbox(state.ramp_up_enabled, theme))
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(theme.foreground)
                                                .child("阶梯 ramp-up"),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                                                if let Some(s) = &mut app.stress_config_dialog {
                                                    s.ramp_up_enabled = !s.ramp_up_enabled;
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                )
                                .when(state.ramp_up_enabled, |this| {
                                    this.child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child("爬升时长(秒)"),
                                            )
                                            .child(
                                                Input::new(&state.ramp_up_secs_input)
                                                    .cleanable(true),
                                            ),
                                    )
                                }),
                        )
                        // 自动重连
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .items_center()
                                .cursor_pointer()
                                .child(render_checkbox(state.auto_reconnect, theme))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.foreground)
                                        .child("断线自动重连"),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                                        if let Some(s) = &mut app.stress_config_dialog {
                                            s.auto_reconnect = !s.auto_reconnect;
                                        }
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
            })
    }

    /// 渲染底部操作按钮
    fn render_actions(
        state: &StressConfigDialogState,
        theme: &Theme,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        let tab_id = state.tab_id.clone();
        div()
            .flex()
            .gap_2()
            // 取消
            .child(
                div()
                    .flex_1()
                    .p_2()
                    .bg(theme.border)
                    .rounded_md()
                    .cursor_pointer()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.foreground)
                            .text_center()
                            .child("取消"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                            app.stress_config_dialog = None;
                            cx.notify();
                        }),
                    ),
            )
            // 开始压测
            .child(
                div()
                    .flex_1()
                    .p_2()
                    .bg(theme.primary)
                    .rounded_md()
                    .cursor_pointer()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.primary_foreground)
                            .text_center()
                            .child("开始压测"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                            if let Some(dialog) = app.stress_config_dialog.take() {
                                let config = dialog.build_config(cx);
                                app.start_stress(tab_id.clone(), config, cx);
                                app.stress_config_dialog = None;
                            }
                        }),
                    ),
            )
    }
}

/// 创建带初始值的输入框实体
fn make_input(
    val: &str,
    window: &mut Window,
    cx: &mut Context<NetAssistantApp>,
) -> Entity<InputState> {
    let input = cx.new(|cx| InputState::new(window, cx));
    input.update(cx, |input, cx| {
        input.set_value(val.to_string(), window, cx);
    });
    input
}

/// 渲染复选框样式
fn render_checkbox(checked: bool, theme: &Theme) -> Div {
    div()
        .w_4()
        .h_4()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(theme.border)
        .when(checked, |d| d.bg(theme.primary))
        .when(checked, |d| {
            d.child(
                div()
                    .text_xs()
                    .text_color(theme.primary_foreground)
                    .child("✓"),
            )
        })
}

/// 渲染压测模式芯片
fn render_stress_mode_chip(
    state: &StressConfigDialogState,
    mode: StressMode,
    label: &str,
    theme: &Theme,
    cx: &mut Context<NetAssistantApp>,
) -> Div {
    let selected = state.stress_mode == mode;
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
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                if let Some(s) = &mut app.stress_config_dialog {
                    s.stress_mode = mode;
                }
                cx.notify();
            }),
        )
}

/// 渲染连接模式芯片
fn render_conn_mode_chip(
    state: &StressConfigDialogState,
    mode: ConnectionMode,
    label: &str,
    theme: &Theme,
    cx: &mut Context<NetAssistantApp>,
) -> Div {
    let selected = state.connection_mode == mode;
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
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                if let Some(s) = &mut app.stress_config_dialog {
                    s.connection_mode = mode;
                }
                cx.notify();
            }),
        )
}

/// 渲染停止条件芯片
fn render_stop_chip(
    state: &StressConfigDialogState,
    stop_type: StopConditionType,
    label: &str,
    theme: &Theme,
    cx: &mut Context<NetAssistantApp>,
) -> Div {
    let selected = state.stop_condition_type == stop_type;
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
        .child(div().text_sm().font_medium().child(label.to_string()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                if let Some(s) = &mut app.stress_config_dialog {
                    s.stop_condition_type = stop_type;
                }
                cx.notify();
            }),
        )
}
