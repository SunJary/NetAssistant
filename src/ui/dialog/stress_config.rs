// 压测配置弹窗
//
// 仿 NewConnectionDialog 布局: 协议芯片(只读回填)、目标地址端口、
// 模式/连接模式选择、并发/报文/间隔输入、更多设置折叠区。
// 报文输入复用 InputWithMode(文本/hex)。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::input::{Input, InputState};
use gpui_component::Theme;

use crate::app::NetAssistantApp;
use crate::config::connection::ConnectionType;
use crate::stress::config::{
    ConnectionMode, RampUpConfig, StopCondition, StressMode, StressTestConfig,
};
use crate::ui::components::input_with_mode::InputWithMode;

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
            StopCondition::Either { duration_secs, count } => {
                (duration_secs.to_string(), count.to_string())
            }
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
                config.global_qps_limit.map(|q| q.to_string()).unwrap_or_default().as_str(),
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
        let state = match &app.stress_config_dialog {
            Some(s) => s,
            None => return div().into_any_element(),
        };

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x80000000))
            .child(
                div()
                    .w(px(520.0))
                    .bg(theme.muted)
                    .rounded_lg()
                    .shadow_2xl()
                    .p_6()
                    // 标题
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .mb_4()
                            .text_color(theme.foreground)
                            .child("压测配置"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            // 目标(只读回填)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div().text_sm().font_semibold().text_color(theme.foreground)
                                            .child("目标"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.muted_foreground)
                                            .child(format!(
                                                "{}:{} ({})",
                                                state.target_address,
                                                state.target_port,
                                                match state.protocol {
                                                    ConnectionType::Tcp => "TCP",
                                                    ConnectionType::Udp => "UDP",
                                                }
                                            )),
                                    ),
                            )
                            // 压测模式
                            .child(Self::render_mode_selector(state, &theme, cx))
                            // 连接模式
                            .child(Self::render_connection_mode_selector(state, &theme, cx))
                            // 并发数 + 发包间隔
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
                                                div().text_sm().font_semibold()
                                                    .text_color(theme.foreground)
                                                    .child("并发客户端数"),
                                            )
                                            .child(Input::new(&state.concurrency_input).cleanable(true)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div().text_sm().font_semibold()
                                                    .text_color(theme.foreground)
                                                    .child("发包间隔(ms, 0=不限速)"),
                                            )
                                            .child(Input::new(&state.send_interval_input).cleanable(true)),
                                    ),
                            )
                            // 报文输入(复用 InputWithMode)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .justify_between()
                                            .child(
                                                div().text_sm().font_semibold()
                                                    .text_color(theme.foreground)
                                                    .child("报文内容"),
                                            )
                                            .child(Self::render_payload_mode_chip(state, &theme, cx)),
                                    )
                                    .child(InputWithMode::render(
                                        &state.payload_input,
                                        &state.message_input_mode,
                                        &theme,
                                        cx,
                                    )),
                            ),
                    )
                    // 更多设置折叠区
                    .child(Self::render_advanced(state, &theme, cx))
                    // 取消 / 开始
                    .child(Self::render_actions(state, &theme, window, cx)),
            )
            .into_any_element()
    }

    /// 渲染压测模式选择(吞吐/往返)
    fn render_mode_selector(
        state: &StressConfigDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div().flex().flex_col().gap_1().child(
            div().text_sm().font_semibold().text_color(theme.foreground).child("压测模式"),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(render_stress_mode_chip(state, StressMode::Throughput, "吞吐(只发不等)", theme, cx))
                .child(render_stress_mode_chip(state, StressMode::PingPong, "往返(测RTT)", theme, cx)),
        )
    }

    /// 渲染连接模式选择(长/短)
    fn render_connection_mode_selector(
        state: &StressConfigDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div().flex().flex_col().gap_1().child(
            div().text_sm().font_semibold().text_color(theme.foreground).child("连接模式"),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(render_conn_mode_chip(state, ConnectionMode::Long, "长连接", theme, cx))
                .child(render_conn_mode_chip(state, ConnectionMode::Short, "短连接", theme, cx)),
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
                    .when(is_text, |d| d.bg(theme.primary).text_color(theme.primary_foreground))
                    .when(!is_text, |d| d.bg(theme.border).text_color(theme.foreground))
                    .child("文本")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp, _, window, cx| {
                            if let Some(s) = &mut app.stress_config_dialog {
                                // 切回文本时，若当前是 hex 默认报文则换回文本默认
                                let current = s.payload_input.read(cx).value().to_string();
                                if current == DEFAULT_HEX_PAYLOAD {
                                    s.payload_input.update(cx, |input, cx| {
                                        input.set_value(DEFAULT_TEXT_PAYLOAD.to_string(), window, cx);
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
                    .when(!is_text, |d| d.bg(theme.primary).text_color(theme.primary_foreground))
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
                                        input.set_value(DEFAULT_HEX_PAYLOAD.to_string(), window, cx);
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
            .mt_2()
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
                                            div().text_sm().font_semibold().text_color(theme.foreground)
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
                                            div().text_sm().font_semibold().text_color(theme.foreground)
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
                                    div().text_sm().font_semibold().text_color(theme.foreground)
                                        .child("停止条件"),
                                )
                                .child(
                                    div().flex().gap_2()
                                        .child(render_stop_chip(state, StopConditionType::Duration, "限时", theme, cx))
                                        .child(render_stop_chip(state, StopConditionType::Count, "定量", theme, cx))
                                        .child(render_stop_chip(state, StopConditionType::Either, "先到先停", theme, cx))
                                        .child(render_stop_chip(state, StopConditionType::Manual, "手动", theme, cx)),
                                )
                                .when(
                                    matches!(
                                        state.stop_condition_type,
                                        StopConditionType::Duration | StopConditionType::Either
                                    ),
                                    |this| {
                                        this.child(
                                            div().flex().flex_col().gap_1()
                                                .child(div().text_xs().text_color(theme.muted_foreground).child("限时(秒)"))
                                                .child(Input::new(&state.duration_input).cleanable(true)),
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
                                            div().flex().flex_col().gap_1()
                                                .child(div().text_xs().text_color(theme.muted_foreground).child("定量(总发包数)"))
                                                .child(Input::new(&state.count_input).cleanable(true)),
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
                                        .child(div().text_sm().text_color(theme.foreground).child("阶梯 ramp-up"))
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
                                        div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(theme.muted_foreground).child("爬升时长(秒)"))
                                            .child(Input::new(&state.ramp_up_secs_input).cleanable(true)),
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
                                .child(div().text_sm().text_color(theme.foreground).child("断线自动重连"))
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
            .mt_6()
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
        .rounded_sm()
        .border_1()
        .border_color(theme.border)
        .when(checked, |d| d.bg(theme.primary))
        .when(checked, |d| {
            d.child(div().text_color(theme.primary_foreground).text_center().child("✓"))
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
        .when(selected, |d| d.bg(theme.primary).text_color(theme.primary_foreground))
        .when(!selected, |d| d.bg(theme.border).text_color(theme.foreground))
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
        .when(selected, |d| d.bg(theme.primary).text_color(theme.primary_foreground))
        .when(!selected, |d| d.bg(theme.border).text_color(theme.foreground))
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
        .when(selected, |d| d.bg(theme.primary).text_color(theme.primary_foreground))
        .when(!selected, |d| d.bg(theme.border).text_color(theme.foreground))
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
