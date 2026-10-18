use crate::message::{Message, MessageDirection, MessageType};
use std::sync::Arc;

pub trait MessageProcessor: Send + Sync + 'static {
    /// 处理接收到的消息
    ///
    /// `raw_data` 用 `Arc<[u8]>` 以便与规则引擎的 `RxFrame` 共享同一份帧缓冲（P-6）。
    fn process_received_message(&self, raw_data: Arc<[u8]>, message_type: MessageType) -> Message;

    /// MQTT 读路径专用：在既有处理结果上填充传输元数据（规划 §5.4）。
    ///
    /// 默认实现转调 [`process_received_message`](Self::process_received_message) 再经
    /// `Message::with_transport` 注入 topic/retain，因此 `DefaultMessageProcessor` 等
    /// 既有实现者零改动。TCP/UDP 读路径以 `topic=None, retain=false` 调用，经默认实现
    /// 还原为与 `Message::new` 完全一致的默认值，行为逐字不变。
    fn process_received_message_with_transport(
        &self,
        raw_data: Arc<[u8]>,
        message_type: MessageType,
        topic: Option<String>,
        retain: bool,
    ) -> Message {
        self.process_received_message(raw_data, message_type)
            .with_transport(topic, retain)
    }
}

/// 默认的消息处理器实现
#[derive(Clone)]
pub struct DefaultMessageProcessor;

impl MessageProcessor for DefaultMessageProcessor {
    fn process_received_message(&self, raw_data: Arc<[u8]>, message_type: MessageType) -> Message {
        Message::new(MessageDirection::Received, raw_data, message_type)
    }
}
