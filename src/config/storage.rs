use crate::config::connection::ConnectionConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 存储错误类型
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON序列化错误: {0}")]
    Json(#[from] serde_json::Error),
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub connections: Vec<ConnectionConfig>,
    pub auto_save: bool,
    pub save_interval: u64, // 秒
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            auto_save: true,
            save_interval: 30,
        }
    }
}

/// 配置存储管理器
#[derive(Clone)]
pub struct ConfigStorage {
    config_file: PathBuf,
    config: AppConfig,
}

impl ConfigStorage {
    /// 创建新的配置存储管理器
    pub fn new() -> Result<Self, StorageError> {
        let config_dir = Self::get_config_dir();
        let config_file = config_dir.join("netassistant_config.json");

        // 确保配置目录存在
        fs::create_dir_all(&config_dir)?;

        let config = if config_file.exists() {
            Self::load_from_file(&config_file)?
        } else {
            AppConfig::default()
        };

        Ok(Self {
            config_file,
            config,
        })
    }

    /// 获取配置目录路径
    fn get_config_dir() -> PathBuf {
        if cfg!(windows) {
            let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(appdata).join("NetAssistant")
        } else if cfg!(target_os = "macos") {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("NetAssistant")
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config").join("netassistant")
        }
    }

    /// 从文件加载配置
    fn load_from_file(path: &Path) -> Result<AppConfig, StorageError> {
        let content = fs::read_to_string(path)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// 保存配置到文件
    fn save_to_file(path: &Path, config: &AppConfig) -> Result<(), StorageError> {
        let content = serde_json::to_string_pretty(config)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// 保存配置
    pub fn save(&self) -> Result<(), StorageError> {
        Self::save_to_file(&self.config_file, &self.config)
    }

    /// 添加连接配置
    pub fn add_connection(&mut self, connection: ConnectionConfig) {
        self.config.connections.push(connection);
        if self.config.auto_save {
            let _ = self.save();
        }
    }

    /// 获取客户端连接配置
    pub fn client_connections(&self) -> Vec<&ConnectionConfig> {
        self.config
            .connections
            .iter()
            .filter(|c| c.is_client())
            .collect()
    }

    /// 获取服务端连接配置
    pub fn server_connections(&self) -> Vec<&ConnectionConfig> {
        self.config
            .connections
            .iter()
            .filter(|c| c.is_server())
            .collect()
    }

    /// 按ID删除客户端连接
    pub fn remove_client_connection(&mut self, connection_id: &str) {
        self.config.connections.retain(|c| {
            if let ConnectionConfig::Client(client) = c {
                client.id != connection_id
            } else {
                true
            }
        });
        if self.config.auto_save {
            let _ = self.save();
        }
    }

    /// 按ID删除服务端连接
    pub fn remove_server_connection(&mut self, connection_id: &str) {
        self.config.connections.retain(|c| {
            if let ConnectionConfig::Server(server) = c {
                server.id != connection_id
            } else {
                true
            }
        });
        if self.config.auto_save {
            let _ = self.save();
        }
    }
    
    /// 获取所有连接配置
    pub fn connections(&self) -> Vec<&ConnectionConfig> {
        self.config.connections.iter().collect()
    }

    /// 检查连接是否存在
    pub fn contains_connection(&self, connection: &ConnectionConfig) -> bool {
        self.config.connections.contains(connection)
    }

    /// 保留满足条件的连接
    pub fn retain_connections(&mut self, f: impl FnMut(&ConnectionConfig) -> bool) {
        self.config.connections.retain(f);
        if self.config.auto_save {
            let _ = self.save();
        }
    }
}

impl Default for ConfigStorage {
    fn default() -> Self {
        Self::new().expect("无法创建配置存储")
    }
}
