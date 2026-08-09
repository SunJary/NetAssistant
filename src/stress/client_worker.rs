// 单客户端压测 worker
//
// 每个 worker = 一个独立连接 + 一个 tokio task，按 send_interval 发包。
// 支持四种组合: {TCP,UDP} × {长连接,短连接}，以及吞吐/往返两种模式。
//
// 与现有 TcpClient/UdpClient 不同: 压测 worker 自管理连接生命周期、
// 速率控制、ping-pong RTT 测量、自动重连，不复用 NetworkConnectionManager。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{debug, warn};
use smol::channel::Sender;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

use crate::config::connection::ConnectionType;
use crate::stress::config::{ResponseValidation, StressMode, StressTestConfig};
use crate::stress::events::StressEvent;
use crate::stress::rate_limiter::TokenBucket;
use crate::stress::stats::{LatencyHistogram, WorkerStats};
use crate::stress::variables::render_payload;
use crate::utils::hex::hex_to_bytes;

/// 预编译的响应校验器
enum Validator {
    None,
    Contains(String),
    Exact(Vec<u8>),
    Regex(regex::Regex),
}

impl Validator {
    fn build(validation: &Option<ResponseValidation>) -> Self {
        match validation {
            None => Validator::None,
            Some(ResponseValidation::Contains(s)) => Validator::Contains(s.clone()),
            Some(ResponseValidation::Exact(s)) => Validator::Exact(s.as_bytes().to_vec()),
            Some(ResponseValidation::Regex(s)) => match regex::Regex::new(s) {
                Ok(re) => Validator::Regex(re),
                Err(e) => {
                    warn!("[压测] 无效正则 '{}': {}, 降级为不校验", s, e);
                    Validator::None
                }
            },
        }
    }

    /// 校验接收到的数据，返回是否通过
    fn validate(&self, received: &[u8]) -> bool {
        match self {
            Validator::None => true,
            Validator::Contains(s) => {
                let text = String::from_utf8_lossy(received);
                text.contains(s)
            }
            Validator::Exact(expected) => received == expected.as_slice(),
            Validator::Regex(re) => {
                let text = String::from_utf8_lossy(received);
                re.is_match(&text)
            }
        }
    }
}

/// 运行单个压测 worker。
///
/// - `worker_id`: worker 编号(用于变量替换与日志)
/// - `config`: 压测配置
/// - `global_seq`: 全局序号(所有 worker 共享)
/// - `limiter`: 全局 QPS 令牌桶
/// - `stats`: 共享原子计数器
/// - `histogram`: 共享延迟直方图(ping-pong 模式写入)
/// - `cancel`: 协作取消令牌
/// - `_error_sender`: 致命错误上报通道(目前仅日志，预留)
pub async fn run_worker(
    worker_id: usize,
    config: StressTestConfig,
    global_seq: Arc<AtomicU64>,
    limiter: Arc<TokenBucket>,
    stats: Arc<WorkerStats>,
    histogram: Arc<Mutex<LatencyHistogram>>,
    cancel: CancellationToken,
    _error_sender: Sender<StressEvent>,
) {
    let validator = Validator::build(&config.response_validation);
    let target_addr = match config.parse_target_addr() {
        Ok(a) => a,
        Err(e) => {
            warn!("[worker{}] 目标地址无效: {}", worker_id, e);
            return;
        }
    };
    let timeout_dur = Duration::from_millis(config.timeout_ms);
    let send_interval = Duration::from_millis(config.send_interval_ms);
    let mut worker_counter: u64 = 0;

    debug!(
        "[worker{}] 启动: {} {:?} {} 并发模式",
        worker_id, target_addr, config.protocol, config.connection_mode
    );

    match config.protocol {
        ConnectionType::Tcp => {
            if config.is_long_connection() {
                run_tcp_long(
                    worker_id,
                    &config,
                    target_addr,
                    &validator,
                    &global_seq,
                    &limiter,
                    &stats,
                    &histogram,
                    cancel,
                    &mut worker_counter,
                    timeout_dur,
                    send_interval,
                )
                .await;
            } else {
                run_tcp_short(
                    worker_id,
                    &config,
                    target_addr,
                    &validator,
                    &global_seq,
                    &limiter,
                    &stats,
                    &histogram,
                    cancel,
                    &mut worker_counter,
                    timeout_dur,
                    send_interval,
                )
                .await;
            }
        }
        ConnectionType::Udp => {
            run_udp(
                worker_id,
                &config,
                target_addr,
                &validator,
                &global_seq,
                &limiter,
                &stats,
                &histogram,
                cancel,
                &mut worker_counter,
                timeout_dur,
                send_interval,
            )
            .await;
        }
    }

    debug!("[worker{}] 退出", worker_id);
}

/// 渲染并编码报文为字节
fn build_payload(
    config: &StressTestConfig,
    global_seq: &AtomicU64,
    worker_id: usize,
    worker_counter: &mut u64,
) -> Vec<u8> {
    let hex_mode = config.message_input_mode == "hex";
    let rendered = render_payload(
        &config.payload_template,
        global_seq,
        worker_id,
        worker_counter,
        hex_mode,
    );
    if hex_mode {
        hex_to_bytes(&rendered)
    } else {
        rendered.into_bytes()
    }
}

/// 发送一包并(若 ping-pong)接收响应、计时、校验。
/// 返回 true 表示成功, false 表示失败(调用方决定是否重连)。
async fn send_and_maybe_recv_tcp(
    socket: &mut TcpStream,
    config: &StressTestConfig,
    validator: &Validator,
    payload: &[u8],
    stats: &WorkerStats,
    histogram: &Mutex<LatencyHistogram>,
    timeout_dur: Duration,
) -> bool {
    // 发送
    let send_result = timeout(timeout_dur, socket.write_all(payload)).await;
    match send_result {
        Ok(Ok(())) => {
            stats.bytes_sent.fetch_add(payload.len() as u64, Ordering::Relaxed);
        }
        _ => {
            stats.failure.fetch_add(1, Ordering::Relaxed);
            return false;
        }
    }

    if config.mode == StressMode::Throughput {
        stats.success.fetch_add(1, Ordering::Relaxed);
        return true;
    }

    // ping-pong: 接收响应并计时
    let start = Instant::now();
    let mut buf = vec![0u8; 8192];
    let recv_result = timeout(timeout_dur, socket.read(&mut buf)).await;
    let elapsed_us = start.elapsed().as_micros() as u64;

    match recv_result {
        Ok(Ok(n)) if n > 0 => {
            stats.bytes_received
                .fetch_add(n as u64, Ordering::Relaxed);
            if validator.validate(&buf[..n]) {
                stats.success.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut h) = histogram.lock() {
                    h.record(elapsed_us);
                }
                true
            } else {
                stats.failure.fetch_add(1, Ordering::Relaxed);
                true // 校验失败但连接正常，不重连
            }
        }
        Ok(Ok(0)) => {
            // 对端关闭
            stats.failure.fetch_add(1, Ordering::Relaxed);
            false
        }
        _ => {
            stats.failure.fetch_add(1, Ordering::Relaxed);
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_tcp_long(
    worker_id: usize,
    config: &StressTestConfig,
    target_addr: std::net::SocketAddr,
    validator: &Validator,
    global_seq: &Arc<AtomicU64>,
    limiter: &Arc<TokenBucket>,
    stats: &Arc<WorkerStats>,
    histogram: &Arc<Mutex<LatencyHistogram>>,
    cancel: CancellationToken,
    worker_counter: &mut u64,
    timeout_dur: Duration,
    send_interval: Duration,
) {
    stats.active.fetch_add(1, Ordering::Relaxed);
    let mut socket: Option<TcpStream> = None;

    loop {
        // 取消检查
        if cancel.is_cancelled() {
            break;
        }

        // 建立连接(首次或重连)
        if socket.is_none() {
            let connect_result = select_connect(&cancel, target_addr, timeout_dur).await;
            match connect_result {
                ConnectOutcome::Connected(s) => {
                    socket = Some(s);
                }
                ConnectOutcome::Cancelled => break,
                ConnectOutcome::Failed => {
                    stats.failure.fetch_add(1, Ordering::Relaxed);
                    if config.auto_reconnect {
                        stats.disconnects.fetch_add(1, Ordering::Relaxed);
                        stats.reconnects.fetch_add(1, Ordering::Relaxed);
                        select_backoff(&cancel, send_interval).await;
                        continue;
                    } else {
                        break;
                    }
                }
            }
        }

        let s = socket.as_mut().unwrap();

        // 速率控制
        if !select_acquire(&cancel, limiter).await {
            break;
        }

        let payload = build_payload(config, global_seq, worker_id, worker_counter);
        stats.sent.fetch_add(1, Ordering::Relaxed);

        let ok = send_and_maybe_recv_tcp(
            s, config, validator, &payload, stats, histogram, timeout_dur,
        )
        .await;

        if !ok {
            // 连接异常，关闭并准备重连
            stats.disconnects.fetch_add(1, Ordering::Relaxed);
            socket.take();
            if !config.auto_reconnect {
                break;
            }
            stats.reconnects.fetch_add(1, Ordering::Relaxed);
            select_backoff(&cancel, send_interval).await;
            continue;
        }

        // 间隔
        if !select_sleep(&cancel, send_interval).await {
            break;
        }
    }

    stats.active.fetch_sub(1, Ordering::Relaxed);
    debug!("[worker{}] TCP长连接退出", worker_id);
}

#[allow(clippy::too_many_arguments)]
async fn run_tcp_short(
    worker_id: usize,
    config: &StressTestConfig,
    target_addr: std::net::SocketAddr,
    validator: &Validator,
    global_seq: &Arc<AtomicU64>,
    limiter: &Arc<TokenBucket>,
    stats: &Arc<WorkerStats>,
    histogram: &Arc<Mutex<LatencyHistogram>>,
    cancel: CancellationToken,
    worker_counter: &mut u64,
    timeout_dur: Duration,
    send_interval: Duration,
) {
    loop {
        if cancel.is_cancelled() {
            break;
        }

        // 速率控制
        if !select_acquire(&cancel, limiter).await {
            break;
        }

        // 短连接: 每包新建连接
        let connect_result = select_connect(&cancel, target_addr, timeout_dur).await;
        let mut socket = match connect_result {
            ConnectOutcome::Connected(s) => s,
            ConnectOutcome::Cancelled => break,
            ConnectOutcome::Failed => {
                stats.failure.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        let payload = build_payload(config, global_seq, worker_id, worker_counter);
        stats.sent.fetch_add(1, Ordering::Relaxed);

        let _ = send_and_maybe_recv_tcp(
            &mut socket, config, validator, &payload, stats, histogram, timeout_dur,
        )
        .await;

        // 短连接: 显式关闭(触发 TIME_WAIT)
        let _ = socket.shutdown().await;

        if !select_sleep(&cancel, send_interval).await {
            break;
        }
    }
    debug!("[worker{}] TCP短连接退出", worker_id);
}

#[allow(clippy::too_many_arguments)]
async fn run_udp(
    worker_id: usize,
    config: &StressTestConfig,
    target_addr: std::net::SocketAddr,
    validator: &Validator,
    global_seq: &Arc<AtomicU64>,
    limiter: &Arc<TokenBucket>,
    stats: &Arc<WorkerStats>,
    histogram: &Arc<Mutex<LatencyHistogram>>,
    cancel: CancellationToken,
    worker_counter: &mut u64,
    timeout_dur: Duration,
    send_interval: Duration,
) {
    let bind_addr = if target_addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let socket = match UdpSocket::bind(bind_addr).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[worker{}] UDP bind 失败: {}", worker_id, e);
            return;
        }
    };
    let _ = socket.connect(target_addr).await;
    stats.active.fetch_add(1, Ordering::Relaxed);

    loop {
        if cancel.is_cancelled() {
            break;
        }

        if !select_acquire(&cancel, limiter).await {
            break;
        }

        let payload = build_payload(config, global_seq, worker_id, worker_counter);
        stats.sent.fetch_add(1, Ordering::Relaxed);

        // 发送
        let send_result = timeout(timeout_dur, socket.send(&payload)).await;
        match send_result {
            Ok(Ok(n)) => {
                stats.bytes_sent.fetch_add(n as u64, Ordering::Relaxed);
            }
            _ => {
                stats.failure.fetch_add(1, Ordering::Relaxed);
                if !select_sleep(&cancel, send_interval).await {
                    break;
                }
                continue;
            }
        }

        if config.mode == StressMode::Throughput {
            stats.success.fetch_add(1, Ordering::Relaxed);
        } else {
            // ping-pong: 接收
            let start = Instant::now();
            let mut buf = vec![0u8; 8192];
            let recv_result = timeout(timeout_dur, socket.recv(&mut buf)).await;
            let elapsed_us = start.elapsed().as_micros() as u64;
            match recv_result {
                Ok(Ok(n)) if n > 0 => {
                    stats.bytes_received.fetch_add(n as u64, Ordering::Relaxed);
                    if validator.validate(&buf[..n]) {
                        stats.success.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut h) = histogram.lock() {
                            h.record(elapsed_us);
                        }
                    } else {
                        stats.failure.fetch_add(1, Ordering::Relaxed);
                    }
                }
                _ => {
                    stats.failure.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if !select_sleep(&cancel, send_interval).await {
            break;
        }
    }

    stats.active.fetch_sub(1, Ordering::Relaxed);
    debug!("[worker{}] UDP退出", worker_id);
}

enum ConnectOutcome {
    Connected(TcpStream),
    Failed,
    Cancelled,
}

/// 在取消令牌与连接之间 select
async fn select_connect(
    cancel: &CancellationToken,
    target: std::net::SocketAddr,
    timeout_dur: Duration,
) -> ConnectOutcome {
    tokio::select! {
        _ = cancel.cancelled() => ConnectOutcome::Cancelled,
        r = timeout(timeout_dur, TcpStream::connect(target)) => match r {
            Ok(Ok(s)) => ConnectOutcome::Connected(s),
            _ => ConnectOutcome::Failed,
        },
    }
}

/// 在取消令牌与令牌桶 acquire 之间 select。
/// 返回 false 表示被取消。
async fn select_acquire(cancel: &CancellationToken, limiter: &Arc<TokenBucket>) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => false,
        _ = limiter.acquire() => true,
    }
}

/// 在取消令牌与 sleep 之间 select。
/// 返回 false 表示被取消。
async fn select_sleep(cancel: &CancellationToken, dur: Duration) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => false,
        _ = sleep(dur) => true,
    }
}

/// 指数退避重连等待，可被取消。
async fn select_backoff(cancel: &CancellationToken, base: Duration) {
    // 简单线性退避: base, 2*base, 4*base ... 上限 5s
    let mut delay = base.max(Duration::from_millis(100));
    let max = Duration::from_secs(5);
    for _ in 0..3 {
        if !select_sleep(cancel, delay).await {
            return;
        }
        delay = (delay * 2).min(max);
    }
}
