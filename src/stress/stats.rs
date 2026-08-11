// 压测统计: 实时快照 + 延迟直方图 + worker 共享原子计数器

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

/// 失败分类快照(用于 StressStats / StressReport)
#[derive(Debug, Clone, Copy, Default)]
pub struct FailureBreakdownSnapshot {
    pub connect_failed: u64,
    pub send_failed: u64,
    pub recv_timeout: u64,
    pub peer_closed: u64,
    pub validate_failed: u64,
}

impl FailureBreakdownSnapshot {
    pub fn from(stats: &FailureBreakdown) -> Self {
        let (c, s, r, p, v) = stats.snapshot();
        Self {
            connect_failed: c,
            send_failed: s,
            recv_timeout: r,
            peer_closed: p,
            validate_failed: v,
        }
    }
}

/// 实时统计快照(引擎每 250ms 生成一份，经 channel 推送 UI)
#[derive(Debug, Clone, Default)]
pub struct StressStats {
    pub elapsed_ms: u64,
    pub current_qps: f64,
    pub total_sent: u64,
    pub total_success: u64,
    pub total_failure: u64,
    pub active_connections: usize,
    pub disconnects: u64,
    pub reconnects: u64,
    pub latency_p50_us: Option<u64>,
    pub latency_p95_us: Option<u64>,
    pub latency_p99_us: Option<u64>,
    pub latency_avg_us: Option<u64>,
    pub latency_max_us: Option<u64>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// 失败分类(连接/发送/超时/对端关闭/校验)
    pub failures: FailureBreakdownSnapshot,
}

/// 延迟直方图
///
/// worker 写入(每包 RTT)、聚合 task 读取(每 250ms 取百分位)。
/// 保留全部样本，超过 cap 后按等概率采样缩减，防止 OOM。
pub struct LatencyHistogram {
    samples: Vec<u64>, // 单位: 微秒
    cap: usize,
    /// 用于等概率采样的计数(已记录总数，含被丢弃的)
    total_recorded: u64,
    /// 简单 LCG 状态用于采样决策
    sample_state: u64,
}

impl LatencyHistogram {
    pub fn new(cap: usize) -> Self {
        Self {
            samples: Vec::with_capacity(cap.min(1024)),
            cap,
            total_recorded: 0,
            sample_state: 0x2545F4914F6CDD1D, // 任意非零初值
        }
    }

    pub fn with_default_cap() -> Self {
        Self::new(100_000)
    }

    /// 记录一个延迟样本(微秒)
    pub fn record(&mut self, latency_us: u64) {
        self.total_recorded += 1;
        if self.samples.len() < self.cap {
            self.samples.push(latency_us);
            return;
        }
        // 容量已满: 等概率替换(蓄水池采样)
        // 用自增计数器驱动伪随机，避免引入 rand
        self.sample_state = self
            .sample_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let rand = self.sample_state >> 33; // 高 31 位作为随机数
        let idx = (rand as usize) % self.total_recorded as usize;
        if idx < self.cap {
            self.samples[idx] = latency_us;
        }
    }

    /// 计算百分位: 返回 (p50, p95, p99, avg, max)，单位微秒。
    /// 样本为空时对应字段为 None。
    ///
    /// 原地排序(不 clone)，避免每 250ms 快照时分配 800KB 临时内存。
    /// 蓄水池采样不依赖样本顺序，排序不影响后续 record() 的正确性。
    #[allow(dead_code)]
    pub fn percentiles(
        &mut self,
    ) -> (
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
    ) {
        if self.samples.is_empty() {
            return (None, None, None, None, None);
        }
        self.samples.sort_unstable();
        let len = self.samples.len();
        let pct = |p: f64| -> u64 {
            // 向上取整索引，确保 p99 真实反映尾延迟
            let idx = ((len as f64) * p).ceil() as usize;
            self.samples[idx.saturating_sub(1).min(len - 1)]
        };
        let avg = self.samples.iter().sum::<u64>() / len as u64;
        let max = *self.samples.last().unwrap();
        (
            Some(pct(0.50)),
            Some(pct(0.95)),
            Some(pct(0.99)),
            Some(avg),
            Some(max),
        )
    }

    /// 样本数
    #[allow(dead_code)]
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// 重置(用于新一轮压测)
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.samples.clear();
        self.total_recorded = 0;
    }
}

/// 分片延迟直方图
///
/// 将 worker 按 `worker_id % shards.len()` 分到不同分片,各分片独立 Mutex,
/// 竞争降低 N 倍(默认 32 分片)。聚合时合并各分片样本再算百分位。
///
/// 解决高并发(≥10000 worker)下所有 worker 串行抢同一把锁的瓶颈。
pub struct ShardedHistogram {
    shards: Vec<Mutex<LatencyHistogram>>,
}

impl ShardedHistogram {
    /// 创建分片直方图,分片数 = min(concurrency, 64)
    pub fn new(concurrency: usize) -> Self {
        let shard_count = concurrency.clamp(1, 64);
        let shards = (0..shard_count)
            .map(|_| Mutex::new(LatencyHistogram::with_default_cap()))
            .collect();
        Self { shards }
    }

    /// 记录一个延迟样本(按 worker_id 分片,降低锁竞争)
    pub fn record(&self, worker_id: usize, latency_us: u64) {
        let idx = worker_id % self.shards.len();
        if let Ok(mut h) = self.shards[idx].lock() {
            h.record(latency_us);
        }
    }

    /// 合并所有分片样本并计算百分位
    pub fn percentiles(
        &self,
    ) -> (
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
    ) {
        let mut all: Vec<u64> = Vec::new();
        for s in &self.shards {
            if let Ok(h) = s.lock() {
                all.extend_from_slice(&h.samples);
            }
        }
        if all.is_empty() {
            return (None, None, None, None, None);
        }
        all.sort_unstable();
        let len = all.len();
        let pct = |p: f64| -> u64 {
            let idx = ((len as f64) * p).ceil() as usize;
            all[idx.saturating_sub(1).min(len - 1)]
        };
        let avg = all.iter().sum::<u64>() / len as u64;
        let max = *all.last().unwrap();
        (
            Some(pct(0.50)),
            Some(pct(0.95)),
            Some(pct(0.99)),
            Some(avg),
            Some(max),
        )
    }
}

/// worker 共享统计(原子计数，无锁)
///
/// 多个 worker 通过 Arc<WorkerStats> 共享，聚合 task 用 load 读取后生成 StressStats。
///
/// 高频原子字段(sent/success/failure/active)用缓存行填充,避免 20000 worker
/// 跨核 fetch_add 时同一缓存行反复失效(伪共享)。
#[repr(align(64))]
#[derive(Default)]
pub struct CacheLinePadded<T>(pub T);

impl<T> std::ops::Deref for CacheLinePadded<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for CacheLinePadded<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// 失败分类计数(原子,无锁)
///
/// 区分失败原因,帮助定位 QPS 下降根因(如大量连接失败 → 端口耗尽/服务端拒绝)。
/// 所有字段原子累加,聚合时 load 读取。
#[derive(Debug, Default)]
pub struct FailureBreakdown {
    /// TCP/UDP 连接建立失败(目标拒绝/超时/端口耗尽)
    pub connect_failed: AtomicU64,
    /// 发送失败(socket write/send 报错)
    pub send_failed: AtomicU64,
    /// 接收超时(等待响应超过 timeout_ms)
    pub recv_timeout: AtomicU64,
    /// 对端关闭连接(read 返回 0)
    pub peer_closed: AtomicU64,
    /// 响应校验失败(连接正常但响应不匹配)
    pub validate_failed: AtomicU64,
}

impl FailureBreakdown {
    pub fn new() -> Self {
        Self::default()
    }

    /// 汇总各分类(load 读取)
    pub fn snapshot(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.connect_failed.load(Ordering::Relaxed),
            self.send_failed.load(Ordering::Relaxed),
            self.recv_timeout.load(Ordering::Relaxed),
            self.peer_closed.load(Ordering::Relaxed),
            self.validate_failed.load(Ordering::Relaxed),
        )
    }
}

pub struct WorkerStats {
    pub sent: CacheLinePadded<AtomicU64>,
    pub success: CacheLinePadded<AtomicU64>,
    pub failure: CacheLinePadded<AtomicU64>,
    pub active: CacheLinePadded<AtomicU64>,
    pub disconnects: AtomicU64,
    pub reconnects: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    /// 失败分类计数
    pub failures: FailureBreakdown,
    /// 因连续连接失败超过上限而退出的 worker 数
    pub workers_gave_up: AtomicU64,
    /// 最近一次连接错误信息(限频写入,读取用于失败日志)
    last_connect_error: RwLock<String>,
}

impl WorkerStats {
    pub fn new() -> Self {
        Self {
            sent: CacheLinePadded(AtomicU64::new(0)),
            success: CacheLinePadded(AtomicU64::new(0)),
            failure: CacheLinePadded(AtomicU64::new(0)),
            active: CacheLinePadded(AtomicU64::new(0)),
            disconnects: AtomicU64::new(0),
            reconnects: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            failures: FailureBreakdown::new(),
            workers_gave_up: AtomicU64::new(0),
            last_connect_error: RwLock::new(String::new()),
        }
    }

    /// 记录最近一次连接错误(限频调用,如每 100 次记一条)
    pub fn set_last_connect_error(&self, err: &str) {
        if let Ok(mut w) = self.last_connect_error.write() {
            *w = err.to_string();
        }
    }

    /// 读取最近一次连接错误
    pub fn last_connect_error(&self) -> String {
        self.last_connect_error
            .read()
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// 生成当前快照(原子读)
    pub fn snapshot(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64) {
        (
            self.sent.0.load(Ordering::Relaxed),
            self.success.0.load(Ordering::Relaxed),
            self.failure.0.load(Ordering::Relaxed),
            self.active.0.load(Ordering::Relaxed),
            self.disconnects.load(Ordering::Relaxed),
            self.reconnects.load(Ordering::Relaxed),
            self.bytes_sent.load(Ordering::Relaxed),
            self.bytes_received.load(Ordering::Relaxed),
        )
    }
}

impl Default for WorkerStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_histogram_percentiles() {
        let mut h = LatencyHistogram::with_default_cap();
        let (p50, p95, p99, avg, max) = h.percentiles();
        assert!(p50.is_none() && p95.is_none() && p99.is_none() && avg.is_none() && max.is_none());
    }

    #[test]
    fn test_record_and_percentiles_basic() {
        let mut h = LatencyHistogram::with_default_cap();
        for v in [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000] {
            h.record(v);
        }
        let (p50, p95, p99, avg, max) = h.percentiles();
        // 10 样本: p50 应在 500/600 附近, p95 在 1000, p99 在 1000
        let p50 = p50.unwrap();
        assert!(p50 >= 500 && p50 <= 600, "p50={}", p50);
        assert_eq!(p95.unwrap(), 1000);
        assert_eq!(p99.unwrap(), 1000);
        assert_eq!(avg.unwrap(), 550); // (100+...+1000)/10 = 550
        assert_eq!(max.unwrap(), 1000);
    }

    #[test]
    fn test_single_sample() {
        let mut h = LatencyHistogram::with_default_cap();
        h.record(42);
        let (p50, p95, p99, avg, max) = h.percentiles();
        assert_eq!(p50.unwrap(), 42);
        assert_eq!(p95.unwrap(), 42);
        assert_eq!(p99.unwrap(), 42);
        assert_eq!(avg.unwrap(), 42);
        assert_eq!(max.unwrap(), 42);
    }

    #[test]
    fn test_reset() {
        let mut h = LatencyHistogram::with_default_cap();
        h.record(100);
        h.record(200);
        assert_eq!(h.sample_count(), 2);
        h.reset();
        assert_eq!(h.sample_count(), 0);
        assert!(h.percentiles().0.is_none());
    }

    #[test]
    fn test_cap_reservoir_sampling() {
        let cap = 100;
        let mut h = LatencyHistogram::new(cap);
        // 记录远超 cap 的样本，验证不会 OOM 且样本数 <= cap
        for i in 0..10_000u64 {
            h.record(i);
        }
        assert_eq!(h.sample_count(), cap);
        // 百分位仍能正常计算
        let (p50, _, _, _, max) = h.percentiles();
        assert!(p50.is_some());
        assert!(max.unwrap() < 10_000);
    }

    #[test]
    fn test_worker_stats_atomic_increment() {
        let ws = WorkerStats::new();
        ws.sent.fetch_add(10, Ordering::Relaxed);
        ws.success.fetch_add(8, Ordering::Relaxed);
        ws.failure.fetch_add(2, Ordering::Relaxed);
        ws.active.fetch_add(5, Ordering::Relaxed);
        ws.disconnects.fetch_add(1, Ordering::Relaxed);
        ws.reconnects.fetch_add(1, Ordering::Relaxed);
        ws.bytes_sent.fetch_add(1024, Ordering::Relaxed);
        ws.bytes_received.fetch_add(512, Ordering::Relaxed);
        let (sent, success, failure, active, disconnects, reconnects, bs, br) = ws.snapshot();
        assert_eq!(
            (
                sent,
                success,
                failure,
                active,
                disconnects,
                reconnects,
                bs,
                br
            ),
            (10, 8, 2, 5, 1, 1, 1024, 512)
        );
    }
}
