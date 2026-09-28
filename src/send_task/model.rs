//! 发送任务数据模型(纯逻辑, 不依赖 gpui, 可无头单测)。
//!
//! 建模: 任务(Task) = 一次导入产生的一批 → 调度单位;
//! 任务项(Item) = 文件的一行 → 发送单位(进度显示为 `i / N`)。

use crate::message::MessageType;
use crate::utils::message_vars::CompiledTemplate;
use smol::channel::Sender;
use std::net::SocketAddr;
use std::sync::Arc;

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

/// 任务类型(本次仅实现逐行发送, 为后期心跳任务预留扩展点)。
#[derive(Debug, Clone)]
pub enum TaskKind {
    /// 逐行发送: 每行一条消息
    SendByLines { items: Vec<TaskItem> },
    // 后期预留(本次不实现):
    // SendHeartbeat { template: Arc<CompiledTemplate> },
}

/// 任务调度配置。
#[derive(Debug, Clone)]
pub struct SendTaskConfig {
    /// UUID
    pub id: String,
    /// 来源文件名(UI 卡片标题)
    pub name: String,
    pub kind: TaskKind,
    /// 每条消息之间的间隔(无下限)
    pub interval_ms: u64,
    /// 是否循环整批
    pub loop_enabled: bool,
    /// Some(n) 限轮次; None = 无限(仅 loop_enabled 时有意义)
    pub max_rounds: Option<u32>,
    /// 与创建时输入框模式一致
    pub hex_mode: bool,
    /// 创建后是否立即开始(否则先置 Paused)
    pub start_immediately: bool,
}

impl SendTaskConfig {
    /// 任务项列表(本次仅 SendByLines)
    pub fn items(&self) -> &[TaskItem] {
        match &self.kind {
            TaskKind::SendByLines { items } => items,
        }
    }

    /// 任务项总数 N
    pub fn total_items(&self) -> usize {
        self.items().len()
    }

    /// 发送消息类型(由创建时输入模式决定)
    pub fn message_type(&self) -> MessageType {
        if self.hex_mode {
            MessageType::Hex
        } else {
            MessageType::Text
        }
    }
}

/// 发送目标(App 注入 / 推送刷新)。
#[derive(Debug, Clone)]
pub enum TaskTarget {
    /// 客户端模式: 该 tab 的连接写通道
    Client(Sender<Vec<u8>>),
    /// 服务端模式: App 在客户端连接/断开时推送的目标快照(addr 仅用于报错定位)
    ServerClients(Vec<(SocketAddr, Sender<Vec<u8>>)>),
}

/// 运行期状态(UI 侧持有, 事件泵写入)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// 预留: 心跳等后期任务初始化前的 Idle 态; 当前 v1 未构造, 保留不删
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
    #[allow(dead_code)]
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
