// TCP/UDP 压测模块
//
// 分层:
//   L1 纯逻辑: config / variables / rate_limiter / stats / report
//   L2 引擎  : events / client_worker / engine (UI 无关)
//
// 引擎层不依赖 GPUI，仅依赖 tokio + smol::channel + CancellationToken，
// 可脱离 UI 在 #[tokio::test] 中对本地 echo server 测试。

pub mod client_worker;
pub mod config;
pub mod engine;
pub mod events;
pub mod rate_limiter;
pub mod report;
pub mod stats;
pub mod variables;

pub use config::{StressTestConfig, TabViewMode};
pub use events::{StressEvent, StressReport};
pub use stats::StressStats;
