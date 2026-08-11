// 压测事件与最终报告类型
//
// 引擎→App 通信: 引擎通过 smol channel 发送 StressEvent，App 事件泵消费。

use crate::stress::config::StressTestConfig;
use crate::stress::stats::{FailureBreakdownSnapshot, StressStats};

/// 引擎→App 事件
///
/// 每个变体携带 `tab_id` 以便 App 层按发起压测的 tab 路由,
/// 而非依赖 `active_tab`(用户可能已切换到其他 tab)。
#[derive(Debug)]
pub enum StressEvent {
    /// 周期统计快照(250ms)
    StatsSnapshot { tab_id: String, stats: StressStats },
    /// 压测结束(含最终报告)
    Finished {
        tab_id: String,
        report: StressReport,
    },
    /// 致命错误(引擎无法继续)
    #[allow(dead_code)]
    Error { tab_id: String, msg: String },
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
    /// 失败分类(连接/发送/超时/对端关闭/校验)
    pub failures: FailureBreakdownSnapshot,
    /// 每秒采样序列(CSV 时序数据)
    pub per_second_samples: Vec<StressStats>,
}
