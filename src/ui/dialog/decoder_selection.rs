// 解码器选择弹窗
//
// 仿 NewConnectionDialog / StressConfigDialog 的 chip 选择风格:
// 顶部 chip 水平排列选择解码器类型, 选中"长度前缀"/"固定长度"时展开配置输入区,
// 底部双按钮(取消/确定)。

use std::borrow::Cow;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::Theme;
use gpui_component::input::{Input, InputState};

use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::{DecoderConfig, LengthDelimitedConfig};

/// 解码器种类(用于 UI 选择, 与具体配置参数分离)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderKind {
    Bytes,
    LineBased,
    LengthDelimited,
    FixedLength,
    Json,
}

impl DecoderKind {
    fn label(self) -> Cow<'static, str> {
        match self {
            DecoderKind::Bytes => t!("decoder_selection.kind_bytes"),
            DecoderKind::LineBased => t!("decoder_selection.kind_line_based"),
            DecoderKind::LengthDelimited => t!("decoder_selection.kind_length_delimited"),
            DecoderKind::FixedLength => t!("decoder_selection.kind_fixed_length"),
            DecoderKind::Json => t!("decoder_selection.kind_json"),
        }
    }

    fn desc(self) -> Cow<'static, str> {
        match self {
            DecoderKind::Bytes => t!("decoder_selection.desc_bytes"),
            DecoderKind::LineBased => t!("decoder_selection.desc_line_based"),
            DecoderKind::LengthDelimited => t!("decoder_selection.desc_length_delimited"),
            DecoderKind::FixedLength => t!("decoder_selection.desc_fixed_length"),
            DecoderKind::Json => t!("decoder_selection.desc_json"),
        }
    }
}

/// 解码器选择弹窗状态(持有所有输入实体, 打开弹窗时创建)
pub struct DecoderSelectionDialogState {
    pub tab_id: String,
    pub selected_kind: DecoderKind,
    // 长度前缀配置输入
    pub max_frame_length_input: Entity<InputState>,
    pub length_field_offset_input: Entity<InputState>,
    pub length_field_length_input: Entity<InputState>,
    pub length_adjustment_input: Entity<InputState>,
    pub length_includes_self: bool,
    pub length_little_endian: bool,
    pub length_keep_full_frame: bool,
    // 固定长度配置输入
    pub fixed_length_input: Entity<InputState>,
}

impl DecoderSelectionDialogState {
    /// 从已有解码器配置构造弹窗状态(回填)
    pub fn new(
        tab_id: String,
        config: DecoderConfig,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        let (kind, ld, fl) = match &config {
            DecoderConfig::Bytes => (DecoderKind::Bytes, LengthDelimitedConfig::default(), 8),
            DecoderConfig::LineBased => {
                (DecoderKind::LineBased, LengthDelimitedConfig::default(), 8)
            }
            DecoderConfig::LengthDelimited(c) => (DecoderKind::LengthDelimited, c.clone(), 8),
            DecoderConfig::FixedLength(n) => (
                DecoderKind::FixedLength,
                LengthDelimitedConfig::default(),
                *n,
            ),
            DecoderConfig::Json => (DecoderKind::Json, LengthDelimitedConfig::default(), 8),
        };

        Self {
            tab_id,
            selected_kind: kind,
            max_frame_length_input: make_input(&ld.max_frame_length.to_string(), window, cx),
            length_field_offset_input: make_input(&ld.length_field_offset.to_string(), window, cx),
            length_field_length_input: make_input(&ld.length_field_length.to_string(), window, cx),
            length_adjustment_input: make_input(&ld.length_adjustment.to_string(), window, cx),
            length_includes_self: ld.length_field_is_including_length_field,
            length_little_endian: ld.length_field_is_little_endian,
            length_keep_full_frame: ld.length_field_keep_full_frame,
            fixed_length_input: make_input(&fl.to_string(), window, cx),
        }
    }

    /// 从弹窗状态构建解码器配置
    pub fn build_config(&self, cx: &mut Context<NetAssistantApp>) -> DecoderConfig {
        let parse_usize = |input: &Entity<InputState>| -> usize {
            input.read(cx).value().trim().parse::<usize>().unwrap_or(0)
        };
        let parse_u8 = |input: &Entity<InputState>| -> u8 {
            input.read(cx).value().trim().parse::<u8>().unwrap_or(0)
        };
        let parse_i32 = |input: &Entity<InputState>| -> i32 {
            input.read(cx).value().trim().parse::<i32>().unwrap_or(0)
        };

        match self.selected_kind {
            DecoderKind::Bytes => DecoderConfig::Bytes,
            DecoderKind::LineBased => DecoderConfig::LineBased,
            DecoderKind::LengthDelimited => DecoderConfig::LengthDelimited(LengthDelimitedConfig {
                max_frame_length: parse_usize(&self.max_frame_length_input).max(1),
                length_field_offset: parse_u8(&self.length_field_offset_input),
                length_field_length: parse_u8(&self.length_field_length_input),
                length_adjustment: parse_i32(&self.length_adjustment_input),
                length_field_is_including_length_field: self.length_includes_self,
                length_field_is_little_endian: self.length_little_endian,
                length_field_keep_full_frame: self.length_keep_full_frame,
            }),
            DecoderKind::FixedLength => {
                DecoderConfig::FixedLength(parse_usize(&self.fixed_length_input).max(1))
            }
            DecoderKind::Json => DecoderConfig::Json,
        }
    }
}

/// 解码器选择弹窗组件
pub struct DecoderSelectionDialog;

impl DecoderSelectionDialog {
    pub fn render(
        app: &NetAssistantApp,
        _window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = match &app.decoder_selection_dialog {
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
                            .child(t!("decoder_selection.title").to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            // 解码器类型 chip 选择
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
                                            .child(t!("decoder_selection.decoder_type_label").to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .gap_2()
                                            .child(Self::render_kind_chip(
                                                state,
                                                DecoderKind::Bytes,
                                                &theme,
                                                cx,
                                            ))
                                            .child(Self::render_kind_chip(
                                                state,
                                                DecoderKind::LineBased,
                                                &theme,
                                                cx,
                                            ))
                                            .child(Self::render_kind_chip(
                                                state,
                                                DecoderKind::LengthDelimited,
                                                &theme,
                                                cx,
                                            ))
                                            .child(Self::render_kind_chip(
                                                state,
                                                DecoderKind::FixedLength,
                                                &theme,
                                                cx,
                                            ))
                                            .child(Self::render_kind_chip(
                                                state,
                                                DecoderKind::Json,
                                                &theme,
                                                cx,
                                            )),
                                    )
                                    // 当前选中说明
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(state.selected_kind.desc().to_string()),
                                    ),
                            )
                            // 长度前缀配置区(条件渲染)
                            .when(
                                state.selected_kind == DecoderKind::LengthDelimited,
                                |this| {
                                    this.child(Self::render_length_delimited_config(
                                        state, &theme, cx,
                                    ))
                                },
                            )
                            // 固定长度配置区(条件渲染)
                            .when(state.selected_kind == DecoderKind::FixedLength, |this| {
                                this.child(Self::render_fixed_length_config(state, &theme, cx))
                            }),
                    )
                    // 取消 / 确定
                    .child(Self::render_actions(state, &theme, cx)),
            )
            .into_any_element()
    }

    /// 渲染解码器类型 chip
    fn render_kind_chip(
        state: &DecoderSelectionDialogState,
        kind: DecoderKind,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        let selected = state.selected_kind == kind;
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
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .child(kind.label().to_string()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                    if let Some(s) = &mut app.decoder_selection_dialog {
                        s.selected_kind = kind;
                    }
                    cx.notify();
                }),
            )
    }

    /// 渲染长度前缀配置区
    fn render_length_delimited_config(
        state: &DecoderSelectionDialogState,
        theme: &Theme,
        cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .pl_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(t!("decoder_selection.length_prefix_params").to_string()),
            )
            // 帧结构示意
            .child(
                div().text_xs().text_color(theme.muted_foreground).child(
                    t!("decoder_selection.frame_structure").to_string(),
                ),
            )
            // 第一行: 长度字段偏移量 + 长度字段长度
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
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.length_field_offset").to_string()),
                            )
                            .child(Input::new(&state.length_field_offset_input).cleanable(true))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.length_field_offset_hint").to_string()),
                            ),
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
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.length_field_length").to_string()),
                            )
                            .child(Input::new(&state.length_field_length_input).cleanable(true))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.length_field_length_hint").to_string()),
                            ),
                    ),
            )
            // 第二行: 长度调整值 + 最大帧长度
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
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.length_adjustment").to_string()),
                            )
                            .child(Input::new(&state.length_adjustment_input).cleanable(true))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.length_adjustment_hint").to_string()),
                            ),
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
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.max_frame_length").to_string()),
                            )
                            .child(Input::new(&state.max_frame_length_input).cleanable(true))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.max_frame_length_hint").to_string()),
                            ),
                    ),
            )
            // 长度包含自身 checkbox
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .cursor_pointer()
                    .child(render_checkbox(state.length_includes_self, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.foreground)
                                    .child(t!("decoder_selection.length_includes_self").to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("decoder_selection.length_includes_self_hint").to_string()),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                            if let Some(s) = &mut app.decoder_selection_dialog {
                                s.length_includes_self = !s.length_includes_self;
                            }
                            cx.notify();
                        }),
                    ),
            )
            // 长度字段字节序: 大端序 / 小端序 chip 选择
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().text_color(theme.foreground).child(t!("decoder_selection.byte_order").to_string()))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            // 大端序 chip
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .when(!state.length_little_endian, |d| {
                                        d.bg(theme.primary).text_color(theme.primary_foreground)
                                    })
                                    .when(state.length_little_endian, |d| {
                                        d.bg(theme.border).text_color(theme.foreground)
                                    })
                                    .child(div().text_xs().font_medium().child(t!("decoder_selection.byte_order_big").to_string()))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                                            if let Some(s) = &mut app.decoder_selection_dialog {
                                                s.length_little_endian = false;
                                            }
                                            cx.notify();
                                        }),
                                    ),
                            )
                            // 小端序 chip
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .when(state.length_little_endian, |d| {
                                        d.bg(theme.primary).text_color(theme.primary_foreground)
                                    })
                                    .when(!state.length_little_endian, |d| {
                                        d.bg(theme.border).text_color(theme.foreground)
                                    })
                                    .child(div().text_xs().font_medium().child(t!("decoder_selection.byte_order_little").to_string()))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                                            if let Some(s) = &mut app.decoder_selection_dialog {
                                                s.length_little_endian = true;
                                            }
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    ),
            )
            // 保留完整帧 checkbox
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .cursor_pointer()
                    .child(render_checkbox(state.length_keep_full_frame, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.foreground)
                                    .child(t!("decoder_selection.keep_full_frame").to_string()),
                            )
                            .child(
                                div().text_xs().text_color(theme.muted_foreground).child(
                                    t!("decoder_selection.keep_full_frame_hint").to_string(),
                                ),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                            if let Some(s) = &mut app.decoder_selection_dialog {
                                s.length_keep_full_frame = !s.length_keep_full_frame;
                            }
                            cx.notify();
                        }),
                    ),
            )
    }

    /// 渲染固定长度配置区
    fn render_fixed_length_config(
        state: &DecoderSelectionDialogState,
        theme: &Theme,
        _cx: &mut Context<NetAssistantApp>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .pl_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(t!("decoder_selection.fixed_length_params").to_string()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(t!("decoder_selection.frame_length").to_string()),
                    )
                    .child(Input::new(&state.fixed_length_input).cleanable(true)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("decoder_selection.fixed_length_hint").to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("decoder_selection.fixed_length_flush_hint").to_string()),
            )
    }

    /// 渲染底部操作按钮
    fn render_actions(
        state: &DecoderSelectionDialogState,
        theme: &Theme,
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
                            .child(t!("decoder_selection.cancel").to_string()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _, _, cx| {
                            app.decoder_selection_dialog = None;
                            cx.notify();
                        }),
                    ),
            )
            // 确定
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
                            .child(t!("decoder_selection.confirm").to_string()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp, _, _, cx| {
                            if let Some(dialog) = app.decoder_selection_dialog.take() {
                                let new_config = dialog.build_config(cx);
                                // 更新配置到连接
                                if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                                    match &mut tab_state.connection_config {
                                        crate::config::connection::ConnectionConfig::Client(
                                            config,
                                        ) => {
                                            config.decoder_config = new_config.clone();
                                        }
                                        crate::config::connection::ConnectionConfig::Server(
                                            config,
                                        ) => {
                                            config.decoder_config = new_config.clone();
                                        }
                                    }
                                    // 保存到JSON配置
                                    app.storage
                                        .update_connection(tab_state.connection_config.clone());
                                }
                                // 运行时下发到在线连接(无需重连, 仅 TCP 生效)
                                app.apply_decoder_config_to_connection(&tab_id, &new_config);
                                app.decoder_selection_dialog = None;
                                cx.notify();
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
