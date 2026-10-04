// 「回复规则」编辑弹窗(新建 / 编辑同用一个)
//
// 分区(A 基本信息 / B 条件构建器 / C 动作 / D 内联试跑), 整体纵向可滚动。
//
// ## 条件树为什么是"平铺 + 缩进"而不是递归渲染
//
// 规范允许降级: GPUI 的 `AnyElement` 递归渲染在借用与生命周期上代价很高
// (每层都要把子元素盒化, 且要跨层传递可变借用), 而这个弹窗的价值在**能用**,
// 不在视觉花哨。因此这里选择:
//   - 结构真源是 `CondDraft`(一棵与 `MatchNode` 同构的**可编辑**树);
//   - 渲染时把它展平为若干行, 每行有 `depth` 缩进与显式的 `All {` / `}` 括号标记,
//     层次的文字化表达与原 AST 一一对应, 不丢任何信息。
//
// ## 文本值为什么住"懒创建的输入实体"里
//
// GPUI 的文本输入必须有稳定的 `Entity<InputState>`, 而条件树的字段是动态的。
// 折中: 每个字段用一个以 `CondDraft.uid` 为主键的 key (`{uid}:{field}`) 在渲染期
// 懒创建并缓存到 `cond_inputs`; 树的结构改动只动 `CondDraft`, 主键不随位置变化,
// 因此**增删节点不会串改已输入的值**。文本真源是输入实体, 每帧回填进 `draft`。
//
// ## 试跑与真实行为的一致性
//
// 试跑直接用 `crate::reply::exec::dry_run` —— 它与网络热路径的 `handle_frame`
// 调用同一个 `evaluate()` 和同一个 `render_reply()`(exec.rs 里有硬断言测试)。
// 因此"预演结果 = 真实行为", 不存在两套匹配逻辑。
//
// **过期结果防误导**: 每帧重建的 `draft` 与上次试跑时的规则用 `PartialEq` 比较,
// 一旦任何字段变化就丢弃上次结果(规范明确点出的误导性 UI 错误)。

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::ElementExt as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::Theme;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{EditorState, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::app::NetAssistantApp;
use crate::core::checksum::ChecksumAlgorithm;
use crate::reply::matcher::PredicateResult;
use crate::reply::model::{
    ByteOp, BytePattern, ChecksumAt, Cmp, Constraint, Endian, MatchNode, ReplyPayload, ReplyRule,
    ReplyRulesConfig, RuleCodec, Severity, Width, format_matcher_summary, validate_rule,
};
use crate::ui::components::hex_editor::{HexEditorState, adapter as hex_adapter};
use crate::ui::components::input_with_mode::InputWithMode;
use crate::ui::dialog::variable_picker::{
    VariableItem, render_grouped_variable_picker, reply_rule_variable_groups,
};
use crate::utils::hex::{hex_to_bytes, validate_hex_input};

use super::{dialog_content_max_height, dialog_height};

// ============================================================================
// 可编辑的条件树（CondDraft）
// ============================================================================

/// 条件节点种类(与 `MatchNode` 的 15 个变体一一对应)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondKind {
    All,
    Any,
    Not,
    Contains,
    FixedExact,
    FixedMask,
    PrefixRange,
    Length,
    ByteAt,
    ScalarAt,
    Suffix,
    Regex,
    From,
    FieldEq,
    ChecksumValid,
}

impl CondKind {
    /// 全部种类(添加条件时的候选顺序 = 常用度顺序)
    pub const ALL: [CondKind; 15] = [
        CondKind::Contains,
        CondKind::Length,
        CondKind::FixedExact,
        CondKind::PrefixRange,
        CondKind::ByteAt,
        CondKind::ScalarAt,
        CondKind::Suffix,
        CondKind::ChecksumValid,
        CondKind::Regex,
        CondKind::From,
        CondKind::FieldEq,
        CondKind::FixedMask,
        CondKind::All,
        CondKind::Any,
        CondKind::Not,
    ];

    fn is_combinator(self) -> bool {
        matches!(self, CondKind::All | CondKind::Any | CondKind::Not)
    }

    fn label(self) -> String {
        let key = match self {
            CondKind::All => "reply_rule_edit.kind_all",
            CondKind::Any => "reply_rule_edit.kind_any",
            CondKind::Not => "reply_rule_edit.kind_not",
            CondKind::Contains => "reply_rule_edit.kind_contains",
            CondKind::FixedExact => "reply_rule_edit.kind_fixed_exact",
            CondKind::FixedMask => "reply_rule_edit.kind_fixed_mask",
            CondKind::PrefixRange => "reply_rule_edit.kind_prefix_range",
            CondKind::Length => "reply_rule_edit.kind_length",
            CondKind::ByteAt => "reply_rule_edit.kind_byte_at",
            CondKind::ScalarAt => "reply_rule_edit.kind_scalar_at",
            CondKind::Suffix => "reply_rule_edit.kind_suffix",
            CondKind::Regex => "reply_rule_edit.kind_regex",
            CondKind::From => "reply_rule_edit.kind_from",
            CondKind::FieldEq => "reply_rule_edit.kind_field_eq",
            CondKind::ChecksumValid => "reply_rule_edit.kind_checksum_valid",
        };
        t!(key).to_string()
    }
}

/// 字节操作的 UI 种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Eq,
    Ne,
    Masked,
    In,
}

impl OpKind {
    const ALL: [OpKind; 4] = [OpKind::Eq, OpKind::Ne, OpKind::Masked, OpKind::In];

    fn label(self) -> String {
        let key = match self {
            OpKind::Eq => "reply_rule_edit.op_eq",
            OpKind::Ne => "reply_rule_edit.op_ne",
            OpKind::Masked => "reply_rule_edit.op_masked",
            OpKind::In => "reply_rule_edit.op_in",
        };
        t!(key).to_string()
    }
}

/// 位置约束的 UI 种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    ByteEq,
    ByteMaskNonZero,
    ByteIn,
    LengthFromByte,
}

impl ConstraintKind {
    const ALL: [ConstraintKind; 4] = [
        ConstraintKind::ByteEq,
        ConstraintKind::ByteMaskNonZero,
        ConstraintKind::ByteIn,
        ConstraintKind::LengthFromByte,
    ];

    fn label(self) -> String {
        let key = match self {
            ConstraintKind::ByteEq => "reply_rule_edit.constraint_byte_eq",
            ConstraintKind::ByteMaskNonZero => "reply_rule_edit.constraint_byte_mask",
            ConstraintKind::ByteIn => "reply_rule_edit.constraint_byte_in",
            ConstraintKind::LengthFromByte => "reply_rule_edit.constraint_length_from_byte",
        };
        t!(key).to_string()
    }
}

/// 一条位置约束的可编辑形态
#[derive(Debug, Clone)]
pub struct ConstraintDraft {
    pub uid: u64,
    pub kind: ConstraintKind,
    pub width: Width,
    pub endian: Endian,
    /// 文本字段的当前值(真源是输入实体, 这里只是种子/回退)
    pub values: HashMap<String, String>,
}

/// 一个条件节点的可编辑形态
///
/// 用"一个结构体 + kind 判别"而不是 15 个变体: 渲染与写回是**同构**的,
/// 变体化只会让每处都要写 15 个分支。
#[derive(Debug, Clone)]
pub struct CondDraft {
    pub uid: u64,
    pub kind: CondKind,
    pub children: Vec<CondDraft>,
    pub constraints: Vec<ConstraintDraft>,
    pub values: HashMap<String, String>,
    // 非文本选择(直接改结构, 无需输入实体)
    pub hex_mode: bool,
    pub width: Width,
    pub endian: Endian,
    pub signed: bool,
    pub cmp: Cmp,
    pub op: OpKind,
    pub algorithm: ChecksumAlgorithm,
    /// 校验位位置: false = Fixed(固定偏移), true = Trailing(末尾倒数)
    pub at_trailing: bool,
}

impl CondDraft {
    fn new(uid: u64, kind: CondKind) -> Self {
        let mut values = HashMap::new();
        match kind {
            CondKind::Contains => {
                values.insert("bytes".into(), "01 03".to_string());
            }
            CondKind::FixedExact => {
                values.insert("len".into(), "8".to_string());
                values.insert("bytes".into(), String::new());
            }
            CondKind::FixedMask => {
                values.insert("len".into(), "8".to_string());
                values.insert("bytes".into(), String::new());
                values.insert("mask".into(), "FF FF FF FF FF FF FF FF".to_string());
            }
            CondKind::PrefixRange => {
                values.insert("prefix".into(), String::new());
                values.insert("min_len".into(), "1".to_string());
                values.insert("max_len".into(), "256".to_string());
            }
            CondKind::Length => {
                values.insert("min".into(), "1".to_string());
                values.insert("max".into(), "64".to_string());
            }
            CondKind::ByteAt => {
                values.insert("offset".into(), "0".to_string());
                values.insert("op_value".into(), "00".to_string());
                values.insert("op_mask".into(), "FF".to_string());
                values.insert("op_values".into(), "01 02 03".to_string());
            }
            CondKind::ScalarAt => {
                values.insert("offset".into(), "0".to_string());
                values.insert("value".into(), "0".to_string());
            }
            CondKind::Suffix => {
                values.insert("bytes".into(), String::new());
            }
            CondKind::Regex => {
                values.insert("pattern".into(), String::new());
            }
            CondKind::From => {
                values.insert("addrs".into(), "127.0.0.1".to_string());
            }
            CondKind::FieldEq => {
                values.insert("accessor".into(), "u8".to_string());
                values.insert("offset".into(), "0".to_string());
                values.insert("value".into(), "0".to_string());
            }
            CondKind::ChecksumValid => {
                values.insert("at_offset".into(), "0".to_string());
                values.insert("at_n".into(), "2".to_string());
                values.insert("range".into(), String::new());
            }
            CondKind::All | CondKind::Any | CondKind::Not => {}
        }
        Self {
            uid,
            kind,
            children: Vec::new(),
            constraints: Vec::new(),
            values,
            // 字节模式默认按 hex 解释: 二进制协议是规则的主要场景
            hex_mode: true,
            width: Width::U16,
            endian: Endian::Big,
            signed: false,
            cmp: Cmp::Eq,
            op: OpKind::Eq,
            algorithm: ChecksumAlgorithm::Crc16Modbus,
            at_trailing: true,
        }
    }

    fn from_node(node: &MatchNode, next_uid: &mut u64) -> Self {
        let uid = *next_uid;
        *next_uid += 1;
        let mut values = HashMap::new();
        let mut children = Vec::new();
        let mut constraints = Vec::new();
        let mut hex_mode = true;
        let mut width = Width::U16;
        let mut endian = Endian::Big;
        let mut signed = false;
        let mut cmp = Cmp::Eq;
        let mut op = OpKind::Eq;
        let mut algorithm = ChecksumAlgorithm::Crc16Modbus;
        let mut at_trailing = true;

        let kind = match node {
            MatchNode::All { children: c } => {
                children = c.iter().map(|n| Self::from_node(n, next_uid)).collect();
                CondKind::All
            }
            MatchNode::Any { children: c } => {
                children = c.iter().map(|n| Self::from_node(n, next_uid)).collect();
                CondKind::Any
            }
            MatchNode::Not { child } => {
                children = vec![Self::from_node(child, next_uid)];
                CondKind::Not
            }
            MatchNode::Contains { bytes } => {
                hex_mode = matches!(bytes, BytePattern::Hex(_));
                values.insert("bytes".into(), bytes.raw_text());
                CondKind::Contains
            }
            MatchNode::FixedExact { len, bytes } => {
                hex_mode = matches!(bytes, BytePattern::Hex(_));
                values.insert("len".into(), len.to_string());
                values.insert("bytes".into(), bytes.raw_text());
                CondKind::FixedExact
            }
            MatchNode::FixedMask { len, bytes, mask } => {
                hex_mode = matches!(bytes, BytePattern::Hex(_));
                values.insert("len".into(), len.to_string());
                values.insert("bytes".into(), bytes.raw_text());
                values.insert(
                    "mask".into(),
                    mask.iter()
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                CondKind::FixedMask
            }
            MatchNode::PrefixRange {
                prefix,
                min_len,
                max_len,
                constraints: cs,
            } => {
                hex_mode = matches!(prefix, BytePattern::Hex(_));
                values.insert("prefix".into(), prefix.raw_text());
                values.insert("min_len".into(), min_len.to_string());
                values.insert("max_len".into(), max_len.to_string());
                for c in cs {
                    constraints.push(ConstraintDraft::from_constraint(c, next_uid));
                }
                CondKind::PrefixRange
            }
            MatchNode::Length { min, max } => {
                values.insert("min".into(), min.to_string());
                values.insert("max".into(), max.to_string());
                CondKind::Length
            }
            MatchNode::ByteAt {
                offset,
                op: byte_op,
            } => {
                values.insert("offset".into(), offset.to_string());
                match byte_op {
                    ByteOp::Eq { value } => {
                        op = OpKind::Eq;
                        values.insert("op_value".into(), format!("{:02X}", value));
                    }
                    ByteOp::Ne { value } => {
                        op = OpKind::Ne;
                        values.insert("op_value".into(), format!("{:02X}", value));
                    }
                    ByteOp::Masked { mask, value } => {
                        op = OpKind::Masked;
                        values.insert("op_mask".into(), format!("{:02X}", mask));
                        values.insert("op_value".into(), format!("{:02X}", value));
                    }
                    ByteOp::In { values: vs } => {
                        op = OpKind::In;
                        values.insert(
                            "op_values".into(),
                            vs.iter()
                                .map(|b| format!("{:02X}", b))
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                    }
                }
                CondKind::ByteAt
            }
            MatchNode::ScalarAt {
                offset,
                width: w,
                endian: e,
                signed: s,
                cmp: c,
                value,
            } => {
                values.insert("offset".into(), offset.to_string());
                values.insert("value".into(), value.to_string());
                width = *w;
                endian = *e;
                signed = *s;
                cmp = *c;
                CondKind::ScalarAt
            }
            MatchNode::Suffix { bytes } => {
                hex_mode = matches!(bytes, BytePattern::Hex(_));
                values.insert("bytes".into(), bytes.raw_text());
                CondKind::Suffix
            }
            MatchNode::Regex { pattern } => {
                values.insert("pattern".into(), pattern.clone());
                CondKind::Regex
            }
            MatchNode::From { addrs } => {
                values.insert("addrs".into(), addrs.join(", "));
                CondKind::From
            }
            MatchNode::FieldEq {
                accessor,
                offset,
                cmp: c,
                value,
            } => {
                values.insert("accessor".into(), accessor.clone());
                values.insert("offset".into(), offset.to_string());
                values.insert("value".into(), value.to_string());
                cmp = *c;
                CondKind::FieldEq
            }
            MatchNode::ChecksumValid {
                algorithm: a,
                at,
                range,
            } => {
                algorithm = *a;
                match at {
                    ChecksumAt::Fixed { offset, width: w } => {
                        at_trailing = false;
                        width = *w;
                        values.insert("at_offset".into(), offset.to_string());
                        values.insert("at_n".into(), "2".to_string());
                    }
                    ChecksumAt::Trailing { n, width: w } => {
                        at_trailing = true;
                        width = *w;
                        values.insert("at_offset".into(), "0".to_string());
                        values.insert("at_n".into(), n.to_string());
                    }
                }
                values.insert(
                    "range".into(),
                    range
                        .map(|(o, l)| format!("{}:{}", o, l))
                        .unwrap_or_default(),
                );
                CondKind::ChecksumValid
            }
        };

        Self {
            uid,
            kind,
            children,
            constraints,
            values,
            hex_mode,
            width,
            endian,
            signed,
            cmp,
            op,
            algorithm,
            at_trailing,
        }
    }

    /// 收集本节点(含子树/约束)需要的全部文本字段: (key, 种子值)
    fn collect_fields(&self, out: &mut Vec<(String, String)>) {
        for field in self.field_names() {
            let seed = self.values.get(field).cloned().unwrap_or_default();
            out.push((format!("{}:{}", self.uid, field), seed));
        }
        for c in &self.children {
            c.collect_fields(out);
        }
        for c in &self.constraints {
            for field in c.field_names() {
                let seed = c.values.get(field).cloned().unwrap_or_default();
                out.push((format!("{}:{}", c.uid, field), seed));
            }
        }
    }

    /// 该节点里按 Hex/文本模式解释的字段名（None = 无）。
    /// 与 `build_node` 里 `pattern_of(..., cd.hex_mode)` 的字段一一对应。
    fn byte_field(&self) -> Option<&'static str> {
        match self.kind {
            CondKind::Contains | CondKind::Suffix | CondKind::FixedExact | CondKind::FixedMask => {
                Some("bytes")
            }
            CondKind::PrefixRange => Some("prefix"),
            _ => None,
        }
    }

    /// 收集字节模式字段的 key 及其当前模式 (需要 hex 编辑器)
    fn collect_byte_fields(&self, out: &mut Vec<(String, bool)>) {
        if let Some(field) = self.byte_field() {
            out.push((format!("{}:{}", self.uid, field), self.hex_mode));
        }
        for c in &self.children {
            c.collect_byte_fields(out);
        }
    }

    /// 该种类需要哪些文本字段
    fn field_names(&self) -> Vec<&'static str> {
        match self.kind {
            CondKind::All | CondKind::Any | CondKind::Not => vec![],
            CondKind::Contains | CondKind::Suffix => vec!["bytes"],
            CondKind::FixedExact => vec!["len", "bytes"],
            CondKind::FixedMask => vec!["len", "bytes", "mask"],
            CondKind::PrefixRange => vec!["prefix", "min_len", "max_len"],
            CondKind::Length => vec!["min", "max"],
            CondKind::ByteAt => match self.op {
                OpKind::Eq | OpKind::Ne => vec!["offset", "op_value"],
                OpKind::Masked => vec!["offset", "op_value", "op_mask"],
                OpKind::In => vec!["offset", "op_values"],
            },
            CondKind::ScalarAt => vec!["offset", "value"],
            CondKind::Regex => vec!["pattern"],
            CondKind::From => vec!["addrs"],
            CondKind::FieldEq => vec!["accessor", "offset", "value"],
            CondKind::ChecksumValid => {
                if self.at_trailing {
                    vec!["at_n", "range"]
                } else {
                    vec!["at_offset", "range"]
                }
            }
        }
    }
}

impl ConstraintDraft {
    fn new(uid: u64, kind: ConstraintKind) -> Self {
        let mut values = HashMap::new();
        match kind {
            ConstraintKind::ByteEq => {
                values.insert("offset".into(), "1".to_string());
                values.insert("value".into(), "03".to_string());
            }
            ConstraintKind::ByteMaskNonZero => {
                values.insert("offset".into(), "1".to_string());
                values.insert("mask".into(), "F0".to_string());
            }
            ConstraintKind::ByteIn => {
                values.insert("offset".into(), "1".to_string());
                values.insert("values".into(), "01 02 03 04".to_string());
            }
            ConstraintKind::LengthFromByte => {
                values.insert("len_offset".into(), "2".to_string());
                values.insert("base".into(), "3".to_string());
            }
        }
        Self {
            uid,
            kind,
            width: Width::U16,
            endian: Endian::Big,
            values,
        }
    }

    fn from_constraint(c: &Constraint, next_uid: &mut u64) -> Self {
        let uid = *next_uid;
        *next_uid += 1;
        let mut values = HashMap::new();
        let mut width = Width::U16;
        let mut endian = Endian::Big;
        let kind = match c {
            Constraint::ByteEq { offset, value } => {
                values.insert("offset".into(), offset.to_string());
                values.insert("value".into(), format!("{:02X}", value));
                ConstraintKind::ByteEq
            }
            Constraint::ByteMaskNonZero { offset, mask } => {
                values.insert("offset".into(), offset.to_string());
                values.insert("mask".into(), format!("{:02X}", mask));
                ConstraintKind::ByteMaskNonZero
            }
            Constraint::ByteIn { offset, values: vs } => {
                values.insert("offset".into(), offset.to_string());
                values.insert(
                    "values".into(),
                    vs.iter()
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                ConstraintKind::ByteIn
            }
            Constraint::LengthFromByte {
                len_offset,
                width: w,
                endian: e,
                base,
            } => {
                values.insert("len_offset".into(), len_offset.to_string());
                values.insert("base".into(), base.to_string());
                width = *w;
                endian = *e;
                ConstraintKind::LengthFromByte
            }
        };
        Self {
            uid,
            kind,
            width,
            endian,
            values,
        }
    }

    fn field_names(&self) -> Vec<&'static str> {
        match self.kind {
            ConstraintKind::ByteEq | ConstraintKind::ByteMaskNonZero => vec!["offset", "value"],
            ConstraintKind::ByteIn => vec!["offset", "values"],
            ConstraintKind::LengthFromByte => vec!["len_offset", "base"],
        }
    }
}

/// `BytePattern` / 掩码的原始文本(展示与再编辑用)
trait RawText {
    fn raw_text(&self) -> String;
}

impl RawText for BytePattern {
    fn raw_text(&self) -> String {
        match self {
            BytePattern::Hex(s) | BytePattern::Text(s) => s.clone(),
        }
    }
}

// ============================================================================
// 文本值读写
// ============================================================================

fn key_of(uid: u64, field: &str) -> String {
    format!("{}:{}", uid, field)
}

fn val(values: &HashMap<String, String>, uid: u64, field: &str) -> String {
    values.get(&key_of(uid, field)).cloned().unwrap_or_default()
}

fn num(values: &HashMap<String, String>, uid: u64, field: &str, default: usize) -> usize {
    val(values, uid, field)
        .trim()
        .parse::<usize>()
        .unwrap_or(default)
}

fn num_i64(values: &HashMap<String, String>, uid: u64, field: &str, default: i64) -> i64 {
    val(values, uid, field)
        .trim()
        .parse::<i64>()
        .unwrap_or(default)
}

/// 「01 02 03」这类 hex 字节列 → Vec<u8>
fn hex_list(text: &str) -> Vec<u8> {
    hex_to_bytes(text)
}

/// 08 → u8(非法时 0)
fn u8_of(text: &str) -> u8 {
    let t = text.trim();
    if t.is_empty() {
        return 0;
    }
    u8::from_str_radix(t.trim_start_matches("0x"), 16).unwrap_or_else(|_| {
        t.parse::<u8>()
            .unwrap_or_else(|_| hex_to_bytes(t).first().copied().unwrap_or(0))
    })
}

fn pattern_of(text: String, hex_mode: bool) -> BytePattern {
    if hex_mode {
        BytePattern::Hex(text)
    } else {
        BytePattern::Text(text)
    }
}

/// 由结构 + 文本值构建 `MatchNode`
fn build_node(cd: &CondDraft, values: &HashMap<String, String>) -> MatchNode {
    match cd.kind {
        CondKind::All => MatchNode::All {
            children: cd.children.iter().map(|c| build_node(c, values)).collect(),
        },
        CondKind::Any => MatchNode::Any {
            children: cd.children.iter().map(|c| build_node(c, values)).collect(),
        },
        CondKind::Not => MatchNode::Not {
            child: Box::new(match cd.children.first() {
                Some(c) => build_node(c, values),
                // 空 Not 没有语义: 退化为空 Any(恒不成立 + 校验报"组合条件不能为空")。
                // 不能退化成空 All —— 它现在恒真(匹配全部报文), 会让"空非"静默变成兜底规则
                None => MatchNode::Any {
                    children: Vec::new(),
                },
            }),
        },
        CondKind::Contains => MatchNode::Contains {
            bytes: pattern_of(val(values, cd.uid, "bytes"), cd.hex_mode),
        },
        CondKind::FixedExact => MatchNode::FixedExact {
            len: num(values, cd.uid, "len", 0),
            bytes: pattern_of(val(values, cd.uid, "bytes"), cd.hex_mode),
        },
        CondKind::FixedMask => MatchNode::FixedMask {
            len: num(values, cd.uid, "len", 0),
            bytes: pattern_of(val(values, cd.uid, "bytes"), cd.hex_mode),
            mask: hex_list(&val(values, cd.uid, "mask")),
        },
        CondKind::PrefixRange => MatchNode::PrefixRange {
            prefix: pattern_of(val(values, cd.uid, "prefix"), cd.hex_mode),
            min_len: num(values, cd.uid, "min_len", 0),
            max_len: num(values, cd.uid, "max_len", 0),
            constraints: cd
                .constraints
                .iter()
                .map(|c| build_constraint(c, values))
                .collect(),
        },
        CondKind::Length => MatchNode::Length {
            min: num(values, cd.uid, "min", 0),
            max: num(values, cd.uid, "max", 0),
        },
        CondKind::ByteAt => MatchNode::ByteAt {
            offset: num(values, cd.uid, "offset", 0),
            op: match cd.op {
                OpKind::Eq => ByteOp::Eq {
                    value: u8_of(&val(values, cd.uid, "op_value")),
                },
                OpKind::Ne => ByteOp::Ne {
                    value: u8_of(&val(values, cd.uid, "op_value")),
                },
                OpKind::Masked => ByteOp::Masked {
                    mask: u8_of(&val(values, cd.uid, "op_mask")),
                    value: u8_of(&val(values, cd.uid, "op_value")),
                },
                OpKind::In => ByteOp::In {
                    values: hex_list(&val(values, cd.uid, "op_values")),
                },
            },
        },
        CondKind::ScalarAt => MatchNode::ScalarAt {
            offset: num(values, cd.uid, "offset", 0),
            width: cd.width,
            endian: cd.endian,
            signed: cd.signed,
            cmp: cd.cmp,
            value: num_i64(values, cd.uid, "value", 0),
        },
        CondKind::Suffix => MatchNode::Suffix {
            bytes: pattern_of(val(values, cd.uid, "bytes"), cd.hex_mode),
        },
        CondKind::Regex => MatchNode::Regex {
            pattern: val(values, cd.uid, "pattern"),
        },
        CondKind::From => MatchNode::From {
            addrs: val(values, cd.uid, "addrs")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        },
        CondKind::FieldEq => MatchNode::FieldEq {
            accessor: val(values, cd.uid, "accessor").trim().to_string(),
            offset: num(values, cd.uid, "offset", 0),
            cmp: cd.cmp,
            value: num_i64(values, cd.uid, "value", 0),
        },
        CondKind::ChecksumValid => MatchNode::ChecksumValid {
            algorithm: cd.algorithm,
            at: if cd.at_trailing {
                ChecksumAt::Trailing {
                    n: num(values, cd.uid, "at_n", 0),
                    width: cd.width,
                }
            } else {
                ChecksumAt::Fixed {
                    offset: num(values, cd.uid, "at_offset", 0),
                    width: cd.width,
                }
            },
            range: parse_range(&val(values, cd.uid, "range")),
        },
    }
}

/// 「起始:长度」文本 → Option<(usize, usize)>（空 = 到校验位之前）
fn parse_range(text: &str) -> Option<(usize, usize)> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let (a, b) = t.split_once(':')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn build_constraint(c: &ConstraintDraft, values: &HashMap<String, String>) -> Constraint {
    match c.kind {
        ConstraintKind::ByteEq => Constraint::ByteEq {
            offset: num(values, c.uid, "offset", 0),
            value: u8_of(&val(values, c.uid, "value")),
        },
        ConstraintKind::ByteMaskNonZero => Constraint::ByteMaskNonZero {
            offset: num(values, c.uid, "offset", 0),
            mask: u8_of(&val(values, c.uid, "value")),
        },
        ConstraintKind::ByteIn => Constraint::ByteIn {
            offset: num(values, c.uid, "offset", 0),
            values: hex_list(&val(values, c.uid, "values")),
        },
        ConstraintKind::LengthFromByte => Constraint::LengthFromByte {
            len_offset: num(values, c.uid, "len_offset", 0),
            width: c.width,
            endian: c.endian,
            base: num_i64(values, c.uid, "base", 0),
        },
    }
}

// ============================================================================
// 条件树的路径操作（平铺渲染的编辑基础）
// ============================================================================

/// 条件结构指纹(零分配): 覆盖影响 `build_node` 输出的全部非文本选择与树形。
/// 用 `discriminant` 避免给 model 侧枚举追加 `Hash` derive。
fn hash_cond(node: &CondDraft, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    node.uid.hash(h);
    std::mem::discriminant(&node.kind).hash(h);
    node.hex_mode.hash(h);
    std::mem::discriminant(&node.width).hash(h);
    std::mem::discriminant(&node.endian).hash(h);
    node.signed.hash(h);
    std::mem::discriminant(&node.cmp).hash(h);
    std::mem::discriminant(&node.op).hash(h);
    std::mem::discriminant(&node.algorithm).hash(h);
    node.at_trailing.hash(h);
    node.children.len().hash(h);
    for c in &node.children {
        hash_cond(c, h);
    }
    node.constraints.len().hash(h);
    for c in &node.constraints {
        c.uid.hash(h);
        std::mem::discriminant(&c.kind).hash(h);
        std::mem::discriminant(&c.width).hash(h);
        std::mem::discriminant(&c.endian).hash(h);
    }
}

fn cond_at_mut<'a>(root: &'a mut CondDraft, path: &[usize]) -> Option<&'a mut CondDraft> {
    let mut cur = root;
    for &i in path {
        cur = cur.children.get_mut(i)?;
    }
    Some(cur)
}

/// 删除 path 指向的节点(不能删根)；返回是否删除
fn cond_remove(root: &mut CondDraft, path: &[usize]) -> bool {
    let Some((last, parent_path)) = path.split_last() else {
        return false;
    };
    let Some(parent) = cond_at_mut(root, parent_path) else {
        return false;
    };
    if *last < parent.children.len() {
        parent.children.remove(*last);
        true
    } else {
        false
    }
}

fn cond_add_child(root: &mut CondDraft, path: &[usize], kind: CondKind, uid: u64) -> bool {
    let Some(node) = cond_at_mut(root, path) else {
        return false;
    };
    if !node.kind.is_combinator() {
        return false;
    }
    // Not 只允许一个子条件
    if node.kind == CondKind::Not && !node.children.is_empty() {
        return false;
    }
    node.children.push(CondDraft::new(uid, kind));
    true
}

fn cond_remove_constraint(root: &mut CondDraft, path: &[usize], index: usize) -> bool {
    let Some(node) = cond_at_mut(root, path) else {
        return false;
    };
    if index < node.constraints.len() {
        node.constraints.remove(index);
        true
    } else {
        false
    }
}

// ============================================================================
// 弹窗状态
// ============================================================================

/// 一次试跑的结果(命中摘要 + 渲染字节 + 变量清单)
pub struct TestRunResult {
    /// 试跑时的规则快照: 与当前草稿不等即说明"结果已过期", 直接丢弃
    pub tested_rule: ReplyRule,
    pub hit: bool,
    pub hit_rule_name: Option<String>,
    /// (label, actual, result) 逐谓词
    pub rows: Vec<(String, String, PredicateResult)>,
    pub first_failure: Option<(String, String)>,
    pub rendered: Option<Vec<u8>>,
    pub variables: Vec<String>,
    /// 帧字节解析失败等硬错误
    pub error: Option<String>,
}

/// 条件树平铺渲染的一行
struct CondRow {
    path: Vec<usize>,
    /// 该节点在 issue path 里的前缀(如 `matcher.children[0]`)
    issue_prefix: String,
    depth: usize,
    role: CondRole,
    node: CondDraft,
}

#[derive(PartialEq, Eq)]
enum CondRole {
    Open,
    Close,
    Leaf,
}

/// 条件树里字节模式字段(Hex/文本)的输入实体
///
/// 与普通条件字段(单行 `InputState`)分开存: 字节模式字段要复用应答载荷那套
/// `InputWithMode` + hex 网格, 需要 `EditorState`(多行) 与 `HexEditorState` 两个实体。
pub struct CondByteField {
    pub input: Entity<EditorState>,
    pub hex_editor: Entity<HexEditorState>,
}

/// 匹配模式：任意消息 / 自定义条件
///
/// 「任意消息」把 matcher 固定为 `All { children: [] }`（空组合恒真 = 匹配全部报文），
/// 回到旧版「勾选即自动回复」的直观体验；「自定义条件」使用条件树。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MatchMode {
    /// 任意消息（匹配全部报文）
    Any,
    /// 自定义条件（条件树）
    Custom,
}

/// 编辑弹窗状态(打开时创建, 关闭时由 app 置 None)
pub struct ReplyRuleEditDialogState {
    /// None = 新建
    pub editing_id: Option<String>,
    /// 当前草稿(每帧由输入实体与条件结构重建, 只读消费)
    pub draft: ReplyRule,
    /// 条件树结构真源
    pub cond: CondDraft,
    /// 匹配模式（任意消息 / 自定义条件）；决定 `draft.matcher` 的构建方式
    pub match_mode: MatchMode,
    /// 条件字段的懒创建输入实体(key = `{uid}:{field}`)
    pub cond_inputs: HashMap<String, Entity<InputState>>,
    /// 字节模式字段的懒创建实体(key = `{uid}:{field}`)
    pub cond_byte_inputs: HashMap<String, CondByteField>,
    /// 上次渲染时的输入指纹: 未变化则跳过重建草稿与校验(避免每帧实时校验)
    last_fingerprint: Option<u64>,
    /// 缓存的校验结果(与 `draft` 同步更新, 供渲染直接读取)
    pub issues: Vec<crate::reply::model::Issue>,
    /// uid 分配器(增删节点用)
    pub next_uid: u64,
    /// 展开"添加条件"菜单的组合节点 uid
    pub add_menu_open: Option<u64>,
    /// 展开"添加约束"菜单的节点 path
    pub add_constraint_open: Option<Vec<usize>>,

    pub name_input: Entity<InputState>,
    pub desc_input: Entity<InputState>,
    pub tags_input: Entity<InputState>,
    pub enabled: bool,
    /// 本规则所属连接 id（= tab_id）。规则严格属于某一连接, 不再有全局作用域。
    pub tab_id: String,
    pub tab_label: String,

    pub payload_input: Entity<EditorState>,
    pub payload_hex_editor: Entity<HexEditorState>,
    pub payload_hex_mode: bool,
    pub payload_codec: RuleCodec,

    pub test_input: Entity<EditorState>,
    pub test_hex_editor: Entity<HexEditorState>,
    pub test_hex_mode: bool,
    pub test_source: Entity<InputState>,
    pub test_result: Option<TestRunResult>,

    /// 「插入变量」浮层(作用于应答载荷输入框)
    pub show_variable_picker: bool,
    pub var_button_bounds: Bounds<Pixels>,

    /// 保存受阻原因(Error 级校验的首条)
    pub error: Option<String>,
}

impl ReplyRuleEditDialogState {
    /// 由已有规则(或新建)构造状态
    pub fn new(
        editing: Option<ReplyRule>,
        tab_id: String,
        tab_label: String,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        let mut next_uid = 1u64;
        // 编辑态由入参决定(而不是看 draft.id): 新建规则的 id 也是预生成的 UUID
        let editing_id = editing.as_ref().map(|r| r.id.clone());
        // 匹配模式: 编辑既有规则时按 matcher 判定(空 All = 任意消息); 新建默认「任意消息」
        let match_mode = match &editing {
            Some(r) if r.matcher.is_match_all() => MatchMode::Any,
            Some(_) => MatchMode::Custom,
            None => MatchMode::Any,
        };
        let (rule, cond) = match editing {
            Some(r) => {
                let cond = CondDraft::from_node(&r.matcher, &mut next_uid);
                (r, cond)
            }
            None => {
                // 新建: 默认匹配模式是「任意消息」(matcher 固定空 All, 见下方)。
                //
                // 条件树草稿预置为**空 All 根**(无任何子条件): 切到「自定义条件」后,
                // 由用户自己点根节点上的「＋ 添加条件」逐条构建。
                // 不预填 `Length { 1..64 }` 之类的假条件 —— 默认值很容易被当成
                // 业务约束直接保存, 从而默默过滤掉本该匹配的报文。
                let root = CondDraft::new(0, CondKind::All);
                let mut r = ReplyRule::new(String::new(), 0);
                r.payload = ReplyPayload::default();
                (r, root)
            }
        };
        let values: HashMap<String, String> = {
            let mut v = HashMap::new();
            let mut fields = Vec::new();
            cond.collect_fields(&mut fields);
            for (k, val) in fields {
                v.insert(k, val);
            }
            v
        };

        let name_input = text_input(&rule.name, window, cx, "reply_rule_edit.name_placeholder");
        let desc_input = text_input(
            &rule.description,
            window,
            cx,
            "reply_rule_edit.description_placeholder",
        );
        let tags_input = text_input(
            &rule.tags.join(", "),
            window,
            cx,
            "reply_rule_edit.tags_placeholder",
        );
        let payload = &rule.payload;
        let (payload_text, payload_hex, payload_codec) =
            (payload.text.clone(), payload.hex_mode, payload.codec);

        let payload_input = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("json")
                .line_number(false)
                .folding(false)
                .context_menu(false)
                .placeholder(t!("reply_rule_edit.payload_placeholder").to_string())
        });
        payload_input.update(cx, |input, cx| {
            input.set_value(payload_text, window, cx);
        });
        let payload_hex_editor = cx.new(|cx| {
            HexEditorState::with_inline_bytes_per_row(cx, hex_adapter::INLINE_BYTES_PER_ROW_WIDE)
        });

        let test_input = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("json")
                .line_number(false)
                .folding(false)
                .context_menu(false)
                .placeholder(t!("reply_rule_edit.test_frame_placeholder").to_string())
        });
        let test_hex_editor = cx.new(|cx| {
            HexEditorState::with_inline_bytes_per_row(cx, hex_adapter::INLINE_BYTES_PER_ROW_WIDE)
        });
        let test_source = text_input(
            "127.0.0.1:12345",
            window,
            cx,
            "reply_rule_edit.test_source_placeholder",
        );

        let mut draft = rule;
        // 「任意消息」固定为空 All 组合(匹配全部报文); 「自定义条件」由条件树构建
        draft.matcher = if match_mode == MatchMode::Any {
            MatchNode::All {
                children: Vec::new(),
            }
        } else {
            build_node(&cond, &values)
        };

        Self {
            editing_id,
            draft,
            cond,
            match_mode,
            cond_inputs: HashMap::new(),
            cond_byte_inputs: HashMap::new(),
            last_fingerprint: None,
            issues: Vec::new(),
            next_uid,
            add_menu_open: None,
            add_constraint_open: None,
            name_input,
            desc_input,
            tags_input,
            enabled: true,
            tab_id,
            tab_label,
            payload_input,
            payload_hex_editor,
            payload_hex_mode: payload_hex,
            payload_codec,
            test_input,
            test_hex_editor,
            test_hex_mode: true,
            test_source,
            test_result: None,
            show_variable_picker: false,
            var_button_bounds: Bounds::default(),
            error: None,
        }
    }

    fn alloc_uid(&mut self) -> u64 {
        let uid = self.next_uid;
        self.next_uid += 1;
        uid
    }

    /// 从条件输入实体收集文本值(普通字段 + 字节模式字段)
    fn cond_values(&self, cx: &App) -> HashMap<String, String> {
        let mut values: HashMap<String, String> = self
            .cond_inputs
            .iter()
            .map(|(k, e)| (k.clone(), e.read(cx).value().to_string()))
            .collect();
        for (k, f) in &self.cond_byte_inputs {
            values.insert(k.clone(), f.input.read(cx).value().to_string());
        }
        values
    }

    /// 输入指纹: 覆盖文本字段、非文本选择与条件结构。
    /// 指纹不变说明本帧没有任何编辑, 可跳过重建草稿与校验(消除每帧实时校验)。
    ///
    /// 直接哈希 `Rope`(text()) 而非 `value()`: 后者会把整段文本物化成 String,
    /// 大载荷下每帧一次分配就会重新变成瓶颈。key 排序后哈希, 不受 HashMap 迭代序影响。
    fn fingerprint(&self, cx: &App) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.name_input.read(cx).text().hash(&mut h);
        self.desc_input.read(cx).text().hash(&mut h);
        self.tags_input.read(cx).text().hash(&mut h);
        self.payload_input.read(cx).text().hash(&mut h);
        self.payload_hex_mode.hash(&mut h);
        std::mem::discriminant(&self.payload_codec).hash(&mut h);
        self.enabled.hash(&mut h);
        self.match_mode.hash(&mut h);
        let mut keys: Vec<&String> = self.cond_inputs.keys().collect();
        keys.sort_unstable();
        for k in keys {
            k.hash(&mut h);
            self.cond_inputs[k].read(cx).text().hash(&mut h);
        }
        let mut keys: Vec<&String> = self.cond_byte_inputs.keys().collect();
        keys.sort_unstable();
        for k in keys {
            k.hash(&mut h);
            self.cond_byte_inputs[k].input.read(cx).text().hash(&mut h);
        }
        hash_cond(&self.cond, &mut h);
        h.finish()
    }

    /// 重建草稿并缓存校验结果(文本真源 = 输入实体, 结构真源 = cond)
    fn refresh_draft(&mut self, cx: &App) {
        let values = self.cond_values(cx);
        self.draft.matcher = if self.match_mode == MatchMode::Any {
            MatchNode::All {
                children: Vec::new(),
            }
        } else {
            build_node(&self.cond, &values)
        };
        self.draft.name = self.name_input.read(cx).value().to_string();
        self.draft.description = self.desc_input.read(cx).value().to_string();
        self.draft.tags = self
            .tags_input
            .read(cx)
            .value()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.draft.enabled = self.enabled;
        let payload_text = self.payload_input.read(cx).text().to_string();
        self.draft.payload = ReplyPayload {
            text: payload_text,
            hex_mode: self.payload_hex_mode,
            codec: self.payload_codec,
        };
        // 过期试跑结果必须丢弃: 编辑任一字段后仍展示旧结果会直接误导用户
        if let Some(result) = &self.test_result {
            if result.tested_rule != self.draft {
                self.test_result = None;
            }
        }
        // 缓存校验结果(渲染直接读取, 避免每帧重复校验)
        self.issues = validate_rule(&self.draft);
        // 保存前的阻塞原因(首条 Error), 供底部提示与按钮禁用
        self.error = self
            .issues
            .iter()
            .find(|i| i.severity == Severity::Error)
            .map(|i| i.message.clone());
    }
}

fn text_input(
    value: &str,
    window: &mut Window,
    cx: &mut Context<NetAssistantApp>,
    placeholder_key: &str,
) -> Entity<InputState> {
    let input =
        cx.new(|cx| InputState::new(window, cx).placeholder(t!(placeholder_key).to_string()));
    input.update(cx, |input, cx| {
        input.set_value(value.to_string(), window, cx);
    });
    input
}

// ============================================================================
// 打开弹窗
// ============================================================================

/// 打开规则编辑弹窗。`rule_id = None` 表示新建。
///
/// `tab_id` = 规则所属连接, 由打开入口(连接页 / 管理弹窗)显式传入,
/// 不再依赖 `app.active_tab`(那会随标签切换而漂移)。
pub fn open_reply_rule_edit_dialog(
    app: WeakEntity<NetAssistantApp>,
    tab_id: String,
    rule_id: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    // 1) 先建状态(需要 Context 才能创建输入实体)
    let _ = app.update(cx, |app, cx| {
        let editing = rule_id.as_ref().and_then(|id| {
            app.storage
                .rules_for_connection(&tab_id)
                .iter()
                .find(|r| &r.id == id)
                .cloned()
        });
        let tab_label = app
            .connection_tabs
            .get(&tab_id)
            .map(|t| t.connection_config.address_label())
            .unwrap_or_default();
        // 重新借用: `window` 会被闭包按 move 捕获, 直接捕获会让后续 open_dialog 用不了它
        let window: &mut Window = &mut *window;
        let state = ReplyRuleEditDialogState::new(editing, tab_id.clone(), tab_label, window, cx);
        app.reply_rule_edit_dialog = Some(state);
    });

    // 2) 再开弹窗(内容闭包每帧从 app 读状态)
    window.open_dialog(cx, move |dialog, window, cx| {
        let editing = app
            .upgrade()
            .and_then(|e| {
                e.read(cx)
                    .reply_rule_edit_dialog
                    .as_ref()
                    .and_then(|s| s.editing_id.clone())
            })
            .is_some();
        let title = if editing {
            t!("reply_rule_edit.title_edit").to_string()
        } else {
            t!("reply_rule_edit.title_new").to_string()
        };
        dialog
            .title(title)
            .w(px(760.0))
            .max_h(dialog_height(window))
            .keyboard(false)
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.reply_rule_edit_dialog = None;
                        cx.notify();
                    });
                    true
                }
            })
            .footer(render_footer(&app, cx))
            .content({
                let app = app.clone();
                move |content, window, cx| {
                    let Some(entity) = app.upgrade() else {
                        return content;
                    };
                    // 阶段 1(可变): 同步 hex 编辑器、补齐懒创建输入、重建草稿
                    let _ = entity.update(cx, |app, cx| {
                        if let Some(state) = app.reply_rule_edit_dialog.as_mut() {
                            prepare(state, window, cx);
                        }
                    });
                    // 阶段 2(只读): 渲染
                    let theme = cx.theme().clone();
                    let content = content.child(render_body(&entity, window, cx));
                    // 「插入变量」浮层(作用于应答载荷)
                    let (show, bounds) = {
                        let st = entity.read(cx);
                        match st.reply_rule_edit_dialog.as_ref() {
                            Some(s) => (s.show_variable_picker, s.var_button_bounds),
                            None => (false, Bounds::default()),
                        }
                    };
                    if !show {
                        return content;
                    }
                    let dismiss_entity = entity.clone();
                    let pick_entity = entity.clone();
                    content.child(render_grouped_variable_picker(
                        reply_rule_variable_groups(),
                        bounds,
                        &theme,
                        Box::new(move |_e: &MouseDownEvent, _w: &mut Window, cx: &mut App| {
                            dismiss_entity.update(cx, |app, cx| {
                                if let Some(s) = app.reply_rule_edit_dialog.as_mut() {
                                    s.show_variable_picker = false;
                                }
                                cx.notify();
                            });
                        }),
                        Box::new(
                            move |item: &VariableItem, window: &mut Window, cx: &mut App| {
                                pick_entity.update(cx, |app, cx| {
                                    if let Some(s) = app.reply_rule_edit_dialog.as_mut() {
                                        let input = s.payload_input.clone();
                                        input.update(cx, |input, cx| {
                                            input.insert(item.insert_text.to_string(), window, cx);
                                        });
                                        s.show_variable_picker = false;
                                    }
                                    cx.notify();
                                });
                            },
                        ),
                    ))
                }
            })
    });
}

/// 渲染前的可变准备: hex 编辑器同步 + 懒创建字段输入 + 重建草稿
fn prepare(state: &mut ReplyRuleEditDialogState, window: &mut Window, cx: &mut App) {
    // hex 网格与文本输入同源: 先按最新文本重解析网格(与 timed_task 同款)
    if state.payload_hex_mode {
        hex_adapter::sync(&state.payload_hex_editor, &state.payload_input, cx);
    }
    if state.test_hex_mode {
        hex_adapter::sync(&state.test_hex_editor, &state.test_input, cx);
    }

    // 收集本帧需要的字段(先不可变借用, 结束后再插入, 避免同结构体双借用)
    let mut needed: Vec<(String, String)> = Vec::new();
    state.cond.collect_fields(&mut needed);
    // 字节模式字段走 InputWithMode + hex 网格(与应答载荷一致), 单独懒创建
    let mut byte_fields: Vec<(String, bool)> = Vec::new();
    state.cond.collect_byte_fields(&mut byte_fields);
    let byte_keys: HashMap<String, bool> = byte_fields.into_iter().collect();
    for (key, seed) in needed {
        if let Some(&hex_mode) = byte_keys.get(&key) {
            if !state.cond_byte_inputs.contains_key(&key) {
                let input = cx.new(|cx| {
                    EditorState::new(window, cx)
                        .language("json")
                        .line_number(false)
                        .folding(false)
                        .context_menu(false)
                });
                input.update(cx, |input, cx| {
                    input.set_value(seed, window, cx);
                });
                let hex_editor = cx.new(|cx| {
                    HexEditorState::with_inline_bytes_per_row(
                        cx,
                        hex_adapter::INLINE_BYTES_PER_ROW_WIDE,
                    )
                });
                state
                    .cond_byte_inputs
                    .insert(key.clone(), CondByteField { input, hex_editor });
            }
            // hex 模式下按最新文本重解析网格(与应答载荷同款)
            if hex_mode {
                if let Some(f) = state.cond_byte_inputs.get(&key) {
                    hex_adapter::sync(&f.hex_editor, &f.input, cx);
                }
            }
        } else if !state.cond_inputs.contains_key(&key) {
            let input = cx.new(|cx| InputState::new(window, cx));
            input.update(cx, |input, cx| {
                input.set_value(seed, window, cx);
            });
            state.cond_inputs.insert(key, input);
        }
    }

    // 指纹未变说明本帧无编辑: 跳过重建草稿与校验(消除每帧实时校验)
    let fp = state.fingerprint(cx);
    if state.last_fingerprint != Some(fp) {
        state.last_fingerprint = Some(fp);
        state.refresh_draft(cx);
    }
}

// ============================================================================
// 渲染
// ============================================================================

fn render_body(app: &Entity<NetAssistantApp>, window: &Window, cx: &App) -> Div {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let Some(s) = state.reply_rule_edit_dialog.as_ref() else {
        return div();
    };
    let issues = &s.issues;
    let errors: Vec<&crate::reply::model::Issue> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();

    let mut body = div().flex().flex_col().gap_4().px_6().pb_4();
    body = body.child(render_topbar(app, s, &theme));
    body = body.child(render_conditions(app, s, issues, &theme, window, cx));
    body = body.child(render_action(app, s, issues, &theme, window, cx));
    body = body.child(render_notes(s, &theme));
    body = body.child(render_test(app, s, &theme, window, cx));

    if let Some(err) = &s.error {
        let text = format!("{}: {}", t!("reply_rule_edit.cannot_save"), err);
        body = body.child(warn_line(text, &theme));
    }

    if !errors.is_empty() {
        let mut box_ = div()
            .p_2()
            .rounded_md()
            .bg(theme.danger.opacity(0.12))
            .border_1()
            .border_color(theme.danger)
            .flex()
            .flex_col()
            .gap_1();
        for issue in errors {
            box_ = box_.child(
                div()
                    .text_xs()
                    .whitespace_normal()
                    .text_color(theme.foreground)
                    .child(format!("[{}] {}", issue.path, issue.message)),
            );
        }
        body = body.child(box_);
    }

    div().max_h(dialog_content_max_height(window)).child(
        div()
            .id("reply-rule-edit-scroll")
            .overflow_y_scrollbar()
            .child(body),
    )
}

/// 分区标题
fn section_title(text: String, theme: &Theme) -> Div {
    div()
        .text_sm()
        .font_semibold()
        .text_color(theme.foreground)
        .child(text)
}

fn field_label(text: String, theme: &Theme) -> Div {
    div()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(text)
}

/// ===== 顶部属性条: 规则名 + 所属连接 + 启用 =====
///
/// 这三项决定"这条规则是什么、属于哪个连接、是否参与求值", 是编辑时最先要确认的
/// 属性; 抽到顶部固定区, 让下面的条件/动作构建器专注"匹配什么、回什么"。
///
/// 规则严格属于单一连接(不再有全局作用域), 因此这里只静态展示所属连接地址。
fn render_topbar(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(theme.primary.opacity(0.4))
        .bg(theme.secondary)
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(t!("reply_rule_edit.name").to_string(), theme))
                .child(input_box(&s.name_input, theme)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .flex_wrap()
                .child(field_label(
                    t!("reply_rule_edit.belongs_to").to_string(),
                    theme,
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(s.tab_label.clone()),
                )
                .child(
                    div()
                        .ml_auto()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(field_label(
                            t!("reply_rule_edit.enabled").to_string(),
                            theme,
                        ))
                        .child(
                            Switch::new("reply-rule-edit-enabled-switch")
                                .checked(s.enabled)
                                .with_size(Size::Small)
                                .on_change({
                                    let entity = app.clone();
                                    move |_next, _window, cx| {
                                        entity.update(cx, |app, cx| {
                                            if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                                                st.enabled = !st.enabled;
                                            }
                                            cx.notify();
                                        });
                                    }
                                }),
                        ),
                ),
        )
}

/// ===== 备注与标签(非必填次要信息, 下沉到动作之后) =====
fn render_notes(s: &ReplyRuleEditDialogState, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_title(
            t!("reply_rule_edit.section_notes").to_string(),
            theme,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    t!("reply_rule_edit.description").to_string(),
                    theme,
                ))
                .child(input_box(&s.desc_input, theme)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(t!("reply_rule_edit.tags").to_string(), theme))
                .child(input_box(&s.tags_input, theme)),
        )
}

/// ===== B 条件构建器(平铺 + 缩进) =====
fn render_conditions(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    issues: &[crate::reply::model::Issue],
    theme: &Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let mut rows = Vec::new();
    collect_rows(&s.cond, Vec::new(), "matcher".to_string(), 0, &mut rows);

    // 匹配模式二选一 chip: 「任意消息」隐藏条件树, 「自定义条件」展示条件树
    let mode_chip = |mode: MatchMode| -> Div {
        let label = if mode == MatchMode::Any {
            t!("reply_rule_edit.match_mode_any").to_string()
        } else {
            t!("reply_rule_edit.match_mode_custom").to_string()
        };
        chip(
            app,
            label,
            s.match_mode == mode,
            false,
            theme,
            move |app, _window, _cx| {
                if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                    // 切换只改模式, 不清空 cond 草稿(切回「自定义条件」时仍在)
                    st.match_mode = mode;
                }
            },
        )
    };

    let mut section = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_title(
            t!("reply_rule_edit.section_condition").to_string(),
            theme,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(field_label(
                    t!("reply_rule_edit.match_mode").to_string(),
                    theme,
                ))
                .child(mode_chip(MatchMode::Any))
                .child(mode_chip(MatchMode::Custom)),
        );

    match s.match_mode {
        MatchMode::Any => {
            // 「任意消息」: 不渲染条件树, 仅提示"匹配全部报文"
            section = section.child(
                div()
                    .text_xs()
                    .whitespace_normal()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{}: {}",
                        t!("reply_rule_edit.summary"),
                        t!("reply_rule_edit.match_all_hint")
                    )),
            );
        }
        MatchMode::Custom => {
            let summary = format_matcher_summary(&s.draft.matcher).join(" 且 ");
            let mut list = div().flex().flex_col().gap_1();
            for row in &rows {
                list = list.child(render_cond_row(app, s, row, issues, theme, window, cx));
            }
            section = section
                .child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(theme.muted_foreground)
                        .child(format!("{}: {}", t!("reply_rule_edit.summary"), summary)),
                )
                .child(list);
        }
    }

    section
}

/// 展平条件树(带显式括号标记)
fn collect_rows(
    node: &CondDraft,
    path: Vec<usize>,
    issue_prefix: String,
    depth: usize,
    out: &mut Vec<CondRow>,
) {
    if node.kind.is_combinator() {
        out.push(CondRow {
            path: path.clone(),
            issue_prefix: issue_prefix.clone(),
            depth,
            role: CondRole::Open,
            node: node.clone(),
        });
        for (i, child) in node.children.iter().enumerate() {
            let child_prefix = if node.kind == CondKind::Not {
                format!("{}.child", issue_prefix)
            } else {
                format!("{}.children[{}]", issue_prefix, i)
            };
            let mut child_path = path.clone();
            child_path.push(i);
            collect_rows(child, child_path, child_prefix, depth + 1, out);
        }
        out.push(CondRow {
            path,
            issue_prefix,
            depth,
            role: CondRole::Close,
            node: node.clone(),
        });
    } else {
        out.push(CondRow {
            path,
            issue_prefix,
            depth,
            role: CondRole::Leaf,
            node: node.clone(),
        });
    }
}

fn render_cond_row(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    row: &CondRow,
    issues: &[crate::reply::model::Issue],
    theme: &Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let indent = px((row.depth as f32) * 16.0);
    let path = row.path.clone();
    let uid = row.node.uid;

    // 该节点(含子树)上的校验问题: 按 issue path 前缀过滤
    let mut node_issues: Vec<String> = issues
        .iter()
        .filter(|i| {
            i.path == row.issue_prefix || i.path.starts_with(&format!("{}.", row.issue_prefix))
        })
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect();
    node_issues.dedup();

    let mut line = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .pl(indent)
        .child(
            div()
                .text_xs()
                .font_medium()
                .font_family("JetBrains Mono")
                .text_color(theme.primary)
                .child(match row.role {
                    CondRole::Open => format!("{} {{", row.node.kind.label()),
                    CondRole::Close => "}".to_string(),
                    CondRole::Leaf => row.node.kind.label(),
                }),
        );

    if row.role == CondRole::Open {
        // 组合节点的子项计数与"添加条件"
        line = line.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("{}", row.node.children.len())),
        );
    }

    // 删除按钮(根节点不可删)
    if !row.path.is_empty() {
        line = line.child(chip(
            app,
            "✕".to_string(),
            false,
            true,
            theme,
            move |app, _window, _cx| {
                if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                    let mut root =
                        std::mem::replace(&mut st.cond, CondDraft::new(0, CondKind::All));
                    cond_remove(&mut root, &path);
                    st.cond = root;
                    st.add_menu_open = None;
                }
            },
        ));
    }

    let mut out = div()
        .flex()
        .flex_col()
        .gap_1()
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .child(line);

    match row.role {
        CondRole::Open => {
            out = out.child(render_add_menu(app, s, &row.path, uid, theme));
        }
        CondRole::Close => {}
        CondRole::Leaf => {
            out = out.child(render_leaf_fields(app, s, &row.node, theme, window, cx));
        }
    }

    // 每个节点旁显示它自己的校验问题(红字)
    for msg in node_issues.iter().take(3) {
        out = out.child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(theme.danger)
                .child(msg.clone()),
        );
    }

    out
}

/// 「添加条件」菜单(展开后列出全部 15 种节点)
fn render_add_menu(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    path: &[usize],
    uid: u64,
    theme: &Theme,
) -> Div {
    let open = s.add_menu_open == Some(uid);
    let target = path.to_vec();
    let entity = app.clone();
    let toggle = chip(
        app,
        if open {
            t!("reply_rule_edit.add_condition_close").to_string()
        } else {
            t!("reply_rule_edit.add_condition").to_string()
        },
        open,
        false,
        theme,
        move |app, _window, _cx| {
            if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                st.add_menu_open = if st.add_menu_open == Some(uid) {
                    None
                } else {
                    Some(uid)
                };
            }
        },
    );
    let mut menu = div().flex().flex_col().gap_1().child(toggle);
    if open {
        menu = menu.child(
            div().flex().flex_wrap().gap_1().children(
                CondKind::ALL
                    .into_iter()
                    .map(|kind| {
                        let target = target.clone();
                        let entity = entity.clone();
                        chip_owned(
                            entity,
                            kind.label(),
                            false,
                            false,
                            theme,
                            move |app, _window, _cx| {
                                if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                                    let new_uid = st.alloc_uid();
                                    let mut root = std::mem::replace(
                                        &mut st.cond,
                                        CondDraft::new(0, CondKind::All),
                                    );
                                    cond_add_child(&mut root, &target, kind, new_uid);
                                    st.cond = root;
                                    st.add_menu_open = None;
                                }
                            },
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
        );
    }
    menu
}

/// 条件叶子的各字段控件(按种类)
fn render_leaf_fields(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    node: &CondDraft,
    theme: &Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let uid = node.uid;
    let mut grid = div().flex().flex_wrap().gap_2();
    let push = |field: &str, label_key: &str, width: f32, grid: Div| -> Div {
        grid.child(text_field(
            t!(label_key).to_string(),
            key_of(uid, field),
            s,
            theme,
            width,
            cx,
        ))
    };

    match node.kind {
        CondKind::Contains | CondKind::Suffix => {
            grid = grid.child(render_byte_field(
                app,
                s,
                node,
                "bytes",
                "reply_rule_edit.field_bytes",
                theme,
                window,
                cx,
            ));
        }
        CondKind::FixedExact => {
            grid = push("len", "reply_rule_edit.field_len", 100.0, grid);
            grid = grid.child(render_byte_field(
                app,
                s,
                node,
                "bytes",
                "reply_rule_edit.field_bytes",
                theme,
                window,
                cx,
            ));
        }
        CondKind::FixedMask => {
            grid = push("len", "reply_rule_edit.field_len", 100.0, grid);
            grid = grid.child(render_byte_field(
                app,
                s,
                node,
                "bytes",
                "reply_rule_edit.field_bytes",
                theme,
                window,
                cx,
            ));
            grid = push("mask", "reply_rule_edit.field_mask", 320.0, grid);
        }
        CondKind::PrefixRange => {
            grid = grid.child(render_byte_field(
                app,
                s,
                node,
                "prefix",
                "reply_rule_edit.field_prefix",
                theme,
                window,
                cx,
            ));
            grid = push("min_len", "reply_rule_edit.field_min_len", 100.0, grid);
            grid = push("max_len", "reply_rule_edit.field_max_len", 100.0, grid);
        }
        CondKind::Length => {
            grid = push("min", "reply_rule_edit.field_min", 100.0, grid);
            grid = push("max", "reply_rule_edit.field_max", 100.0, grid);
        }
        CondKind::ByteAt => {
            grid = push("offset", "reply_rule_edit.field_offset", 100.0, grid);
            // 操作种类
            let mut ops = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(field_label(
                    t!("reply_rule_edit.field_op").to_string(),
                    theme,
                ));
            for op in OpKind::ALL {
                let entity = app.clone();
                ops = ops.child(chip_owned(
                    entity,
                    op.label(),
                    node.op == op,
                    false,
                    theme,
                    move |app, _window, _cx| {
                        set_node_field(app, uid, |n| n.op = op);
                    },
                ));
            }
            grid = grid.child(ops);
            match node.op {
                OpKind::Eq | OpKind::Ne => {
                    grid = push("op_value", "reply_rule_edit.field_byte_value", 100.0, grid);
                }
                OpKind::Masked => {
                    grid = push("op_mask", "reply_rule_edit.field_byte_mask", 100.0, grid);
                    grid = push("op_value", "reply_rule_edit.field_byte_value", 100.0, grid);
                }
                OpKind::In => {
                    grid = push("op_values", "reply_rule_edit.field_values", 240.0, grid);
                }
            }
        }
        CondKind::ScalarAt => {
            grid = push("offset", "reply_rule_edit.field_offset", 100.0, grid);
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_width").to_string(),
                node.width,
                &width_options(),
                |n, v| n.width = v,
                theme,
            ));
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_endian").to_string(),
                node.endian,
                &[
                    (Endian::Big, "BE".to_string()),
                    (Endian::Little, "LE".to_string()),
                ],
                |n, v| n.endian = v,
                theme,
            ));
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_signed").to_string(),
                node.signed,
                &[(false, "u".to_string()), (true, "i".to_string())],
                |n, v| n.signed = v,
                theme,
            ));
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_cmp").to_string(),
                node.cmp,
                &cmp_options(),
                |n, v| n.cmp = v,
                theme,
            ));
            grid = push("value", "reply_rule_edit.field_value", 120.0, grid);
        }
        CondKind::Regex => {
            grid = push("pattern", "reply_rule_edit.field_pattern", 420.0, grid);
        }
        CondKind::From => {
            grid = push("addrs", "reply_rule_edit.field_addrs", 420.0, grid);
        }
        CondKind::FieldEq => {
            grid = push("accessor", "reply_rule_edit.field_accessor", 120.0, grid);
            grid = push("offset", "reply_rule_edit.field_offset", 100.0, grid);
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_cmp").to_string(),
                node.cmp,
                &cmp_options(),
                |n, v| n.cmp = v,
                theme,
            ));
            grid = push("value", "reply_rule_edit.field_value", 120.0, grid);
        }
        CondKind::ChecksumValid => {
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_algorithm").to_string(),
                node.algorithm,
                &ChecksumAlgorithm::ALL.map(|a| (a, a.compact_name().to_string())),
                |n, v| n.algorithm = v,
                theme,
            ));
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_at").to_string(),
                node.at_trailing,
                &[
                    (false, t!("reply_rule_edit.at_fixed").to_string()),
                    (true, t!("reply_rule_edit.at_trailing").to_string()),
                ],
                |n, v| n.at_trailing = v,
                theme,
            ));
            grid = grid.child(enum_chips(
                app,
                uid,
                t!("reply_rule_edit.field_width").to_string(),
                node.width,
                &width_options(),
                |n, v| n.width = v,
                theme,
            ));
            if node.at_trailing {
                grid = push("at_n", "reply_rule_edit.field_n", 100.0, grid);
            } else {
                grid = push("at_offset", "reply_rule_edit.field_offset", 100.0, grid);
            }
            grid = push("range", "reply_rule_edit.field_range", 160.0, grid);
        }
        CondKind::All | CondKind::Any | CondKind::Not => {}
    }

    // PrefixRange 的约束列表
    if node.kind == CondKind::PrefixRange {
        let path = node_path(&s.cond, uid);
        let open = s.add_constraint_open.as_ref() == Some(&path);
        let mut wrap = div().flex().flex_col().gap_1().child(field_label(
            t!("reply_rule_edit.field_constraints").to_string(),
            theme,
        ));
        for (i, c) in node.constraints.iter().enumerate() {
            let c_path = path.clone();
            let entity = app.clone();
            let mut row = div().flex().flex_row().items_center().gap_1().child(
                div()
                    .text_xs()
                    .text_color(theme.primary)
                    .child(c.kind.label()),
            );
            for field in c.field_names() {
                row = row.child(text_field(
                    t!(constraint_label_key(c.kind, field)).to_string(),
                    key_of(c.uid, field),
                    s,
                    theme,
                    if field == "value" { 90.0 } else { 120.0 },
                    cx,
                ));
            }
            row = row.child(chip_owned(
                entity,
                "✕".to_string(),
                false,
                true,
                theme,
                move |app, _window, _cx| {
                    if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                        let mut root =
                            std::mem::replace(&mut st.cond, CondDraft::new(0, CondKind::All));
                        cond_remove_constraint(&mut root, &c_path, i);
                        st.cond = root;
                    }
                },
            ));
            wrap = wrap.child(row);
        }
        let path_for_menu = path.clone();
        let open_now = open;
        wrap = wrap.child(chip_owned(
            app.clone(),
            if open_now {
                t!("reply_rule_edit.add_constraint_close").to_string()
            } else {
                t!("reply_rule_edit.add_constraint").to_string()
            },
            open_now,
            false,
            theme,
            move |app, _window, _cx| {
                if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                    st.add_constraint_open =
                        if st.add_constraint_open.as_ref() == Some(&path_for_menu) {
                            None
                        } else {
                            Some(path_for_menu.clone())
                        };
                }
            },
        ));
        if open_now {
            let mut menu = div().flex().flex_wrap().gap_1();
            for kind in ConstraintKind::ALL {
                let path = path.clone();
                menu = menu.child(chip_owned(
                    app.clone(),
                    kind.label(),
                    false,
                    false,
                    theme,
                    move |app, _window, _cx| {
                        if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                            let uid = st.alloc_uid();
                            let mut root =
                                std::mem::replace(&mut st.cond, CondDraft::new(0, CondKind::All));
                            if let Some(n) = cond_at_mut(&mut root, &path) {
                                n.constraints.push(ConstraintDraft::new(uid, kind));
                            }
                            st.cond = root;
                            st.add_constraint_open = None;
                        }
                    },
                ));
            }
            wrap = wrap.child(menu);
        }
        grid = grid.child(wrap);
    }

    grid
}

/// 字节模式字段(Hex/文本): 标签 + 模式切换 + `InputWithMode`(与应答载荷一致)
fn render_byte_field(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    node: &CondDraft,
    field: &str,
    label_key: &str,
    theme: &Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let uid = node.uid;
    let key = key_of(uid, field);
    let mode_hex = node.hex_mode;

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(field_label(t!(label_key).to_string(), theme))
        .child(chip_owned(
            app.clone(),
            t!("reply_rule_edit.mode_hex").to_string(),
            mode_hex,
            false,
            theme,
            move |app, window, cx| switch_cond_byte_mode(app, uid, true, window, cx),
        ))
        .child(chip_owned(
            app.clone(),
            t!("reply_rule_edit.mode_text").to_string(),
            !mode_hex,
            false,
            theme,
            move |app, window, cx| switch_cond_byte_mode(app, uid, false, window, cx),
        ));

    let editor: AnyElement = match s.cond_byte_inputs.get(&key) {
        Some(f) => InputWithMode::render(
            &f.input,
            Some(&f.hex_editor),
            if mode_hex { "hex" } else { "text" },
            theme,
            window,
            cx,
        )
        .into_any_element(),
        // 实体在 prepare 阶段懒创建, 正常渲染时必然存在; 兜底占位不丢布局
        None => div().w_full().h(px(32.0)).into_any_element(),
    };

    div()
        .flex()
        .flex_col()
        .gap_1()
        .w_full()
        .child(header)
        .child(editor)
}

/// 切换条件字节字段的 Hex/文本模式(转换型语义: 内容整体互转, 与应答载荷一致)
fn switch_cond_byte_mode(
    app: &mut NetAssistantApp,
    uid: u64,
    to_hex: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(st) = app.reply_rule_edit_dialog.as_mut() else {
        return;
    };
    // 取字段 key 与当前模式(此处借用 cond, 结束后释放)
    let (key, from_hex) = {
        let path = node_path(&st.cond, uid);
        let Some(node) = cond_at_mut(&mut st.cond, &path) else {
            return;
        };
        if node.hex_mode == to_hex {
            return;
        }
        let Some(field) = node.byte_field() else {
            return;
        };
        (key_of(uid, field), node.hex_mode)
    };
    let Some(input) = st.cond_byte_inputs.get(&key).map(|f| f.input.clone()) else {
        return;
    };
    let value = input.read(cx).value().to_string();
    let converted = crate::utils::hex::convert_value(
        &value,
        if from_hex { "hex" } else { "text" },
        if to_hex { "hex" } else { "text" },
    );
    // hex → text 且内容非法时不切换(不擅自改动用户内容)
    if converted.is_none() && !to_hex {
        return;
    }
    set_node_field(app, uid, |n| n.hex_mode = to_hex);
    if let Some(next) = converted {
        let next = if to_hex {
            hex_adapter::normalize_hex_value(&next).unwrap_or(next)
        } else {
            next
        };
        input.update(cx, |input, cx| input.replace_all(next, window, cx));
    }
}

fn constraint_label_key(kind: ConstraintKind, field: &str) -> &'static str {
    match (kind, field) {
        (_, "offset") => "reply_rule_edit.field_offset",
        (_, "value") => "reply_rule_edit.field_byte_value",
        (_, "mask") => "reply_rule_edit.field_byte_mask",
        (_, "values") => "reply_rule_edit.field_values",
        (_, "len_offset") => "reply_rule_edit.field_len_offset",
        (_, "base") => "reply_rule_edit.field_base",
        _ => "reply_rule_edit.field_value",
    }
}

/// 找到 uid 对应的节点路径(约束列表的增删需要路径)
fn node_path(root: &CondDraft, uid: u64) -> Vec<usize> {
    fn walk(node: &CondDraft, uid: u64, path: &mut Vec<usize>) -> bool {
        if node.uid == uid {
            return true;
        }
        for (i, child) in node.children.iter().enumerate() {
            path.push(i);
            if walk(child, uid, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    if walk(root, uid, &mut path) {
        path
    } else {
        Vec::new()
    }
}

/// 就地修改 uid 对应节点的非文本字段
fn set_node_field(app: &mut NetAssistantApp, uid: u64, f: impl FnOnce(&mut CondDraft)) {
    let Some(st) = app.reply_rule_edit_dialog.as_mut() else {
        return;
    };
    let path = node_path(&st.cond, uid);
    if let Some(node) = cond_at_mut(&mut st.cond, &path) {
        f(node);
    }
}

/// 宽度候选(多处复用)
fn width_options() -> [(Width, String); 4] {
    [
        (Width::U8, "u8".to_string()),
        (Width::U16, "u16".to_string()),
        (Width::U32, "u32".to_string()),
        (Width::U64, "u64".to_string()),
    ]
}

/// 比较符候选(多处复用)
fn cmp_options() -> [(Cmp, String); 6] {
    [
        (Cmp::Eq, "==".to_string()),
        (Cmp::Ne, "!=".to_string()),
        (Cmp::Gt, ">".to_string()),
        (Cmp::Ge, ">=".to_string()),
        (Cmp::Lt, "<".to_string()),
        (Cmp::Le, "<=".to_string()),
    ]
}

/// 一组枚举 chip(单选): 宽度 / 字节序 / 比较符 / 算法 / 校验位位置
fn enum_chips<T: Copy + PartialEq + 'static>(
    app: &Entity<NetAssistantApp>,
    uid: u64,
    label: String,
    current: T,
    options: &[(T, String)],
    set: impl Fn(&mut CondDraft, T) + Copy + 'static,
    theme: &Theme,
) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(field_label(label, theme));
    for (value, text) in options.iter() {
        let value = *value;
        row = row.child(chip_owned(
            app.clone(),
            text.clone(),
            current == value,
            false,
            theme,
            move |app, _window, _cx| {
                set_node_field(app, uid, |n| set(n, value));
            },
        ));
    }
    row
}

/// 渲染一个带标签的文本输入(值住在 `cond_inputs` 的缓存实体里)
fn text_field(
    label: String,
    key: String,
    s: &ReplyRuleEditDialogState,
    theme: &Theme,
    width: f32,
    _cx: &App,
) -> Div {
    let mut cell = div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(field_label(label, theme));
    match s.cond_inputs.get(&key) {
        Some(entity) => {
            cell = cell.child(
                div()
                    .w(px(width))
                    .h_7()
                    .bg(theme.background)
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        Input::new(entity)
                            .w_full()
                            .h_full()
                            .bg(theme.background)
                            .rounded_md()
                            .border_0(),
                    ),
            );
        }
        None => {
            cell = cell.child(
                div()
                    .w(px(width))
                    .h_7()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border),
            );
        }
    }
    cell
}

/// ===== C 动作 =====
fn render_action(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    issues: &[crate::reply::model::Issue],
    theme: &Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let mut body = div().flex().flex_col().gap_2().child(section_title(
        t!("reply_rule_edit.section_action").to_string(),
        theme,
    ));

    // 动作只有一个变体：回复（匹配到就回这段内容）
    {
        // 模式切换
        let entity = app.clone();
        let mode_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(field_label(
                t!("reply_rule_edit.byte_mode").to_string(),
                theme,
            ))
            .child(chip_owned(
                entity.clone(),
                t!("reply_rule_edit.mode_text").to_string(),
                !s.payload_hex_mode,
                false,
                theme,
                move |app, window, cx| {
                    switch_payload_mode(app, false, window, cx);
                },
            ))
            .child(chip_owned(
                entity,
                t!("reply_rule_edit.mode_hex").to_string(),
                s.payload_hex_mode,
                false,
                theme,
                move |app, window, cx| {
                    switch_payload_mode(app, true, window, cx);
                },
            ))
            .child({
                // 「插入变量」按钮: on_prepaint 记录 bounds 供浮层锚定
                let prepaint_entity = app.clone();
                let handler: Box<dyn Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static> =
                    Box::new(move |bounds, _window, cx| {
                        prepaint_entity.update(cx, |app, _| {
                            if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                                st.var_button_bounds = bounds;
                            }
                        });
                    });
                let toggle_entity = app.clone();
                div()
                    .on_prepaint(handler)
                    .id("reply-rule-insert-var")
                    .ml_auto()
                    .px_1p5()
                    .py_0p5()
                    .rounded_md()
                    .text_xs()
                    .font_medium()
                    .cursor_pointer()
                    .text_color(theme.primary)
                    .bg(theme.primary.opacity(0.06))
                    .hover(|d| d.text_color(theme.primary_foreground).bg(theme.primary))
                    .child(t!("reply_rule_edit.insert_variable").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        toggle_entity.update(cx, |app, cx| {
                            if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                                st.show_variable_picker = !st.show_variable_picker;
                            }
                            cx.notify();
                        });
                    })
            });

        body = body.child(mode_row);
        body = body.child(InputWithMode::render(
            &s.payload_input,
            Some(&s.payload_hex_editor),
            if s.payload_hex_mode { "hex" } else { "text" },
            theme,
            window,
            cx,
        ));

        // 编码方式
        let mut codec_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(field_label(t!("reply_rule_edit.codec").to_string(), theme));
        for codec in RuleCodec::ALL {
            let label = match codec {
                RuleCodec::Inherit => t!("reply_rule_edit.codec_inherit").to_string(),
                RuleCodec::Raw => t!("reply_rule_edit.codec_raw").to_string(),
                RuleCodec::Lf => t!("reply_rule_edit.codec_lf").to_string(),
                RuleCodec::Crlf => t!("reply_rule_edit.codec_crlf").to_string(),
            };
            codec_row = codec_row.child(chip_owned(
                app.clone(),
                label,
                s.payload_codec == codec,
                false,
                theme,
                move |app, _window, _cx| {
                    if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                        st.payload_codec = codec;
                    }
                },
            ));
        }
        body = body.child(codec_row);

        if s.payload_hex_mode && s.payload_codec != RuleCodec::Raw {
            body = body.child(warn_line(
                t!("reply_rule_edit.hex_codec_warning").to_string(),
                theme,
            ));
        }
        body = body.child(field_label(
            t!("reply_rule_edit.codec_hint").to_string(),
            theme,
        ));
    }

    // 载荷级校验问题
    for issue in issues.iter().filter(|i| i.path.starts_with("payload")) {
        body = body.child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(if issue.severity == Severity::Error {
                    theme.danger
                } else {
                    theme.warning
                })
                .child(format!("{}: {}", issue.path, issue.message)),
        );
    }

    body
}

/// 切换应答载荷的文本/hex 模式(转换型语义: 内容整体互转)
fn switch_payload_mode(app: &mut NetAssistantApp, hex: bool, window: &mut Window, cx: &mut App) {
    let Some(st) = app.reply_rule_edit_dialog.as_mut() else {
        return;
    };
    if st.payload_hex_mode == hex {
        return;
    }
    let value = st.payload_input.read(cx).text().to_string();
    let converted = crate::utils::hex::convert_value(
        &value,
        if hex { "text" } else { "hex" },
        if hex { "hex" } else { "text" },
    );
    // hex → text 且内容非法时不切换(不擅自改动用户内容)
    if converted.is_none() && !hex {
        return;
    }
    st.payload_hex_mode = hex;
    if let Some(next) = converted {
        let next = if hex {
            hex_adapter::normalize_hex_value(&next).unwrap_or(next)
        } else {
            next
        };
        let input = st.payload_input.clone();
        input.update(cx, |input, cx| input.replace_all(next, window, cx));
    }
}

/// ===== D 内联试跑 =====
fn render_test(
    app: &Entity<NetAssistantApp>,
    s: &ReplyRuleEditDialogState,
    theme: &Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let entity = app.clone();
    let mode_hex = s.test_hex_mode;
    let mode_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(field_label(
            t!("reply_rule_edit.byte_mode").to_string(),
            theme,
        ))
        .child(chip_owned(
            entity.clone(),
            t!("reply_rule_edit.mode_text").to_string(),
            !mode_hex,
            false,
            theme,
            move |app, _window, _cx| {
                if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                    st.test_hex_mode = false;
                }
            },
        ))
        .child(chip_owned(
            entity.clone(),
            t!("reply_rule_edit.mode_hex").to_string(),
            mode_hex,
            false,
            theme,
            move |app, _window, _cx| {
                if let Some(st) = app.reply_rule_edit_dialog.as_mut() {
                    st.test_hex_mode = true;
                }
            },
        ))
        .child({
            let run_entity = app.clone();
            div()
                .id("reply-rule-run-test")
                .ml_auto()
                .px_3()
                .py_1()
                .rounded_md()
                .text_xs()
                .font_medium()
                .cursor_pointer()
                .bg(theme.primary)
                .text_color(theme.primary_foreground)
                .child(t!("reply_rule_edit.run_test").to_string())
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    run_entity.update(cx, |app, cx| {
                        run_test(app, cx);
                        cx.notify();
                    });
                })
        });

    let mut body = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_title(
            t!("reply_rule_edit.section_test").to_string(),
            theme,
        ))
        .child(field_label(
            t!("reply_rule_edit.test_frame").to_string(),
            theme,
        ))
        .child(InputWithMode::render(
            &s.test_input,
            Some(&s.test_hex_editor),
            if mode_hex { "hex" } else { "text" },
            theme,
            window,
            cx,
        ))
        .child(mode_row)
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(
                    t!("reply_rule_edit.test_source").to_string(),
                    theme,
                ))
                .child(div().w(px(240.0)).child(input_box(&s.test_source, theme))),
        );

    match &s.test_result {
        None => {
            body = body.child(field_label(
                t!("reply_rule_edit.test_no_result").to_string(),
                theme,
            ));
        }
        Some(result) => {
            if let Some(err) = &result.error {
                body = body.child(warn_line(err.clone(), theme));
            }
            body = body.child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(if result.hit {
                        theme.success
                    } else {
                        theme.danger
                    })
                    .child(if result.hit {
                        format!(
                            "{} {}",
                            t!("reply_rule_edit.test_hit"),
                            result.hit_rule_name.clone().unwrap_or_default()
                        )
                    } else {
                        t!("reply_rule_edit.test_miss").to_string()
                    }),
            );
            if let Some((label, actual)) = &result.first_failure {
                body = body.child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(theme.danger)
                        .child(format!(
                            "{}: {} / {} {}",
                            t!("reply_rule_edit.test_first_failure"),
                            label,
                            t!("reply_rule_edit.test_actual"),
                            actual
                        )),
                );
            }
            let mut trace = div().flex().flex_col().gap_0p5();
            for (label, actual, res) in &result.rows {
                let (mark, color) = match res {
                    PredicateResult::Pass => ("✓", theme.success),
                    PredicateResult::Fail => ("✗", theme.danger),
                    PredicateResult::Skipped => ("–", theme.muted_foreground),
                };
                trace = trace.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(div().text_xs().text_color(color).child(mark))
                        .child(
                            div()
                                .text_xs()
                                .font_family("JetBrains Mono")
                                .text_color(theme.foreground)
                                .child(label.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(actual.clone()),
                        ),
                );
            }
            body = body.child(trace);

            // 渲染出的应答
            let rendered_text = match &result.rendered {
                Some(bytes) if !bytes.is_empty() => bytes_to_hex(bytes),
                _ => t!("reply_rule_edit.test_rendered_empty").to_string(),
            };
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(field_label(
                        t!("reply_rule_edit.test_rendered").to_string(),
                        theme,
                    ))
                    .child(
                        div()
                            .text_xs()
                            .font_family("JetBrains Mono")
                            .whitespace_normal()
                            .text_color(theme.foreground)
                            .child(rendered_text),
                    ),
            );

            // 变量清单
            let vars = if result.variables.is_empty() {
                t!("reply_rule_edit.test_vars_empty").to_string()
            } else {
                result
                    .variables
                    .iter()
                    .map(|v| format!("{} = <按接收帧取值>", v))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(field_label(
                        t!("reply_rule_edit.test_vars").to_string(),
                        theme,
                    ))
                    .child(
                        div()
                            .text_xs()
                            .font_family("JetBrains Mono")
                            .whitespace_normal()
                            .text_color(theme.muted_foreground)
                            .child(vars),
                    ),
            );
        }
    }

    body
}

/// 执行试跑: 用草稿单独构造一个临时 store, 走与网络路径同一个 `dry_run`
fn run_test(app: &mut NetAssistantApp, cx: &mut App) {
    let Some(st) = app.reply_rule_edit_dialog.as_mut() else {
        return;
    };
    let rule = st.draft.clone();
    let raw = st.test_input.read(cx).text().to_string();
    let source_text = st.test_source.read(cx).value().to_string();
    let hex_mode = st.test_hex_mode;

    let bytes = match parse_bytes(&raw, hex_mode) {
        Ok(b) => b,
        Err(e) => {
            st.test_result = Some(TestRunResult {
                tested_rule: rule,
                hit: false,
                hit_rule_name: None,
                rows: Vec::new(),
                first_failure: None,
                rendered: None,
                variables: Vec::new(),
                error: Some(e),
            });
            return;
        }
    };

    let source: SocketAddr = source_text
        .trim()
        .parse()
        .unwrap_or_else(|_| "127.0.0.1:12345".parse().expect("静态地址可解析"));

    // 临时 store: 规则集未保存也要能试跑(草稿只在内存里)
    let store = crate::reply::ReplyRulesStore::new();
    let mut connections = std::collections::HashMap::new();
    connections.insert("tab".to_string(), vec![rule.clone()]);
    store.replace(&ReplyRulesConfig {
        connections,
        ..Default::default()
    });
    let frame: Arc<crate::reply::RxFrame> = Arc::new(crate::reply::RxFrame::new(
        bytes,
        source,
        crate::reply::FrameMeta::decoded(),
    ));
    let rules = store.enabled_rules();
    let (outcome, rendered) = crate::reply::exec::dry_run(&store, rules.as_slice(), &frame);

    let rows = outcome
        .trace
        .entries
        .first()
        .map(crate::reply::exec::trace_rows)
        .unwrap_or_default();
    let first_failure = outcome
        .trace
        .entries
        .first()
        .and_then(|e| e.first_failure())
        .map(|p| (p.label.clone(), p.actual.clone()));
    let hit_rule_name = outcome
        .trace
        .entries
        .first()
        .filter(|e| e.hit)
        .map(|e| e.rule_name.clone());
    let variables = crate::reply::exec::rx_variables(&rule.payload);

    st.test_result = Some(TestRunResult {
        tested_rule: rule,
        hit: outcome.is_hit(),
        hit_rule_name,
        rows,
        first_failure,
        rendered,
        variables,
        error: None,
    });
}

fn parse_bytes(text: &str, hex_mode: bool) -> Result<Vec<u8>, String> {
    if hex_mode {
        if !validate_hex_input(text) {
            return Err(t!("input_mode.hex_invalid").to_string());
        }
        Ok(hex_to_bytes(text))
    } else {
        Ok(text.as_bytes().to_vec())
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

// ============================================================================
// 小部件
// ============================================================================

fn input_box(entity: &Entity<InputState>, theme: &Theme) -> Div {
    div()
        .w_full()
        .h_7()
        .bg(theme.background)
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .child(
            Input::new(entity)
                .w_full()
                .h_full()
                .bg(theme.background)
                .rounded_md()
                .border_0(),
        )
}

fn warn_line(text: String, theme: &Theme) -> Div {
    div()
        .p_2()
        .rounded_md()
        .bg(theme.warning.opacity(0.12))
        .border_1()
        .border_color(theme.warning)
        .child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(theme.foreground)
                .child(text),
        )
}

/// 单选 chip(自带实体)
fn chip(
    app: &Entity<NetAssistantApp>,
    label: String,
    selected: bool,
    danger: bool,
    theme: &Theme,
    on_click: impl Fn(&mut NetAssistantApp, &mut Window, &mut App) + 'static,
) -> Div {
    chip_owned(app.clone(), label, selected, danger, theme, on_click)
}

/// 单选 chip(消费实体, 便于在循环里 clone)
fn chip_owned(
    entity: Entity<NetAssistantApp>,
    label: String,
    selected: bool,
    danger: bool,
    theme: &Theme,
    on_click: impl Fn(&mut NetAssistantApp, &mut Window, &mut App) + 'static,
) -> Div {
    let primary = theme.primary;
    let primary_fg = theme.primary_foreground;
    let border = theme.border;
    let fg = theme.foreground;
    let danger_color = theme.danger;
    div()
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .cursor_pointer()
        .when(selected, |d| {
            if danger {
                d.bg(danger_color).text_color(primary_fg)
            } else {
                d.bg(primary).text_color(primary_fg)
            }
        })
        .when(!selected, |d| d.bg(border).text_color(fg))
        .hover(move |d| d.bg(primary).text_color(primary_fg))
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            entity.update(cx, |app, cx| on_click(app, window, cx));
        })
}

/// 底部按钮: 保存 / 取消
fn render_footer(app: &WeakEntity<NetAssistantApp>, cx: &App) -> DialogFooter {
    let can_save = app
        .upgrade()
        .and_then(|e| {
            e.read(cx)
                .reply_rule_edit_dialog
                .as_ref()
                .map(|s| !s.issues.iter().any(|i| i.severity == Severity::Error))
        })
        .unwrap_or(false);

    let app_cancel = app.clone();
    let cancel = Button::new("reply-rule-edit-cancel")
        .outline()
        .label(t!("reply_rule_edit.cancel").to_string())
        .on_click(move |_, window, cx| {
            let _ = app_cancel.update(cx, |app, cx| {
                app.reply_rule_edit_dialog = None;
                cx.notify();
            });
            window.close_dialog(cx);
        });

    let app_save = app.clone();
    let save = Button::new("reply-rule-edit-save")
        .primary()
        .label(t!("reply_rule_edit.save").to_string());
    let save = if can_save {
        save.on_click(move |_, window, cx| {
            let _ = app_save.update(cx, |app, cx| {
                if let Some(st) = app.reply_rule_edit_dialog.as_ref() {
                    let rule = st.draft.clone();
                    let tab_id = st.tab_id.clone();
                    app.storage.upsert_reply_rule(&tab_id, rule);
                    // 新建时 priority 由列表位置决定，须重排以保证列表顺序 = 求值顺序
                    app.storage.renumber_reply_rule_priorities(&tab_id);
                    app.sync_reply_rules_to_network(cx);
                }
                app.reply_rule_edit_dialog = None;
                cx.notify();
            });
            window.close_dialog(cx);
        })
    } else {
        save.disabled(true)
    };

    DialogFooter::new().child(cancel).child(save)
}
