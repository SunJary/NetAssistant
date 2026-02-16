use serde::{Deserialize, Serialize};
use std::fmt;

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
            ConnectionStatus::NotConnected => write!(f, "未连接"),
            ConnectionStatus::Disconnected => write!(f, "已断开"),
            ConnectionStatus::Connecting => write!(f, "连接中"),
            ConnectionStatus::Connected => write!(f, "已连接"),
            ConnectionStatus::Listening => write!(f, "监听中"),
            ConnectionStatus::Error => write!(f, "错误"),
        }
    }
}

/// 长度前缀解码器配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LengthDelimitedConfig {
    pub max_frame_length: usize, // 最大帧长度
    pub length_field_offset: u8, // 长度字段偏移量
    pub length_field_length: u8, // 长度字段长度
    pub length_adjustment: i32,  // 长度调整值
    pub length_field_is_including_length_field: bool, // 长度字段是否包含自身长度
}

impl Default for LengthDelimitedConfig {
    fn default() -> Self {
        Self {
            max_frame_length: 8192,
            length_field_offset: 0,
            length_field_length: 4,
            length_adjustment: 0,
            length_field_is_including_length_field: false,
        }
    }
}

/// 解码器配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecoderConfig {
    Bytes,
    LineBased,
    LengthDelimited(LengthDelimitedConfig),
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
            DecoderConfig::Bytes => write!(f, "原始数据"),
            DecoderConfig::LineBased => write!(f, "换行符"),
            DecoderConfig::LengthDelimited(_) => write!(f, "长度前缀"),
            DecoderConfig::Json => write!(f, "JSON"),
        }
    }
}

/// 客户端连接配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "generate_uuid")]
    pub id: String,
    pub name: String,
    pub protocol: ConnectionType,
    pub server_address: String,
    pub server_port: u16,
    pub timeout: u64,
    pub auto_reconnect: bool,
    #[serde(default)]
    pub decoder_config: DecoderConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            id: generate_uuid(),
            name: "新客户端连接".to_string(),
            protocol: ConnectionType::Tcp,
            server_address: "127.0.0.1".to_string(),
            server_port: 8080,
            timeout: 30,
            auto_reconnect: false,
            decoder_config: DecoderConfig::default(),
        }
    }
}

/// 服务端监听配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "generate_uuid")]
    pub id: String,
    pub name: String,
    pub protocol: ConnectionType,
    pub listen_address: String,
    pub listen_port: u16,
    pub max_connections: usize,
    pub timeout: u64,
    #[serde(default)]
    pub decoder_config: DecoderConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            id: generate_uuid(),
            name: "新服务端监听".to_string(),
            protocol: ConnectionType::Tcp,
            listen_address: "0.0.0.0".to_string(),
            listen_port: 8080,
            max_connections: 100,
            timeout: 30,
            decoder_config: DecoderConfig::default(),
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
    pub fn name(&self) -> &str {
        match self {
            ConnectionConfig::Client(config) => &config.name,
            ConnectionConfig::Server(config) => &config.name,
        }
    }

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
    
    /// 设置连接名称
    // pub fn set_name(&mut self, name: String) {
    //     match self {
    //         ConnectionConfig::Client(config) => config.name = name,
    //         ConnectionConfig::Server(config) => config.name = name,
    //     }
    // }
    
    
    /// 创建新的客户端连接配置（自动生成ID）
    pub fn new_client(
        name: String,
        server_address: String,
        server_port: u16,
        protocol: ConnectionType,
    ) -> Self {
        ConnectionConfig::Client(ClientConfig {
            id: generate_uuid(),
            name,
            protocol,
            server_address,
            server_port,
            timeout: 30,
            auto_reconnect: false,
            decoder_config: DecoderConfig::default(),
        })
    }
    
    /// 创建新的服务端监听配置（自动生成ID）
    pub fn new_server(
        name: String,
        listen_address: String,
        listen_port: u16,
        protocol: ConnectionType,
    ) -> Self {
        ConnectionConfig::Server(ServerConfig {
            id: generate_uuid(),
            name,
            protocol,
            listen_address,
            listen_port,
            max_connections: 100,
            timeout: 30,
            decoder_config: DecoderConfig::default(),
        })
    }
}




#[cfg(test)]
mod tests {
    use super::{ClientConfig, ConnectionConfig, ConnectionType, ServerConfig};

    #[test]
    /// 测试客户端配置的默认值
    fn test_client_config_default() {
        let default_config = ClientConfig::default();
        assert_eq!(default_config.name, "新客户端连接");
        assert_eq!(default_config.protocol, ConnectionType::Tcp);
        assert_eq!(default_config.server_address, "127.0.0.1");
        assert_eq!(default_config.server_port, 8080);
        assert_eq!(default_config.timeout, 30);
        assert!(!default_config.auto_reconnect);
    }

    #[test]
    /// 测试创建自定义客户端配置
    fn test_client_config_new() {
        let connection_config = ConnectionConfig::new_client(
            "测试客户端".to_string(),
            "192.168.1.1".to_string(),
            1234,
            ConnectionType::Udp,
        );
        
        if let ConnectionConfig::Client(custom_config) = connection_config {
            assert_eq!(custom_config.name, "测试客户端");
            assert_eq!(custom_config.protocol, ConnectionType::Udp);
            assert_eq!(custom_config.server_address, "192.168.1.1");
            assert_eq!(custom_config.server_port, 1234);
            assert_eq!(custom_config.timeout, 30);
            assert!(!custom_config.auto_reconnect);
        } else {
            panic!("应该创建客户端配置");
        }
    }

    #[test]
    /// 测试服务端配置的默认值
    fn test_server_config_default() {
        let default_config = ServerConfig::default();
        assert_eq!(default_config.name, "新服务端监听");
        assert_eq!(default_config.protocol, ConnectionType::Tcp);
        assert_eq!(default_config.listen_address, "0.0.0.0");
        assert_eq!(default_config.listen_port, 8080);
        assert_eq!(default_config.max_connections, 100);
        assert_eq!(default_config.timeout, 30);
    }

    #[test]
    /// 测试创建自定义服务端配置
    fn test_server_config_new() {
        let connection_config = ConnectionConfig::new_server(
            "测试服务端".to_string(),
            "192.168.1.1".to_string(),
            5678,
            ConnectionType::Udp,
        );
        
        if let ConnectionConfig::Server(custom_config) = connection_config {
            assert_eq!(custom_config.name, "测试服务端");
            assert_eq!(custom_config.protocol, ConnectionType::Udp);
            assert_eq!(custom_config.listen_address, "192.168.1.1");
            assert_eq!(custom_config.listen_port, 5678);
            assert_eq!(custom_config.max_connections, 100);
            assert_eq!(custom_config.timeout, 30);
        } else {
            panic!("应该创建服务端配置");
        }
    }

    #[test]
    /// 测试客户端连接配置的功能
    /// 包括类型判断、名称获取和协议获取
    fn test_connection_config_client() {
        let client_config = ClientConfig::default();
        let connection_config = ConnectionConfig::Client(client_config.clone());

        assert!(connection_config.is_client());
        assert!(!connection_config.is_server());
        assert_eq!(connection_config.name(), &client_config.name);
        assert_eq!(connection_config.protocol(), client_config.protocol);
    }

    #[test]
    /// 测试服务端连接配置的功能
    /// 包括类型判断、名称获取和协议获取
    fn test_connection_config_server() {
        let server_config = ServerConfig::default();
        let connection_config = ConnectionConfig::Server(server_config.clone());

        assert!(!connection_config.is_client());
        assert!(connection_config.is_server());
        assert_eq!(connection_config.name(), &server_config.name);
        assert_eq!(connection_config.protocol(), server_config.protocol);
    }
}
