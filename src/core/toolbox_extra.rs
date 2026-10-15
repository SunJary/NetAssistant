// 工具箱扩展组（时间 / JSON / 编码）的纯计算逻辑。
//
// 与 `core/toolbox.rs` 的 SelectionTool 注册表解耦：这三组的输入输出形态
// （双向自动识别、一输入多表示）不适合「mode + 单工具单结果」模型，
// 由 UI 按分组直接调用这里的纯函数；全部无头可单测。

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Weekday};

// ============== 时间：时间戳 ⇄ 日期双向识别 ==============

/// 同一时刻的七种等价表示（时间组输出行）
#[derive(Debug, Clone, PartialEq)]
pub struct TimeView {
    /// 秒级时间戳（10 位量级）
    pub ts_sec: String,
    /// 毫秒级时间戳（13 位量级）
    pub ts_ms: String,
    /// 本地时间 "YYYY-MM-DD HH:MM:SS"
    pub local: String,
    /// UTC 时间 "YYYY-MM-DD HH:MM:SS"
    pub utc: String,
    /// 本地时区 ISO 8601（带 ±HH:MM 偏移）
    pub iso: String,
    /// 星期（面板负责本地化文案）
    pub weekday: Weekday,
    /// 相对当前时刻（面板负责本地化文案）
    pub relative: RelativeTime,
}

/// 相对当前时刻：1 分钟内为 Now；其余为数量 + 单位，future 表示未来
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeTime {
    Now,
    Minute(u64, bool),
    Hour(u64, bool),
    Day(u64, bool),
}

/// 含时间的日期格式（按本地时区解释）
const DATETIME_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
    "%Y/%m/%d %H:%M:%S",
    "%Y-%m-%d %H:%M",
];

/// 仅日期的格式（午夜 00:00:00）
const DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%Y/%m/%d"];

/// 任意输入 → 七种表示。
///
/// - 纯数字按时间戳：量级 ≥ 1e11 视为毫秒，否则视为秒（覆盖 10/13 位主流写法）
/// - 其余按日期文本解析（按本地时区解释）
/// - 空输入 / 无法识别返回 None
pub fn parse_time_input(input: &str) -> Option<TimeView> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(ms) = timestamp_digits_to_millis(trimmed) {
        return time_view_from_millis(ms);
    }
    // ISO 8601 带时区偏移（...+08:00 / ...Z）：直接按声明时区换算
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return time_view_from_millis(dt.timestamp_millis());
    }
    let naive = parse_date_text(trimmed)?;
    // 按本地时区解释；DST 歧义时刻（.single() 非 Some）视为无法识别
    let dt = Local.from_local_datetime(&naive).single()?;
    time_view_from_millis(dt.timestamp_millis())
}

/// 当前时刻的毫秒级时间戳（实时条显示与「填入」按钮共用）
pub fn now_millis() -> i64 {
    Local::now().timestamp_millis()
}

/// 当前时刻的毫秒级时间戳字符串（供「填入当前」按钮填充输入框）
pub fn now_millis_string() -> String {
    now_millis().to_string()
}

/// 当前时刻的本地时间字符串（「日期 → 时间戳」块默认值与「填入当前」共用）
pub fn now_local_string() -> String {
    match time_view_from_millis(now_millis()) {
        Some(view) => view.local,
        None => String::new(),
    }
}

/// 纯数字字符串 → 归一化毫秒；非纯数字或超长返回 None
fn timestamp_digits_to_millis(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // 16 位封顶，避免无意义的大数解析
    if s.len() > 16 {
        return None;
    }
    let n: i64 = s.parse().ok()?;
    // 1e11 秒 ≈ 公元 5138 年：大于等于该量级只可能是毫秒
    Some(if n >= 100_000_000_000 { n } else { n * 1000 })
}

/// 毫秒 → 七种表示
fn time_view_from_millis(ms: i64) -> Option<TimeView> {
    let secs = ms.div_euclid(1000);
    let nanos = (ms.rem_euclid(1000) * 1_000_000) as u32;
    let utc_dt = DateTime::from_timestamp(secs, nanos)?;
    let local_dt = utc_dt.with_timezone(&Local);
    let fmt = "%Y-%m-%d %H:%M:%S";
    Some(TimeView {
        ts_sec: secs.to_string(),
        ts_ms: ms.to_string(),
        local: local_dt.format(fmt).to_string(),
        utc: utc_dt.format(fmt).to_string(),
        iso: local_dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        weekday: local_dt.weekday(),
        relative: relative_from_millis(ms),
    })
}

/// 相对当前时刻：1 分钟内为「刚刚」，其余按分钟/小时/天归档，未来为正
pub fn relative_from_millis(ms: i64) -> RelativeTime {
    let diff_secs = (Local::now().timestamp_millis() - ms).div_euclid(1000);
    let future = diff_secs < 0;
    let abs = diff_secs.unsigned_abs();
    if abs < 60 {
        return RelativeTime::Now;
    }
    if abs < 3600 {
        RelativeTime::Minute(abs / 60, future)
    } else if abs < 86_400 {
        RelativeTime::Hour(abs / 3600, future)
    } else {
        RelativeTime::Day(abs / 86_400, future)
    }
}

/// 日期文本 → 本地 naive 时刻；无法解析返回 None
fn parse_date_text(s: &str) -> Option<NaiveDateTime> {
    for fmt in DATETIME_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
    }
    for fmt in DATE_FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(s, fmt) {
            return Some(date.and_hms_opt(0, 0, 0)?);
        }
    }
    None
}

// ============== Base64 编解码（标准字母表，带填充） ==============

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 文本 → Base64（UTF-8 字节编码，标准字母表 + `=` 填充）
pub fn base64_encode(text: &str) -> String {
    base64_encode_bytes(text.as_bytes())
}

/// 字节 → Base64（标准字母表 + `=` 填充）
fn base64_encode_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[(n >> 18) as usize & 0x3F] as char);
        out.push(B64_ALPHABET[(n >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[(n >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[n as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

/// Base64 → 文本；非法字符/长度/填充或结果非 UTF-8 返回 Err。
/// 宽松点：忽略全部空白字符（换行分段的常见写法）。
pub fn base64_decode(text: &str) -> Result<String, ()> {
    let cleaned: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if cleaned.is_empty() {
        return Ok(String::new());
    }
    if cleaned.len() % 4 != 0 {
        return Err(());
    }
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    for quad in cleaned.chunks(4) {
        let pad = quad.iter().filter(|&&b| b == b'=').count();
        // '=' 至多 2 个，且只能出现在组内末尾
        if pad > 2 || quad[..4 - pad].iter().any(|&b| b == b'=') {
            return Err(());
        }
        let mut n: u32 = 0;
        for &b in &quad[..4 - pad] {
            n = (n << 6) | sextet(b)?;
        }
        n <<= 6 * pad as u32;
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8][..3 - pad]);
    }
    String::from_utf8(out).map_err(|_| ())
}

/// 单个 Base64 字符 → 6 bit 值
fn sextet(b: u8) -> Result<u32, ()> {
    Ok(match b {
        b'A'..=b'Z' => (b - b'A') as u32,
        b'a'..=b'z' => (b - b'a' + 26) as u32,
        b'0'..=b'9' => (b - b'0' + 52) as u32,
        b'+' => 62,
        b'/' => 63,
        _ => return Err(()),
    })
}

// ============== URL 百分号编解码 ==============

/// RFC 3986 unreserved 字符：A-Z a-z 0-9 - _ . ~
fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')
}

/// URL 编码：unreserved 字符直出，其余按 UTF-8 字节输出 %XX（大写）
pub fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// URL 解码：`%XX` 字节序列按 UTF-8 还原，`+` 视为空格；非法序列返回 Err。
/// 宽松点：未经编码的多字节 UTF-8 字符原样保留。
pub fn url_decode(text: &str) -> Result<String, ()> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let hex = bytes.get(i + 1..i + 3).ok_or(())?;
                out.push(hex_val(hex[0])? * 16 + hex_val(hex[1])?);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

/// 单个十六进制字符 → 4 bit 值
fn hex_val(b: u8) -> Result<u8, ()> {
    Ok(match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => return Err(()),
    })
}

// ============== JSON 美化 / 压缩 ==============

/// JSON 文本 → serde_json::Value：先按标准 JSON 解析，失败回退 JSON5 宽松解析
/// （key 无引号、单引号字符串、尾逗号、注释——兼容从 JS 复制的对象字面量；
/// 函数值 / undefined 等 JS 特有内容无法转换，返回 None）
fn parse_json_loose(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text)
        .ok()
        .or_else(|| json5::from_str(text).ok())
}

/// JSON 美化（缩进 2 空格）；非法 JSON 返回 None
pub fn json_pretty(text: &str) -> Option<String> {
    let value = parse_json_loose(text)?;
    serde_json::to_string_pretty(&value).ok()
}

/// JSON 压缩（无多余空白）；非法 JSON 返回 None
pub fn json_minify(text: &str) -> Option<String> {
    let value = parse_json_loose(text)?;
    serde_json::to_string(&value).ok()
}

// ============== 哈希摘要（MD5 / SHA-1 / SHA-2，UTF-8 字节） ==============

use md5::Md5;
use sha1::Sha1;
// `digest` 未直接声明依赖，经 sha2 重导出使用（md-5/sha1 同源同版本）
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};

use argon2::{Algorithm, Argon2, Params, Version};

/// 任意文本 → 小写 hex 摘要
fn digest_hex<D: Digest>(text: &str) -> String {
    let mut hasher = D::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn hash_md5(text: &str) -> String {
    digest_hex::<Md5>(text)
}

pub fn hash_sha1(text: &str) -> String {
    digest_hex::<Sha1>(text)
}

pub fn hash_sha224(text: &str) -> String {
    digest_hex::<Sha224>(text)
}

pub fn hash_sha256(text: &str) -> String {
    digest_hex::<Sha256>(text)
}

pub fn hash_sha384(text: &str) -> String {
    digest_hex::<Sha384>(text)
}

pub fn hash_sha512(text: &str) -> String {
    digest_hex::<Sha512>(text)
}

// ============== 密码哈希（PHC / `$` 格式 KDF） ==============

// KDF 与普通摘要的区别：慢（成本因子）+ 加盐，专用于口令存储。
// 与生产库 hash 一致：盐由函数内部随机生成并内嵌于 `$` 串，调用方无需提供；
// 因此每次调用结果都不同（随机盐），UI 层在密码输入防抖后于后台线程生成并缓存
// （见 app.rs 订阅），无关重绘不刷新，保证屏幕上的值与复制值一致。

/// bcrypt（`$2b$` 格式）：cost=6，内置随机盐
pub fn password_bcrypt(password: &str) -> Result<String, ()> {
    bcrypt::hash(password, 6)
        .map(|parts| parts.to_string())
        .map_err(|_| ())
}

/// Argon2id（PHC 字符串格式）：m=8MiB、t=1、p=1，输出 32 字节，内置随机盐
pub fn password_argon2id(password: &str) -> Result<String, ()> {
    use argon2::password_hash::PasswordHasher;
    let salt_bytes = random_salt_bytes();
    let params = Params::new(8192, 1, 1, Some(32)).map_err(|_| ())?;
    let ctx = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    ctx.hash_password_with_salt(password.as_bytes(), &salt_bytes)
        .map(|hash| hash.to_string())
        .map_err(|_| ())
}

/// PBKDF2-SHA256（PHC 字符串格式）：10000 轮，输出 32 字节，内置随机盐
pub fn password_pbkdf2_sha256(password: &str) -> Result<String, ()> {
    let salt = random_salt_bytes();
    let mut dk = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, 10_000, &mut dk);
    Ok(format!(
        "$pbkdf2-sha256$i=10000,l=32${}${}",
        b64_nopad(&salt),
        b64_nopad(&dk)
    ))
}

/// 标准 base64 去填充（PHC 规范的 B64 编码）
fn b64_nopad(bytes: &[u8]) -> String {
    base64_encode_bytes(bytes)
        .trim_end_matches('=')
        .to_string()
}

/// 16 字节系统熵（密码哈希随机盐）
fn random_salt_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("系统熵源不可用");
    bytes
}

// ============== 文件摘要（256KB 块 · 单遍喂全部已选算法） ==============

/// 文件摘要算法描述（数组顺序 = UI 行序）
pub struct HashAlgo {
    pub id: &'static str,
    pub label: &'static str,
    /// 摘要 hex 长度（对比值按长度映射算法）
    pub hex_len: usize,
}

pub const FILE_HASH_ALGOS: &[HashAlgo] = &[
    HashAlgo { id: "md5", label: "MD5", hex_len: 32 },
    HashAlgo { id: "sha1", label: "SHA-1", hex_len: 40 },
    HashAlgo { id: "sha224", label: "SHA-224", hex_len: 56 },
    HashAlgo { id: "sha256", label: "SHA-256", hex_len: 64 },
    HashAlgo { id: "sha384", label: "SHA-384", hex_len: 96 },
    HashAlgo { id: "sha512", label: "SHA-512", hex_len: 128 },
];

pub fn file_hash_algo(id: &str) -> Option<&'static HashAlgo> {
    FILE_HASH_ALGOS.iter().find(|a| a.id == id)
}

/// 摘要 hex 长度 → 算法（对比值自动勾选）
pub fn file_hash_algo_by_hex_len(len: usize) -> Option<&'static HashAlgo> {
    FILE_HASH_ALGOS.iter().find(|a| a.hex_len == len)
}

/// 读取块大小：哈希总工作量只与总字节数成正比，块大小仅影响内存占用
/// 与系统调用次数，256KB 已越过两者的收益拐点（更大无收益只有内存代价）。
const FILE_HASH_CHUNK: usize = 256 * 1024;

/// 固定算法集的具体 hasher（Digest 非 dyn-safe，用枚举分发）
enum AnyHasher {
    Md5(Md5),
    Sha1(Sha1),
    Sha224(Sha224),
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
}

impl AnyHasher {
    fn new(id: &str) -> Option<Self> {
        Some(match id {
            "md5" => AnyHasher::Md5(Md5::new()),
            "sha1" => AnyHasher::Sha1(Sha1::new()),
            "sha224" => AnyHasher::Sha224(Sha224::new()),
            "sha256" => AnyHasher::Sha256(Sha256::new()),
            "sha384" => AnyHasher::Sha384(Sha384::new()),
            "sha512" => AnyHasher::Sha512(Sha512::new()),
            _ => return None,
        })
    }

    fn update(&mut self, chunk: &[u8]) {
        match self {
            AnyHasher::Md5(h) => h.update(chunk),
            AnyHasher::Sha1(h) => h.update(chunk),
            AnyHasher::Sha224(h) => h.update(chunk),
            AnyHasher::Sha256(h) => h.update(chunk),
            AnyHasher::Sha384(h) => h.update(chunk),
            AnyHasher::Sha512(h) => h.update(chunk),
        }
    }

    fn finalize_hex(self) -> String {
        match self {
            AnyHasher::Md5(h) => hex::encode(h.finalize()),
            AnyHasher::Sha1(h) => hex::encode(h.finalize()),
            AnyHasher::Sha224(h) => hex::encode(h.finalize()),
            AnyHasher::Sha256(h) => hex::encode(h.finalize()),
            AnyHasher::Sha384(h) => hex::encode(h.finalize()),
            AnyHasher::Sha512(h) => hex::encode(h.finalize()),
        }
    }
}

pub struct FileHashResult {
    pub id: &'static str,
    pub hex: String,
}

/// 单遍顺序读取文件，同一循环喂给全部传入算法（磁盘只读一遍，
/// 勾选多个算法不叠加 IO 成本）。结果顺序与 ids 一致；未知 id 报错。
pub fn file_digest_set(path: &std::path::Path, ids: &[&'static str]) -> std::io::Result<Vec<FileHashResult>> {
    use std::io::Read;

    let mut hashers = ids
        .iter()
        .map(|id| {
            AnyHasher::new(id)
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("未知哈希算法：{id}")))
        })
        .collect::<std::io::Result<Vec<_>>>()?;

    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; FILE_HASH_CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for hasher in &mut hashers {
            hasher.update(&buf[..n]);
        }
    }

    Ok(hashers
        .into_iter()
        .zip(ids.iter())
        .map(|(h, id)| FileHashResult { id, hex: h.finalize_hex() })
        .collect())
}

/// 字节数自适应显示（1024 进制，与资源管理器一致）
pub fn format_file_size(bytes: u64) -> String {
    const UNIT: f64 = 1024.0;
    if bytes < UNIT as u64 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= UNIT && unit < 3 {
        value /= UNIT;
        unit += 1;
    }
    let name = ["KB", "MB", "GB"][unit - 1];
    if value >= 100.0 {
        format!("{:.0} {name}", value)
    } else {
        format!("{value:.1} {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 时间 ----

    #[test]
    fn test_parse_timestamp_seconds() {
        // 10 位按秒（UTC 期望值与时区无关）
        let view = parse_time_input("1700000000").unwrap();
        assert_eq!(view.utc, "2023-11-14 22:13:20");
        assert_eq!(view.ts_sec, "1700000000");
        assert_eq!(view.ts_ms, "1700000000000");
        assert!(!view.local.is_empty());
    }

    #[test]
    fn test_parse_timestamp_millis() {
        // 13 位按毫秒，与秒级等价
        let view = parse_time_input("1700000000000").unwrap();
        assert_eq!(view.utc, "2023-11-14 22:13:20");
    }

    #[test]
    fn test_parse_date_to_timestamp() {
        // 日期按本地时区解析，与 chrono 本地构造自洽（不依赖运行机器时区）
        let view = parse_time_input("2026-01-02 03:04:05").unwrap();
        let expected = Local
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .unwrap();
        assert_eq!(view.ts_sec, expected.timestamp().to_string());
        // 双向一致：换算出的秒级时间戳再解析，本地时间应回到原样
        assert_eq!(parse_time_input(&view.ts_sec).unwrap().local, "2026-01-02 03:04:05");
    }

    #[test]
    fn test_parse_date_only_and_slash() {
        // 仅日期按本地午夜解释，本地时间字符串与时区无关
        assert_eq!(parse_time_input("2026-10-11").unwrap().local, "2026-10-11 00:00:00");
        assert_eq!(parse_time_input("2026/10/11").unwrap().local, "2026-10-11 00:00:00");
        assert_eq!(
            parse_time_input("2026-10-11T12:30:45").unwrap().local,
            "2026-10-11 12:30:45"
        );
    }

    #[test]
    fn test_parse_time_invalid() {
        assert!(parse_time_input("").is_none());
        assert!(parse_time_input("   ").is_none());
        assert!(parse_time_input("abc").is_none());
        assert!(parse_time_input("2026-13-01").is_none());
        assert!(parse_time_input("17000000000000000000").is_none()); // 超 16 位
    }

    #[test]
    fn test_parse_rfc3339_with_offset() {
        // 带时区偏移的 ISO 8601：+08:00 与 Z 指同一时刻（UTC 视角一致）
        let view = parse_time_input("2026-10-11T18:30:00+08:00").unwrap();
        assert_eq!(view.utc, "2026-10-11 10:30:00");
        let view_z = parse_time_input("2026-10-11T10:30:00Z").unwrap();
        assert_eq!(view_z.utc, "2026-10-11 10:30:00");
        assert_eq!(view.ts_sec, view_z.ts_sec);
        // 本地视角下 ISO 行应回显本地时区偏移
        assert!(view.iso.contains('+') || view.iso.contains('-') || view.iso.contains('Z'));
    }

    #[test]
    fn test_time_view_weekday_and_relative() {
        let view = parse_time_input("2026-10-11T18:30:00+08:00").unwrap();
        // 2026-10-11 是星期日（日历事实，与时区无关——UTC 与 +08:00 同一天）
        assert_eq!(view.weekday, Weekday::Sun);
        // 相对时间只断言与当前时刻有固定偏移的值；
        // 固定时刻的相对档位会随时钟逼近 ±1 小时窗口而漂移（当天 18:30 前后闪断），不断言
        let just_now = relative_from_millis(now_millis() - 10_000);
        assert_eq!(just_now, RelativeTime::Now);
        let minutes_ago = relative_from_millis(now_millis() - 5 * 60_000);
        assert_eq!(minutes_ago, RelativeTime::Minute(5, false));
        let minutes_later = relative_from_millis(now_millis() + 5 * 60_000);
        assert_eq!(minutes_later, RelativeTime::Minute(5, true));
    }

    // ---- Base64 ----

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_encode("ok"), "b2s=");
        assert_eq!(base64_encode("abc"), "YWJj");
        assert_eq!(base64_encode("ab"), "YWI=");
        assert_eq!(base64_encode("a"), "YQ==");
        assert_eq!(base64_encode("你好"), "5L2g5aW9");
    }

    #[test]
    fn test_base64_roundtrip() {
        for source in ["", "ok", "你好，网络调试! \n\t 中文ABC 0123 ~!@#$%^&*()"] {
            assert_eq!(base64_decode(&base64_encode(source)).unwrap(), source);
        }
    }

    #[test]
    fn test_base64_decode_whitespace_tolerant() {
        assert_eq!(base64_decode("b2\n\ts =").unwrap(), "ok");
    }

    #[test]
    fn test_base64_decode_invalid() {
        assert!(base64_decode("a").is_err()); // 长度不足
        assert!(base64_decode("abcde").is_err()); // 长度非 4 倍数
        assert!(base64_decode("ab!c").is_err()); // 非法字符
        assert!(base64_decode("a=b=").is_err()); // '=' 不在末尾
        assert!(base64_decode("====").is_err()); // 填充超 2 个
        assert!(base64_decode("//8=").is_err()); // 解码结果非 UTF-8
    }

    // ---- URL ----

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("a b&c=1"), "a%20b%26c%3D1");
        assert_eq!(url_encode("你好"), "%E4%BD%A0%E5%A5%BD");
        assert_eq!(url_encode("a.b~c-d_e"), "a.b~c-d_e"); // unreserved 直出
        assert_eq!(url_encode(""), "");
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("a%20b%26c%3D1").unwrap(), "a b&c=1");
        assert_eq!(url_decode("%E4%BD%A0%E5%A5%BD").unwrap(), "你好");
        assert_eq!(url_decode("a+b").unwrap(), "a b"); // '+' 视为空格
        assert_eq!(url_decode("你好").unwrap(), "你好"); // 未编码字符宽松保留
    }

    #[test]
    fn test_url_roundtrip() {
        for source in ["", "hello world", "q=网络调试&lang=zh-CN", "~/.path?a=1;b"] {
            assert_eq!(url_decode(&url_encode(source)).unwrap(), source);
        }
    }

    #[test]
    fn test_url_decode_invalid() {
        assert!(url_decode("%G1").is_err()); // 非 hex
        assert!(url_decode("%A").is_err()); // 截断
        assert!(url_decode("%E4%BD").is_err()); // 字节序列非 UTF-8
    }

    // ---- JSON ----

    #[test]
    fn test_json_pretty_and_minify() {
        let pretty = json_pretty(r#"{"a":1,"b":[true,null]}"#).unwrap();
        assert_eq!(pretty, "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    null\n  ]\n}");
        assert_eq!(json_minify(r#" { "a" : 1 } "#).unwrap(), r#"{"a":1}"#);
        // 标量也是合法 JSON
        assert_eq!(json_pretty("42").unwrap(), "42");
    }

    #[test]
    fn test_json_invalid() {
        assert!(json_pretty("").is_none());
        // `{a:1}` 现已由 JSON5 回退解析成功，真正非法的输入（截断/纯文本）仍为 None
        assert!(json_pretty("{a:").is_none());
        assert!(json_pretty("just text").is_none());
        assert!(json_minify("[1,2").is_none());
    }

    // ---- 哈希 ----

    #[test]
    fn test_hash_known_vectors() {
        // cmd5 示例输入 "123456" 的公开摘要值
        assert_eq!(hash_md5("123456"), "e10adc3949ba59abbe56e057f20f883e");
        assert_eq!(
            hash_sha1("123456"),
            "7c4a8d09ca3762af61e59520943dc26494f8941b"
        );
        assert_eq!(
            hash_sha256("123456"),
            "8d969eef6ecad3c29a3a629280e686cf0c3f5d5a86aff3ca12020c923adc6c92"
        );
        assert_eq!(hash_sha224("").len(), 56);
        assert_eq!(hash_sha384("").len(), 96);
        assert_eq!(hash_sha512("").len(), 128);
    }

    #[test]
    fn test_hash_empty_and_lengths() {
        assert_eq!(hash_md5(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hash_sha1("").len(), 40);
        assert_eq!(hash_sha224("").len(), 56);
        assert_eq!(hash_sha256("").len(), 64);
        assert_eq!(hash_sha384("").len(), 96);
        assert_eq!(hash_sha512("").len(), 128);
        // UTF-8 多字节输入按字节摘要且输出稳定
        assert_eq!(
            hash_md5("你好"),
            "7eca689f0d3389d9dea66ae112e5cfd7"
        );
    }

    // ---- 密码哈希（KDF） ----

    #[test]
    fn test_password_hash_formats() {
        let bcrypt_hash = password_bcrypt("123456").unwrap();
        assert!(bcrypt_hash.starts_with("$2b$06$"), "格式不符: {bcrypt_hash}");
        let argon2_hash = password_argon2id("123456").unwrap();
        assert!(
            argon2_hash.starts_with("$argon2id$v=19$m=8192,t=1,p=1$"),
            "格式不符: {argon2_hash}"
        );
        let pbkdf2_hash = password_pbkdf2_sha256("123456").unwrap();
        assert!(
            pbkdf2_hash.starts_with("$pbkdf2-sha256$i=10000,l=32$"),
            "格式不符: {pbkdf2_hash}"
        );
        // PBKDF2 PHC 串结构：盐 22 字符（16 字节）、摘要 43 字符（32 字节）
        let parts: Vec<&str> = pbkdf2_hash.split('$').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[3].len(), 22);
        assert_eq!(parts[4].len(), 43);
        // 随机盐：同密码两次调用结果必不相同
        assert_ne!(bcrypt_hash, password_bcrypt("123456").unwrap());
        assert_ne!(argon2_hash, password_argon2id("123456").unwrap());
        assert_ne!(pbkdf2_hash, password_pbkdf2_sha256("123456").unwrap());
    }

    #[test]
    fn test_password_hash_verify_roundtrip() {
        // 生成的哈希必须能通过各自算法的校验（bcrypt / Argon2 官方 crate 验证）
        let bcrypt_hash = password_bcrypt("p@ss").unwrap();
        assert!(bcrypt::verify("p@ss", &bcrypt_hash).unwrap());

        use argon2::password_hash::phc::PasswordHash as Ph;
        use argon2::password_hash::PasswordVerifier;
        let argon2_hash = password_argon2id("p@ss").unwrap();
        let parsed = Ph::new(&argon2_hash).unwrap();
        assert!(Argon2::default().verify_password("p@ss".as_bytes(), &parsed).is_ok());
        // 错误密码必须验证失败
        assert!(Argon2::default().verify_password(b"wrong", &parsed).is_err());
    }

    #[test]
    fn test_json5_loose_fallback() {
        // JS 对象字面量（无引号 key + 单引号 + 尾逗号 + 注释）→ 归一化为标准 JSON
        let loose = "{\n  // 行注释\n  name: 'net-assistant',\n  ports: [8000, 8001,],\n  debug: true,\n}";
        let pretty = json_pretty(loose).unwrap();
        let value: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(value["name"], "net-assistant");
        assert_eq!(value["ports"][1], 8001);
        // 压缩走同一解析路径
        assert!(json_minify(loose).unwrap().contains("\"ports\":[8000,8001]"));
        // 标准 JSON 仍走原路径（结果不变）
        assert_eq!(json_minify("{\"a\":1}").unwrap(), "{\"a\":1}");
        // JS 特有内容（函数 / undefined）两种解析都失败 → None
        assert!(json_pretty("{fn: () => {}}").is_none());
        assert!(json_minify("{v: undefined}").is_none());
    }
}

#[cfg(test)]
mod file_hash_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TMP_SEQ: AtomicUsize = AtomicUsize::new(0);

    /// 临时文件（内容 = content，测试结束删除）
    fn temp_file(content: &[u8]) -> std::path::PathBuf {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("na_file_hash_{}_{seq}", std::process::id()));
        std::fs::write(&path, content).expect("写临时文件失败");
        path
    }

    #[test]
    fn test_file_digest_set_matches_text_hash() {
        let path = temp_file(b"hello world");
        let ids: Vec<&'static str> = FILE_HASH_ALGOS.iter().map(|a| a.id).collect();
        let results = file_digest_set(&path, &ids).expect("计算失败");
        std::fs::remove_file(&path).ok();

        assert_eq!(results.len(), 6);
        let get = |id: &str| results.iter().find(|r| r.id == id).unwrap().hex.clone();
        assert_eq!(get("md5"), hash_md5("hello world"));
        assert_eq!(get("sha1"), hash_sha1("hello world"));
        assert_eq!(get("sha224"), hash_sha224("hello world"));
        assert_eq!(get("sha256"), hash_sha256("hello world"));
        assert_eq!(get("sha384"), hash_sha384("hello world"));
        assert_eq!(get("sha512"), hash_sha512("hello world"));
    }

    #[test]
    fn test_file_digest_set_chunk_boundary() {
        // 256KB + 7 字节：跨块边界，与文本函数对拍
        let content = vec![b'a'; FILE_HASH_CHUNK + 7];
        let path = temp_file(&content);
        let results = file_digest_set(&path, &["sha256"]).expect("计算失败");
        std::fs::remove_file(&path).ok();

        assert_eq!(results[0].hex, hash_sha256(std::str::from_utf8(&content).unwrap()));
    }

    #[test]
    fn test_file_digest_set_order_and_subset() {
        let path = temp_file(b"abc");
        let results = file_digest_set(&path, &["sha256", "md5"]).expect("计算失败");
        std::fs::remove_file(&path).ok();
        assert_eq!(results[0].id, "sha256");
        assert_eq!(results[1].id, "md5");
        assert_eq!(results[1].hex, hash_md5("abc"));
    }

    #[test]
    fn test_file_digest_set_missing_file_and_bad_id() {
        let missing = std::env::temp_dir().join("na_file_hash_no_such_file_998877");
        assert!(file_digest_set(&missing, &["md5"]).is_err());

        let path = temp_file(b"x");
        assert!(file_digest_set(&path, &["not-an-algo"]).is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_file_hash_algo_lookup() {
        assert_eq!(file_hash_algo("sha256").unwrap().label, "SHA-256");
        assert!(file_hash_algo("nope").is_none());
        assert_eq!(file_hash_algo_by_hex_len(64).unwrap().id, "sha256");
        assert_eq!(file_hash_algo_by_hex_len(32).unwrap().id, "md5");
        assert_eq!(file_hash_algo_by_hex_len(128).unwrap().id, "sha512");
        assert!(file_hash_algo_by_hex_len(33).is_none());
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1023), "1023 B");
        assert_eq!(format_file_size(2048), "2.0 KB");
        assert_eq!(format_file_size(88_472_576), "84.4 MB");
        assert_eq!(format_file_size(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(format_file_size(5 * 1024 * 1024 * 1024 * 1024), "5120 GB");
    }
}
