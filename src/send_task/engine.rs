//! 发送任务引擎(仅依赖 smol channel 与 tokio, 不依赖 GPUI)。
//!
//! 关键设计: **发送节拍直连网络写通道**, 展示走主线程 16ms 泵。
//! - 每条消息 `target.try_send(bytes)` 直连网络层写通道 → 编码(含结尾追加) +
//!   `write_all` + 计数在网络层自动生效, 发送时刻由 `tokio::time::sleep` 决定,
//!   完全不经过 UI, 不受 16ms 帧节拍量化。
//! - 同时 `events.try_send(MessageReceived(Sent, 原文))` 复用既有簿记路径
//!   (消息列表 + 日志文件), 不新增事件类型。
//! - 进度经 `TaskProgress` 节流上报, 结束经 `TaskFinished`。

use super::model::{SendTaskConfig, TaskEndReason, TaskItem, TaskStatus, TaskTarget};
use crate::message::{Message, MessageDirection, MessageType};
use crate::network::events::ConnectionEvent;
use crate::utils::message_vars::RenderContext;
use log::warn;
use rust_i18n::t;
use smol::channel::Sender;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// 进度上报的最小时间间隔(节流)
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(16);
/// 进度上报的最大累积条数(节流, 防止高频任务挤占事件通道)
const PROGRESS_MAX_BATCH: u32 = 64;

/// 发送任务引擎句柄(启停包装)。
pub struct SendTaskEngine {
    /// App 可随时 set_target 刷新(重连 / 客户端变动)
    target: Arc<RwLock<TaskTarget>>,
    cancel: CancellationToken,
    pause: Arc<AtomicBool>,
    pause_reason: Arc<StdMutex<Option<String>>>,
    wake: Arc<Notify>,
    handle: Option<JoinHandle<()>>,
}

impl SendTaskEngine {
    /// 启动引擎(在 tokio runtime 上下文内调用)。
    pub fn start(
        config: Arc<SendTaskConfig>,
        tab_id: String,
        target: TaskTarget,
        events: Sender<ConnectionEvent>,
        seq: Arc<AtomicU64>,
    ) -> Self {
        let target = Arc::new(RwLock::new(target));
        let cancel = CancellationToken::new();
        let pause = Arc::new(AtomicBool::new(false));
        let pause_reason = Arc::new(StdMutex::new(None));
        let wake = Arc::new(Notify::new());
        let message_type = config.message_type();

        let runner = Runner {
            config,
            tab_id,
            target: target.clone(),
            events,
            seq,
            cancel: cancel.clone(),
            pause: pause.clone(),
            pause_reason: pause_reason.clone(),
            wake: wake.clone(),
            message_type,
            round: 1,
            sent_items: 0,
            last_report: Instant::now(),
            since_report: 0,
        };

        let handle = tokio::spawn(async move {
            let mut runner = runner;
            runner.run().await;
        });

        Self {
            target,
            cancel,
            pause,
            pause_reason,
            wake,
            handle: Some(handle),
        }
    }

    /// 刷新发送目标(重连 / 客户端连接变动时由 App 推送)。
    pub fn set_target(&self, target: TaskTarget) {
        if let Ok(mut t) = self.target.write() {
            *t = target;
        }
        // 立即唤醒: 目标刷新后不必等满一个间隔
        self.wake.notify_one();
    }

    /// 暂停(可带原因, 如「连接已断开」)。响应立即, 不延迟一个间隔。
    pub fn pause(&self, reason: Option<String>) {
        if let Ok(mut r) = self.pause_reason.lock() {
            *r = reason;
        }
        self.pause.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }

    /// 继续运行。
    pub fn resume(&self) {
        if let Ok(mut r) = self.pause_reason.lock() {
            *r = None;
        }
        self.pause.store(false, Ordering::Relaxed);
        self.wake.notify_one();
    }

    /// 停止(协作取消; runner 退出时会上报 TaskFinished(Stopped))。
    pub fn stop(&mut self) {
        self.cancel.cancel();
        self.wake.notify_one();
        // 不立即 abort: 让 runner 正常退出并回传结束事件; JoinHandle 保留兜底
        if let Some(handle) = self.handle.take() {
            drop(handle);
        }
    }
}

impl Drop for SendTaskEngine {
    /// 句柄被丢弃(如关闭 tab)时兜底取消后台任务, 避免 runner 泄漏。
    fn drop(&mut self) {
        self.cancel.cancel();
        self.wake.notify_one();
    }
}

/// 运行期状态机(独占于后台 tokio task)。
struct Runner {
    config: Arc<SendTaskConfig>,
    tab_id: String,
    target: Arc<RwLock<TaskTarget>>,
    events: Sender<ConnectionEvent>,
    seq: Arc<AtomicU64>,
    cancel: CancellationToken,
    pause: Arc<AtomicBool>,
    pause_reason: Arc<StdMutex<Option<String>>>,
    wake: Arc<Notify>,
    message_type: MessageType,
    round: u32,
    sent_items: u64,
    last_report: Instant,
    since_report: u32,
}

impl Runner {
    async fn run(&mut self) {
        let config = self.config.clone();
        let items = config.items();
        if items.is_empty() {
            self.send_finished(TaskEndReason::Completed);
            return;
        }
        let interval = Duration::from_millis(config.interval_ms);

        self.send_progress(TaskStatus::Running);
        // 绝对基准: 首条等待一个间隔, 之后每条 deadline 累加, 无漂移
        let mut deadline = Instant::now() + interval;

        loop {
            if self.cancel.is_cancelled() {
                self.send_finished(TaskEndReason::Stopped);
                return;
            }

            // 用下标循环而非 for: 投递失败的条目在恢复后重试同一条, 不丢数据
            let mut idx = 0;
            while idx < items.len() {
                if !self.wait_turn(&mut deadline, interval).await {
                    self.send_finished(TaskEndReason::Stopped);
                    return;
                }

                let bytes = self.render_item(&items[idx]);
                if self.dispatch(&bytes) {
                    // 客户端写通道断开: 暂停并保留任务(不推进 idx, 恢复后重试本条)
                    self.set_paused_disconnected();
                    continue;
                }
                self.sent_items += 1;
                self.since_report += 1;
                self.maybe_report();
                deadline += interval;
                idx += 1;
            }

            if !config.loop_enabled {
                self.send_progress(TaskStatus::Finished);
                self.send_finished(TaskEndReason::Completed);
                return;
            }
            if let Some(max) = config.max_rounds {
                if self.round >= max {
                    self.send_progress(TaskStatus::Finished);
                    self.send_finished(TaskEndReason::Completed);
                    return;
                }
            }
            self.round += 1;
            self.sent_items = 0;
            self.since_report = 0;
            self.send_progress(TaskStatus::Running);
        }
    }

    /// 等待到可以发送下一条: 处理暂停(上报状态)与取消。
    ///
    /// 返回 `false` 表示被取消, `true` 表示可以发送。
    /// 暂停期间不推进 deadline; 恢复时重置节拍基准, 避免暂停累积的欠账瞬间连发。
    async fn wait_turn(&mut self, deadline: &mut Instant, interval: Duration) -> bool {
        let mut paused_reported = false;
        loop {
            if self.cancel.is_cancelled() {
                return false;
            }
            if self.pause.load(Ordering::Relaxed) {
                if !paused_reported {
                    paused_reported = true;
                    self.send_progress(TaskStatus::Paused);
                }
                tokio::select! {
                    _ = self.cancel.cancelled() => return false,
                    _ = self.wake.notified() => {}
                }
                continue;
            }
            if paused_reported {
                paused_reported = false;
                *deadline = Instant::now() + interval;
                self.last_report = Instant::now();
                self.since_report = 0;
                self.send_progress(TaskStatus::Running);
            }
            if Instant::now() >= *deadline {
                return true;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(*deadline) => {}
                _ = self.cancel.cancelled() => return false,
                _ = self.wake.notified() => {}
            }
        }
    }

    /// 渲染一条任务项为线上字节(hex 模式先渲染文本再解码)。
    fn render_item(&self, item: &TaskItem) -> Vec<u8> {
        let seq_value = if item.compiled.needs_seq() {
            Some(self.seq.fetch_add(1, Ordering::Relaxed))
        } else {
            None
        };
        let ctx = RenderContext::common(seq_value);
        let mut out = String::with_capacity(item.compiled.template_len() + 32);
        item.compiled.render(&ctx, self.config.hex_mode, &mut out);
        if self.config.hex_mode {
            crate::utils::hex::hex_to_bytes(&out)
        } else {
            out.into_bytes()
        }
    }

    /// 投递一条消息: ① 直连写通道 ② 回传 Sent 簿记事件。
    ///
    /// 返回 `true` 表示目标已不可用(应暂停并保留任务):
    /// - 客户端: 写通道 `try_send` 失败(连接已断开)
    /// - 服务端: 客户端列表为空(App 推「空目标」表示已无可用客户端)
    ///
    /// 服务端非空列表中个别客户端发送失败只记警告, 不影响整批(由既有
    /// `ServerClientDisconnected` 事件在下一次快照刷新时剔除)。
    fn dispatch(&self, bytes: &[u8]) -> bool {
        let (delivered, disconnected) = {
            let target = match self.target.read() {
                Ok(t) => t,
                Err(_) => return false,
            };
            match &*target {
                TaskTarget::Client(tx) => match tx.try_send(bytes.to_vec()) {
                    Ok(()) => (true, false),
                    Err(_) => (false, true),
                },
                TaskTarget::ServerClients(clients) => {
                    if clients.is_empty() {
                        // 空目标 = 服务端暂无可用客户端, 与「未连接」同义, 暂停而非空转推进进度
                        (false, true)
                    } else {
                        let mut delivered = false;
                        for (addr, tx) in clients {
                            if tx.try_send(bytes.to_vec()).is_err() {
                                warn!("[发送任务] 发送到客户端 {} 失败(客户端可能已断开)", addr);
                            } else {
                                delivered = true;
                            }
                        }
                        (delivered, false)
                    }
                }
            }
        };

        if delivered {
            let message = Message::new(MessageDirection::Sent, bytes.to_vec(), self.message_type);
            let _ = self.events.try_send(ConnectionEvent::MessageReceived(
                self.tab_id.clone(),
                message,
            ));
        }
        disconnected
    }

    fn set_paused_disconnected(&self) {
        if let Ok(mut r) = self.pause_reason.lock() {
            *r = Some(t!("send_task.reason_disconnected").to_string());
        }
        self.pause.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }

    fn maybe_report(&mut self) {
        if self.since_report >= PROGRESS_MAX_BATCH
            || self.last_report.elapsed() >= PROGRESS_MIN_INTERVAL
        {
            self.send_progress(TaskStatus::Running);
            self.last_report = Instant::now();
            self.since_report = 0;
        }
    }

    fn send_progress(&self, status: TaskStatus) {
        let reason = self.pause_reason.lock().ok().and_then(|r| r.clone());
        let _ = self.events.try_send(ConnectionEvent::TaskProgress {
            tab_id: self.tab_id.clone(),
            task_id: self.config.id.clone(),
            sent_items: self.sent_items,
            total_items: self.config.total_items() as u64,
            round: self.round,
            status,
            pause_reason: reason,
        });
    }

    fn send_finished(&self, reason: TaskEndReason) {
        let _ = self.events.try_send(ConnectionEvent::TaskFinished {
            tab_id: self.tab_id.clone(),
            task_id: self.config.id.clone(),
            reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::send_task::model::TaskKind;
    use crate::send_task::parse::parse_lines;
    use std::sync::atomic::AtomicU64;
    use tokio::time::timeout;

    fn config(
        text: &str,
        interval_ms: u64,
        loop_enabled: bool,
        max_rounds: Option<u32>,
    ) -> Arc<SendTaskConfig> {
        Arc::new(SendTaskConfig {
            id: "task-1".to_string(),
            name: "f.txt".to_string(),
            kind: TaskKind::SendByLines {
                items: parse_lines(text, false).unwrap(),
            },
            interval_ms,
            loop_enabled,
            max_rounds,
            hex_mode: false,
            start_immediately: true,
        })
    }

    /// 逐行发送: 三条消息按序发出, 结束后收到 TaskFinished(Completed)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_sends_all_items_and_finishes() {
        let (event_tx, event_rx) = smol::channel::unbounded::<ConnectionEvent>();
        let (write_tx, write_rx) = smol::channel::unbounded::<Vec<u8>>();

        let _engine = SendTaskEngine::start(
            config("a\nb\nc", 1, false, None),
            "tab".to_string(),
            TaskTarget::Client(write_tx),
            event_tx,
            Arc::new(AtomicU64::new(0)),
        );

        let mut got = Vec::new();
        let mut finished = false;
        let deadline = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => break,
                r = write_rx.recv() => {
                    if let Ok(b) = r { got.push(b); }
                }
                r = event_rx.recv() => {
                    if let Ok(ConnectionEvent::TaskFinished { reason, .. }) = r {
                        assert_eq!(reason, TaskEndReason::Completed);
                        finished = true;
                        break;
                    }
                }
            }
        }
        assert_eq!(got, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
        assert!(finished, "应收到 TaskFinished(Completed)");
    }

    /// 循环 + 轮次上限: 3 条 × 2 轮 = 6 次发送
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_loop_with_max_rounds() {
        let (event_tx, event_rx) = smol::channel::unbounded::<ConnectionEvent>();
        let (write_tx, write_rx) = smol::channel::unbounded::<Vec<u8>>();

        let _engine = SendTaskEngine::start(
            config("a\nb\nc", 1, true, Some(2)),
            "tab".to_string(),
            TaskTarget::Client(write_tx),
            event_tx,
            Arc::new(AtomicU64::new(0)),
        );

        let mut count = 0usize;
        let mut finished = false;
        let deadline = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => break,
                r = write_rx.recv() => { if r.is_ok() { count += 1; } }
                r = event_rx.recv() => {
                    if let Ok(ConnectionEvent::TaskFinished { reason, .. }) = r {
                        assert_eq!(reason, TaskEndReason::Completed);
                        finished = true;
                        break;
                    }
                }
            }
        }
        assert!(finished, "循环 2 轮后应结束");
        assert_eq!(count, 6, "3 条 × 2 轮");
    }

    /// 暂停期间不再发送; 继续后恢复
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_pause_and_resume() {
        let (event_tx, event_rx) = smol::channel::unbounded::<ConnectionEvent>();
        let (write_tx, write_rx) = smol::channel::unbounded::<Vec<u8>>();

        let engine = SendTaskEngine::start(
            config("a\nb\nc\nd\ne", 10, false, None),
            "tab".to_string(),
            TaskTarget::Client(write_tx),
            event_tx,
            Arc::new(AtomicU64::new(0)),
        );

        // 等第一条
        let first = timeout(Duration::from_secs(2), write_rx.recv())
            .await
            .expect("应收到第一条")
            .unwrap();
        assert_eq!(first, b"a".to_vec());

        // 暂停并排空在途
        engine.pause(None);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut drained = 0;
        while write_rx.try_recv().is_ok() {
            drained += 1;
        }
        let _ = drained;

        // 暂停期间应稳定不再增长
        tokio::time::sleep(Duration::from_millis(80)).await;
        let mut extra = 0;
        while write_rx.try_recv().is_ok() {
            extra += 1;
        }
        assert_eq!(extra, 0, "暂停期间不应再发送");

        // 继续后能跑完
        engine.resume();
        let mut finished = false;
        let deadline = tokio::time::sleep(Duration::from_secs(3));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => break,
                r = write_rx.recv() => { let _ = r; }
                r = event_rx.recv() => {
                    if let Ok(ConnectionEvent::TaskFinished { reason, .. }) = r {
                        assert_eq!(reason, TaskEndReason::Completed);
                        finished = true;
                        break;
                    }
                }
            }
        }
        assert!(finished, "继续后应跑完");
    }

    /// 停止: 单条任务停止后应尽快收到 TaskFinished(Stopped)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_stop_emits_stopped() {
        let (event_tx, event_rx) = smol::channel::unbounded::<ConnectionEvent>();
        let (write_tx, _write_rx) = smol::channel::unbounded::<Vec<u8>>();

        let mut engine = SendTaskEngine::start(
            config("a\nb\nc", 1000, true, None),
            "tab".to_string(),
            TaskTarget::Client(write_tx),
            event_tx,
            Arc::new(AtomicU64::new(0)),
        );
        engine.stop();

        let deadline = tokio::time::sleep(Duration::from_secs(3));
        tokio::pin!(deadline);
        let mut stopped = false;
        loop {
            tokio::select! {
                _ = &mut deadline => break,
                r = event_rx.recv() => {
                    if let Ok(ConnectionEvent::TaskFinished { reason, .. }) = r {
                        assert_eq!(reason, TaskEndReason::Stopped);
                        stopped = true;
                        break;
                    }
                }
            }
        }
        assert!(stopped, "停止后应收到 TaskFinished(Stopped)");
    }

    /// 写通道断开: 客户端行任务应暂停而非静默失败
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_disconnect_pauses() {
        let (event_tx, event_rx) = smol::channel::unbounded::<ConnectionEvent>();
        let (write_tx, write_rx) = smol::channel::unbounded::<Vec<u8>>();

        let _engine = SendTaskEngine::start(
            config("a\nb\nc", 1, false, None),
            "tab".to_string(),
            TaskTarget::Client(write_tx),
            event_tx,
            Arc::new(AtomicU64::new(0)),
        );

        // 关闭接收端 → 写通道 try_send 失败
        drop(write_rx);

        let deadline = tokio::time::sleep(Duration::from_secs(3));
        tokio::pin!(deadline);
        let mut paused = false;
        loop {
            tokio::select! {
                _ = &mut deadline => break,
                r = event_rx.recv() => {
                    if let Ok(ConnectionEvent::TaskProgress { status: TaskStatus::Paused, .. }) = r {
                        paused = true;
                        break;
                    }
                }
            }
        }
        assert!(paused, "写通道断开后应上报 Paused");
    }

    /// 空的服务端目标(无可用客户端): 应暂停而非静默空转推进进度
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_empty_server_target_pauses() {
        let (event_tx, event_rx) = smol::channel::unbounded::<ConnectionEvent>();
        let (write_tx, mut write_rx) = smol::channel::unbounded::<Vec<u8>>();

        let _engine = SendTaskEngine::start(
            config("a\nb\nc", 1, false, None),
            "tab".to_string(),
            TaskTarget::ServerClients(Vec::new()),
            event_tx,
            Arc::new(AtomicU64::new(0)),
        );

        let deadline = tokio::time::sleep(Duration::from_secs(3));
        tokio::pin!(deadline);
        let mut paused = false;
        loop {
            tokio::select! {
                _ = &mut deadline => break,
                r = write_rx.recv() => {
                    if r.is_ok() {
                        panic!("空目标不应真的发出数据");
                    }
                }
                r = event_rx.recv() => {
                    if let Ok(ConnectionEvent::TaskProgress { status: TaskStatus::Paused, .. }) = r {
                        paused = true;
                        break;
                    }
                }
            }
        }
        assert!(paused, "空目标应上报 Paused 并保留任务");
        // 暂停期间不应有任何投递, 且 write_tx 仍可用(通道未被关闭)
        assert!(write_rx.try_recv().is_err(), "暂停后不应再投递");
        let _ = write_tx;
    }
}
