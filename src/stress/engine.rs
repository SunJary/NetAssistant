// 压测引擎编排
//
// 职责:
//   - 创建共享状态(全局序号/令牌桶/原子统计/直方图)
//   - 按 ramp-up 间隔 spawn N 个 worker
//   - 250ms 聚合 task 生成 StatsSnapshot 推送 UI
//   - 监督 task 按停止条件(限时/定量)触发取消
//   - 结束时汇总最终报告 StressReport
//
// 引擎不依赖 GPUI，仅通过 smol channel::Sender<StressEvent> 与外界通信。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Local;
use log::info;
use smol::channel::Sender;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::stress::client_worker::run_worker;
use crate::stress::config::{StopCondition, StressTestConfig};
use crate::stress::events::{StressEvent, StressReport};
use crate::stress::rate_limiter::TokenBucket;
use crate::stress::stats::{LatencyHistogram, StressStats, WorkerStats};

/// 统计快照上报间隔
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(250);

/// 压测引擎
pub struct StressTestEngine {
    cancel: CancellationToken,
    orchestrator: Option<JoinHandle<()>>,
}

impl StressTestEngine {
    /// 启动压测。event_sender 由 App 层提供(smol channel)。
    /// `tab_id` 用于事件路由(App 层按 tab_id 投递, 不依赖 active_tab)。
    pub fn start(config: StressTestConfig, tab_id: String, event_sender: Sender<StressEvent>) -> Self {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let orchestrator = tokio::spawn(async move {
            Self::run(config, tab_id, event_sender, cancel_clone).await;
        });
        Self {
            cancel,
            orchestrator: Some(orchestrator),
        }
    }

    /// 停止: 先协作取消(让 worker 清理), 再 abort 兜底
    pub fn stop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.orchestrator.take() {
            h.abort();
        }
    }

    /// 是否仍在运行
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        match &self.orchestrator {
            Some(h) => !h.is_finished(),
            None => false,
        }
    }

    async fn run(config: StressTestConfig, tab_id: String, event_sender: Sender<StressEvent>, cancel: CancellationToken) {
        let start_time_str = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let start_instant = Instant::now();

        // 共享状态
        let global_seq = Arc::new(AtomicU64::new(0));
        let limiter = match config.global_qps_limit {
            Some(qps) => Arc::new(TokenBucket::new(qps)),
            None => Arc::new(TokenBucket::unbounded()),
        };
        let stats = Arc::new(WorkerStats::new());
        let histogram = Arc::new(Mutex::new(LatencyHistogram::with_default_cap()));
        let per_second_samples: Arc<Mutex<Vec<StressStats>>> =
            Arc::new(Mutex::new(Vec::new()));

        // ---- 启动 worker (按 ramp-up 间隔) ----
        let concurrency = config.concurrency.max(1);
        let ramp_interval = if config.ramp_up.enabled && config.ramp_up.ramp_up_secs > 0 && concurrency > 1 {
            Duration::from_secs_f64(config.ramp_up.ramp_up_secs as f64 / concurrency as f64)
        } else {
            Duration::ZERO
        };

        let mut worker_handles: Vec<JoinHandle<()>> = Vec::with_capacity(concurrency);
        for i in 0..concurrency {
            // ramp-up 间隔(可被取消)
            if i > 0 && ramp_interval > Duration::ZERO {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = sleep(ramp_interval) => {}
                }
            }
            let cfg = config.clone();
            let seq = global_seq.clone();
            let lim = limiter.clone();
            let st = stats.clone();
            let hg = histogram.clone();
            let c = cancel.clone();
            let es = event_sender.clone();
            worker_handles.push(tokio::spawn(async move {
                run_worker(i, cfg, seq, lim, st, hg, c, es).await;
            }));
        }

        // ---- 聚合 task (250ms 快照) ----
        let agg_cancel = cancel.clone();
        let agg_stats = stats.clone();
        let agg_hist = histogram.clone();
        let agg_samples = per_second_samples.clone();
        let agg_sender = event_sender.clone();
        let agg_tab_id = tab_id.clone();
        let aggregator = tokio::spawn(async move {
            let mut last_second_mark = 0u64;
            loop {
                tokio::select! {
                    _ = agg_cancel.cancelled() => {
                        // 最终快照
                        let snapshot = build_snapshot(&agg_stats, &agg_hist, start_instant);
                        push_per_second(&agg_samples, &mut last_second_mark, &snapshot);
                        let _ = agg_sender.send(StressEvent::StatsSnapshot {
                            tab_id: agg_tab_id.clone(),
                            stats: snapshot,
                        }).await;
                        break;
                    }
                    _ = sleep(SNAPSHOT_INTERVAL) => {
                        let snapshot = build_snapshot(&agg_stats, &agg_hist, start_instant);
                        push_per_second(&agg_samples, &mut last_second_mark, &snapshot);
                        let _ = agg_sender.send(StressEvent::StatsSnapshot {
                            tab_id: agg_tab_id.clone(),
                            stats: snapshot.clone(),
                        }).await;
                    }
                }
            }
        });

        // ---- 监督 task (停止条件) ----
        let sup_cancel = cancel.clone();
        let sup_stats = stats.clone();
        let stop_condition = config.stop_condition.clone();
        let supervisor = tokio::spawn(async move {
            match stop_condition {
                StopCondition::Manual => {
                    // 仅等待外部取消
                    sup_cancel.cancelled().await;
                }
                StopCondition::Duration(secs) => {
                    tokio::select! {
                        _ = sup_cancel.cancelled() => {}
                        _ = sleep(Duration::from_secs(secs)) => {
                            sup_cancel.cancel();
                        }
                    }
                }
                StopCondition::Count(target) => {
                    loop {
                        if sup_cancel.is_cancelled() {
                            break;
                        }
                        if sup_stats.sent.load(Ordering::Relaxed) >= target {
                            sup_cancel.cancel();
                            break;
                        }
                        sleep(Duration::from_millis(50)).await;
                    }
                }
                StopCondition::Either { duration_secs, count } => {
                    loop {
                        tokio::select! {
                            _ = sup_cancel.cancelled() => break,
                            _ = sleep(Duration::from_millis(50)) => {
                                if sup_stats.sent.load(Ordering::Relaxed) >= count {
                                    sup_cancel.cancel();
                                    break;
                                }
                            }
                        }
                        if start_instant.elapsed() >= Duration::from_secs(duration_secs) {
                            sup_cancel.cancel();
                            break;
                        }
                    }
                }
            }
        });

        // ---- watchdog: 所有 worker 退出后触发 cancel ----
        // 场景: 连接断开且 auto_reconnect=false 时，worker 会自行退出。
        // 若不检测，引擎会阻塞在 supervisor.await 直到停止条件到期。
        let watchdog_cancel = cancel.clone();
        let watchdog = tokio::spawn(async move {
            for h in worker_handles {
                let _ = h.await;
            }
            // 所有 worker 已退出，触发取消让 supervisor 和 aggregator 结束
            watchdog_cancel.cancel();
        });

        // 等待 supervisor 返回(停止条件触发 或 watchdog 触发 cancel)
        let _ = supervisor.await;
        // 确保 cancel 已触发(幂等: supervisor 或 watchdog 可能已触发)
        cancel.cancel();

        // 等待 watchdog 完成(所有 worker 已退出)
        let _ = watchdog.await;
        // 等待聚合 task 完成最终快照
        let _ = aggregator.await;

        let end_time_str = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let duration_ms = start_instant.elapsed().as_millis() as u64;

        // 汇总最终报告
        let (sent, success, failure, _active, disconnects, reconnects, bytes_sent, bytes_received) =
            stats.snapshot();
        let (p50, p95, p99, avg, max) = histogram.lock().unwrap().percentiles();

        let samples = per_second_samples.lock().unwrap().clone();
        let (avg_qps, peak_qps) = compute_qps_stats(&samples, duration_ms);

        let report = StressReport {
            config,
            start_time: start_time_str,
            end_time: end_time_str,
            duration_ms,
            total_sent: sent,
            total_success: success,
            total_failure: failure,
            avg_qps,
            peak_qps,
            disconnects,
            reconnects,
            latency_p50_us: p50,
            latency_p95_us: p95,
            latency_p99_us: p99,
            latency_avg_us: avg,
            latency_max_us: max,
            bytes_sent,
            bytes_received,
            per_second_samples: samples,
        };

        info!(
            "[压测] 结束: 发送={}, 成功={}, 失败={}, 平均QPS={:.1}",
            report.total_sent, report.total_success, report.total_failure, report.avg_qps
        );
        let _ = event_sender.send(StressEvent::Finished {
            tab_id,
            report,
        }).await;
    }
}

impl Drop for StressTestEngine {
    fn drop(&mut self) {
        // 安全兜底: 确保即使未显式调用 stop()，也能触发取消让 worker 退出
        self.cancel.cancel();
        if let Some(h) = self.orchestrator.take() {
            h.abort();
        }
    }
}

/// 从共享统计构建快照
fn build_snapshot(
    stats: &WorkerStats,
    histogram: &Mutex<LatencyHistogram>,
    start: Instant,
) -> StressStats {
    let (sent, success, failure, active, disconnects, reconnects, bytes_sent, bytes_received) =
        stats.snapshot();
    let (p50, p95, p99, avg, max) = histogram
        .lock()
        .map(|mut h| h.percentiles())
        .unwrap_or((None, None, None, None, None));
    let elapsed_ms = start.elapsed().as_millis() as u64;
    // current_qps = 最近一秒的发送速率(近似: sent / elapsed_s)
    let current_qps = if elapsed_ms > 0 {
        sent as f64 / (elapsed_ms as f64 / 1000.0)
    } else {
        0.0
    };
    StressStats {
        elapsed_ms,
        current_qps,
        total_sent: sent,
        total_success: success,
        total_failure: failure,
        active_connections: active as usize,
        disconnects,
        reconnects,
        latency_p50_us: p50,
        latency_p95_us: p95,
        latency_p99_us: p99,
        latency_avg_us: avg,
        latency_max_us: max,
        bytes_sent,
        bytes_received,
    }
}

/// 每整秒记录一个累计快照(用于 CSV 时序)
fn push_per_second(
    samples: &Mutex<Vec<StressStats>>,
    last_second_mark: &mut u64,
    snapshot: &StressStats,
) {
    let current_sec = snapshot.elapsed_ms / 1000;
    if current_sec > *last_second_mark {
        *last_second_mark = current_sec;
        if let Ok(mut s) = samples.lock() {
            s.push(snapshot.clone());
        }
    }
}

/// 从每秒采样计算平均/峰值 QPS
fn compute_qps_stats(samples: &[StressStats], duration_ms: u64) -> (f64, f64) {
    if samples.is_empty() {
        let total = samples.last().map(|s| s.total_sent).unwrap_or(0);
        let avg = if duration_ms > 0 {
            total as f64 / (duration_ms as f64 / 1000.0)
        } else {
            0.0
        };
        return (avg, 0.0);
    }
    // peak = 单秒内最大增量发送数
    let mut peak: f64 = 0.0;
    let mut prev = 0u64;
    for s in samples {
        let delta = s.total_sent.saturating_sub(prev);
        peak = peak.max(delta as f64);
        prev = s.total_sent;
    }
    let total = samples.last().map(|s| s.total_sent).unwrap_or(0);
    let avg = if duration_ms > 0 {
        total as f64 / (duration_ms as f64 / 1000.0)
    } else {
        0.0
    };
    (avg, peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_qps_empty() {
        let (avg, peak) = compute_qps_stats(&[], 0);
        assert_eq!((avg, peak), (0.0, 0.0));
    }

    #[test]
    fn test_compute_qps_basic() {
        let s1 = StressStats { total_sent: 100, ..Default::default() };
        let s2 = StressStats { total_sent: 250, ..Default::default() };
        let s3 = StressStats { total_sent: 400, ..Default::default() };
        let samples = vec![s1, s2, s3];
        // 增量: 100, 150, 150 → peak=150
        // avg = 400 / 3s = 133.33
        let (avg, peak) = compute_qps_stats(&samples, 3000);
        assert!((peak - 150.0).abs() < 0.01, "peak={}", peak);
        assert!((avg - 133.33).abs() < 0.1, "avg={}", avg);
    }

    #[test]
    fn test_push_per_second_accumulates() {
        let samples: Arc<Mutex<Vec<StressStats>>> = Arc::new(Mutex::new(Vec::new()));
        let mut mark = 0u64;
        // 第 0 秒内多次快照: 不记录(尚未跨入新秒)
        push_per_second(&samples, &mut mark, &StressStats { elapsed_ms: 100, ..Default::default() });
        push_per_second(&samples, &mut mark, &StressStats { elapsed_ms: 500, ..Default::default() });
        push_per_second(&samples, &mut mark, &StressStats { elapsed_ms: 900, ..Default::default() });
        assert_eq!(samples.lock().unwrap().len(), 0);
        // 跨入第 1 秒: 记录一次
        push_per_second(&samples, &mut mark, &StressStats { elapsed_ms: 1100, ..Default::default() });
        assert_eq!(samples.lock().unwrap().len(), 1);
        // 第 1 秒内再次快照: 不重复记录
        push_per_second(&samples, &mut mark, &StressStats { elapsed_ms: 1500, ..Default::default() });
        assert_eq!(samples.lock().unwrap().len(), 1);
        // 跨入第 2 秒
        push_per_second(&samples, &mut mark, &StressStats { elapsed_ms: 2100, ..Default::default() });
        assert_eq!(samples.lock().unwrap().len(), 2);
    }

    /// 端到端集成测试: 本地 TCP echo server + 引擎 ping-pong 模式
    #[tokio::test]
    async fn test_engine_tcp_ping_pong_e2e() {
        use smol::channel::unbounded;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        use crate::stress::config::{ConnectionMode, StressMode};
        use crate::config::connection::ConnectionType;

        // 启动本地 echo server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let echo_handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut socket, _)) => {
                        tokio::spawn(async move {
                            let mut buf = [0u8; 8192];
                            loop {
                                match socket.read(&mut buf).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(n) => {
                                        if socket.write_all(&buf[..n]).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        let (sender, receiver) = unbounded::<StressEvent>();

        let config = StressTestConfig {
            target_address: "127.0.0.1".to_string(),
            target_port: port,
            protocol: ConnectionType::Tcp,
            mode: StressMode::PingPong,
            connection_mode: ConnectionMode::Long,
            concurrency: 5,
            message_input_mode: "text".to_string(),
            payload_template: "PING ${seq}".to_string(),
            send_interval_ms: 10,
            global_qps_limit: None,
            stop_condition: StopCondition::Count(50),
            ramp_up: crate::stress::config::RampUpConfig::default(),
            auto_reconnect: true,
            response_validation: None,
            timeout_ms: 2000,
        };

        let mut engine = StressTestEngine::start(config, "test".to_string(), sender);

        // 收集事件直到 Finished
        let mut got_finished = false;
        let mut snapshot_count = 0u32;
        while let Ok(event) = tokio::time::timeout(
            Duration::from_secs(10),
            receiver.recv(),
        ).await {
            match event {
                Ok(StressEvent::StatsSnapshot { .. }) => snapshot_count += 1,
                Ok(StressEvent::Finished { report, .. }) => {
                    assert!(report.total_sent >= 50, "应至少发送 50, 实际 {}", report.total_sent);
                    assert!(report.total_success > 0, "应有成功包");
                    assert!(report.latency_p50_us.is_some(), "应有延迟统计");
                    got_finished = true;
                    break;
                }
                Ok(StressEvent::Error { msg, .. }) => panic!("引擎错误: {}", msg),
                Err(_) => break, // timeout
            }
        }
        assert!(got_finished, "应收到 Finished 事件");
        assert!(snapshot_count > 0, "应收到至少一个快照");

        engine.stop();
        echo_handle.abort();
    }

    /// 端到端: UDP echo + 吞吐模式
    #[tokio::test]
    async fn test_engine_udp_throughput_e2e() {
        use smol::channel::unbounded;
        use tokio::net::UdpSocket;

        use crate::stress::config::{ConnectionMode, StressMode};
        use crate::config::connection::ConnectionType;

        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        let echo_handle = tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match server.recv_from(&mut buf).await {
                    Ok((n, addr)) => {
                        let _ = server.send_to(&buf[..n], addr).await;
                    }
                    Err(_) => break,
                }
            }
        });

        let (sender, receiver) = unbounded::<StressEvent>();

        let config = StressTestConfig {
            target_address: "127.0.0.1".to_string(),
            target_port: port,
            protocol: ConnectionType::Udp,
            mode: StressMode::Throughput,
            connection_mode: ConnectionMode::Long,
            concurrency: 3,
            message_input_mode: "text".to_string(),
            payload_template: "data${seq}".to_string(),
            send_interval_ms: 5,
            global_qps_limit: None,
            stop_condition: StopCondition::Count(30),
            ramp_up: crate::stress::config::RampUpConfig::default(),
            auto_reconnect: false,
            response_validation: None,
            timeout_ms: 2000,
        };

        let mut engine = StressTestEngine::start(config, "test".to_string(), sender);

        let mut got_finished = false;
        while let Ok(event) = tokio::time::timeout(
            Duration::from_secs(10),
            receiver.recv(),
        ).await {
            match event {
                Ok(StressEvent::Finished { report, .. }) => {
                    assert!(report.total_sent >= 30, "UDP 应至少发送 30, 实际 {}", report.total_sent);
                    assert!(report.total_success >= 30, "吞吐模式成功数应等于发送数");
                    got_finished = true;
                    break;
                }
                Ok(StressEvent::Error { msg, .. }) => panic!("引擎错误: {}", msg),
                Ok(StressEvent::StatsSnapshot { .. }) => {}
                Err(_) => break,
            }
        }
        assert!(got_finished, "应收到 Finished 事件");

        engine.stop();
        echo_handle.abort();
    }
}
