use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// 源地址是否为非预期地址（如UDP广播场景下，回复来自非目标地址）
    #[serde(default)]
    pub source_unexpected: bool,
    #[serde(default = "default_cached_content")]
    cached_content: String,
}

fn default_cached_content() -> String {
    String::new()
}

impl Message {
    pub fn new(direction: MessageDirection, raw_data: Vec<u8>, message_type: MessageType) -> Self {
        let cached_content = Self::compute_content(&raw_data, message_type);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f").to_string(),
            direction,
            message_type,
            raw_data,
            source: None,
            source_unexpected: false,
            cached_content,
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    /// 设置来源并标记是否为非预期地址（IP部分与 expected_host 不匹配时为 true）
    pub fn with_unexpected_source(mut self, source: String, expected_host: &str) -> Self {
        let is_unexpected = match source.split(':').next() {
            Some(source_ip) => source_ip != expected_host,
            None => false,
        };
        self.source = Some(source);
        self.source_unexpected = is_unexpected;
        self
    }

    fn compute_content(raw_data: &[u8], message_type: MessageType) -> String {
        match message_type {
            MessageType::Text => match String::from_utf8(raw_data.to_vec()) {
                Ok(text) => text,
                Err(_) => "[非UTF-8数据]".to_string(),
            },
            MessageType::Hex => raw_data
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<String>>()
                .join(" "),
        }
    }

    pub fn get_content_by_type(&self) -> &str {
        &self.cached_content
    }

    pub fn set_message_type(&mut self, message_type: MessageType) {
        self.message_type = message_type;
        self.cached_content = Self::compute_content(&self.raw_data, message_type);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteItem {
    pub id: String,
    pub content: String,
    pub message_type: MessageType,
    pub remark: String,
    pub created_at: String,
}

impl FavoriteItem {
    pub fn new(content: String, message_type: MessageType, remark: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content,
            message_type,
            remark,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

pub type FavoritesMap = HashMap<String, Vec<FavoriteItem>>;

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