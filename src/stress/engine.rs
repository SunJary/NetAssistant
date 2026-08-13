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
use crate::stress::stats::{ShardedHistogram, StressStats, WorkerStats};

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
    pub fn start(
        config: StressTestConfig,
        tab_id: String,
        event_sender: Sender<StressEvent>,
    ) -> Self {
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

    async fn run(
        config: StressTestConfig,
        tab_id: String,
        event_sender: Sender<StressEvent>,
        cancel: CancellationToken,
    ) {
        let start_time_str = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let start_instant = Instant::now();

        // 共享状态
        let global_seq = Arc::new(AtomicU64::new(0));
        let limiter = match config.global_qps_limit {
            Some(qps) => Arc::new(TokenBucket::new(qps)),
            None => Arc::new(TokenBucket::unbounded()),
        };
        let stats = Arc::new(WorkerStats::new());
        let concurrency = config.concurrency.max(1);
        let histogram = Arc::new(ShardedHistogram::new(concurrency));
        let per_second_samples: Arc<Mutex<Vec<StressStats>>> = Arc::new(Mutex::new(Vec::new()));

        // 注: 端口范围检测与警告在压测配置弹窗 (stress_config.rs render_port_warning) 完成,
        // 引擎不再用信号量限流 —— 超出系统临时端口范围的连接会由 OS 直接报错
        // (AddrNotAvailable / WSAEADDRINUSE), 计入 connect_failed, 由 diagnose_failures 给出建议。

        // ---- 聚合 task (250ms 快照) ----
        // 注意: 必须在 worker spawn 循环之前启动, 否则 ramp-up 期间 UI 收不到任何快照,
        // 用户会看到"前 N 秒完全无反应"。
        let agg_cancel = cancel.clone();
        let agg_stats = stats.clone();
        let agg_hist = histogram.clone();
        let agg_samples = per_second_samples.clone();
        let agg_sender = event_sender.clone();
        let agg_tab_id = tab_id.clone();
        let aggregator = tokio::spawn(async move {
            let mut last_second_mark = 0u64;
            let mut last_total_sent: u64 = 0;
            let mut last_snapshot_at = start_instant;
            loop {
                let is_final = tokio::select! {
                    _ = agg_cancel.cancelled() => true,
                    _ = sleep(SNAPSHOT_INTERVAL) => false,
                };
                let snapshot = build_snapshot(&agg_stats, &agg_hist, start_instant);
                push_per_second(&agg_samples, &mut last_second_mark, &snapshot);
                // 更新峰值 QPS: 基于相邻快照的 delta_sent / dt(瞬时发送速率)
                let now = Instant::now();
                let dt = now.duration_since(last_snapshot_at).as_secs_f64();
                if dt > 0.0 {
                    let delta = snapshot.total_sent.saturating_sub(last_total_sent);
                    let inst_qps = delta as f64 / dt;
                    agg_stats
                        .peak_qps_milli
                        .fetch_max((inst_qps * 1000.0) as u64, Ordering::Relaxed);
                }
                last_total_sent = snapshot.total_sent;
                last_snapshot_at = now;
                let _ = agg_sender
                    .send(StressEvent::StatsSnapshot {
                        tab_id: agg_tab_id.clone(),
                        stats: snapshot,
                    })
                    .await;
                if is_final {
                    break;
                }
            }
        });

        // ---- 监督 task (停止条件) ----
        // 注意: 必须在 worker spawn 循环之前启动, 这样 Duration 计时从 t=0 起算,
        // 包含 ramp-up 时间, 总墙钟时长 = duration (而非 ramp_up + duration)。
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
                StopCondition::Count(target) => loop {
                    if sup_cancel.is_cancelled() {
                        break;
                    }
                    if sup_stats.sent.load(Ordering::Relaxed) >= target {
                        sup_cancel.cancel();
                        break;
                    }
                    sleep(Duration::from_millis(50)).await;
                },
                StopCondition::Either {
                    duration_secs,
                    count,
                } => loop {
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
                },
            }
        });

        // ---- 启动 worker (瞬间全 spawn, ramp-up 由连接速率令牌桶控制) ----
        // ramp-up 不再用逐 worker sleep 间隔(高并发下 ramp_up_secs/concurrency 低于
        // 定时器精度且只控 spawn 时机)。改为在 worker 首次 connect() 前获取令牌,
        // 令牌按 concurrency/ramp_up_secs 速率补充, 线性控制 active 增长曲线。
        let connect_limiter = if config.ramp_up.enabled && config.ramp_up.ramp_up_secs > 0 {
            // 建连速率 = 并发数 / ramp_up_secs (conn/s)
            let conn_rate = concurrency as f64 / config.ramp_up.ramp_up_secs as f64;
            // 初始放行一批令牌避免冷启动: 首批 worker 立即建连, 后续按速率补充
            let initial_batch = (concurrency as f64 / 10.0).max(1.0);
            Arc::new(TokenBucket::with_initial_tokens(
                conn_rate as u32,
                initial_batch,
            ))
        } else {
            Arc::new(TokenBucket::unbounded())
        };

        let mut worker_handles: Vec<JoinHandle<()>> = Vec::with_capacity(concurrency);
        for i in 0..concurrency {
            let cfg = config.clone();
            let seq = global_seq.clone();
            let lim = limiter.clone();
            let st = stats.clone();
            let hg = histogram.clone();
            let c = cancel.clone();
            let es = event_sender.clone();
            let cl = connect_limiter.clone();
            worker_handles.push(tokio::spawn(async move {
                run_worker(i, cfg, seq, lim, st, hg, c, es, cl).await;
            }));
        }

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
        let (p50, p95, p99, avg, max) = histogram.percentiles();
        let failure_breakdown =
            crate::stress::stats::FailureBreakdownSnapshot::from(&stats.failures);

        let samples = per_second_samples.lock().unwrap().clone();
        let (avg_qps, peak_qps) = compute_qps_stats(&samples, duration_ms);

        // 将失败分类写入单独日志文件(不污染主日志)
        let workers_gave_up = stats
            .workers_gave_up
            .load(std::sync::atomic::Ordering::Relaxed);
        write_failure_log(
            &start_time_str,
            &end_time_str,
            duration_ms,
            sent,
            success,
            failure,
            &failure_breakdown,
            stats.last_connect_error(),
            workers_gave_up,
            &config,
        );

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
            failures: failure_breakdown,
            per_second_samples: samples,
        };

        info!(
            "[压测] 结束: 发送={}, 成功={}, 失败={}, 平均QPS={:.1}",
            report.total_sent, report.total_success, report.total_failure, report.avg_qps
        );
        let _ = event_sender
            .send(StressEvent::Finished { tab_id, report })
            .await;
    }
}

/// 将失败分类汇总写入单独日志文件
///
/// 路径: {documents}/NetAssistant/logs/stress_failure_{timestamp}.log
/// 内容: 失败分类统计 + 配置摘要,便于定位高失败率根因。
/// 一次性写入,不影响压测性能。
fn write_failure_log(
    start_time: &str,
    end_time: &str,
    duration_ms: u64,
    sent: u64,
    success: u64,
    failure: u64,
    fb: &crate::stress::stats::FailureBreakdownSnapshot,
    last_connect_error: String,
    workers_gave_up: u64,
    config: &StressTestConfig,
) {
    use std::io::Write;

    let mut dir = match dirs::document_dir() {
        Some(d) => d,
        None => return,
    };
    dir.push("NetAssistant");
    dir.push("logs");
    let _ = std::fs::create_dir_all(&dir);

    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("stress_failure_{}.log", ts);
    dir.push(filename);

    // 失败率: 以 (sent + connect_failed) 为分母,因为连接失败不计入 sent
    // 避免出现 >100% 的误导性失败率
    let total_attempts = sent + fb.connect_failed;
    let failure_rate = if total_attempts > 0 {
        failure as f64 / total_attempts as f64 * 100.0
    } else {
        0.0
    };
    let is_long_conn = config.is_long_connection();

    let content = format!(
        "===== 压测失败分析报告 =====\n\
         开始时间: {start_time}\n\
         结束时间: {end_time}\n\
         持续时长: {duration_ms} ms ({:.1}s)\n\
         \n\
         ===== 总览 =====\n\
         总发送(发包): {sent}\n\
         成功:   {success}\n\
         失败:   {failure}\n\
         失败率: {failure_rate:.1}%  (分母 = 发包{sent} + 连接尝试{cf})\n\
         因重试上限退出: {gave_up} / {concurrency}  (连续连接失败 > 30 次后 worker 放弃)\n\
         \n\
         ===== 失败分类 =====\n\
         连接失败(connect_failed): {cf}\n\
         发送失败(send_failed):    {sf}\n\
         接收超时(recv_timeout):   {rt}\n\
         对端关闭(peer_closed):    {pc}\n\
         校验失败(validate_failed): {vf}\n\
         \n\
         ===== 最近连接错误(采样) =====\n\
         {last_err}\n\
         \n\
         ===== 压测配置 =====\n\
         目标: {addr}:{port} ({proto:?})\n\
         模式: {mode} / {conn_mode}\n\
         并发: {concurrency}\n\
         发包间隔: {interval} ms\n\
         全局QPS限制: {qps_limit}\n\
         超时: {timeout_ms} ms\n\
         自动重连: {auto_reconnect}\n\
         \n\
         ===== 诊断建议 =====\n\
         {advice}\n",
        duration_ms as f64 / 1000.0,
        cf = fb.connect_failed,
        sf = fb.send_failed,
        rt = fb.recv_timeout,
        pc = fb.peer_closed,
        vf = fb.validate_failed,
        gave_up = workers_gave_up,
        last_err = if last_connect_error.is_empty() {
            "(无)"
        } else {
            &last_connect_error
        },
        addr = config.target_address,
        port = config.target_port,
        proto = config.protocol,
        mode = config.mode,
        conn_mode = config.connection_mode,
        concurrency = config.concurrency,
        interval = config.send_interval_ms,
        qps_limit = config
            .global_qps_limit
            .map(|q| q.to_string())
            .unwrap_or_else(|| "无限制".to_string()),
        timeout_ms = config.timeout_ms,
        auto_reconnect = config.auto_reconnect,
        advice = diagnose_failures(fb, config, &last_connect_error, is_long_conn),
    );

    match std::fs::File::create(&dir) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                log::warn!("[压测] 失败日志写入失败: {}", e);
            }
            log::info!("[压测] 失败分析已写入: {}", dir.display());
        }
        Err(e) => log::warn!("[压测] 失败日志创建失败: {}", e),
    }
}

/// 根据失败分类 + 连接错误 + 连接模式给出诊断建议
fn diagnose_failures(
    fb: &crate::stress::stats::FailureBreakdownSnapshot,
    config: &StressTestConfig,
    last_err: &str,
    is_long_conn: bool,
) -> String {
    let total_fail =
        fb.connect_failed + fb.send_failed + fb.recv_timeout + fb.peer_closed + fb.validate_failed;
    if total_fail == 0 {
        return "无失败".to_string();
    }

    // 找出占比最高的失败类型
    let max = [
        fb.connect_failed,
        fb.send_failed,
        fb.recv_timeout,
        fb.peer_closed,
        fb.validate_failed,
    ]
    .iter()
    .copied()
    .max()
    .unwrap_or(0);

    let mut tips = Vec::new();

    if fb.connect_failed == max && fb.connect_failed > 0 {
        // 根据连接模式 + 错误类型给出不同建议
        let mode_advice = if last_err.contains("ConnectionRefused") {
            // ConnectionRefused: 服务端 accept 队列满或未监听,不是客户端端口问题
            format!(
                "• 连接失败占主导({}次),错误为 ConnectionRefused(os error 10061)。\n  \
                 这是服务端主动拒绝连接,不是客户端端口耗尽。原因:\n  \
                 - 服务端 accept 队列(backlog)已满,无法处理更多新连接。\n  \
                 - 服务端处理能力达上限,来不及 accept 新连接。\n  \
                 - 并发 {} 超过了服务端可承载的连接数。\n  \
                 \n  \
                 解决方案(按优先级):\n  \
                 1) 降低并发数,匹配服务端承载能力(如降到 5000 试试)\n  \
                 2) 开启 ramp-up(渐进式启动),避免瞬时连接洪峰\n  \
                 3) 调大服务端的 listen backlog(somaxconn)\n  \
                 4) 提升服务端处理能力(多线程 accept / 连接池)",
                fb.connect_failed, config.concurrency
            )
        } else if last_err.contains("AddrNotAvailable") || last_err.contains("端口耗尽") {
            // AddrNotAvailable: 客户端临时端口耗尽
            if is_long_conn {
                format!(
                    "• 连接失败占主导({}次),错误为 AddrNotAvailable(临时端口耗尽)。\n  \
                     当前为长连接模式,并发 {} 超过 Windows 默认临时端口范围(49152-65535,约 1.6万)。\n  \
                     调大端口范围(管理员 CMD):\n  \
                     netsh int ipv4 set dynamicport tcp start=10000 num=55535\n  \
                     netsh int ipv6 set dynamicport tcp start=10000 num=55535\n  \
                     或降低并发数到 15000 以下。",
                    fb.connect_failed, config.concurrency
                )
            } else {
                format!(
                    "• 连接失败占主导({}次),错误为 AddrNotAvailable(临时端口耗尽)。\n  \
                     当前为短连接模式,每包新建连接极易耗尽端口。\n  \
                     1) 改用长连接模式  2) 降低并发  3) 调大端口范围:\n  \
                     netsh int ipv4 set dynamicport tcp start=10000 num=55535",
                    fb.connect_failed
                )
            }
        } else {
            // 通用连接失败建议
            if is_long_conn {
                format!(
                    "• 连接失败占主导({}次),当前为长连接模式。可能原因:\n  \
                     - 临时端口耗尽(Windows 默认约 1.6万),并发 {} 可能超过此范围。\n  \
                       netsh int ipv4 set dynamicport tcp start=10000 num=55535\n  \
                     - socket 句柄上限。\n  \
                     - 目标服务端 accept 队列满,需调大 backlog。",
                    fb.connect_failed, config.concurrency
                )
            } else {
                format!(
                    "• 连接失败占主导({}次),当前为短连接模式。建议:\n  \
                     - 改用长连接模式  - 降低并发  - 调大临时端口范围",
                    fb.connect_failed
                )
            }
        };
        tips.push(mode_advice);
    }
    if fb.recv_timeout == max && fb.recv_timeout > 0 {
        tips.push(format!(
            "• 接收超时占主导({}次)。当前 timeout_ms={}。\n  \
             - 服务端处理不过来,响应慢。可适当调大 timeout_ms。\n  \
             - 并发过高导致服务端排队,降低并发试试。\n  \
             - PingPong 模式下 20000 worker 串行等待响应,QPS 天然受限。",
            fb.recv_timeout, config.timeout_ms
        ));
    }
    if fb.peer_closed == max && fb.peer_closed > 0 {
        tips.push(format!(
            "• 对端关闭占主导({}次)。服务端主动断开连接:\n  \
             - 服务端有连接数上限/超时清理机制。\n  \
             - 服务端进程崩溃或重启。\n  \
             - 开启 auto_reconnect 可自动重连。",
            fb.peer_closed
        ));
    }
    if fb.send_failed == max && fb.send_failed > 0 {
        tips.push(format!(
            "• 发送失败占主导({}次)。socket 写入报错:\n  \
             - 连接已被对端 RST。\n  \
             - 发送缓冲区满。",
            fb.send_failed
        ));
    }
    if fb.validate_failed == max && fb.validate_failed > 0 {
        tips.push(format!(
            "• 校验失败占主导({}次)。响应不匹配校验规则,检查响应校验配置。",
            fb.validate_failed
        ));
    }
    if tips.is_empty() {
        "无明显主导失败类型".to_string()
    } else {
        tips.join("\n")
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
    histogram: &ShardedHistogram,
    start: Instant,
) -> StressStats {
    let (sent, success, failure, active, disconnects, reconnects, bytes_sent, bytes_received) =
        stats.snapshot();
    // 更新峰值活跃连接数: fetch_max 返回旧值, 取 max(旧峰值, 当前active) 即新峰值
    let peak_active = stats
        .peak_active
        .fetch_max(active, Ordering::Relaxed)
        .max(active);
    let (p50, p95, p99, avg, max) = histogram.percentiles();
    let elapsed_ms = start.elapsed().as_millis() as u64;
    // current_qps = 最近一秒的发送速率(近似: sent / elapsed_s)
    let current_qps = if elapsed_ms > 0 {
        sent as f64 / (elapsed_ms as f64 / 1000.0)
    } else {
        0.0
    };
    let peak_qps = stats.peak_qps_milli.load(Ordering::Relaxed) as f64 / 1000.0;
    StressStats {
        elapsed_ms,
        current_qps,
        peak_qps,
        total_sent: sent,
        total_success: success,
        total_failure: failure,
        active_connections: active as usize,
        peak_active_connections: peak_active as usize,
        disconnects,
        reconnects,
        latency_p50_us: p50,
        latency_p95_us: p95,
        latency_p99_us: p99,
        latency_avg_us: avg,
        latency_max_us: max,
        bytes_sent,
        bytes_received,
        failures: crate::stress::stats::FailureBreakdownSnapshot::from(&stats.failures),
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
        let s1 = StressStats {
            total_sent: 100,
            ..Default::default()
        };
        let s2 = StressStats {
            total_sent: 250,
            ..Default::default()
        };
        let s3 = StressStats {
            total_sent: 400,
            ..Default::default()
        };
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
        push_per_second(
            &samples,
            &mut mark,
            &StressStats {
                elapsed_ms: 100,
                ..Default::default()
            },
        );
        push_per_second(
            &samples,
            &mut mark,
            &StressStats {
                elapsed_ms: 500,
                ..Default::default()
            },
        );
        push_per_second(
            &samples,
            &mut mark,
            &StressStats {
                elapsed_ms: 900,
                ..Default::default()
            },
        );
        assert_eq!(samples.lock().unwrap().len(), 0);
        // 跨入第 1 秒: 记录一次
        push_per_second(
            &samples,
            &mut mark,
            &StressStats {
                elapsed_ms: 1100,
                ..Default::default()
            },
        );
        assert_eq!(samples.lock().unwrap().len(), 1);
        // 第 1 秒内再次快照: 不重复记录
        push_per_second(
            &samples,
            &mut mark,
            &StressStats {
                elapsed_ms: 1500,
                ..Default::default()
            },
        );
        assert_eq!(samples.lock().unwrap().len(), 1);
        // 跨入第 2 秒
        push_per_second(
            &samples,
            &mut mark,
            &StressStats {
                elapsed_ms: 2100,
                ..Default::default()
            },
        );
        assert_eq!(samples.lock().unwrap().len(), 2);
    }

    /// 端到端集成测试: 本地 TCP echo server + 引擎 ping-pong 模式
    #[tokio::test]
    async fn test_engine_tcp_ping_pong_e2e() {
        use smol::channel::unbounded;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        use crate::config::connection::ConnectionType;
        use crate::stress::config::{ConnectionMode, StressMode};

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
        while let Ok(event) = tokio::time::timeout(Duration::from_secs(10), receiver.recv()).await {
            match event {
                Ok(StressEvent::StatsSnapshot { .. }) => snapshot_count += 1,
                Ok(StressEvent::Finished { report, .. }) => {
                    assert!(
                        report.total_sent >= 50,
                        "应至少发送 50, 实际 {}",
                        report.total_sent
                    );
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

        use crate::config::connection::ConnectionType;
        use crate::stress::config::{ConnectionMode, StressMode};

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
        while let Ok(event) = tokio::time::timeout(Duration::from_secs(10), receiver.recv()).await {
            match event {
                Ok(StressEvent::Finished { report, .. }) => {
                    assert!(
                        report.total_sent >= 30,
                        "UDP 应至少发送 30, 实际 {}",
                        report.total_sent
                    );
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

    /// 验证 ramp-up: 连接速率令牌桶应让 active 线性增长而非瞬间打满。
    ///
    /// 50 并发 + 3s ramp-up → 初始放行 5 个, 之后约 16.7 conn/s。
    /// 本地 echo server 上无 ramp-up 时 50 连接会在 <100ms 内全部建立,
    /// 因此 1s 时刻 active < 50 即证明 ramp-up 在生效。
    #[tokio::test]
    async fn test_engine_ramp_up_linear_growth() {
        use smol::channel::unbounded;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        use crate::config::connection::ConnectionType;
        use crate::stress::config::{ConnectionMode, RampUpConfig, StressMode};

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
            mode: StressMode::Throughput,
            connection_mode: ConnectionMode::Long,
            concurrency: 50,
            message_input_mode: "text".to_string(),
            payload_template: "PING ${seq}".to_string(),
            send_interval_ms: 100,
            global_qps_limit: None,
            stop_condition: StopCondition::Duration(5),
            ramp_up: RampUpConfig {
                enabled: true,
                ramp_up_secs: 3,
            },
            auto_reconnect: false,
            response_validation: None,
            timeout_ms: 2000,
        };

        let mut engine = StressTestEngine::start(config, "test".to_string(), sender);

        // 收集快照, 找到首个 elapsed_ms >= 1000 的快照
        let mut snapshot_at_1s: Option<StressStats> = None;
        let mut got_finished = false;
        while let Ok(event) = tokio::time::timeout(Duration::from_secs(10), receiver.recv()).await {
            match event {
                Ok(StressEvent::StatsSnapshot { stats, .. }) => {
                    if snapshot_at_1s.is_none() && stats.elapsed_ms >= 1000 {
                        snapshot_at_1s = Some(stats);
                    }
                }
                Ok(StressEvent::Finished { .. }) => {
                    got_finished = true;
                    break;
                }
                Ok(StressEvent::Error { msg, .. }) => panic!("引擎错误: {}", msg),
                Err(_) => break,
            }
        }
        assert!(got_finished, "应收到 Finished 事件");

        let snap = snapshot_at_1s.expect("应收到至少一个 elapsed_ms>=1000 的快照");
        // ramp-up 生效: 1s 时刻 active 应明显小于满并发 50
        // (无 ramp-up 时本地 echo 会在 <100ms 全连上, active 会立刻 = 50)
        assert!(
            snap.active_connections < 50,
            "ramp-up 应在 1s 时刻阻止全部连接建立, 实际 active={}",
            snap.active_connections
        );
        // 但应有部分连接已建立(初始批量 + 速率补充)
        assert!(
            snap.active_connections > 0,
            "1s 时刻应已有连接建立, 实际 active={}",
            snap.active_connections
        );

        engine.stop();
        echo_handle.abort();
    }
}
