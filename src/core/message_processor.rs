use crate::message::{Message, MessageDirection, MessageType};

pub trait MessageProcessor: Send + Sync + 'static {
    /// 处理接收到的消息
    fn process_received_message(&self, raw_data: Vec<u8>, message_type: MessageType) -> Message;
}

/// 默认的消息处理器实现
#[derive(Clone)]
pub struct DefaultMessageProcessor;



impl MessageProcessor for DefaultMessageProcessor {
    fn process_received_message(&self, raw_data: Vec<u8>, message_type: MessageType) -> Message {
        Message::new(
            MessageDirection::Received,
            raw_data,
            message_type
        )
    }
}
