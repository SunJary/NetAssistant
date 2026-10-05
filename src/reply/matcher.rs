// 规则匹配器（L1 纯逻辑，零副作用、可无头单测）
//
// 设计要点（docs/plan-reply-rules.md §5）：
//   1. `evaluate()` 是**纯函数**：`(规则表, 帧快照, 是否收集轨迹) -> 命中结果`。
//      调试面板直接复用同一个函数做预演，因此"预演结果 = 真实行为"是结构上保证的。
//   2. `collect_trace` 是**必须显式传入**的参数而非可选项：热路径（压测洪泛）下
//      为每个失败谓词格式化"实际值"字符串会成为主要开销。写在签名里强制调用方表态。
//   3. 所有越界访问一律返回 false，**绝不 panic** —— 调试工具崩了比不工作更糟。

use crate::reply::frame::{RxContext, RxFrame};
use crate::reply::model::{ByteOp, Constraint, MatchNode};
use crate::reply::store::RuleRuntime;
use log::warn;
use std::net::IpAddr;
use std::sync::Arc;

/// 单一谓词的求值结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateResult {
    Pass,
    Fail,
    /// 被短路（前一个子条件已决定组合节点结果），未参与求值
    Skipped,
}

/// 一个谓词的匹配轨迹项（调试面板的"期望 vs 实际"）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateTrace {
    /// 谓词的可读描述，如 `prefix_range(01 03, len=8..8)`
    pub label: String,
    pub result: PredicateResult,
    /// 帧上的实际值（调试面板并排显示）
    pub actual: String,
}

impl PredicateTrace {
    fn new(label: String, result: PredicateResult, actual: String) -> Self {
        Self {
            label,
            result,
            actual,
        }
    }

    pub fn is_fail(&self) -> bool {
        self.result == PredicateResult::Fail
    }
}

/// 单条规则的匹配轨迹
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    pub rule_id: String,
    pub rule_name: String,
    /// 该规则整体是否命中
    pub hit: bool,
    /// 各谓词的逐项结果（深度优先，含被短路的项标记 skipped）
    pub predicates: Vec<PredicateTrace>,
}

impl TraceEntry {
    /// 第一个失败项 —— 直接回答"为什么没命中"，是调试体验的核心
    pub fn first_failure(&self) -> Option<&PredicateTrace> {
        self.predicates.iter().find(|p| p.is_fail())
    }
}

/// 匹配轨迹：按求值顺序记录每条规则的每个谓词结果
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchTrace {
    pub entries: Vec<TraceEntry>,
}

/// 一次求值的完整结果
#[derive(Debug, Clone)]
pub struct RuleOutcome {
    /// 命中的规则 id（未命中为 None）
    pub rule_id: Option<String>,
    /// 命中规则在传入规则表中的下标（P-8：调用方按下标直取 runtime，
    /// 既免去热路径 `payload` 的 String 克隆，也免去按 id 线性回查）
    pub rule_index: Option<usize>,
    /// 命中规则的预编译载荷（`Arc` 克隆，无堆分配；exec 层渲染用）
    pub compiled: Option<std::sync::Arc<crate::reply::model::CompiledReply>>,
    /// 匹配轨迹（仅在 `collect_trace == true` 时填充）
    pub trace: MatchTrace,
}

impl RuleOutcome {
    fn miss() -> Self {
        Self {
            rule_id: None,
            rule_index: None,
            compiled: None,
            trace: MatchTrace::default(),
        }
    }

    pub fn is_hit(&self) -> bool {
        self.rule_id.is_some()
    }
}

/// 规则求值：按已排序顺序遍历，命中即停。**纯函数，零副作用**。
///
/// `collect_trace == true` 时填充完整轨迹（调试面板用）；网络热路径必须传 `false`，
/// 只做短路布尔判断，避免每帧构造大量 String。
///
/// 参数用 `&[Arc<RuleRuntime>]` 而非 `&[RuleRuntime]`：规则表本体就是
/// `Arc<Vec<Arc<RuleRuntime>>>`，直接借用可避免每帧克隆规则（含正则缓存）。
pub fn evaluate(rules: &[Arc<RuleRuntime>], frame: &RxFrame, collect_trace: bool) -> RuleOutcome {
    // 残帧（被静默强刷或 EOF 冲刷出来的半包）不参与匹配，直接当作未命中。
    // 半截数据同样可能"碰巧"命中条件，从而回出一条与真实请求不匹配的应答；
    // 这里只关掉匹配这一件事 —— 帧本身照常进展示明细。
    if frame_is_partial(frame) {
        return RuleOutcome::miss();
    }

    let mut trace = MatchTrace::default();
    let rx = RxContext::new(frame);

    for (index, runtime) in rules.iter().enumerate() {
        let rule = &runtime.rule;
        let mut predicates = Vec::new();
        let hit = if collect_trace {
            match_node(
                &rule.matcher,
                &rx,
                runtime,
                &mut Some(&mut predicates),
                true,
            )
        } else {
            match_node(&rule.matcher, &rx, runtime, &mut None, false)
        };

        if collect_trace {
            trace.entries.push(TraceEntry {
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                hit,
                predicates,
            });
        }

        if hit {
            // 命中即停是硬语义：不存在"命中后又命中兜底规则"的状态
            return RuleOutcome {
                rule_id: Some(rule.id.clone()),
                rule_index: Some(index),
                // P-2：直接取运行期实例在 `RuleRuntime::new` 时构建好的预编译载荷
                compiled: Some(runtime.compiled_payload.clone()),
                trace,
            };
        }
    }

    let mut outcome = RuleOutcome::miss();
    outcome.trace = trace;
    outcome
}

/// 递归求值一个条件节点（深度优先 + 短路）
///
/// `trace` 为 `Some` 时收集逐项轨迹，同时 `record` 决定是否为最外层（最外层才记录 label）。
fn match_node(
    node: &MatchNode,
    rx: &RxContext<'_>,
    runtime: &RuleRuntime,
    trace: &mut Option<&mut Vec<PredicateTrace>>,
    record: bool,
) -> bool {
    // 组合节点自身不产生轨迹项，只有原子谓词产生（避免轨迹里塞满 "all/any" 噪音）
    match node {
        MatchNode::All { children } => {
            let mut all_pass = true;
            for (i, child) in children.iter().enumerate() {
                let pass = match_node(child, rx, runtime, trace, record);
                if !pass {
                    all_pass = false;
                    // 短路：剩余子项标记为 skipped（调试面板要能看出"因为前项已失败，
                    // 后面这些根本没跑"，否则用户会误以为后面的条件也失败了）
                    if let Some(items) = trace.as_deref_mut() {
                        for skipped in &children[i + 1..] {
                            mark_skipped(skipped, items);
                        }
                    }
                    break;
                }
            }
            all_pass
        }
        MatchNode::Any { children } => {
            let mut any_pass = false;
            for (i, child) in children.iter().enumerate() {
                let pass = match_node(child, rx, runtime, trace, record);
                if pass {
                    any_pass = true;
                    if let Some(items) = trace.as_deref_mut() {
                        for skipped in &children[i + 1..] {
                            mark_skipped(skipped, items);
                        }
                    }
                    break;
                }
            }
            any_pass
        }
        MatchNode::Not { child } => {
            let inner = match_node(child, rx, runtime, trace, record);
            if record {
                if let Some(items) = trace.as_deref_mut() {
                    items.push(PredicateTrace::new(
                        format!("非({})", node_kind_label(child)),
                        if inner {
                            PredicateResult::Fail
                        } else {
                            PredicateResult::Pass
                        },
                        if inner {
                            "子条件成立"
                        } else {
                            "子条件不成立"
                        }
                        .to_string(),
                    ));
                }
            }
            !inner
        }
        atomic => {
            let (pass, actual) = eval_atomic(atomic, rx, runtime, record);
            if record {
                if let Some(items) = trace.as_deref_mut() {
                    items.push(PredicateTrace::new(
                        atomic_label(atomic),
                        if pass {
                            PredicateResult::Pass
                        } else {
                            PredicateResult::Fail
                        },
                        actual,
                    ));
                }
            }
            pass
        }
    }
}

/// 把被短路的子树整体标记为 skipped（保持轨迹结构完整，便于 UI 渲染整棵树）
fn mark_skipped(node: &MatchNode, items: &mut Vec<PredicateTrace>) {
    match node {
        MatchNode::All { children } | MatchNode::Any { children } => {
            items.push(PredicateTrace::new(
                node_kind_label(node),
                PredicateResult::Skipped,
                "未求值".to_string(),
            ));
            for child in children {
                mark_skipped(child, items);
            }
        }
        MatchNode::Not { child } => {
            items.push(PredicateTrace::new(
                node_kind_label(node),
                PredicateResult::Skipped,
                "未求值".to_string(),
            ));
            mark_skipped(child, items);
        }
        atomic => items.push(PredicateTrace::new(
            atomic_label(atomic),
            PredicateResult::Skipped,
            "未求值".to_string(),
        )),
    }
}

/// 原子谓词求值 → (是否通过, 实际值的可读描述)
///
/// `record == false`（网络热路径，不收集轨迹）时**不构造任何描述字符串**（P-1）：
/// 描述只供调试面板展示，洪泛下每帧为每个原子谓词 `format!` 是主要堆分配来源，
/// 与模块头注释「热路径必须避免每帧构造大量 String」直接冲突。
fn eval_atomic(
    node: &MatchNode,
    rx: &RxContext<'_>,
    runtime: &RuleRuntime,
    record: bool,
) -> (bool, String) {
    let frame = rx.bytes();

    // 仅当需要轨迹时才构造描述串；否则返回空串（零堆分配）。
    macro_rules! actual {
        ($($arg:tt)*) => {
            if record { format!($($arg)*) } else { String::new() }
        };
    }

    match node {
        MatchNode::Contains { bytes } => {
            let Ok(pattern) = runtime.pattern(bytes) else {
                return (false, actual!("字节模式非法"));
            };
            // 空模式恒不成立：否则"匹配一切"是个隐式陷阱，应显式用 Length{0,MAX}
            if pattern.is_empty() {
                return (false, actual!("模式为空 → 恒不成立"));
            }
            let found = contains_subslice(frame, pattern);
            (found, actual!("帧 {} 字节", frame.len()))
        }
        MatchNode::FixedExact { len, bytes } => {
            let Ok(pattern) = runtime.pattern(bytes) else {
                return (false, actual!("字节模式非法"));
            };
            if pattern.len() != *len {
                return (false, actual!("模式 {} 字节 ≠ 声明 {}", pattern.len(), len));
            }
            let pass = frame.len() == *len && frame == pattern;
            (pass, actual!("帧 {} 字节", frame.len()))
        }
        MatchNode::FixedMask { len, bytes, mask } => {
            let Ok(pattern) = runtime.pattern(bytes) else {
                return (false, actual!("字节模式非法"));
            };
            if pattern.len() != *len || mask.len() != *len {
                return (
                    false,
                    actual!(
                        "长度不一致: 帧声明 {} / 模式 {} / 掩码 {}",
                        len,
                        pattern.len(),
                        mask.len()
                    ),
                );
            }
            if frame.len() != *len {
                return (false, actual!("帧 {} 字节 ≠ {}", frame.len(), len));
            }
            let pass = frame
                .iter()
                .zip(pattern.iter())
                .zip(mask.iter())
                .all(|((&a, &b), &m)| (a & m) == (b & m));
            (
                pass,
                actual!("{}", crate::reply::frame::format_hex(frame, true)),
            )
        }
        MatchNode::PrefixRange {
            prefix,
            min_len,
            max_len,
            constraints,
        } => {
            let Ok(prefix_bytes) = runtime.pattern(prefix) else {
                return (false, actual!("前缀非法"));
            };
            if min_len > max_len {
                return (false, actual!("区间非法 {}..{}", min_len, max_len));
            }
            if frame.len() < *min_len || frame.len() > *max_len {
                return (
                    false,
                    actual!("帧 {} 字节, 期望 {}..{}", frame.len(), min_len, max_len),
                );
            }
            if prefix_bytes.len() > frame.len() || !frame.starts_with(prefix_bytes) {
                return (
                    false,
                    actual!("{}", crate::reply::frame::format_hex(frame, true)),
                );
            }
            for (i, c) in constraints.iter().enumerate() {
                if let Err(reason) = check_constraint(c, frame, record) {
                    return (false, actual!("约束 {} 失败: {}", i + 1, reason));
                }
            }
            (
                true,
                actual!(
                    "{} 字节, {}",
                    frame.len(),
                    crate::reply::frame::format_hex(&frame[..frame.len().min(8)], true)
                ),
            )
        }
        MatchNode::Length { min, max } => (
            *min <= frame.len() && frame.len() <= *max,
            actual!("帧 {} 字节", frame.len()),
        ),
        MatchNode::ByteAt { offset, op } => match frame.get(*offset) {
            Some(&b) => (test_byte_op(op, b), actual!("[{:02X}] {:02X}", offset, b)),
            None => (
                false,
                actual!("无第 {} 字节（帧 {} 字节）", offset, frame.len()),
            ),
        },
        MatchNode::ScalarAt {
            offset,
            width,
            endian,
            signed,
            cmp,
            value,
        } => {
            // 用同一套取值语义，保证匹配器与 ${rx.*} 对同一帧的理解完全一致
            let accessor = scalar_accessor(*width, *endian, *signed);
            match rx.get_scalar(&accessor, *offset) {
                Ok(value_actual) => (
                    cmp.test(value_actual, *value),
                    actual!("{} {}", value_actual, cmp.symbol()),
                ),
                Err(e) => (false, if record { e.describe() } else { String::new() }),
            }
        }
        MatchNode::Suffix { bytes } => {
            let Ok(pattern) = runtime.pattern(bytes) else {
                return (false, actual!("字节模式非法"));
            };
            if pattern.is_empty() || frame.len() < pattern.len() {
                return (
                    false,
                    actual!("帧 {} 字节, 模式 {} 字节", frame.len(), pattern.len()),
                );
            }
            let pass = frame.ends_with(pattern);
            (
                pass,
                actual!(
                    "{}",
                    crate::reply::frame::format_hex(&frame[frame.len() - pattern.len()..], true)
                ),
            )
        }
        MatchNode::Regex { .. } => {
            let Some(re) = runtime.regex() else {
                return (false, actual!("正则未编译"));
            };
            // P-8①：长度检查前置 —— 超大帧不必先做一次全量 UTF-8 校验再被丢弃
            if frame.len() > REGEX_MAX_FRAME_LEN {
                warn!(
                    "[reply] 帧长 {} 超过正则匹配上限 {}, 跳过正则谓词",
                    frame.len(),
                    REGEX_MAX_FRAME_LEN
                );
                return (false, actual!("帧过长({} 字节), 跳过", frame.len()));
            }
            match std::str::from_utf8(frame) {
                Ok(text) => (re.is_match(text), actual!("{}", preview_text(text))),
                Err(_) => (false, actual!("帧不是合法 UTF-8，跳过")),
            }
        }
        MatchNode::From { .. } => {
            // P-5：地址规格已在 `RuleRuntime` 预解析，这里不再每帧解析 IP/CIDR 字符串
            let ip = rx.source_addr().ip();
            let pass = runtime
                .addr_specs()
                .iter()
                .any(|spec| addr_spec_matches(spec, ip));
            if record {
                (pass, rx.source())
            } else {
                (pass, String::new())
            }
        }
        MatchNode::FieldEq {
            accessor,
            offset,
            cmp,
            value,
        } => match rx.get_scalar(accessor, *offset) {
            Ok(value_actual) => (cmp.test(value_actual, *value), actual!("{}", value_actual)),
            Err(e) => (false, if record { e.describe() } else { String::new() }),
        },
        MatchNode::ChecksumValid {
            algorithm,
            at,
            range,
        } => {
            let Some(at_offset) = at.resolve_offset(frame.len()) else {
                return (false, actual!("帧 {} 字节放不下校验位", frame.len()));
            };
            let width = at.width().bytes();
            let (start, len) = match range {
                Some((s, l)) => (*s, *l),
                // 未指定覆盖区间：默认覆盖校验位之前的全部字节（最常见的约定）
                None => (0, at_offset),
            };
            let computed = match rx.checksum(*algorithm, start, len, false) {
                Ok(v) => v,
                Err(e) => return (false, if record { e.describe() } else { String::new() }),
            };
            let actual_bytes = &frame[at_offset..at_offset + width];
            // 期望值可能按大端或小端放置 —— 两种都接受，因为 Modbus RTU 用小端而
            // 多数 CRC16 变体用大端，用户不该为此在匹配器里再做一次字节序开关
            // （用迭代器反转比较，避免为"小端"再克隆一份校验值）
            let pass = actual_bytes == computed.as_slice()
                || computed.iter().rev().eq(actual_bytes.iter());
            (
                pass,
                actual!(
                    "{} (期望 {})",
                    crate::reply::frame::format_hex(actual_bytes, true),
                    crate::reply::frame::format_hex(&computed, true)
                ),
            )
        }
        // 组合节点不会走到这里（match_node 已分派）
        MatchNode::All { .. } | MatchNode::Any { .. } | MatchNode::Not { .. } => {
            (false, actual!("组合节点不应作为原子谓词求值"))
        }
    }
}

/// 正则参与匹配的帧长上限（超过则跳过并告警，避免超大帧拖慢热路径）
pub const REGEX_MAX_FRAME_LEN: usize = 64 * 1024;

/// 由宽度/字节序/符号拼出取值标识（与 `${rx.*}` 共用同一套解析）
fn scalar_accessor(
    width: crate::reply::model::Width,
    endian: crate::reply::model::Endian,
    signed: bool,
) -> String {
    let sign = if signed { "i" } else { "u" };
    let bits = width.bytes() * 8;
    if bits == 8 {
        format!("{}{}", sign, bits)
    } else {
        let e = if endian.is_little() { "le" } else { "be" };
        format!("{}{}{}", sign, bits, e)
    }
}

fn test_byte_op(op: &ByteOp, b: u8) -> bool {
    match op {
        ByteOp::Eq { value } => b == *value,
        ByteOp::Ne { value } => b != *value,
        ByteOp::Masked { mask, value } => (b & *mask) == *value,
        ByteOp::In { values } => values.contains(&b),
    }
}

/// 检查一条位置约束；Err 携带失败原因（供轨迹展示）。
///
/// `record == false`（热路径）时失败原因为空串，不做 `format!` —— 与
/// [`eval_atomic`] 的 P-1 约定一致。
fn check_constraint(c: &Constraint, frame: &[u8], record: bool) -> Result<(), String> {
    macro_rules! reason {
        ($($arg:tt)*) => {
            if record { format!($($arg)*) } else { String::new() }
        };
    }
    match c {
        Constraint::ByteEq { offset, value } => match frame.get(*offset) {
            Some(b) if b == value => Ok(()),
            Some(b) => Err(reason!("[{}] = {:02X}, 期望 {:02X}", offset, b, value)),
            None => Err(reason!("无第 {} 字节", offset)),
        },
        Constraint::ByteMaskNonZero { offset, mask } => match frame.get(*offset) {
            Some(b) if b & mask != 0 => Ok(()),
            Some(b) => Err(reason!("[{}] = {:02X}, & {:02X} = 0", offset, b, mask)),
            None => Err(reason!("无第 {} 字节", offset)),
        },
        Constraint::ByteIn { offset, values } => match frame.get(*offset) {
            Some(b) if values.contains(b) => Ok(()),
            Some(b) => Err(reason!("[{}] = {:02X} 不在集合内", offset, b)),
            None => Err(reason!("无第 {} 字节", offset)),
        },
        Constraint::LengthFromByte {
            len_offset,
            width,
            endian,
            base,
        } => {
            let w = width.bytes();
            let Some(slice) = frame.get(*len_offset..len_offset + w) else {
                return Err(reason!("长度位越界（帧 {} 字节）", frame.len()));
            };
            let mut value: i64 = 0;
            if endian.is_little() {
                for (i, &b) in slice.iter().enumerate() {
                    value |= (b as i64) << (8 * i);
                }
            } else {
                for &b in slice {
                    value = (value << 8) | b as i64;
                }
            }
            let expected = base + value;
            if expected == frame.len() as i64 {
                Ok(())
            } else {
                Err(reason!("长度位给出 {} 而帧长 {}", expected, frame.len()))
            }
        }
    }
}

/// 朴素子串搜索（帧通常 < 8KB，无需 KMP）
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// 原子谓词的人可读标签（含参数值 —— 调试面板需要看到"期望什么"）
pub fn atomic_label(node: &MatchNode) -> String {
    match node {
        MatchNode::All { .. } => "全部满足".to_string(),
        MatchNode::Any { .. } => "任一满足".to_string(),
        MatchNode::Not { .. } => "非".to_string(),
        MatchNode::Contains { bytes } => format!("包含 {}", bytes.display()),
        MatchNode::FixedExact { len, bytes } => {
            format!("定长精确 {} 字节 = {}", len, bytes.display())
        }
        MatchNode::FixedMask { len, bytes, mask } => format!(
            "定长模糊 {} 字节 ≈ {} 掩码 {}",
            len,
            bytes.display(),
            crate::reply::frame::format_hex(mask, true)
        ),
        MatchNode::PrefixRange {
            prefix,
            min_len,
            max_len,
            constraints,
        } => format!(
            "前缀 {} 长度 {}..{} 约束 {}",
            prefix.display(),
            min_len,
            max_len,
            constraints.len()
        ),
        MatchNode::Length { min, max } => format!("长度 {}..{}", min, max),
        MatchNode::ByteAt { offset, op } => format!("字节[{}] {}", offset, op.summary()),
        MatchNode::ScalarAt {
            offset,
            width,
            endian,
            signed,
            cmp,
            value,
        } => format!(
            "{}({}) {} {}",
            scalar_accessor(*width, *endian, *signed),
            offset,
            cmp.symbol(),
            value
        ),
        MatchNode::Suffix { bytes } => format!("帧尾 {}", bytes.display()),
        MatchNode::Regex { pattern } => format!("正则 /{}/", pattern),
        MatchNode::From { addrs } => format!("来源 ∈ [{}]", addrs.join(", ")),
        MatchNode::FieldEq {
            accessor,
            offset,
            cmp,
            value,
        } => format!("{}({}) {} {}", accessor, offset, cmp.symbol(), value),
        MatchNode::ChecksumValid { algorithm, at, .. } => {
            format!("{} 校验有效（{}）", algorithm.name(), at.summary())
        }
    }
}

fn node_kind_label(node: &MatchNode) -> String {
    match node {
        MatchNode::All { .. } => "全部满足".to_string(),
        MatchNode::Any { .. } => "任一满足".to_string(),
        MatchNode::Not { .. } => "非".to_string(),
        other => atomic_label(other),
    }
}

/// 文本预览（正则匹配轨迹里展示帧内容，截断避免撑爆列表）
fn preview_text(text: &str) -> String {
    let escaped: String = text
        .chars()
        .take(32)
        .map(|c| if c.is_control() { '.' } else { c })
        .collect();
    if text.chars().count() > 32 {
        format!("{}…", escaped)
    } else {
        escaped
    }
}

// ============================================================================
// 来源地址匹配（F-14：白名单过滤）
// ============================================================================

/// 解析一条地址规格：`192.168.1.7` / `192.168.1.0/24` / `::1` / `fe80::/10`
pub fn parse_addr_spec(spec: &str) -> Option<AddrSpec> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    match spec.split_once('/') {
        Some((addr, prefix)) => {
            let ip: IpAddr = addr.trim().parse().ok()?;
            let prefix: u8 = prefix.trim().parse().ok()?;
            let max = if ip.is_ipv4() { 32 } else { 128 };
            if prefix > max {
                return None;
            }
            Some(AddrSpec::Cidr { ip, prefix })
        }
        None => spec.parse::<IpAddr>().ok().map(AddrSpec::Exact),
    }
}

/// 单条来源地址规格
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddrSpec {
    Exact(IpAddr),
    Cidr { ip: IpAddr, prefix: u8 },
}

/// 单条**已解析**规格是否命中来源 IP（CIDR 按位比较，IPv4/IPv6 族不同直接不匹配）
pub fn addr_spec_matches(spec: &AddrSpec, ip: IpAddr) -> bool {
    match spec {
        AddrSpec::Exact(target) => *target == ip,
        AddrSpec::Cidr { ip: net, prefix } => match (*net, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = if *prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - *prefix)
                };
                (u32::from(net) & mask) == (u32::from(ip) & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = if *prefix == 0 {
                    0u128
                } else {
                    u128::MAX << (128 - *prefix)
                };
                (u128::from(net) & mask) == (u128::from(ip) & mask)
            }
            _ => false,
        },
    }
}

/// 来源 IP 是否命中规格字符串（解析 + 匹配；单测用）。
///
/// 热路径（`From` 谓词）不走这里 —— 它用 `RuleRuntime` 预解析好的 [`AddrSpec`]，
/// 避免每帧重复解析 CIDR/IP 字符串。
#[cfg(test)]
fn addr_matches(spec: &str, ip: IpAddr) -> bool {
    parse_addr_spec(spec)
        .map(|s| addr_spec_matches(&s, ip))
        .unwrap_or(false)
}

// ============================================================================
// 残帧判定
// ============================================================================

/// 是否为"残帧"：分帧解码器在**静默强刷**或 **EOF 冲刷**时落地的不完整帧。
///
/// 残帧不参与规则匹配（见 [`evaluate`]），因为半截数据命中条件后回出的应答
/// 与对端真实请求并不对应。帧本身仍照常进展示明细，只是不触发规则。
pub fn frame_is_partial(frame: &RxFrame) -> bool {
    frame.meta.origin.is_partial()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::checksum::ChecksumAlgorithm;
    use crate::reply::frame::FrameOrigin;
    use crate::reply::model::{
        BytePattern, ChecksumAt, Cmp, Endian, ReplyPayload, ReplyRule, ReplyRulesConfig, RuleCodec,
        Width,
    };
    use crate::reply::store::ReplyRulesStore;
    use std::sync::Arc;

    /// 构造一个只含单条规则的运行期表（默认空回复，仅用于验证匹配）
    fn one_rule(node: MatchNode) -> Vec<Arc<RuleRuntime>> {
        one_rule_with_action(node, ReplyPayload::default())
    }

    fn one_rule_with_action(node: MatchNode, payload: ReplyPayload) -> Vec<Arc<RuleRuntime>> {
        let rule = ReplyRule {
            matcher: node,
            payload,
            ..ReplyRule::new("测试", 10)
        };
        let store = ReplyRulesStore::new();
        store.replace(&rules_config(vec![rule]));
        store.enabled_rules().as_ref().clone()
    }

    /// 构造某连接下的规则集（按连接分组存储）；测试统一挂到 "tab"
    fn rules_config(rules: Vec<ReplyRule>) -> ReplyRulesConfig {
        let mut cfg = ReplyRulesConfig::default();
        if !rules.is_empty() {
            cfg.connections.insert("tab".to_string(), rules);
        }
        cfg
    }

    fn frame(bytes: &[u8]) -> Arc<RxFrame> {
        RxFrame::for_test(bytes.to_vec())
    }

    fn hit(rules: &[Arc<RuleRuntime>], bytes: &[u8]) -> bool {
        evaluate(rules, &frame(bytes), false).is_hit()
    }

    // ===== Contains =====
    #[test]
    fn test_contains() {
        let rules = one_rule(MatchNode::Contains {
            bytes: BytePattern::Hex("01 03".to_string()),
        });
        assert!(hit(&rules, &[0xAA, 0x01, 0x03, 0xBB]));
        assert!(hit(&rules, &[0x01, 0x03]));
        assert!(!hit(&rules, &[0x01, 0x04]));
        assert!(!hit(&rules, &[]));
        // 空模式恒不成立（否则是"匹配一切"的隐式陷阱）
        let empty = one_rule(MatchNode::Contains {
            bytes: BytePattern::Hex("".to_string()),
        });
        assert!(!hit(&empty, &[1, 2, 3]));
    }

    // ===== FixedExact =====
    #[test]
    fn test_fixed_exact() {
        let rules = one_rule(MatchNode::FixedExact {
            len: 2,
            bytes: BytePattern::Hex("01 03".to_string()),
        });
        assert!(hit(&rules, &[0x01, 0x03]));
        assert!(!hit(&rules, &[0x01, 0x03, 0x00]), "帧更长应不匹配");
        assert!(!hit(&rules, &[0x01]), "帧更短应不匹配");
        assert!(!hit(&rules, &[0x01, 0x04]), "内容不同应不匹配");
    }

    // ===== FixedMask =====
    #[test]
    fn test_fixed_mask() {
        // 第 2 字节用掩码 0x00 忽略
        let rules = one_rule(MatchNode::FixedMask {
            len: 3,
            bytes: BytePattern::Hex("01 03 00".to_string()),
            mask: vec![0xFF, 0x00, 0xFF],
        });
        assert!(hit(&rules, &[0x01, 0xAA, 0x00]));
        assert!(
            !hit(&rules, &[0x02, 0xAA, 0x00]),
            "被掩码保护的字节必须相等"
        );
        assert!(!hit(&rules, &[0x01, 0xAA]), "长度不等");
        // 掩码长度与 len 不一致 → 保守返回 false，不 panic
        let bad = one_rule(MatchNode::FixedMask {
            len: 3,
            bytes: BytePattern::Hex("01 03 00".to_string()),
            mask: vec![0xFF],
        });
        assert!(!hit(&bad, &[0x01, 0x03, 0x00]));
    }

    // ===== PrefixRange =====
    #[test]
    fn test_prefix_range() {
        let rules = one_rule(MatchNode::PrefixRange {
            prefix: BytePattern::Hex("01 03".to_string()),
            min_len: 8,
            max_len: 8,
            constraints: vec![Constraint::ByteEq {
                offset: 5,
                value: 2,
            }],
        });
        assert!(hit(
            &rules,
            &[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B]
        ));
        assert!(!hit(
            &rules,
            &[0x01, 0x03, 0x00, 0x00, 0x00, 0x03, 0xC4, 0x0B]
        ));
        assert!(!hit(
            &rules,
            &[0x01, 0x04, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B]
        ));
        assert!(!hit(&rules, &[0x01, 0x03, 0x00]), "长度不在区间");
        // 无约束时只校验前缀与长度
        let loose = one_rule(MatchNode::PrefixRange {
            prefix: BytePattern::Hex("01 03".to_string()),
            min_len: 8,
            max_len: 8,
            constraints: Vec::new(),
        });
        assert!(hit(
            &loose,
            &[0x01, 0x03, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00]
        ));
    }

    /// LengthFromByte：可变长协议的核心表达（长度位决定帧长）
    #[test]
    fn test_length_from_byte_constraint() {
        // 帧长 = 2 + bytes[2]（帧头 2 字节 + 长度位 1 字节）
        let rules = one_rule(MatchNode::PrefixRange {
            prefix: BytePattern::Hex("AA 55".to_string()),
            min_len: 3,
            max_len: 20,
            constraints: vec![Constraint::LengthFromByte {
                len_offset: 2,
                width: Width::U8,
                endian: Endian::Big,
                base: 3,
            }],
        });
        // 长度位 = 2 → 帧长应为 5
        assert!(hit(&rules, &[0xAA, 0x55, 0x02, 0x11, 0x22]));
        assert!(!hit(&rules, &[0xAA, 0x55, 0x03, 0x11, 0x22]));
        // 长度位越界
        assert!(!hit(&rules, &[0xAA, 0x55]));
    }

    /// 约束集合：ByteMaskNonZero / ByteIn
    #[test]
    fn test_other_constraints() {
        let mask_non_zero = one_rule(MatchNode::PrefixRange {
            prefix: BytePattern::Hex("".to_string()),
            min_len: 1,
            max_len: 4,
            constraints: vec![Constraint::ByteMaskNonZero {
                offset: 0,
                mask: 0x80,
            }],
        });
        assert!(hit(&mask_non_zero, &[0x80]));
        assert!(!hit(&mask_non_zero, &[0x7F]));

        let byte_in = one_rule(MatchNode::PrefixRange {
            prefix: BytePattern::Hex("".to_string()),
            min_len: 1,
            max_len: 4,
            constraints: vec![Constraint::ByteIn {
                offset: 0,
                values: vec![0x03, 0x04, 0x06],
            }],
        });
        assert!(hit(&byte_in, &[0x03]));
        assert!(!hit(&byte_in, &[0x05]));
    }

    // ===== Length =====
    #[test]
    fn test_length() {
        let rules = one_rule(MatchNode::Length { min: 2, max: 4 });
        assert!(!hit(&rules, &[1]));
        assert!(hit(&rules, &[1, 2]));
        assert!(hit(&rules, &[1, 2, 3, 4]));
        assert!(!hit(&rules, &[1, 2, 3, 4, 5]));
        // 显式的"空帧规则"可以命中（与"空模式恒不成立"对比）
        let empty_ok = one_rule(MatchNode::Length { min: 0, max: 0 });
        assert!(hit(&empty_ok, &[]));
    }

    // ===== ByteAt =====
    #[test]
    fn test_byte_at() {
        let eq = one_rule(MatchNode::ByteAt {
            offset: 1,
            op: ByteOp::Eq { value: 3 },
        });
        assert!(hit(&eq, &[0x01, 0x03]));
        assert!(!hit(&eq, &[0x01, 0x04]));
        // 越界 → false，不 panic
        assert!(!hit(&eq, &[0x01]));

        let ne = one_rule(MatchNode::ByteAt {
            offset: 0,
            op: ByteOp::Ne { value: 3 },
        });
        assert!(hit(&ne, &[0x01]));
        assert!(!hit(&ne, &[0x03]));

        let masked = one_rule(MatchNode::ByteAt {
            offset: 0,
            op: ByteOp::Masked {
                mask: 0xF0,
                value: 0x30,
            },
        });
        assert!(hit(&masked, &[0x35]));
        assert!(!hit(&masked, &[0x45]));

        let inside = one_rule(MatchNode::ByteAt {
            offset: 1,
            op: ByteOp::In {
                values: vec![1, 3, 6],
            },
        });
        assert!(hit(&inside, &[0x00, 0x06]));
        assert!(!hit(&inside, &[0x00, 0x02]));
    }

    // ===== ScalarAt =====
    #[test]
    fn test_scalar_at() {
        let be = one_rule(MatchNode::ScalarAt {
            offset: 2,
            width: Width::U16,
            endian: Endian::Big,
            signed: false,
            cmp: Cmp::Eq,
            value: 2,
        });
        assert!(hit(&be, &[0x01, 0x03, 0x00, 0x02]));
        assert!(!hit(&be, &[0x01, 0x03, 0x00, 0x03]));

        let le = one_rule(MatchNode::ScalarAt {
            offset: 2,
            width: Width::U16,
            endian: Endian::Little,
            signed: false,
            cmp: Cmp::Ge,
            value: 0x0100,
        });
        assert!(hit(&le, &[0x01, 0x03, 0x00, 0x01]));

        let signed = one_rule(MatchNode::ScalarAt {
            offset: 0,
            width: Width::U8,
            endian: Endian::Big,
            signed: true,
            cmp: Cmp::Lt,
            value: 0,
        });
        assert!(hit(&signed, &[0xFF]));
        assert!(!hit(&signed, &[0x01]));

        // 越界 → false
        assert!(!hit(&be, &[0x01, 0x03, 0x00]));
    }

    // ===== Suffix =====
    #[test]
    fn test_suffix() {
        let rules = one_rule(MatchNode::Suffix {
            bytes: BytePattern::Hex("0D 0A".to_string()),
        });
        assert!(hit(&rules, b"AT\r\n"));
        assert!(!hit(&rules, b"AT\n"));
        assert!(!hit(&rules, b"\r"), "帧短于模式");
        let empty = one_rule(MatchNode::Suffix {
            bytes: BytePattern::Hex("".to_string()),
        });
        assert!(!hit(&empty, b"abc"));
    }

    // ===== Regex =====
    #[test]
    fn test_regex() {
        let rules = one_rule(MatchNode::Regex {
            pattern: "^AT\\+CSQ".to_string(),
        });
        assert!(hit(&rules, b"AT+CSQ"));
        assert!(!hit(&rules, b"AT+CGMR"));
        assert!(!hit(&rules, b"OK\r\nAT+CSQ"));
        // 非 UTF-8 帧不参与正则匹配（不 panic）
        assert!(!hit(&rules, &[0xFF, 0xFE, 0x00]));
    }

    /// 非法正则：编译失败后恒 false（缓存 None，不每帧重试编译）
    #[test]
    fn test_invalid_regex_never_matches() {
        let rules = one_rule(MatchNode::Regex {
            pattern: "([".to_string(),
        });
        assert!(!hit(&rules, b"anything"));
        assert!(!hit(&rules, b"anything"));
    }

    // ===== From =====
    #[test]
    fn test_from_matching() {
        let exact = one_rule(MatchNode::From {
            addrs: vec!["127.0.0.1".to_string()],
        });
        assert!(
            hit(&exact, b"x"),
            "RxFrame::for_test 的来源是 127.0.0.1:12345"
        );

        let other = one_rule(MatchNode::From {
            addrs: vec!["10.0.0.1".to_string()],
        });
        assert!(!hit(&other, b"x"));

        // CIDR 网段
        let cidr = one_rule(MatchNode::From {
            addrs: vec!["127.0.0.0/8".to_string()],
        });
        assert!(hit(&cidr, b"x"));
        let narrow = one_rule(MatchNode::From {
            addrs: vec!["127.0.1.0/24".to_string()],
        });
        assert!(!hit(&narrow, b"x"));

        // 非法规格不匹配（但也不 panic）
        let bad = one_rule(MatchNode::From {
            addrs: vec!["nonsense".to_string()],
        });
        assert!(!hit(&bad, b"x"));
        // CIDR 前缀越界
        assert!(parse_addr_spec("127.0.0.0/33").is_none());
        assert!(parse_addr_spec("127.0.0.0/8").is_some());
        assert!(parse_addr_spec("::1").is_some());
        assert!(parse_addr_spec("fe80::/10").is_some());
        assert!(parse_addr_spec("").is_none());
    }

    /// 地址匹配的族隔离：IPv4 规格不匹配 IPv6 来源
    #[test]
    fn test_addr_family_isolation() {
        assert!(!addr_matches("10.0.0.0/8", "::1".parse().unwrap()));
        assert!(!addr_matches("::1", "127.0.0.1".parse().unwrap()));
        assert!(addr_matches("::1", "::1".parse().unwrap()));
        assert!(addr_matches("0.0.0.0/0", "1.2.3.4".parse().unwrap()));
        assert!(addr_matches("::/0", "::1".parse().unwrap()));
    }

    // ===== FieldEq =====
    #[test]
    fn test_field_eq() {
        let rules = one_rule(MatchNode::FieldEq {
            accessor: "u8".to_string(),
            offset: 1,
            cmp: Cmp::Eq,
            value: 3,
        });
        assert!(hit(&rules, &[0x01, 0x03]));
        assert!(!hit(&rules, &[0x01, 0x04]));
        assert!(!hit(&rules, &[0x01]), "越界 → false");
    }

    // ===== ChecksumValid =====
    #[test]
    fn test_checksum_valid_modbus() {
        // 完整 Modbus RTU 帧：01 03 00 00 00 02 C4 0B（线上小端）
        let rules = one_rule(MatchNode::ChecksumValid {
            algorithm: ChecksumAlgorithm::Crc16Modbus,
            at: ChecksumAt::Trailing {
                n: 2,
                width: Width::U16,
            },
            range: Some((0, 6)),
        });
        assert!(hit(
            &rules,
            &[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B]
        ));
        // 大端放置也应接受（多数 CRC16 变体的习惯）
        assert!(hit(
            &rules,
            &[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0x0B, 0xC4]
        ));
        // 校验值错误
        assert!(!hit(
            &rules,
            &[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00]
        ));
        // 帧太短放不下校验位
        assert!(!hit(&rules, &[0x01, 0x03]));
    }

    /// 未指定覆盖区间时默认覆盖"校验位之前的全部字节"
    #[test]
    fn test_checksum_valid_default_range() {
        let rules = one_rule(MatchNode::ChecksumValid {
            algorithm: ChecksumAlgorithm::Xor,
            at: ChecksumAt::Fixed {
                offset: 3,
                width: Width::U8,
            },
            range: None,
        });
        // 01 ^ 03 ^ 00 = 02
        assert!(hit(&rules, &[0x01, 0x03, 0x00, 0x02]));
        assert!(!hit(&rules, &[0x01, 0x03, 0x00, 0x03]));
    }

    // ===== 组合与短路 =====
    #[test]
    fn test_all_requires_everything() {
        let rules = one_rule(MatchNode::All {
            children: vec![
                MatchNode::Length { min: 8, max: 8 },
                MatchNode::ByteAt {
                    offset: 0,
                    op: ByteOp::Eq { value: 1 },
                },
            ],
        });
        assert!(hit(&rules, &[0x01, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!hit(&rules, &[0x01, 0, 0, 0, 0, 0, 0]), "长度不符");
        assert!(!hit(&rules, &[0x02, 0, 0, 0, 0, 0, 0, 0]), "字节不符");
    }

    /// 空 `All{}` 恒真 = 匹配全部报文，这是"兜底回复"规则的实现方式
    #[test]
    fn test_empty_all_matches_everything() {
        let rules = one_rule(MatchNode::All {
            children: Vec::new(),
        });
        assert!(hit(&rules, &[]), "空帧也要命中");
        assert!(hit(&rules, &[0x01]));
        assert!(hit(&rules, &[0xFF; 64]));
    }

    #[test]
    fn test_any_requires_one() {
        let rules = one_rule(MatchNode::Any {
            children: vec![
                MatchNode::Length { min: 1, max: 1 },
                MatchNode::Length { min: 3, max: 3 },
            ],
        });
        assert!(hit(&rules, &[1]));
        assert!(hit(&rules, &[1, 2, 3]));
        assert!(!hit(&rules, &[1, 2]));
        // 空 Any 恒不成立（与空 All 相反）
        let empty = one_rule(MatchNode::Any {
            children: Vec::new(),
        });
        assert!(!hit(&empty, &[1]));
    }

    #[test]
    fn test_not() {
        let rules = one_rule(MatchNode::Not {
            child: Box::new(MatchNode::Length { min: 0, max: 0 }),
        });
        assert!(hit(&rules, &[1]));
        assert!(!hit(&rules, &[]));
    }

    /// 短路：All 的前项失败后，后续项必须标记 skipped 且不被求值
    #[test]
    fn test_short_circuit_marks_skipped() {
        let rules = one_rule(MatchNode::All {
            children: vec![
                // 第一项必然失败（帧长 1，要求 8）
                MatchNode::Length { min: 8, max: 8 },
                // 第二项若被求值会 panic 吗？不会，但轨迹里必须是 Skipped
                MatchNode::Contains {
                    bytes: BytePattern::Hex("FF".to_string()),
                },
            ],
        });
        let outcome = evaluate(&rules, &frame(&[0x01]), true);
        assert!(!outcome.is_hit());
        let entry = &outcome.trace.entries[0];
        assert_eq!(entry.predicates.len(), 2);
        assert_eq!(entry.predicates[0].result, PredicateResult::Fail);
        assert_eq!(entry.predicates[1].result, PredicateResult::Skipped);
        assert!(entry.first_failure().is_some());
    }

    /// Any 的命中短路同样标记 skipped
    #[test]
    fn test_any_short_circuit_marks_skipped() {
        let rules = one_rule(MatchNode::Any {
            children: vec![
                MatchNode::Length { min: 1, max: 1 },
                MatchNode::Length { min: 9, max: 9 },
            ],
        });
        let outcome = evaluate(&rules, &frame(&[0x01]), true);
        assert!(outcome.is_hit());
        let entry = outcome.trace.entries.iter().find(|e| e.hit).unwrap();
        assert_eq!(entry.predicates[0].result, PredicateResult::Pass);
        assert_eq!(entry.predicates[1].result, PredicateResult::Skipped);
    }

    /// collect_trace 开关只影响轨迹，绝不影响判定结果 —— 这是"预演 = 真实行为"的根基
    #[test]
    fn test_trace_flag_does_not_change_result() {
        let cases: Vec<(MatchNode, Vec<u8>)> = vec![
            (
                MatchNode::Contains {
                    bytes: BytePattern::Hex("01 03".to_string()),
                },
                vec![0x01, 0x03],
            ),
            (
                MatchNode::Regex {
                    pattern: "^AT".to_string(),
                },
                b"AT+CSQ".to_vec(),
            ),
            (
                MatchNode::ChecksumValid {
                    algorithm: ChecksumAlgorithm::Crc16Modbus,
                    at: ChecksumAt::Trailing {
                        n: 2,
                        width: Width::U16,
                    },
                    range: Some((0, 6)),
                },
                vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B],
            ),
            (
                MatchNode::From {
                    addrs: vec!["127.0.0.1".to_string()],
                },
                vec![1],
            ),
        ];
        for (node, bytes) in cases {
            let rules = one_rule(node);
            let f = frame(&bytes);
            let without = evaluate(&rules, &f, false);
            let with = evaluate(&rules, &f, true);
            assert_eq!(without.is_hit(), with.is_hit(), "轨迹开关不得改变判定结果");
            assert_eq!(without.rule_id, with.rule_id);
        }
    }

    // ===== 顺序与命中即停 =====
    /// 规则按 priority 升序求值，命中即停；命中后不得继续求值后续规则
    #[test]
    fn test_priority_order_and_stop_on_first_hit() {
        let store = ReplyRulesStore::new();
        store.replace(&rules_config(vec![
            ReplyRule {
                name: "兜底".to_string(),
                priority: 9000,
                matcher: MatchNode::Length { min: 1, max: 65535 },
                payload: ReplyPayload {
                    text: "兜底".to_string(),
                    ..Default::default()
                },
                ..ReplyRule::new("兜底", 9000)
            },
            ReplyRule {
                name: "精确".to_string(),
                priority: 10,
                matcher: MatchNode::Contains {
                    bytes: BytePattern::Hex("01 03".to_string()),
                },
                payload: ReplyPayload {
                    text: "精确".to_string(),
                    ..Default::default()
                },
                ..ReplyRule::new("精确", 10)
            },
        ]));
        let rules = store.enabled_rules();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].rule.name, "精确", "priority 小者先求值");

        let outcome = evaluate(&rules, &frame(&[0x01, 0x03]), true);
        assert_eq!(outcome.trace.entries.len(), 1, "命中即停，后续规则不进轨迹");
        assert!(outcome.is_hit());
    }

    /// 相同 priority 时按 id 字典序稳定排序（保证求值顺序可重复）
    #[test]
    fn test_same_priority_stable_by_id() {
        let mut a = ReplyRule::new("b", 10);
        a.id = "bbb".to_string();
        a.matcher = MatchNode::Length { min: 1, max: 1 };
        let mut b = ReplyRule::new("a", 10);
        b.id = "aaa".to_string();
        b.matcher = MatchNode::Length { min: 2, max: 2 };

        let store = ReplyRulesStore::new();
        store.replace(&rules_config(vec![a, b]));
        let rules = store.enabled_rules();
        assert_eq!(rules[0].rule.id, "aaa");
        assert_eq!(rules[1].rule.id, "bbb");
    }

    /// 空规则集：evaluate 不命中且不 panic
    #[test]
    fn test_empty_ruleset() {
        let outcome = evaluate(&[], &frame(&[1, 2, 3]), true);
        assert!(!outcome.is_hit());
        assert!(outcome.trace.entries.is_empty());
    }

    /// 越界访问绝不 panic：所有原子谓词在空帧/短帧上都要安全返回 false
    #[test]
    fn test_no_panic_on_short_frames() {
        let nodes = vec![
            MatchNode::Contains {
                bytes: BytePattern::Hex("FF FF FF FF".to_string()),
            },
            MatchNode::FixedExact {
                len: 8,
                bytes: BytePattern::Hex("00 00 00 00 00 00 00 00".to_string()),
            },
            MatchNode::FixedMask {
                len: 8,
                bytes: BytePattern::Hex("00 00 00 00 00 00 00 00".to_string()),
                mask: vec![0xFF; 8],
            },
            MatchNode::PrefixRange {
                prefix: BytePattern::Hex("AA".to_string()),
                min_len: 0,
                max_len: 100,
                constraints: vec![
                    Constraint::ByteEq {
                        offset: 50,
                        value: 1,
                    },
                    Constraint::LengthFromByte {
                        len_offset: 60,
                        width: Width::U32,
                        endian: Endian::Big,
                        base: 0,
                    },
                ],
            },
            MatchNode::ByteAt {
                offset: 99,
                op: ByteOp::Eq { value: 1 },
            },
            MatchNode::ScalarAt {
                offset: 99,
                width: Width::U64,
                endian: Endian::Little,
                signed: true,
                cmp: Cmp::Eq,
                value: 0,
            },
            MatchNode::Suffix {
                bytes: BytePattern::Hex("FF FF".to_string()),
            },
            MatchNode::FieldEq {
                accessor: "u32be".to_string(),
                offset: 99,
                cmp: Cmp::Eq,
                value: 0,
            },
            MatchNode::ChecksumValid {
                algorithm: ChecksumAlgorithm::Crc32,
                at: ChecksumAt::Fixed {
                    offset: 99,
                    width: Width::U32,
                },
                range: Some((0, 100)),
            },
        ];
        for node in nodes {
            for bytes in [vec![], vec![0x01], vec![0x01, 0x02, 0x03]] {
                let rules = one_rule(node.clone());
                // 不 panic 即通过；结果本身不重要
                let _ = evaluate(&rules, &frame(&bytes), true);
            }
        }
    }

    /// 命中后必须带出载荷与预编译载荷（exec 层据此发送）
    #[test]
    fn test_outcome_carries_action_and_compiled() {
        let rules = one_rule_with_action(
            MatchNode::Length { min: 1, max: 100 },
            ReplyPayload {
                text: "6F 6B".to_string(),
                hex_mode: true,
                codec: RuleCodec::Raw,
            },
        );
        let outcome = evaluate(&rules, &frame(&[1]), false);
        assert!(outcome.is_hit());
        assert!(outcome.rule_index.is_some());
        let compiled = outcome.compiled.expect("命中必须带出预编译载荷");
        assert_eq!(compiled.fixed.as_deref(), Some(&b"ok"[..]));
    }

    /// 原子谓词标签必须包含"期望值"，否则调试面板无法展示"期望 vs 实际"
    #[test]
    fn test_predicate_labels_contain_expectations() {
        assert!(atomic_label(&MatchNode::Length { min: 8, max: 8 }).contains('8'));
        assert!(
            atomic_label(&MatchNode::Contains {
                bytes: BytePattern::Hex("01 03".to_string())
            })
            .contains("0103"),
            "摘要用紧凑 hex 展示字节模式"
        );
        assert!(
            atomic_label(&MatchNode::ChecksumValid {
                algorithm: ChecksumAlgorithm::Crc16Modbus,
                at: ChecksumAt::Trailing {
                    n: 2,
                    width: Width::U16
                },
                range: None
            })
            .contains("crc16_modbus")
        );
        // 掩码状态也必须在标签里可见（固定长度模糊匹配的核心信息）
        assert!(
            atomic_label(&MatchNode::FixedMask {
                len: 2,
                bytes: BytePattern::Hex("01 03".to_string()),
                mask: vec![0xFF, 0x00]
            })
            .contains("FF 00")
        );
    }

    /// 帧来源标记（F-21 的语义基础）
    #[test]
    fn test_frame_is_partial() {
        assert!(!frame_is_partial(&frame(&[1])));
        let partial = RxFrame::new(
            vec![1],
            "127.0.0.1:1".parse().unwrap(),
            crate::reply::frame::FrameMeta {
                origin: FrameOrigin::ForceFlushed,
            },
        );
        assert!(frame_is_partial(&partial));
    }

    /// 残帧即便字节完全命中条件，也不得触发规则（半截数据"碰巧"命中会回错应答）
    #[test]
    fn test_partial_frame_never_hits() {
        let rules = one_rule(MatchNode::Contains {
            bytes: BytePattern::Hex("01 03".to_string()),
        });
        // 同样的字节，Decoded 命中
        assert!(hit(&rules, &[0x01, 0x03]));

        for origin in [FrameOrigin::ForceFlushed, FrameOrigin::EofFlushed] {
            let partial = RxFrame::new(
                vec![0x01, 0x03],
                "127.0.0.1:1".parse().unwrap(),
                crate::reply::frame::FrameMeta { origin },
            );
            // 不命中，且收集轨迹时也不产生任何条目（残帧在遍历规则前就短路）
            let outcome = evaluate(&rules, &partial, true);
            assert!(!outcome.is_hit(), "残帧不应命中: {:?}", origin);
            assert!(
                outcome.rule_index.is_none(),
                "残帧不应产生应答: {:?}",
                origin
            );
            assert!(
                outcome.trace.entries.is_empty(),
                "残帧不应留下轨迹: {:?}",
                origin
            );
        }
    }
}
