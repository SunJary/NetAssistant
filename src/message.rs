use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::fmt;

/// 消息方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection {
    Sent,
    Received,
}

impl fmt::Display for MessageDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageDirection::Sent => write!(f, "发送"),
            MessageDirection::Received => write!(f, "接收"),
        }
    }
}

/// 消息类型（用于标识发送时的模式）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Text,
    Hex,
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageType::Text => write!(f, "文本"),
            MessageType::Hex => write!(f, "十六进制"),
        }
    }
}

/// 单条消息记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub timestamp: String,
    pub direction: MessageDirection,
    pub message_type: MessageType,
    pub raw_data: Vec<u8>,
    pub source: Option<String>,
    #[serde(skip)]
    pub message_height: Cell<Option<f32>>, // 使用Cell实现内部可变性
    #[serde(skip)]
    pub bubble_width: Cell<Option<f32>>,    // 用于检测宽度变化
}

impl Message {
    pub fn new(direction: MessageDirection, raw_data: Vec<u8>, message_type: MessageType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            direction,
            message_type,
            raw_data,
            source: None,
            message_height: Cell::new(None), // 初始化为None
            bubble_width: Cell::new(None),   // 初始化为None
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    pub fn get_content_by_type(&self) -> String {
        match self.message_type {
            MessageType::Text => match String::from_utf8(self.raw_data.clone()) {
                Ok(text) => text,
                Err(_) => "[非UTF-8数据]".to_string(),
            },
            MessageType::Hex => self
                .raw_data
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<String>>()
                .join(" "),
        }
    }

    // 移除了calculate_content_height方法，因为在渲染时无法修改消息对象
}

/// 消息列表状态
#[derive(Debug, Clone, Default)]
pub struct MessageListState {
    pub messages: Vec<Message>,
    pub total_sent: usize,
    pub total_received: usize,
}

impl MessageListState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_message(&mut self, message: Message) {
        match message.direction {
            MessageDirection::Sent => self.total_sent += 1,
            MessageDirection::Received => self.total_received += 1,
        }
        self.messages.push(message);
    }

    pub fn total_messages(&self) -> usize {
        self.messages.len()
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.total_sent = 0;
        self.total_received = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{Message, MessageDirection, MessageListState, MessageType};

    #[test]
    fn test_message_creation() {
        let text_message = Message::new(
            MessageDirection::Sent,
            b"Hello World".to_vec(),
            MessageType::Text,
        );
        assert_eq!(text_message.direction, MessageDirection::Sent);
        assert_eq!(text_message.raw_data, b"Hello World".to_vec());
        assert_eq!(text_message.message_type, MessageType::Text);
        assert!(text_message.id.len() > 0);
        assert!(text_message.timestamp.len() > 0);
        assert_eq!(text_message.source, None);

        let hex_message = Message::new(
            MessageDirection::Received,
            b"48656c6c6f".to_vec(),
            MessageType::Hex,
        );
        assert_eq!(hex_message.direction, MessageDirection::Received);
        assert_eq!(hex_message.raw_data, b"48656c6c6f".to_vec());
        assert_eq!(hex_message.message_type, MessageType::Hex);
        assert!(hex_message.id.len() > 0);
        assert!(hex_message.timestamp.len() > 0);
        assert_eq!(hex_message.source, None);
    }

    #[test]
    fn test_message_with_source() {
        let message = Message::new(MessageDirection::Sent, b"Test".to_vec(), MessageType::Text)
            .with_source("127.0.0.1:1234".to_string());

        assert_eq!(message.source, Some("127.0.0.1:1234".to_string()));
    }

    #[test]
    fn test_message_list_state() {
        let mut state = MessageListState::new();

        assert_eq!(state.messages.len(), 0);
        assert_eq!(state.total_sent, 0);
        assert_eq!(state.total_received, 0);
        assert_eq!(state.total_messages(), 0);

        let sent_message = Message::new(
            MessageDirection::Sent,
            b"Sent message".to_vec(),
            MessageType::Text,
        );
        state.add_message(sent_message);

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.total_sent, 1);
        assert_eq!(state.total_received, 0);
        assert_eq!(state.total_messages(), 1);

        let received_message = Message::new(
            MessageDirection::Received,
            b"Received message".to_vec(),
            MessageType::Text,
        );
        state.add_message(received_message);

        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.total_sent, 1);
        assert_eq!(state.total_received, 1);
        assert_eq!(state.total_messages(), 2);
    }
}