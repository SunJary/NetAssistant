// 压测报文变量替换引擎(薄适配层)
//
// 解析/渲染/时间格式/hex 编码规则统一由 `crate::utils::message_vars` 提供,
// 本模块只负责压测语义的适配:
//   - worker_id / counter / seq 全部 Some(填实值)
//   - 序号无条件消费(与历史行为逐字一致: 每包都 fetch_add)
//
// 支持变量(在文本层替换, hex 模式下替换后再 hex_to_bytes):
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

/// 压测侧渲染上下文: 无条件消费序号, worker_id / counter 填实值
fn stress_context(
    global_seq: &AtomicU64,
    worker_id: usize,
    worker_counter: &mut u64,
) -> crate::utils::message_vars::RenderContext {
    let seq = global_seq.fetch_add(1, Ordering::Relaxed);
    *worker_counter += 1;
    // 压测路径没有接收帧上下文(`rx` 为 None): 与既有行为逐字一致
    crate::utils::message_vars::RenderContext::for_worker(
        Local::now(),
        Some(worker_id),
        Some(*worker_counter),
        Some(seq),
    )
}

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

    let compiled = CompiledTemplate::new(template);
    let mut out = String::with_capacity(template.len() + 32);
    compiled.render(global_seq, worker_id, worker_counter, hex_mode, &mut out);
    out
}

/// 预编译的报文模板(压测调用点使用的薄包装)
///
/// 委托共享引擎; `render` 的签名与旧实现一致, 因此 `client_worker.rs` 调用点无需改动。
pub struct CompiledTemplate(crate::utils::message_vars::CompiledTemplate);

impl CompiledTemplate {
    /// 从模板字符串构造预编译模板
    pub fn new(template: &str) -> Self {
        Self(crate::utils::message_vars::CompiledTemplate::new(template))
    }

    /// 原始模板长度(用于调用方预分配缓冲)
    pub fn template_len(&self) -> usize {
        self.0.template_len()
    }

    /// 渲染到给定的 String 缓冲(调用方负责 clear + 预分配)
    ///
    /// 压测语义: 无条件消费一个全局序号, 本地计数每包 +1。
    pub fn render(
        &self,
        global_seq: &AtomicU64,
        worker_id: usize,
        worker_counter: &mut u64,
        hex_mode: bool,
        out: &mut String,
    ) {
        let ctx = stress_context(global_seq, worker_id, worker_counter);
        self.0.render(&ctx, hex_mode, out);
    }
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
        assert!(uuid::Uuid::parse_str(uuid_part).is_ok(), "应生成合法 UUID");
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
