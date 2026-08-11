use crate::config::connection::DecoderConfig;
use crate::message::Message;
use smol::channel::Sender;
use std::net::SocketAddr;

/// 连接事件枚举，用于在网络线程和UI线程之间传递信息
#[derive(Debug)]
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
}
