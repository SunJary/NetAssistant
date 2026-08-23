// 报文变量替换引擎
//
// 支持变量(在文本层替换，hex 模式下替换后再 hex_to_bytes):
//   ${seq}            全局递增序号(所有 worker 共享)
//   ${worker_id}      当前 worker 编号
//   ${counter}        当前 worker 的本地计数(每包+1)
//   ${timestamp}      当前毫秒时间戳(Unix epoch)
//   ${uuid}           随机 UUID v4
//   ${random:min:max} [min,max] 闭区间随机整数
//
// 未知变量(如 ${foo})原样保留。
// hex 模式下，数值变量(seq/worker_id/counter/timestamp/random)输出为零填充偶数长度十六进制。

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Local;
use uuid::Uuid;

/// 渲染单条报文。
///
/// - `template`: 报文模板
/// - `global_seq`: 全局递增计数器(所有 worker 共享)
/// - `worker_id`: 当前 worker 编号
/// - `worker_counter`: 当前 worker 本地计数(每包自增)
/// - `hex_mode`: hex 模式下数值变量格式化为十六进制(偶数长度)
#[allow(dead_code)]
pub fn render_payload(
    template: &str,
    global_seq: &AtomicU64,
    worker_id: usize,
    worker_counter: &mut u64,
    hex_mode: bool,
) -> String {
    // 快速路径: 无变量直接返回
    if !template.contains("${") {
        return template.to_string();
    }

    let seq = global_seq.fetch_add(1, Ordering::Relaxed);
    *worker_counter += 1;
    let local_counter = *worker_counter;
    let timestamp = Local::now().timestamp_millis();
    let uuid = Uuid::new_v4();

    let mut output = String::with_capacity(template.len() + 32);
    // 用 char_indices 遍历以保证 UTF-8 安全(模板可能含中文等多字节字符)
    let chars: Vec<(usize, char)> = template.char_indices().collect();

    let mut ci = 0;
    while ci < chars.len() {
        let (_, ch) = chars[ci];
        if ch == '$' && ci + 1 < chars.len() && chars[ci + 1].1 == '{' {
            // 在剩余子串中查找匹配的 '}'
            let after_brace_byte = chars[ci + 1].0 + chars[ci + 1].1.len_utf8();
            if let Some(close_rel) = template[after_brace_byte..].find('}') {
                let var_name = &template[after_brace_byte..after_brace_byte + close_rel];
                let replacement = resolve_variable(
                    var_name,
                    seq,
                    worker_id,
                    local_counter,
                    timestamp,
                    &uuid,
                    hex_mode,
                );
                output.push_str(&replacement);
                // 推进 ci 到 '}' 之后
                let close_byte = after_brace_byte + close_rel + 1;
                ci = chars.partition_point(|(p, _)| *p < close_byte);
                continue;
            }
        }
        output.push(ch);
        ci += 1;
    }

    output
}

/// 预编译的报文模板段
#[derive(Debug, Clone)]
enum TemplateSegment {
    /// 字面量文本
    Literal(String),
    /// ${seq}
    Seq,
    /// ${worker_id}
    WorkerId,
    /// ${counter}
    Counter,
    /// ${timestamp}
    Timestamp,
    /// ${uuid}
    Uuid,
    /// ${random:min:max} (预解析的 min/max)
    Random(i64, i64),
    /// 未知变量,原样保留
    Unknown(String),
}

/// 预编译的报文模板
///
/// 在 worker 启动时解析一次模板,拆分为段(Literal / 变量),
/// 避免每包重复执行 `char_indices().collect()` 和字符串搜索。
pub struct CompiledTemplate {
    segments: Vec<TemplateSegment>,
    /// 原始模板长度(用于预分配输出缓冲)
    template_len: usize,
}

impl CompiledTemplate {
    /// 从模板字符串构造预编译模板
    pub fn new(template: &str) -> Self {
        let template_len = template.len();
        // 快速路径: 无变量
        if !template.contains("${") {
            return Self {
                segments: vec![TemplateSegment::Literal(template.to_string())],
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
                        segments.push(TemplateSegment::Literal(
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
            segments.push(TemplateSegment::Literal(
                template[literal_start..].to_string(),
            ));
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

    /// 渲染到给定的 String 缓冲(调用方负责 clear + 预分配)
    pub fn render(
        &self,
        global_seq: &AtomicU64,
        worker_id: usize,
        worker_counter: &mut u64,
        hex_mode: bool,
        out: &mut String,
    ) {
        let seq = global_seq.fetch_add(1, Ordering::Relaxed);
        *worker_counter += 1;
        let local_counter = *worker_counter;
        let timestamp = Local::now().timestamp_millis();
        let uuid = Uuid::new_v4();

        for seg in &self.segments {
            match seg {
                TemplateSegment::Literal(s) => out.push_str(s),
                TemplateSegment::Seq => {
                    if hex_mode {
                        out.push_str(&format_hex_u64(seq))
                    } else {
                        out.push_str(&seq.to_string())
                    }
                }
                TemplateSegment::WorkerId => {
                    if hex_mode {
                        out.push_str(&format_hex_u64(worker_id as u64))
                    } else {
                        out.push_str(&worker_id.to_string())
                    }
                }
                TemplateSegment::Counter => {
                    if hex_mode {
                        out.push_str(&format_hex_u64(local_counter))
                    } else {
                        out.push_str(&local_counter.to_string())
                    }
                }
                TemplateSegment::Timestamp => {
                    if hex_mode {
                        out.push_str(&format_hex_i64(timestamp))
                    } else {
                        out.push_str(&timestamp.to_string())
                    }
                }
                // hex 模式下输出纯 32 字符十六进制(去掉连字符), 否则连字符不是合法 hex 字符
                TemplateSegment::Uuid => {
                    if hex_mode {
                        out.push_str(&uuid.simple().to_string().to_uppercase())
                    } else {
                        out.push_str(&uuid.to_string())
                    }
                }
                TemplateSegment::Random(min, max) => {
                    let span = (*max - *min) as u64 + 1;
                    let val = *min + (random_u64() % span) as i64;
                    if hex_mode {
                        out.push_str(&format_hex_i64(val))
                    } else {
                        out.push_str(&val.to_string())
                    }
                }
                TemplateSegment::Unknown(s) => out.push_str(s),
            }
        }
    }
}

/// 将变量名解析为预编译段
fn parse_segment(var_name: &str) -> TemplateSegment {
    match var_name {
        "seq" => TemplateSegment::Seq,
        "worker_id" => TemplateSegment::WorkerId,
        "counter" => TemplateSegment::Counter,
        "timestamp" => TemplateSegment::Timestamp,
        "uuid" => TemplateSegment::Uuid,
        _ if var_name.starts_with("random:") => {
            let params = &var_name["random:".len()..];
            let parts: Vec<&str> = params.split(':').collect();
            if parts.len() != 2 {
                return TemplateSegment::Unknown(format!("${{{}}}", var_name));
            }
            match (
                parts[0].trim().parse::<i64>(),
                parts[1].trim().parse::<i64>(),
            ) {
                (Ok(min), Ok(max)) if min <= max => TemplateSegment::Random(min, max),
                _ => TemplateSegment::Unknown(format!("${{{}}}", var_name)),
            }
        }
        _ => TemplateSegment::Unknown(format!("${{{}}}", var_name)),
    }
}

/// 解析单个变量名，返回替换文本。未知变量原样返回 `${name}`。
#[allow(dead_code)]
fn resolve_variable(
    var_name: &str,
    seq: u64,
    worker_id: usize,
    counter: u64,
    timestamp: i64,
    uuid: &Uuid,
    hex_mode: bool,
) -> String {
    match var_name {
        "seq" => {
            if hex_mode {
                format_hex_u64(seq)
            } else {
                seq.to_string()
            }
        }
        "worker_id" => {
            if hex_mode {
                format_hex_u64(worker_id as u64)
            } else {
                worker_id.to_string()
            }
        }
        "counter" => {
            if hex_mode {
                format_hex_u64(counter)
            } else {
                counter.to_string()
            }
        }
        "timestamp" => {
            if hex_mode {
                format_hex_i64(timestamp)
            } else {
                timestamp.to_string()
            }
        }
        "uuid" => {
            if hex_mode {
                uuid.simple().to_string().to_uppercase()
            } else {
                uuid.to_string()
            }
        }
        _ if var_name.starts_with("random:") => resolve_random(var_name, hex_mode),
        _ => format!("${{{}}}", var_name), // 未知变量原样保留
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

/// 将 i64 格式化为偶数长度大写十六进制字符串
fn format_hex_i64(v: i64) -> String {
    format_hex_u64(v as u64)
}

/// 解析 ${random:min:max} → [min, max] 闭区间随机整数
///
/// 使用 std 的 RandomState 哈希作为熵源(非加密用途，仅为压测报文变化)，
/// 避免引入 rand 依赖。
#[allow(dead_code)]
fn resolve_random(var_name: &str, hex_mode: bool) -> String {
    let params = &var_name["random:".len()..];
    let parts: Vec<&str> = params.split(':').collect();
    if parts.len() != 2 {
        return format!("${{{}}}", var_name);
    }
    let min = match parts[0].trim().parse::<i64>() {
        Ok(v) => v,
        Err(_) => return format!("${{{}}}", var_name),
    };
    let max = match parts[1].trim().parse::<i64>() {
        Ok(v) => v,
        Err(_) => return format!("${{{}}}", var_name),
    };
    if min > max {
        return format!("${{{}}}", var_name);
    }
    let span = (max - min) as u64 + 1;
    let val = min + (random_u64() % span) as i64;
    if hex_mode {
        format_hex_i64(val)
    } else {
        val.to_string()
    }
}

/// 用 std RandomState 产生一个伪随机 u64
fn random_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq() -> AtomicU64 {
        AtomicU64::new(0)
    }

    #[test]
    fn test_no_variable_fast_path() {
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("hello world", &s, 0, &mut c, false);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn test_seq_global_increment() {
        let s = seq();
        let mut c = 0u64;
        let a = render_payload("req-${seq}", &s, 0, &mut c, false);
        let b = render_payload("req-${seq}", &s, 0, &mut c, false);
        assert_eq!(a, "req-0");
        assert_eq!(b, "req-1");
    }

    #[test]
    fn test_worker_id_and_counter() {
        let s = seq();
        let mut c = 0u64;
        let a = render_payload("w${worker_id}-c${counter}", &s, 7, &mut c, false);
        let b = render_payload("w${worker_id}-c${counter}", &s, 7, &mut c, false);
        assert_eq!(a, "w7-c1");
        assert_eq!(b, "w7-c2");
    }

    #[test]
    fn test_multiple_vars_in_one_template() {
        let s = seq();
        let mut c = 5u64;
        let ts_before = Local::now().timestamp_millis();
        let out = render_payload(
            "${seq}|${worker_id}|${counter}|${timestamp}",
            &s,
            3,
            &mut c,
            false,
        );
        let ts_after = Local::now().timestamp_millis();
        // timestamp 在 render_payload 内部取,允许 ±几毫秒误差
        let expected_prefix = "0|3|6|";
        assert!(out.starts_with(expected_prefix), "got: {}", out);
        let ts_str = &out[expected_prefix.len()..];
        let ts: i64 = ts_str.parse().expect("timestamp 应为数字");
        assert!(
            ts >= ts_before && ts <= ts_after + 1,
            "timestamp {} 不在 [{}, {}] 范围",
            ts,
            ts_before,
            ts_after
        );
    }

    #[test]
    fn test_uuid_is_valid_format() {
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("id=${uuid}", &s, 0, &mut c, false);
        let uuid_part = &out[3..];
        assert!(Uuid::parse_str(uuid_part).is_ok(), "应生成合法 UUID");
    }

    #[test]
    fn test_uuid_hex_no_hyphens() {
        // hex 模式下 uuid 输出为纯 32 字符十六进制(无连字符), 且可被 hex_to_bytes 解析为 16 字节
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("0000${uuid}", &s, 0, &mut c, true);
        let uuid_hex = &out[4..];
        assert_eq!(uuid_hex.len(), 32, "uuid hex 应为 32 字符: {}", uuid_hex);
        assert!(
            uuid_hex.chars().all(|ch| ch.is_ascii_hexdigit()),
            "uuid hex 应全为十六进制字符: {}",
            uuid_hex
        );
        assert!(!uuid_hex.contains('-'), "uuid hex 不应包含连字符");
        let bytes = crate::utils::hex::hex_to_bytes(&out);
        assert_eq!(bytes.len(), 18, "前缀 2 字节 + uuid 16 字节");
    }

    #[test]
    fn test_uuid_text_keeps_hyphens() {
        // 文本模式下 uuid 输出保持标准带连字符格式(向后兼容)
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("${uuid}", &s, 0, &mut c, false);
        assert!(out.contains('-'), "文本模式 uuid 应保留连字符: {}", out);
    }

    #[test]
    fn test_random_in_range() {
        let s = seq();
        let mut c = 0u64;
        for _ in 0..100 {
            let out = render_payload("${random:1:10}", &s, 0, &mut c, false);
            let n: i64 = out.parse().unwrap();
            assert!((1..=10).contains(&n));
        }
    }

    #[test]
    fn test_random_equal_min_max() {
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("${random:5:5}", &s, 0, &mut c, false);
        assert_eq!(out, "5");
    }

    #[test]
    fn test_unknown_variable_preserved() {
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("v=${unknown_var}", &s, 0, &mut c, false);
        assert_eq!(out, "v=${unknown_var}");
    }

    #[test]
    fn test_malformed_random_preserved() {
        let s = seq();
        let mut c = 0u64;
        assert_eq!(
            render_payload("${random:abc:5}", &s, 0, &mut c, false),
            "${random:abc:5}"
        );
        assert_eq!(
            render_payload("${random:1}", &s, 0, &mut c, false),
            "${random:1}"
        );
        assert_eq!(
            render_payload("${random:5:1}", &s, 0, &mut c, false),
            "${random:5:1}"
        );
    }

    #[test]
    fn test_unclosed_brace_preserved() {
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("x=${seq y", &s, 0, &mut c, false);
        // 没有 } → 原样保留
        assert_eq!(out, "x=${seq y");
    }

    #[test]
    fn test_hex_template_with_var() {
        // hex 模式: 数值变量输出为偶数长度十六进制
        // worker_id=12 → hex "0C", seq=0 → hex "00"
        let s = seq();
        let mut c = 0u64;
        let out = render_payload("4142${worker_id}${seq}", &s, 12, &mut c, true);
        assert_eq!(out, "41420C00");
        let bytes = crate::utils::hex::hex_to_bytes(&out);
        assert_eq!(bytes, vec![0x41, 0x42, 0x0C, 0x00]);
    }

    #[test]
    fn test_hex_seq_values() {
        // hex 模式下 seq 递增: 0→00, 10→0A, 255→FF, 256→0100
        let s = AtomicU64::new(10);
        let mut c = 0u64;
        let out = render_payload("${seq}", &s, 0, &mut c, true);
        assert_eq!(out, "0A");

        let s2 = AtomicU64::new(255);
        let out2 = render_payload("${seq}", &s2, 0, &mut c, true);
        assert_eq!(out2, "FF");

        let s3 = AtomicU64::new(256);
        let out3 = render_payload("${seq}", &s3, 0, &mut c, true);
        assert_eq!(out3, "0100"); // 偶数长度
    }

    #[test]
    fn test_hex_random_even_length() {
        let s = seq();
        let mut c = 0u64;
        for _ in 0..100 {
            let out = render_payload("${random:0:255}", &s, 0, &mut c, true);
            assert_eq!(out.len(), 2, "0-255 hex 应为 2 字符: {}", out);
            assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
}
