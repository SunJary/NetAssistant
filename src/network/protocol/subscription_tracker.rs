//! 订阅确认追踪：对标 mqtt.js 的 `messageIdToTopic` / `outgoing` 映射。
//!
//! rumqttc 的 `subscribe()` 不返回 pkid，但会在 SUBSCRIBE 真正写出后产出
//! `Event::Outgoing(Outgoing::Subscribe(pkid))`。因此这里在「下发前登记 topics」
//! 与「Outgoing 时绑定 pkid」之间补上关联，SUBACK 到达时即可按 pkid 还原逐 topic 结果。
//!
//! 职责边界：只做映射、超时与数量校验，不碰 `AsyncClient`、不发事件，
//! 因而可以用假 pkid 写纯单测；v3.1.1 / v5 共用同一份实现。

use crate::network::events::MqttSubAckItem;
use rust_i18n::t;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// 单个 SUBSCRIBE 包最多携带的 topic 数（对齐 mqtt.js `subscribeBatchSize` 的切块思路）。
pub(crate) const CHUNK_SIZE: usize = 50;

/// 同时未确认的批次数上限。
///
/// rumqttc 的 `next_pkid()` 在 `max_inflight`（默认 100）处回绕且不统计未确认的
/// SUBSCRIBE，限制在途批次可避免 pkid 复用导致 SUBACK 映射到错误批次。
pub(crate) const MAX_OUTSTANDING: usize = 16;

/// 单批 SUBACK 等待上限；超时视为失败并自动关闭对应订阅。
pub(crate) const ACK_TIMEOUT: Duration = Duration::from_secs(10);

struct PendingBatch {
    topics: Vec<String>,
    deadline: Instant,
    /// 超时后保留条目（不删除），用于吞掉迟到的 SUBACK，避免命中同 pkid 的新批次。
    timed_out: bool,
}

/// 订阅批次追踪器（v3.1.1 / v5 共用）。
#[derive(Default)]
pub(crate) struct SubscriptionTracker {
    /// 已登记、等待网络层写出（Outgoing 事件）的批次；FIFO 与请求顺序一致
    unbound: VecDeque<Vec<String>>,
    /// 已写出、等待 SUBACK 的批次
    inflight: HashMap<u16, PendingBatch>,
}

impl SubscriptionTracker {
    /// 对齐 mqtt.js：`client.subscribe(topics)` 入队，等待 Outgoing 事件绑定 pkid。
    pub(crate) fn queue(&mut self, topics: Vec<String>) {
        if topics.is_empty() {
            return;
        }
        self.unbound.push_back(topics);
    }

    /// 同步下发失败时回滚刚登记的批次，避免后续批次整体错位一位。
    pub(crate) fn rollback_last(&mut self) {
        self.unbound.pop_back();
    }

    /// 网络层看到 `Outgoing::Subscribe(pkid)` 时绑定最早登记的批次。
    ///
    /// 返回 `false` 表示没有待绑定批次（例如重连后残留请求的迟到 Outgoing），调用方忽略即可。
    pub(crate) fn bind(&mut self, pkid: u16) -> bool {
        match self.unbound.pop_front() {
            Some(topics) => {
                self.inflight.insert(
                    pkid,
                    PendingBatch {
                        topics,
                        deadline: Instant::now() + ACK_TIMEOUT,
                        timed_out: false,
                    },
                );
                true
            }
            None => false,
        }
    }

    /// SUBACK 到达：按 pkid 还原为逐 topic 结果。
    ///
    /// - 未知 pkid（重连残留 / 未登记）→ `None`，调用方忽略；
    /// - 已超时批次 → `None`（吞掉迟到的 ack）；
    /// - `codes.len() != topics.len()` → 整批标记失败（broker 协议错误，对齐 mqtt.js）。
    pub(crate) fn resolve(
        &mut self,
        pkid: u16,
        codes: &[Option<String>],
    ) -> Option<Vec<MqttSubAckItem>> {
        let batch = self.inflight.remove(&pkid)?;
        if batch.timed_out {
            return None;
        }
        let items = if codes.len() != batch.topics.len() {
            // 协议错误：数量对不上，整批失败；文案自带 topic，便于直接用于通知
            batch
                .topics
                .into_iter()
                .map(|topic| MqttSubAckItem {
                    reason: Some(
                        t!("mqtt_net.suback_code_mismatch", topic => topic.clone()).to_string(),
                    ),
                    topic,
                    ok: false,
                })
                .collect()
        } else {
            batch
                .topics
                .into_iter()
                .zip(codes.iter())
                .map(|(topic, code)| match code {
                    None => MqttSubAckItem {
                        topic,
                        ok: true,
                        reason: None,
                    },
                    Some(reason) => MqttSubAckItem {
                        topic,
                        ok: false,
                        reason: Some(reason.clone()),
                    },
                })
                .collect()
        };
        Some(items)
    }

    /// 1s tick：返回所有刚超时的批次，并保留条目（`timed_out = true`）以吞掉迟到 ack。
    pub(crate) fn expire(&mut self, now: Instant) -> Vec<Vec<MqttSubAckItem>> {
        let mut expired = Vec::new();
        for batch in self.inflight.values_mut() {
            if !batch.timed_out && now >= batch.deadline {
                batch.timed_out = true;
                expired.push(
                    batch
                        .topics
                        .iter()
                        .map(|topic| MqttSubAckItem {
                            reason: Some(
                                t!("mqtt_net.suback_timeout", topic => topic.clone()).to_string(),
                            ),
                            topic: topic.clone(),
                            ok: false,
                        })
                        .collect(),
                );
            }
        }
        expired
    }

    /// 断线 / 重连：清空全部状态，避免旧 pkid 被新会话的 SUBACK 命中。
    pub(crate) fn reset(&mut self) {
        self.unbound.clear();
        self.inflight.clear();
    }

    /// 背压：未确认批次（含尚未写出）是否已达上限。
    pub(crate) fn is_full(&self) -> bool {
        self.unbound.len() + self.inflight.len() >= MAX_OUTSTANDING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(pkid: u16) -> Option<Vec<MqttSubAckItem>> {
        let mut tracker = SubscriptionTracker::default();
        tracker.queue(vec!["a".into(), "b".into()]);
        assert!(tracker.bind(pkid));
        tracker.resolve(pkid, &[None, None])
    }

    #[test]
    fn queue_bind_resolve_maps_topics_in_order() {
        let items = ok(7).expect("已绑定批次应可解析");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].topic, "a");
        assert!(items[0].ok);
        assert_eq!(items[1].topic, "b");
        assert!(items[1].ok);
    }

    #[test]
    fn bind_is_fifo_and_supports_out_of_order_acks() {
        let mut tracker = SubscriptionTracker::default();
        tracker.queue(vec!["first".into()]);
        tracker.queue(vec!["second".into()]);
        assert!(tracker.bind(1));
        assert!(tracker.bind(2));

        // 乱序回包：先解 pkid=2，仍应映射到 second
        let second = tracker.resolve(2, &[None]).expect("pkid=2 应可解析");
        assert_eq!(second[0].topic, "second");
        let first = tracker.resolve(1, &[None]).expect("pkid=1 应可解析");
        assert_eq!(first[0].topic, "first");
    }

    #[test]
    fn unknown_or_already_resolved_pkid_returns_none() {
        let mut tracker = SubscriptionTracker::default();
        assert!(tracker.resolve(999, &[None]).is_none(), "未知 pkid 应忽略");
        assert!(ok(1).is_some());
        // 第二次解析同一 pkid：条目已消费
        let mut tracker2 = SubscriptionTracker::default();
        tracker2.queue(vec!["a".into()]);
        tracker2.bind(3);
        assert!(tracker2.resolve(3, &[None]).is_some());
        assert!(tracker2.resolve(3, &[None]).is_none());
    }

    #[test]
    fn code_count_mismatch_marks_whole_batch_failed() {
        let mut tracker = SubscriptionTracker::default();
        tracker.queue(vec!["a".into(), "b".into()]);
        tracker.bind(5);
        let items = tracker
            .resolve(5, &[None])
            .expect("数量不匹配仍应返回整批失败");
        assert_eq!(items.len(), 2, "两个 topic 都要有结果");
        assert!(items.iter().all(|item| !item.ok));
        let expected_a = t!("mqtt_net.suback_code_mismatch", topic => "a").to_string();
        let expected_b = t!("mqtt_net.suback_code_mismatch", topic => "b").to_string();
        assert_eq!(items[0].reason.as_deref(), Some(expected_a.as_str()));
        assert_eq!(items[1].reason.as_deref(), Some(expected_b.as_str()));
    }

    #[test]
    fn failure_code_only_marks_that_topic() {
        let mut tracker = SubscriptionTracker::default();
        tracker.queue(vec!["a".into(), "b".into()]);
        tracker.bind(9);
        let items = tracker
            .resolve(9, &[None, Some("未授权".into())])
            .expect("应可解析");
        assert!(items[0].ok);
        assert_eq!(items[0].reason, None);
        assert!(!items[1].ok);
        assert_eq!(items[1].reason.as_deref(), Some("未授权"));
    }

    #[test]
    fn rollback_last_removes_failed_queued_batch() {
        let mut tracker = SubscriptionTracker::default();
        tracker.queue(vec!["ok".into()]);
        tracker.queue(vec!["failed".into()]);
        tracker.rollback_last();
        assert!(tracker.bind(1));
        // 只剩第一批，绑定第二个 pkid 应失败
        assert!(!tracker.bind(2));
    }

    #[test]
    fn expired_batch_emits_failures_and_swallows_late_ack() {
        let mut tracker = SubscriptionTracker::default();
        tracker.queue(vec!["a".into()]);
        tracker.bind(11);

        assert!(tracker.expire(Instant::now()).is_empty(), "未到期不应超时");
        let expired = tracker.expire(Instant::now() + ACK_TIMEOUT + Duration::from_millis(1));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0][0].topic, "a");
        assert!(!expired[0][0].ok);
        assert!(expired[0][0].reason.is_some());

        // 迟到 SUBACK 必须被吞掉
        assert!(
            tracker.resolve(11, &[None]).is_none(),
            "超时批次应吞掉迟到 ack"
        );
        // 且不会重复上报超时
        assert!(tracker.expire(Instant::now() + ACK_TIMEOUT * 2).is_empty());
    }

    #[test]
    fn reset_clears_everything() {
        let mut tracker = SubscriptionTracker::default();
        tracker.queue(vec!["a".into()]);
        tracker.bind(1);
        tracker.queue(vec!["b".into()]);
        tracker.reset();
        assert!(tracker.resolve(1, &[None]).is_none());
        assert!(!tracker.bind(2));
        assert!(!tracker.is_full());
    }

    #[test]
    fn is_full_counts_unbound_and_inflight() {
        let mut tracker = SubscriptionTracker::default();
        for i in 0..MAX_OUTSTANDING {
            tracker.queue(vec![format!("t{i}")]);
        }
        assert!(tracker.is_full(), "未写出批次也计入在途上限");
        tracker.bind(1);
        assert!(tracker.is_full(), "绑定后仍在途");
        tracker.resolve(1, &[None]);
        assert!(!tracker.is_full());
    }
}
