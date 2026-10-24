use crate::config::connection::ConnectionConfig;
use crate::message::{FavoriteItem, FavoritesMap};
use crate::reply::model::{ReplyRule, ReplyRulesConfig};
use crate::send_task::model::TimedTaskProfile;
use crate::stress::config::StressTestConfig;
use log::{error, warn};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use thiserror::Error;

/// 存储错误类型
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON序列化错误: {0}")]
    Json(#[from] serde_json::Error),
}

/// 一条 MQTT 发布历史（topic + QoS + retain 三元组）。
///
/// 复用 `Qos::try_from(u8)` 映射；用 u8 而非直接 serde `Qos`，避免改动 `Qos` 枚举的派生约束。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MqttTopicHistoryEntry {
    pub topic: String,
    pub qos: u8,
    pub retain: bool,
}

/// 每连接 MQTT 发布历史（按 `connection_id` 索引，与 `FavoritesMap` 同构）。
pub type MqttTopicHistoryMap = HashMap<String, Vec<MqttTopicHistoryEntry>>;

/// 一条持久化的 MQTT 订阅（topic + QoS）。
///
/// 与运行期 `ConnectionTabState::mqtt_subscriptions: Vec<(String, Qos)>` 同构；
/// 用 u8 而非 serde `Qos`，理由同 `MqttTopicHistoryEntry`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MqttSubscriptionEntry {
    pub topic: String,
    pub qos: u8,
}

/// 每连接 MQTT 订阅（按 `connection_id` 索引）。
pub type MqttSubscriptionMap = HashMap<String, Vec<MqttSubscriptionEntry>>;

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 连接列表：本版本不认识的条目会原样保留并在保存时写回，避免旧版本吃掉新版本的数据
    ///
    /// 见 `ConnectionList`。
    #[serde(default)]
    pub connections: ConnectionList,
    pub auto_save: bool,
    pub save_interval: u64,
    /// 旧版本的窗口位置字段：仅作首次升级时迁移到 `window_state.json` 的来源。
    /// 新代码不再写入（窗口状态由 `WindowStateStore` 独立持久化，避免两份内存快照互相覆盖）。
    pub window_x: Option<f64>,
    pub window_y: Option<f64>,
    pub window_width: Option<f64>,
    pub window_height: Option<f64>,
    pub sidebar_width: Option<f64>,
    pub sidebar_collapsed: Option<bool>,
    #[serde(default)]
    pub favorites: FavoritesMap,
    /// 压测配置(按 connection_id 索引)
    #[serde(default)]
    pub stress_profiles: HashMap<String, StressTestConfig>,
    /// 定时任务(心跳)配置(按 connection_id 索引)
    #[serde(default)]
    pub timed_tasks: HashMap<String, TimedTaskProfile>,
    /// 界面语言（如 "zh-CN" / "en"），None 表示用户未选择过
    #[serde(default)]
    pub language: Option<String>,
    /// 回复规则集（全局表 + `scope` 字段区分作用域）
    ///
    /// 旧配置无此字段 → `serde(default)` 得到"空规则集 + 总开关关闭"，
    /// 网络层走既有固定回复路径，**升级后行为与升级前完全一致**。
    #[serde(default)]
    pub reply_rules: ReplyRulesConfig,
    /// 「全局作用域回复规则」在哪些连接上启用（按 connection_id 索引）
    ///
    /// 查询契约：**缺省 = false**（未列出即未启用）。因此升级前已存在的连接、
    /// 以及新建连接都默认不启用，需用户在连接面板逐个打开；无需迁移标记，
    /// 也无需在 `add_connection` 播种。
    #[serde(default)]
    pub reply_connection_enabled: HashMap<String, bool>,
    /// MQTT 发布 topic 历史（按 connection_id 索引）
    ///
    /// 旧配置无此字段 → `serde(default)` 得空 map，行为与升级前一致（无历史可下拉）。
    #[serde(default)]
    pub mqtt_topic_history: MqttTopicHistoryMap,
    /// MQTT 订阅（按 connection_id 索引）：重开标签页 / 重启后自动恢复
    ///
    /// 旧配置无此字段 → `serde(default)` 得空 map，行为与升级前一致（不自动订阅）。
    #[serde(default)]
    pub mqtt_subscriptions: MqttSubscriptionMap,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            connections: ConnectionList::default(),
            auto_save: true,
            save_interval: 30,
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            sidebar_width: None,
            sidebar_collapsed: None,
            favorites: HashMap::new(),
            stress_profiles: HashMap::new(),
            timed_tasks: HashMap::new(),
            language: None,
            reply_rules: ReplyRulesConfig::default(),
            reply_connection_enabled: HashMap::new(),
            mqtt_topic_history: HashMap::new(),
            mqtt_subscriptions: HashMap::new(),
        }
    }
}

/// 连接配置列表：强类型条目 + 本版本无法识别的原始条目。
///
/// 配置文件由所有版本共用。更新版本可能写入本版本不认识的连接（例如新增协议 `mqtt`）。
/// 旧版本读到时若直接丢弃，它随后的一次保存（切窗口尺寸、增删连接都会触发）就会把
/// 这条连接从文件里抹掉，用户回到新版本也找不回来——所以这里只做"透传"：
/// 无法解析的条目按原始 JSON 保留，保存时插回原来的下标位置。
///
/// `Deref`/`DerefMut` 到 `Vec<ConnectionConfig>`：业务代码只会看到自己认得的条目，
/// 未识别条目不参与任何逻辑，也不会被"删除连接"这类操作误伤。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConnectionList {
    connections: Vec<ConnectionConfig>,
    /// 本版本无法解析的原始条目，及其在原数组中的下标（用于保存时插回原位）
    unrecognized: Vec<(usize, serde_json::Value)>,
}

impl Deref for ConnectionList {
    type Target = Vec<ConnectionConfig>;

    fn deref(&self) -> &Self::Target {
        &self.connections
    }
}

impl DerefMut for ConnectionList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connections
    }
}

impl Serialize for ConnectionList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // 把未识别条目按下标插回，让写回的数组顺序与用户上次看到的顺序一致
        let mut merged: Vec<EntryRef<'_>> =
            self.connections.iter().map(EntryRef::Known).collect();
        for (index, raw) in &self.unrecognized {
            merged.insert((*index).min(merged.len()), EntryRef::Raw(raw));
        }

        let mut seq = serializer.serialize_seq(Some(merged.len()))?;
        for entry in &merged {
            seq.serialize_element(entry)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for ConnectionList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
        let mut connections = Vec::with_capacity(raw.len());
        let mut unrecognized = Vec::new();

        for (index, item) in raw.into_iter().enumerate() {
            match serde_json::from_value::<ConnectionConfig>(item.clone()) {
                Ok(connection) => connections.push(connection),
                Err(e) => {
                    error!(
                        "=== 第 {} 条连接本版本无法识别，已原样保留(可能来自更新版本): {} ===",
                        index + 1,
                        e
                    );
                    unrecognized.push((index, item));
                }
            }
        }

        Ok(Self {
            connections,
            unrecognized,
        })
    }
}

/// `ConnectionList` 序列化时的单个元素：强类型条目走自身 `Serialize`
///（保留字段顺序），未识别条目原样输出
enum EntryRef<'a> {
    Known(&'a ConnectionConfig),
    Raw(&'a serde_json::Value),
}

impl Serialize for EntryRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            EntryRef::Known(connection) => connection.serialize(serializer),
            EntryRef::Raw(raw) => raw.serialize(serializer),
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
    ///
    /// 该函数不会失败：配置目录建不出来、或既有配置文件解析不了时，都以默认配置继续，
    /// 绝不 panic。用户侧"读不动配置 → 永远起不来且无法自愈"是不可接受的。
    /// 解析失败时先把坏文件改名备份，保留用户手工修复/找回连接的可能。
    pub fn new() -> Self {
        let config_dir = Self::get_config_dir();
        let config_file = config_dir.join("netassistant_config.json");

        // 确保配置目录存在；失败只告警：后续保存会静默失败，但不应阻止启动
        if let Err(e) = fs::create_dir_all(&config_dir) {
            warn!(
                "=== 创建配置目录失败({}): {} ===",
                config_dir.display(),
                e
            );
        }

        let config = if config_file.exists() {
            match Self::load_from_file(&config_file) {
                Ok(config) => config,
                Err(e) => {
                    error!("=== 配置文件无法解析，回退默认配置: {} ===", e);
                    match Self::backup_unreadable(&config_file) {
                        Some(backup) => error!(
                            "=== 原文件已备份到 {} (修复后可手工还原) ===",
                            backup.display()
                        ),
                        None => error!("=== 备份原文件失败，下次保存将覆盖它 ==="),
                    }
                    AppConfig::default()
                }
            }
        } else {
            AppConfig::default()
        };

        Self {
            config_file,
            config,
        }
    }

    /// 把无法解析的配置文件改名备份，避免下次启动再读到它。
    ///
    /// 返回备份后的路径；重命名失败(如文件被占用)时返回 `None`，
    /// 调用方仍应以默认配置继续启动。
    fn backup_unreadable(path: &Path) -> Option<PathBuf> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = path.with_extension(format!("corrupted-{}.json", stamp));
        fs::rename(path, &backup).ok().map(|_| backup)
    }

    /// 加载窗口位置和尺寸（旧版配置字段，仅用于首次迁移）。
    ///
    /// 新版本的窗口状态由 `WindowStateStore` 读取 `window_state.json`；
    /// 这里保留只为让升级用户不丢失一次启动位置。
    pub fn load_window_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        match (
            self.config.window_x,
            self.config.window_y,
            self.config.window_width,
            self.config.window_height,
        ) {
            (Some(x), Some(y), Some(width), Some(height)) => Some((x, y, width, height)),
            _ => None,
        }
    }

    /// 保存侧边栏宽度
    pub fn save_sidebar_width(&mut self, width: f64) {
        self.config.sidebar_width = Some(width);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 加载侧边栏宽度
    pub fn load_sidebar_width(&self) -> Option<f64> {
        self.config.sidebar_width
    }

    /// 保存侧边栏折叠状态
    pub fn save_sidebar_collapsed(&mut self, collapsed: bool) {
        self.config.sidebar_collapsed = Some(collapsed);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 加载侧边栏折叠状态
    pub fn load_sidebar_collapsed(&self) -> Option<bool> {
        self.config.sidebar_collapsed
    }

    /// 保存界面语言
    pub fn save_language(&mut self, language: &str) {
        self.config.language = Some(language.to_string());
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 加载界面语言
    pub fn load_language(&self) -> Option<String> {
        self.config.language.clone()
    }

    /// 获取配置目录路径
    pub fn get_config_dir() -> PathBuf {
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
    ///
    /// 采用原子写：先写同目录临时文件并 `sync_all`，再 `rename` 覆盖目标。
    /// 直接 `fs::write` 会在截断后崩溃/断电时留下空文件或半截 JSON，
    /// 导致整份配置（连接、收藏、规则）丢失。
    fn save_to_file(path: &Path, config: &AppConfig) -> Result<(), StorageError> {
        let content = serde_json::to_string_pretty(config)?;
        write_atomic(path, content.as_bytes())
    }

    /// 保存配置
    pub fn save(&self) -> Result<(), StorageError> {
        Self::save_to_file(&self.config_file, &self.config)
    }

    /// 自动保存并记录失败原因。
    ///
    /// 原先各处 `let _ = self.save();` 会把磁盘满、文件被占用等错误完全吞掉，
    /// 用户只会看到"设置又丢了"却没有任何线索。
    fn save_and_log(&self) {
        if let Err(e) = self.save() {
            warn!("=== 保存配置失败({}): {} ===", self.config_file.display(), e);
        }
    }

    /// 添加连接配置
    pub fn add_connection(&mut self, connection: ConnectionConfig) {
        self.config.connections.push(connection);
        if self.config.auto_save {
            self.save_and_log();
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
        // 连带删除该连接的定时任务(心跳)配置
        self.config.timed_tasks.remove(connection_id);
        // 连带删除该连接的「自动回复」开关与规则，避免残留孤儿规则
        self.config.reply_connection_enabled.remove(connection_id);
        self.config.reply_rules.connections.remove(connection_id);
        // 连带删除该连接的 MQTT 发布 topic 历史与订阅，避免孤儿数据
        self.config.mqtt_topic_history.remove(connection_id);
        self.config.mqtt_subscriptions.remove(connection_id);
        if self.config.auto_save {
            self.save_and_log();
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
        // 连带删除该连接的定时任务(心跳)配置
        self.config.timed_tasks.remove(connection_id);
        // 连带删除该连接的「自动回复」开关与规则，避免残留孤儿规则
        self.config.reply_connection_enabled.remove(connection_id);
        self.config.reply_rules.connections.remove(connection_id);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 更新连接配置
    pub fn update_connection(&mut self, connection: ConnectionConfig) {
        if let Some(index) = self
            .config
            .connections
            .iter()
            .position(|c| c.id() == connection.id())
        {
            self.config.connections[index] = connection;
            if self.config.auto_save {
                self.save_and_log();
            }
        }
    }

    pub fn add_favorite(&mut self, connection_id: &str, item: FavoriteItem) {
        self.config
            .favorites
            .entry(connection_id.to_string())
            .or_default()
            .push(item);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    pub fn remove_favorite(&mut self, connection_id: &str, favorite_id: &str) {
        if let Some(list) = self.config.favorites.get_mut(connection_id) {
            list.retain(|item| item.id != favorite_id);
            if list.is_empty() {
                self.config.favorites.remove(connection_id);
            }
        }
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    pub fn get_favorites_ref(&self, connection_id: &str) -> &[FavoriteItem] {
        self.config
            .favorites
            .get(connection_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn find_favorite_by_content(
        &self,
        connection_id: &str,
        content: &str,
    ) -> Option<FavoriteItem> {
        self.config
            .favorites
            .get(connection_id)
            .and_then(|list| list.iter().find(|item| item.content == content).cloned())
    }

    /// 记录一条 MQTT 发布历史：以 topic 为键去重（命中则更新 QoS/retain 并置顶），
    /// 每连接上限 20，超出截断尾部。
    pub fn push_mqtt_topic_history(&mut self, connection_id: &str, entry: MqttTopicHistoryEntry) {
        const MAX_HISTORY: usize = 20;
        let list = self
            .config
            .mqtt_topic_history
            .entry(connection_id.to_string())
            .or_default();
        list.retain(|e| e.topic != entry.topic);
        list.insert(0, entry);
        list.truncate(MAX_HISTORY);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 读取指定连接的 MQTT 发布历史（最近优先）。
    pub fn mqtt_topic_history(&self, connection_id: &str) -> &[MqttTopicHistoryEntry] {
        self.config
            .mqtt_topic_history
            .get(connection_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 清空指定连接的 MQTT 发布历史。
    pub fn clear_mqtt_topic_history(&mut self, connection_id: &str) {
        self.config.mqtt_topic_history.remove(connection_id);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 读取指定连接的持久化 MQTT 订阅（按记录顺序）。
    pub fn mqtt_subscriptions(&self, connection_id: &str) -> &[MqttSubscriptionEntry] {
        self.config
            .mqtt_subscriptions
            .get(connection_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 新增/更新一条 MQTT 订阅：同 topic 去重（命中则更新 QoS），保持插入顺序。
    pub fn add_mqtt_subscription(&mut self, connection_id: &str, topic: &str, qos: u8) {
        let list = self
            .config
            .mqtt_subscriptions
            .entry(connection_id.to_string())
            .or_default();
        match list.iter_mut().find(|e| e.topic == topic) {
            Some(e) => e.qos = qos,
            None => list.push(MqttSubscriptionEntry {
                topic: topic.to_string(),
                qos,
            }),
        }
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 删除指定连接的一条 MQTT 订阅。
    pub fn remove_mqtt_subscription(&mut self, connection_id: &str, topic: &str) {
        if let Some(list) = self.config.mqtt_subscriptions.get_mut(connection_id) {
            list.retain(|e| e.topic != topic);
        }
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 编辑指定连接的一条 MQTT 订阅：`old_topic` → `new_topic` 并更新 QoS，**保持原列表位置**。
    ///
    /// - `old == new` 时退化为仅更新 QoS；
    /// - 命中多条同 topic 只改第一条（调用方已前置判重，正常不会出现）；
    /// - `new_topic` 命中其它条目时不做合并（由调用方负责拒绝），行为与 `add` 的幂等语义区分。
    pub fn update_mqtt_subscription(
        &mut self,
        connection_id: &str,
        old_topic: &str,
        new_topic: &str,
        qos: u8,
    ) {
        if let Some(list) = self.config.mqtt_subscriptions.get_mut(connection_id) {
            if let Some(e) = list.iter_mut().find(|e| e.topic == old_topic) {
                e.topic = new_topic.to_string();
                e.qos = qos;
            }
        }
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 获取指定连接的压测配置(回填用)
    pub fn get_stress_profile(&self, connection_id: &str) -> Option<&StressTestConfig> {
        self.config.stress_profiles.get(connection_id)
    }

    /// 保存(或更新)指定连接的压测配置
    pub fn save_stress_profile(&mut self, connection_id: &str, config: StressTestConfig) {
        self.config
            .stress_profiles
            .insert(connection_id.to_string(), config);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 获取指定连接的定时任务(心跳)配置(回填用)
    pub fn get_timed_profile(&self, connection_id: &str) -> Option<&TimedTaskProfile> {
        self.config.timed_tasks.get(connection_id)
    }

    /// 保存(或更新)指定连接的定时任务配置
    pub fn save_timed_profile(&mut self, connection_id: &str, profile: TimedTaskProfile) {
        self.config
            .timed_tasks
            .insert(connection_id.to_string(), profile);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 删除指定连接的定时任务配置
    pub fn remove_timed_profile(&mut self, connection_id: &str) {
        self.config.timed_tasks.remove(connection_id);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    // ========================================================================
    // 回复规则（按连接分组：connection_id -> Vec<ReplyRule>）
    // 规则只属于某一个连接，其他连接看不到；连接删除时级联删除。
    // ========================================================================

    /// 读取回复规则集（回填规则管理弹窗用）
    pub fn reply_rules(&self) -> &ReplyRulesConfig {
        &self.config.reply_rules
    }

    /// 读取某个连接的规则列表（空则返回空切片）
    pub fn rules_for_connection(&self, connection_id: &str) -> &[ReplyRule] {
        self.config
            .reply_rules
            .connections
            .get(connection_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 整表替换回复规则集（UI 保存后一次性下发）
    pub fn save_reply_rules(&mut self, config: ReplyRulesConfig) {
        self.config.reply_rules = config;
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 指定连接是否启用「自动回复」总闸。**缺省 false**（未列出即未启用）
    pub fn reply_connection_enabled(&self, connection_id: &str) -> bool {
        self.config
            .reply_connection_enabled
            .get(connection_id)
            .copied()
            .unwrap_or(false)
    }

    /// 设置指定连接的「自动回复」总闸（写入 + 按需 auto_save）
    pub fn set_reply_connection_enabled(&mut self, connection_id: &str, enabled: bool) {
        self.config
            .reply_connection_enabled
            .insert(connection_id.to_string(), enabled);
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 连接开关整表快照（供下发运行期 store）
    pub fn reply_connection_enabled_map(&self) -> HashMap<String, bool> {
        self.config.reply_connection_enabled.clone()
    }

    /// 在指定连接下新增或更新一条规则
    ///
    /// 按 `id` 匹配：存在则**原地替换**（保持列表位置），不存在则追加到末尾。
    /// 原地替换而非"先删后加"是必要的：规则顺序即求值优先级，先删后加会把
    /// 用户调好的顺序打乱。
    pub fn upsert_reply_rule(&mut self, connection_id: &str, rule: ReplyRule) {
        let rules = self
            .config
            .reply_rules
            .connections
            .entry(connection_id.to_string())
            .or_default();
        match rules.iter_mut().find(|r| r.id == rule.id) {
            Some(existing) => *existing = rule,
            None => rules.push(rule),
        }
        if self.config.auto_save {
            self.save_and_log();
        }
    }

    /// 在指定连接下按 id 删除一条规则；返回是否确实删除了
    pub fn delete_reply_rule(&mut self, connection_id: &str, rule_id: &str) -> bool {
        let Some(rules) = self.config.reply_rules.connections.get_mut(connection_id) else {
            return false;
        };
        let before = rules.len();
        rules.retain(|r| r.id != rule_id);
        let removed = before != rules.len();
        if removed && self.config.auto_save {
            self.save_and_log();
        }
        removed
    }

    /// 按当前列表顺序重写指定连接全部 `priority`（列表顺序 = 求值顺序）
    ///
    /// `index * 10` 留出间隔，便于手工微调；返回被改动的规则数。
    pub fn renumber_reply_rule_priorities(&mut self, connection_id: &str) -> usize {
        let Some(rules) = self.config.reply_rules.connections.get_mut(connection_id) else {
            return 0;
        };
        let mut changed = 0;
        for (i, rule) in rules.iter_mut().enumerate() {
            let priority = (i as u32) * 10;
            if rule.priority != priority {
                rule.priority = priority;
                changed += 1;
            }
        }
        if changed > 0 && self.config.auto_save {
            self.save_and_log();
        }
        changed
    }

    /// 把指定连接下的某条规则移动到新位置（上移 / 下移），并重排 priority
    pub fn move_reply_rule(
        &mut self,
        connection_id: &str,
        rule_id: &str,
        new_index: usize,
    ) -> bool {
        let Some(rules) = self.config.reply_rules.connections.get_mut(connection_id) else {
            return false;
        };
        let Some(from) = rules.iter().position(|r| r.id == rule_id) else {
            return false;
        };
        let to = new_index.min(rules.len().saturating_sub(1));
        if from == to {
            return false;
        }
        let rule = rules.remove(from);
        rules.insert(to, rule);
        self.renumber_reply_rule_priorities(connection_id);
        true
    }
}

impl Default for ConfigStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// 临时文件序号：避免同一进程内并发保存时互相覆盖临时文件
static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 原子写文件：写临时文件 → `sync_all` → `rename` 覆盖目标。
///
/// 崩溃/断电只会留下无人引用的临时文件，目标文件要么是旧内容、要么是完整新内容。
fn write_atomic(path: &Path, content: &[u8]) -> Result<(), StorageError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.json".to_string());
    let tmp = parent.join(format!(
        ".{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content)?;
        // 先落盘再 rename：确保 rename 成功时内容已经持久化
        file.sync_all()
    })();
    if let Err(e) = write_result {
        // 写入/落盘失败时清理半个临时文件，避免长期累积
        let _ = fs::remove_file(&tmp);
        return Err(StorageError::Io(e));
    }

    // Windows 上杀毒软件/索引器可能短暂占用目标文件，重试几次
    let mut last_err = None;
    for _ in 0..3 {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err(StorageError::Io(
        last_err.unwrap_or_else(|| std::io::Error::other("rename 失败")),
    ))
}

/// 窗口位置/尺寸的独立持久化文件
const WINDOW_STATE_FILE: &str = "window_state.json";
/// 抖动静默期：最后一次窗口变化后超过该时长才落盘
const WINDOW_STATE_DEBOUNCE: Duration = Duration::from_millis(1000);

/// 持久化的窗口状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WindowState {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
}

struct WindowStateShared {
    path: PathBuf,
    inner: Mutex<WindowStateInner>,
    cvar: Condvar,
    writer: Mutex<Option<JoinHandle<()>>>,
}

struct WindowStateInner {
    /// 最近一次已知的窗口状态（位置为 `None` 时保留旧值）
    latest: WindowState,
    /// 待落盘状态及最后一次更新时间（用于防抖）
    pending: Option<(WindowState, Instant)>,
    stop: bool,
}

/// 窗口状态存储：独立文件 + 单写线程 + 防抖。
///
/// 不复用 `netassistant_config.json` 的原因：整份配置由 UI 线程持有的
/// `ConfigStorage` 内存快照写入，而窗口移动是高频事件。两者共用一个文件时，
/// 任一方的保存都会用自己内存里的旧值覆盖对方的最新值（表现为"移动窗口后
/// 重启，位置又回去了"）。拆成独立文件后：
///   - 窗口状态只有本类型一个写者；
///   - 配置写者不再持有窗口字段；
///   - UI 线程只更新内存，磁盘 I/O 交给后台线程防抖执行。
#[derive(Clone)]
pub struct WindowStateStore {
    shared: Arc<WindowStateShared>,
}

impl WindowStateStore {
    /// 创建存储并启动唯一的后台写线程
    pub fn new() -> Self {
        Self::with_path(ConfigStorage::get_config_dir().join(WINDOW_STATE_FILE))
    }

    fn with_path(path: PathBuf) -> Self {
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            warn!("=== 创建配置目录失败({}): {} ===", parent.display(), e);
        }

        let latest = match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<WindowState>(&content) {
                Ok(state) => state,
                Err(e) => {
                    warn!("=== 窗口状态文件无法解析，按默认处理: {} ===", e);
                    WindowState::default()
                }
            },
            Err(_) => WindowState::default(),
        };

        let shared = Arc::new(WindowStateShared {
            path,
            inner: Mutex::new(WindowStateInner {
                latest,
                pending: None,
                stop: false,
            }),
            cvar: Condvar::new(),
            writer: Mutex::new(None),
        });

        let worker = Arc::clone(&shared);
        match std::thread::Builder::new()
            .name("window-state-writer".into())
            .spawn(move || writer_loop(worker))
        {
            Ok(handle) => {
                *shared.writer.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
            }
            Err(e) => warn!(
                "=== 启动窗口状态写线程失败，窗口位置仅在退出时保存: {} ===",
                e
            ),
        }

        Self { shared }
    }

    /// 启动时读取上次保存的窗口位置（四元组齐全才返回）
    pub fn initial_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let inner = self.shared.inner.lock().unwrap_or_else(|e| e.into_inner());
        let state = &inner.latest;
        match (state.x, state.y) {
            (Some(x), Some(y)) if state.width > 0.0 && state.height > 0.0 => {
                Some((x, y, state.width, state.height))
            }
            _ => None,
        }
    }

    /// 请求保存：只更新内存，由后台线程在静默期后原子落盘（UI 线程不做磁盘 I/O）
    pub fn request_save(&self, x: Option<f64>, y: Option<f64>, width: f64, height: f64) {
        let mut inner = self.shared.inner.lock().unwrap_or_else(|e| e.into_inner());
        if x.is_some() {
            inner.latest.x = x;
        }
        if y.is_some() {
            inner.latest.y = y;
        }
        inner.latest.width = width;
        inner.latest.height = height;
        inner.pending = Some((inner.latest.clone(), Instant::now()));
        drop(inner);
        self.shared.cvar.notify_one();
    }

    /// 退出前同步落盘最后一个待写状态。
    ///
    /// 进程随后会 `process::exit`，后台线程不会被执行器等待，因此必须在这里
    /// 先让写线程退出、再同步写一次（写线程退出后不会再有旧值覆盖新值）。
    pub fn flush(&self) {
        {
            let mut inner = self.shared.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.stop = true;
            self.shared.cvar.notify_all();
        }
        let handle = self
            .shared
            .writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }

        let pending = {
            let mut inner = self.shared.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.pending.take()
        };
        if let Some((state, _)) = pending
            && let Err(e) = save_window_state(&self.shared.path, &state)
        {
            warn!("=== 保存窗口状态失败: {} ===", e);
        }
    }
}

impl Default for WindowStateStore {
    fn default() -> Self {
        Self::new()
    }
}

fn writer_loop(shared: Arc<WindowStateShared>) {
    loop {
        let mut inner = shared.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.stop {
            return;
        }

        let wait = match inner.pending.as_ref() {
            Some((_, updated_at)) => WINDOW_STATE_DEBOUNCE.saturating_sub(updated_at.elapsed()),
            None => {
                // 无待写数据：无限等待，由 request_save/flush 唤醒（不轮询，避免无谓唤醒）
                let guard = shared
                    .cvar
                    .wait(inner)
                    .unwrap_or_else(|e| e.into_inner());
                drop(guard);
                continue;
            }
        };

        if wait.is_zero() {
            let Some((state, _)) = inner.pending.take() else {
                // 理论上不可达（wait 为 0 意味着 pending 存在）；保持防御，避免空转
                continue;
            };
            drop(inner);
            if let Err(e) = save_window_state(&shared.path, &state) {
                warn!("=== 保存窗口状态失败: {} ===", e);
            }
            continue;
        }

        let (guard, _) = shared
            .cvar
            .wait_timeout(inner, wait)
            .unwrap_or_else(|e| e.into_inner());
        drop(guard);
    }
}

fn save_window_state(path: &Path, state: &WindowState) -> Result<(), StorageError> {
    let content = serde_json::to_string_pretty(state)?;
    write_atomic(path, content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection::ClientConfig;
    use crate::reply::model::{BytePattern, MatchNode, ReplyPayload, ReplyRule, RuleCodec};

    fn rule(name: &str) -> ReplyRule {
        ReplyRule {
            matcher: MatchNode::Length { min: 1, max: 8 },
            payload: ReplyPayload {
                text: "ok".to_string(),
                hex_mode: false,
                codec: RuleCodec::Raw,
            },
            // 与 UI「新建规则」一致：priority 由列表位置决定，初始都是 10
            ..ReplyRule::new(name, 10)
        }
    }

    /// 构造一个不落盘的测试存储
    fn test_storage() -> ConfigStorage {
        ConfigStorage {
            config_file: PathBuf::from("unused.json"),
            config: AppConfig {
                auto_save: false,
                ..Default::default()
            },
        }
    }

    /// 旧配置文件（无 `reply_rules` 字段）必须无损加载：得到空规则集
    #[test]
    fn test_legacy_app_config_loads_without_reply_rules() {
        let json = r#"{
        "connections": [],
        "auto_save": true,
        "save_interval": 30,
        "window_x": null,
        "window_y": null,
        "window_width": null,
        "window_height": null,
        "sidebar_width": null,
        "sidebar_collapsed": null
    }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.reply_rules.connections.is_empty());
        assert_eq!(config.reply_rules.version, 1);
        // 行为等价于"升级前"：无规则可命中
        assert!(config.reply_rules.connections.values().all(|rules| rules.is_empty()));
    }

    /// 更新版本写入的未知协议必须原样保留并插回原下标：
    /// 旧版本保存一次之后，用户回到新版本仍然能读到这条连接。
    ///
    /// 本机实际踩到的就是这个坑：旧版本读到新版本写入的 `protocol: "mqtt"` 后整份失败，
    /// 启动直接 panic。这里用一个本版本必然不认识的协议值复现该场景。
    #[test]
    fn test_unrecognized_connection_survives_old_version_save() {
        let json = r#"{
        "connections": [
            {"type":"Client","config":{"id":"keep-1","protocol":"tcp","server_address":"127.0.0.1","server_port":12345}},
            {"type":"Client","config":{"id":"from-future","protocol":"somefutureproto","server_address":"10.0.0.82","server_port":1883}},
            {"type":"Client","config":{"id":"keep-2","protocol":"udp","server_address":"127.0.0.1","server_port":54321}}
        ],
        "auto_save": true,
        "save_interval": 30
    }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();

        // 业务侧只看到自己认得的条目
        let ids: Vec<&str> = config.connections.iter().map(|c| c.id()).collect();
        assert_eq!(ids, vec!["keep-1", "keep-2"]);

        // 旧版本保存后：未识别条目仍在原下标，内容一字未改
        let saved: serde_json::Value = serde_json::to_value(&config).unwrap();
        let saved = saved["connections"].as_array().expect("connections 应为数组");
        assert_eq!(saved.len(), 3, "未识别条目不能被丢弃");
        assert_eq!(saved[0]["config"]["id"], "keep-1");
        assert_eq!(saved[1]["config"]["id"], "from-future");
        assert_eq!(saved[1]["config"]["protocol"], "somefutureproto");
        assert_eq!(saved[1]["config"]["server_port"], 1883);
        assert_eq!(saved[2]["config"]["id"], "keep-2");
    }

    /// 整份配置无法解析时，坏文件被改名备份而不是留在原地反复失败
    #[test]
    fn test_unreadable_config_is_backed_up_before_falling_back() {
        let dir = std::env::temp_dir().join("na_storage_backup_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("netassistant_config.json");
        fs::write(&file, "{ 截断的 json").unwrap();

        let backup = ConfigStorage::backup_unreadable(&file).expect("应能备份坏文件");
        assert!(!file.exists(), "坏文件不应留在原路径");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{ 截断的 json");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 含规则集的配置序列化往返一致
    #[test]
    fn test_app_config_reply_rules_roundtrip() {
        let mut config = AppConfig::default();
        config.reply_rules.connections.insert(
            "conn-1".to_string(),
            vec![ReplyRule {
                matcher: MatchNode::Contains {
                    bytes: BytePattern::Hex("01 03".to_string()),
                },
                ..rule("Modbus")
            }],
        );

        let json = serde_json::to_string_pretty(&config).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.reply_rules, config.reply_rules);
        assert!(back
            .reply_rules
            .connections
            .values()
            .any(|rules| rules.iter().any(|r| r.enabled)));
    }

    /// upsert 必须**原地替换**（保持列表顺序 = 求值优先级），新规则追加到末尾
    #[test]
    fn test_upsert_keeps_order() {
        let mut storage = test_storage();
        let a = rule("A");
        let b = rule("B");
        let c = rule("C");
        storage.upsert_reply_rule("conn-1", a.clone());
        storage.upsert_reply_rule("conn-1", b.clone());
        storage.upsert_reply_rule("conn-1", c.clone());
        assert_eq!(
            storage
                .rules_for_connection("conn-1")
                .iter()
                .map(|r| r.name.clone())
                .collect::<Vec<_>>(),
            vec!["A", "B", "C"]
        );

        // 改 A 不应把它挪到末尾
        let mut a2 = a.clone();
        a2.name = "A2".to_string();
        storage.upsert_reply_rule("conn-1", a2);
        assert_eq!(
            storage
                .rules_for_connection("conn-1")
                .iter()
                .map(|r| r.name.clone())
                .collect::<Vec<_>>(),
            vec!["A2", "B", "C"]
        );
        assert_eq!(storage.rules_for_connection("conn-1").len(), 3);
    }

    /// 不同连接的规则完全隔离：互不可见、互不影响
    #[test]
    fn test_rules_isolated_per_connection() {
        let mut storage = test_storage();
        storage.upsert_reply_rule("conn-1", rule("A"));
        storage.upsert_reply_rule("conn-2", rule("B"));
        assert_eq!(storage.rules_for_connection("conn-1").len(), 1);
        assert_eq!(storage.rules_for_connection("conn-2").len(), 1);
        assert_eq!(storage.rules_for_connection("conn-1")[0].name, "A");
        assert_eq!(storage.rules_for_connection("conn-2")[0].name, "B");
        // 未建规则的连接返回空切片
        assert!(storage.rules_for_connection("conn-x").is_empty());
    }

    /// 删除与连接开关
    #[test]
    fn test_delete_and_toggles() {
        let mut storage = test_storage();
        let a = rule("A");
        let id = a.id.clone();
        storage.upsert_reply_rule("conn-1", a);
        assert!(storage.delete_reply_rule("conn-1", &id));
        assert!(
            !storage.delete_reply_rule("conn-1", &id),
            "重复删除应返回 false"
        );
        assert!(storage.rules_for_connection("conn-1").is_empty());
        // 删除不存在的连接返回 false
        assert!(!storage.delete_reply_rule("conn-x", &id));

        storage.set_reply_connection_enabled("conn-1", true);
        assert!(storage.reply_connection_enabled("conn-1"));
    }

    /// 连接开关持久化语义：缺省 false、显式设置后可读、删除连接连带清理、serde 往返
    #[test]
    fn test_reply_connection_enabled_defaults_and_cleanup() {
        let mut storage = test_storage();
        // 缺省 false（未列出的连接一律视为未启用）
        assert!(!storage.reply_connection_enabled("conn-a"));

        storage.set_reply_connection_enabled("conn-a", true);
        assert!(storage.reply_connection_enabled("conn-a"));
        assert_eq!(
            storage
                .reply_connection_enabled_map()
                .get("conn-a")
                .copied(),
            Some(true)
        );

        storage.set_reply_connection_enabled("conn-a", false);
        assert!(!storage.reply_connection_enabled("conn-a"));

        // 删除连接连带清理开关与规则
        storage.add_connection(ConnectionConfig::Client(ClientConfig {
            id: "conn-b".to_string(),
            ..Default::default()
        }));
        storage.set_reply_connection_enabled("conn-b", true);
        storage.upsert_reply_rule("conn-b", rule("B"));
        storage.remove_client_connection("conn-b");
        assert!(
            !storage
                .reply_connection_enabled_map()
                .contains_key("conn-b")
        );
        assert!(
            !storage.reply_rules().connections.contains_key("conn-b"),
            "删除连接应级联删除其规则"
        );

        // serde 往返保留开关
        storage.set_reply_connection_enabled("conn-c", true);
        let json = serde_json::to_string(&storage.config).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.reply_connection_enabled.get("conn-c").copied(),
            Some(true)
        );
    }

    /// 旧配置无 `reply_connection_enabled` 字段 → 空 map，查询恒为 false（不兼容语义）
    #[test]
    fn test_legacy_config_has_no_reply_connection_enabled() {
        let json = r#"{
        "connections": [],
        "auto_save": true,
        "save_interval": 30,
        "window_x": null,
        "window_y": null,
        "window_width": null,
        "window_height": null,
        "sidebar_width": null,
        "sidebar_collapsed": null
    }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.reply_connection_enabled.is_empty());
    }

    /// MQTT 发布历史：按连接隔离、去重置顶、上限 20、清空、删连接连带清理、serde 往返。
    #[test]
    fn test_mqtt_topic_history() {
        let mut storage = test_storage();
        let entry = |topic: &str, qos: u8, retain: bool| MqttTopicHistoryEntry {
            topic: topic.to_string(),
            qos,
            retain,
        };

        // 初始为空，未建历史的连接返回空切片
        assert!(storage.mqtt_topic_history("conn-1").is_empty());
        assert!(storage.mqtt_topic_history("conn-x").is_empty());

        // 最近优先：新纪录插到队首
        storage.push_mqtt_topic_history("conn-1", entry("a/b", 1, false));
        storage.push_mqtt_topic_history("conn-1", entry("c/d", 0, true));
        let topics: Vec<&str> = storage
            .mqtt_topic_history("conn-1")
            .iter()
            .map(|e| e.topic.as_str())
            .collect();
        assert_eq!(topics, vec!["c/d", "a/b"]);

        // 同 topic 去重：更新 QoS/retain 并置顶，长度不增
        storage.push_mqtt_topic_history("conn-1", entry("a/b", 2, true));
        let list = storage.mqtt_topic_history("conn-1");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].topic, "a/b");
        assert_eq!(list[0].qos, 2);
        assert!(list[0].retain);

        // 按连接隔离：互不可见
        storage.push_mqtt_topic_history("conn-2", entry("x/y", 0, false));
        assert_eq!(storage.mqtt_topic_history("conn-1").len(), 2);
        assert_eq!(storage.mqtt_topic_history("conn-2").len(), 1);
        assert_eq!(storage.mqtt_topic_history("conn-2")[0].topic, "x/y");

        // 上限 20：超出截断尾部（最早的被丢弃）
        for i in 0..25 {
            storage.push_mqtt_topic_history("conn-3", entry(&format!("t/{i}"), 0, false));
        }
        let list3 = storage.mqtt_topic_history("conn-3");
        assert_eq!(list3.len(), 20);
        assert_eq!(list3[0].topic, "t/24", "最新在队首");
        assert_eq!(list3[19].topic, "t/5", "最老的溢出被截断");

        // serde 往返
        let json = serde_json::to_string(&storage.config).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mqtt_topic_history, storage.config.mqtt_topic_history);

        // 清空仅影响当前连接
        storage.clear_mqtt_topic_history("conn-1");
        assert!(storage.mqtt_topic_history("conn-1").is_empty());
        assert_eq!(storage.mqtt_topic_history("conn-2").len(), 1);

        // 删除连接连带清理历史
        storage.add_connection(ConnectionConfig::Client(ClientConfig {
            id: "conn-b".to_string(),
            ..Default::default()
        }));
        storage.push_mqtt_topic_history("conn-b", entry("k/l", 1, false));
        storage.remove_client_connection("conn-b");
        assert!(storage.mqtt_topic_history("conn-b").is_empty());
    }

    /// MQTT 订阅持久化：按连接隔离、同 topic 去重更新 QoS、保序、删除、删连接连带清理、serde 往返。
    #[test]
    fn test_mqtt_subscription_persistence() {
        let mut storage = test_storage();

        // 初始为空
        assert!(storage.mqtt_subscriptions("conn-1").is_empty());

        // 保序追加
        storage.add_mqtt_subscription("conn-1", "a/b", 0);
        storage.add_mqtt_subscription("conn-1", "c/d", 1);
        let list = storage.mqtt_subscriptions("conn-1");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].topic, "a/b");
        assert_eq!(list[0].qos, 0);
        assert_eq!(list[1].topic, "c/d");
        assert_eq!(list[1].qos, 1);

        // 同 topic 去重：仅更新 QoS，长度与顺序不变
        storage.add_mqtt_subscription("conn-1", "a/b", 2);
        let list = storage.mqtt_subscriptions("conn-1");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].topic, "a/b");
        assert_eq!(list[0].qos, 2, "同 topic 应更新 QoS 而非新增");

        // 按连接隔离
        storage.add_mqtt_subscription("conn-2", "x/y", 0);
        assert_eq!(storage.mqtt_subscriptions("conn-1").len(), 2);
        assert_eq!(storage.mqtt_subscriptions("conn-2").len(), 1);

        // serde 往返
        let json = serde_json::to_string(&storage.config).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mqtt_subscriptions, storage.config.mqtt_subscriptions);

        // 删除单条：不影响同连接其它订阅
        storage.remove_mqtt_subscription("conn-1", "a/b");
        let list = storage.mqtt_subscriptions("conn-1");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].topic, "c/d");
        // 删除不存在的 topic 幂等
        storage.remove_mqtt_subscription("conn-1", "nope");
        assert_eq!(storage.mqtt_subscriptions("conn-1").len(), 1);

        // 删除连接连带清理订阅
        storage.add_connection(ConnectionConfig::Client(ClientConfig {
            id: "conn-b".to_string(),
            ..Default::default()
        }));
        storage.add_mqtt_subscription("conn-b", "k/l", 1);
        storage.remove_client_connection("conn-b");
        assert!(storage.mqtt_subscriptions("conn-b").is_empty());
    }

    /// 编辑订阅：保序替换 topic 并更新 QoS；old==new 退化为仅改 QoS；跨连接隔离。
    #[test]
    fn test_update_mqtt_subscription_keeps_order() {
        let mut storage = test_storage();
        storage.add_mqtt_subscription("conn-1", "a/b", 0);
        storage.add_mqtt_subscription("conn-1", "c/d", 1);
        storage.add_mqtt_subscription("conn-1", "e/f", 2);

        // 改 topic + QoS：位置不变
        storage.update_mqtt_subscription("conn-1", "c/d", "c/d2", 2);
        let list = storage.mqtt_subscriptions("conn-1");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].topic, "a/b");
        assert_eq!(list[1].topic, "c/d2", "编辑应就地替换 topic，保持原位置");
        assert_eq!(list[1].qos, 2);
        assert_eq!(list[2].topic, "e/f");

        // old == new：仅更新 QoS
        storage.update_mqtt_subscription("conn-1", "a/b", "a/b", 2);
        let list = storage.mqtt_subscriptions("conn-1");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].topic, "a/b");
        assert_eq!(list[0].qos, 2, "old==new 应退化为仅更新 QoS");

        // 跨连接隔离：conn-2 不受影响，编辑不存在的 topic 幂等
        storage.add_mqtt_subscription("conn-2", "x/y", 0);
        storage.update_mqtt_subscription("conn-1", "nope", "nope2", 1);
        assert_eq!(storage.mqtt_subscriptions("conn-1").len(), 3);
        assert_eq!(storage.mqtt_subscriptions("conn-2").len(), 1);
        assert_eq!(storage.mqtt_subscriptions("conn-2")[0].topic, "x/y");
    }

    /// 旧配置无 `mqtt_subscriptions` 字段 → 空 map（零迁移）
    #[test]
    fn test_legacy_config_has_no_mqtt_subscriptions() {
        let json = r#"{
        "connections": [],
        "auto_save": true,
        "save_interval": 30,
        "window_x": null,
        "window_y": null,
        "window_width": null,
        "window_height": null,
        "sidebar_width": null,
        "sidebar_collapsed": null
    }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.mqtt_subscriptions.is_empty());
    }

    /// 旧配置无 `mqtt_topic_history` 字段 → 空 map（零迁移）
    #[test]
    fn test_legacy_config_has_no_mqtt_topic_history() {
        let json = r#"{
        "connections": [],
        "auto_save": true,
        "save_interval": 30,
        "window_x": null,
        "window_y": null,
        "window_width": null,
        "window_height": null,
        "sidebar_width": null,
        "sidebar_collapsed": null
    }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.mqtt_topic_history.is_empty());
    }

    /// 上移/下移必须真正改变求值顺序，并重排 priority 的间隔
    #[test]
    fn test_move_rule_renumbers_priorities() {
        let mut storage = test_storage();
        let a = rule("A");
        let b = rule("B");
        let c = rule("C");
        let (ia, ib, ic) = (a.id.clone(), b.id.clone(), c.id.clone());
        storage.upsert_reply_rule("conn-1", a);
        storage.upsert_reply_rule("conn-1", b);
        storage.upsert_reply_rule("conn-1", c);
        assert_eq!(
            storage
                .rules_for_connection("conn-1")
                .iter()
                .map(|r| r.priority)
                .collect::<Vec<_>>(),
            vec![10, 10, 10],
            "构造时 priority 都是 10"
        );

        // C 移到最前
        assert!(storage.move_reply_rule("conn-1", &ic, 0));
        let names: Vec<String> = storage
            .rules_for_connection("conn-1")
            .iter()
            .map(|r| r.name.clone())
            .collect();
        assert_eq!(names, vec!["C", "A", "B"]);
        let priorities: Vec<u32> = storage
            .rules_for_connection("conn-1")
            .iter()
            .map(|r| r.priority)
            .collect();
        assert_eq!(priorities, vec![0, 10, 20], "顺序即优先级，间隔 10");

        // 移到越界位置会被夹到末尾
        assert!(storage.move_reply_rule("conn-1", &ic, 99));
        assert_eq!(
            storage.rules_for_connection("conn-1").last().unwrap().id,
            ic
        );

        // 不存在的 id / 位置不变 / 不存在的连接 都返回 false
        assert!(!storage.move_reply_rule("conn-1", "nope", 0));
        assert!(!storage.move_reply_rule("conn-1", &ic, 2));
        assert!(!storage.move_reply_rule("conn-x", &ic, 0));
        // 三个 id 都还在（没有被 move 弄丢）
        for id in [ia, ib, ic] {
            assert!(storage
                .rules_for_connection("conn-1")
                .iter()
                .any(|r| r.id == id));
        }
    }

    /// renumber 只改需要改的，返回值反映实际改动数
    #[test]
    fn test_renumber_reports_changes() {
        let mut storage = test_storage();
        storage.upsert_reply_rule("conn-1", rule("A"));
        storage.upsert_reply_rule("conn-1", rule("B"));
        // 构造时两条规则的 priority 都是 10 → 重排后第二条要改（第一条落到 0）
        assert_eq!(storage.rules_for_connection("conn-1")[0].priority, 10);
        assert_eq!(storage.renumber_reply_rule_priorities("conn-1"), 1);
        let priorities: Vec<u32> = storage
            .rules_for_connection("conn-1")
            .iter()
            .map(|r| r.priority)
            .collect();
        assert_eq!(priorities, vec![0, 10], "列表顺序即优先级");
        assert_eq!(
            storage.renumber_reply_rule_priorities("conn-1"),
            0,
            "已就绪时应为 0"
        );
        // 不存在的连接返回 0
        assert_eq!(storage.renumber_reply_rule_priorities("conn-x"), 0);
    }

    /// 原子写：覆盖目标文件后不残留临时文件，内容为完整新值
    #[test]
    fn test_write_atomic_replaces_without_leaving_temp_files() {
        let dir = std::env::temp_dir().join("na_storage_atomic_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("netassistant_config.json");
        fs::write(&file, "old").unwrap();

        write_atomic(&file, br#"{"a":1}"#).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), r#"{"a":1}"#);

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "不应残留临时文件: {leftovers:?}");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 窗口状态：防抖期间只保留最后一个值，flush 后落盘且可重新读取
    #[test]
    fn test_window_state_store_flushes_latest_bounds() {
        let dir = std::env::temp_dir().join("na_window_state_test");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join(WINDOW_STATE_FILE);

        let store = WindowStateStore::with_path(path.clone());
        assert!(store.initial_bounds().is_none(), "无文件时不应有默认位置");

        store.request_save(Some(10.0), Some(20.0), 800.0, 600.0);
        store.request_save(Some(30.0), Some(40.0), 900.0, 700.0);
        store.flush();
        assert_eq!(store.initial_bounds(), Some((30.0, 40.0, 900.0, 700.0)));

        // 重新打开（模拟下次启动）读到同一份状态
        let reopened = WindowStateStore::with_path(path.clone());
        assert_eq!(reopened.initial_bounds(), Some((30.0, 40.0, 900.0, 700.0)));

        // 位置为 None（窗口关闭时的无效坐标）只更新尺寸，保留旧坐标
        reopened.request_save(None, None, 1000.0, 800.0);
        reopened.flush();
        let after = WindowStateStore::with_path(path.clone());
        assert_eq!(after.initial_bounds(), Some((30.0, 40.0, 1000.0, 800.0)));
        after.flush();

        let _ = fs::remove_dir_all(&dir);
    }
}
