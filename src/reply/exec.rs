// 规则执行器（L2：副作用边界在此收口）
//
// 职责划分（docs/plan-reply-rules.md §6）：
//   `evaluate()`（matcher.rs，纯函数）  →  命中哪条规则
//   `render_reply()`（本文件，纯函数）  →  应答字节是什么
//   `handle_frame()`（本文件）          →  计数、留痕、决定帧的去向
//
// 「渲染」与「投递」刻意分开：
//   - 纯渲染让**调试面板能直接复用**，保证"预演结果 = 真实行为"；
//   - 投递是网络层的形状（TCP 用 `Sender<Vec<u8>>`，UDP 用 `Sender<(addr, Vec<u8>)>`），
//     由网络层拿到 `FrameOutcome::reply` 后自行选择通道，执行器不感知传输类型。

use crate::reply::frame::RxFrame;
use crate::reply::matcher::{PredicateResult, RuleOutcome, TraceEntry, evaluate};
use crate::reply::model::{CompiledReply, ReplyPayload, RuleCodec};
use crate::reply::store::{ReplyRulesStore, RuleRuntime};
use crate::utils::message_vars::{CompiledTemplate, RenderContext, VarSegment};
use log::debug;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// 一帧的处理结果。
///
/// 帧本身永远照常进入展示明细 —— 回复规则只**读**帧、决定"要不要回一段"，
/// 不改变帧的去向（不抑制、不丢弃、不断开）。
#[derive(Debug)]
pub struct FrameOutcome {
    /// 命中规则时，已渲染好的应答字节（等待网络层投递）
    pub reply: Option<Vec<u8>>,
    /// 应答的编码方式（决定网络层是否需要绕过连接 trailer —— 见 `rule_wire_mode`）
    pub codec: Option<RuleCodec>,
    /// 命中的规则 id（UI「最近命中」展示与命中计数）
    pub rule_id: Option<String>,
}

impl FrameOutcome {
    fn keep() -> Self {
        Self {
            reply: None,
            codec: None,
            rule_id: None,
        }
    }
}

/// 网络层对单帧的完整处理（服务端 / 客户端共用形态）。
///
/// **未启用时的开销**：一次 `is_enabled()` 原子读即返回，连读锁都不碰。
pub fn handle_frame(
    store: &ReplyRulesStore,
    frame: &Arc<RxFrame>,
    connection_id: &str,
) -> FrameOutcome {
    // 1) 未启用：零开销直通（继承既有 is_enabled 快速路径约定）
    if !store.is_enabled() {
        return FrameOutcome::keep();
    }

    // 2) 纯函数求值（热路径不收集轨迹 —— 这是签名里强制的性能约定）
    let rules = store.rules_for(connection_id);
    if rules.is_empty() {
        return FrameOutcome::keep();
    }
    let outcome = evaluate(rules.as_slice(), frame, false);

    // 3) 命中计数（原子累加，无锁；按下标直取 runtime，不再按 id 线性回查）
    if let Some(index) = outcome.rule_index {
        rules[index].hits.fetch_add(1, Ordering::Relaxed);
    }

    // 4) 命中则渲染应答；未命中什么都不做（不回、不记、不提示）。
    let mut out = FrameOutcome {
        rule_id: outcome.rule_id.clone(),
        ..FrameOutcome::keep()
    };

    let Some(index) = outcome.rule_index else {
        return out;
    };
    // P-8②：载荷直接从运行期实例借用，不再在 `evaluate` 里 clone 一份 String
    let payload = &rules[index].rule.payload;

    let bytes = render_reply(payload, outcome.compiled.as_deref(), frame, store);
    out.codec = Some(payload.codec);
    if bytes.is_empty() {
        // 空载荷不发（沿用既有约定：内容为空时不产生多余的发送事件）
        debug!("[reply] 规则 {:?} 命中但渲染结果为空, 不发送", out.rule_id);
    } else {
        out.reply = Some(bytes);
    }
    out
}

/// 渲染应答载荷（纯函数，可被调试面板直接调用）。
///
/// 两条路径：
///   - `CompiledReply.fixed`：无变量，直接返回预转换字节（零渲染开销）
///   - `CompiledReply.template`：逐条渲染（时间/UUID/rx.* 逐条不同）
///
/// 载荷文本的运算符优先级高于对象引用：`payload.text` 是持久化真源，
/// `compiled` 只是它的派生缓存；若二者不一致（例如调试面板塞了一个临时载荷），
/// 以 `compiled` 为准会给出错误结果，因此 `compiled` 为 None 时按原文现编译。
pub fn render_reply(
    payload: &ReplyPayload,
    compiled: Option<&CompiledReply>,
    frame: &Arc<RxFrame>,
    store: &ReplyRulesStore,
) -> Vec<u8> {
    let owned;
    let compiled = match compiled {
        Some(c) => c,
        None => {
            owned = CompiledReply::build(&payload.text, payload.hex_mode);
            &owned
        }
    };

    // 快路径：预转换字节
    if let Some(fixed) = &compiled.fixed {
        return fixed.clone();
    }
    let Some(template) = &compiled.template else {
        return Vec::new();
    };

    let seq = if template.needs_seq() {
        Some(store.next_seq())
    } else {
        None
    };
    let ctx = if compiled.needs_rx {
        // `Arc` 克隆而非复制帧字节：`handle_frame` 收到的本就是 `Arc<RxFrame>`
        RenderContext::for_reply(seq, Arc::clone(frame))
    } else {
        RenderContext::common(seq)
    };

    let mut out = String::with_capacity(template.template_len() + 32);
    template.render(&ctx, payload.hex_mode, &mut out);
    if payload.hex_mode {
        crate::utils::hex::hex_to_bytes(&out)
    } else {
        out.into_bytes()
    }
}

/// 把编码方式解析为"该用什么 trailer 写出"。
///
/// 语义（对齐 plan-reply-rules.md §6.3 的表）：
///   - `None` / `Inherit` → 返回 `None` 的**双关**需要区分，故这里返回 `RuleWireMode`
///   - `Raw`              → 原样输出（不加任何结尾）
///   - `Lf` / `Crlf`      → 固定追加
///
/// **为什么在这里处理而不是改 encoder**：连接的 encoder 对**所有**写入生效
/// （手动发送、发送任务、规则应答共用一条写通道）。给 encoder 加
/// "bypass 开关"是跨线程竞态 —— 并发手动发送会被误 bypass。因此把规则自己的
/// 编码意图随数据一起投递（见 `network::events::WireMessage`）。
pub enum RuleWireMode {
    /// 继承连接级 trailer 设置（与手动发送完全一致）
    Inherit,
    /// 绕过连接设置：None = 原样输出，Some = 追加指定结尾
    Override(Option<crate::config::connection::TrailerKind>),
}

/// 解析规则的编码方式
pub fn rule_wire_mode(codec: Option<RuleCodec>) -> RuleWireMode {
    use crate::config::connection::TrailerKind;
    match codec {
        None | Some(RuleCodec::Inherit) => RuleWireMode::Inherit,
        Some(RuleCodec::Raw) => RuleWireMode::Override(None),
        Some(RuleCodec::Lf) => RuleWireMode::Override(Some(TrailerKind::Lf)),
        Some(RuleCodec::Crlf) => RuleWireMode::Override(Some(TrailerKind::CrLf)),
    }
}

/// 跨作用域的调试预演：对给定帧跑**全部**规则并返回完整轨迹（调试面板用）。
///
/// 与网络层热路径调用同一个 `evaluate()`，只是 `collect_trace = true`
/// 且不过滤作用域 —— 用户需要看到"这条规则为什么没管这帧"。
pub fn dry_run(
    store: &ReplyRulesStore,
    rules: &[Arc<RuleRuntime>],
    frame: &Arc<RxFrame>,
) -> (RuleOutcome, Option<Vec<u8>>) {
    // 直接复用运行期实例（它们已是 `Arc<RuleRuntime>`），避免每次预演重建规则表
    // —— 重建会重新编译正则，正是预演最该避免的开销。
    let outcome = evaluate(rules, frame, true);
    // P-8②：载荷按下标从运行期实例借用，`RuleOutcome` 不再携带 clone 的载荷
    let rendered = outcome.rule_index.map(|index| {
        render_reply(
            &rules[index].rule.payload,
            outcome.compiled.as_deref(),
            frame,
            store,
        )
    });
    (outcome, rendered)
}

/// 把轨迹条目格式化为可渲染的"期望 vs 实际"行（调试面板 / 规则编辑弹窗共用）
pub fn trace_rows(entry: &TraceEntry) -> Vec<(String, String, PredicateResult)> {
    entry
        .predicates
        .iter()
        .map(|p| (p.label.clone(), p.actual.clone(), p.result))
        .collect()
}

/// 载荷模板里引用的接收帧变量（调试面板逐项列出"${rx.u16be:2} 取到什么"）
pub fn rx_variables(payload: &ReplyPayload) -> Vec<String> {
    if !payload.text.contains("${") {
        return Vec::new();
    }
    let template = CompiledTemplate::new(&payload.text);
    template
        .segments()
        .iter()
        .filter_map(|seg| match seg {
            VarSegment::RxAccess {
                accessor,
                offset,
                len,
                ..
            } => Some(match len {
                Some(l) => format!("${{rx.{}:{}:{}}}", accessor, offset, l),
                None => format!("${{rx.{}:{}}}", accessor, offset),
            }),
            VarSegment::RxMeta(kind) => Some(format!("${{rx.{}}}", kind.name())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reply::frame::RxFrame;
    use crate::reply::model::{
        BytePattern, MatchNode, ReplyRule, ReplyRulesConfig, RuleCodec,
    };
    use crate::reply::store::ReplyRulesStore;
    use std::collections::HashMap;

    /// 把规则挂到指定连接下构造 store, 并开启这些连接的自动回复总闸。
    ///
    /// 规则现在严格属于单一连接: 想在哪条连接上生效, 就必须挂到该连接 id 下。
    fn store_with_conns(pairs: Vec<(&str, Vec<ReplyRule>)>) -> Arc<ReplyRulesStore> {
        let store = ReplyRulesStore::new();
        store.set_connection_gates(
            ["tab", "tab-a", "tab-b"]
                .into_iter()
                .map(|s| (s.to_string(), true))
                .collect(),
        );
        let mut connections: HashMap<String, Vec<ReplyRule>> = HashMap::new();
        for (conn, rules) in pairs {
            connections.insert(conn.to_string(), rules);
        }
        store.replace(&ReplyRulesConfig {
            connections,
            ..Default::default()
        });
        store
    }

    /// 单连接测试的便捷封装: 规则挂到 "tab" 下。
    fn store_with(rules: Vec<ReplyRule>) -> Arc<ReplyRulesStore> {
        store_with_conns(vec![("tab", rules)])
    }

    fn reply_rule(matcher: MatchNode, text: &str, hex_mode: bool, codec: RuleCodec) -> ReplyRule {
        ReplyRule {
            matcher,
            payload: ReplyPayload {
                text: text.to_string(),
                hex_mode,
                codec,
            },
            ..ReplyRule::new("应答", 1)
        }
    }

    fn frame(bytes: &[u8]) -> Arc<RxFrame> {
        RxFrame::for_test(bytes.to_vec())
    }

    /// 未启用时 `handle_frame` 直通：不改去向、不计数、不发送
    #[test]
    fn test_disabled_store_is_passthrough() {
        let store = ReplyRulesStore::new();
        let f = frame(&[1, 2, 3]);
        let outcome = handle_frame(&store, &f, "tab");
        assert!(outcome.reply.is_none());
        assert!(outcome.rule_id.is_none());
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 0);
    }

    /// 命中 `Reply`：返回渲染好的字节 + 累加命中计数
    #[test]
    fn test_reply_action_renders_and_counts() {
        let store = store_with(vec![reply_rule(
            MatchNode::Length { min: 1, max: 8 },
            "6F 6B",
            true,
            RuleCodec::Raw,
        )]);
        let f = frame(&[1, 2, 3]);
        let outcome = handle_frame(&store, &f, "tab");
        assert_eq!(outcome.reply.as_deref(), Some(&b"ok"[..]));
        assert_eq!(outcome.codec, Some(RuleCodec::Raw));
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 1);
    }

    /// `${seq}` 在应答渲染时按序递增（网络线程自取，不回 UI 线程）
    #[test]
    fn test_seq_in_reply() {
        let store = store_with(vec![reply_rule(
            MatchNode::Length { min: 1, max: 8 },
            "n=${seq}",
            false,
            RuleCodec::Inherit,
        )]);
        let f = frame(&[1]);
        assert_eq!(
            handle_frame(&store, &f, "tab").reply.as_deref(),
            Some(&b"n=0"[..])
        );
        assert_eq!(
            handle_frame(&store, &f, "tab").reply.as_deref(),
            Some(&b"n=1"[..])
        );
    }

    /// T-5：`${seq}` 是**进程级**序号（每个 store 一个原子量），**不按连接独立**。
    ///
    /// 有意为之：序号服务于"应答帧连续编号"，而网络线程本就无可依赖的 UI 侧状态；
    /// 本测试把该语义固定下来（跨连接共享同一序列），防止将来被误改为按连接计数。
    #[test]
    fn test_seq_is_process_global_across_connections() {
        let shared = reply_rule(
            MatchNode::Length { min: 1, max: 8 },
            "n=${seq}",
            false,
            RuleCodec::Inherit,
        );
        let store = store_with_conns(vec![
            ("tab-a", vec![shared.clone()]),
            ("tab-b", vec![shared]),
        ]);
        let f = frame(&[1]);
        assert_eq!(
            handle_frame(&store, &f, "tab-a").reply.as_deref(),
            Some(&b"n=0"[..])
        );
        // 换一条连接：序号继续递增，不重置
        assert_eq!(
            handle_frame(&store, &f, "tab-b").reply.as_deref(),
            Some(&b"n=1"[..])
        );
        assert_eq!(
            handle_frame(&store, &f, "tab-a").reply.as_deref(),
            Some(&b"n=2"[..])
        );
    }

    /// T-4：**列表顺序 = 求值顺序** —— 两条都命中的规则里，先列出的先命中（命中即停）。
    ///
    /// UI 的"上移/下移"（`ConfigStorage::move_reply_rule`）改的正是这个顺序，
    /// 并同步重排 priority；本测试锁住"顺序变了，谁先应答也要跟着变"。
    #[test]
    fn test_first_hit_wins_follows_list_order() {
        let mut first = reply_rule(
            MatchNode::Length { min: 1, max: 8 },
            "FIRST",
            false,
            RuleCodec::Inherit,
        );
        first.priority = 0;
        let mut second = reply_rule(
            MatchNode::Length { min: 1, max: 8 },
            "SECOND",
            false,
            RuleCodec::Inherit,
        );
        second.priority = 10;

        let store = store_with(vec![first.clone(), second.clone()]);
        assert_eq!(
            handle_frame(&store, &frame(&[1]), "tab").reply.as_deref(),
            Some(&b"FIRST"[..]),
            "列表在前者优先生效"
        );

        // 下移一次后的顺序（等价于 priority 重排：SECOND=0, FIRST=10）
        let mut moved_first = first;
        moved_first.priority = 10;
        let mut moved_second = second;
        moved_second.priority = 0;
        let store = store_with(vec![moved_second, moved_first]);
        assert_eq!(
            handle_frame(&store, &frame(&[1]), "tab").reply.as_deref(),
            Some(&b"SECOND"[..]),
            "顺序改变后，先应答者随之改变"
        );
    }

    /// `${rx.raw}` 原样回显（NetAssist §2.3 的 ECHO 场景）
    #[test]
    fn test_echo_via_rx_raw() {
        let store = store_with(vec![reply_rule(
            MatchNode::Length { min: 1, max: 64 },
            "${rx.raw}",
            true,
            RuleCodec::Raw,
        )]);
        let f = frame(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let outcome = handle_frame(&store, &f, "tab");
        assert_eq!(
            outcome.reply.as_deref(),
            Some(&[0xDE, 0xAD, 0xBE, 0xEF][..])
        );
    }

    /// 生成型校验：Modbus RTU 完整帧 `01 03 00 00 00 02 ${crc16modbus:0:6:le}`
    /// 必须渲染出 `C4 0B`（规划 M2/M3 的验收项，也是 F-32 的核心用法）
    #[test]
    fn test_generated_checksum_modbus_frame() {
        let store = store_with(vec![reply_rule(
            MatchNode::Length { min: 1, max: 8 },
            "01 03 00 00 00 02 ${crc16modbus:0:6:le}",
            true,
            RuleCodec::Raw,
        )]);
        let outcome = handle_frame(&store, &frame(&[1, 2, 3]), "tab");
        assert_eq!(
            outcome.reply.as_deref(),
            Some(&[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B][..])
        );
    }

    /// `${rx.crc16modbus:0:6}` 作用于**接收帧**（与生成型变量的作用域不同）
    #[test]
    fn test_rx_checksum_scope() {
        let store = store_with(vec![reply_rule(
            MatchNode::Length { min: 6, max: 6 },
            "${rx.crc16modbus:0:6:le}",
            true,
            RuleCodec::Raw,
        )]);
        let f = frame(&[0x01, 0x03, 0x00, 0x00, 0x00, 0x02]);
        let outcome = handle_frame(&store, &f, "tab");
        assert_eq!(outcome.reply.as_deref(), Some(&[0xC4, 0x0B][..]));
    }

    /// 空载荷不发（沿用既有约定）
    #[test]
    fn test_empty_payload_not_sent() {
        let store = store_with(vec![reply_rule(
            MatchNode::Length { min: 1, max: 8 },
            "",
            false,
            RuleCodec::Inherit,
        )]);
        let outcome = handle_frame(&store, &frame(&[1]), "tab");
        assert!(outcome.reply.is_none());
        // 仍然计数（规则确实命中了，"命中但无内容"是配置问题，不该隐藏）
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 1);
    }

    /// 未命中：什么都不做（不回、不计数、不留痕）
    #[test]
    fn test_no_match_is_silent() {
        let rule = reply_rule(
            MatchNode::Length { min: 8, max: 8 },
            "ok",
            false,
            RuleCodec::Inherit,
        );
        let store = store_with(vec![rule]);
        let outcome = handle_frame(&store, &frame(&[1]), "tab");
        assert!(outcome.reply.is_none());
        assert!(outcome.rule_id.is_none());
        assert_eq!(store.hits_snapshot().values().sum::<u64>(), 0);
    }

    /// 连接隔离：挂到别的连接(id=tab-a)下的规则不得在 tab-b 上生效
    #[test]
    fn test_scope_respected_in_handle_frame() {
        let store = store_with_conns(vec![(
            "tab-a",
            vec![ReplyRule {
                matcher: MatchNode::Length { min: 1, max: 8 },
                payload: ReplyPayload {
                    text: "ok".to_string(),
                    hex_mode: false,
                    codec: RuleCodec::Raw,
                },
                ..ReplyRule::new("局部", 1)
            }],
        )]);
        let f = frame(&[1]);
        assert!(handle_frame(&store, &f, "tab-a").reply.is_some());
        assert!(handle_frame(&store, &f, "tab-b").reply.is_none());
        assert_eq!(
            store.hits_snapshot().values().sum::<u64>(),
            1,
            "只有生效的那次才计数"
        );
    }

    /// `rule_wire_mode`：Raw 必须绕过连接 trailer（二进制协议正确性的关键）
    #[test]
    fn test_rule_wire_mode_mapping() {
        use crate::config::connection::TrailerKind;
        assert!(matches!(rule_wire_mode(None), RuleWireMode::Inherit));
        assert!(matches!(
            rule_wire_mode(Some(RuleCodec::Inherit)),
            RuleWireMode::Inherit
        ));
        assert!(matches!(
            rule_wire_mode(Some(RuleCodec::Raw)),
            RuleWireMode::Override(None)
        ));
        assert!(matches!(
            rule_wire_mode(Some(RuleCodec::Lf)),
            RuleWireMode::Override(Some(TrailerKind::Lf))
        ));
        assert!(matches!(
            rule_wire_mode(Some(RuleCodec::Crlf)),
            RuleWireMode::Override(Some(TrailerKind::CrLf))
        ));
    }

    /// `dry_run` 与真实路径必须给出相同判定（"预演结果 = 真实行为"的硬断言）
    #[test]
    fn test_dry_run_matches_real_path() {
        let store = store_with(vec![
            reply_rule(
                MatchNode::Contains {
                    bytes: BytePattern::Hex("01 03".to_string()),
                },
                "aa",
                true,
                RuleCodec::Raw,
            ),
            reply_rule(
                MatchNode::Length { min: 1, max: 64 },
                "bb",
                true,
                RuleCodec::Raw,
            ),
        ]);
        let rules = store.enabled_rules();
        for bytes in [
            vec![0x01, 0x03],
            vec![0xAA, 0xBB],
            vec![],
            vec![0x01, 0x03, 0x04, 0x05],
        ] {
            let f = frame(&bytes);
            let real = handle_frame(&store, &f, "tab");
            let (preview, rendered) = dry_run(&store, rules.as_slice(), &f);
            assert_eq!(
                real.rule_id, preview.rule_id,
                "预演与真实路径命中判定必须一致: {:?}",
                bytes
            );
            assert_eq!(
                real.reply, rendered,
                "预演与真实路径应答必须一致: {:?}",
                bytes
            );
        }
    }

    /// `dry_run` 必须收集完整轨迹（含未命中规则的失败原因）
    #[test]
    fn test_dry_run_collects_trace() {
        let store = store_with(vec![reply_rule(
            MatchNode::All {
                children: vec![
                    MatchNode::Length { min: 8, max: 8 },
                    MatchNode::ByteAt {
                        offset: 0,
                        op: crate::reply::model::ByteOp::Eq { value: 1 },
                    },
                ],
            },
            "ok",
            false,
            RuleCodec::Inherit,
        )]);
        let rules = store.enabled_rules();
        let (outcome, rendered) = dry_run(&store, rules.as_slice(), &frame(&[0x02]));
        assert!(!outcome.is_hit());
        assert!(rendered.is_none());
        assert_eq!(outcome.trace.entries.len(), 1);
        let entry = &outcome.trace.entries[0];
        assert_eq!(entry.predicates.len(), 2);
        assert_eq!(entry.predicates[0].result, PredicateResult::Fail);
        assert_eq!(entry.predicates[1].result, PredicateResult::Skipped);
        let rows = trace_rows(entry);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].0.contains('8'), "标签应含期望值: {}", rows[0].0);
    }

    /// 渲染载荷的兜底路径（compiled 为 None 时按原文现编译）
    #[test]
    fn test_render_reply_fallback_without_compiled() {
        let store = ReplyRulesStore::new();
        let payload = ReplyPayload {
            text: "6F 6B".to_string(),
            hex_mode: true,
            codec: RuleCodec::Raw,
        };
        let f = frame(&[1]);
        assert_eq!(render_reply(&payload, None, &f, &store), vec![0x6F, 0x6B]);
    }

    /// rx 变量清单（调试面板逐项展示取值）
    #[test]
    fn test_rx_variables_listing() {
        let payload = ReplyPayload {
            text: "01 ${rx.raw:0:2} ${rx.u16be:2} ${rx.len} ${seq}".to_string(),
            hex_mode: true,
            codec: RuleCodec::Raw,
        };
        let vars = rx_variables(&payload);
        assert_eq!(
            vars,
            vec![
                "${rx.raw:0:2}".to_string(),
                "${rx.u16be:2}".to_string(),
                "${rx.len}".to_string()
            ],
            "只列出 rx.* 变量，不含 seq"
        );
    }

    // ========================================================================
    // 性能回归（对应 plan-reply-rules.md §9.1 与 §9.7 的待验证项）
    // ========================================================================

    /// V1 回归：未启用规则时，每帧只付一次 `is_enabled()` 原子读。
    ///
    /// 断言口径是**相对关系**而非绝对 QPS（绝对数值依赖机器与构建 profile，
    /// 写成常量必然在别的机器上假失败）：未启用路径必须比"10 条规则的完整求值"
    /// 快至少一个数量级。这条约束一旦被破坏（比如有人把 `is_enabled` 挪到求值之后、
    /// 或给快速路径加了锁），本测试立刻失败。
    ///
    /// 同时给出绝对量级（`println!`，用 `--nocapture` 可见）用于回填文档。
    #[test]
    fn test_disabled_fast_path_is_orders_magnitude_cheaper() {
        use std::time::Instant;

        let frame_bytes = [0x01u8, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B];
        let iterations = 200_000u32;

        // 场景 A：未启用（快速路径）
        let disabled = ReplyRulesStore::new();

        // 场景 B：启用 10 条规则（含正则这个最贵的谓词），最后一击命中并渲染应答
        let store = ReplyRulesStore::new();
        let mut rules: Vec<ReplyRule> = (0..8)
            .map(|i| ReplyRule {
                // 刻意让它**不命中**：这样前面的规则都会跑完各自的谓词，
                // 才测得到"10 条规则逐条求值"的真实成本
                matcher: MatchNode::All {
                    children: vec![
                        MatchNode::Length { min: 1, max: 64 },
                        MatchNode::ByteAt {
                            offset: 1,
                            op: crate::reply::model::ByteOp::Eq { value: 0xFF },
                        },
                    ],
                },
                ..ReplyRule::new(format!("规则{}", i), i)
            })
            .collect();
        rules.push(ReplyRule {
            // 正则作用在 UTF-8 文本上；二进制帧会被跳过（这是设计行为，见 matcher）
            matcher: MatchNode::Regex {
                pattern: "^NEVER".to_string(),
            },
            ..ReplyRule::new("正则不命中", 100)
        });
        rules.push(ReplyRule {
            matcher: MatchNode::Length { min: 1, max: 64 },
            payload: ReplyPayload {
                text: "6F 6B".to_string(),
                hex_mode: true,
                codec: RuleCodec::Raw,
            },
            ..ReplyRule::new("兜底命中", 9999)
        });
        let mut connections: HashMap<String, Vec<ReplyRule>> = HashMap::new();
        connections.insert("tab".to_string(), rules);
        store.replace(&ReplyRulesConfig {
            connections,
            ..Default::default()
        });
        // 规则需连接总闸开启后才在该连接求值: 本性能回归需让 "tab" 上真正求值
        store.set_connection_gates([("tab".to_string(), true)].into_iter().collect());

        let disabled_frame = frame(&frame_bytes);
        let start = Instant::now();
        for _ in 0..iterations {
            let out = handle_frame(&disabled, &disabled_frame, "tab");
            debug_assert!(out.reply.is_none() && out.rule_id.is_none());
        }
        let disabled_elapsed = start.elapsed();

        let active_frame = frame(&frame_bytes);
        let probe = handle_frame(&store, &active_frame, "tab");
        println!(
            "[perf] 探针: 启用={} 规则数={} 应答={:?}",
            store.is_enabled(),
            store.enabled_rule_count(),
            probe.reply.as_ref().map(|r| r.len()),
        );
        let start = Instant::now();
        for _ in 0..iterations {
            let out = handle_frame(&store, &active_frame, "tab");
            debug_assert!(out.reply.is_some());
        }
        let active_elapsed = start.elapsed();

        let disabled_qps = iterations as f64 / disabled_elapsed.as_secs_f64();
        let active_qps = iterations as f64 / active_elapsed.as_secs_f64();
        println!(
            "[perf] 未启用快速路径: {:.0} 帧/秒 ({:?} / {} 次); 10 条规则求值: {:.0} 帧/秒 ({:?} / {} 次); 倍数 {:.1}x",
            disabled_qps,
            disabled_elapsed,
            iterations,
            active_qps,
            active_elapsed,
            iterations,
            disabled_qps / active_qps.max(1.0)
        );

        // 快速路径必须至少 10 倍便宜（实测通常 100x+，留足机器差异余量）
        assert!(
            disabled_qps > active_qps * 10.0,
            "未启用快速路径必须显著便宜于完整求值: 未启用 {:.0} 帧/秒 vs 求值 {:.0} 帧/秒",
            disabled_qps,
            active_qps
        );
        // 绝对下限：debug 构建下也应有实用吞吐（回归检测，不是性能目标）
        assert!(
            disabled_qps > 50_000.0,
            "未启用路径吞吐过低: {:.0} 帧/秒",
            disabled_qps
        );
    }

    /// V2 回归：并发读（`rules_for`）与低频写（`replace`）并存时不丢帧、不死锁。
    ///
    /// 这是"UI 改规则时网络线程仍能正常求值"的保证：`RwLock` 读多写少，
    /// 写是整表替换（`Arc` 换指针），读只多一次引用计数。
    #[test]
    fn test_concurrent_read_while_replacing() {
        let store = store_with(vec![reply_rule(
            MatchNode::Length { min: 1, max: 64 },
            "6F 6B",
            true,
            RuleCodec::Raw,
        )]);

        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_store = store.clone();
        let reader_stop = stop.clone();
        let reader = std::thread::spawn(move || {
            let frame = frame(&[1, 2, 3]);
            let mut hits = 0u64;
            while !reader_stop.load(Ordering::Relaxed) {
                if handle_frame(&reader_store, &frame, "tab").reply.is_some() {
                    hits += 1;
                }
            }
            hits
        });

        // 写线程：反复整表替换（模拟 UI 连续编辑规则）
        for i in 0..300u32 {
            let mut connections: HashMap<String, Vec<ReplyRule>> = HashMap::new();
            connections.insert(
                "tab".to_string(),
                vec![reply_rule(
                    MatchNode::Length { min: 1, max: 64 },
                    "6F 6B",
                    true,
                    RuleCodec::Raw,
                )],
            );
            store.replace(&ReplyRulesConfig {
                connections,
                ..Default::default()
            });
            let _ = i;
        }
        stop.store(true, Ordering::Relaxed);
        let hits = reader.join().expect("读线程不应 panic");
        assert!(hits > 0, "并发替换期间必须仍然正常应答");
    }
}
