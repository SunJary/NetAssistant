// 压测配置弹窗
//
// 基于 gpui_component::Dialog 实现:
// 通过 window.open_dialog 命令式打开(Root 管理对话框栈), 内容区用动态 content 模式
// 每帧从 NetAssistantApp 读取最新状态(chip 选择、折叠区、端口警告、插入变量浮层随状态刷新),
// 滚动结构为「外层 max_h 钳制可视区 + 内层 overflow_y_scrollbar」
// (见 dialog_content_max_height 文档, max_h 不能直接放滚动容器上, 否则滚轮不响应)。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::DialogFooter;
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme as _;
use gpui_component::ElementExt as _;
use gpui_component::StyledExt;
use gpui_component::Theme;
use gpui_component::WindowExt as _;

use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::ConnectionType;
use crate::stress::config::{
    ConnectionMode, RampUpConfig, StopCondition, StressMode, StressTestConfig,
};
use crate::stress::port_range::{EphemeralPortRange, STATIC_THRESHOLD};
use crate::ui::components::input_with_mode::InputWithMode;
use crate::ui::dialog::open_port_limit_help_dialog;
use crate::ui::dialog::variable_picker::render_variable_picker;

use super::{dialog_content_max_height, dialog_height};

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
    /// 是否显示「插入变量」浮层
    pub show_variable_picker: bool,
    /// 「插入变量」按钮在窗口坐标系中的 bounds (由 on_prepaint 更新, 用于浮层定位)
    pub var_button_bounds: Bounds<Pixels>,
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
                        .placeholder(t!("stress_config.payload_placeholder").to_string())
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
            show_variable_picker: false,
            var_button_bounds: Bounds::default(),
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

/// 打开压测配置对话框(命令式, 由 Root 管理层叠)
///
/// 弹窗状态保存在 `app.stress_config_dialog`, 由 `NetAssistantApp::open_stress_config`
/// 先创建好; 内容闭包每帧从 app 读取最新状态。
///
/// 注意 keyboard 保持关闭: Enter 会传播为"开始压测"(网络动作), 避免输入时误触;
/// 关闭方式: 取消按钮 / X / 点击蒙层。
pub fn open_stress_config_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        dialog
            .title(t!("stress_config.title").to_string())
            .w(px(520.0))
            .max_h(dialog_height(window))
            .keyboard(false)
            // X 按钮 / 蒙层关闭时同步清理弹窗状态
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.stress_config_dialog = None;
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
                    // 端口范围兜底检测 (仅首次超过静态阈值时 spawn, 见 ensure_port_range_detected)
                    let _ = entity.update(cx, |app, cx| {
                        StressConfigDialog::ensure_port_range_detected(app, cx)
                    });
                    let theme = cx.theme().clone();
                    let content = content.child(
                        div().max_h(dialog_content_max_height(window)).child(
                            div()
                                .overflow_y_scrollbar()
                                .px_6()
                                .pb_4()
                                .child(render_form(&entity, &theme, cx)),
                        ),
                    );
                    // 「插入变量」浮层: deferred 独立合成层 + anchored 按按钮 bounds 定位
                    let (show_picker, bounds) = {
                        let app_state = entity.read(cx);
                        match app_state.stress_config_dialog.as_ref() {
                            Some(s) => (s.show_variable_picker, s.var_button_bounds),
                            None => (false, Bounds::default()),
                        }
                    };
                    if show_picker {
                        content.child(render_variable_picker(&entity, bounds, &theme, cx))
                    } else {
                        content
                    }
                }
            })
    });
}

/// 压测配置弹窗组件(命名空间: 兜底端口检测)
pub struct StressConfigDialog;

impl StressConfigDialog {
    /// render 时的兜底检测 (防 storm)
    ///
    /// 端口范围由 trigger_port_range_detect 主动触发 (启动时 + 打开弹窗时 + 手动按钮)。
    /// 这里只在"从未检测过" (port_range_detected=false) 且并发超过静态阈值时兜底 spawn 一次,
    /// 防止用户在启动瞬间打开弹窗、且 trigger 尚未完成时拿到空值。
    /// port_range_detected=true 后此函数直接返回, 避免 render 每帧疯狂 spawn netsh。
    pub fn ensure_port_range_detected(app: &NetAssistantApp, cx: &mut Context<NetAssistantApp>) {
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
}

/// 渲染表单主体
fn render_form(app: &Entity<NetAssistantApp>, theme: &Theme, cx: &App) -> Div {
    let state = app.read(cx);
    let Some(state) = state.stress_config_dialog.as_ref() else {
        return div();
    };

    div()
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
                        .child(t!("stress_config.target_label").to_string()),
                )
                .child(
                    div().text_sm().text_color(theme.muted_foreground).child(format!(
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
        .child(render_mode_selector(app, state, theme, cx).mt_4())
        // 连接模式
        .child(render_connection_mode_selector(app, state, theme, cx).mt_4())
        // 并发数 + 发包间隔
        .child(
            div()
                .flex()
                .gap_4()
                .mt_4()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(theme.foreground)
                                .child(t!("stress_config.concurrency_label").to_string()),
                        )
                        .child(Input::new(&state.concurrency_input).cleanable(true)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(theme.foreground)
                                .child(t!("stress_config.send_interval_label").to_string()),
                        )
                        .child(Input::new(&state.send_interval_input).cleanable(true)),
                ),
        )
        // 端口上限警告行 (按需渲染)
        .child(render_port_warning(app, state, theme, cx))
        // 报文输入(复用 InputWithMode)
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .gap_1()
                .mt_4()
                // 标题行: 标题 + 「插入变量」按钮 | 文本/Hex 切换芯片
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
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
                                        .child(t!("stress_config.payload_content_label").to_string()),
                                )
                                .child(render_insert_var_button(app, state, theme, cx)),
                        )
                        .child(render_payload_mode_chip(app, state, theme, cx)),
                )
                .child(InputWithMode::render(
                    &state.payload_input,
                    &state.message_input_mode,
                    theme,
                    cx,
                )),
        )
        // 更多设置折叠区
        .child(render_advanced(app, state, theme, cx))
}

/// 渲染端口上限警告行 (条件渲染)
///
/// 仅当并发超过系统默认端口数 (STATIC_THRESHOLD) 时介入:
///   1. 懒触发检测未完成: 暂不显示, 等异步完成 (避免红框闪烁)
///   2. 手动"重新检测"进行中: 显示"正在读取…"
///   3. 检测失败: 显示"未能读取…", 附「重新检测」+「端口说明」, 不展示编造的默认值
///   4. 检测成功且并发 > 建议上限: 显示真实端口数与建议上限
fn render_port_warning(
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    theme: &Theme,
    cx: &App,
) -> Div {
    let app_state = app.read(cx);
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
    if app_state.port_range_detected && app_state.port_range_detecting {
        return render_warning_box(app, theme, &t!("stress_config.reading_port_config"), false, cx);
    }
    // 懒触发尚未完成 (detected=false): 暂不显示
    if !app_state.port_range_detected {
        return div();
    }

    // 检测已完成 (detected && !detecting)
    let port_range = match &app_state.detected_port_range {
        // 检测失败: 不展示编造的默认值, 引导用户手动获取
        None => {
            return render_warning_box(
                app,
                theme,
                &t!("stress_config.port_config_unreadable"),
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

    let warning_text = t!(
        "stress_config.port_warning_detected",
        count = port_range.count,
        start = port_range.start,
        end = port_range.end(),
        max = suggested_max,
    )
    .to_string();
    render_warning_box(app, theme, &warning_text, false, cx)
}

/// 警告行容器: 文本 + 右下角操作链接 (始终含「端口说明」, 可选「重新检测」)
fn render_warning_box(
    app: &Entity<NetAssistantApp>,
    theme: &Theme,
    text: &str,
    retry: bool,
    _cx: &App,
) -> Div {
    let entity = app.clone();
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
                .child(t!("stress_config.port_help_link").to_string())
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    // 打开端口说明对话框(层叠在压测配置之上)
                    open_port_limit_help_dialog(entity.downgrade(), window, cx);
                }),
        );
    if retry {
        let entity = app.clone();
        actions = actions.child(
            div()
                .text_xs()
                .text_color(theme.primary)
                .cursor_pointer()
                .hover(|d| d.underline())
                .child(t!("stress_config.re_detect_link").to_string())
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    entity.update(cx, |app, cx| {
                        app.trigger_port_range_detect(cx);
                    });
                }),
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
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    theme: &Theme,
    _cx: &App,
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
                .child(t!("stress_config.stress_mode_label").to_string()),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(render_stress_mode_chip(
                    app,
                    state,
                    StressMode::Throughput,
                    &t!("stress_config.mode_throughput"),
                    theme,
                ))
                .child(render_stress_mode_chip(
                    app,
                    state,
                    StressMode::PingPong,
                    &t!("stress_config.mode_pingpong"),
                    theme,
                )),
        )
}

/// 渲染连接模式选择(长/短)
fn render_connection_mode_selector(
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    theme: &Theme,
    _cx: &App,
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
                .child(t!("stress_config.connection_mode_label").to_string()),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(render_conn_mode_chip(
                    app,
                    state,
                    ConnectionMode::Long,
                    &t!("stress_config.mode_long_connection"),
                    theme,
                ))
                .child(render_conn_mode_chip(
                    app,
                    state,
                    ConnectionMode::Short,
                    &t!("stress_config.mode_short_connection"),
                    theme,
                )),
        )
}

/// 渲染「插入变量」按钮 (切换显隐浮层, on_prepaint 追踪位置供浮层定位)
fn render_insert_var_button(
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    theme: &Theme,
    _cx: &App,
) -> impl IntoElement {
    // 记录按钮在窗口坐标系的 bounds, 供 deferred+anchored 浮层定位
    let prepaint_entity = app.clone();
    let prepaint_handler: Box<dyn Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static> =
        Box::new(move |bounds, _, cx| {
            prepaint_entity.update(cx, |app, _| {
                if let Some(s) = &mut app.stress_config_dialog {
                    s.var_button_bounds = bounds;
                }
            });
        });

    // 激活态(浮层展开)或悬停/按下时均用实底主色+白字, 保证文字与背景高对比
    let filled = state.show_variable_picker;
    let entity = app.clone();
    div()
        .id("insert-var-button")
        .flex()
        .items_center()
        .gap_0p5()
        .px_1p5()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .font_medium()
        .cursor_pointer()
        .when(filled, |this| {
            this.text_color(gpui::white()).bg(theme.primary)
        })
        .when(!filled, |this| {
            this.text_color(theme.primary).bg(theme.primary.opacity(0.06))
        })
        .hover(|this| {
            this.text_color(gpui::white()).bg(theme.primary)
        })
        .active(|this| {
            this.text_color(gpui::white()).bg(theme.primary)
        })
        .child(t!("stress_config.insert_variable").to_string())
        .on_mouse_down(
            MouseButton::Left,
            move |_, _, cx| {
                entity.update(cx, |app, cx| {
                    if let Some(s) = &mut app.stress_config_dialog {
                        s.show_variable_picker = !s.show_variable_picker;
                    }
                    cx.notify();
                });
            },
        )
        .on_prepaint(prepaint_handler)
}

/// 渲染报文模式芯片(文本/十六进制)
fn render_payload_mode_chip(
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    theme: &Theme,
    _cx: &App,
) -> Div {
    let is_text = state.message_input_mode == "text";
    let entity = app.clone();
    let entity_hex = app.clone();
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
                .child(t!("stress_config.mode_text").to_string())
                .on_mouse_down(
                    MouseButton::Left,
                    move |_, window, cx| {
                        entity.update(cx, |app, cx| {
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
                                s.show_variable_picker = false;
                            }
                            cx.notify();
                        });
                    },
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
                    move |_, window, cx| {
                        entity_hex.update(cx, |app, cx| {
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
                                s.show_variable_picker = false;
                            }
                            cx.notify();
                        });
                    },
                ),
        )
}

/// 渲染更多设置折叠区
fn render_advanced(
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    theme: &Theme,
    cx: &App,
) -> Div {
    let entity = app.clone();
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
                    t!("stress_config.more_settings_collapse").to_string()
                } else {
                    t!("stress_config.more_settings_expand").to_string()
                })
                .on_mouse_down(
                    MouseButton::Left,
                    {
                        let entity = entity.clone();
                        move |_, _, cx| {
                            entity.update(cx, |app, cx| {
                                if let Some(s) = &mut app.stress_config_dialog {
                                    s.show_advanced = !s.show_advanced;
                                }
                                cx.notify();
                            });
                        }
                    },
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
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(theme.foreground)
                                            .child(t!("stress_config.qps_limit_label").to_string()),
                                    )
                                    .child(Input::new(&state.qps_limit_input).cleanable(true)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(theme.foreground)
                                            .child(t!("stress_config.timeout_label").to_string()),
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
                                    .child(t!("stress_config.stop_condition_label").to_string()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(render_stop_chip(
                                        app,
                                        state,
                                        StopConditionType::Duration,
                                        &t!("stress_config.stop_duration"),
                                        theme,
                                        cx,
                                    ))
                                    .child(render_stop_chip(
                                        app,
                                        state,
                                        StopConditionType::Count,
                                        &t!("stress_config.stop_count"),
                                        theme,
                                        cx,
                                    ))
                                    .child(render_stop_chip(
                                        app,
                                        state,
                                        StopConditionType::Either,
                                        &t!("stress_config.stop_either"),
                                        theme,
                                        cx,
                                    ))
                                    .child(render_stop_chip(
                                        app,
                                        state,
                                        StopConditionType::Manual,
                                        &t!("stress_config.stop_manual"),
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
                                                    .child(t!("stress_config.stop_duration_secs").to_string()),
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
                                                    .child(t!("stress_config.stop_count_total").to_string()),
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
                                            .child(t!("stress_config.ramp_up_label").to_string()),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        {
                                            let entity = entity.clone();
                                            move |_, _, cx| {
                                                entity.update(cx, |app, cx| {
                                                    if let Some(s) = &mut app.stress_config_dialog {
                                                        s.ramp_up_enabled = !s.ramp_up_enabled;
                                                    }
                                                    cx.notify();
                                                });
                                            }
                                        },
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
                                                .child(t!("stress_config.ramp_up_duration").to_string()),
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
                                    .child(t!("stress_config.auto_reconnect_label").to_string()),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                {
                                    let entity = entity.clone();
                                    move |_, _, cx| {
                                        entity.update(cx, |app, cx| {
                                            if let Some(s) = &mut app.stress_config_dialog {
                                                s.auto_reconnect = !s.auto_reconnect;
                                            }
                                            cx.notify();
                                        });
                                    }
                                },
                            ),
                    ),
            )
        })
}

/// 应用压测配置并开始压测, 返回是否成功(成功后由调用方关闭对话框)
fn confirm_start_stress(app: &WeakEntity<NetAssistantApp>, cx: &mut App) -> bool {
    app.update(cx, |app, cx| {
        if let Some(dialog) = app.stress_config_dialog.take() {
            let tab_id = dialog.tab_id.clone();
            let config = dialog.build_config(cx);
            app.start_stress(tab_id, config, cx);
            app.stress_config_dialog = None;
        }
        cx.notify();
        true
    })
    .unwrap_or(true)
}

/// 渲染底部操作按钮
fn render_footer(app: &WeakEntity<NetAssistantApp>, _cx: &App) -> DialogFooter {
    let app_ok = app.clone();
    let app_cancel = app.clone();
    DialogFooter::new()
        // 取消
        .child(
            Button::new("stress-dialog-cancel")
                .outline()
                .label(t!("stress_config.cancel").to_string())
                .on_click(move |_, window, cx| {
                    let _ = app_cancel.update(cx, |app, cx| {
                        app.stress_config_dialog = None;
                        cx.notify();
                    });
                    window.close_dialog(cx);
                }),
        )
        // 开始压测
        .child(
            Button::new("stress-dialog-ok")
                .primary()
                .label(t!("stress_config.start_stress").to_string())
                .on_click(move |_, window, cx| {
                    confirm_start_stress(&app_ok, cx);
                    window.close_dialog(cx);
                }),
        )
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
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    mode: StressMode,
    label: &str,
    theme: &Theme,
) -> Div {
    let selected = state.stress_mode == mode;
    let entity = app.clone();
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
            move |_, _, cx| {
                entity.update(cx, |app, cx| {
                    if let Some(s) = &mut app.stress_config_dialog {
                        s.stress_mode = mode;
                    }
                    cx.notify();
                });
            },
        )
}

/// 渲染连接模式芯片
fn render_conn_mode_chip(
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    mode: ConnectionMode,
    label: &str,
    theme: &Theme,
) -> Div {
    let selected = state.connection_mode == mode;
    let entity = app.clone();
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
            move |_, _, cx| {
                entity.update(cx, |app, cx| {
                    if let Some(s) = &mut app.stress_config_dialog {
                        s.connection_mode = mode;
                    }
                    cx.notify();
                });
            },
        )
}

/// 渲染停止条件芯片
fn render_stop_chip(
    app: &Entity<NetAssistantApp>,
    state: &StressConfigDialogState,
    stop_type: StopConditionType,
    label: &str,
    theme: &Theme,
    _cx: &App,
) -> Div {
    let selected = state.stop_condition_type == stop_type;
    let entity = app.clone();
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
            move |_, _, cx| {
                entity.update(cx, |app, cx| {
                    if let Some(s) = &mut app.stress_config_dialog {
                        s.stop_condition_type = stop_type;
                    }
                    cx.notify();
                });
            },
        )
}
