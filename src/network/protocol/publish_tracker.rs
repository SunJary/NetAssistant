//! 发布确认追踪：为「发布失败提示」补上 pkid → topic 关联。
//!
//! rumqttc 的 `publish()` 不返回报文标识符，但 PUBLISH 真正写出后会产出
//! `Event::Outgoing(Outgoing::Publish(pkid))`；PUBACK / PUBREC / PUBCOMP 只带 pkid。
//! 这里在「下发前登记 topic/QoS」与「Outgoing 时绑定 pkid」之间补上关联，
//! 确认报文到达时可还原出被拒的是哪个主题（对标 mqtt.js 的 `outgoing` 映射）。
//!
//! 职责边界：只做映射、回滚、TTL 与上限，不碰 `AsyncClient`、不发事件，
//! 因而可以用假 pkid 写纯单测。
//!
//! 与 `SubscriptionTracker` 的差异：
//! - 发布是高频路径，QoS 0 无任何确认，**不入本表**（零额外开销）；
//! - 发布不需要「超时即失败」的主动上报，TTL 仅用于清理泄漏条目，
//!   真正的失败只来自 broker 的 reason code 或本地 `publish()` 错误。

use crate::config::connection::Qos;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// 在途发布的等待上限：超过即清理，避免 ACK 丢失导致 map 长期增长。
///
/// 正常 QoS 1/2 往返远小于该值；断线时调用方会直接 `reset()`。
pub(crate) const PUBLISH_ACK_TTL: Duration = Duration::from_secs(120);

/// 尚未绑定 pkid 的登记队列上限。
///
/// 写通道 1024 + rumqttc 请求通道 10 + 内部在途约 100，理论上限约 1.2k；
/// 取 2048 留出余量。真正触顶说明队列已严重积压，此时**忽略新登记**：
/// 已入队的最早项仍与最早写出的 PUBLISH 一一对应，只让新发布在失败时退化为
/// 「未知主题」。不能丢最早项或整体清空——那会让之后所有 bind 整体错位。
pub(crate) const MAX_UNBOUND: usize = 2048;

struct PendingPublish {
    topic: String,
    qos: Qos,
    deadline: Instant,
}

/// 发布确认追踪器（仅 MQTT 5 使用：3.1.1 的 PUBACK 没有 reason code）。
#[derive(Default)]
pub(crate) struct PublishTracker {
    /// 已登记、等待网络层写出（Outgoing 事件）的发布；FIFO 与 publish 调用顺序一致
    unbound: VecDeque<(String, Qos)>,
    /// 已写出、等待 PUBACK / PUBREC / PUBCOMP 的发布
    inflight: HashMap<u16, PendingPublish>,
}

impl PublishTracker {
    /// 在 `client.publish()` 之前登记；QoS 0 无回执，直接忽略。
    pub(crate) fn queue(&mut self, topic: String, qos: Qos) {
        if qos == Qos::AtMostOnce {
            return;
        }
        if self.unbound.len() >= MAX_UNBOUND {
            // 队列已积压到上限：忽略新登记（不背压发布，也不破坏已有 FIFO 对应关系）。
            // 该条发布若被拒，确认时退化为「未知主题」文案。
            return;
        }
        self.unbound.push_back((topic, qos));
    }

    /// 同步下发失败时回滚刚登记的项，避免后续发布整体错位一位。
    pub(crate) fn rollback_last(&mut self) {
        self.unbound.pop_back();
    }

    /// 网络层看到 `Outgoing::Publish(pkid)` 时绑定最早登记的发布。
    ///
    /// `pkid == 0` 表示 QoS 0（无确认），返回 `false`；
    /// 没有待绑定项（如重连后 rumqttc 重发的旧在途报文）同样返回 `false`。
    pub(crate) fn bind(&mut self, pkid: u16) -> bool {
        if pkid == 0 {
            return false;
        }
        match self.unbound.pop_front() {
            Some((topic, qos)) => {
                self.inflight.insert(
                    pkid,
                    PendingPublish {
                        topic,
                        qos,
                        deadline: Instant::now() + PUBLISH_ACK_TTL,
                    },
                );
                true
            }
            None => false,
        }
    }

    /// 确认报文到达：按 pkid 取出（并移除）对应的发布。
    ///
    /// 未知 pkid（旧会话残留 / 未登记）→ `None`，调用方忽略。
    pub(crate) fn resolve(&mut self, pkid: u16) -> Option<(String, Qos)> {
        self.inflight
            .remove(&pkid)
            .map(|pending| (pending.topic, pending.qos))
    }

    /// 清理超时未确认的条目，返回清理数量（仅用于日志）。
    pub(crate) fn expire(&mut self, now: Instant) -> usize {
        let before = self.inflight.len();
        self.inflight.retain(|_, pending| now < pending.deadline);
        before - self.inflight.len()
    }

    /// 断线 / 重连：清空全部状态，避免旧 pkid 命中新会话的确认。
    pub(crate) fn reset(&mut self) {
        self.unbound.clear();
        self.inflight.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_bind_resolve_maps_topic_in_fifo_order() {
        let mut tracker = PublishTracker::default();
        tracker.queue("a/1".into(), Qos::AtLeastOnce);
        tracker.queue("b/1".into(), Qos::ExactlyOnce);
        assert!(tracker.bind(1));
        assert!(tracker.bind(2));

        // 乱序确认：先回 pkid=2，仍映射到第二条
        assert_eq!(
            tracker.resolve(2),
            Some(("b/1".to_string(), Qos::ExactlyOnce))
        );
        assert_eq!(
            tracker.resolve(1),
            Some(("a/1".to_string(), Qos::AtLeastOnce))
        );
        assert!(tracker.resolve(1).is_none(), "条目已消费，不应重复命中");
    }

    #[test]
    fn qos0_is_not_tracked_and_pkid0_never_binds() {
        let mut tracker = PublishTracker::default();
        tracker.queue("fire/forget".into(), Qos::AtMostOnce);
        assert!(!tracker.bind(0), "QoS 0 无确认，不得登记");
        assert_eq!(tracker.resolve(0), None);
    }

    #[test]
    fn rollback_keeps_subsequent_bindings_aligned() {
        let mut tracker = PublishTracker::default();
        tracker.queue("first".into(), Qos::AtLeastOnce);
        tracker.rollback_last();
        tracker.queue("second".into(), Qos::AtLeastOnce);
        assert!(tracker.bind(9));
        assert_eq!(
            tracker.resolve(9),
            Some(("second".to_string(), Qos::AtLeastOnce))
        );
    }

    #[test]
    fn unknown_or_resolved_pkid_returns_none() {
        let mut tracker = PublishTracker::default();
        assert!(tracker.resolve(999).is_none());
        tracker.queue("a".into(), Qos::AtLeastOnce);
        tracker.bind(3);
        assert!(tracker.resolve(3).is_some());
        assert!(tracker.resolve(3).is_none());
    }

    #[test]
    fn expire_drops_stale_entries_only() {
        let mut tracker = PublishTracker::default();
        tracker.queue("stale".into(), Qos::AtLeastOnce);
        tracker.bind(1);
        let cleared = tracker.expire(Instant::now() + PUBLISH_ACK_TTL + Duration::from_secs(1));
        assert_eq!(cleared, 1);
        assert!(tracker.resolve(1).is_none(), "过期条目必须已清理");

        tracker.queue("fresh".into(), Qos::AtLeastOnce);
        tracker.bind(2);
        assert_eq!(tracker.expire(Instant::now()), 0);
        assert!(tracker.resolve(2).is_some());
    }

    #[test]
    fn reset_clears_unbound_and_inflight() {
        let mut tracker = PublishTracker::default();
        tracker.queue("a".into(), Qos::AtLeastOnce);
        tracker.bind(1);
        tracker.queue("b".into(), Qos::AtLeastOnce);
        tracker.reset();
        assert!(tracker.resolve(1).is_none());
        assert!(!tracker.bind(2), "reset 后不应再有待绑定项");
    }

    #[test]
    fn unbound_overflow_drops_newest_without_misbinding() {
        let mut tracker = PublishTracker::default();
        for i in 0..MAX_UNBOUND {
            tracker.queue(format!("t/{i}"), Qos::AtLeastOnce);
        }
        // 触顶后新登记被忽略：已有 FIFO 对应关系不被破坏
        tracker.queue("overflow".into(), Qos::AtLeastOnce);
        for pkid in 1..=MAX_UNBOUND as u16 {
            assert!(tracker.bind(pkid), "已登记项都能依次绑定");
        }
        assert!(
            !tracker.bind(9999),
            "被忽略的登记不会凭空绑定，确认退化为未知主题而非错配"
        );
    }
}
