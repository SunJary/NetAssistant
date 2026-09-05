use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// 连接类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionType {
    Tcp,
    Udp,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionType::Tcp => write!(f, "TCP"),
            ConnectionType::Udp => write!(f, "UDP"),
        }
    }
}

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    NotConnected,
    Disconnected,
    Connecting,
    Connected,
    Listening,
    Error,
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionStatus::NotConnected => {
                write!(f, "{}", t!("connection_status.not_connected"))
            }
            ConnectionStatus::Disconnected => write!(f, "{}", t!("connection_status.disconnected")),
            ConnectionStatus::Connecting => write!(f, "{}", t!("connection_status.connecting")),
            ConnectionStatus::Connected => write!(f, "{}", t!("connection_status.connected")),
            ConnectionStatus::Listening => write!(f, "{}", t!("connection_status.listening")),
            ConnectionStatus::Error => write!(f, "{}", t!("connection_status.error")),
        }
    }
}

/// 长度前缀解码器配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LengthDelimitedConfig {
    pub max_frame_length: usize,                      // 最大帧长度
    pub length_field_offset: u8,                      // 长度字段偏移量
    pub length_field_length: u8,                      // 长度字段长度
    pub length_adjustment: i32,                       // 长度调整值
    pub length_field_is_including_length_field: bool, // 长度字段是否包含自身长度
    #[serde(default)]
    pub length_field_is_little_endian: bool, // 长度字段字节序: true=小端, false=大端(默认)
    #[serde(default = "default_true")]
    pub length_field_keep_full_frame: bool, // 输出是否保留完整帧(含偏移与长度字段), 默认true=保留完整帧
}

fn default_true() -> bool {
    true
}

impl Default for LengthDelimitedConfig {
    fn default() -> Self {
        Self {
            max_frame_length: 8192,
            length_field_offset: 0,
            length_field_length: 4,
            length_adjustment: 0,
            length_field_is_including_length_field: false,
            length_field_is_little_endian: false,
            length_field_keep_full_frame: true,
        }
    }
}

/// 解码器配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecoderConfig {
    Bytes,
    LineBased,
    LengthDelimited(LengthDelimitedConfig),
    /// 固定字节长度分帧，每个报文固定 N 字节
    FixedLength(usize),
    Json,
}

impl Default for DecoderConfig {
    fn default() -> Self {
        DecoderConfig::Bytes
    }
}

impl fmt::Display for DecoderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecoderConfig::Bytes => write!(f, "{}", t!("decoder_config.bytes")),
            DecoderConfig::LineBased => write!(f, "{}", t!("decoder_config.line_based")),
            DecoderConfig::LengthDelimited(_) => {
                write!(f, "{}", t!("decoder_config.length_delimited"))
            }
            DecoderConfig::FixedLength(_) => write!(f, "{}", t!("decoder_config.fixed_length")),
            DecoderConfig::Json => write!(f, "JSON"),
        }
    }
}

/// 默认发送消息输入模式
fn default_message_input_mode() -> String {
    "text".to_string()
}

/// 客户端连接配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "generate_uuid")]
    pub id: String,
    pub protocol: ConnectionType,
    pub server_address: String,
    pub server_port: u16,
    #[serde(default)]
    pub decoder_config: DecoderConfig,
    /// 发送消息输入模式：text / hex
    #[serde(default = "default_message_input_mode")]
    pub message_input_mode: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            id: generate_uuid(),
            protocol: ConnectionType::Tcp,
            server_address: "127.0.0.1".to_string(),
            server_port: 8080,
            decoder_config: DecoderConfig::default(),
            message_input_mode: default_message_input_mode(),
        }
    }
}

/// 服务端监听配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "generate_uuid")]
    pub id: String,
    pub protocol: ConnectionType,
    pub listen_address: String,
    pub listen_port: u16,
    #[serde(default)]
    pub decoder_config: DecoderConfig,
    /// 发送消息输入模式：text / hex
    #[serde(default = "default_message_input_mode")]
    pub message_input_mode: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            id: generate_uuid(),
            protocol: ConnectionType::Tcp,
            listen_address: "0.0.0.0".to_string(),
            listen_port: 8080,
            decoder_config: DecoderConfig::default(),
            message_input_mode: default_message_input_mode(),
        }
    }
}

/// 生成UUID
fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 连接配置（统一客户端和服务端）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "config")]
pub enum ConnectionConfig {
    Client(ClientConfig),
    Server(ServerConfig),
}

impl ConnectionConfig {
    pub fn protocol(&self) -> ConnectionType {
        match self {
            ConnectionConfig::Client(config) => config.protocol,
            ConnectionConfig::Server(config) => config.protocol,
        }
    }

    pub fn is_client(&self) -> bool {
        matches!(self, ConnectionConfig::Client(_))
    }

    pub fn is_server(&self) -> bool {
        matches!(self, ConnectionConfig::Server(_))
    }

    /// 获取连接ID
    pub fn id(&self) -> &str {
        match self {
            ConnectionConfig::Client(config) => &config.id,
            ConnectionConfig::Server(config) => &config.id,
        }
    }

    /// 获取发送消息输入模式（text / hex）
    pub fn message_input_mode(&self) -> &str {
        match self {
            ConnectionConfig::Client(config) => &config.message_input_mode,
            ConnectionConfig::Server(config) => &config.message_input_mode,
        }
    }

    /// 获取包含地址端口的标识字符串，格式如 TCP_127.0.0.1_8080
    pub fn address_label(&self) -> String {
        match self {
            ConnectionConfig::Client(config) => {
                format!(
                    "{}_{}_{}",
                    config.protocol, config.server_address, config.server_port
                )
            }
            ConnectionConfig::Server(config) => {
                format!(
                    "{}_{}_{}",
                    config.protocol, config.listen_address, config.listen_port
                )
            }
        }
    }

    /// 创建新的客户端连接配置（自动生成ID）
    pub fn new_client(server_address: String, server_port: u16, protocol: ConnectionType) -> Self {
        ConnectionConfig::Client(ClientConfig {
            id: generate_uuid(),
            protocol,
            server_address,
            server_port,
            decoder_config: DecoderConfig::default(),
            message_input_mode: default_message_input_mode(),
        })
    }

    /// 创建新的服务端监听配置（自动生成ID）
    pub fn new_server(listen_address: String, listen_port: u16, protocol: ConnectionType) -> Self {
        ConnectionConfig::Server(ServerConfig {
            id: generate_uuid(),
            protocol,
            listen_address,
            listen_port,
            decoder_config: DecoderConfig::default(),
            message_input_mode: default_message_input_mode(),
        })
    }
}

/// 自动回复配置（运行时共享状态，UI 下发 → 网络层读取）
///
/// - `content` 为已按 `message_input_mode` 转换好的原始字节：
///   文本模式 = UTF-8 字节；十六进制模式 = 解析后的字节。
/// - 网络层回复时只投递这些字节，经用户配置的 encoder 编码后发送，
///   不额外修改内容、不擅自添加换行符。
#[derive(Debug)]
pub struct AutoReplyConfig {
    enabled: AtomicBool,
    content: StdMutex<Vec<u8>>,
}

impl Default for AutoReplyConfig {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            content: StdMutex::new(Vec::new()),
        }
    }
}

impl AutoReplyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否启用自动回复（无锁快速路径，未启用时网络层零开销）
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// 读取当前回复内容（克隆原始字节）
    pub fn content(&self) -> Vec<u8> {
        self.content.lock().unwrap().clone()
    }

    /// UI 下发更新（启用开关 + 回复内容原始字节）
    pub fn set(&self, enabled: bool, content: Vec<u8>) {
        self.enabled.store(enabled, Ordering::Relaxed);
        *self.content.lock().unwrap() = content;
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientConfig, ConnectionConfig, ConnectionType, ServerConfig};

    #[test]
    /// 测试客户端配置的默认值
    fn test_client_config_default() {
        let default_config = ClientConfig::default();
        assert_eq!(default_config.protocol, ConnectionType::Tcp);
        assert_eq!(default_config.server_address, "127.0.0.1");
        assert_eq!(default_config.server_port, 8080);
    }

    #[test]
    /// 测试创建自定义客户端配置
    fn test_client_config_new() {
        let connection_config =
            ConnectionConfig::new_client("192.168.1.1".to_string(), 1234, ConnectionType::Udp);

        if let ConnectionConfig::Client(custom_config) = connection_config {
            assert_eq!(custom_config.protocol, ConnectionType::Udp);
            assert_eq!(custom_config.server_address, "192.168.1.1");
            assert_eq!(custom_config.server_port, 1234);
        } else {
            panic!("应该创建客户端配置");
        }
    }

    #[test]
    /// 测试服务端配置的默认值
    fn test_server_config_default() {
        let default_config = ServerConfig::default();
        assert_eq!(default_config.protocol, ConnectionType::Tcp);
        assert_eq!(default_config.listen_address, "0.0.0.0");
        assert_eq!(default_config.listen_port, 8080);
    }

    #[test]
    /// 测试创建自定义服务端配置
    fn test_server_config_new() {
        let connection_config =
            ConnectionConfig::new_server("192.168.1.1".to_string(), 5678, ConnectionType::Udp);

        if let ConnectionConfig::Server(custom_config) = connection_config {
            assert_eq!(custom_config.protocol, ConnectionType::Udp);
            assert_eq!(custom_config.listen_address, "192.168.1.1");
            assert_eq!(custom_config.listen_port, 5678);
        } else {
            panic!("应该创建服务端配置");
        }
    }

    #[test]
    /// 测试客户端连接配置的功能
    /// 包括类型判断和协议获取
    fn test_connection_config_client() {
        let client_config = ClientConfig::default();
        let connection_config = ConnectionConfig::Client(client_config.clone());

        assert!(connection_config.is_client());
        assert!(!connection_config.is_server());
        assert_eq!(connection_config.protocol(), client_config.protocol);
    }

    #[test]
    /// 测试服务端连接配置的功能
    /// 包括类型判断和协议获取
    fn test_connection_config_server() {
        let server_config = ServerConfig::default();
        let connection_config = ConnectionConfig::Server(server_config.clone());

        assert!(!connection_config.is_client());
        assert!(connection_config.is_server());
        assert_eq!(connection_config.protocol(), server_config.protocol);
    }
}
