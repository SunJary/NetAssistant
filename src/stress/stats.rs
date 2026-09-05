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
    /// 本次压测出现过的最大瞬时 QPS(基于相邻快照 delta_sent/dt)
    pub peak_qps: f64,
    pub total_sent: u64,
    pub total_success: u64,
    pub total_failure: u64,
    pub active_connections: usize,
    /// 本次压测出现过的最大活跃连接数(停止后保持, 反映实际达成度)
    pub peak_active_connections: usize,
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

/// 对数分桶延迟直方图
///
/// worker 写入(每包 RTT)、聚合 task 读取(每 250ms 取百分位)。
/// 记录 O(1),聚合 O(桶数) —— 替代全量样本 clone+sort,高 QPS 下不再有周期性 CPU 热点。
///
/// 桶按数量级划分: 值 v 落在 [2^e, 2^(e+1)) 内,再均分 4 个子桶(64 × 4 = 256 桶)。
/// 百分位在命中桶内按均匀分布线性插值,误差上界约 ±19%,对 p50/p95/p99 展示足够;
/// avg 与 max 精确统计。内存固定 1KB/实例,无蓄水池采样。
pub struct BucketHistogram {
    buckets: [u32; 256],
    count: u64,
    sum: u64, // 单位: 微秒
    max: u64,
}

impl BucketHistogram {
    pub fn new() -> Self {
        Self {
            buckets: [0; 256],
            count: 0,
            sum: 0,
            max: 0,
        }
    }

    /// 记录一个延迟样本(微秒)
    pub fn record(&mut self, latency_us: u64) {
        let idx = if latency_us == 0 {
            0
        } else {
            let e = 63 - latency_us.leading_zeros() as usize; // v ∈ [2^e, 2^(e+1))
            let base = 1u64 << e;
            // u128 中间量避免 e=63 时 (v-base)*4 溢出
            let sub = (((latency_us - base) as u128 * 4) >> e) as usize; // 0..=3
            e * 4 + sub
        };
        self.buckets[idx] = self.buckets[idx].wrapping_add(1);
        self.count += 1;
        self.sum = self.sum.wrapping_add(latency_us);
        self.max = self.max.max(latency_us);
    }

    /// 合并另一个直方图的计数(用于分片聚合)
    pub fn merge(&mut self, other: &BucketHistogram) {
        for (dst, src) in self.buckets.iter_mut().zip(other.buckets.iter()) {
            *dst = dst.wrapping_add(*src);
        }
        self.count += other.count;
        self.sum = self.sum.wrapping_add(other.sum);
        self.max = self.max.max(other.max);
    }

    /// 计算百分位: 返回 (p50, p95, p99, avg, max)，单位微秒。
    /// 样本为空时对应字段为 None。
    pub fn percentiles(
        &self,
    ) -> (
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
    ) {
        if self.count == 0 {
            return (None, None, None, None, None);
        }
        let pct = |p: f64| -> u64 {
            // 向上取整秩，确保 p99 真实反映尾延迟
            let rank = ((self.count as f64) * p).ceil() as u64;
            let rank = rank.clamp(1, self.count);
            let mut acc = 0u64;
            for (idx, &c) in self.buckets.iter().enumerate() {
                let c = c as u64;
                if c == 0 {
                    continue;
                }
                if acc + c >= rank {
                    let (lo, hi) = bucket_range(idx);
                    // 桶内均匀分布假设: 按秩在桶内的比例线性插值
                    let within = (rank - acc) as f64 / c as f64;
                    let est = lo as f64 + within * (hi - lo) as f64;
                    return est.round() as u64;
                }
                acc += c;
            }
            self.max
        };
        let avg = self.sum / self.count;
        (
            Some(pct(0.50)),
            Some(pct(0.95)),
            Some(pct(0.99)),
            Some(avg),
            Some(self.max),
        )
    }

    /// 样本数
    #[allow(dead_code)]
    pub fn sample_count(&self) -> u64 {
        self.count
    }
}

impl Default for BucketHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// 桶 idx 覆盖的延迟区间 [lo, hi)(微秒)
fn bucket_range(idx: usize) -> (u64, u64) {
    let e = idx / 4;
    let sub = (idx % 4) as u128;
    let base = 1u128 << e;
    let width = base / 4;
    let lo = if e == 0 && sub == 0 {
        0
    } else {
        (base + width * sub) as u64
    };
    let hi = if e >= 63 && sub >= 3 {
        u64::MAX
    } else {
        (base + width * (sub + 1)) as u64
    };
    (lo, hi)
}

/// 分片延迟直方图
///
/// 将 worker 按 `worker_id % shards.len()` 分到不同分片,各分片独立 Mutex,
/// 竞争降低 N 倍(默认 32 分片)。聚合时合并各分片桶计数再算百分位。
///
/// 解决高并发(≥10000 worker)下所有 worker 串行抢同一把锁的瓶颈。
pub struct ShardedHistogram {
    shards: Vec<Mutex<BucketHistogram>>,
}

impl ShardedHistogram {
    /// 创建分片直方图,分片数 = min(concurrency, 64)
    pub fn new(concurrency: usize) -> Self {
        let shard_count = concurrency.clamp(1, 64);
        let shards = (0..shard_count)
            .map(|_| Mutex::new(BucketHistogram::new()))
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

    /// 合并所有分片桶计数并计算百分位
    pub fn percentiles(
        &self,
    ) -> (
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
    ) {
        let mut merged = BucketHistogram::new();
        for s in &self.shards {
            if let Ok(h) = s.lock() {
                merged.merge(&h);
            }
        }
        merged.percentiles()
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
    /// 本次压测出现过的最大活跃连接数
    /// 由 build_snapshot 每 250ms 用 fetch_max 更新, 低频写入无需 padding
    pub peak_active: AtomicU64,
    /// 本次压测出现过的最大瞬时 QPS(毫 qps = qps * 1000, 整数 fetch_max)
    /// 由 aggregator 每 250ms 用相邻快照的 delta_sent/dt 计算并 fetch_max
    pub peak_qps_milli: AtomicU64,
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
            peak_active: AtomicU64::new(0),
            peak_qps_milli: AtomicU64::new(0),
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
        let h = BucketHistogram::new();
        let (p50, p95, p99, avg, max) = h.percentiles();
        assert!(p50.is_none() && p95.is_none() && p99.is_none() && avg.is_none() && max.is_none());
    }

    #[test]
    fn test_record_and_percentiles_basic() {
        let mut h = BucketHistogram::new();
        for v in [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000] {
            h.record(v);
        }
        let (p50, p95, p99, avg, max) = h.percentiles();
        // 分桶插值有上界 ~19% 误差(数量级 4 子步),分位数断言放宽到 ±25%;
        // avg 与 max 为精确统计
        let p50 = p50.unwrap();
        assert!(p50 >= 375 && p50 <= 625, "p50={}", p50);
        let p95 = p95.unwrap();
        assert!(p95 >= 750 && p95 <= 1250, "p95={}", p95);
        let p99 = p99.unwrap();
        assert!(p99 >= 750 && p99 <= 1250, "p99={}", p99);
        assert_eq!(avg.unwrap(), 550); // (100+...+1000)/10 = 550
        assert_eq!(max.unwrap(), 1000);
    }

    #[test]
    fn test_single_sample() {
        let mut h = BucketHistogram::new();
        h.record(42);
        let (p50, p95, p99, avg, max) = h.percentiles();
        // 42 落在 [40,48) 桶,单样本插值取桶上界 48
        let p50 = p50.unwrap();
        assert!((40..=48).contains(&p50), "p50={}", p50);
        assert_eq!(p95.unwrap(), p50);
        assert_eq!(p99.unwrap(), p50);
        assert_eq!(avg.unwrap(), 42);
        assert_eq!(max.unwrap(), 42);
    }

    #[test]
    fn test_large_volume_bounded_memory() {
        let mut h = BucketHistogram::new();
        // 远超旧版蓄水池 cap(10万) 的样本量,分桶结构内存固定不增长
        for i in 0..1_000_000u64 {
            h.record(i % 10_000);
        }
        assert_eq!(h.sample_count(), 1_000_000);
        let (p50, _, _, avg, max) = h.percentiles();
        let p50 = p50.unwrap();
        assert!(p50 > 4000 && p50 < 6000, "p50={}", p50);
        assert_eq!(avg.unwrap(), 4999);
        assert_eq!(max.unwrap(), 9999);
    }

    #[test]
    fn test_merge() {
        let mut a = BucketHistogram::new();
        let mut b = BucketHistogram::new();
        a.record(100);
        b.record(900);
        a.merge(&b);
        assert_eq!(a.sample_count(), 2);
        let (_, _, _, avg, max) = a.percentiles();
        assert_eq!(avg.unwrap(), 500);
        assert_eq!(max.unwrap(), 900);
    }

    #[test]
    fn test_sharded_histogram_percentiles() {
        let h = ShardedHistogram::new(8);
        for w in 0..64usize {
            for i in 1..=100u64 {
                h.record(w, i);
            }
        }
        let (p50, _, _, avg, max) = h.percentiles();
        let p50 = p50.unwrap();
        assert!(p50 > 40 && p50 < 60, "p50={}", p50);
        assert_eq!(avg.unwrap(), 50);
        assert_eq!(max.unwrap(), 100);
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
