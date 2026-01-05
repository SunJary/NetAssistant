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
    Disconnected,
    Connecting,
    Connected,
    Listening,
    Error,
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionStatus::Disconnected => write!(f, "未连接"),
            ConnectionStatus::Connecting => write!(f, "连接中"),
            ConnectionStatus::Connected => write!(f, "已连接"),
            ConnectionStatus::Listening => write!(f, "监听中"),
            ConnectionStatus::Error => write!(f, "错误"),
        }
    }
}

/// 客户端连接配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub name: String,
    pub protocol: ConnectionType,
    pub server_address: String,
    pub server_port: u16,
    pub timeout: u64,
    pub auto_reconnect: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            name: "新客户端连接".to_string(),
            protocol: ConnectionType::Tcp,
            server_address: "127.0.0.1".to_string(),
            server_port: 8080,
            timeout: 30,
            auto_reconnect: false,
        }
    }
}

impl ClientConfig {
    pub fn new(name: String, server_address: String, server_port: u16, protocol: ConnectionType) -> Self {
        Self {
            name,
            protocol,
            server_address,
            server_port,
            timeout: 30,
            auto_reconnect: false,
        }
    }
}

/// 服务端监听配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub protocol: ConnectionType,
    pub listen_address: String,
    pub listen_port: u16,
    pub max_connections: usize,
    pub timeout: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "新服务端监听".to_string(),
            protocol: ConnectionType::Tcp,
            listen_address: "0.0.0.0".to_string(),
            listen_port: 8080,
            max_connections: 100,
            timeout: 30,
        }
    }
}

impl ServerConfig {
    pub fn new(name: String, listen_address: String, listen_port: u16, protocol: ConnectionType) -> Self {
        Self {
            name,
            protocol,
            listen_address,
            listen_port,
            max_connections: 100,
            timeout: 30,
        }
    }
}

/// 连接配置（统一客户端和服务端）
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}