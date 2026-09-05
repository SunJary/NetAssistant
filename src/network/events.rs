use crate::config::connection::{AutoReplyConfig, DecoderConfig};
use crate::message::{Message, MessageDirection, MessageType};
use smol::channel::Sender;
use std::net::SocketAddr;
use std::sync::Arc;

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

impl ConnectionEvent {
    /// 构造自动回复的 Sent 消息事件(用于 UI 展示; 消息类型由 UI 侧按 tab 的输入模式校正)
    pub fn auto_reply_sent(connection_id: &str, content: Vec<u8>, source: &str) -> Self {
        ConnectionEvent::MessageReceived(
            connection_id.to_string(),
            Message::new(MessageDirection::Sent, content, MessageType::Text)
                .with_source(source.to_string()),
        )
    }
}
