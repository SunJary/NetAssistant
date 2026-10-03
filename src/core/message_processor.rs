use crate::message::{Message, MessageDirection, MessageType};
use std::sync::Arc;

pub trait MessageProcessor: Send + Sync + 'static {
    /// 处理接收到的消息
    ///
    /// `raw_data` 用 `Arc<[u8]>` 以便与规则引擎的 `RxFrame` 共享同一份帧缓冲（P-6）。
    fn process_received_message(&self, raw_data: Arc<[u8]>, message_type: MessageType) -> Message;
}

/// 默认的消息处理器实现
#[derive(Clone)]
pub struct DefaultMessageProcessor;

impl MessageProcessor for DefaultMessageProcessor {
    fn process_received_message(&self, raw_data: Arc<[u8]>, message_type: MessageType) -> Message {
        Message::new(MessageDirection::Received, raw_data, message_type)
    }
}
