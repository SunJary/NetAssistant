//! 发送任务体系(逐行发送; 为后期心跳任务预留扩展点)。
//!
//! 结构:
//! - `model`: 数据模型(纯逻辑)
//! - `parse`: 行解析(纯逻辑 + 单测)
//! - `engine`: 调度引擎(直连网络写通道, 进度走事件)

pub mod engine;
pub mod model;
pub mod parse;

pub use engine::SendTaskEngine;
pub use model::{
    IntervalHandle, PeriodicSource, SendTaskConfig, SendTaskState, TaskEndReason, TaskKind,
    TaskStatus, TaskTarget, TimedTaskProfile, build_periodic_config, build_timed_config,
};
pub use parse::{LineParseError, MAX_TASK_ITEMS, parse_lines};

use std::sync::{Arc, Mutex as StdMutex};

/// UI 持有的任务条目: 引擎句柄 + 运行期状态快照。
///
/// 引擎句柄用 `Arc<StdMutex<..>>` 包装以支持 `ConnectionTabState: Clone`;
/// `Mutex<Option<..>>` 便于删除任务时 `take()` 出引擎并 `stop()`。
#[derive(Clone)]
pub struct SendTaskEntry {
    pub engine: Arc<StdMutex<Option<SendTaskEngine>>>,
    pub state: SendTaskState,
}
