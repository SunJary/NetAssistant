use crate::message::{Message, MessageDirection, MessageType};
use hex;

pub trait MessageProcessor: Send + Sync + 'static {
    /// 处理接收到的消息
    fn process_received_message(&self, raw_data: Vec<u8>, message_type: MessageType) -> Message;
    
    /// 处理要发送的消息
    fn process_send_message(&self, content: &str, message_type: MessageType) -> Vec<u8>;
    
    /// 将消息转换为字符串表示
    fn message_to_string(&self, message: &Message) -> String;
}

/// 默认的消息处理器实现
#[derive(Clone)]
pub struct DefaultMessageProcessor;

impl DefaultMessageProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl MessageProcessor for DefaultMessageProcessor {
    fn process_received_message(&self, raw_data: Vec<u8>, message_type: MessageType) -> Message {
        Message::new(
            MessageDirection::Received,
            raw_data,
            message_type
        )
    }
    
    fn process_send_message(&self, content: &str, message_type: MessageType) -> Vec<u8> {
        match message_type {
            MessageType::Text => {
                // 直接将文本内容转换为字节
                content.as_bytes().to_vec()
            },
            MessageType::Hex => {
                // 清理十六进制字符串
                let cleaned_hex = content.replace(|c: char| !c.is_ascii_hexdigit(), "");
                
                // 确保十六进制字符串的长度是偶数
                let hex_str = if cleaned_hex.len() % 2 == 0 {
                    cleaned_hex
                } else {
                    format!("{cleaned_hex}0")
                };
                
                // 解码十六进制字符串
                hex::decode(hex_str).expect("无效的十六进制格式")
            },
        }
    }
    
    fn message_to_string(&self, message: &Message) -> String {
        match message.message_type {
            MessageType::Text => {
                match String::from_utf8(message.raw_data.clone()) {
                    Ok(text) => text,
                    Err(_) => "[非UTF-8数据]".to_string(),
                }
            },
            MessageType::Hex => {
                message.raw_data
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<String>>()
                    .join(" ")
            },
        }
    }
}
