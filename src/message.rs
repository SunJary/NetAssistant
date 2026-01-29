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

/// 显示模式（用于控制消息显示格式）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Text,
    Hex,
}

impl fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayMode::Text => write!(f, "文本"),
            DisplayMode::Hex => write!(f, "十六进制"),
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
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    pub fn get_display_content(&self, mode: DisplayMode) -> String {
        match mode {
            DisplayMode::Text => match String::from_utf8(self.raw_data.clone()) {
                Ok(text) => text,
                Err(_) => "[非UTF-8数据]".to_string(),
            },
            DisplayMode::Hex => self
                .raw_data
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<String>>()
                .join(" "),
        }
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
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<String>>()
                .join(" "),
        }
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

#[cfg(test)]
mod tests {
    use super::{DisplayMode, Message, MessageDirection, MessageListState, MessageType};

    #[test]
    /// 测试消息创建功能
    /// 包括文本消息和十六进制消息的创建
    fn test_message_creation() {
        // 测试创建文本消息
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

        // 测试创建十六进制消息
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
    /// 测试为消息添加源信息
    fn test_message_with_source() {
        let message = Message::new(MessageDirection::Sent, b"Test".to_vec(), MessageType::Text)
            .with_source("127.0.0.1:1234".to_string());

        assert_eq!(message.source, Some("127.0.0.1:1234".to_string()));
    }

    #[test]
    /// 测试消息的显示内容转换
    /// 包括文本模式和十六进制模式的显示
    fn test_message_get_display_content() {
        // 测试文本模式
        let text_message = Message::new(
            MessageDirection::Sent,
            b"Hello World".to_vec(),
            MessageType::Text,
        );
        assert_eq!(
            text_message.get_display_content(DisplayMode::Text),
            "Hello World"
        );

        // 测试十六进制模式
        let hex_message = Message::new(
            MessageDirection::Received,
            b"Hello".to_vec(),
            MessageType::Hex,
        );
        assert_eq!(
            hex_message.get_display_content(DisplayMode::Hex),
            "48 65 6c 6c 6f"
        );
    }

    #[test]
    /// 测试消息列表状态管理
    /// 包括消息的添加、统计和总数计算
    fn test_message_list_state() {
        let mut state = MessageListState::new();

        // 测试初始状态
        assert_eq!(state.messages.len(), 0);
        assert_eq!(state.total_sent, 0);
        assert_eq!(state.total_received, 0);
        assert_eq!(state.total_messages(), 0);

        // 添加发送消息
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

        // 添加接收消息
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
