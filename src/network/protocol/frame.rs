//! 与协议无关的「读路径帧处理」共享骨架。
//!
//! TCP 与 MQTT 复用同一段逻辑：规则引擎调用 → 应答投递（闭包注入）→ 计数 →
//! `ReceivedBatch` 组装、批量事件推送。UDP 因应答须回发**数据报来源**（`target`）
//! 且附带 `unexpected_source` 装饰，其读路径另有独立实现（`udp.rs` 的
//! `try_udp_rule_reply`），不复用本模块。
//!
//! 之所以抽到中立模块而非放在 `tcp.rs`：避免 `mqtt` → `tcp` 的协议间横向依赖，
//! 三个协议的读路径共享同一份实现（规划 §6.3 前置重构的模块归属修正）。

use crate::core::message_processor::MessageProcessor;
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::{ConnectionEvent, NetCounters, ReceivedBatch};
use crate::reply::exec::{RuleWireMode, handle_frame, rule_wire_mode};
use crate::reply::{ReplyRulesStore, RxFrame};
use bytes::BytesMut;
use log::{error, warn};
use smol::channel::Sender;
use std::net::SocketAddr;
use std::sync::Arc;

/// 规则应答的投递方式：由各协议注入。
///
/// 参数为（应答字节、规则自带的编码方式），返回是否投递成功——成功则由共享
/// 骨架记入 `batch.sent_messages`。TCP 用 `WireMessage::inherit/bypass`；
/// MQTT 用 `inherit_mqtt` 携带回发的 topic/QoS（规划 D-3）。
pub(crate) type ReplyDelivery<'a> = &'a dyn Fn(Arc<[u8]>, RuleWireMode) -> bool;

/// 共享帧处理骨架：规则引擎调用 / 计数 / `ReceivedBatch` 组装（规划 §6.3）。
///
/// 先跑回复规则（只可能追加一条应答），帧本身照常进明细、照常计数。应答的
/// **构造方式**由 `deliver_reply` 闭包注入，因此调用方各自的行为保持不变。
///
/// `topic`/`retain` 为 MQTT 传输元数据：TCP/UDP 传 `None`/`false`，经
/// `process_received_message_with_transport` 的默认实现还原为默认值，行为逐字不变。
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_frame_shared(
    data: BytesMut,
    processor: &Arc<dyn MessageProcessor>,
    batch: &mut ReceivedBatch,
    counters: &Option<NetCounters>,
    message_type: MessageType,
    rules: &Arc<ReplyRulesStore>,
    connection_id: &str,
    source: SocketAddr,
    topic: Option<String>,
    retain: bool,
    deliver_reply: ReplyDelivery<'_>,
) {
    let raw_len = data.len() as u64;
    // P-6：整帧只复制一次（`BytesMut` → `Arc<[u8]>`），规则引擎的 `RxFrame`
    // 与消息明细共享同一份缓冲，不再各复制一份。
    let raw: Arc<[u8]> = Arc::from(&data[..]);

    if rules.is_enabled() {
        // P-6：`raw` 已由调用方与本帧的 Message 共享，这里不再 `to_vec()`
        let frame = Arc::new(RxFrame::from_shared(raw.clone(), source));
        let outcome = handle_frame(rules, &frame, connection_id);

        if let Some(bytes) = &outcome.reply {
            // P-6：应答字节也共享一份 `Arc<[u8]>` —— 明细用 Arc，投递项仍需独立 Vec
            let shared: Arc<[u8]> = Arc::from(bytes.as_slice());
            // 规则自带的编码方式：`Raw` = 原样输出（二进制协议必需）；
            // `Inherit` 时是否追加连接级 trailer 由各协议的投递闭包决定（决策 D-8）
            let mode = rule_wire_mode(outcome.codec);
            if deliver_reply(shared.clone(), mode) {
                batch.sent_messages.push(
                    Message::new(MessageDirection::Sent, shared, MessageType::Text)
                        .with_source(source.to_string()),
                );
            } else {
                // 通道满/关闭：不阻塞网络任务，只告警（沿用既有 try_send 语义）
                warn!("[reply] 投递应答失败: rule={:?}", outcome.rule_id);
            }
        }
    }

    let message = processor.process_received_message_with_transport(raw, message_type, topic, retain);
    if let Some(c) = counters {
        c.add_received(1);
        c.add_received_bytes(raw_len);
    }
    batch.count += 1;
    batch.bytes += raw_len;
    batch.messages.push(message);
}

/// 将本批消息一次性推送为单个 `MessagesReceived` 事件（洪泛聚合路径）。
pub(crate) fn flush_batch(
    event_sender: &Option<Sender<ConnectionEvent>>,
    connection_id: &str,
    batch: &mut ReceivedBatch,
) {
    if batch.is_empty() {
        return;
    }
    let batch = std::mem::take(batch);
    if let Some(sender) = event_sender {
        if let Err(e) = sender.try_send(ConnectionEvent::MessagesReceived(
            connection_id.to_string(),
            batch,
        )) {
            error!("发送 MessagesReceived 事件失败: {:?}", e);
        }
    }
}
