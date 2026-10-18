use rust_i18n::t;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// 连接类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionType {
    Tcp,
    Udp,
    /// MQTT 3.1.1 客户端（仅客户端角色，无服务端；见 plans/plan-mqtt-client.md）
    Mqtt,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionType::Tcp => write!(f, "TCP"),
            ConnectionType::Udp => write!(f, "UDP"),
            ConnectionType::Mqtt => write!(f, "MQTT"),
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

/// 发送结尾追加字符（连接级配置，默认 None 保证既有行为不变）。
///
/// 应用点在网络层写路径: 手动发送、周期发送、逐行发送任务、自动回复
/// 最终都经 `write_sender` → 网络层写任务, 在这一层追加可一处实现全覆盖,
/// 且计数(`add_sent_bytes`)自动包含追加字节。消息列表与日志仍展示原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum TrailerKind {
    /// 不追加(默认)
    #[default]
    None = 0,
    /// 追加 \n
    Lf = 1,
    /// 追加 \r\n
    CrLf = 2,
}

impl TrailerKind {
    /// 转成可原子存储的 u8
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// 从原子存储还原; 未知值退回 None(默认), 保证既有行为
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => TrailerKind::Lf,
            2 => TrailerKind::CrLf,
            _ => TrailerKind::None,
        }
    }

    /// 界面上循环切换的顺序: 无 → LF → CRLF → 无
    pub fn next_cycle(self) -> Self {
        match self {
            TrailerKind::None => TrailerKind::Lf,
            TrailerKind::Lf => TrailerKind::CrLf,
            TrailerKind::CrLf => TrailerKind::None,
        }
    }
}

/// 运行期「结尾追加字符」设置(连接级, 可原子读写)。
///
/// 与 `ConnectionConfig::send_trailer`(持久化真源)配合: 持久化字段决定下次
/// 连接的初始值, 本结构承载运行期即时生效 —— 下拉改动立即影响已建立的连接,
/// 无需重连。Clone 共享同一 `Arc`, 由 `ConnectionTabState` 持有并 clone 给网络层。
#[derive(Debug, Clone, Default)]
pub struct TrailerSetting(Arc<AtomicU8>);

impl TrailerSetting {
    pub fn new(kind: TrailerKind) -> Self {
        Self(Arc::new(AtomicU8::new(kind.to_u8())))
    }

    pub fn get(&self) -> TrailerKind {
        TrailerKind::from_u8(self.0.load(Ordering::Relaxed))
    }

    pub fn set(&self, kind: TrailerKind) {
        self.0.store(kind.to_u8(), Ordering::Relaxed);
    }
}

/// 按配置给线上字节追加结尾字符(纯函数, encoder 与 UDP 发送点共用)。
///
/// `None` 时借用原字节(零拷贝), 其余返回新分配的 `Cow::Owned`。
pub fn apply_trailer(bytes: &[u8], kind: TrailerKind) -> std::borrow::Cow<'_, [u8]> {
    use std::borrow::Cow;
    match kind {
        TrailerKind::None => Cow::Borrowed(bytes),
        TrailerKind::Lf => {
            let mut out = Vec::with_capacity(bytes.len() + 1);
            out.extend_from_slice(bytes);
            out.push(b'\n');
            Cow::Owned(out)
        }
        TrailerKind::CrLf => {
            let mut out = Vec::with_capacity(bytes.len() + 2);
            out.extend_from_slice(bytes);
            out.push(b'\r');
            out.push(b'\n');
            Cow::Owned(out)
        }
    }
}

/// 默认发送消息输入模式
fn default_message_input_mode() -> String {
    "text".to_string()
}

/// MQTT QoS（项目内枚举，规避裸 u8 越界/魔法值；serde 以 0/1/2 数字持久化）。
///
/// 发送时经 `to_rumqttc` 转换为库枚举，订阅/遗嘱/发布共用（规划 §5.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Qos {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl Qos {
    pub fn to_rumqttc(self) -> rumqttc::QoS {
        match self {
            Qos::AtMostOnce => rumqttc::QoS::AtMostOnce,
            Qos::AtLeastOnce => rumqttc::QoS::AtLeastOnce,
            Qos::ExactlyOnce => rumqttc::QoS::ExactlyOnce,
        }
    }
}

impl From<Qos> for u8 {
    fn from(q: Qos) -> u8 {
        q as u8
    }
}

impl TryFrom<u8> for Qos {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(Qos::AtMostOnce),
            1 => Ok(Qos::AtLeastOnce),
            2 => Ok(Qos::ExactlyOnce),
            other => Err(other),
        }
    }
}

impl Serialize for Qos {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Qos {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(deserializer)?;
        Qos::try_from(v).map_err(|_| serde::de::Error::custom(format!("invalid QoS: {v}")))
    }
}

fn default_keep_alive() -> u16 {
    60
}

/// 生成 MQTT Client ID（规划 §5.1: `netassist-{uuid 前 8 位}`）
pub fn generate_mqtt_client_id() -> String {
    format!("netassist-{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

/// MQTT 遗嘱消息设置（首版 payload 仅文本；规划 §5.1）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WillSettings {
    pub topic: String,
    pub payload: String,
    pub qos: Qos,
    pub retain: bool,
}

/// MQTT 专属设置（`ClientConfig` 内嵌，仅 protocol == Mqtt 时有意义；规划 D-4）。
///
/// 向后兼容：旧配置文件无 `mqtt` 字段 → 反序列化为 `None`，既有 TCP/UDP 配置不受影响。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MqttSettings {
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_keep_alive")]
    pub keep_alive_secs: u16,
    #[serde(default = "default_true")]
    pub clean_session: bool,
    #[serde(default)]
    pub will: Option<WillSettings>,
}

impl Default for MqttSettings {
    fn default() -> Self {
        Self {
            client_id: generate_mqtt_client_id(),
            username: None,
            password: None,
            keep_alive_secs: default_keep_alive(),
            clean_session: true,
            will: None,
        }
    }
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
    /// 发送结尾追加字符（默认不追加；MQTT 协议下访问器强制视为 None，见规划 D-7）
    #[serde(default)]
    pub send_trailer: TrailerKind,
    /// MQTT 专属设置（仅 protocol == Mqtt 时有意义；旧配置无此字段 → None）
    #[serde(default)]
    pub mqtt: Option<MqttSettings>,
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
            send_trailer: TrailerKind::None,
            mqtt: None,
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
    /// 发送结尾追加字符(默认不追加)
    #[serde(default)]
    pub send_trailer: TrailerKind,
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
            send_trailer: TrailerKind::None,
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

    /// 获取连接级发送结尾追加字符配置
    ///
    /// MQTT 载荷不可污染（规划 D-7）：无论持久化值为何，MQTT 连接一律视为不追加。
    pub fn send_trailer(&self) -> TrailerKind {
        if self.protocol() == ConnectionType::Mqtt {
            return TrailerKind::None;
        }
        match self {
            ConnectionConfig::Client(config) => config.send_trailer,
            ConnectionConfig::Server(config) => config.send_trailer,
        }
    }

    /// 设置连接级发送结尾追加字符配置
    pub fn set_send_trailer(&mut self, kind: TrailerKind) {
        match self {
            ConnectionConfig::Client(config) => config.send_trailer = kind,
            ConnectionConfig::Server(config) => config.send_trailer = kind,
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
            send_trailer: TrailerKind::None,
            mqtt: None,
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
            send_trailer: TrailerKind::None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClientConfig, ConnectionConfig, ConnectionType, MqttSettings, Qos, ServerConfig,
        TrailerKind, WillSettings, apply_trailer,
    };

    #[test]
    fn mqtt_settings_serde_roundtrip() {
        let settings = MqttSettings {
            client_id: "netassist-test01".to_string(),
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            keep_alive_secs: 30,
            clean_session: false,
            will: Some(WillSettings {
                topic: "device/1/status".to_string(),
                payload: "offline".to_string(),
                qos: Qos::AtLeastOnce,
                retain: true,
            }),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: MqttSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, settings);
    }

    #[test]
    fn mqtt_settings_defaults() {
        let settings = MqttSettings::default();
        assert!(settings.client_id.starts_with("netassist-"));
        assert_eq!(settings.client_id.len(), "netassist-".len() + 8);
        assert_eq!(settings.keep_alive_secs, 60);
        assert!(settings.clean_session);
        assert!(settings.username.is_none());
        assert!(settings.password.is_none());
        assert!(settings.will.is_none());
    }

    #[test]
    fn qos_serde_numeric_roundtrip() {
        for (qos, json) in [
            (Qos::AtMostOnce, "0"),
            (Qos::AtLeastOnce, "1"),
            (Qos::ExactlyOnce, "2"),
        ] {
            assert_eq!(serde_json::to_string(&qos).unwrap(), json);
            let back: Qos = serde_json::from_str(json).unwrap();
            assert_eq!(back, qos);
        }
        // 非法值拒绝
        assert!(serde_json::from_str::<Qos>("3").is_err());
    }

    #[test]
    fn client_config_backward_compatible_without_mqtt_field() {
        // 旧版配置 JSON 无 mqtt 字段 → None，既有 TCP/UDP 配置不受影响
        let legacy = r#"{
            "id": "legacy-id",
            "protocol": "tcp",
            "server_address": "127.0.0.1",
            "server_port": 8080
        }"#;
        let config: ClientConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(config.protocol, ConnectionType::Tcp);
        assert!(config.mqtt.is_none());
    }

    #[test]
    fn mqtt_client_config_serialization_roundtrip() {
        let mut config = ClientConfig::default();
        config.protocol = ConnectionType::Mqtt;
        config.server_address = "broker.emqx.io".to_string();
        config.server_port = 1883;
        config.mqtt = Some(MqttSettings::default());
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"protocol\":\"mqtt\""));
        let back: ClientConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn mqtt_trailer_forced_to_none() {
        // 规划 D-7：MQTT 载荷不可污染 —— 即使持久化值被手改为 Lf，访问器也强制 None；
        // TCP/UDP 行为不变
        let mut mqtt_config = ClientConfig::default();
        mqtt_config.protocol = ConnectionType::Mqtt;
        mqtt_config.send_trailer = TrailerKind::Lf;
        let mqtt_wrapper = ConnectionConfig::Client(mqtt_config);
        assert_eq!(mqtt_wrapper.send_trailer(), TrailerKind::None);

        let mut tcp_config = ClientConfig::default();
        tcp_config.send_trailer = TrailerKind::Lf;
        let tcp_wrapper = ConnectionConfig::Client(tcp_config);
        assert_eq!(tcp_wrapper.send_trailer(), TrailerKind::Lf);
    }

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
    /// 结尾追加字符: None 原样借用, Lf / CrLf 追加对应字节
    fn test_apply_trailer() {
        assert_eq!(&*apply_trailer(b"AT", TrailerKind::None), b"AT");
        assert_eq!(&*apply_trailer(b"AT", TrailerKind::Lf), b"AT\n");
        assert_eq!(&*apply_trailer(b"AT", TrailerKind::CrLf), b"AT\r\n");
        // 空消息也能追加(仅结尾)
        assert_eq!(&*apply_trailer(b"", TrailerKind::Lf), b"\n");
    }

    #[test]
    /// 旧版配置(无 send_trailer 字段)反序列化后默认为 None, 保持既有行为
    fn test_send_trailer_serde_default() {
        let json = r#"{
            "id": "test-id",
            "protocol": "tcp",
            "server_address": "192.168.1.1",
            "server_port": 8080
        }"#;
        let config: ClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.send_trailer, TrailerKind::None);

        // 序列化往返保持取值
        let mut config = ClientConfig::default();
        config.send_trailer = TrailerKind::CrLf;
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ClientConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.send_trailer, TrailerKind::CrLf);
    }
}
