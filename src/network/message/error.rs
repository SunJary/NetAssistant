use thiserror::Error;

/// 消息处理相关错误
#[derive(Debug, Error)]
pub enum MessageError {
    /// 连接未建立
    #[error("连接未建立")]
    NotConnected,
    
    /// 没有可用的客户端连接
    #[error("没有可用的客户端连接")]
    NoClientsConnected,
    
    /// 客户端写入发送器不可用
    #[error("客户端写入发送器不可用")]
    WriteSenderUnavailable,
    
    /// 网络错误
    #[error("网络错误: {0}")]
    NetworkError(String),
    
    /// 无效的目标地址
    #[error("无效的目标地址: {0}")]
    InvalidAddress(String),
    
    /// 无效的十六进制格式
    #[error("无效的十六进制格式: {0}")]
    InvalidHexFormat(String),
    
    /// 标签页不存在
    #[error("标签页不存在: {0}")]
    TabNotFound(String),
}

/// 统一的错误处理函数
pub fn handle_message_error(
    error: MessageError,
    tab_id: &str,
    event_sender: &Option<tokio::sync::mpsc::UnboundedSender<crate::network::events::ConnectionEvent>>
) {
    error!("消息处理错误: {:?}", error);
    if let Some(sender) = event_sender {
        let _ = sender.send(crate::network::events::ConnectionEvent::Error(
            tab_id.to_string(),
            error.to_string(),
        ));
    }
}

use log::error;