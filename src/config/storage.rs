use crate::config::connection::ConnectionConfig;
use crate::message::{FavoriteItem, FavoritesMap};
use crate::reply::model::{ReplyRule, ReplyRulesConfig};
use crate::send_task::model::TimedTaskProfile;
use crate::stress::config::StressTestConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub save_interval: u64,
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
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

    /// 保存窗口位置和尺寸
    pub fn save_window_bounds(&mut self, x: Option<f64>, y: Option<f64>, width: f64, height: f64) {
        // 只有当位置有效时才更新位置
        if let Some(valid_x) = x {
            self.config.window_x = Some(valid_x);
        }
        if let Some(valid_y) = y {
            self.config.window_y = Some(valid_y);
        }
        // 总是更新尺寸
        self.config.window_width = Some(width);
        self.config.window_height = Some(height);
        if self.config.auto_save {
            let _ = self.save();
        }
    }

    /// 加载窗口位置和尺寸
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
            let _ = self.save();
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
            let _ = self.save();
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
            let _ = self.save();
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
        // 连带删除该连接的定时任务(心跳)配置
        self.config.timed_tasks.remove(connection_id);
        // 连带删除该连接的「自动回复」开关与规则，避免残留孤儿规则
        self.config.reply_connection_enabled.remove(connection_id);
        self.config.reply_rules.connections.remove(connection_id);
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
        // 连带删除该连接的定时任务(心跳)配置
        self.config.timed_tasks.remove(connection_id);
        // 连带删除该连接的「自动回复」开关与规则，避免残留孤儿规则
        self.config.reply_connection_enabled.remove(connection_id);
        self.config.reply_rules.connections.remove(connection_id);
        if self.config.auto_save {
            let _ = self.save();
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
                let _ = self.save();
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
            let _ = self.save();
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
            let _ = self.save();
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
            let _ = self.save();
        }
    }

    /// 删除指定连接的压测配置
    #[allow(dead_code)]
    pub fn remove_stress_profile(&mut self, connection_id: &str) {
        self.config.stress_profiles.remove(connection_id);
        if self.config.auto_save {
            let _ = self.save();
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
            let _ = self.save();
        }
    }

    /// 删除指定连接的定时任务配置
    pub fn remove_timed_profile(&mut self, connection_id: &str) {
        self.config.timed_tasks.remove(connection_id);
        if self.config.auto_save {
            let _ = self.save();
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
            let _ = self.save();
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
            let _ = self.save();
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
            let _ = self.save();
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
            let _ = self.save();
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
            let _ = self.save();
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
        Self::new().expect("无法创建配置存储")
    }
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
}
