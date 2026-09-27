// 通用消息变量引擎(文本 / hex 通用)
//
// 支持变量(在文本层替换, hex 模式下替换后再 hex_to_bytes):
//   ${seq}            递增序号(由调用方提供计数器)
//   ${worker_id}      当前 worker 编号(压测专属, 普通消息上下文传 None → 原样保留)
//   ${counter}        当前 worker 本地计数(压测专属, 同上)
//   ${timestamp}      当前毫秒时间戳(Unix epoch, 既有语义不变)
//   ${timestamp_s}    当前秒时间戳(Unix epoch)
//   ${date}           本地日期 %Y-%m-%d
//   ${time}           本地时间 %H:%M:%S
//   ${datetime}       本地日期时间 %Y-%m-%d %H:%M:%S
//   ${datetime_ms}    本地日期时间(含毫秒)
//   ${iso}            RFC3339(带时区偏移)
//   ${utc}            UTC ISO 8601(以 Z 结尾)
//   ${time:格式}      任意 strftime 格式(格式非法则原样保留)
//   ${uuid}           随机 UUID v4(既有语义不变)
//   ${random:min:max} [min,max] 闭区间随机整数(既有语义不变)
//
// 未知变量(如 ${foo})原样保留; 无 } 结尾的 ${ 也原样保留。
//
// hex 模式下:
//   - 数值变量(seq/worker_id/counter/timestamp/timestamp_s/random)输出零填充偶数长度十六进制
//   - uuid 输出 32 字符无连字符十六进制
//   - 文本类变量(时间/日期等)按 UTF-8 字节输出为大写十六进制
//
// 一条消息内所有时间变量共用同一个 `now`, 避免 ${datetime} 与 ${timestamp} 跨毫秒不一致。

use chrono::{DateTime, Local, SecondsFormat, Utc};
use uuid::Uuid;

/// 单次渲染的上下文
///
/// `worker_id` / `counter` / `seq` 用 `Option` 表达"当前场景是否适用":
/// - 普通消息: 三者均可为 None, 对应变量按"未知变量"原样保留(不误导成 0)
/// - 压测: 三者均为 Some, 行为与既有实现逐字一致
#[derive(Debug, Clone)]
pub struct RenderContext {
    /// 同一条消息共享的时间基准
    pub now: DateTime<Local>,
    /// 压测 worker 编号(普通消息传 None → ${worker_id} 原样保留)
    pub worker_id: Option<usize>,
    /// 压测 worker 本地计数(普通消息传 None → ${counter} 原样保留)
    pub counter: Option<u64>,
    /// 递增序号(不消费序号时传 None; 与 CompiledTemplate::needs_seq 配合)
    pub seq: Option<u64>,
}

impl RenderContext {
    /// 构造普通消息上下文: 取一次当前时间, 压测专属变量不适用
    ///
    /// `seq` 仅在模板含 `${seq}` 时由调用方 `fetch_add` 后传入。
    pub fn common(seq: Option<u64>) -> Self {
        Self {
            now: Local::now(),
            worker_id: None,
            counter: None,
            seq,
        }
    }
}

/// 预编译的模板段
#[derive(Debug, Clone)]
pub enum VarSegment {
    /// 字面量文本
    Literal(String),
    /// ${timestamp} Unix 毫秒
    Timestamp,
    /// ${timestamp_s} Unix 秒
    TimestampSecs,
    /// ${date} 本地日期
    Date,
    /// ${time} 本地时间
    Time,
    /// ${datetime} 本地日期时间
    DateTime,
    /// ${datetime_ms} 本地日期时间(含毫秒)
    DateTimeMs,
    /// ${iso} RFC3339(带时区偏移)
    Iso,
    /// ${utc} UTC ISO 8601
    Utc,
    /// ${time:格式} 任意 strftime(已预检合法)
    TimeFormat(String),
    /// ${uuid} UUID v4
    Uuid,
    /// ${random:min:max} 预解析的闭区间
    Random(i64, i64),
    /// ${seq}
    Seq,
    /// ${worker_id}
    WorkerId,
    /// ${counter}
    Counter,
    /// 未知变量 / 非法格式, 原样保留 ${name}
    Unknown(String),
}

/// 预编译的模板
///
/// 解析一次, 拆分为段(Literal / 变量), 避免每包重复执行 `char_indices().collect()`
/// 和字符串搜索。
#[derive(Debug, Clone)]
pub struct CompiledTemplate {
    segments: Vec<VarSegment>,
    /// 原始模板长度(用于调用方预分配输出缓冲)
    template_len: usize,
}

impl CompiledTemplate {
    /// 从模板字符串构造预编译模板
    pub fn new(template: &str) -> Self {
        let template_len = template.len();
        // 快速路径: 无变量
        if !template.contains("${") {
            return Self {
                segments: vec![VarSegment::Literal(template.to_string())],
                template_len,
            };
        }

        let chars: Vec<(usize, char)> = template.char_indices().collect();
        let mut segments = Vec::new();
        let mut ci = 0;
        let mut literal_start = 0;

        while ci < chars.len() {
            let (_, ch) = chars[ci];
            if ch == '$' && ci + 1 < chars.len() && chars[ci + 1].1 == '{' {
                let after_brace_byte = chars[ci + 1].0 + chars[ci + 1].1.len_utf8();
                if let Some(close_rel) = template[after_brace_byte..].find('}') {
                    // 先冲刷已积累的字面量
                    if literal_start < chars[ci].0 {
                        segments.push(VarSegment::Literal(
                            template[literal_start..chars[ci].0].to_string(),
                        ));
                    }
                    let var_name = &template[after_brace_byte..after_brace_byte + close_rel];
                    segments.push(parse_segment(var_name));
                    let close_byte = after_brace_byte + close_rel + 1;
                    literal_start = close_byte;
                    ci = chars.partition_point(|(p, _)| *p < close_byte);
                    continue;
                }
            }
            ci += 1;
        }
        // 冲刷尾部字面量
        if literal_start < template.len() {
            segments.push(VarSegment::Literal(template[literal_start..].to_string()));
        }

        Self {
            segments,
            template_len,
        }
    }

    /// 原始模板长度(用于调用方预分配缓冲)
    pub fn template_len(&self) -> usize {
        self.template_len
    }

    /// 模板是否含 `${seq}`
    ///
    /// 调用方据此决定是否 `fetch_add`, 避免"只发 ${uuid} 也吃掉一个序号"。
    pub fn needs_seq(&self) -> bool {
        self.segments.iter().any(|s| matches!(s, VarSegment::Seq))
    }

    /// 渲染到给定的 String 缓冲(调用方负责 clear + 预分配)
    pub fn render(&self, ctx: &RenderContext, hex_mode: bool, out: &mut String) {
        // uuid 每条消息只生成一次(仅当模板含 ${uuid} 时才产生随机数开销)
        let uuid = if self.segments.iter().any(|s| matches!(s, VarSegment::Uuid)) {
            Some(Uuid::new_v4())
        } else {
            None
        };

        for seg in &self.segments {
            match seg {
                VarSegment::Literal(s) => out.push_str(s),
                VarSegment::Timestamp => push_i64(out, ctx.now.timestamp_millis(), hex_mode),
                VarSegment::TimestampSecs => push_i64(out, ctx.now.timestamp(), hex_mode),
                VarSegment::Date => push_text(out, &ctx.now.format("%Y-%m-%d").to_string(), hex_mode),
                VarSegment::Time => push_text(out, &ctx.now.format("%H:%M:%S").to_string(), hex_mode),
                VarSegment::DateTime => {
                    push_text(out, &ctx.now.format("%Y-%m-%d %H:%M:%S").to_string(), hex_mode)
                }
                VarSegment::DateTimeMs => push_text(
                    out,
                    &ctx.now.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                    hex_mode,
                ),
                VarSegment::Iso => push_text(
                    out,
                    &ctx.now.to_rfc3339_opts(SecondsFormat::Secs, false),
                    hex_mode,
                ),
                VarSegment::Utc => push_text(
                    out,
                    &ctx
                        .now
                        .with_timezone(&Utc)
                        .to_rfc3339_opts(SecondsFormat::Secs, true),
                    hex_mode,
                ),
                VarSegment::TimeFormat(fmt) => {
                    push_text(out, &ctx.now.format(fmt).to_string(), hex_mode)
                }
                VarSegment::Uuid => {
                    if let Some(uuid) = &uuid {
                        if hex_mode {
                            out.push_str(&uuid.simple().to_string().to_uppercase())
                        } else {
                            out.push_str(&uuid.to_string())
                        }
                    }
                }
                VarSegment::Random(min, max) => {
                    let span = (*max - *min) as u64 + 1;
                    let val = *min + (random_u64() % span) as i64;
                    push_i64(out, val, hex_mode);
                }
                // 上下文不适用的变量按"未知变量"原样保留(不静默渲染成 0)
                VarSegment::Seq => match ctx.seq {
                    Some(v) => push_u64(out, v, hex_mode),
                    None => out.push_str("${seq}"),
                },
                VarSegment::WorkerId => match ctx.worker_id {
                    Some(v) => push_u64(out, v as u64, hex_mode),
                    None => out.push_str("${worker_id}"),
                },
                VarSegment::Counter => match ctx.counter {
                    Some(v) => push_u64(out, v, hex_mode),
                    None => out.push_str("${counter}"),
                },
                VarSegment::Unknown(s) => out.push_str(s),
            }
        }
    }
}

/// 将变量名解析为预编译段
fn parse_segment(var_name: &str) -> VarSegment {
    match var_name {
        "seq" => VarSegment::Seq,
        "worker_id" => VarSegment::WorkerId,
        "counter" => VarSegment::Counter,
        "timestamp" => VarSegment::Timestamp,
        "timestamp_s" => VarSegment::TimestampSecs,
        "date" => VarSegment::Date,
        "time" => VarSegment::Time,
        "datetime" => VarSegment::DateTime,
        "datetime_ms" => VarSegment::DateTimeMs,
        "iso" => VarSegment::Iso,
        "utc" => VarSegment::Utc,
        "uuid" => VarSegment::Uuid,
        _ if var_name.starts_with("time:") => {
            let fmt = &var_name["time:".len()..];
            // 空格式按 ${time} 默认处理
            if fmt.is_empty() {
                return VarSegment::Time;
            }
            if is_valid_strftime(fmt) {
                VarSegment::TimeFormat(fmt.to_string())
            } else {
                VarSegment::Unknown(format!("${{{}}}", var_name))
            }
        }
        _ if var_name.starts_with("random:") => {
            let params = &var_name["random:".len()..];
            let parts: Vec<&str> = params.split(':').collect();
            if parts.len() != 2 {
                return VarSegment::Unknown(format!("${{{}}}", var_name));
            }
            match (
                parts[0].trim().parse::<i64>(),
                parts[1].trim().parse::<i64>(),
            ) {
                (Ok(min), Ok(max)) if min <= max => VarSegment::Random(min, max),
                _ => VarSegment::Unknown(format!("${{{}}}", var_name)),
            }
        }
        _ => VarSegment::Unknown(format!("${{{}}}", var_name)),
    }
}

/// 预检 strftime 格式串是否合法
///
/// chrono 的 `DelayedFormat` 对非法指示符(如 `%Q`)会在格式化时 panic,
/// 因此必须在编译期用 `StrftimeItems` 预检, 命中 `Item::Error` 即视为未知变量原样保留。
fn is_valid_strftime(fmt: &str) -> bool {
    use chrono::format::{Item, StrftimeItems};
    !StrftimeItems::new(fmt).any(|item| matches!(item, Item::Error))
}

/// 追加数值: hex 模式输出偶数长度大写十六进制, 否则十进制文本
fn push_u64(out: &mut String, v: u64, hex_mode: bool) {
    if hex_mode {
        out.push_str(&format_hex_u64(v))
    } else {
        out.push_str(&v.to_string())
    }
}

fn push_i64(out: &mut String, v: i64, hex_mode: bool) {
    push_u64(out, v as u64, hex_mode)
}

/// 追加文本: hex 模式按 UTF-8 字节输出为大写十六进制, 否则原样文本
fn push_text(out: &mut String, text: &str, hex_mode: bool) {
    if hex_mode {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for b in text.as_bytes() {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    } else {
        out.push_str(text);
    }
}

/// 将 u64 格式化为偶数长度大写十六进制字符串(如 0→"00", 10→"0A", 256→"0100")
fn format_hex_u64(v: u64) -> String {
    let hex = format!("{:X}", v);
    if hex.len() % 2 != 0 {
        format!("0{}", hex)
    } else {
        hex
    }
}

/// 用 std RandomState 产生一个伪随机 u64
///
/// 非加密用途(仅为报文变化), 避免引入 rand 依赖。
fn random_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_text(template: &str, ctx: &RenderContext) -> String {
        let compiled = CompiledTemplate::new(template);
        let mut out = String::new();
        compiled.render(ctx, false, &mut out);
        out
    }

    fn render_hex(template: &str, ctx: &RenderContext) -> String {
        let compiled = CompiledTemplate::new(template);
        let mut out = String::new();
        compiled.render(ctx, true, &mut out);
        out
    }

    fn common_ctx() -> RenderContext {
        RenderContext::common(Some(0))
    }

    #[test]
    fn test_no_variable_fast_path() {
        let compiled = CompiledTemplate::new("hello world");
        assert!(!compiled.needs_seq());
        assert_eq!(render_text("hello world", &common_ctx()), "hello world");
    }

    #[test]
    fn test_seq_needs_seq_and_render() {
        let compiled = CompiledTemplate::new("req-${seq}");
        assert!(compiled.needs_seq());
        assert_eq!(render_text("req-${seq}", &RenderContext::common(Some(0))), "req-0");
        assert_eq!(render_text("req-${seq}", &RenderContext::common(Some(1))), "req-1");
        // 不含 ${seq} 的模板不消费序号
        assert!(!CompiledTemplate::new("id=${uuid}").needs_seq());
    }

    #[test]
    fn test_seq_none_preserved() {
        // 传 None 时 ${seq} 原样保留(不静默渲染成 0)
        assert_eq!(render_text("${seq}", &RenderContext::common(None)), "${seq}");
    }

    #[test]
    fn test_worker_id_and_counter() {
        let ctx = RenderContext {
            now: Local::now(),
            worker_id: Some(7),
            counter: Some(1),
            seq: Some(0),
        };
        assert_eq!(render_text("w${worker_id}-c${counter}", &ctx), "w7-c1");
    }

    #[test]
    fn test_worker_id_counter_none_preserved() {
        // 普通消息上下文: 压测专属变量原样保留, 不渲染成 0
        assert_eq!(
            render_text("${worker_id}${counter}", &common_ctx()),
            "${worker_id}${counter}"
        );
    }

    #[test]
    fn test_timestamp_and_secs_share_now() {
        let ctx = common_ctx();
        let out = render_text("${timestamp}|${timestamp_s}", &ctx);
        let (ms, secs) = out.split_once('|').unwrap();
        assert_eq!(
            ms.parse::<i64>().unwrap() / 1000,
            secs.parse::<i64>().unwrap()
        );
    }

    #[test]
    fn test_time_presets_shape() {
        let ctx = common_ctx();
        assert_eq!(render_text("${date}", &ctx).len(), 10);
        assert_eq!(render_text("${time}", &ctx).len(), 8);
        assert_eq!(render_text("${datetime}", &ctx).len(), 19);
        assert!(render_text("${datetime_ms}", &ctx).len() >= 23);
        assert!(render_text("${iso}", &ctx).ends_with("+08:00") || render_text("${iso}", &ctx).contains('T'));
        assert!(render_text("${utc}", &ctx).ends_with('Z'));
    }

    #[test]
    fn test_time_custom_format() {
        let ctx = RenderContext::common(Some(0));
        let out = render_text("${time:%Y/%m/%d}", &ctx);
        // 与 ${date} 的连字符格式对照: 分隔符应被替换为 /
        assert_eq!(out.len(), 10);
        assert!(out.contains('/'));
        // 空格式回退到 ${time} 默认
        assert_eq!(render_text("${time:}", &ctx).len(), 8);
    }

    #[test]
    fn test_invalid_time_format_preserved() {
        // 非法指示符(%Q)必须原样保留而非 panic
        let ctx = common_ctx();
        assert_eq!(render_text("${time:%Q}", &ctx), "${time:%Q}");
    }

    #[test]
    fn test_text_var_hex_encoding() {
        // hex 模式下文本类变量按 UTF-8 字节输出为大写 hex
        let ctx = RenderContext {
            now: Local::now(),
            worker_id: None,
            counter: None,
            seq: None,
        };
        let out = render_hex("${date}", &ctx);
        // "YYYY-MM-DD" → 10 字节 → 20 个 hex 字符
        assert_eq!(out.len(), 20);
        assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(out, out.to_uppercase());
    }

    #[test]
    fn test_uuid_is_valid_format() {
        let out = render_text("id=${uuid}", &common_ctx());
        assert!(Uuid::parse_str(&out[3..]).is_ok(), "应生成合法 UUID");
    }

    #[test]
    fn test_uuid_hex_no_hyphens() {
        // hex 模式下 uuid 输出为纯 32 字符十六进制, 可被 hex_to_bytes 解析为 16 字节
        let out = render_hex("0000${uuid}", &common_ctx());
        let uuid_hex = &out[4..];
        assert_eq!(uuid_hex.len(), 32, "uuid hex 应为 32 字符: {}", uuid_hex);
        assert!(uuid_hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!uuid_hex.contains('-'));
        let bytes = crate::utils::hex::hex_to_bytes(&out);
        assert_eq!(bytes.len(), 18, "前缀 2 字节 + uuid 16 字节");
    }

    #[test]
    fn test_uuid_text_keeps_hyphens() {
        let out = render_text("${uuid}", &common_ctx());
        assert!(out.contains('-'), "文本模式 uuid 应保留连字符: {}", out);
    }

    #[test]
    fn test_random_in_range() {
        for _ in 0..100 {
            let out = render_text("${random:1:10}", &common_ctx());
            let n: i64 = out.parse().unwrap();
            assert!((1..=10).contains(&n));
        }
    }

    #[test]
    fn test_random_equal_min_max() {
        assert_eq!(render_text("${random:5:5}", &common_ctx()), "5");
    }

    #[test]
    fn test_unknown_variable_preserved() {
        assert_eq!(render_text("v=${unknown_var}", &common_ctx()), "v=${unknown_var}");
    }

    #[test]
    fn test_malformed_random_preserved() {
        let ctx = common_ctx();
        assert_eq!(render_text("${random:abc:5}", &ctx), "${random:abc:5}");
        assert_eq!(render_text("${random:1}", &ctx), "${random:1}");
        assert_eq!(render_text("${random:5:1}", &ctx), "${random:5:1}");
    }

    #[test]
    fn test_unclosed_brace_preserved() {
        assert_eq!(render_text("x=${seq y", &common_ctx()), "x=${seq y");
    }

    #[test]
    fn test_hex_numeric_even_length() {
        let ctx = RenderContext {
            now: Local::now(),
            worker_id: Some(12),
            counter: None,
            seq: Some(0),
        };
        let out = render_hex("4142${worker_id}${seq}", &ctx);
        assert_eq!(out, "41420C00");
        assert_eq!(
            crate::utils::hex::hex_to_bytes(&out),
            vec![0x41, 0x42, 0x0C, 0x00]
        );
    }

    #[test]
    fn test_hex_seq_values() {
        for (v, expected) in [(10u64, "0A"), (255, "FF"), (256, "0100")] {
            let ctx = RenderContext::common(Some(v));
            assert_eq!(render_hex("${seq}", &ctx), expected);
        }
    }

    #[test]
    fn test_hex_random_even_length() {
        for _ in 0..100 {
            let out = render_hex("${random:0:255}", &common_ctx());
            assert_eq!(out.len(), 2, "0-255 hex 应为 2 字符: {}", out);
            assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn test_multibyte_literal_safe() {
        // 模板含中文等多字节字符时按 char 边界解析, 不 panic 且字面量完整保留
        let out = render_text("中文-${random:1:1}-文", &common_ctx());
        assert_eq!(out, "中文-1-文");
    }
}