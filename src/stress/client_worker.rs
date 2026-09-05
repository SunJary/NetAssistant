// 单客户端压测 worker
//
// 每个 worker = 一个独立连接 + 一个 tokio task，按 send_interval 发包。
// 支持四种组合: {TCP,UDP} × {长连接,短连接}，以及吞吐/往返两种模式。
//
// 与现有 TcpClient/UdpClient 不同: 压测 worker 自管理连接生命周期、
// 速率控制、ping-pong RTT 测量、自动重连，不复用 NetworkConnectionManager。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
use crate::stress::stats::{ShardedHistogram, WorkerStats};
use crate::stress::variables::CompiledTemplate;
use crate::utils::hex::hex_to_bytes;

/// 连续连接失败的最大重试次数。
///
/// 超过此次数后 worker 退出,避免端口耗尽时永远空转重连。
/// 配合指数退避(100ms→5s),30 次重试约持续 2 分钟,足以覆盖大多数压测场景。
/// 压测时长 < 2 分钟时 worker 会一直重试;超长压测时最终放弃,active 自然下降。
const MAX_CONNECT_RETRIES: u32 = 30;

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
/// - `limiter`: 全局 QPS 令牌桶(控制发包速率)
/// - `stats`: 共享原子计数器
/// - `histogram`: 共享延迟直方图(ping-pong 模式写入)
/// - `cancel`: 协作取消令牌
/// - `_error_sender`: 致命错误上报通道(目前仅日志，预留)
/// - `connect_limiter`: 首次建连速率令牌桶(ramp-up; unbounded 时零开销)
pub async fn run_worker(
    worker_id: usize,
    config: StressTestConfig,
    global_seq: Arc<AtomicU64>,
    limiter: Arc<TokenBucket>,
    stats: Arc<WorkerStats>,
    histogram: Arc<ShardedHistogram>,
    cancel: CancellationToken,
    _error_sender: Sender<StressEvent>,
    connect_limiter: Arc<TokenBucket>,
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
    // 预编译模板: 启动时解析一次,避免每包重复 char_indices().collect()
    let compiled = CompiledTemplate::new(&config.payload_template);

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
                    &compiled,
                    &connect_limiter,
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
                    &compiled,
                    &connect_limiter,
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
                &compiled,
                &connect_limiter,
            )
            .await;
        }
    }

    debug!("[worker{}] 退出", worker_id);
}

/// 渲染并编码报文为字节(使用预编译模板,避免每包重复解析)
fn build_payload(
    compiled: &CompiledTemplate,
    global_seq: &AtomicU64,
    worker_id: usize,
    worker_counter: &mut u64,
    hex_mode: bool,
) -> Vec<u8> {
    let mut rendered = String::with_capacity(compiled.template_len() + 32);
    compiled.render(
        global_seq,
        worker_id,
        worker_counter,
        hex_mode,
        &mut rendered,
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
    histogram: &ShardedHistogram,
    worker_id: usize,
    timeout_dur: Duration,
    cancel: &CancellationToken,
) -> bool {
    // 发送(响应 cancel,避免关闭标签页后 worker 卡在 write 上等待超时)
    let send_result = tokio::select! {
        _ = cancel.cancelled() => return false,
        r = timeout(timeout_dur, socket.write_all(payload)) => r,
    };
    match send_result {
        Ok(Ok(())) => {
            stats
                .bytes_sent
                .fetch_add(payload.len() as u64, Ordering::Relaxed);
        }
        _ => {
            stats.failure.fetch_add(1, Ordering::Relaxed);
            stats.failures.send_failed.fetch_add(1, Ordering::Relaxed);
            return false;
        }
    }

    if config.mode == StressMode::Throughput {
        stats.success.fetch_add(1, Ordering::Relaxed);
        return true;
    }

    // ping-pong: 接收响应并计时(栈缓冲,避免每包 8KB 堆分配)
    let start = Instant::now();
    let mut buf = [0u8; 8192];
    let recv_result = tokio::select! {
        _ = cancel.cancelled() => return false,
        r = timeout(timeout_dur, socket.read(&mut buf)) => r,
    };
    let elapsed_us = start.elapsed().as_micros() as u64;

    match recv_result {
        Ok(Ok(n)) if n > 0 => {
            stats.bytes_received.fetch_add(n as u64, Ordering::Relaxed);
            if validator.validate(&buf[..n]) {
                stats.success.fetch_add(1, Ordering::Relaxed);
                histogram.record(worker_id, elapsed_us);
                true
            } else {
                stats.failure.fetch_add(1, Ordering::Relaxed);
                stats
                    .failures
                    .validate_failed
                    .fetch_add(1, Ordering::Relaxed);
                true // 校验失败但连接正常，不重连
            }
        }
        Ok(Ok(0)) => {
            // 对端关闭
            stats.failure.fetch_add(1, Ordering::Relaxed);
            stats.failures.peer_closed.fetch_add(1, Ordering::Relaxed);
            false
        }
        _ => {
            // 接收超时或错误
            stats.failure.fetch_add(1, Ordering::Relaxed);
            stats.failures.recv_timeout.fetch_add(1, Ordering::Relaxed);
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
    histogram: &Arc<ShardedHistogram>,
    cancel: CancellationToken,
    worker_counter: &mut u64,
    timeout_dur: Duration,
    send_interval: Duration,
    compiled: &CompiledTemplate,
    connect_limiter: &Arc<TokenBucket>,
) {
    let mut socket: Option<TcpStream> = None;
    let hex_mode = config.message_input_mode == "hex";
    // 连续连接失败次数(成功后重置),用于指数退避
    let mut consecutive_connect_failures: u32 = 0;
    // 是否持有活跃连接(用于准确维护 active 计数)
    let mut is_active = false;
    // 首次建连受 ramp-up 令牌桶限速; 重连不限速(ramp-up 只控初始加压曲线)
    let mut first_connect = true;

    loop {
        // 取消检查
        if cancel.is_cancelled() {
            break;
        }

        // 建立连接(首次或重连)
        if socket.is_none() {
            // 首次建连前获取 ramp-up 令牌(控制 active 线性增长)
            if first_connect {
                if !select_acquire(&cancel, connect_limiter).await {
                    break;
                }
                first_connect = false;
            }
            // 连接断开后递减 active(反映真实活跃连接数,而非配置的并发数)
            if is_active {
                stats.active.fetch_sub(1, Ordering::Relaxed);
                is_active = false;
            }
            let connect_result = select_connect(&cancel, target_addr, timeout_dur).await;
            match connect_result {
                ConnectOutcome::Connected(s) => {
                    socket = Some(s);
                    stats.active.fetch_add(1, Ordering::Relaxed);
                    is_active = true;
                    consecutive_connect_failures = 0; // 重置退避计数
                }
                ConnectOutcome::Cancelled => break,
                ConnectOutcome::Failed(err) => {
                    stats.failure.fetch_add(1, Ordering::Relaxed);
                    stats
                        .failures
                        .connect_failed
                        .fetch_add(1, Ordering::Relaxed);
                    // 记录最近一次连接错误(限频:每 100 次记一条,避免锁竞争)
                    if stats.failures.connect_failed.load(Ordering::Relaxed) % 100 == 1 {
                        stats.set_last_connect_error(&err);
                    }
                    if config.auto_reconnect {
                        consecutive_connect_failures =
                            consecutive_connect_failures.saturating_add(1);
                        // 超过最大重试次数则放弃,避免端口耗尽时永远空转
                        if consecutive_connect_failures > MAX_CONNECT_RETRIES {
                            stats.workers_gave_up.fetch_add(1, Ordering::Relaxed);
                            debug!(
                                "[worker{}] 连续连接失败 {} 次,超过上限 {},放弃重连",
                                worker_id, consecutive_connect_failures, MAX_CONNECT_RETRIES
                            );
                            break;
                        }
                        stats.disconnects.fetch_add(1, Ordering::Relaxed);
                        stats.reconnects.fetch_add(1, Ordering::Relaxed);
                        backoff_on_failure(&cancel, consecutive_connect_failures).await;
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

        let payload = build_payload(compiled, global_seq, worker_id, worker_counter, hex_mode);
        stats.sent.fetch_add(1, Ordering::Relaxed);

        let ok = send_and_maybe_recv_tcp(
            s,
            config,
            validator,
            &payload,
            stats,
            histogram,
            worker_id,
            timeout_dur,
            &cancel,
        )
        .await;

        // cancel 触发时立即退出,不计入 disconnect/reconnect 统计
        if cancel.is_cancelled() {
            break;
        }

        if !ok {
            // 连接异常，关闭并准备重连
            stats.disconnects.fetch_add(1, Ordering::Relaxed);
            socket.take();
            // is_active 保持 true,下次循环 socket.is_none() 时会递减 active
            if !config.auto_reconnect {
                break;
            }
            stats.reconnects.fetch_add(1, Ordering::Relaxed);
            // 发包失败后的退避:用 send_interval 或至少 100ms
            if !select_sleep(&cancel, send_interval.max(Duration::from_millis(100))).await {
                break;
            }
            continue;
        }

        // 间隔
        if !select_sleep(&cancel, send_interval).await {
            break;
        }
    }

    // 退出时递减 active(如果还持有连接)
    if is_active {
        stats.active.fetch_sub(1, Ordering::Relaxed);
    }
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
    histogram: &Arc<ShardedHistogram>,
    cancel: CancellationToken,
    worker_counter: &mut u64,
    timeout_dur: Duration,
    send_interval: Duration,
    compiled: &CompiledTemplate,
    connect_limiter: &Arc<TokenBucket>,
) {
    let hex_mode = config.message_input_mode == "hex";
    // 首次建连受 ramp-up 限速; 后续每包建连不受限(由 global QPS 令牌桶控制)
    let mut first_connect = true;
    loop {
        if cancel.is_cancelled() {
            break;
        }

        // 速率控制(QPS)
        if !select_acquire(&cancel, limiter).await {
            break;
        }

        // 首次建连前获取 ramp-up 令牌
        if first_connect {
            if !select_acquire(&cancel, connect_limiter).await {
                break;
            }
            first_connect = false;
        }

        // 短连接: 每包新建连接
        let connect_result = select_connect(&cancel, target_addr, timeout_dur).await;
        let mut socket = match connect_result {
            ConnectOutcome::Connected(s) => {
                // 修复: 短连接也维护 active 计数, 连接存活期间 +1
                stats.active.fetch_add(1, Ordering::Relaxed);
                s
            }
            ConnectOutcome::Cancelled => break,
            ConnectOutcome::Failed(err) => {
                stats.failure.fetch_add(1, Ordering::Relaxed);
                stats
                    .failures
                    .connect_failed
                    .fetch_add(1, Ordering::Relaxed);
                // 记录最近一次连接错误(限频)
                if stats.failures.connect_failed.load(Ordering::Relaxed) % 100 == 1 {
                    stats.set_last_connect_error(&err);
                }
                continue;
            }
        };

        let payload = build_payload(compiled, global_seq, worker_id, worker_counter, hex_mode);
        stats.sent.fetch_add(1, Ordering::Relaxed);

        let _ = send_and_maybe_recv_tcp(
            &mut socket,
            config,
            validator,
            &payload,
            stats,
            histogram,
            worker_id,
            timeout_dur,
            &cancel,
        )
        .await;

        // cancel 触发时立即退出,不再执行 shutdown/sleep
        if cancel.is_cancelled() {
            break;
        }

        // 短连接: 显式关闭(触发 TIME_WAIT)
        let _ = socket.shutdown().await;
        // 递减 active (连接已关闭)
        stats.active.fetch_sub(1, Ordering::Relaxed);

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
    histogram: &Arc<ShardedHistogram>,
    cancel: CancellationToken,
    worker_counter: &mut u64,
    timeout_dur: Duration,
    send_interval: Duration,
    compiled: &CompiledTemplate,
    connect_limiter: &Arc<TokenBucket>,
) {
    let bind_addr = if target_addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    // 首次建连(bind)前获取 ramp-up 令牌(UDP 只建连一次,无需 first_connect 标志)
    if !select_acquire(&cancel, connect_limiter).await {
        return;
    }
    let socket = match UdpSocket::bind(bind_addr).await {
        Ok(s) => s,
        Err(e) => {
            // bind 失败常见于端口耗尽 (AddrNotAvailable/WSAEADDRINUSE), 计入 connect_failed
            // 让压测结束日志的 diagnose_failures 能给出"调大端口范围"建议
            warn!("[worker{}] UDP bind 失败: {}", worker_id, e);
            stats.failure.fetch_add(1, Ordering::Relaxed);
            stats
                .failures
                .connect_failed
                .fetch_add(1, Ordering::Relaxed);
            if stats.failures.connect_failed.load(Ordering::Relaxed) % 100 == 1 {
                stats.set_last_connect_error(&e.to_string());
            }
            return;
        }
    };
    let _ = socket.connect(target_addr).await;
    stats.active.fetch_add(1, Ordering::Relaxed);
    let hex_mode = config.message_input_mode == "hex";

    loop {
        if cancel.is_cancelled() {
            break;
        }

        if !select_acquire(&cancel, limiter).await {
            break;
        }

        let payload = build_payload(compiled, global_seq, worker_id, worker_counter, hex_mode);
        stats.sent.fetch_add(1, Ordering::Relaxed);

        // 发送(响应 cancel)
        let send_result = tokio::select! {
            _ = cancel.cancelled() => break,
            r = timeout(timeout_dur, socket.send(&payload)) => r,
        };
        match send_result {
            Ok(Ok(n)) => {
                stats.bytes_sent.fetch_add(n as u64, Ordering::Relaxed);
            }
            _ => {
                stats.failure.fetch_add(1, Ordering::Relaxed);
                stats.failures.send_failed.fetch_add(1, Ordering::Relaxed);
                if !select_sleep(&cancel, send_interval).await {
                    break;
                }
                continue;
            }
        }

        if config.mode == StressMode::Throughput {
            stats.success.fetch_add(1, Ordering::Relaxed);
        } else {
            // ping-pong: 接收(栈缓冲,避免每包堆分配)
            let start = Instant::now();
            let mut buf = [0u8; 8192];
            let recv_result = tokio::select! {
                _ = cancel.cancelled() => break,
                r = timeout(timeout_dur, socket.recv(&mut buf)) => r,
            };
            let elapsed_us = start.elapsed().as_micros() as u64;
            match recv_result {
                Ok(Ok(n)) if n > 0 => {
                    stats.bytes_received.fetch_add(n as u64, Ordering::Relaxed);
                    if validator.validate(&buf[..n]) {
                        stats.success.fetch_add(1, Ordering::Relaxed);
                        histogram.record(worker_id, elapsed_us);
                    } else {
                        stats.failure.fetch_add(1, Ordering::Relaxed);
                        stats
                            .failures
                            .validate_failed
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                _ => {
                    stats.failure.fetch_add(1, Ordering::Relaxed);
                    stats.failures.recv_timeout.fetch_add(1, Ordering::Relaxed);
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
    /// 携带错误信息,便于写入失败日志定位根因
    Failed(String),
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
            Ok(Err(e)) => ConnectOutcome::Failed(format_io_error(&e)),
            Err(_) => ConnectOutcome::Failed("连接超时".to_string()),
        },
    }
}

/// 将 io::Error 格式化为可读字符串(含 ErrorKind 分类)
fn format_io_error(e: &std::io::Error) -> String {
    use std::io::ErrorKind;
    let kind_str = match e.kind() {
        ErrorKind::ConnectionRefused => "连接被拒绝(ConnectionRefused)",
        ErrorKind::AddrInUse => "地址被占用(AddrInUse)",
        ErrorKind::AddrNotAvailable => "地址不可用(AddrNotAvailable, 可能临时端口耗尽)",
        ErrorKind::TimedOut => "操作超时(TimedOut)",
        ErrorKind::PermissionDenied => "权限被拒(PermissionDenied)",
        ErrorKind::ConnectionReset => "连接被重置(ConnectionReset)",
        _ => "其他",
    };
    format!("{}: {}", kind_str, e)
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
    // 快速路径: 间隔为 0 时不 sleep,直接检查取消并返回。
    // 避免 20000 task 反复 sleep(0) → yield 导致调度器空转,QPS 被调度开销吃掉。
    if dur.is_zero() {
        return !cancel.is_cancelled();
    }
    tokio::select! {
        _ = cancel.cancelled() => false,
        _ = sleep(dur) => true,
    }
}

/// 指数退避重连等待，可被取消。
/// 连续失败退避: 基于连续失败次数递增退避时间。
///
/// 与旧的 select_backoff 不同,此函数接受外部维护的 `consecutive_failures` 计数器,
/// 退避时间随失败次数指数增长(100ms → 200ms → 400ms → ... → 上限 5s),
/// 且不会在每次调用时重置,避免端口耗尽时疯狂重连产生海量 connect_failed。
///
/// 成功连接后调用方应将 consecutive_failures 重置为 0。
async fn backoff_on_failure(cancel: &CancellationToken, consecutive_failures: u32) {
    let base = Duration::from_millis(100);
    let max = Duration::from_secs(5);
    // 退避 = min(100ms * 2^(failures-1), 5s)
    let exp = consecutive_failures.saturating_sub(1).min(6); // 2^6=64, 100ms*64=6.4s→cap 5s
    let delay = std::cmp::min(base * (1u32 << exp), max);
    select_sleep(cancel, delay).await;
}
