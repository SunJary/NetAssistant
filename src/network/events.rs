use crate::config::connection::{AutoReplyConfig, DecoderConfig};
use crate::message::Message;
use smol::channel::Sender;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
    ClientWriteSenderReady(String, Sender<Vec<u8>>),
    /// 服务端客户端连接
    ServerClientConnected(String, SocketAddr, Sender<Vec<u8>>),
    /// 服务端客户端断开
    ServerClientDisconnected(String, SocketAddr),
    /// 周期发送文本消息
    PeriodicSend(String, String),
    /// 周期发送字节消息
    PeriodicSendBytes(String, Vec<u8>, String),
    /// 客户端解码器控制发送器就绪(用于运行时下发解码器配置, 无需重连)
    DecoderControlSenderReady(String, Sender<DecoderConfig>),
    /// 服务端某客户端的解码器控制发送器就绪
    ServerDecoderControlSenderReady(String, SocketAddr, Sender<DecoderConfig>),
    /// 服务端自动回复共享状态就绪(UI 运行时下发启用开关与回复内容)
    ServerAutoReplyStateReady(String, Arc<AutoReplyConfig>),
}
