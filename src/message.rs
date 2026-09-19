use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

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
///
/// Clone 为手动实现(display_cache 不可派生克隆,且克隆时重置缓存)
#[derive(Debug, Serialize, Deserialize)]
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
    /// 显示格式化缓存(惰性): Some((模式, 格式化后的内容))。
    /// 切换显示模式时不再全量重算(toggle 为 O(1)),渲染可见项时按需计算并填充。
    /// Clone 时重置为 None(日志/导出克隆走基础内容,无需携带缓存)。
    #[serde(skip)]
    display_cache: Mutex<Option<(MessageDisplayMode, String)>>,
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
            display_cache: Mutex::new(None),
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
        // 基础内容已变化,显示缓存失效
        if let Ok(mut cache) = self.display_cache.lock() {
            *cache = None;
        }
    }

    /// 获取当前显示模式下的内容（惰性计算并缓存）
    ///
    /// Normal 模式直接返回基础内容;JSON 美化/压缩模式首次调用时从 raw_data
    /// 计算并写入 display_cache,后续渲染命中缓存。仅可见项产生计算开销,
    /// 1 万条消息的列表切换模式为 O(1) 而非 O(全部)。
    pub fn display_content(&self, mode: MessageDisplayMode) -> String {
        if mode == MessageDisplayMode::Normal {
            return self.cached_content.clone();
        }
        if let Ok(cache) = self.display_cache.lock() {
            if let Some((cached_mode, content)) = cache.as_ref() {
                if *cached_mode == mode {
                    return content.clone();
                }
            }
        }
        let base = Self::compute_content(&self.raw_data, self.message_type);
        let formatted = match mode {
            MessageDisplayMode::JsonPretty | MessageDisplayMode::JsonMinified
                if self.message_type == MessageType::Text =>
            {
                format_json_text(&base, mode)
            }
            _ => base,
        };
        if let Ok(mut cache) = self.display_cache.lock() {
            *cache = Some((mode, formatted.clone()));
        }
        formatted
    }
}

// 手动实现 Clone: display_cache(Mutex)不可派生克隆,且克隆件(日志/导出)无需携带缓存
impl Clone for Message {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            timestamp: self.timestamp.clone(),
            direction: self.direction,
            message_type: self.message_type,
            raw_data: self.raw_data.clone(),
            source: self.source.clone(),
            source_unexpected: self.source_unexpected,
            cached_content: self.cached_content.clone(),
            display_cache: Mutex::new(None),
        }
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

/// 默认「保留最后 N 条」条数（0 = 不限制）。
pub const DEFAULT_KEEP_LAST: usize = 10_000;

/// 「保留最后 N 条」的上限，防止手误输入超大值（10_000_000 条约 2.4 GB）。
pub const MAX_KEEP_LAST: usize = 10_000_000;

/// 触发裁剪的滞后量：积累到 keep_last + slack 才裁回 keep_last，
/// 把一次 O(N) 的 memmove 摊薄到 slack 次追加上。
fn evict_slack(keep_last: usize) -> usize {
    (keep_last / 30).max(1)
}

/// 消息列表状态
///
/// 尾部追加 + 可选按条数淘汰：`keep_last` 为 0 时不淘汰，否则只保留最后
/// `keep_last` 条。不变式：仅当标签页的「自动滚动」开启时才可能非 0，
/// 这样头部淘汰只会发生在视口跟随尾部、用户不可见的时刻。
#[derive(Debug, Clone)]
pub struct MessageListState {
    /// 使用 Arc 包装，渲染时 clone 仅增加引用计数（O(1)），避免每帧克隆整个 Vec。
    /// 写入时通过 `Arc::make_mut` 获取可变引用，refcount==1 时零拷贝。
    pub messages: Arc<Vec<Message>>,
    pub total_sent: usize,
    pub total_received: usize,
    /// 保留最后 N 条；0 = 不限制（不淘汰）。
    pub keep_last: usize,
}

impl Default for MessageListState {
    fn default() -> Self {
        Self {
            messages: Arc::new(Vec::new()),
            total_sent: 0,
            total_received: 0,
            keep_last: DEFAULT_KEEP_LAST,
        }
    }
}

impl MessageListState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 精确裁剪到 keep_last；返回被丢弃条数。keep_last == 0 时不动。
    ///
    /// 注意：无需淘汰时必须在 `Arc::make_mut` 之前返回。导出等路径会 clone 这个 Arc
    /// （refcount>1），此时 make_mut 会整体深拷贝整个 Vec，白白付出几十~几百 MB。
    fn evict(&mut self) -> usize {
        let keep_last = self.keep_last;
        let len = self.messages.len();
        if keep_last == 0 || len <= keep_last {
            return 0;
        }
        let dropped = len - keep_last;
        Arc::make_mut(&mut self.messages).drain(0..dropped);
        dropped
    }

    /// 滞后裁剪，常规追加路径使用。未超过阈值时不产生任何开销。
    fn evict_with_hysteresis(&mut self) -> usize {
        if self.keep_last == 0
            || self.messages.len() <= self.keep_last + evict_slack(self.keep_last)
        {
            return 0;
        }
        self.evict()
    }

    /// 设置保留条数并**立即**精确裁剪（不走滞后），返回被丢弃条数。
    ///
    /// 用户显式改设置时要求立刻看到效果（内存立刻释放、条数立刻收敛）。
    pub fn set_keep_last(&mut self, keep_last: usize) -> usize {
        self.keep_last = keep_last;
        self.evict()
    }

    /// 添加一条消息，返回被淘汰条数。
    pub fn add_message(&mut self, message: Message) -> usize {
        match message.direction {
            MessageDirection::Sent => self.total_sent += 1,
            MessageDirection::Received => self.total_received += 1,
        }
        Arc::make_mut(&mut self.messages).push(message);
        self.evict_with_hysteresis()
    }

    /// 批量添加消息，返回被淘汰条数。相比逐条 add_message，仅重建一次 Arc 且只做一次
    /// reserve，显著降低高并发消息洪泛下的开销。
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
        messages.extend(new_messages);
        self.evict_with_hysteresis()
    }

    /// 用网络层精确计数覆盖显示计数(接收/发送 total)。
    ///
    /// 洪泛压测下 UI 侧按批次累加可能漏计(通道限流时), 每拍以网络层原子计数快照为准覆盖,
    /// 使展示计数与网络层真实收发一致。
    pub fn sync_totals(&mut self, total_received: u64, total_sent: u64) {
        self.total_received = total_received as usize;
        self.total_sent = total_sent as usize;
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
    fn test_display_content_lazy_cache() {
        use super::MessageDisplayMode;

        let json = br#"{"name":"test","value":1}"#.to_vec();
        let message = Message::new(MessageDirection::Received, json, MessageType::Text);

        // Normal 模式: 直接返回基础内容
        assert_eq!(
            message.display_content(MessageDisplayMode::Normal),
            r#"{"name":"test","value":1}"#
        );

        // JsonPretty: 惰性计算并缓存(多次调用结果一致)
        let pretty = message.display_content(MessageDisplayMode::JsonPretty);
        assert!(pretty.contains("\n"), "美化后应含换行: {}", pretty);
        assert_eq!(
            message.display_content(MessageDisplayMode::JsonPretty),
            pretty
        );

        // JsonMinified: 与基础内容一致(本就是压缩 JSON)
        assert_eq!(
            message.display_content(MessageDisplayMode::JsonMinified),
            r#"{"name":"test","value":1}"#
        );

        // 非 JSON 文本: 格式化失败时原样返回
        let plain = Message::new(MessageDirection::Sent, b"Hello".to_vec(), MessageType::Text);
        assert_eq!(
            plain.display_content(MessageDisplayMode::JsonPretty),
            "Hello"
        );
    }

    #[test]
    fn test_display_cache_invalidated_by_set_message_type() {
        use super::MessageDisplayMode;

        let mut message =
            Message::new(MessageDirection::Sent, b"Hello".to_vec(), MessageType::Text);
        assert_eq!(
            message.display_content(MessageDisplayMode::JsonPretty),
            "Hello"
        );

        // 切换类型后显示缓存应失效,按新基础内容重新计算
        message.set_message_type(MessageType::Hex);
        assert_eq!(
            message.display_content(MessageDisplayMode::JsonPretty),
            "48 65 6C 6C 6F"
        );
    }

    #[test]
    fn test_clone_drops_display_cache() {
        use super::MessageDisplayMode;

        let message = Message::new(MessageDirection::Sent, b"Hello".to_vec(), MessageType::Text);
        let _ = message.display_content(MessageDisplayMode::JsonPretty);

        // 克隆件(日志/导出路径)不携带显示缓存,但显示结果仍可按需重算
        let clone = message.clone();
        assert_eq!(
            clone.display_content(MessageDisplayMode::JsonPretty),
            "Hello"
        );
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

    /// 造一条内容为序号 i 的文本消息，便于断言保留窗口。
    fn numbered_message(i: usize) -> Message {
        Message::new(
            MessageDirection::Received,
            i.to_string().into_bytes(),
            MessageType::Text,
        )
    }

    /// 取消息内容中的序号。
    fn message_number(message: &Message) -> usize {
        String::from_utf8(message.raw_data.clone())
            .unwrap()
            .parse::<usize>()
            .unwrap()
    }

    #[test]
    fn test_message_list_append_within_cap() {
        let mut state = MessageListState::new();

        // 默认上限 10000，连续添加 100 条：不触发淘汰，最早的消息仍在头部
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
        assert_eq!(state.messages[0].raw_data, vec![0]);
        assert_eq!(state.messages[99].raw_data, vec![99]);
    }

    #[test]
    fn test_add_messages_batch_within_cap() {
        let mut state = MessageListState::new();

        let batch: Vec<Message> = (0..3u8)
            .map(|i| Message::new(MessageDirection::Received, vec![i], MessageType::Hex))
            .collect();
        assert_eq!(state.add_messages_batch(batch), 0);
        assert_eq!(state.messages.len(), 3);
        assert_eq!(state.total_received, 3);
        assert_eq!(state.messages[0].raw_data, vec![0]);
        assert_eq!(state.messages[2].raw_data, vec![2]);

        // 第二批：远未到上限，全部追加，头部未被丢弃
        let batch: Vec<Message> = (3..6u8)
            .map(|i| Message::new(MessageDirection::Sent, vec![i], MessageType::Hex))
            .collect();
        assert_eq!(state.add_messages_batch(batch), 0);
        assert_eq!(state.messages.len(), 6);
        assert_eq!(state.messages[0].raw_data, vec![0]);
        assert_eq!(state.messages.last().unwrap().raw_data, vec![5]);
        // 累计：3 接收 + 3 发送 = 6
        assert_eq!(state.total_messages(), 6);
    }

    #[test]
    fn test_add_messages_batch_empty() {
        let mut state = MessageListState::new();
        assert_eq!(state.add_messages_batch(Vec::new()), 0);
        assert_eq!(state.messages.len(), 0);
    }

    #[test]
    fn test_default_keep_last() {
        assert_eq!(MessageListState::default().keep_last, super::DEFAULT_KEEP_LAST);
        assert_eq!(super::DEFAULT_KEEP_LAST, 10_000);
    }

    #[test]
    fn test_keep_last_zero_never_evicts() {
        let mut state = MessageListState::new();
        assert_eq!(state.set_keep_last(0), 0);

        // 0 = 不限制：追加 50000 条也不能丢弃任何消息
        for i in 0..50_000usize {
            assert_eq!(state.add_message(numbered_message(i)), 0);
        }
        assert_eq!(state.messages.len(), 50_000);
        assert_eq!(message_number(&state.messages[0]), 0);
        assert_eq!(message_number(state.messages.last().unwrap()), 49_999);
    }

    #[test]
    fn test_keep_last_evicts_oldest() {
        let mut state = MessageListState::new();
        state.set_keep_last(100);
        let slack = super::evict_slack(100);

        for i in 0..500usize {
            let dropped = state.add_message(numbered_message(i));
            // 滞后裁剪：每次追加后长度不超过 keep_last + slack
            assert!(state.messages.len() <= 100 + slack);
            // 滞后窗口内不淘汰，超阈值时一次裁回 keep_last
            assert!(dropped == 0 || dropped >= slack + 1);
        }

        assert_eq!(state.messages.len(), 100);
        // 保留窗口是从尾部起来的连续区间
        let first = message_number(&state.messages[0]);
        assert_eq!(first + state.messages.len(), 500);
        assert_eq!(message_number(state.messages.last().unwrap()), 499);
    }

    #[test]
    fn test_add_messages_batch_evicts() {
        let mut state = MessageListState::new();
        state.set_keep_last(100);
        let slack = super::evict_slack(100);

        let mut total_dropped = 0;
        for batch_ix in 0..5usize {
            let batch: Vec<Message> = (0..50usize)
                .map(|i| numbered_message(batch_ix * 50 + i))
                .collect();
            total_dropped += state.add_messages_batch(batch);
            assert!(state.messages.len() <= 100 + slack);
        }

        assert!(total_dropped > 0);
        assert_eq!(state.messages.len(), 100);
        // 保留的是最后 100 条
        assert_eq!(message_number(&state.messages[0]), 150);
        assert_eq!(message_number(state.messages.last().unwrap()), 249);
    }

    #[test]
    fn test_no_drop_keeps_arc_shared() {
        // 无需淘汰时不得触发 Arc::make_mut 的深拷贝（导出路径会 clone 该 Arc）
        let mut state = MessageListState::new();
        for i in 0..10usize {
            state.add_message(numbered_message(i));
        }
        // 模拟导出：外部持有一份引用
        let export_ref = state.messages.clone();
        let ptr_before = Arc::as_ptr(&state.messages);

        // 0（不淘汰）与放大上限都不该产生任何拷贝
        assert_eq!(state.set_keep_last(0), 0);
        assert_eq!(state.set_keep_last(100), 0);
        assert_eq!(Arc::as_ptr(&state.messages), ptr_before);
        assert_eq!(export_ref.len(), 10);
    }

    #[test]
    fn test_set_keep_last_trims_immediately() {
        let mut state = MessageListState::new();
        for i in 0..1000usize {
            state.add_message(numbered_message(i));
        }
        assert_eq!(state.messages.len(), 1000);

        // 显式改设置不走滞后，立即精确裁剪
        assert_eq!(state.set_keep_last(100), 900);
        assert_eq!(state.messages.len(), 100);
        assert_eq!(message_number(&state.messages[0]), 900);

        // 放大上限不恢复已丢弃的消息
        assert_eq!(state.set_keep_last(500), 0);
        assert_eq!(state.messages.len(), 100);
    }

    #[test]
    fn test_batch_larger_than_keep_last() {
        let mut state = MessageListState::new();
        state.set_keep_last(10);

        let batch: Vec<Message> = (0..100usize).map(numbered_message).collect();
        let dropped = state.add_messages_batch(batch);

        assert_eq!(dropped, 90);
        assert_eq!(state.messages.len(), 10);
        // 保留本批最后 10 条
        assert_eq!(message_number(&state.messages[0]), 90);
        assert_eq!(message_number(state.messages.last().unwrap()), 99);
        // 累计计数不回退
        assert_eq!(state.total_received, 100);
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
