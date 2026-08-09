// 压测事件与最终报告类型
//
// 引擎→App 通信: 引擎通过 smol channel 发送 StressEvent，App 事件泵消费。

use crate::stress::config::StressTestConfig;
use crate::stress::stats::StressStats;

/// 引擎→App 事件
#[derive(Debug)]
pub enum StressEvent {
    /// 周期统计快照(250ms)
    StatsSnapshot(StressStats),
    /// 压测结束(含最终报告)
    Finished(StressReport),
    /// 致命错误(引擎无法继续)
    #[allow(dead_code)]
    Error(String),
}

/// 最终报告(结束后发送 + CSV 导出源)
#[derive(Debug, Clone)]
pub struct StressReport {
    pub config: StressTestConfig,
    pub start_time: String,
    pub end_time: String,
    pub duration_ms: u64,
    pub total_sent: u64,
    pub total_success: u64,
    pub total_failure: u64,
    pub avg_qps: f64,
    pub peak_qps: f64,
    pub disconnects: u64,
    pub reconnects: u64,
    pub latency_p50_us: Option<u64>,
    pub latency_p95_us: Option<u64>,
    pub latency_p99_us: Option<u64>,
    pub latency_avg_us: Option<u64>,
    pub latency_max_us: Option<u64>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// 每秒采样序列(CSV 时序数据)
    pub per_second_samples: Vec<StressStats>,
}
