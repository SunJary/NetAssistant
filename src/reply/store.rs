// 规则集运行期状态（L1 纯逻辑，UI 写 / 网络线程读）
//
// 并发模型（决策 D-6 / D-7）：
//   - 未启用时 `is_enabled()` 走 `AtomicBool` 无锁快速路径，网络线程**零开销**直达返回，
//     连读锁都不碰 —— 这是"未用此功能的用户不受任何影响"的硬保证。
//   - 命中计数用 `Arc<AtomicU64>`：UI 可在锁外遍历计数快照，避免长时间持读锁阻塞
//     `replace()`；压测洪泛下也无写锁竞争。
//   - 规则表本体是 `RwLock<Arc<Vec<Arc<RuleRuntime>>>>`：网络线程读 = 一次读锁 + 一次
//     `Arc` clone，之后完全无锁遍历。UI 改规则是低频整表替换。
//   - 排序只在 `replace()` 时做一次，不在每帧做。

use crate::reply::matcher::{AddrSpec, parse_addr_spec};
use crate::reply::model::{BytePattern, CompiledReply, ReplyRule, ReplyRulesConfig};
use log::warn;
use regex::Regex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// 单条规则的运行期状态
#[derive(Debug)]
pub struct RuleRuntime {
    /// 持久化真源（clone 一份，UI 改规则时随整表替换）
    pub rule: Arc<ReplyRule>,
    /// 命中计数（原子，压测洪泛下无锁）
    pub hits: Arc<AtomicU64>,
    /// 预编译载荷（P-2）：命中时直接 `Arc` 克隆取出，
    /// 不再每帧重新解析模板 / 复制固定字节。
    pub compiled_payload: Arc<CompiledReply>,
    /// 正则惰性编译缓存：`Regex::new` 是重操作，编译失败缓存为 `None`，
    /// 后续恒 false，避免每帧重试编译（决策见 plan-reply-rules.md §5.3）
    pub regex_cache: OnceLock<Option<Regex>>,
    /// 字节模式解析缓存（hex 文本 → 字节）
    pub pattern_cache: OnceLock<Result<Vec<u8>, String>>,
    /// `From` 谓词的地址规格预解析缓存（P-5）：避免每帧重新解析 IP/CIDR
    pub addr_cache: OnceLock<Vec<AddrSpec>>,
}

impl RuleRuntime {
    /// 由规则构造运行期状态
    pub fn new(rule: ReplyRule, hits: Arc<AtomicU64>) -> Arc<Self> {
        let compiled_payload = Arc::new(rule.compiled_payload());
        let runtime = Arc::new(Self {
            rule: Arc::new(rule),
            hits,
            compiled_payload,
            regex_cache: OnceLock::new(),
            pattern_cache: OnceLock::new(),
            addr_cache: OnceLock::new(),
        });
        runtime.prime_caches();
        runtime
    }

    /// 预编译：保存期校验已保证大部分模式合法，但规则也可能来自手工编辑的 JSON，
    /// 因此在构造时就把模式解析结果与正则编译结果固化下来（失败即缓存失败结果）。
    fn prime_caches(&self) {
        self.prime_pattern(&self.rule.matcher);
        self.prime_regex(&self.rule.matcher);
        self.prime_addrs(&self.rule.matcher);
    }

    fn prime_pattern(&self, node: &crate::reply::model::MatchNode) {
        use crate::reply::model::MatchNode;
        match node {
            MatchNode::Contains { bytes }
            | MatchNode::FixedExact { bytes, .. }
            | MatchNode::Suffix { bytes } => {
                let _ = self.pattern(bytes);
            }
            MatchNode::FixedMask { bytes, .. } => {
                let _ = self.pattern(bytes);
            }
            MatchNode::PrefixRange { prefix, .. } => {
                let _ = self.pattern(prefix);
            }
            MatchNode::All { children } | MatchNode::Any { children } => {
                for child in children {
                    self.prime_pattern(child);
                }
            }
            MatchNode::Not { child } => self.prime_pattern(child),
            _ => {}
        }
    }

    fn prime_regex(&self, node: &crate::reply::model::MatchNode) {
        use crate::reply::model::MatchNode;
        match node {
            MatchNode::Regex { pattern } => {
                // 正则长度超限视为不可用（与保存期校验同一判据）
                if pattern.chars().count() > crate::reply::model::MAX_REGEX_LEN {
                    warn!(
                        "[reply] 正则超长，已禁用该谓词: {} 字符",
                        pattern.chars().count()
                    );
                    let _ = self.regex_cache.set(None);
                } else {
                    match Regex::new(pattern) {
                        Ok(re) => {
                            let _ = self.regex_cache.set(Some(re));
                        }
                        Err(e) => {
                            warn!("[reply] 正则编译失败，该谓词恒为 false: {}", e);
                            let _ = self.regex_cache.set(None);
                        }
                    }
                }
            }
            MatchNode::All { children } | MatchNode::Any { children } => {
                for child in children {
                    self.prime_regex(child);
                }
            }
            MatchNode::Not { child } => self.prime_regex(child),
            _ => {}
        }
    }

    fn prime_addrs(&self, node: &crate::reply::model::MatchNode) {
        use crate::reply::model::MatchNode;
        match node {
            MatchNode::From { addrs } => {
                // 非法项在预解析期丢弃 —— 与旧行为一致（非法地址恒不匹配），
                // 但不再每帧重试解析。
                let _ = self
                    .addr_cache
                    .set(addrs.iter().filter_map(|a| parse_addr_spec(a)).collect());
            }
            MatchNode::All { children } | MatchNode::Any { children } => {
                for child in children {
                    self.prime_addrs(child);
                }
            }
            MatchNode::Not { child } => self.prime_addrs(child),
            _ => {}
        }
    }

    /// 解析字节模式（带缓存）。**每个运行期实例只缓存一份模式**——
    /// 一条规则里通常只有一个字节模式（`PrefixRange.prefix` 或 `Contains.bytes`），
    /// 若同一条规则含多个不同模式，则退化为每次解析（正确性不受影响）。
    ///
    /// 返回**借用**（P-4）：缓存本就常驻，命中时不该再 `Vec::clone`。
    pub fn pattern(&self, pattern: &BytePattern) -> Result<&[u8], &str> {
        if self.pattern_cache.get().is_none() {
            let _ = self.pattern_cache.set(pattern.resolve());
        }
        match self
            .pattern_cache
            .get()
            .expect("pattern cache 已在上一行人填充")
        {
            Ok(bytes) => Ok(bytes.as_slice()),
            Err(e) => Err(e.as_str()),
        }
    }

    /// 预解析好的来源地址规格（P-5）；空表示该规则没有 `From` 谓词或全部非法
    pub fn addr_specs(&self) -> &[AddrSpec] {
        self.addr_cache.get().map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 取编译好的正则（未编译或编译失败返回 None → 谓词恒 false）
    pub fn regex(&self) -> Option<&Regex> {
        self.regex_cache.get().and_then(|opt| opt.as_ref())
    }

    /// 当前命中次数
    pub fn hit_count(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }
}

/// 规则集运行期状态（UI 写 / 网络线程读）
#[derive(Debug)]
pub struct ReplyRulesStore {
    /// 快速路径开关：没有任何启用规则时为 false，网络线程零开销
    enabled: AtomicBool,
    /// 按连接分组的运行期规则表（每连接已按 priority 升序排序，含禁用规则）
    tables: RwLock<Arc<HashMap<String, Arc<Vec<Arc<RuleRuntime>>>>>>,
    /// 表版本号：`replace` / `set_connection_gates` 递增，令 `scope_cache` 作废
    version: AtomicU64,
    /// 每连接「已启用规则」过滤结果的缓存（以 `version` 为边界标记）
    scope_cache: Mutex<ScopeCache>,
    /// 各连接的「自动回复」总闸（缺省 = false）。由 AppConfig 下发。
    gates: RwLock<Arc<HashMap<String, bool>>>,
    /// 应答独立递增序号（供 `${seq}` 使用，网络线程自取，不回 UI 线程取数）
    seq: AtomicU64,
}

/// `rules_for` 的按连接缓存
#[derive(Debug, Default)]
struct ScopeCache {
    /// 缓存对应的表版本；与 store 的 `version` 不一致即整表作废
    version: u64,
    by_connection: HashMap<String, Arc<Vec<Arc<RuleRuntime>>>>,
}

impl Default for ReplyRulesStore {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            tables: RwLock::new(Arc::new(HashMap::new())),
            version: AtomicU64::new(0),
            scope_cache: Mutex::new(ScopeCache::default()),
            gates: RwLock::new(Arc::new(HashMap::new())),
            seq: AtomicU64::new(0),
        }
    }
}

impl ReplyRulesStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 是否启用（无锁快速路径，未启用时网络层零开销）
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// 应答序号（`${seq}`）
    pub fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// 指定连接下的启用规则数（连接页状态文案用）
    pub fn enabled_rule_count_for(&self, connection_id: &str) -> usize {
        self.rules_for(connection_id)
            .iter()
            .filter(|r| r.rule.enabled)
            .count()
    }

    /// UI 下发：整表替换（低频写）
    ///
    /// 命中计数按 `id` 继承：编辑规则不会让用户辛苦积累的调试计数归零，
    /// 但被删除的规则其计数自然消失。
    ///
    /// 规则按连接分组构建，每组独立按 `priority` 升序排序（顺序即该连接的求值优先级）。
    pub fn replace(&self, config: &ReplyRulesConfig) {
        // 收集既有计数（跨全部连接），供同 id 规则继承
        let previous: HashMap<String, Arc<AtomicU64>> = self
            .all_runtimes()
            .values()
            .flat_map(|rules| rules.iter())
            .map(|r| (r.rule.id.clone(), r.hits.clone()))
            .collect();

        let mut tables: HashMap<String, Arc<Vec<Arc<RuleRuntime>>>> = HashMap::new();
        for (connection_id, rules) in &config.connections {
            let mut runtimes: Vec<Arc<RuleRuntime>> = rules
                .iter()
                .map(|rule| {
                    let hits = previous
                        .get(&rule.id)
                        .cloned()
                        .unwrap_or_else(|| Arc::new(AtomicU64::new(0)));
                    RuleRuntime::new(rule.clone(), hits)
                })
                .collect();
            // 求值顺序即语义：priority 升序，同值按 id 字典序稳定排序
            runtimes.sort_by(|a, b| {
                a.rule
                    .priority
                    .cmp(&b.rule.priority)
                    .then_with(|| a.rule.id.cmp(&b.rule.id))
            });
            tables.insert(connection_id.clone(), Arc::new(runtimes));
        }

        // 没有启用规则时直接关闭快速路径：网络线程连读锁都不碰。
        let active = tables
            .values()
            .any(|rules| rules.iter().any(|r| r.rule.enabled));

        {
            let mut guard = self.tables.write().unwrap_or_else(|e| e.into_inner());
            *guard = Arc::new(tables);
        }

        self.enabled.store(active, Ordering::Relaxed);
        // Release：与 `rules_for` 的 Acquire 读配对，保证"看到新版本号 ⇒ 看到新规则表"
        self.version.fetch_add(1, Ordering::Release);
    }

    /// 全部连接下的运行期规则快照（一次读锁 + 一次 Arc clone）
    fn all_runtimes(&self) -> Arc<HashMap<String, Arc<Vec<Arc<RuleRuntime>>>>> {
        self.tables.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// UI 下发：整表替换各连接的「自动回复」总闸。**缺省 false**
    ///
    /// 递增 `version`（Release）令 `scope_cache` 整表作废，使下帧起按新开关过滤。
    pub fn set_connection_gates(&self, gates: HashMap<String, bool>) {
        {
            let mut guard = self.gates.write().unwrap_or_else(|e| e.into_inner());
            *guard = Arc::new(gates);
        }
        self.version.fetch_add(1, Ordering::Release);
    }

    /// 指定连接是否启用「自动回复」总闸（缺省 false）
    fn gate_of(&self, connection_id: &str) -> bool {
        self.gates
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(connection_id)
            .copied()
            .unwrap_or(false)
    }

    /// 全部连接下的启用规则并集（供纯函数求值测试等场景使用）
    pub fn enabled_rules(&self) -> Arc<Vec<Arc<RuleRuntime>>> {
        let tables = self.all_runtimes();
        let mut all: Vec<Arc<RuleRuntime>> = tables
            .values()
            .flat_map(|rules| rules.iter())
            .filter(|r| r.rule.enabled)
            .cloned()
            .collect();
        // 跨连接排序：priority 升序 + id 稳定
        all.sort_by(|a, b| {
            a.rule
                .priority
                .cmp(&b.rule.priority)
                .then_with(|| a.rule.id.cmp(&b.rule.id))
        });
        Arc::new(all)
    }

    /// 网络线程读：某连接下**已启用的规则**
    ///
    /// 该连接的「自动回复」总闸必须为 true，否则返回空（该连接不参与规则求值）。
    /// 返回 `Arc<Vec<...>>` 让调用方在锁外遍历，避免长时间持读锁。
    ///
    /// **每连接缓存**：过滤结果与表版本一一对应，版本未变时直接复用，
    /// 洪泛场景下省去每帧的 N 次 `Arc` 克隆与 `Vec` 分配。
    /// 只有"构建前后版本一致"才写入缓存 —— 否则可能把旧快照挂到新版本号上。
    pub fn rules_for(&self, connection_id: &str) -> Arc<Vec<Arc<RuleRuntime>>> {
        let version = self.version.load(Ordering::Acquire);
        {
            let cache = self.scope_cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.version == version {
                if let Some(cached) = cache.by_connection.get(connection_id) {
                    return cached.clone();
                }
            }
        }

        let filtered: Arc<Vec<Arc<RuleRuntime>>> = if self.gate_of(connection_id) {
            let tables = self.all_runtimes();
            match tables.get(connection_id) {
                Some(rules) => Arc::new(
                    rules
                        .iter()
                        .filter(|r| r.rule.enabled)
                        .cloned()
                        .collect(),
                ),
                None => Arc::new(Vec::new()),
            }
        } else {
            Arc::new(Vec::new())
        };

        if self.version.load(Ordering::Acquire) == version {
            let mut cache = self.scope_cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.version != version {
                cache.version = version;
                cache.by_connection.clear();
            }
            cache
                .by_connection
                .insert(connection_id.to_string(), filtered.clone());
        }
        filtered
    }

    /// 命中计数的快照（UI 每拍读取，节流后调用）
    pub fn hits_snapshot(&self) -> HashMap<String, u64> {
        self.all_runtimes()
            .values()
            .flat_map(|rules| rules.iter())
            .map(|r| (r.rule.id.clone(), r.hit_count()))
            .collect()
    }

    /// 单条规则计数归零（决策 U-10：调一条规则时不想丢其他规则的计数）
    pub fn reset_hits_of(&self, rule_id: &str) {
        for runtime in self.all_runtimes().values().flat_map(|rules| rules.iter()) {
            if runtime.rule.id == rule_id {
                runtime.hits.store(0, Ordering::Relaxed);
            }
        }
    }

    /// 全部归零
    pub fn reset_hits(&self) {
        for runtime in self.all_runtimes().values().flat_map(|rules| rules.iter()) {
            runtime.hits.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reply::frame::RxFrame;
    use crate::reply::matcher::evaluate;
    use crate::reply::model::{BytePattern, MatchNode, ReplyPayload, ReplyRule, RuleCodec};
    use std::sync::atomic::AtomicBool;

    fn rule(id: &str, priority: u32, enabled: bool) -> ReplyRule {
        ReplyRule {
            id: id.to_string(),
            enabled,
            matcher: MatchNode::Length { min: 1, max: 8 },
            ..ReplyRule::new(id, priority)
        }
    }

    /// 构造某连接下的规则集（按连接分组存储）
    fn config(connection_id: &str, rules: Vec<ReplyRule>) -> ReplyRulesConfig {
        let mut cfg = ReplyRulesConfig::default();
        if !rules.is_empty() {
            cfg.connections.insert(connection_id.to_string(), rules);
        }
        cfg
    }

    /// 构造带连接开关的 store：给定连接 id 全部置 true。
    /// 缺省 false 会令该连接规则不生效，故凡用 `rules_for` 断言规则的用例都需先开启。
    fn gated_store(ids: &[&str]) -> Arc<ReplyRulesStore> {
        let store = ReplyRulesStore::new();
        let gates = ids.iter().map(|id| (id.to_string(), true)).collect();
        store.set_connection_gates(gates);
        store
    }

    /// 未启用时 `is_enabled` 为 false（网络层零开销快速路径的前提）
    #[test]
    fn test_disabled_by_default() {
        let store = ReplyRulesStore::new();
        assert!(!store.is_enabled());
        assert!(store.hits_snapshot().is_empty());
        store.replace(&config("tab", vec![rule("a", 1, true)]));
        assert!(store.is_enabled());
    }

    /// 规则全被禁用时快速路径关闭（等价于"功能整体关闭"）
    #[test]
    fn test_all_disabled_keeps_fast_path_off() {
        let store = ReplyRulesStore::new();
        store.replace(&config("tab", vec![rule("a", 1, false)]));
        assert!(!store.is_enabled(), "无启用规则时不得进入规则求值路径");
    }

    /// 排序：priority 升序 + 同值按 id 稳定
    #[test]
    fn test_sorting() {
        let store = gated_store(&["tab"]);
        store.replace(&config(
            "tab",
            vec![rule("c", 300, true), rule("a", 100, true), rule("b", 200, true)],
        ));
        let ids: Vec<String> = store
            .rules_for("tab")
            .iter()
            .map(|r| r.rule.id.clone())
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    /// 连接隔离：规则只属于其所属连接，其他连接看不到
    #[test]
    fn test_connection_isolation() {
        let store = gated_store(&["tab-1", "tab-2"]);
        let mut cfg = config("tab-1", vec![rule("a", 1, true)]);
        cfg.connections
            .insert("tab-2".to_string(), vec![rule("b", 2, true)]);
        store.replace(&cfg);
        assert_eq!(store.rules_for("tab-1").len(), 1);
        assert_eq!(store.rules_for("tab-2").len(), 1);
        assert_eq!(store.rules_for("tab-1")[0].rule.id, "a");
        assert_eq!(store.rules_for("tab-2")[0].rule.id, "b");
        assert!(store.rules_for("tab-x").is_empty());
    }

    /// 连接总闸：关闭时该连接全部规则不生效；开启后立即生效（缓存失效）
    #[test]
    fn test_connection_gate_controls_all_rules() {
        let store = ReplyRulesStore::new();
        store.replace(&config(
            "tab-1",
            vec![rule("a", 1, true), rule("b", 2, true)],
        ));

        // 缺省 false：本连接规则不生效
        assert!(store.rules_for("tab-1").is_empty());

        // 显式开启后立即生效（gate 变更令 scope_cache 作废）
        store.set_connection_gates(HashMap::from([("tab-1".to_string(), true)]));
        let ids: Vec<String> = store
            .rules_for("tab-1")
            .iter()
            .map(|r| r.rule.id.clone())
            .collect();
        assert_eq!(ids, vec!["a", "b"]);
        // 未开启的连接依旧为空
        assert!(store.rules_for("tab-2").is_empty());

        // 关闭后重新回到不生效
        store.set_connection_gates(HashMap::from([("tab-1".to_string(), false)]));
        assert!(store.rules_for("tab-1").is_empty());
    }

    /// 启用计数：按连接
    #[test]
    fn test_enabled_rule_count() {
        let store = gated_store(&["tab-1", "tab-2"]);
        let mut cfg = config("tab-1", vec![rule("a", 1, true), rule("off", 2, false)]);
        cfg.connections
            .insert("tab-2".to_string(), vec![rule("b", 1, true)]);
        store.replace(&cfg);
        assert_eq!(store.enabled_rule_count_for("tab-1"), 1);
        assert_eq!(store.enabled_rule_count_for("tab-2"), 1);
        assert_eq!(store.enabled_rule_count_for("tab-x"), 0);
    }

    /// 禁用规则不参与求值，但仍在列表里可见
    #[test]
    fn test_disabled_rule_skipped() {
        let store = ReplyRulesStore::new();
        store.replace(&config(
            "tab",
            vec![rule("off", 1, false), rule("on", 2, true)],
        ));
        assert_eq!(store.enabled_rules().len(), 1);
        // 快照覆盖全部规则（含禁用项），故仍为 2
        assert_eq!(store.hits_snapshot().len(), 2);
    }

    /// 编辑规则不得让命中计数归零（同一 id 继承计数）
    #[test]
    fn test_hits_survive_replace_by_id() {
        let store = gated_store(&["tab"]);
        store.replace(&config("tab", vec![rule("a", 1, true)]));
        store.rules_for("tab")[0]
            .hits
            .fetch_add(5, Ordering::Relaxed);
        assert_eq!(store.hits_snapshot()["a"], 5);

        // 改规则内容（同 id）后计数保留
        let mut edited = rule("a", 1, true);
        edited.name = "改名了".to_string();
        store.replace(&config("tab", vec![edited]));
        assert_eq!(store.hits_snapshot()["a"], 5, "同 id 规则改名后计数应保留");

        // 删除规则后计数消失
        store.replace(&config("tab", vec![]));
        assert!(store.hits_snapshot().is_empty());
    }

    /// 单条清零与全部清零（决策 U-10）
    #[test]
    fn test_reset_hits() {
        let store = gated_store(&["tab"]);
        store.replace(&config(
            "tab",
            vec![rule("a", 1, true), rule("b", 2, true)],
        ));
        let rules = store.rules_for("tab");
        rules
            .iter()
            .find(|r| r.rule.id == "a")
            .unwrap()
            .hits
            .fetch_add(3, Ordering::Relaxed);
        rules
            .iter()
            .find(|r| r.rule.id == "b")
            .unwrap()
            .hits
            .fetch_add(4, Ordering::Relaxed);

        store.reset_hits_of("a");
        let snapshot = store.hits_snapshot();
        assert_eq!(snapshot["a"], 0);
        assert_eq!(snapshot["b"], 4);
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 4);

        store.reset_hits();
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 0);
    }

    /// 多线程并发累加计数不得丢失（压测洪泛下的正确性）
    #[test]
    fn test_concurrent_hit_counting() {
        let store = gated_store(&["tab"]);
        store.replace(&config("tab", vec![rule("a", 1, true)]));
        let handle = store.rules_for("tab")[0].hits.clone();

        let threads: Vec<_> = (0..8)
            .map(|_| {
                let h = handle.clone();
                std::thread::spawn(move || {
                    for _ in 0..10_000 {
                        h.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 80_000);
    }

    /// `replace()` 与读并发不产生死锁、不 panic（UI 改规则与网络线程求值同时发生）
    #[test]
    fn test_concurrent_replace_and_read() {
        let store = gated_store(&["tab"]);
        store.replace(&config("tab", vec![rule("a", 1, true)]));

        let stop = Arc::new(AtomicBool::new(false));
        let reader_store = store.clone();
        let reader_stop = stop.clone();
        let reader = std::thread::spawn(move || {
            let mut count = 0u64;
            while !reader_stop.load(Ordering::Relaxed) {
                let rules = reader_store.rules_for("tab");
                count += rules.len() as u64;
            }
            count
        });

        for i in 0..200u32 {
            store.replace(&config(
                "tab",
                vec![rule("a", 1, true), rule(&format!("extra-{}", i), 2 + i, i % 2 == 0)],
            ));
        }
        stop.store(true, Ordering::Relaxed);
        let count = reader.join().unwrap();
        assert!(count > 0, "并发读应确实读到过规则");
        // 最终状态一致
        assert_eq!(store.hits_snapshot().len(), 2);
    }

    /// 序号独立递增（`${seq}` 由网络线程自取）
    #[test]
    fn test_seq_increments() {
        let store = ReplyRulesStore::new();
        assert_eq!(store.next_seq(), 0);
        assert_eq!(store.next_seq(), 1);
        assert_eq!(store.next_seq(), 2);
    }

    /// 非法正则的规则仍在表里（可编辑修复），但谓词恒 false 且只编译一次
    #[test]
    fn test_invalid_regex_cached_as_none() {
        let store = ReplyRulesStore::new();
        store.replace(&config(
            "tab",
            vec![ReplyRule {
                matcher: MatchNode::Regex {
                    pattern: "([".to_string(),
                },
                ..ReplyRule::new("bad regex", 1)
            }],
        ));
        let rules = store.enabled_rules();
        let runtime = &rules[0];
        assert!(runtime.regex().is_none());
        // 再次读取仍是 None（缓存生效，不重复编译）
        assert!(runtime.regex().is_none());

        let frame = RxFrame::for_test(b"anything".to_vec());
        assert!(!evaluate(&rules, &frame, false).is_hit());
    }

    /// 超长正则视为不可用（安全边界）
    #[test]
    fn test_overlong_regex_disabled() {
        let store = ReplyRulesStore::new();
        let long = "a".repeat(crate::reply::model::MAX_REGEX_LEN + 1);
        store.replace(&config(
            "tab",
            vec![ReplyRule {
                matcher: MatchNode::Regex { pattern: long },
                ..ReplyRule::new("long regex", 1)
            }],
        ));
        assert!(store.enabled_rules()[0].regex().is_none());
    }

    /// 字节模式缓存：合法模式解析一次即可复用；非法模式缓存 Err（不每帧重试）
    #[test]
    fn test_pattern_cache() {
        let store = ReplyRulesStore::new();
        store.replace(&config(
            "tab",
            vec![ReplyRule {
                matcher: MatchNode::Contains {
                    bytes: BytePattern::Hex("01 03".to_string()),
                },
                ..ReplyRule::new("ok pattern", 1)
            }],
        ));
        let rules = store.enabled_rules();
        let runtime = &rules[0];
        assert_eq!(
            runtime
                .pattern(&BytePattern::Hex("01 03".to_string()))
                .unwrap(),
            vec![0x01, 0x03]
        );

        let store2 = ReplyRulesStore::new();
        store2.replace(&config(
            "tab",
            vec![ReplyRule {
                matcher: MatchNode::Contains {
                    bytes: BytePattern::Hex("ZZ".to_string()),
                },
                ..ReplyRule::new("bad pattern", 1)
            }],
        ));
        let rules2 = store2.enabled_rules();
        assert!(
            rules2[0]
                .pattern(&BytePattern::Hex("ZZ".to_string()))
                .is_err(),
            "非法 hex 模式必须缓存为 Err（既不静默通过也不每帧重试）"
        );
    }

    /// 命中计数通过 evaluate 累加后可在快照里读到（端到端的最小闭环）
    #[test]
    fn test_hit_counting_through_evaluate() {
        let store = gated_store(&["tab"]);
        store.replace(&config(
            "tab",
            vec![ReplyRule {
                matcher: MatchNode::Length { min: 1, max: 8 },
                payload: ReplyPayload {
                    text: "6F 6B".to_string(),
                    hex_mode: true,
                    codec: RuleCodec::Raw,
                },
                ..ReplyRule::new("命中", 1)
            }],
        ));
        let rules = store.rules_for("tab");
        let frame = RxFrame::for_test(vec![1, 2, 3]);
        for _ in 0..3 {
            let outcome = evaluate(&rules, &frame, false);
            assert!(outcome.is_hit());
            if let Some(id) = &outcome.rule_id {
                rules
                    .iter()
                    .find(|r| &r.rule.id == id)
                    .unwrap()
                    .hits
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 3);
    }
}
