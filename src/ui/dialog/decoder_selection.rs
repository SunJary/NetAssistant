// 解码器选择弹窗
//
// 基于 gpui_component::Dialog 实现:
// 通过 window.open_dialog 命令式打开(Root 管理对话框栈), 内容区用动态 content 模式
// 每帧从 NetAssistantApp 读取最新状态(chip 切换、参数表单随状态刷新),
// 内部使用 overflow_y_scrollbar 让内容过高时中间滚动, 标题与底部按钮固定。

use std::borrow::Cow;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::DialogFooter;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::ActiveTheme as _;
use gpui_component::StyledExt;
use gpui_component::Theme;
use gpui_component::WindowExt as _;
use gpui_component::input::{Input, InputState};

use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::{DecoderConfig, LengthDelimitedConfig};

use super::{dialog_content_max_height, dialog_height};

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

/// 打开解码器选择对话框(命令式, 由 Root 管理层叠)
///
/// 弹窗状态保存在 `app.decoder_selection_dialog`, 由调用方先创建好;
/// 内容闭包每帧从 app 读取最新状态, 保证 chip 切换/参数区动态刷新。
pub fn open_decoder_selection_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        dialog
            .title(t!("decoder_selection.title").to_string())
            .w(px(520.0))
            .max_h(dialog_height(window))
            // 表单类: 点击蒙层不关闭, 防止误触丢数据(与迁移前行为一致)
            .overlay_closable(false)
            // ESC 取消 / Enter 确认: Input 对 Enter 的处理是 propagate,
            // 输入框内按键会继续传播到 Dialog 动作, 经 tests/dialog_layout.rs 验证无误触
            .keyboard(true)
            .on_ok({
                let app = app.clone();
                move |_, _, cx| confirm_decoder_selection(&app, cx)
            })
            // X 按钮 / 蒙层关闭时同步清理弹窗状态
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.decoder_selection_dialog = None;
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
                    let theme = cx.theme().clone();
                    let Some(state) = entity.read(cx).decoder_selection_dialog.as_ref() else {
                        return content;
                    };
                    content.child(
                        // 滚动结构(经 tests/dialog_layout.rs 无头测试验证):
                        // max_h 必须在外层普通 div 上钳制可视区, 内层滚动容器不限高让内容自然撑高。
                        // 若 max_h 直接放滚动容器上, 被跟踪元素自身高度=可视区高度,
                        // 滚动机制认为内容未溢出, 滚轮不响应。
                        div().max_h(dialog_content_max_height(window)).child(
                            div().overflow_y_scrollbar().child(render_form(&entity, state, &theme, cx)),
                        ),
                    )
                }
            })
    });
}

/// 渲染表单主体(解码器类型 chips + 条件参数区)
fn render_form(
    app: &Entity<NetAssistantApp>,
    state: &DecoderSelectionDialogState,
    theme: &Theme,
    cx: &App,
) -> Div {
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
                        .child(render_kind_chip(app, state, DecoderKind::Bytes, theme, cx))
                        .child(render_kind_chip(app, state, DecoderKind::LineBased, theme, cx))
                        .child(render_kind_chip(
                            app,
                            state,
                            DecoderKind::LengthDelimited,
                            theme,
                            cx,
                        ))
                        .child(render_kind_chip(app, state, DecoderKind::FixedLength, theme, cx))
                        .child(render_kind_chip(app, state, DecoderKind::Json, theme, cx)),
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
        .when(state.selected_kind == DecoderKind::LengthDelimited, |this| {
            this.child(render_length_delimited_config(app, state, theme, cx))
        })
        // 固定长度配置区(条件渲染)
        .when(state.selected_kind == DecoderKind::FixedLength, |this| {
            this.child(render_fixed_length_config(state, theme, cx))
        })
}

/// 渲染解码器类型 chip
fn render_kind_chip(
    app: &Entity<NetAssistantApp>,
    state: &DecoderSelectionDialogState,
    kind: DecoderKind,
    theme: &Theme,
    _cx: &App,
) -> Div {
    let selected = state.selected_kind == kind;
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
        .child(
            div()
                .text_sm()
                .font_medium()
                .child(kind.label().to_string()),
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                if let Some(s) = &mut app.decoder_selection_dialog {
                    s.selected_kind = kind;
                }
                cx.notify();
            });
        })
}

/// 渲染长度前缀配置区
fn render_length_delimited_config(
    app: &Entity<NetAssistantApp>,
    state: &DecoderSelectionDialogState,
    theme: &Theme,
    _cx: &App,
) -> Div {
    let entity = app.clone();
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
                    {
                        let entity = entity.clone();
                        move |_, _, cx| {
                            entity.update(cx, |app, cx| {
                                if let Some(s) = &mut app.decoder_selection_dialog {
                                    s.length_includes_self = !s.length_includes_self;
                                }
                                cx.notify();
                            });
                        }
                    },
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
                                    {
                                        let entity = entity.clone();
                                        move |_, _, cx| {
                                            entity.update(cx, |app, cx| {
                                                if let Some(s) = &mut app.decoder_selection_dialog {
                                                    s.length_little_endian = false;
                                                }
                                                cx.notify();
                                            });
                                        }
                                    },
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
                                    {
                                        let entity = entity.clone();
                                        move |_, _, cx| {
                                            entity.update(cx, |app, cx| {
                                                if let Some(s) = &mut app.decoder_selection_dialog {
                                                    s.length_little_endian = true;
                                                }
                                                cx.notify();
                                            });
                                        }
                                    },
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
                    {
                        let entity = entity.clone();
                        move |_, _, cx| {
                            entity.update(cx, |app, cx| {
                                if let Some(s) = &mut app.decoder_selection_dialog {
                                    s.length_keep_full_frame = !s.length_keep_full_frame;
                                }
                                cx.notify();
                            });
                        }
                    },
                ),
        )
}

/// 渲染固定长度配置区
fn render_fixed_length_config(state: &DecoderSelectionDialogState, theme: &Theme, _cx: &App) -> Div {
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

/// 应用解码器选择(构建 DecoderConfig 并下发), 返回是否成功(成功后关闭对话框)
fn confirm_decoder_selection(app: &WeakEntity<NetAssistantApp>, cx: &mut App) -> bool {
    app.update(cx, |app, cx| {
        if let Some(dialog) = app.decoder_selection_dialog.take() {
            let tab_id = dialog.tab_id.clone();
            let new_config = dialog.build_config(cx);
            // 更新配置到连接
            if let Some(tab_state) = app.connection_tabs.get_mut(&tab_id) {
                match &mut tab_state.connection_config {
                    crate::config::connection::ConnectionConfig::Client(config) => {
                        config.decoder_config = new_config.clone();
                    }
                    crate::config::connection::ConnectionConfig::Server(config) => {
                        config.decoder_config = new_config.clone();
                    }
                }
                // 保存到JSON配置
                app.storage
                    .update_connection(tab_state.connection_config.clone());
            }
            // 运行时下发到在线连接(无需重连, 仅 TCP 生效)
            app.apply_decoder_config_to_connection(&tab_id, &new_config);
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
            Button::new("decoder-dialog-cancel")
                .outline()
                .label(t!("decoder_selection.cancel").to_string())
                .on_click(move |_, window, cx| {
                    let _ = app_cancel.update(cx, |app, cx| {
                        app.decoder_selection_dialog = None;
                        cx.notify();
                    });
                    window.close_dialog(cx);
                }),
        )
        // 确定
        .child(
            Button::new("decoder-dialog-ok")
                .primary()
                .label(t!("decoder_selection.confirm").to_string())
                .on_click(move |_, window, cx| {
                    confirm_decoder_selection(&app_ok, cx);
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
