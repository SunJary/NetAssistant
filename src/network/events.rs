use crate::config::connection::DecoderConfig;
use crate::message::Message;
use crate::reply::ReplyRulesStore;
use crate::send_task::model::{TaskEndReason, TaskStatus};
use smol::channel::Sender;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 写入通道里的一个待发项。
///
/// 绝大多数发送走"继承"路径：由连接级「结尾追加字符」（trailer）设置决定是否追加。
/// **回复规则**可以要求"原样输出"（二进制协议必需）—— 若让 Modbus 应答继承 CRLF，
/// CRC 后面会多出 2 字节，协议直接错且症状隐晦（对端只报校验错）。
///
/// 为什么用"通道里带标记"而不是给 encoder 加 bypass 开关：连接的 encoder 对
/// **所有**写入生效（手动发送、发送任务、规则应答共用一条写通道），加一个
/// 跨线程的 bypass 标志会让并发的手动发送被误 bypass —— 那是不可接受的竞态。
/// 把意图随数据一起传递，既不改变既有通道语义，也没有竞态。
///
/// `target` 只有 UDP 用得上：TCP 连接的写通道天然绑定在一条 socket 上，
/// 而 UDP 的应答必须发回**数据报的真实来源**（广播发现场景下通常是设备自己的 IP，
/// 而不是广播地址），因此目标地址需要随数据传递。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireMessage {
    data: Vec<u8>,
    /// true = 字节已按规则指定的方式处理好，发送点**不得**再追加连接 trailer
    finalized: bool,
    /// UDP 显式目标地址；None = 使用连接/会话的默认目标
    target: Option<std::net::SocketAddr>,
}

impl WireMessage {
    /// 普通发送：由连接 trailer 设置决定是否追加结尾（与既有行为逐字一致）
    pub fn inherit(data: Vec<u8>) -> Self {
        Self {
            data,
            finalized: false,
            target: None,
        }
    }

    /// 普通发送 + 指定目标（UDP 回发源地址场景）
    pub fn inherit_to(data: Vec<u8>, target: std::net::SocketAddr) -> Self {
        Self {
            data,
            finalized: false,
            target: Some(target),
        }
    }

    /// 规则应答：`trailer` 为 None 表示原样输出，否则追加指定结尾
    pub fn bypass(data: Vec<u8>, trailer: Option<crate::config::connection::TrailerKind>) -> Self {
        Self::bypass_to(data, trailer, None)
    }

    /// 规则应答 + 指定目标（UDP）
    pub fn bypass_to(
        data: Vec<u8>,
        trailer: Option<crate::config::connection::TrailerKind>,
        target: Option<std::net::SocketAddr>,
    ) -> Self {
        use crate::config::connection::apply_trailer;
        let data = match trailer {
            None => data,
            Some(kind) => apply_trailer(&data, kind).into_owned(),
        };
        Self {
            data,
            finalized: true,
            target,
        }
    }

    /// 最终要写到 socket 上的字节
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn bypasses_trailer(&self) -> bool {
        self.finalized
    }

    /// UDP 显式目标地址（None = 用会话默认目标）
    pub fn target(&self) -> Option<std::net::SocketAddr> {
        self.target
    }
}

/// 既有发送点统一投递 `Vec<u8>`：默认继承连接 trailer（零改动语义）
impl From<Vec<u8>> for WireMessage {
    fn from(data: Vec<u8>) -> Self {
        Self::inherit(data)
    }
}

impl AsRef<[u8]> for WireMessage {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl std::ops::Deref for WireMessage {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// 一批接收到的消息(源头聚合)。
///
/// 洪泛压测下, 一次 read/recv 循环解码出的所有帧聚合为一个事件,
/// 使事件数量与「消息条数」解耦, 避免千万级事件对象进入通道积压。
/// 自动回复的 Sent 方向消息同样聚合到本批(避免开启自动回复压测时的二次洪泛)。
#[derive(Debug, Default)]
pub struct ReceivedBatch {
    /// 明细: 全量模式( SAMPLE_KEEP=0 )下为整批全部消息; 抽样模式下仅保留最新样本; 纯计数批可为空
    pub messages: Vec<Message>,
    /// 自动回复产生的 Sent 方向明细(UI 合并展示)
    pub sent_messages: Vec<Message>,
    /// 本批消息总条数(含未保留明细的, 仅接收方向)
    pub count: u64,
    /// 本批总字节数(仅接收方向)
    pub bytes: u64,
}

impl ReceivedBatch {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            messages: Vec::with_capacity(cap),
            sent_messages: Vec::new(),
            count: 0,
            bytes: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        // 明细非空即视为有内容: 极端情况下 count 与明细可能不同步, 以明细为准
        self.count == 0 && self.messages.is_empty() && self.sent_messages.is_empty()
    }
}

/// 网络层精确计数器(每 tab 一份, 创建连接时生成并注入网络层实现)。
///
/// 与显示完全解耦: 解码出一条消息立即原子累加(无锁, ns 级),
/// 洪泛千万条也不漏计; UI 每拍读取快照同步展示。
#[derive(Clone, Default)]
pub struct NetCounters(Arc<CountersInner>);

#[derive(Default)]
struct CountersInner {
    received: AtomicU64,
    sent: AtomicU64,
    received_bytes: AtomicU64,
    sent_bytes: AtomicU64,
}

impl NetCounters {
    pub fn add_received(&self, n: u64) {
        self.0.received.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_sent(&self, n: u64) {
        self.0.sent.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_received_bytes(&self, n: u64) {
        self.0.received_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_sent_bytes(&self, n: u64) {
        self.0.sent_bytes.fetch_add(n, Ordering::Relaxed);
    }

    /// 读取快照(received, sent, received_bytes, sent_bytes)
    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.0.received.load(Ordering::Relaxed),
            self.0.sent.load(Ordering::Relaxed),
            self.0.received_bytes.load(Ordering::Relaxed),
            self.0.sent_bytes.load(Ordering::Relaxed),
        )
    }

    /// 计数归零(UI「清空消息」时调用, 否则下一拍同步会把显示计数覆盖回原值)。
    ///
    /// 与网络线程的 add_* 并发时可能丢失极少量增量, 对"清空"语义可接受。
    pub fn reset(&self) {
        self.0.received.store(0, Ordering::Relaxed);
        self.0.sent.store(0, Ordering::Relaxed);
        self.0.received_bytes.store(0, Ordering::Relaxed);
        self.0.sent_bytes.store(0, Ordering::Relaxed);
    }
}

/// 连接事件枚举，用于在网络线程和UI线程之间传递信息
#[derive(Debug)]
pub enum ConnectionEvent {
    /// 客户端连接成功(携带实际生效的本地端点, 如 UDP 自动分配的临时端口)
    Connected(String, SocketAddr),
    /// 客户端或服务端连接断开
    Disconnected(String),
    /// 服务端开始监听
    Listening(String),
    /// 错误事件
    Error(String, String),
    /// 收到消息
    MessageReceived(String, Message),
    /// 收到一批消息(洪泛压测聚合路径, 高频消息全部走此事件)
    MessagesReceived(String, ReceivedBatch),
    /// 客户端写入发送器准备就绪
    ///
    /// 通道携带 `WireMessage` 而非裸字节：回复规则可能要求"原样输出"（绕过连接 trailer），
    /// 这个意图必须与数据一起传递（见 `WireMessage` 的说明）。
    /// 既有发送点直接 `send(vec)` 仍然可用（`From<Vec<u8>>` 默认继承 trailer）。
    ClientWriteSenderReady(String, Sender<WireMessage>),
    /// 服务端客户端连接
    ServerClientConnected(String, SocketAddr, Sender<WireMessage>),
    /// 服务端客户端断开
    ServerClientDisconnected(String, SocketAddr),
    /// 客户端解码器控制发送器就绪(用于运行时下发解码器配置, 无需重连)
    DecoderControlSenderReady(String, Sender<DecoderConfig>),
    /// 服务端某客户端的解码器控制发送器就绪
    ServerDecoderControlSenderReady(String, SocketAddr, Sender<DecoderConfig>),
    /// 回复规则集共享状态就绪(UI 运行时下发整表；服务端多客户端共享同一 store)
    ///
    /// 客户端侧不需要此事件：客户端是 1:1，在 `TcpClient::new` / `UdpClient::new`
    /// 构造时直接注入 store（决策 D-12），少一次事件往返。
    ReplyRulesStoreReady(String, Arc<ReplyRulesStore>),
    /// 发送任务进度(节流上报; UI 只刷新展示)
    TaskProgress {
        tab_id: String,
        task_id: String,
        sent_items: u64,
        total_items: u64,
        round: u32,
        status: TaskStatus,
        pause_reason: Option<String>,
    },
    /// 发送任务结束(正常完成 / 被停止 / 失败原因)
    TaskFinished {
        tab_id: String,
        task_id: String,
        reason: TaskEndReason,
    },
}
