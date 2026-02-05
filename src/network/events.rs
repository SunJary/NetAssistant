use std::net::SocketAddr;
use tokio::sync::mpsc;
use crate::message::Message;

/// 连接事件枚举，用于在网络线程和UI线程之间传递信息
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// 客户端连接成功
    Connected(String),
    /// 客户端或服务端连接断开
    Disconnected(String),
    /// 服务端开始监听
    Listening(String),
    /// 错误事件
    Error(String, String),
    /// 收到消息
    MessageReceived(String, Message),
    /// 客户端写入发送器准备就绪
    ClientWriteSenderReady(String, mpsc::UnboundedSender<Vec<u8>>),
    /// 服务端客户端连接
    ServerClientConnected(String, SocketAddr, mpsc::UnboundedSender<Vec<u8>>),
    /// 服务端客户端断开
    ServerClientDisconnected(String, SocketAddr),
    /// 周期发送文本消息
    PeriodicSend(String, String),
    /// 周期发送字节消息
    PeriodicSendBytes(String, Vec<u8>, String),
}