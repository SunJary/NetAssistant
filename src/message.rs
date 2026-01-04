use serde::{Deserialize, Serialize};
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

/// 消息类型
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
    pub content: String,
    pub length: usize,
    pub source: Option<String>,
}

impl Message {
    pub fn new(direction: MessageDirection, content: String, message_type: MessageType) -> Self {
        let length = content.len();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            direction,
            message_type,
            content,
            length,
            source: None,
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }
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
}
