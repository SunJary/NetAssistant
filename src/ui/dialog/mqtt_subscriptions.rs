// 「MQTT 订阅」编辑弹窗
//
// 入口:
//   - 侧栏 MQTT 订阅块标题行右侧的「＋」图标 → **新增**模式;
//   - 侧栏订阅行右键菜单的「编辑订阅…」 → **编辑**模式。
// 职责: 单条订阅的表单 —— topic 输入 + QoS 选择 + 取消/保存。
// 弹窗**只做单条编辑**: 没有列表、没有底部新增、没有行内编辑、没有删除入口;
// 取消订阅只走侧栏右键菜单。
//
// 订阅严格按连接隔离: 弹窗持有 `tab_id`, 只读写该连接的订阅。
//
// 打开/关闭沿用 gpui_component 命令式对话框惯例(见 reply_rules.rs / timed_task.rs):
// 状态挂在 `app.mqtt_subscriptions_dialog`, 内容闭包每帧从 app 读取最新状态。

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::*;
use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::Qos;
use crate::ui::connection_tab::qos_chips_row;

use super::dialog_height;

/// 「MQTT 订阅」编辑弹窗状态(打开时创建, 关闭时由 app 置 None)
pub struct MqttSubscriptionDialogState {
    /// 本弹窗所属的连接 id（= tab_id）。订阅严格按连接隔离。
    pub tab_id: String,
    /// 编辑的原 topic；None = 新增模式
    pub old_topic: Option<String>,
    /// topic 输入实体(打开弹窗时创建并预填)
    pub input: Entity<InputState>,
    /// QoS 选择草稿
    pub qos: Qos,
}

impl MqttSubscriptionDialogState {
    /// 新增模式: 空白输入 + 默认 QoS 0
    pub fn new_add(
        tab_id: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("connection_tab.mqtt_sub_topic_placeholder").to_string())
        });
        Self {
            tab_id: tab_id.into(),
            old_topic: None,
            input,
            qos: Qos::AtMostOnce,
        }
    }

    /// 编辑模式: 预填原 topic 与 QoS
    pub fn new_edit(
        tab_id: impl Into<String>,
        topic: impl Into<String>,
        qos: Qos,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        let topic = topic.into();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("connection_tab.mqtt_sub_topic_placeholder").to_string())
        });
        input.update(cx, |st, cx| st.set_value(topic.clone(), window, cx));
        Self {
            tab_id: tab_id.into(),
            old_topic: Some(topic),
            input,
            qos,
        }
    }

    /// 是否编辑模式(标题文案由此分流)
    pub fn is_edit(&self) -> bool {
        self.old_topic.is_some()
    }
}

/// 打开订阅编辑弹窗(命令式, 由 Root 管理层叠)
pub fn open_mqtt_subscriptions_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        dialog
            .title(dialog_title(&app, cx))
            .w(px(600.0))
            .max_h(dialog_height(window))
            .keyboard(false)
            .on_cancel({
                let app = app.clone();
                // Esc / 右上角关闭: 关窗并清空订阅块局部错误, 避免残留红字
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        let tab_id = app
                            .mqtt_subscriptions_dialog
                            .as_ref()
                            .map(|d| d.tab_id.clone());
                        app.mqtt_subscriptions_dialog = None;
                        clear_sub_error(app, tab_id);
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
                    content.child(render_body(&entity, window, cx))
                }
            })
    });
}

/// 标题按模式分流: 新增订阅 / 编辑订阅
fn dialog_title(app: &WeakEntity<NetAssistantApp>, cx: &App) -> String {
    let is_edit = app
        .upgrade()
        .map(|e| {
            e.read(cx)
                .mqtt_subscriptions_dialog
                .as_ref()
                .is_some_and(MqttSubscriptionDialogState::is_edit)
        })
        .unwrap_or(false);
    if is_edit {
        t!("mqtt_subscriptions.title_edit").to_string()
    } else {
        t!("mqtt_subscriptions.title_add").to_string()
    }
}

/// 清空指定连接的订阅块局部错误
fn clear_sub_error(app: &mut NetAssistantApp, tab_id: Option<String>) {
    if let Some(tab_id) = tab_id {
        if let Some(tab) = app.connection_tabs.get_mut(&tab_id) {
            tab.mqtt_sub_error = None;
        }
    }
}

/// 渲染弹窗主体: 单条订阅表单(topic 输入 + QoS 芯片) + 错误红字
fn render_body(app: &Entity<NetAssistantApp>, _window: &Window, cx: &App) -> Div {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let Some(dialog) = state.mqtt_subscriptions_dialog.as_ref() else {
        return div();
    };
    let tab_id = dialog.tab_id.clone();
    let qos = dialog.qos;
    let input = dialog.input.clone();
    let error = state
        .connection_tabs
        .get(&tab_id)
        .and_then(|t| t.mqtt_sub_error.clone());

    // QoS 芯片: 写弹窗内的表单草稿 `qos`
    let on_qos = {
        let entity = app.clone();
        move |qos: Qos, _window: &mut Window, cx: &mut App| {
            entity.update(cx, |app, cx| {
                if let Some(d) = app.mqtt_subscriptions_dialog.as_mut() {
                    d.qos = qos;
                }
                cx.notify();
            });
        }
    };

    let mut body = div()
        .flex()
        .flex_col()
        .gap_3()
        .px_6()
        .pb_4()
        .child(div().w_full().child(Input::new(&input).cleanable(true)))
        .child(qos_chips_row(&theme, qos, on_qos));
    if let Some(err) = error {
        body = body.child(
            div()
                .text_xs()
                .font_medium()
                .text_color(theme.danger)
                .child(err),
        );
    }
    body
}

/// 底部操作按钮: 取消 / 保存
fn render_footer(app: &WeakEntity<NetAssistantApp>, _cx: &App) -> DialogFooter {
    // 取消: 关窗并清空订阅块局部错误
    let app_cancel = app.clone();
    let cancel = Button::new("mqtt-sub-cancel")
        .outline()
        .label(t!("mqtt_subscriptions.cancel").to_string())
        .on_click(move |_, window, cx| {
            let _ = app_cancel.update(cx, |app, cx| {
                let tab_id = app
                    .mqtt_subscriptions_dialog
                    .as_ref()
                    .map(|d| d.tab_id.clone());
                app.mqtt_subscriptions_dialog = None;
                clear_sub_error(app, tab_id);
                cx.notify();
            });
            window.close_dialog(cx);
        });

    // 保存: 读输入框当前值, 新增/编辑分流。
    // 只有操作**确实生效**才关窗; 失败(空 topic / 主题重复 / 未连接投递失败)
    // 保留弹窗与已输入内容, 配合红字提示便于就地修正。
    let app_save = app.clone();
    let save = Button::new("mqtt-sub-save")
        .primary()
        .label(t!("mqtt_subscriptions.save").to_string())
        .on_click(move |_, window, cx| {
            let applied = app_save
                .update(cx, |app, cx| {
                    // 先取快照, 让 `mqtt_subscriptions_dialog` 的不可变借用在此结束,
                    // 之后才能对 app 做可变调用。
                    let (tab_id, old_topic, input, qos) = {
                        let Some(d) = app.mqtt_subscriptions_dialog.as_ref() else {
                            return false;
                        };
                        (d.tab_id.clone(), d.old_topic.clone(), d.input.clone(), d.qos)
                    };
                    let value = input.read(cx).value().trim().to_string();
                    let applied = match &old_topic {
                        Some(old) => app.mqtt_edit_subscription(&tab_id, old, &value, qos, cx),
                        None => app.mqtt_subscribe_topic(&tab_id, &value, qos, cx),
                    };
                    if applied {
                        app.mqtt_subscriptions_dialog = None;
                    }
                    cx.notify();
                    applied
                })
                .unwrap_or(false);
            if applied {
                window.close_dialog(cx);
            }
        });

    DialogFooter::new().child(cancel).child(save)
}
