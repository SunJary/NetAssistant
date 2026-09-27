use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::utils::message_vars::{CompiledTemplate, RenderContext};

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
    /// 本地绑定地址(None=系统自动选择网卡)，仅支持 IP 字面量，如 "192.168.1.100"
    #[serde(default)]
    pub local_address: Option<String>,
    /// 本地绑定端口(None=系统自动分配临时端口)
    #[serde(default)]
    pub local_port: Option<u16>,
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
            local_address: None,
            local_port: None,
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
            local_address: None,
            local_port: None,
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

/// 自动回复载荷
#[derive(Debug)]
enum AutoReplyPayload {
    /// 无变量: UI 已按 `message_input_mode` 转好的字节, 保持既有性能与语义
    Fixed(Vec<u8>),
    /// 含变量: 模板 + 模式, 每条回复重新渲染
    Template {
        compiled: Arc<CompiledTemplate>,
        hex_mode: bool,
    },
}

/// 自动回复配置（运行时共享状态，UI 下发 → 网络层读取）
///
/// UI 下发回复**原文**与输入模式，网络层在每次回复时按需转换：
/// - 原文不含 `${` → 存为已转换字节（文本模式 = UTF-8 字节；十六进制模式 = 解析后的字节），
///   与升级前逐字节一致，不引入每条回复的渲染开销
/// - 原文含 `${` → 存为模板，每条回复重新渲染（时间/UUID 等逐条不同）
///
/// 回复内容经用户配置的 encoder 编码后发送，不额外修改内容、不擅自添加换行符。
#[derive(Debug)]
pub struct AutoReplyConfig {
    enabled: AtomicBool,
    payload: StdMutex<AutoReplyPayload>,
    /// 自动回复独立递增序号（网络层线程，不回 UI 线程取数）
    seq: AtomicU64,
}

impl Default for AutoReplyConfig {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            payload: StdMutex::new(AutoReplyPayload::Fixed(Vec::new())),
            seq: AtomicU64::new(0),
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

    /// UI 下发更新（启用开关 + 回复原文 + 输入模式）
    ///
    /// 含 `${` 的原文保存为模板由网络层逐条渲染；否则按当前模式预转换为字节。
    pub fn set(&self, enabled: bool, text: &str, hex_mode: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        let payload = if text.contains("${") {
            AutoReplyPayload::Template {
                compiled: Arc::new(CompiledTemplate::new(text)),
                hex_mode,
            }
        } else {
            AutoReplyPayload::Fixed(if hex_mode {
                crate::utils::hex::hex_to_bytes(text)
            } else {
                text.as_bytes().to_vec()
            })
        };
        *self.payload.lock().unwrap() = payload;
    }

    /// 取本次回复内容
    ///
    /// - `Fixed`：直接克隆已转换字节
    /// - `Template`：按当前时间渲染（hex 模式再解码），`${seq}` 仅在模板含它时消费序号
    pub fn render_content(&self) -> Vec<u8> {
        // 先在锁内取出所需数据，渲染在锁外进行，避免阻塞 UI 侧的 set
        let (compiled, hex_mode) = {
            let payload = self.payload.lock().unwrap();
            match &*payload {
                AutoReplyPayload::Fixed(bytes) => return bytes.clone(),
                AutoReplyPayload::Template { compiled, hex_mode } => (compiled.clone(), *hex_mode),
            }
        };

        let seq = if compiled.needs_seq() {
            Some(self.seq.fetch_add(1, Ordering::Relaxed))
        } else {
            None
        };
        let ctx = RenderContext::common(seq);
        let mut rendered = String::with_capacity(compiled.template_len() + 32);
        compiled.render(&ctx, hex_mode, &mut rendered);
        if hex_mode {
            crate::utils::hex::hex_to_bytes(&rendered)
        } else {
            rendered.into_bytes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AutoReplyConfig, ClientConfig, ConnectionConfig, ConnectionType, ServerConfig};

    #[test]
    /// 测试旧版配置 JSON(无 local_address/local_port 字段)反序列化后为 None,
    /// 保证升级时旧配置文件无损加载
    fn test_client_config_deserialize_without_local_bind() {
        let json = r#"{
            "id": "test-id",
            "protocol": "tcp",
            "server_address": "192.168.1.1",
            "server_port": 8080
        }"#;
        let config: ClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.local_address, None);
        assert_eq!(config.local_port, None);
    }

    #[test]
    /// 测试本地绑定字段的序列化往返
    fn test_client_config_local_bind_roundtrip() {
        let config = ClientConfig {
            local_address: Some("192.168.1.100".to_string()),
            local_port: Some(50000),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ClientConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.local_address, Some("192.168.1.100".to_string()));
        assert_eq!(parsed.local_port, Some(50000));
    }

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

    #[test]
    /// 无变量自动回复必须与升级前逐字节一致(文本 / hex 两种模式)
    fn test_auto_reply_fixed_equivalence() {
        let cfg = AutoReplyConfig::new();
        cfg.set(true, "ok", false);
        assert!(cfg.is_enabled());
        assert_eq!(cfg.render_content(), b"ok".to_vec());

        cfg.set(true, "6F 6B", true);
        assert_eq!(cfg.render_content(), b"ok".to_vec());

        // 未启用时仍可读内容(是否回复由 is_enabled 决定)
        cfg.set(false, "hello", false);
        assert!(!cfg.is_enabled());
        assert_eq!(cfg.render_content(), b"hello".to_vec());
    }

    #[test]
    /// 含变量的自动回复逐条重新渲染(时间/UUID 不同, 未知变量原样保留)
    fn test_auto_reply_template_renders_each_time() {
        let cfg = AutoReplyConfig::new();
        cfg.set(true, "id=${uuid}", false);
        let a = cfg.render_content();
        let b = cfg.render_content();
        assert_ne!(a, b, "含 uuid 变量的回复每条应不同");
        assert_eq!(a.len(), 3 + 36);

        // 未知变量原样保留(与文本发送语义一致)
        cfg.set(true, "${unknown}", false);
        assert_eq!(cfg.render_content(), b"${unknown}".to_vec());
    }

    #[test]
    /// hex 模式含变量的回复先渲染再解码为字节
    fn test_auto_reply_template_hex_mode() {
        let cfg = AutoReplyConfig::new();
        cfg.set(true, "4142${random:1:1}", true);
        assert_eq!(cfg.render_content(), vec![0x41, 0x42, 0x01]);
    }
}
