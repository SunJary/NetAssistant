// 压测统计: 实时快照 + 延迟直方图 + worker 共享原子计数器

use std::sync::atomic::{AtomicU64, Ordering};

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
    pub fn percentiles(&mut self) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
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
        (Some(pct(0.50)), Some(pct(0.95)), Some(pct(0.99)), Some(avg), Some(max))
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

/// worker 共享统计(原子计数，无锁)
///
/// 多个 worker 通过 Arc<WorkerStats> 共享，聚合 task 用 load 读取后生成 StressStats。
pub struct WorkerStats {
    pub sent: AtomicU64,
    pub success: AtomicU64,
    pub failure: AtomicU64,
    pub active: AtomicU64,
    pub disconnects: AtomicU64,
    pub reconnects: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
}

impl WorkerStats {
    pub fn new() -> Self {
        Self {
            sent: AtomicU64::new(0),
            success: AtomicU64::new(0),
            failure: AtomicU64::new(0),
            active: AtomicU64::new(0),
            disconnects: AtomicU64::new(0),
            reconnects: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
        }
    }

    /// 生成当前快照(原子读)
    pub fn snapshot(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64) {
        (
            self.sent.load(Ordering::Relaxed),
            self.success.load(Ordering::Relaxed),
            self.failure.load(Ordering::Relaxed),
            self.active.load(Ordering::Relaxed),
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
        assert_eq!((sent, success, failure, active, disconnects, reconnects, bs, br),
                   (10, 8, 2, 5, 1, 1, 1024, 512));
    }
}
