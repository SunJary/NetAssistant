// 「插入变量」浮层组件(通用)
//
// 调用方传入变量列表(items)与两个回调(on_dismiss / on_pick), 由本组件负责渲染与定位:
// - 一行一个变量, 显示变量名 + 简短说明, 点击后回调调用方插入变量并关闭
// - 普通消息区工具栏与自动回复区各挂一个按钮, 复用同一组件
//
// 采用 gpui-component combobox 同款 deferred+anchored 模式:
// - deferred 在独立合成层渲染, 不会被滚动区/兄弟节点覆盖
// - anchored 依据按钮的窗口坐标定位, 并自动吸附窗口边缘防止越界
// - on_mouse_down_out 在面板外点击时关闭
//
// 分组: 规则编辑弹窗需要「接收帧变量 / 生成校验 / 表达式」三组额外变量,
// 用 `VariableGroup` 表达分组(无标题 = 与旧行为完全一致的单组扁平列表)。

use std::borrow::Cow;
use std::sync::Arc;

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::Theme;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;
use rust_i18n::t;

/// 变量浮层的当前目标(同一时刻只允许一个浮层)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariablePickerTarget {
    /// 普通消息输入框
    Message,
}

/// 变量定义: 名称、简短说明、点击后插入的文本
#[derive(Clone)]
pub struct VariableItem {
    pub name: &'static str,
    pub description: Cow<'static, str>,
    pub insert_text: &'static str,
}

/// 变量分组(标题可空: 空 = 扁平列表, 与旧行为一致)
pub struct VariableGroup {
    pub title: Option<Cow<'static, str>>,
    pub items: Vec<VariableItem>,
}

/// 普通消息可用变量(顺序即展示顺序)
///
/// 只列公共变量(时间类 / uuid / random / seq); 压测专属的 worker_id / counter 不暴露。
pub fn message_variable_items() -> Vec<VariableItem> {
    vec![
        VariableItem {
            name: "${date}",
            description: t!("variable_picker.desc_date"),
            insert_text: "${date}",
        },
        VariableItem {
            name: "${time}",
            description: t!("variable_picker.desc_time"),
            insert_text: "${time}",
        },
        VariableItem {
            name: "${datetime}",
            description: t!("variable_picker.desc_datetime"),
            insert_text: "${datetime}",
        },
        VariableItem {
            name: "${datetime_ms}",
            description: t!("variable_picker.desc_datetime_ms"),
            insert_text: "${datetime_ms}",
        },
        VariableItem {
            name: "${iso}",
            description: t!("variable_picker.desc_iso"),
            insert_text: "${iso}",
        },
        VariableItem {
            name: "${utc}",
            description: t!("variable_picker.desc_utc"),
            insert_text: "${utc}",
        },
        VariableItem {
            name: "${timestamp}",
            description: t!("variable_picker.desc_timestamp"),
            insert_text: "${timestamp}",
        },
        VariableItem {
            name: "${timestamp_s}",
            description: t!("variable_picker.desc_timestamp_s"),
            insert_text: "${timestamp_s}",
        },
        VariableItem {
            name: "${time:%Y/%m/%d %H:%M:%S}",
            description: t!("variable_picker.desc_time_custom"),
            insert_text: "${time:%Y/%m/%d %H:%M:%S}",
        },
        VariableItem {
            name: "${uuid}",
            description: t!("variable_picker.desc_uuid"),
            insert_text: "${uuid}",
        },
        VariableItem {
            name: "${random:min:max}",
            description: t!("variable_picker.desc_random"),
            insert_text: "${random:1:100}",
        },
        VariableItem {
            name: "${seq}",
            description: t!("variable_picker.desc_seq_common"),
            insert_text: "${seq}",
        },
    ]
}

/// 压测浮层的变量列表(顺序与文案保持既有 6 项不变)
pub fn stress_variable_items() -> Vec<VariableItem> {
    vec![
        VariableItem {
            name: "${seq}",
            description: t!("variable_picker.desc_seq"),
            insert_text: "${seq}",
        },
        VariableItem {
            name: "${worker_id}",
            description: t!("variable_picker.desc_worker_id"),
            insert_text: "${worker_id}",
        },
        VariableItem {
            name: "${counter}",
            description: t!("variable_picker.desc_counter"),
            insert_text: "${counter}",
        },
        VariableItem {
            name: "${timestamp}",
            description: t!("variable_picker.desc_timestamp"),
            insert_text: "${timestamp}",
        },
        VariableItem {
            name: "${uuid}",
            description: t!("variable_picker.desc_uuid"),
            insert_text: "${uuid}",
        },
        VariableItem {
            name: "${random:min:max}",
            description: t!("variable_picker.desc_random"),
            insert_text: "${random:1:100}",
        },
    ]
}

/// 「接收帧变量」分组: `${rx.*}` —— 取自**触发规则的那一帧**(仅规则应答可用)
pub fn rx_variable_items() -> Vec<VariableItem> {
    vec![
        VariableItem {
            name: "${rx.raw}",
            description: t!("variable_picker.desc_rx_raw"),
            insert_text: "${rx.raw}",
        },
        VariableItem {
            name: "${rx.raw:0:4}",
            description: t!("variable_picker.desc_rx_raw_slice"),
            insert_text: "${rx.raw:0:4}",
        },
        VariableItem {
            name: "${rx.len}",
            description: t!("variable_picker.desc_rx_len"),
            insert_text: "${rx.len}",
        },
        VariableItem {
            name: "${rx.src}",
            description: t!("variable_picker.desc_rx_src"),
            insert_text: "${rx.src}",
        },
        VariableItem {
            name: "${rx.src_ip}",
            description: t!("variable_picker.desc_rx_src_ip"),
            insert_text: "${rx.src_ip}",
        },
        VariableItem {
            name: "${rx.port}",
            description: t!("variable_picker.desc_rx_port"),
            insert_text: "${rx.port}",
        },
        VariableItem {
            name: "${rx.u8:0}",
            description: t!("variable_picker.desc_rx_u8"),
            insert_text: "${rx.u8:0}",
        },
        VariableItem {
            name: "${rx.u16be:0}",
            description: t!("variable_picker.desc_rx_u16be"),
            insert_text: "${rx.u16be:0}",
        },
        VariableItem {
            name: "${rx.u16le:0}",
            description: t!("variable_picker.desc_rx_u16le"),
            insert_text: "${rx.u16le:0}",
        },
        VariableItem {
            name: "${rx.i32be:0}",
            description: t!("variable_picker.desc_rx_i32be"),
            insert_text: "${rx.i32be:0}",
        },
        VariableItem {
            name: "${rx.hex:0:4}",
            description: t!("variable_picker.desc_rx_hex"),
            insert_text: "${rx.hex:0:4}",
        },
        VariableItem {
            name: "${rx.ascii:0:4}",
            description: t!("variable_picker.desc_rx_ascii"),
            insert_text: "${rx.ascii:0:4}",
        },
        VariableItem {
            name: "${rx.crc16modbus:0:6}",
            description: t!("variable_picker.desc_rx_crc16modbus"),
            insert_text: "${rx.crc16modbus:0:6}",
        },
    ]
}

/// 「生成校验(发送时填充)」分组: 渲染应答时按**应答内容**现算校验位
pub fn checksum_variable_items() -> Vec<VariableItem> {
    vec![
        VariableItem {
            name: "${crc16modbus:0:6}",
            description: t!("variable_picker.desc_gen_crc16modbus"),
            insert_text: "${crc16modbus:0:6}",
        },
        VariableItem {
            name: "${crc16modbus:0:6:le}",
            description: t!("variable_picker.desc_gen_crc16modbus_le"),
            insert_text: "${crc16modbus:0:6:le}",
        },
        VariableItem {
            name: "${xor:0:6}",
            description: t!("variable_picker.desc_gen_xor"),
            insert_text: "${xor:0:6}",
        },
        VariableItem {
            name: "${sum8:0:6}",
            description: t!("variable_picker.desc_gen_sum8"),
            insert_text: "${sum8:0:6}",
        },
        VariableItem {
            name: "${lrc:0:6}",
            description: t!("variable_picker.desc_gen_lrc"),
            insert_text: "${lrc:0:6}",
        },
        VariableItem {
            name: "${crc32:0:6}",
            description: t!("variable_picker.desc_gen_crc32"),
            insert_text: "${crc32:0:6}",
        },
    ]
}

/// 「表达式」分组: `${= ... }` 单表达式引擎(与 `rx.*` 取值层共用同一实现)
pub fn expr_variable_items() -> Vec<VariableItem> {
    vec![
        VariableItem {
            name: "${= 1 + 1 }",
            description: t!("variable_picker.desc_expr_arith"),
            insert_text: "${= 1 + 1 }",
        },
        VariableItem {
            name: "${= rx.u16be(0) + 1 }",
            description: t!("variable_picker.desc_expr_rx"),
            insert_text: "${= rx.u16be(0) + 1 }",
        },
        VariableItem {
            name: "${= if(rx.u8(0) == 1, 1, 0) }",
            description: t!("variable_picker.desc_expr_if"),
            insert_text: "${= if(rx.u8(0) == 1, 1, 0) }",
        },
        VariableItem {
            name: "${= bits(rx.u16be(0), 0, 4) }",
            description: t!("variable_picker.desc_expr_bits"),
            insert_text: "${= bits(rx.u16be(0), 0, 4) }",
        },
    ]
}

/// 规则编辑弹窗的完整分组列表(公共变量 + 三个规则专属分组)
pub fn reply_rule_variable_groups() -> Vec<VariableGroup> {
    vec![
        VariableGroup {
            title: Some(t!("variable_picker.group_common")),
            items: message_variable_items(),
        },
        VariableGroup {
            title: Some(t!("variable_picker.group_rx")),
            items: rx_variable_items(),
        },
        VariableGroup {
            title: Some(t!("variable_picker.group_checksum")),
            items: checksum_variable_items(),
        },
        VariableGroup {
            title: Some(t!("variable_picker.group_expr")),
            items: expr_variable_items(),
        },
    ]
}

/// 渲染「插入变量」浮层(单组扁平列表, 保持既有调用方行为不变)
///
/// `button_bounds` 为「插入变量」按钮在窗口坐标系中的 bounds (由 on_prepaint 提供),
/// 面板锚定在按钮右下角, 向下展开; 若越界则由 anchored 自动吸附窗口边缘。
pub fn render_variable_picker(
    items: Vec<VariableItem>,
    button_bounds: Bounds<Pixels>,
    theme: &Theme,
    on_dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_pick: impl Fn(&VariableItem, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    render_grouped_variable_picker(
        vec![VariableGroup { title: None, items }],
        button_bounds,
        theme,
        on_dismiss,
        on_pick,
    )
}

/// 渲染「插入变量」浮层(带分组标题)
pub fn render_grouped_variable_picker(
    groups: Vec<VariableGroup>,
    button_bounds: Bounds<Pixels>,
    theme: &Theme,
    on_dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_pick: impl Fn(&VariableItem, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_dismiss: Arc<dyn Fn(&MouseDownEvent, &mut Window, &mut App)> = Arc::new(on_dismiss);
    let on_pick: Arc<dyn Fn(&VariableItem, &mut Window, &mut App)> = Arc::new(on_pick);

    // 点击面板外任意区域关闭浮层
    let dismiss_handler: Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static> = Box::new(
        move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
            on_dismiss(event, window, cx);
        },
    );

    let mut rows: Vec<Div> = Vec::new();
    for group in groups {
        if let Some(title) = group.title {
            rows.push(
                div()
                    .px_3()
                    .pt_2()
                    .pb_1()
                    .text_xs()
                    .font_semibold()
                    .text_color(theme.muted_foreground)
                    .child(title.to_string()),
            );
        }
        for item in group.items {
            rows.push(render_variable_row(item, theme, on_pick.clone()));
        }
    }

    let popup = anchored()
        .anchor(Anchor::TopRight)
        .position(button_bounds.bottom_right())
        .offset(point(px(0.0), px(4.0)))
        .snap_to_window_with_margin(px(8.0))
        .child(
            div()
                .occlude()
                .w(px(320.0))
                .h(px(320.0))
                .flex()
                .flex_col()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .rounded_md()
                .shadow_lg()
                .overflow_hidden()
                .on_mouse_down_out(dismiss_handler)
                // 标题 (固定)
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(theme.border)
                        .text_xs()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(t!("variable_picker.title").to_string()),
                )
                // 变量列表 (两层结构: 外层分配剩余高度, 内层滚动)
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .child(div().size_full().overflow_y_scrollbar().children(rows)),
                )
                // 底部提示 (固定)
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("variable_picker.hint").to_string()),
                ),
        );

    // deferred 在独立合成层渲染; 用 POPUP_PRIORITY 保证盖过弹窗(open_dialog 以 10+layer 渲染)
    deferred(popup).with_priority(gpui_kit::base::POPUP_PRIORITY)
}

/// 渲染单行变量(变量名 + 说明), 点击后交给调用方插入并关闭浮层
fn render_variable_row(
    item: VariableItem,
    theme: &Theme,
    on_pick: Arc<dyn Fn(&VariableItem, &mut Window, &mut App)>,
) -> Div {
    let pick_item = item.clone();
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .px_3()
        .py_1p5()
        .cursor_pointer()
        .hover(|d| d.bg(theme.border))
        .child(
            div()
                .text_xs()
                .font_medium()
                .font_family("JetBrains Mono")
                .text_color(theme.primary)
                .child(item.name.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(item.description.to_string()),
        )
        .on_mouse_down(
            MouseButton::Left,
            move |_event, window: &mut Window, cx: &mut App| {
                on_pick(&pick_item, window, cx);
            },
        )
}
