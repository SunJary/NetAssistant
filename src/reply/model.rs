// 回复规则数据模型（L1 纯逻辑，不依赖 gpui）
//
// 对齐 docs/plan-reply-rules.md §4。一条规则 = 条件（`MatchNode` AST）+ 应答载荷
// （`ReplyPayload`）。规则集为有序列表，按 `priority` 升序求值，**命中即停**。
//
// 本文件同时提供三件"保存期校验"能力（UI 依赖它们才能做到"出错当场指出哪一项"）：
//   validate_rule()           规则级校验（非法 hex / 非法正则 / 区间倒置 / 空组合…）
//   format_matcher_summary()  条件摘要（规则列表里一眼看懂每条规则在做什么）
//   reply_preview()           载荷预览（列表里显示"回复什么"）

use crate::core::checksum::ChecksumAlgorithm;
use crate::utils::hex::hex_to_bytes;
use crate::utils::message_vars::CompiledTemplate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 字节模式：hex 文本（hex 模式下是字节；文本模式下是 UTF-8 字节）
///
/// 之所以不在模型里存 `Vec<u8>`：规则要能被用户读懂和编辑。
/// 运行期由 `resolve()` 转换，转换结果在 `RuleRuntime` 里缓存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BytePattern {
    /// hex 字节串："01 03 00 00 00 02"
    Hex(String),
    /// 文本（按 UTF-8 编码）
    Text(String),
}

impl BytePattern {
    /// 解析为字节；非法 hex 返回 Err（UI 侧据此禁用保存）
    ///
    /// **严格**：既有的 `hex_to_bytes` 面向"用户手输的发送框"，对非法字符与奇数位
    /// 采取静默丢弃。规则匹配不能这样 —— 掩码/前缀写错一位却静默变成另一个字节，
    /// 会让规则"永不命中"且毫无线索。因此这里先做字符级校验，再交给同一个转换函数。
    pub fn resolve(&self) -> Result<Vec<u8>, String> {
        match self {
            BytePattern::Hex(s) => {
                let normalized = normalize_hex_text(s);
                if normalized.is_empty() {
                    return Ok(Vec::new());
                }
                if normalized.len() % 2 != 0 {
                    return Err(format!("十六进制位数必须为偶数: \"{}\"", s));
                }
                if !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(format!("含非十六进制字符: \"{}\"", s));
                }
                Ok(hex_to_bytes(&normalized))
            }
            BytePattern::Text(s) => Ok(s.as_bytes().to_vec()),
        }
    }

    /// 是否为空模式（空模式在多数谓词里恒不成立，需在 UI 上提示）
    pub fn is_empty(&self) -> bool {
        match self {
            BytePattern::Hex(s) => normalize_hex_text(s).is_empty(),
            BytePattern::Text(s) => s.is_empty(),
        }
    }

    /// 可读展示（规则列表的摘要用）
    pub fn display(&self) -> String {
        match self {
            BytePattern::Hex(s) => {
                let compact = normalize_hex_text(s);
                if compact.is_empty() {
                    "(空)".to_string()
                } else {
                    compact.to_uppercase()
                }
            }
            BytePattern::Text(s) => {
                if s.is_empty() {
                    "(空)".to_string()
                } else {
                    format!("\"{}\"", s)
                }
            }
        }
    }
}

/// 归一化 hex 文本：去掉空白与常见分隔符（`0x` / 逗号 / 冒号 / 连字符）。
///
/// 归一化后的字符串既是"字符级校验"的对象，也是 `hex_to_bytes` 的输入 ——
/// 两者用同一个字符串，保证"校验通过"与"解析结果"永远一致。
fn normalize_hex_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() || c == ',' || c == ':' || c == '-' {
            continue;
        }
        // 允许 `0x` 前缀（大小写均可）
        if c == '0' {
            if let Some(&next) = chars.peek() {
                if next == 'x' || next == 'X' {
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// 统计 hex 文本里的有效十六进制字符数（忽略空白、`0x` 前缀与分隔符）
fn count_hex_digits(s: &str) -> usize {
    normalize_hex_text(s).chars().count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Width {
    U8,
    U16,
    U32,
    U64,
}

impl Width {
    pub fn bytes(self) -> usize {
        match self {
            Width::U8 => 1,
            Width::U16 => 2,
            Width::U32 => 4,
            Width::U64 => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Endian {
    Big,
    Little,
}

impl Endian {
    pub fn is_little(self) -> bool {
        matches!(self, Endian::Little)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

impl Cmp {
    pub fn test(self, left: i64, right: i64) -> bool {
        match self {
            Cmp::Eq => left == right,
            Cmp::Ne => left != right,
            Cmp::Gt => left > right,
            Cmp::Ge => left >= right,
            Cmp::Lt => left < right,
            Cmp::Le => left <= right,
        }
    }

    /// 运算符符号（摘要展示用）
    pub fn symbol(self) -> &'static str {
        match self {
            Cmp::Eq => "==",
            Cmp::Ne => "!=",
            Cmp::Gt => ">",
            Cmp::Ge => ">=",
            Cmp::Lt => "<",
            Cmp::Le => "<=",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ByteOp {
    Eq {
        value: u8,
    },
    Ne {
        value: u8,
    },
    /// (b & mask) == value
    Masked {
        mask: u8,
        value: u8,
    },
    /// b 在集合内
    In {
        values: Vec<u8>,
    },
}

impl ByteOp {
    /// 摘要展示
    pub fn summary(&self) -> String {
        match self {
            ByteOp::Eq { value } => format!("== {:02X}", value),
            ByteOp::Ne { value } => format!("!= {:02X}", value),
            ByteOp::Masked { mask, value } => format!("& {:02X} == {:02X}", mask, value),
            ByteOp::In { values } => {
                let list: Vec<String> = values.iter().map(|v| format!("{:02X}", v)).collect();
                format!("∈ [{}]", list.join(" "))
            }
        }
    }
}

/// 校验值在帧中的位置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChecksumAt {
    /// 固定偏移 + 宽度
    Fixed { offset: usize, width: Width },
    /// 帧尾倒数第 n 个字节起、宽度 width（Modbus RTU 是末尾 2 字节）
    Trailing { n: usize, width: Width },
}

impl ChecksumAt {
    /// 计算校验值在给定帧长下的实际起始偏移；帧太短则 None
    pub fn resolve_offset(&self, frame_len: usize) -> Option<usize> {
        match self {
            ChecksumAt::Fixed { offset, width } => {
                if offset.checked_add(width.bytes())? <= frame_len {
                    Some(*offset)
                } else {
                    None
                }
            }
            ChecksumAt::Trailing { n, width } => {
                let end = frame_len.checked_sub(*n)?;
                if end.checked_add(width.bytes())? > frame_len {
                    return None;
                }
                Some(end)
            }
        }
    }

    pub fn width(&self) -> Width {
        match self {
            ChecksumAt::Fixed { width, .. } | ChecksumAt::Trailing { width, .. } => *width,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            ChecksumAt::Fixed { offset, width } => {
                format!("偏移 {} 宽 {} 字节", offset, width.bytes())
            }
            ChecksumAt::Trailing { n, width } => {
                format!("末尾倒数 {} 起 {} 字节", n, width.bytes())
            }
        }
    }
}

/// 位置约束（PrefixRange 的 constraints 元素）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    /// bytes[offset] 等于 value
    ByteEq { offset: usize, value: u8 },
    /// bytes[offset] 的 mask 位非零（如"功能码高 4 位 = 0x0"）
    ByteMaskNonZero { offset: usize, mask: u8 },
    /// bytes[offset] 在集合内（如合法功能码集合）
    ByteIn { offset: usize, values: Vec<u8> },
    /// 帧长 == base + bytes[len_offset]（"第 N 字节是后续数据长度位"）
    ///
    /// 这是可变长协议的**核心表达**：长度位在 `len_offset`（宽度 `width`，字节序 `endian`），
    /// 帧总长 = `base` + 该值。与 ProtocolSpec.length 是同一概念的两种粒度
    /// （此处为匹配侧校验，ProtocolSpec 为解码侧切帧）。
    LengthFromByte {
        len_offset: usize,
        width: Width,
        endian: Endian,
        base: i64,
    },
}

impl Constraint {
    pub fn summary(&self) -> String {
        match self {
            Constraint::ByteEq { offset, value } => format!("[{}] == {:02X}", offset, value),
            Constraint::ByteMaskNonZero { offset, mask } => {
                format!("[{}] & {:02X} != 0", offset, mask)
            }
            Constraint::ByteIn { offset, values } => {
                let list: Vec<String> = values.iter().map(|v| format!("{:02X}", v)).collect();
                format!("[{}] ∈ [{}]", offset, list.join(" "))
            }
            Constraint::LengthFromByte {
                len_offset,
                width,
                endian,
                base,
            } => {
                let endian = match endian {
                    Endian::Big => "BE",
                    Endian::Little => "LE",
                };
                format!(
                    "帧长 = {} + {}([{}] {})",
                    base,
                    width.bytes(),
                    len_offset,
                    endian
                )
            }
        }
    }
}

/// 匹配条件 AST（组合节点 + 原子谓词）
///
/// 组合节点支持 AND / OR / NOT 嵌套；原子谓词各自对应 NetAssist 的一种匹配形态。
///
/// **有意不提供"字段比较"之外的正则等昂贵谓词的前置**：求值顺序由用户拖拽决定，
/// 且短路求值保证廉价谓词先失败时昂贵谓词不执行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MatchNode {
    // ===== 组合节点 =====
    /// 全部子条件成立（空列表在数学上恒为 true，UI 必须拒绝保存空组合 —— 决策 U-3）
    All { children: Vec<MatchNode> },
    /// 任一子条件成立（空列表恒不成立）
    Any { children: Vec<MatchNode> },
    /// 子条件不成立
    Not { child: Box<MatchNode> },

    // ===== 原子谓词：字节形态（NetAssist §3.1–§3.2.3）=====
    /// 【立即数匹配】帧中含指定字节序列（NetAssist §3.1）
    Contains { bytes: BytePattern },
    /// 【定长精确匹配】帧长 = len 且逐字节相等（NetAssist §3.2.1）
    FixedExact { len: usize, bytes: BytePattern },
    /// 【定长模糊匹配】帧长 = len，mask 为 1 的位参与比较（NetAssist §3.2.2）
    ///
    /// `bytes.len() == mask.len() == len`；mask 中每个字节的位为 1 表示该位必须匹配。
    FixedMask {
        len: usize,
        bytes: BytePattern,
        mask: Vec<u8>,
    },
    /// 【弹性模糊匹配】头部匹配 + 长度区间 + 位置约束（NetAssist §3.2.3）
    PrefixRange {
        prefix: BytePattern,
        min_len: usize,
        max_len: usize,
        /// 附加的位置约束，留空则只校验前缀与长度（模式 A：变长帧头识别）
        #[serde(default)]
        constraints: Vec<Constraint>,
    },

    // ===== 原子谓词：位置 / 长度 =====
    /// 帧长度在 [min, max] 闭区间内
    Length { min: usize, max: usize },
    /// 指定偏移处的字节满足约束
    ByteAt { offset: usize, op: ByteOp },
    /// 指定偏移处的多字节整数值满足比较（带字节序与符号）
    ScalarAt {
        offset: usize,
        width: Width,
        endian: Endian,
        signed: bool,
        cmp: Cmp,
        value: i64,
    },
    /// 帧尾若干字节满足约束
    Suffix { bytes: BytePattern },

    // ===== 原子谓词：文本 / 正则 =====
    /// 正则匹配（按 UTF-8 文本解释帧；非法 UTF-8 帧不参与）
    Regex { pattern: String },

    // ===== 原子谓词：来源 =====
    /// 来源地址在给定集合内（IP 字面量或 CIDR 网段；对应 F-14）
    From { addrs: Vec<String> },

    // ===== 原子谓词：语义 =====
    /// 命名/路径字段等于期望值（走 `rx.*` 取值层，见 plan-reply-rules-expr.md）
    ///
    /// `path` 形如 "u16be" / "u8"，`offset` 是内联偏移。
    /// 二期接入 ProtocolSpec 后，会优先解析命名字段、回退内联偏移（决策 D-11）。
    FieldEq {
        accessor: String,
        offset: usize,
        cmp: Cmp,
        value: i64,
    },

    /// 校验值验算通过
    ///
    /// `range` 是参与计算的**字节区间**（起始偏移 + 长度，None=到校验位之前的帧尾），
    /// `at` 是校验值在帧中的位置。
    ChecksumValid {
        algorithm: ChecksumAlgorithm,
        at: ChecksumAt,
        #[serde(default)]
        range: Option<(usize, usize)>,
    },
}

impl MatchNode {
    /// 条件树最大深度（UI 与解析期双重限制，超过拒绝保存）
    pub fn depth(&self) -> usize {
        match self {
            MatchNode::All { children } | MatchNode::Any { children } => {
                1 + children.iter().map(|c| c.depth()).max().unwrap_or(0)
            }
            MatchNode::Not { child } => 1 + child.depth(),
            _ => 1,
        }
    }

    /// 是否表示「匹配全部报文」（空 `All` 组合，语义 = 任意消息）。
    ///
    /// 仅用于 UI 判定与摘要展示，不参与求值（求值路径见 `matcher.rs`）。
    pub fn is_match_all(&self) -> bool {
        matches!(self, MatchNode::All { children } if children.is_empty())
    }
}

/// 规则级编码器覆盖。
///
/// **这是二进制协议（Modbus / JT808）能不能用的关键**：连接的 trailer（CRLF）
/// 对**所有**写入生效，如果应答继承了它，就会在 CRC 后面多出 2 字节，
/// 协议直接错且症状隐晦（对端只报校验错）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCodec {
    /// 原样写出，不加任何结尾（二进制协议必需）
    Raw,
    /// 追加 LF
    Lf,
    /// 追加 CRLF
    Crlf,
    /// 继承连接级 send_trailer 设置（默认，与手动发送完全一致）
    #[default]
    Inherit,
}

impl RuleCodec {
    pub const ALL: [RuleCodec; 4] = [
        RuleCodec::Inherit,
        RuleCodec::Raw,
        RuleCodec::Lf,
        RuleCodec::Crlf,
    ];
}

/// 应答载荷
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplyPayload {
    /// 原文（可含 `${rx.*}` / `${seq}` / `${crc16modbus:...}` 等变量）
    pub text: String,
    /// 原文是否按 hex 解释（与连接的 message_input_mode 解耦，规则自带模式）
    #[serde(default)]
    pub hex_mode: bool,
    /// 发送编码器覆盖；`Inherit` = 继承连接 encoder（含 trailer）
    #[serde(default)]
    pub codec: RuleCodec,
}

impl Default for ReplyPayload {
    fn default() -> Self {
        Self {
            text: String::new(),
            hex_mode: false,
            codec: RuleCodec::default(),
        }
    }
}

/// 一次命中所需的全部预编译产物。
///
/// 运行期派生值，不入持久化（`text` 是真源，在 `ReplyRulesStore::replace()` 时重建）。
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledReply {
    /// 无 `${` 时的最终字节（零渲染开销快路径）
    pub fixed: Option<Vec<u8>>,
    /// 含变量时的预解析模板
    pub template: Option<CompiledTemplate>,
    /// 模板是否引用接收帧（决定是否构造 `RenderContext::for_reply`）
    pub needs_rx: bool,
}

impl CompiledReply {
    /// 按载荷原文构建：无变量走固定字节快路径，含变量则预解析模板
    pub fn build(text: &str, hex_mode: bool) -> Self {
        if !text.contains("${") {
            let bytes = if hex_mode {
                hex_to_bytes(text)
            } else {
                text.as_bytes().to_vec()
            };
            return Self {
                fixed: Some(bytes),
                template: None,
                needs_rx: false,
            };
        }
        let template = CompiledTemplate::new(text);
        Self {
            needs_rx: template.needs_rx(),
            fixed: None,
            template: Some(template),
        }
    }
}

/// 一条完整规则
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplyRule {
    /// 稳定 UUID（持久化真源；运行期表的键）
    pub id: String,
    /// 展示名，如「Modbus 读保持寄存器」
    pub name: String,
    /// 备注（NetAssist 的「模板注解」，纯展示）
    #[serde(default)]
    pub description: String,
    pub enabled: bool,
    /// 求值顺序，升序；相同值按 `id` 字典序稳定排序
    pub priority: u32,
    /// 标签（用于分组筛选与整包导出，如 ["modbus", "hj212"]）
    #[serde(default)]
    pub tags: Vec<String>,
    pub matcher: MatchNode,
    /// 命中后发送的应答载荷
    pub payload: ReplyPayload,
}

impl ReplyRule {
    /// 新建一条最小规则（UI「新建规则」入口用）
    ///
    /// 默认条件是 `All{}` 空组合 —— 恒真，语义为「匹配全部报文」。
    /// 这意味着不填任何条件的新规则会兜底回复所有报文，UI 需在条件区明确显示这一点。
    pub fn new(name: impl Into<String>, priority: u32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            enabled: true,
            priority,
            tags: Vec::new(),
            matcher: MatchNode::All {
                children: Vec::new(),
            },
            payload: ReplyPayload::default(),
        }
    }

    /// 该规则的预编译载荷
    pub fn compiled_payload(&self) -> CompiledReply {
        CompiledReply::build(&self.payload.text, self.payload.hex_mode)
    }
}

/// 规则集持久化容器（按连接分组）
///
/// 每条规则只属于一个连接：`connections` 的 key 即 `connection_id`（= tab_id），
/// value 是该连接的规则列表，物理顺序即求值优先级（自上而下、命中即停）。
/// 连接页的「自动回复」开关是该连接唯一总闸；规则只存在于这张表里。
///
/// 没有"未命中时的行为"开关：未命中一律什么都不做（不回、不记、不提示），
/// 这是整个功能的语义边界，不需要用户配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplyRulesConfig {
    /// schema 版本，为未来迁移预留
    #[serde(default = "default_schema_version")]
    pub version: u32,
    /// 按连接分组的规则表：connection_id -> 该连接的规则列表
    #[serde(default)]
    pub connections: HashMap<String, Vec<ReplyRule>>,
}

/// 当前 schema 版本
pub const REPLY_RULES_SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    REPLY_RULES_SCHEMA_VERSION
}

impl Default for ReplyRulesConfig {
    fn default() -> Self {
        Self {
            version: REPLY_RULES_SCHEMA_VERSION,
            connections: HashMap::new(),
        }
    }
}

// ============================================================================
// 保存期校验
// ============================================================================

/// 校验问题的严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// 必须修（UI 禁用保存）
    Error,
    /// 建议修（UI 黄字提示，仍可保存）
    Warning,
}

/// 校验问题：带**字段路径**，UI 据此滚动并高亮到具体那一项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    /// 字段路径，如 `matcher.children[1].value`
    pub path: String,
    /// 问题描述（i18n key 由 UI 映射；这里给中文兜底文案）
    pub message: String,
}

impl Issue {
    pub fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn warning(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            path: path.into(),
            message: message.into(),
        }
    }
}

/// 条件树最大深度（超过则拒绝保存；UI 递归渲染的可控性依赖它）
pub const MAX_MATCH_DEPTH: usize = 8;
/// 正则最大长度（安全边界，见 plan-reply-rules.md §9.2）
pub const MAX_REGEX_LEN: usize = 256;
/// 规则名最大长度
pub const MAX_RULE_NAME_LEN: usize = 64;
/// 备注最大长度
pub const MAX_RULE_DESC_LEN: usize = 256;

/// 校验一条规则；返回全部问题（空 = 可保存）
pub fn validate_rule(rule: &ReplyRule) -> Vec<Issue> {
    let mut issues = Vec::new();

    if rule.name.trim().is_empty() {
        issues.push(Issue::error("name", "规则名不能为空"));
    }
    if rule.name.chars().count() > MAX_RULE_NAME_LEN {
        issues.push(Issue::error(
            "name",
            format!("规则名不能超过 {} 个字符", MAX_RULE_NAME_LEN),
        ));
    }
    if rule.description.chars().count() > MAX_RULE_DESC_LEN {
        issues.push(Issue::error(
            "description",
            format!("备注不能超过 {} 个字符", MAX_RULE_DESC_LEN),
        ));
    }
    for (i, tag) in rule.tags.iter().enumerate() {
        if tag.trim().is_empty() {
            issues.push(Issue::error(format!("tags[{}]", i), "标签不能为空"));
        } else if tag.chars().count() > 32 {
            issues.push(Issue::error(
                format!("tags[{}]", i),
                "标签不能超过 32 个字符",
            ));
        }
    }
    if rule.matcher.depth() > MAX_MATCH_DEPTH {
        issues.push(Issue::error(
            "matcher",
            format!("条件嵌套不能超过 {} 层", MAX_MATCH_DEPTH),
        ));
    }

    validate_match_node(&rule.matcher, "matcher", &mut issues);
    validate_payload(&rule.payload, "payload", &mut issues);
    issues
}

/// 校验一个条件节点（递归）
pub fn validate_match_node(node: &MatchNode, path: &str, issues: &mut Vec<Issue>) {
    match node {
        // 空 `All{}` 数学上恒成立 → 语义就是「匹配全部报文」，是**合法配置**：
        // 用户想兜底回复时只需留一个空的「全部满足」。空 `Any{}` 恒不成立，
        // 只会得到一条永不触发的死规则，仍然报错。
        MatchNode::All { children } => {
            for (i, child) in children.iter().enumerate() {
                validate_match_node(child, &format!("{}.children[{}]", path, i), issues);
            }
        }
        MatchNode::Any { children } => {
            if children.is_empty() {
                issues.push(Issue::error(path, "组合条件不能为空"));
            }
            for (i, child) in children.iter().enumerate() {
                validate_match_node(child, &format!("{}.children[{}]", path, i), issues);
            }
        }
        MatchNode::Not { child } => {
            validate_match_node(child, &format!("{}.child", path), issues);
        }
        MatchNode::Contains { bytes } => {
            validate_pattern(bytes, &format!("{}.bytes", path), issues);
            if bytes.is_empty() {
                issues.push(Issue::error(
                    &format!("{}.bytes", path),
                    "「包含字节序列」不能为空（空模式恒不成立）",
                ));
            }
        }
        MatchNode::FixedExact { len, bytes } => {
            validate_pattern(bytes, &format!("{}.bytes", path), issues);
            match bytes.resolve() {
                Ok(b) if b.len() != *len => issues.push(Issue::error(
                    path,
                    format!("定长精确匹配的长度 {} 与字节数 {} 不一致", len, b.len()),
                )),
                Err(e) => issues.push(Issue::error(&format!("{}.bytes", path), e)),
                _ => {}
            }
        }
        MatchNode::FixedMask { len, bytes, mask } => {
            validate_pattern(bytes, &format!("{}.bytes", path), issues);
            if mask.len() != *len {
                issues.push(Issue::error(
                    &format!("{}.mask", path),
                    format!("掩码长度 {} 与帧长度 {} 不一致", mask.len(), len),
                ));
            }
            match bytes.resolve() {
                Ok(b) if b.len() != *len => issues.push(Issue::error(
                    path,
                    format!("定长模糊匹配的长度 {} 与字节数 {} 不一致", len, b.len()),
                )),
                Err(e) => issues.push(Issue::error(&format!("{}.bytes", path), e)),
                _ => {}
            }
        }
        MatchNode::PrefixRange {
            prefix,
            min_len,
            max_len,
            constraints,
        } => {
            validate_pattern(prefix, &format!("{}.prefix", path), issues);
            if min_len > max_len {
                issues.push(Issue::error(
                    path,
                    format!("长度区间非法: 最小 {} 大于最大 {}", min_len, max_len),
                ));
            }
            if let Ok(p) = prefix.resolve() {
                if p.len() > *min_len {
                    issues.push(Issue::error(
                        &format!("{}.prefix", path),
                        format!("前缀 {} 字节长于最小帧长 {}", p.len(), min_len),
                    ));
                }
            }
            for (i, c) in constraints.iter().enumerate() {
                let cpath = format!("{}.constraints[{}]", path, i);
                match c {
                    Constraint::ByteIn { values, offset } => {
                        if values.is_empty() {
                            issues.push(Issue::error(&cpath, "字节集合不能为空"));
                        }
                        if *offset >= *max_len {
                            issues.push(Issue::error(
                                &cpath,
                                format!("约束偏移 {} 超出最大帧长 {}", offset, max_len),
                            ));
                        }
                    }
                    Constraint::ByteEq { offset, .. }
                    | Constraint::ByteMaskNonZero { offset, .. } => {
                        if *offset >= *max_len {
                            issues.push(Issue::error(
                                &cpath,
                                format!("约束偏移 {} 超出最大帧长 {}", offset, max_len),
                            ));
                        }
                    }
                    Constraint::LengthFromByte {
                        len_offset, width, ..
                    } => {
                        if len_offset + width.bytes() > *min_len {
                            issues.push(Issue::error(
                                &cpath,
                                "长度位超出最小帧长（长度位本身必须落在帧内）",
                            ));
                        }
                    }
                }
            }
        }
        MatchNode::Length { min, max } => {
            if min > max {
                issues.push(Issue::error(
                    path,
                    format!("长度区间非法: 最小 {} 大于最大 {}", min, max),
                ));
            }
        }
        MatchNode::ByteAt { op, .. } => {
            if let ByteOp::In { values } = op {
                if values.is_empty() {
                    issues.push(Issue::error(&format!("{}.op", path), "字节集合不能为空"));
                }
            }
        }
        MatchNode::ScalarAt { .. } => {}
        MatchNode::Suffix { bytes } => {
            validate_pattern(bytes, &format!("{}.bytes", path), issues);
            if bytes.is_empty() {
                issues.push(Issue::error(
                    &format!("{}.bytes", path),
                    "「帧尾匹配」不能为空",
                ));
            }
        }
        MatchNode::Regex { pattern } => {
            if pattern.chars().count() > MAX_REGEX_LEN {
                issues.push(Issue::error(
                    &format!("{}.pattern", path),
                    format!("正则不能超过 {} 个字符", MAX_REGEX_LEN),
                ));
            } else if let Err(e) = regex::Regex::new(pattern) {
                issues.push(Issue::error(
                    &format!("{}.pattern", path),
                    format!("正则非法: {}", e),
                ));
            }
        }
        MatchNode::From { addrs } => {
            if addrs.is_empty() {
                issues.push(Issue::error(&format!("{}.addrs", path), "来源地址不能为空"));
            }
            for (i, addr) in addrs.iter().enumerate() {
                if crate::reply::matcher::parse_addr_spec(addr).is_none() {
                    issues.push(Issue::error(
                        &format!("{}.addrs[{}]", path, i),
                        format!("非法地址或网段: {}", addr),
                    ));
                }
            }
        }
        MatchNode::FieldEq { accessor, .. } => {
            if crate::reply::frame::parse_scalar_accessor(accessor).is_none() {
                issues.push(Issue::error(
                    &format!("{}.accessor", path),
                    format!("未知取值标识: {}（如 u8 / u16be / i32le）", accessor),
                ));
            }
        }
        MatchNode::ChecksumValid { at, range, .. } => {
            if let Some((offset, len)) = range {
                if len == &0 {
                    issues.push(Issue::error(
                        &format!("{}.range", path),
                        "校验覆盖长度不能为 0",
                    ));
                }
                if offset.checked_add(*len).is_none() {
                    issues.push(Issue::error(&format!("{}.range", path), "校验覆盖区间溢出"));
                }
            }
            if let ChecksumAt::Trailing { n, .. } = at {
                if *n == 0 {
                    issues.push(Issue::error(
                        &format!("{}.at", path),
                        "末尾倒数字节数不能为 0",
                    ));
                }
            }
        }
    }
}

/// 校验字节模式（非法 hex 当场拒绝）
fn validate_pattern(pattern: &BytePattern, path: &str, issues: &mut Vec<Issue>) {
    if let Err(e) = pattern.resolve() {
        issues.push(Issue::error(path, e));
    }
}

/// 校验应答载荷（只剩"回复"一种语义）
pub fn validate_payload(payload: &ReplyPayload, path: &str, issues: &mut Vec<Issue>) {
    if payload.text.trim().is_empty() {
        // T-1：空载荷**合法** —— 运行期语义是"命中但不发送、仍然计数"
        // （见 exec.rs `handle_frame` 与 `test_empty_payload_not_sent`）。
        // 保存期只给提示、不阻塞：用户可用它做"只观察命中、不回复"的调试规则。
        issues.push(Issue::warning(
            &format!("{}.text", path),
            "应答内容为空：命中后不会发送任何数据（仅计数）",
        ));
    }
    if payload.hex_mode {
        // hex 模式下原文必须能解析成字节；含变量的模板无法静态判定，
        // 因此只在完全无变量时严格校验，含变量时给出提示
        if !payload.text.contains("${") {
            let digits = count_hex_digits(&payload.text);
            if digits % 2 != 0 {
                issues.push(Issue::error(
                    &format!("{}.text", path),
                    "十六进制位数必须为偶数",
                ));
            }
        } else if payload.codec == RuleCodec::Inherit {
            issues.push(Issue::warning(
                &format!("{}.codec", path),
                "二进制协议建议把编码方式改为「原样输出」，否则会在帧尾追加换行",
            ));
        }
    }
}

// ============================================================================
// 条件摘要（规则列表的「一眼看懂」）
// ============================================================================

/// 生成条件摘要（返回结构化 token 而非直接拼字符串，便于 i18n 与富文本渲染）。
///
/// 这是「零代码可用」的关键：用户扫一眼列表就知道每条规则在做什么。
pub fn format_matcher_summary(node: &MatchNode) -> Vec<String> {
    match node {
        MatchNode::All { children } => {
            if children.is_empty() {
                return vec!["匹配全部报文".to_string()];
            }
            let mut parts = Vec::new();
            for child in children {
                parts.extend(format_matcher_summary(child));
            }
            vec![parts.join(" 且 ")]
        }
        MatchNode::Any { children } => {
            let parts: Vec<String> = children
                .iter()
                .map(|c| format_matcher_summary(c).join(" 且 "))
                .collect();
            vec![parts.join(" 或 ")]
        }
        MatchNode::Not { child } => {
            vec![format!(
                "非({})",
                format_matcher_summary(child).join(" 且 ")
            )]
        }
        MatchNode::Contains { bytes } => vec![format!("包含 {}", bytes.display())],
        MatchNode::FixedExact { len, bytes } => {
            vec![format!("定长精确 {} 字节 = {}", len, bytes.display())]
        }
        MatchNode::FixedMask { len, bytes, .. } => {
            vec![format!("定长模糊 {} 字节 ≈ {}", len, bytes.display())]
        }
        MatchNode::PrefixRange {
            prefix,
            min_len,
            max_len,
            constraints,
        } => {
            let mut parts = vec![format!(
                "前缀 {} · 长度 {}..{}",
                prefix.display(),
                min_len,
                max_len
            )];
            for c in constraints {
                parts.push(c.summary());
            }
            vec![parts.join(" · ")]
        }
        MatchNode::Length { min, max } => {
            if min == max {
                vec![format!("帧长 = {}", min)]
            } else {
                vec![format!("帧长 {}..{}", min, max)]
            }
        }
        MatchNode::ByteAt { offset, op } => {
            vec![format!("字节[{}] {}", offset, op.summary())]
        }
        MatchNode::ScalarAt {
            offset,
            width,
            endian,
            signed,
            cmp,
            value,
        } => {
            let sign = if *signed { "i" } else { "u" };
            let endian = match endian {
                Endian::Big => "be",
                Endian::Little => "le",
            };
            vec![format!(
                "{}{}{}[{}] {} {}",
                sign,
                width.bytes() * 8,
                endian,
                offset,
                cmp.symbol(),
                value
            )]
        }
        MatchNode::Suffix { bytes } => vec![format!("帧尾 {}", bytes.display())],
        MatchNode::Regex { pattern } => vec![format!("正则 /{}/", pattern)],
        MatchNode::From { addrs } => vec![format!("来源 ∈ [{}]", addrs.join(", "))],
        MatchNode::FieldEq {
            accessor,
            offset,
            cmp,
            value,
        } => vec![format!(
            "{}({}) {} {}",
            accessor,
            offset,
            cmp.symbol(),
            value
        )],
        MatchNode::ChecksumValid { algorithm, at, .. } => {
            vec![format!("{} 校验有效（{}）", algorithm.name(), at.summary())]
        }
    }
}

/// 回复内容摘要（规则列表第二行）
pub fn format_reply_summary(payload: &ReplyPayload) -> String {
    let mode = if payload.hex_mode { "Hex" } else { "文本" };
    let codec = match payload.codec {
        RuleCodec::Inherit => "继承",
        RuleCodec::Raw => "原样",
        RuleCodec::Lf => "LF",
        RuleCodec::Crlf => "CRLF",
    };
    let text = if payload.text.chars().count() > 40 {
        let head: String = payload.text.chars().take(40).collect();
        format!("{}…", head)
    } else {
        payload.text.clone()
    };
    format!("回复 {} ({}, {})", text, mode, codec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_with(node: MatchNode) -> ReplyRule {
        // 动作已收敛为唯一的「回复」，默认载荷为空会被校验判为**告警**（T-1）；
        // 本 helper 用于"条件非法"的用例，故补一个合法载荷，避免噪音干扰断言。
        ReplyRule {
            matcher: node,
            payload: ReplyPayload {
                text: "OK".to_string(),
                hex_mode: false,
                codec: RuleCodec::Inherit,
            },
            ..ReplyRule::new("测试规则", 10)
        }
    }

    /// serde 往返：规则集 JSON 与内存模型完全一致（持久化真源）
    #[test]
    fn test_config_serde_roundtrip() {
        let mut config = ReplyRulesConfig::default();
        config
            .connections
            .entry("conn-1".to_string())
            .or_default()
            .push(ReplyRule {
            name: "Modbus 读保持寄存器应答".to_string(),
            description: "功能码 0x03；帧长 8".to_string(),
            priority: 10,
            tags: vec!["modbus".to_string(), "rtu".to_string()],
            matcher: MatchNode::All {
                children: vec![
                    MatchNode::PrefixRange {
                        prefix: BytePattern::Hex("01 03".to_string()),
                        min_len: 8,
                        max_len: 8,
                        constraints: vec![Constraint::ByteEq {
                            offset: 1,
                            value: 3,
                        }],
                    },
                    MatchNode::ChecksumValid {
                        algorithm: ChecksumAlgorithm::Crc16Modbus,
                        at: ChecksumAt::Trailing {
                            n: 2,
                            width: Width::U16,
                        },
                        range: Some((0, 6)),
                    },
                ],
            },
            payload: ReplyPayload {
                text: "01 03 04 00 0A 00 14 ${crc16modbus:0:9:le}".to_string(),
                hex_mode: true,
                codec: RuleCodec::Raw,
            },
            ..ReplyRule::new("占位", 0)
        });

        let json = serde_json::to_string_pretty(&config).unwrap();
        let back: ReplyRulesConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);

        // 字段名是稳定的对外契约（文档 §4.4 的示例必须能加载）
        assert!(json.contains("\"crc16_modbus\""), "算法应序列化为字符串");
        assert!(
            json.contains("\"kind\": \"prefix_range\""),
            "条件 tag 为 kind"
        );
        assert!(
            json.contains("\"connections\""),
            "规则按连接分组存储"
        );
    }

    /// 向后兼容：旧配置文件没有 `reply_rules` 字段时得到空规则集，
    /// 行为与升级前完全一致
    #[test]
    fn test_legacy_config_without_reply_rules() {
        let json = r#"{
    "connections": [],
    "auto_save": true,
    "save_interval": 30
}"#;
        let config: ReplyRulesConfig = serde_json::from_str("{}").unwrap();
        assert!(config.connections.is_empty());
        assert_eq!(config.version, REPLY_RULES_SCHEMA_VERSION);
        // 上面那份"旧配置"里没有 reply_rules 字段 —— 由 AppConfig 的 serde(default) 兜底
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(parsed.get("reply_rules").is_none());
    }

    /// 文档 §4.4 的 JSON 示例必须能原样加载（对外契约）
    #[test]
    fn test_documented_json_example_loads() {
        let json = r#"{
  "version": 1,
  "connections": {
    "3f2a9c14-8b6e-4d21-9f03-7c1e5a4b8d02": [
      {
        "id": "3f2a9c14-8b6e-4d21-9f03-7c1e5a4b8d02",
        "name": "Modbus 读保持寄存器应答",
        "description": "功能码 0x03；帧长 8；返回固定 2 个寄存器",
        "enabled": true,
        "priority": 10,
        "tags": ["modbus", "rtu"],
        "matcher": {
          "kind": "all",
          "children": [
            {
              "kind": "prefix_range",
              "prefix": { "hex": "01 03" },
              "min_len": 8,
              "max_len": 8,
              "constraints": [
                { "kind": "byte_eq", "offset": 1, "value": 3 }
              ]
            },
            {
              "kind": "checksum_valid",
              "algorithm": "crc16_modbus",
              "at": { "kind": "trailing", "n": 2, "width": "u16" },
              "range": [0, 6]
            }
          ]
        },
        "payload": {
          "text": "01 03 04 00 0A 00 14 ${rx.crc16modbus:0:9}",
          "hex_mode": true,
          "codec": "raw"
        }
      },
      {
        "id": "9b1d4e77-2c05-4a8f-b3e6-1d7f2a9c4e10",
        "name": "ECHO 回显一切",
        "enabled": true,
        "priority": 9000,
        "tags": ["echo"],
        "matcher": { "kind": "length", "min": 1, "max": 65535 },
        "payload": { "text": "${rx.raw}", "hex_mode": true, "codec": "raw" }
      }
    ]
  }
}"#;
        let config: ReplyRulesConfig = serde_json::from_str(json).unwrap();
        let rules = config.connections.get("3f2a9c14-8b6e-4d21-9f03-7c1e5a4b8d02").unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules.iter().any(|r| r.enabled));
    }

    /// 空 `All{}` 是**合法配置**：恒真，语义即「匹配全部报文」（用户显式留空即兜底）
    #[test]
    fn test_empty_all_allowed() {
        let rule = rule_with(MatchNode::All {
            children: Vec::new(),
        });
        let issues = validate_rule(&rule);
        assert!(
            !issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.path == "matcher"),
            "空 All 不应报错: {:?}",
            issues
        );
    }

    /// `is_match_all()` 只对空 `All` 返回 true；空 `Any` / 非空 `All` 均不是。
    #[test]
    fn test_is_match_all() {
        assert!(
            MatchNode::All {
                children: Vec::new()
            }
            .is_match_all()
        );

        assert!(
            !MatchNode::All {
                children: vec![MatchNode::Length { min: 1, max: 8 }]
            }
            .is_match_all()
        );

        assert!(
            !MatchNode::Any {
                children: Vec::new()
            }
            .is_match_all()
        );

        assert!(!MatchNode::Length { min: 1, max: 8 }.is_match_all());
    }

    /// 空 `Any{}` 恒不成立，只会得到一条永不触发的死规则 → 仍然必须报错
    #[test]
    fn test_empty_any_rejected() {
        let rule = rule_with(MatchNode::Any {
            children: Vec::new(),
        });
        let issues = validate_rule(&rule);
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.path == "matcher"),
            "空 Any 必须报错: {:?}",
            issues
        );
    }

    /// T-1：空载荷是**合法配置**（运行期语义 = 命中但不发送、仍计数），
    /// 校验只给告警，不得再判 Error —— 否则 UI 会禁用保存，用户永远存不下这类规则。
    ///
    /// 运行期侧的对应断言见 `exec::tests::test_empty_payload_not_sent`
    /// （命中计数 +1 且 `reply` 为 None）。
    #[test]
    fn test_empty_payload_allowed_with_warning_only() {
        let rule = ReplyRule {
            matcher: MatchNode::Length { min: 1, max: 8 },
            payload: ReplyPayload {
                text: "   ".to_string(),
                hex_mode: false,
                codec: RuleCodec::Inherit,
            },
            ..ReplyRule::new("只计数不回", 10)
        };
        let issues = validate_rule(&rule);
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Warning && i.path == "payload.text"),
            "空载荷应给告警: {:?}",
            issues
        );
        assert!(
            !issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.path == "payload.text"),
            "空载荷不得判 Error（T-1 语义为合法）: {:?}",
            issues
        );
    }

    /// 非法 hex / 长度不一致 / 区间倒置 / 非法正则 必须当场指出（并给字段路径）
    #[test]
    fn test_validation_rejects_bad_shapes() {
        // 非法 hex（奇数位）
        let issues = validate_rule(&rule_with(MatchNode::Contains {
            bytes: BytePattern::Hex("0 12".to_string()),
        }));
        assert!(
            issues.iter().any(|i| i.path == "matcher.bytes"),
            "{:?}",
            issues
        );

        // FixedExact 长度与字节数不一致
        let issues = validate_rule(&rule_with(MatchNode::FixedExact {
            len: 8,
            bytes: BytePattern::Hex("01 03".to_string()),
        }));
        assert!(issues.iter().any(|i| i.path == "matcher"), "{:?}", issues);

        // FixedMask 掩码长度不一致
        let issues = validate_rule(&rule_with(MatchNode::FixedMask {
            len: 2,
            bytes: BytePattern::Hex("01 03".to_string()),
            mask: vec![0xFF],
        }));
        assert!(
            issues.iter().any(|i| i.path == "matcher.mask"),
            "{:?}",
            issues
        );

        // Length 区间倒置
        let issues = validate_rule(&rule_with(MatchNode::Length { min: 9, max: 2 }));
        assert!(issues.iter().any(|i| i.path == "matcher"), "{:?}", issues);

        // PrefixRange 区间倒置 + 前缀长于 min_len
        let issues = validate_rule(&rule_with(MatchNode::PrefixRange {
            prefix: BytePattern::Hex("01 03 04".to_string()),
            min_len: 2,
            max_len: 1,
            constraints: Vec::new(),
        }));
        assert!(
            issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count()
                >= 2,
            "{:?}",
            issues
        );

        // 非法正则
        let issues = validate_rule(&rule_with(MatchNode::Regex {
            pattern: "([".to_string(),
        }));
        assert!(
            issues.iter().any(|i| i.path == "matcher.pattern"),
            "{:?}",
            issues
        );

        // 非法来源地址
        let issues = validate_rule(&rule_with(MatchNode::From {
            addrs: vec!["not-an-ip".to_string()],
        }));
        assert!(
            issues.iter().any(|i| i.path == "matcher.addrs[0]"),
            "{:?}",
            issues
        );

        // 未知取值标识
        let issues = validate_rule(&rule_with(MatchNode::FieldEq {
            accessor: "u17be".to_string(),
            offset: 0,
            cmp: Cmp::Eq,
            value: 1,
        }));
        assert!(
            issues.iter().any(|i| i.path == "matcher.accessor"),
            "{:?}",
            issues
        );
    }

    /// hex 模式 + 未选 Raw 时必须给黄字警告（二进制协议继承 CRLF 会静默发错帧）
    #[test]
    fn test_hex_mode_without_raw_warns() {
        let issues = validate_rule(&rule_with(MatchNode::Length { min: 1, max: 8 })).clone();
        assert!(issues.is_empty());

        let rule = ReplyRule {
            matcher: MatchNode::Length { min: 1, max: 8 },
            payload: ReplyPayload {
                text: "01 03 ${seq}".to_string(),
                hex_mode: true,
                codec: RuleCodec::Inherit,
            },
            ..ReplyRule::new("二进制", 1)
        };
        let issues = validate_rule(&rule);
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Warning && i.path == "payload.codec"),
            "{:?}",
            issues
        );
    }

    /// BytePattern::resolve 的容错与严格边界
    ///
    /// 严格性是刻意的：`hex_to_bytes` 面向"用户手输的发送框"会静默丢弃非法字符，
    /// 而规则模式写错一位却静默变成另一个字节，会让规则永不命中且毫无线索。
    #[test]
    fn test_byte_pattern_resolve() {
        assert_eq!(
            BytePattern::Hex("01 03".to_string()).resolve().unwrap(),
            vec![0x01, 0x03]
        );
        assert_eq!(
            BytePattern::Hex("0103".to_string()).resolve().unwrap(),
            vec![0x01, 0x03]
        );
        assert_eq!(
            BytePattern::Hex("0x01,0x03".to_string()).resolve().unwrap(),
            vec![0x01, 0x03]
        );
        assert_eq!(
            BytePattern::Hex("01-03:04".to_string()).resolve().unwrap(),
            vec![0x01, 0x03, 0x04]
        );
        assert_eq!(
            BytePattern::Hex("  ".to_string()).resolve().unwrap(),
            Vec::<u8>::new()
        );
        assert_eq!(
            BytePattern::Text("AT".to_string()).resolve().unwrap(),
            b"AT".to_vec()
        );
        // 奇数位必须失败（不能静默丢弃半字节）
        assert!(BytePattern::Hex("0 1 2".to_string()).resolve().is_err());
        assert!(BytePattern::Hex("010".to_string()).resolve().is_err());
        // 非法字符必须失败（不能静默解析为空）
        assert!(BytePattern::Hex("ZZ".to_string()).resolve().is_err());
        assert!(BytePattern::Hex("01 ZZ".to_string()).resolve().is_err());
        assert!(BytePattern::Hex("你好".to_string()).resolve().is_err());
        // is_empty
        assert!(BytePattern::Hex("".to_string()).is_empty());
        assert!(BytePattern::Text("".to_string()).is_empty());
        assert!(!BytePattern::Text("A".to_string()).is_empty());
        // display：hex 用紧凑大写，text 加引号
        assert_eq!(BytePattern::Hex("01 0a".to_string()).display(), "010A");
        assert_eq!(BytePattern::Text("AT".to_string()).display(), "\"AT\"");
        assert_eq!(BytePattern::Hex("".to_string()).display(), "(空)");
    }

    /// 条件摘要：15 种节点都要给出非空且可读的文本
    #[test]
    fn test_matcher_summary_all_nodes() {
        let cases: Vec<MatchNode> = vec![
            MatchNode::All {
                children: vec![MatchNode::Length { min: 1, max: 8 }],
            },
            MatchNode::Any {
                children: vec![
                    MatchNode::Length { min: 1, max: 8 },
                    MatchNode::Length { min: 10, max: 20 },
                ],
            },
            MatchNode::Not {
                child: Box::new(MatchNode::Length { min: 0, max: 0 }),
            },
            MatchNode::Contains {
                bytes: BytePattern::Hex("01 03".to_string()),
            },
            MatchNode::FixedExact {
                len: 2,
                bytes: BytePattern::Hex("01 03".to_string()),
            },
            MatchNode::FixedMask {
                len: 2,
                bytes: BytePattern::Hex("01 03".to_string()),
                mask: vec![0xFF, 0x00],
            },
            MatchNode::PrefixRange {
                prefix: BytePattern::Hex("01 03".to_string()),
                min_len: 8,
                max_len: 8,
                constraints: vec![Constraint::ByteMaskNonZero {
                    offset: 1,
                    mask: 0x03,
                }],
            },
            MatchNode::Length { min: 8, max: 8 },
            MatchNode::ByteAt {
                offset: 0,
                op: ByteOp::Eq { value: 1 },
            },
            MatchNode::ScalarAt {
                offset: 2,
                width: Width::U16,
                endian: Endian::Big,
                signed: false,
                cmp: Cmp::Eq,
                value: 2,
            },
            MatchNode::Suffix {
                bytes: BytePattern::Hex("0D 0A".to_string()),
            },
            MatchNode::Regex {
                pattern: "^AT".to_string(),
            },
            MatchNode::From {
                addrs: vec!["127.0.0.1".to_string()],
            },
            MatchNode::FieldEq {
                accessor: "u8".to_string(),
                offset: 1,
                cmp: Cmp::Eq,
                value: 3,
            },
            MatchNode::ChecksumValid {
                algorithm: ChecksumAlgorithm::Crc16Modbus,
                at: ChecksumAt::Trailing {
                    n: 2,
                    width: Width::U16,
                },
                range: Some((0, 6)),
            },
        ];
        assert_eq!(cases.len(), 15, "必须覆盖全部 15 种节点");
        for node in cases {
            let summary = format_matcher_summary(&node).join(" 且 ");
            assert!(!summary.trim().is_empty(), "{:?} 的摘要不应为空", node);
        }

        // 具体文案锁定（防止 i18n 改造时丢语义）
        assert_eq!(
            format_matcher_summary(&MatchNode::Length { min: 8, max: 8 }),
            vec!["帧长 = 8"]
        );
        assert_eq!(
            format_matcher_summary(&MatchNode::Length { min: 1, max: 8 }),
            vec!["帧长 1..8"]
        );
        let combo = format_matcher_summary(&MatchNode::All {
            children: vec![
                MatchNode::Length { min: 8, max: 8 },
                MatchNode::ByteAt {
                    offset: 0,
                    op: ByteOp::Eq { value: 1 },
                },
            ],
        });
        assert_eq!(combo, vec!["帧长 = 8 且 字节[0] == 01"]);
    }

    /// 回复摘要只有一种形态
    #[test]
    fn test_reply_summary() {
        let payload = ReplyPayload {
            text: "ok".to_string(),
            hex_mode: false,
            codec: RuleCodec::Inherit,
        };
        assert_eq!(format_reply_summary(&payload), "回复 ok (文本, 继承)");

        // 超长文本截断（列表一行放不下）
        let long = ReplyPayload {
            text: "x".repeat(50),
            hex_mode: true,
            codec: RuleCodec::Raw,
        };
        let summary = format_reply_summary(&long);
        assert!(summary.ends_with("… (Hex, 原样)"), "实际: {}", summary);
    }

    /// 应答载荷直接序列化为 `payload` 字段（不再有动作枚举包装）
    #[test]
    fn test_rule_payload_serde_roundtrip() {
        let rule = ReplyRule {
            matcher: MatchNode::Length { min: 1, max: 8 },
            payload: ReplyPayload {
                text: "ok".to_string(),
                hex_mode: false,
                codec: RuleCodec::Inherit,
            },
            ..ReplyRule::new("serde", 1)
        };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"payload\""), "实际: {}", json);
        assert!(!json.contains("\"action\""), "动作枚举已删除: {}", json);
        let back: ReplyRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload.text, "ok");
        assert_eq!(back, rule);
    }

    /// 预编译载荷：无变量走固定字节快路径，含变量建模板并正确探测 needs_rx
    #[test]
    fn test_compiled_reply_paths() {
        let fixed = CompiledReply::build("6F 6B", true);
        assert!(fixed.template.is_none());
        assert_eq!(fixed.fixed.as_deref(), Some(&b"ok"[..]));
        assert!(!fixed.needs_rx);

        let text_fixed = CompiledReply::build("ok", false);
        assert_eq!(text_fixed.fixed.as_deref(), Some(&b"ok"[..]));

        let tpl = CompiledReply::build("${rx.u16be:2}", true);
        assert!(tpl.fixed.is_none());
        assert!(tpl.needs_rx);
        assert!(tpl.template.is_some());

        let no_rx = CompiledReply::build("seq=${seq}", false);
        assert!(!no_rx.needs_rx, "不含 rx.* 的模板不应要求接收帧上下文");

        assert_eq!(
            CompiledReply::build("", false).fixed.as_deref(),
            Some(&b""[..])
        );
    }

    /// 规则默认构造：新建即为"空 All + 空回复"，UI 必须提示后由用户补齐
    #[test]
    fn test_new_rule_defaults() {
        let rule = ReplyRule::new("新规则", 100);
        assert!(rule.enabled);
        assert_eq!(rule.priority, 100);
        assert!(
            uuid::Uuid::parse_str(&rule.id).is_ok(),
            "id 必须是合法 UUID"
        );
        assert_eq!(
            rule.compiled_payload().fixed.as_deref(),
            Some(&b""[..]),
            "默认回复内容为空"
        );
    }
}
