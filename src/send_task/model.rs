//! 发送任务数据模型(纯逻辑, 不依赖 gpui, 可无头单测)。
//!
//! 建模: 任务(Task) = 一次导入产生的一批 → 调度单位;
//! 任务项(Item) = 文件的一行 → 发送单位(进度显示为 `i / N`)。
//!
//! 三条运行路径共用同一套模型与引擎:
//! - `SendByLines`: 逐行发送(导入文件 / 粘贴多行)
//! - `SendTimed`: 定时任务(心跳), 固定单条无限循环(面板可见)
//! - `SendPeriodic`: 周期发送(行内勾选), 每轮实时取发送框内容(面板隐藏)

use crate::network::events::WireMessage;
use crate::utils::message_vars::{CompiledTemplate, RenderContext};
use serde::{Deserialize, Serialize};
use smol::channel::Sender;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// 任务项: 一行 = 一条待发消息(模板预编译一次, 每轮重新渲染)。
#[derive(Debug, Clone)]
pub struct TaskItem {
    /// 0-based 行号(UI 展示 index + 1)
    pub index: usize,
    /// 原文(UI 展开时展示)
    pub raw: String,
    /// 预编译模板(复用 utils::message_vars::CompiledTemplate)
    pub compiled: Arc<CompiledTemplate>,
}

/// 可热更新的间隔句柄(引擎每轮读取最新值; 所有任务统一使用, 普通任务不变更它)。
///
/// 界面改动发送间隔时写入新值并唤醒引擎, 引擎据此从当前时刻重新计时,
/// 无需停止重建任务即可「立刻生效」。
#[derive(Clone, Debug)]
pub struct IntervalHandle(Arc<AtomicU64>);

impl IntervalHandle {
    pub fn new(ms: u64) -> Self {
        Self(Arc::new(AtomicU64::new(ms)))
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::Relaxed);
    }
}

/// 周期发送的实时载荷(主线程写, 引擎每轮读; 无需重建任务)。
#[derive(Debug)]
pub struct PeriodicSource {
    /// 原文(模板), 随发送框内容变化写入
    pub content: RwLock<String>,
    /// 随发送框模式切换写入
    pub hex_mode: AtomicBool,
    /// `${seq}` 用(与手动发送共享同一计数器)
    pub seq: Arc<AtomicU64>,
}

impl PeriodicSource {
    pub fn new(seq: Arc<AtomicU64>, content: String, hex_mode: bool) -> Self {
        Self {
            content: RwLock::new(content),
            hex_mode: AtomicBool::new(hex_mode),
            seq,
        }
    }

    /// 写入最新发送框内容(不唤醒引擎; 下一轮自然取到)
    pub fn set_content(&self, content: String) {
        if let Ok(mut c) = self.content.write() {
            *c = content;
        }
    }

    /// 写入最新模式(hex / text)
    pub fn set_hex_mode(&self, hex_mode: bool) {
        self.hex_mode.store(hex_mode, Ordering::Relaxed);
    }
}

/// 单步渲染结果(引擎投递用)。
#[derive(Debug, Clone)]
pub struct RenderedStep {
    pub bytes: Vec<u8>,
    /// 该步是否为 hex 模式(决定消息展示类型)
    pub hex_mode: bool,
}

/// 任务类型。
#[derive(Debug, Clone)]
pub enum TaskKind {
    /// 逐行发送: 每行一条消息
    SendByLines { items: Vec<TaskItem> },
    /// 定时任务(心跳): 固定单条, 无限循环
    SendTimed { item: TaskItem },
    /// 周期发送: 每轮实时读取 `PeriodicSource`
    SendPeriodic { source: Arc<PeriodicSource> },
}

/// 任务调度配置。
#[derive(Debug, Clone)]
pub struct SendTaskConfig {
    /// UUID
    pub id: String,
    /// 展示名(UI 卡片标题)
    pub name: String,
    pub kind: TaskKind,
    /// 每条消息之间的间隔(可热更新; 无下限)
    pub interval: IntervalHandle,
    /// 是否循环整批
    pub loop_enabled: bool,
    /// Some(n) 限轮次; None = 无限(仅 loop_enabled 时有意义)
    pub max_rounds: Option<u32>,
    /// 与创建时输入框模式一致(周期发送改为每轮读 `PeriodicSource.hex_mode`)
    pub hex_mode: bool,
    /// 创建后是否立即开始(否则先置 Paused)
    pub start_immediately: bool,
    /// 是否在任务面板隐藏(周期发送恒为 true; 仍留在 `send_tasks` 表内以复用目标刷新/断线暂停)
    pub hidden: bool,
}

impl SendTaskConfig {
    /// 任务项列表(仅逐行发送有条目; 其余为空)
    pub fn items(&self) -> &[TaskItem] {
        match &self.kind {
            TaskKind::SendByLines { items } => items,
            TaskKind::SendTimed { .. } | TaskKind::SendPeriodic { .. } => &[],
        }
    }

    /// 单个调度单位的步数(逐行 = 行数; 定时/周期 = 1)
    pub fn step_count(&self) -> usize {
        match &self.kind {
            TaskKind::SendByLines { items } => items.len(),
            TaskKind::SendTimed { .. } | TaskKind::SendPeriodic { .. } => 1,
        }
    }

    /// 单个调度单位的总步数(逐行 = 行数; 定时/周期 = 1), 用于 `i / N` 展示
    pub fn total_items(&self) -> usize {
        self.step_count()
    }

    /// 是否为定时任务(心跳)
    pub fn is_timed(&self) -> bool {
        matches!(self.kind, TaskKind::SendTimed { .. })
    }

    /// 是否为周期发送(行内勾选, 面板隐藏)
    pub fn is_periodic(&self) -> bool {
        matches!(self.kind, TaskKind::SendPeriodic { .. })
    }

    /// 取第 `idx` 步载荷(原文, 字节); `None` = 本轮跳过(空内容 / 非法 HEX)。
    ///
    /// - `SendByLines` / `SendTimed`: 预编译模板, 每轮按需消费 `${seq}`
    /// - `SendPeriodic`: 实时读 `PeriodicSource.content` / `hex_mode`, 每轮重新编译
    pub fn render_step(&self, idx: usize, seq: &AtomicU64) -> Option<RenderedStep> {
        match &self.kind {
            TaskKind::SendByLines { items } => {
                let item = items.get(idx)?;
                Some(render_item(item, self.hex_mode, seq))
            }
            TaskKind::SendTimed { item } => Some(render_item(item, self.hex_mode, seq)),
            TaskKind::SendPeriodic { source } => {
                let content = source.content.read().ok()?.clone();
                if content.trim().is_empty() {
                    return None;
                }
                let hex_mode = source.hex_mode.load(Ordering::Relaxed);
                let compiled = CompiledTemplate::new(&content);
                let seq_value = if compiled.needs_seq() {
                    Some(source.seq.fetch_add(1, Ordering::Relaxed))
                } else {
                    None
                };
                let ctx = RenderContext::common(seq_value);
                let mut out = String::with_capacity(compiled.template_len() + 32);
                compiled.render(&ctx, hex_mode, &mut out);
                if hex_mode {
                    // 非法 HEX 直接跳过本轮, 绝不发出错误字节
                    if !crate::utils::hex::validate_hex_input(&out) {
                        return None;
                    }
                    Some(RenderedStep {
                        bytes: crate::utils::hex::hex_to_bytes(&out),
                        hex_mode: true,
                    })
                } else {
                    Some(RenderedStep {
                        bytes: out.into_bytes(),
                        hex_mode: false,
                    })
                }
            }
        }
    }
}

/// 渲染一条预编译任务项为线上字节(hex 模式先渲染文本再解码)。
fn render_item(item: &TaskItem, hex_mode: bool, seq: &AtomicU64) -> RenderedStep {
    let seq_value = if item.compiled.needs_seq() {
        Some(seq.fetch_add(1, Ordering::Relaxed))
    } else {
        None
    };
    let ctx = RenderContext::common(seq_value);
    let mut out = String::with_capacity(item.compiled.template_len() + 32);
    item.compiled.render(&ctx, hex_mode, &mut out);
    let bytes = if hex_mode {
        crate::utils::hex::hex_to_bytes(&out)
    } else {
        out.into_bytes()
    };
    RenderedStep { bytes, hex_mode }
}

/// 定时任务(心跳)的持久化真源(按 connection_id 索引)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedTaskProfile {
    /// 卡片「暂停/继续」切换并持久化; 断线只暂停不改它
    pub enabled: bool,
    /// 原文; hex 模式下为 hex 文本(可含 `${...}`)
    pub message: String,
    /// 创建时跟随连接 message_input_mode
    pub hex_mode: bool,
    #[serde(default = "default_timed_interval")]
    pub interval_ms: u64,
}

fn default_timed_interval() -> u64 {
    30_000
}

/// 由持久化 profile 构建心跳运行期配置。
///
/// - kind: `SendTimed`(单条预编译模板); `loop_enabled = true`; `max_rounds = None`
/// - `hidden = false`(面板可见); 间隔为可热更新的 `IntervalHandle`
pub fn build_timed_config(
    profile: &TimedTaskProfile,
    start_immediately: bool,
    name: String,
) -> SendTaskConfig {
    let item = TaskItem {
        index: 0,
        raw: profile.message.clone(),
        compiled: Arc::new(CompiledTemplate::new(&profile.message)),
    };
    SendTaskConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        kind: TaskKind::SendTimed { item },
        interval: IntervalHandle::new(profile.interval_ms),
        loop_enabled: true,
        max_rounds: None,
        hex_mode: profile.hex_mode,
        start_immediately,
        hidden: false,
    }
}

/// 构建周期发送(hidden)运行期配置。
///
/// - kind: `SendPeriodic`; `hidden = true`; `loop_enabled = true`; `max_rounds = None`
/// - 间隔与载荷均由 UI 实时写入, 故此处只接收初始间隔句柄
pub fn build_periodic_config(
    source: Arc<PeriodicSource>,
    interval: IntervalHandle,
    name: String,
) -> SendTaskConfig {
    SendTaskConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        kind: TaskKind::SendPeriodic { source },
        interval,
        loop_enabled: true,
        max_rounds: None,
        hex_mode: false,
        start_immediately: true,
        hidden: true,
    }
}

/// 发送目标(App 注入 / 推送刷新)。
#[derive(Debug, Clone)]
pub enum TaskTarget {
    /// 客户端模式: 该 tab 的连接写通道（目标由连接自身决定）
    Client(Sender<WireMessage>),
    /// 服务端模式: App 在客户端连接/断开时推送的目标快照
    ServerClients(Vec<(SocketAddr, Sender<WireMessage>)>),
}

/// 运行期状态(UI 侧持有, 事件泵写入)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// 预留: 心跳等后期任务初始化前的 Idle 态; 当前未构造, 保留不删
    #[allow(dead_code)]
    Idle,
    Running,
    Paused,
    /// 正常跑完(含达到轮次上限)
    Finished,
    /// 被用户停止
    Stopped,
    /// 预留: `TaskEndReason::Failed` 的信源(心跳探活/运行期编码错误等确定性失败时构造);
    /// 当前引擎对发送失败一律走「暂停保留」, 故尚未有构造点
    Failed(String),
}

/// 任务结束原因(正常完成 / 被停止 / 失败)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEndReason {
    Completed,
    Stopped,
    /// 预留信源: 一旦引擎产生确定性失败(如心跳探活超阈值)即由此投递到
    /// `TaskStatus::Failed(msg)`(见 app.rs TaskFinished 分支), UI 红色失败链路已就绪
    #[allow(dead_code)]
    Failed(String),
}

/// 任务的运行期状态快照(UI 渲染读)。
///
/// `config` 用 `Arc` 包装: 任务项可能上万条, 借 `Arc` 让 `ConnectionTabState`
/// 的 Clone 保持 O(1), 避免切 tab / 渲染期深拷贝整批消息。
#[derive(Debug, Clone)]
pub struct SendTaskState {
    pub config: Arc<SendTaskConfig>,
    pub status: TaskStatus,
    /// 当前轮已发条数
    pub sent_items: u64,
    /// N(任务项总数)
    pub total_items: u64,
    /// 已完成轮数 + 1
    pub round: u32,
    /// 暂停原因(如「连接已断开」)
    pub pause_reason: Option<String>,
}

impl SendTaskState {
    pub fn new(config: Arc<SendTaskConfig>) -> Self {
        let total_items = config.total_items() as u64;
        let status = if config.start_immediately {
            TaskStatus::Running
        } else {
            TaskStatus::Paused
        };
        Self {
            config,
            status,
            sent_items: 0,
            total_items,
            round: 1,
            pause_reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(0))
    }

    fn periodic(content: &str, hex_mode: bool) -> (Arc<SendTaskConfig>, Arc<PeriodicSource>) {
        let source = Arc::new(PeriodicSource::new(seq(), content.to_string(), hex_mode));
        let config = Arc::new(build_periodic_config(
            source.clone(),
            IntervalHandle::new(1000),
            "周期发送".to_string(),
        ));
        (config, source)
    }

    /// IntervalHandle: 写入后读到新值(引擎每轮据此重算 deadline)
    #[test]
    fn interval_handle_reads_latest_value() {
        let handle = IntervalHandle::new(1000);
        assert_eq!(handle.get(), 1000);
        handle.set(50);
        assert_eq!(handle.get(), 50);
    }

    /// 周期发送配置: 面板隐藏 + 单步 + 初始间隔来自句柄
    #[test]
    fn build_periodic_config_is_hidden_single_step() {
        let (config, _source) = periodic("a", false);
        assert!(config.hidden, "周期发送必须对面板隐藏");
        assert!(config.is_periodic());
        assert!(!config.is_timed());
        assert_eq!(config.step_count(), 1);
        assert_eq!(config.interval.get(), 1000);
        assert!(config.items().is_empty());
    }

    /// 心跳配置: 面板可见 + 定时语义 + start_immediately 决定初始状态
    #[test]
    fn build_timed_config_is_visible_timed() {
        let profile = TimedTaskProfile {
            enabled: true,
            message: "ping".to_string(),
            hex_mode: false,
            interval_ms: 2000,
        };
        let config = build_timed_config(&profile, true, "心跳".to_string());
        assert!(!config.hidden, "心跳任务应出现在面板");
        assert!(config.is_timed());
        assert_eq!(config.step_count(), 1);
        assert_eq!(config.interval.get(), 2000);

        let paused = build_timed_config(&profile, false, "心跳".to_string());
        assert_eq!(
            SendTaskState::new(Arc::new(paused)).status,
            TaskStatus::Paused,
            "start_immediately=false 应创建为暂停态"
        );
    }

    /// 周期发送渲染: 实时读取当前内容, 空内容跳过本轮(不发空包)
    #[test]
    fn periodic_render_follows_content_and_skips_empty() {
        let (config, source) = periodic("a", false);
        let rendered = config.render_step(0, &seq()).unwrap();
        assert_eq!(rendered.bytes, b"a".to_vec());
        assert!(!rendered.hex_mode);

        source.set_content("  ".to_string());
        assert!(config.render_step(0, &seq()).is_none(), "空内容应跳过");

        source.set_content("b".to_string());
        assert_eq!(config.render_step(0, &seq()).unwrap().bytes, b"b".to_vec());
    }

    /// 周期发送渲染: 模式切换实时生效; 非法 HEX 跳过本轮
    #[test]
    fn periodic_render_follows_mode_and_skips_invalid_hex() {
        let (config, source) = periodic("41 42", true);
        let rendered = config.render_step(0, &seq()).unwrap();
        assert_eq!(rendered.bytes, vec![0x41, 0x42]);
        assert!(rendered.hex_mode);

        source.set_content("4".to_string());
        assert!(config.render_step(0, &seq()).is_none(), "非法 HEX 应跳过");

        source.set_hex_mode(false);
        assert_eq!(
            config.render_step(0, &seq()).unwrap().bytes,
            b"4".to_vec(),
            "切回文本模式后按原文发送"
        );
    }
}
