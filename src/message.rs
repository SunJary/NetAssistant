use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

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

/// 消息显示模式（用于消息列表内容格式化切换）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageDisplayMode {
    /// 原始内容
    #[default]
    Normal,
    /// JSON 美化格式（2空格缩进、换行）
    JsonPretty,
    /// JSON 压缩格式（无空格无换行）
    JsonMinified,
}

impl MessageDisplayMode {
    /// 切换到下一个显示模式：Normal -> JsonPretty -> JsonMinified -> Normal
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::JsonPretty,
            Self::JsonPretty => Self::JsonMinified,
            Self::JsonMinified => Self::Normal,
        }
    }

    /// 显示标签
    pub fn label(self) -> Cow<'static, str> {
        match self {
            Self::Normal => t!("display_mode.normal"),
            Self::JsonPretty => t!("display_mode.json_pretty"),
            Self::JsonMinified => t!("display_mode.json_minified"),
        }
    }
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
            timestamp: chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S.%3f")
                .to_string(),
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
        // 幂等优化: 类型未变时跳过重算。
        // 事件泵批处理对每条消息调用此方法，而 Message::new 已算过一次内容；
        // 压测洪泛场景下避免主线程重复做 O(payload) 的内容计算。
        if self.message_type == message_type {
            return;
        }
        self.message_type = message_type;
        self.cached_content = Self::compute_content(&self.raw_data, message_type);
    }

    /// 根据显示模式重算 cached_content（从 raw_data 重新生成，保留原始字节）
    pub fn recompute_content_for_display(&mut self, mode: MessageDisplayMode) {
        // 先还原为基础内容
        let base = Self::compute_content(&self.raw_data, self.message_type);
        self.cached_content = match mode {
            MessageDisplayMode::Normal => base,
            MessageDisplayMode::JsonPretty | MessageDisplayMode::JsonMinified => {
                if self.message_type == MessageType::Text {
                    format_json_text(&base, mode)
                } else {
                    base
                }
            }
        };
    }
}

/// 对文本进行 JSON 格式化处理。
/// - `JsonPretty`：美化（2空格缩进、换行）
/// - `JsonMinified`：压缩（无空格无换行）
/// - `Normal`：原样返回
/// 解析失败时原样返回。
pub fn format_json_text(text: &str, mode: MessageDisplayMode) -> String {
    match mode {
        MessageDisplayMode::Normal => text.to_string(),
        MessageDisplayMode::JsonPretty => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.to_string()),
            Err(_) => text.to_string(),
        },
        MessageDisplayMode::JsonMinified => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => serde_json::to_string(&value).unwrap_or_else(|_| text.to_string()),
            Err(_) => text.to_string(),
        },
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

/// 默认消息列表最大保留条数（超出后丢弃最旧的消息以控制内存占用）
const DEFAULT_MAX_MESSAGES: usize = 10000;

/// 消息列表状态
#[derive(Debug, Clone)]
pub struct MessageListState {
    /// 使用 Arc 包装，渲染时 clone 仅增加引用计数（O(1)），避免每帧克隆整个 Vec。
    /// 写入时通过 `Arc::make_mut` 获取可变引用，refcount==1 时零拷贝。
    pub messages: Arc<Vec<Message>>,
    pub total_sent: usize,
    pub total_received: usize,
    /// 消息列表最大保留条数，0 表示不限制。超出后丢弃最旧的消息。
    pub max_messages: usize,
}

impl Default for MessageListState {
    fn default() -> Self {
        Self {
            messages: Arc::new(Vec::new()),
            total_sent: 0,
            total_received: 0,
            max_messages: DEFAULT_MAX_MESSAGES,
        }
    }
}

impl MessageListState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一条消息，返回因超出上限而从列表头部丢弃的消息条数。
    pub fn add_message(&mut self, message: Message) -> usize {
        match message.direction {
            MessageDirection::Sent => self.total_sent += 1,
            MessageDirection::Received => self.total_received += 1,
        }
        let messages = Arc::make_mut(&mut self.messages);
        let dropped = if self.max_messages > 0 && messages.len() >= self.max_messages {
            // 分批丢弃最旧的消息以分摊开销（10% 或至少 1 条）
            let drop_count = (self.max_messages / 30).max(1).min(messages.len());
            messages.drain(0..drop_count);
            drop_count
        } else {
            0
        };
        messages.push(message);
        dropped
    }

    /// 批量添加消息，返回因超出上限而从列表头部丢弃的消息条数。
    /// 相比逐条 add_message，仅重建一次 Arc，显著降低高并发消息洪泛下的开销。
    pub fn add_messages_batch(&mut self, new_messages: Vec<Message>) -> usize {
        if new_messages.is_empty() {
            return 0;
        }
        for message in &new_messages {
            match message.direction {
                MessageDirection::Sent => self.total_sent += 1,
                MessageDirection::Received => self.total_received += 1,
            }
        }
        let messages = Arc::make_mut(&mut self.messages);
        messages.reserve(new_messages.len());
        let mut dropped = 0;
        for message in new_messages {
            if self.max_messages > 0 && messages.len() >= self.max_messages {
                let drop_count = (self.max_messages / 30).max(1).min(messages.len());
                messages.drain(0..drop_count);
                dropped += drop_count;
            }
            messages.push(message);
        }
        dropped
    }

    /// 累计消息总数（含已丢弃的）
    pub fn total_messages(&self) -> usize {
        self.total_sent + self.total_received
    }

    pub fn clear_messages(&mut self) {
        self.messages = Arc::new(Vec::new());
        self.total_sent = 0;
        self.total_received = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{Message, MessageDirection, MessageListState, MessageType};
    use std::sync::Arc;

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
    fn test_set_message_type_idempotent() {
        // 同类型重复设置: 幂等跳过重算，内容保持不变
        let mut message =
            Message::new(MessageDirection::Sent, b"Hello".to_vec(), MessageType::Text);
        message.set_message_type(MessageType::Text);
        assert_eq!(message.get_content_by_type(), "Hello");

        // 切换类型: 仍正常重算
        message.set_message_type(MessageType::Hex);
        assert_eq!(message.get_content_by_type(), "48 65 6C 6C 6F");

        // 切回原类型: 重算回文本内容
        message.set_message_type(MessageType::Text);
        assert_eq!(message.get_content_by_type(), "Hello");
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

    #[test]
    fn test_message_list_max_limit() {
        let mut state = MessageListState::new();
        state.max_messages = 5;

        // 添加 5 条消息（未超限，不丢弃）
        for i in 0..5u8 {
            let dropped = state.add_message(Message::new(
                MessageDirection::Received,
                vec![i],
                MessageType::Hex,
            ));
            assert_eq!(dropped, 0);
        }
        assert_eq!(state.messages.len(), 5);
        assert_eq!(state.total_messages(), 5);

        // 添加第 6 条，触发丢弃（max_messages/10=0，至少丢弃 1 条）
        let dropped = state.add_message(Message::new(
            MessageDirection::Received,
            vec![5],
            MessageType::Hex,
        ));
        assert_eq!(dropped, 1);
        assert_eq!(state.messages.len(), 5);
        // 最旧的消息（vec![0]）已被丢弃
        assert_eq!(state.messages[0].raw_data, vec![1]);
        assert_eq!(state.messages.last().unwrap().raw_data, vec![5]);
        // 累计总数仍为 6（含已丢弃的）
        assert_eq!(state.total_messages(), 6);

        // 再添加一条，继续丢弃最旧的
        let dropped = state.add_message(Message::new(
            MessageDirection::Sent,
            vec![6],
            MessageType::Hex,
        ));
        assert_eq!(dropped, 1);
        assert_eq!(state.messages.len(), 5);
        assert_eq!(state.messages[0].raw_data, vec![2]);
        // 累计：5 接收 + 1 发送 = 6，加新发送 = 7
        assert_eq!(state.total_messages(), 7);
    }

    #[test]
    fn test_message_list_unlimited() {
        let mut state = MessageListState::new();
        state.max_messages = 0; // 不限制

        for i in 0..100u8 {
            let dropped = state.add_message(Message::new(
                MessageDirection::Received,
                vec![i],
                MessageType::Hex,
            ));
            assert_eq!(dropped, 0);
        }
        assert_eq!(state.messages.len(), 100);
        assert_eq!(state.total_messages(), 100);
    }

    #[test]
    fn test_message_list_default_max() {
        // 默认上限应为 10000
        let state = MessageListState::new();
        assert_eq!(state.max_messages, 10000);
    }

    #[test]
    fn test_add_messages_batch() {
        let mut state = MessageListState::new();
        state.max_messages = 5;

        // 批量添加 3 条消息（未超限，不丢弃）
        let batch: Vec<Message> = (0..3u8)
            .map(|i| Message::new(MessageDirection::Received, vec![i], MessageType::Hex))
            .collect();
        let dropped = state.add_messages_batch(batch);
        assert_eq!(dropped, 0);
        assert_eq!(state.messages.len(), 3);
        assert_eq!(state.total_received, 3);
        assert_eq!(state.messages[0].raw_data, vec![0]);
        assert_eq!(state.messages[2].raw_data, vec![2]);

        // 批量添加 3 条消息，触发丢弃（max=5, 当前=3, 加3=6 超1）
        let batch: Vec<Message> = (3..6u8)
            .map(|i| Message::new(MessageDirection::Sent, vec![i], MessageType::Hex))
            .collect();
        let dropped = state.add_messages_batch(batch);
        assert_eq!(dropped, 1);
        assert_eq!(state.messages.len(), 5);
        // 最旧的 vec![0] 已被丢弃
        assert_eq!(state.messages[0].raw_data, vec![1]);
        assert_eq!(state.messages.last().unwrap().raw_data, vec![5]);
        // 累计：3 接收 + 3 发送 = 6
        assert_eq!(state.total_messages(), 6);
    }

    #[test]
    fn test_add_messages_batch_empty() {
        let mut state = MessageListState::new();
        let dropped = state.add_messages_batch(Vec::new());
        assert_eq!(dropped, 0);
        assert_eq!(state.messages.len(), 0);
    }

    #[test]
    fn test_add_messages_batch_unlimited() {
        let mut state = MessageListState::new();
        state.max_messages = 0;

        let batch: Vec<Message> = (0..100u8)
            .map(|i| Message::new(MessageDirection::Received, vec![i], MessageType::Hex))
            .collect();
        let dropped = state.add_messages_batch(batch);
        assert_eq!(dropped, 0);
        assert_eq!(state.messages.len(), 100);
        assert_eq!(state.total_messages(), 100);
    }

    #[test]
    fn test_messages_arc_zero_clone_on_render() {
        // 验证 Arc 化后渲染路径的 clone 仅增加引用计数
        let mut state = MessageListState::new();
        state.add_message(Message::new(
            MessageDirection::Received,
            b"test".to_vec(),
            MessageType::Text,
        ));

        // 模拟渲染时的 clone（Arc clone, O(1)）
        let render_ref1 = state.messages.clone();
        let render_ref2 = state.messages.clone();
        assert_eq!(Arc::strong_count(&state.messages), 3);

        // 通过 Arc::make_mut 写入时，由于 refcount>1 会触发克隆
        // 但渲染引用释放后（refcount==1），写入零拷贝
        drop(render_ref1);
        drop(render_ref2);
        assert_eq!(Arc::strong_count(&state.messages), 1);

        // 此时写入应零拷贝
        let ptr_before = Arc::as_ptr(&state.messages);
        state.add_message(Message::new(
            MessageDirection::Received,
            b"test2".to_vec(),
            MessageType::Text,
        ));
        let ptr_after = Arc::as_ptr(&state.messages);
        // refcount==1 时 Arc::make_mut 原地修改，指针不变
        assert_eq!(ptr_before, ptr_after);
    }
}
