// 回复规则包（导入 / 导出 / 跨连接复制的唯一数据格式）
//
// 设计要点（见 plans/plan-reply-rules-transfer.md）：
//   - 包内**不保存** `id` 与 `priority`：id 是连接内的运行期主键，跨连接必须重新生成；
//     优先级由数组顺序表达（导入后按 index*10 重排），避免两套顺序来源。
//   - 顶层 `kind` + `version` 用于识别文件类型与未来迁移，避免把任意 JSON 当规则包导入。
//   - 复用 `MatchNode` / `ReplyPayload` 的既有 serde 表示，不新增镜像类型。
//
// **本模块不 `use gpui`**：解析、校验、UUID 生成全部可无头单测。

use crate::reply::model::{MatchNode, ReplyPayload, ReplyRule, Severity, validate_rule};
use serde::{Deserialize, Serialize};

/// 规则包类型标识（写入 JSON，导入时严格校验）
pub const BUNDLE_KIND: &str = "netassistant.reply-rules";
/// 当前规则包 schema 版本
pub const BUNDLE_VERSION: u32 = 1;
/// 单个规则包最多规则数（防御异常文件）
pub const MAX_BUNDLE_RULES: usize = 500;
/// 规则包文件大小上限（UI 读取前先判元数据）
pub const MAX_BUNDLE_BYTES: u64 = 1024 * 1024;

/// 规则包顶层结构
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleBundle {
    /// 固定为 [`BUNDLE_KIND`]
    pub kind: String,
    /// 规则包 schema 版本
    pub version: u32,
    /// 导出时间（仅展示，不参与导入语义）
    #[serde(default)]
    pub exported_at: Option<String>,
    /// 来源连接的可读标识（仅展示）
    #[serde(default)]
    pub source: Option<String>,
    /// 规则列表；数组顺序即导入后的求值顺序
    pub rules: Vec<BundleRule>,
}

/// 包内单条规则（无 id / priority）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleRule {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub matcher: MatchNode,
    pub payload: ReplyPayload,
}

fn default_enabled() -> bool {
    true
}

impl BundleRule {
    fn from_rule(rule: &ReplyRule) -> Self {
        Self {
            name: rule.name.clone(),
            description: rule.description.clone(),
            enabled: rule.enabled,
            tags: rule.tags.clone(),
            matcher: rule.matcher.clone(),
            payload: rule.payload.clone(),
        }
    }

    /// 还原成可持久化规则：新 UUID + 给定 priority
    fn into_rule(self, priority: u32) -> ReplyRule {
        ReplyRule {
            id: uuid::Uuid::new_v4().to_string(),
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            priority,
            tags: self.tags,
            matcher: self.matcher,
            payload: self.payload,
        }
    }
}

/// 导入结果
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedBundle {
    /// 已生成新 UUID、按 `index * 10` 排好 priority 的规则
    pub rules: Vec<ReplyRule>,
    /// `validate_rule` 报 Error 的条数（这些规则仍会导入，UI 提示用户后续修复）
    pub invalid: usize,
}

impl RuleBundle {
    /// 从当前连接的选中规则构建（数组顺序即导出顺序）
    pub fn from_rules(rules: &[ReplyRule], source: Option<String>) -> Self {
        Self {
            kind: BUNDLE_KIND.to_string(),
            version: BUNDLE_VERSION,
            exported_at: Some(chrono::Local::now().to_rfc3339()),
            source,
            rules: rules.iter().map(BundleRule::from_rule).collect(),
        }
    }

    /// 序列化为适合写文件/阅读的 pretty JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 解析并做类型/版本/数量校验。
    ///
    /// 只负责"包是否可信"，不接触磁盘，也不修改任何现有规则。
    pub fn parse(json: &str) -> Result<Self, BundleError> {
        let bundle: RuleBundle =
            serde_json::from_str(json).map_err(|e| BundleError::Json(e.to_string()))?;
        if bundle.kind != BUNDLE_KIND {
            return Err(BundleError::NotBundle);
        }
        if bundle.version != BUNDLE_VERSION {
            return Err(BundleError::UnsupportedVersion(bundle.version));
        }
        if bundle.rules.is_empty() {
            return Err(BundleError::EmptyRules);
        }
        if bundle.rules.len() > MAX_BUNDLE_RULES {
            return Err(BundleError::TooManyRules(bundle.rules.len()));
        }
        Ok(bundle)
    }

    /// 转为可追加到目标连接的规则（重新生成 UUID、重排 priority）
    pub fn into_rules(self) -> ImportedBundle {
        let mut invalid = 0;
        let rules = self
            .rules
            .into_iter()
            .enumerate()
            .map(|(index, bundle_rule)| {
                let rule = bundle_rule.into_rule((index as u32) * 10);
                if validate_rule(&rule)
                    .iter()
                    .any(|issue| issue.severity == Severity::Error)
                {
                    invalid += 1;
                }
                rule
            })
            .collect();
        ImportedBundle { rules, invalid }
    }
}

/// 规则包解析失败原因（UI 层映射为 i18n 文案）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleError {
    /// `kind` 不匹配，不是 NetAssistant 规则包
    NotBundle,
    /// 规则包版本高于当前应用支持
    UnsupportedVersion(u32),
    /// 包内没有任何规则
    EmptyRules,
    /// 规则数超过 [`MAX_BUNDLE_RULES`]
    TooManyRules(usize),
    /// JSON 语法/结构错误（含 serde 递归深度超限）
    Json(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reply::model::{BytePattern, ReplyPayload, RuleCodec};

    fn sample_rule(name: &str, enabled: bool) -> ReplyRule {
        ReplyRule {
            name: name.to_string(),
            description: "备注".to_string(),
            enabled,
            tags: vec!["modbus".to_string()],
            matcher: MatchNode::Contains {
                bytes: BytePattern::Hex("01 03".to_string()),
            },
            payload: ReplyPayload {
                text: "01 03 02 00 01".to_string(),
                hex_mode: true,
                codec: RuleCodec::Raw,
            },
            ..ReplyRule::new(name, 0)
        }
    }

    /// 导出再导入：语义字段完整保留，id/priority 被重组
    #[test]
    fn test_bundle_roundtrip() {
        let original = vec![sample_rule("A", true), sample_rule("B", false)];
        let bundle = RuleBundle::from_rules(&original, Some("TCP_127.0.0.1_8080".to_string()));
        assert_eq!(bundle.kind, BUNDLE_KIND);
        assert_eq!(bundle.version, BUNDLE_VERSION);
        assert_eq!(bundle.rules.len(), 2);

        let json = bundle.to_json().unwrap();
        let parsed = RuleBundle::parse(&json).unwrap();
        let imported = parsed.into_rules();

        assert_eq!(imported.rules.len(), 2);
        assert_eq!(imported.invalid, 0);
        assert_eq!(imported.rules[0].name, "A");
        assert_eq!(imported.rules[0].description, "备注");
        assert!(imported.rules[0].enabled);
        assert!(!imported.rules[1].enabled, "启用状态必须原样保留");
        assert_eq!(imported.rules[0].tags, vec!["modbus".to_string()]);
        assert_eq!(imported.rules[0].matcher, original[0].matcher);
        assert_eq!(imported.rules[0].payload, original[0].payload);
        assert_eq!(imported.rules[0].priority, 0);
        assert_eq!(imported.rules[1].priority, 10, "priority 按数组顺序重排");
        assert_ne!(
            imported.rules[0].id, original[0].id,
            "导入必须生成新 UUID，避免跨连接 id 冲突"
        );
        assert_ne!(imported.rules[0].id, imported.rules[1].id);
    }

    /// kind 不匹配必须拒绝（防止把其它 JSON 当规则包）
    #[test]
    fn test_bundle_rejects_wrong_kind() {
        let mut bundle = RuleBundle::from_rules(&[sample_rule("A", true)], None);
        bundle.kind = "something-else".to_string();
        let json = bundle.to_json().unwrap();
        assert_eq!(RuleBundle::parse(&json), Err(BundleError::NotBundle));
    }

    /// 更高版本拒绝导入并带上版本号（便于提示）
    #[test]
    fn test_bundle_rejects_unsupported_version() {
        let mut bundle = RuleBundle::from_rules(&[sample_rule("A", true)], None);
        bundle.version = BUNDLE_VERSION + 1;
        let json = bundle.to_json().unwrap();
        assert_eq!(
            RuleBundle::parse(&json),
            Err(BundleError::UnsupportedVersion(BUNDLE_VERSION + 1))
        );
    }

    /// 空规则包与超量规则包都拒绝
    #[test]
    fn test_bundle_rejects_empty_and_too_many() {
        let empty = RuleBundle {
            kind: BUNDLE_KIND.to_string(),
            version: BUNDLE_VERSION,
            exported_at: None,
            source: None,
            rules: Vec::new(),
        };
        assert_eq!(
            RuleBundle::parse(&empty.to_json().unwrap()),
            Err(BundleError::EmptyRules)
        );

        let mut many = RuleBundle::from_rules(&[sample_rule("A", true)], None);
        many.rules = vec![many.rules[0].clone(); MAX_BUNDLE_RULES + 1];
        assert_eq!(
            RuleBundle::parse(&many.to_json().unwrap()),
            Err(BundleError::TooManyRules(MAX_BUNDLE_RULES + 1))
        );
    }

    /// 无效规则照常导入，但被统计（UI 提示可在列表中修复）
    #[test]
    fn test_bundle_counts_invalid_rules() {
        let invalid = ReplyRule {
            matcher: MatchNode::Any {
                children: Vec::new(),
            },
            ..sample_rule("无效", true)
        };
        let bundle = RuleBundle::from_rules(&[sample_rule("A", true), invalid], None);
        let imported = bundle.into_rules();
        assert_eq!(imported.rules.len(), 2);
        assert_eq!(imported.invalid, 1);
    }

    /// 非法 JSON 给出 Json 错误而不是 panic
    #[test]
    fn test_bundle_rejects_malformed_json() {
        match RuleBundle::parse("{ not json") {
            Err(BundleError::Json(_)) => {}
            other => panic!("应返回 Json 错误，实际: {:?}", other),
        }
    }
}
