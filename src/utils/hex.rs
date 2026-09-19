use std::iter::Peekable;
use std::str::Chars;

/// 十六进制转换工具函数
///
/// 单次遍历,无中间全串分配: 空白字符跳过不占配对位,两位合法 hex 合成一个字节,
/// 非法字符占一个配对位并丢弃未配对的半字节(与旧实现逐对解析的语义一致)。
/// `\xNN` / `\n` / `\r` / `\t` / `\\` 转义各自成字节(与 `hex_to_text` 的输出互为逆),
/// 其余自成一个配对位的字符会丢弃未配对的半字节。
/// 按 char 边界处理,输入含多字节 UTF-8 字符(如中文)时安全跳过,不会 panic。
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut pending: Option<u8> = None;
    let mut chars = hex.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(byte) = take_escape(&mut chars) {
                pending = None;
                bytes.push(byte);
                continue;
            }
        }
        if c.is_whitespace() {
            continue;
        }
        match c.to_digit(16) {
            Some(d) => match pending.take() {
                Some(hi) => bytes.push((hi << 4) | d as u8),
                None => pending = Some(d as u8),
            },
            None => pending = None,
        }
    }

    bytes
}

/// 取 `\` 之后的转义字节,未识别时不消费任何字符并返回 None
/// (调用方已消费反斜杠,此时按普通非法字符处理)。
fn take_escape(chars: &mut Peekable<Chars<'_>>) -> Option<u8> {
    let mut probe = chars.clone();
    match probe.next()? {
        'n' => {
            chars.next();
            Some(b'\n')
        }
        'r' => {
            chars.next();
            Some(b'\r')
        }
        't' => {
            chars.next();
            Some(b'\t')
        }
        '\\' => {
            chars.next();
            Some(b'\\')
        }
        'x' | 'X' => {
            let hi = probe.next()?.to_digit(16)?;
            let lo = probe.next()?.to_digit(16)?;
            *chars = probe;
            Some(((hi << 4) | lo) as u8)
        }
        _ => None,
    }
}

/// 移除空白字符(单次遍历,替代多次 replace 全串分配)
fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 验证十六进制输入
///
/// 支持变量占位符 `${...}`: 含变量时仅验证非变量部分的字符合法性，
/// 跳过长度检查(变量输出长度在运行时才能确定)。
pub fn validate_hex_input(input: &str) -> bool {
    let cleaned = strip_whitespace(input);
    if cleaned.is_empty() {
        return true;
    }
    let has_vars = cleaned.contains("${");
    // 跳过变量占位符，仅验证 hex 部分
    let hex_only = strip_variables(&cleaned);
    if hex_only.is_empty() {
        return true;
    }
    // 含变量时仅检查字符合法性(长度在运行时才能确定)
    if has_vars {
        return hex_only.chars().all(|c| c.is_ascii_hexdigit());
    }
    // 无变量时检查长度 + 字符
    if hex_only.len() % 2 != 0 {
        return false;
    }
    hex_only.chars().all(|c| c.is_ascii_hexdigit())
}

/// 移除 ${...} 变量占位符，返回剩余内容
fn strip_variables(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// 转换片段：字节序列（hex 部分）或 `${...}` 变量占位符（原样保留）
enum Seg {
    Bytes(Vec<u8>),
    Token(String),
}

/// 文本 → hex 文本：转义感知的可逆编码。
///
/// - `\xNN`（大小写均可）→ 该字节；`\n` `\r` `\t` `\\` → 对应字节
/// - 其余字符 → 其 UTF-8 字节（中文等多字节字符逐字节编码）
/// - `${...}` 整体原样保留（与 `validate_hex_input` / `core::parse` 的 token 语义一致）
/// - 不完整/非法转义（如 `\xG1`、串尾 `\`）按字面反斜杠编码
///
/// 输出两位一组、空格分隔，字母统一大写。
pub fn text_to_hex(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for seg in text_segments(text) {
        match seg {
            Seg::Token(token) => push_item(&mut out, &token),
            Seg::Bytes(bytes) => {
                for byte in bytes {
                    push_hex_byte(&mut out, byte);
                }
            }
        }
    }
    out
}

/// hex 文本 → 文本：`text_to_hex` 的逆运算。
///
/// - 可打印 ASCII（0x20..=0x7E，含空格）直出；`\` 输出为 `\\` 保证可逆
/// - `\n` `\r` `\t` 具名转义；其余字节（含不可打印、非法 UTF-8）输出 `\xNN`（大写）
/// - 合法 UTF-8 多字节字符（如中文）直接输出字符
/// - `${...}` 变量占位符原样保留
///
/// 解析语义与 `hex_to_bytes` 一致：空白跳过，非法字符丢弃，落单半字节丢弃。
pub fn hex_to_text(hex: &str) -> String {
    let mut out = String::with_capacity(hex.len());
    for seg in hex_segments(hex) {
        match seg {
            Seg::Token(token) => out.push_str(&token),
            Seg::Bytes(bytes) => push_bytes_as_text(&mut out, &bytes),
        }
    }
    out
}

/// 模式切换时的内容转换；失败（内容与方向不匹配）返回 None，调用方不动内容。
///
/// - `text → hex`：任何文本都可编码，恒成功
/// - `hex → text`：内容须为合法 hex（含 `${...}` 时跳过变量校验），否则返回 None
pub fn convert_value(value: &str, from_mode: &str, to_mode: &str) -> Option<String> {
    if from_mode == to_mode {
        return None;
    }
    match (from_mode, to_mode) {
        ("text", "hex") => Some(text_to_hex(value)),
        ("hex", "text") => {
            if validate_hex_input(value) {
                Some(hex_to_text(value))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 文本按转义切分为字节段与变量段
fn text_segments(text: &str) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut chars = text.chars().peekable();
    let mut utf8 = [0u8; 4];

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut token = String::from("${");
            for inner in chars.by_ref() {
                token.push(inner);
                if inner == '}' {
                    break;
                }
            }
            if !buf.is_empty() {
                segs.push(Seg::Bytes(std::mem::take(&mut buf)));
            }
            segs.push(Seg::Token(token));
            continue;
        }
        if c == '\\' {
            if let Some(byte) = take_escape(&mut chars) {
                buf.push(byte);
                continue;
            }
        }
        buf.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
    }
    if !buf.is_empty() {
        segs.push(Seg::Bytes(buf));
    }
    segs
}

/// hex 文本解析为字节段与变量段（语义与 `hex_to_bytes` 一致）
fn hex_segments(hex: &str) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut pending: Option<u8> = None;
    let mut chars = hex.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut token = String::from("${");
            for inner in chars.by_ref() {
                token.push(inner);
                if inner == '}' {
                    break;
                }
            }
            if !buf.is_empty() {
                segs.push(Seg::Bytes(std::mem::take(&mut buf)));
            }
            segs.push(Seg::Token(token));
            continue;
        }
        if c == '\\' {
            if let Some(byte) = take_escape(&mut chars) {
                pending = None;
                buf.push(byte);
                continue;
            }
        }
        if c.is_whitespace() {
            continue;
        }
        match c.to_digit(16) {
            Some(d) => match pending.take() {
                Some(hi) => buf.push((hi << 4) | d as u8),
                None => pending = Some(d as u8),
            },
            None => pending = None,
        }
    }
    if !buf.is_empty() {
        segs.push(Seg::Bytes(buf));
    }
    segs
}

/// 追加一个以空格分隔的条目（首个条目不加分隔符）
fn push_item(out: &mut String, item: &str) {
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(item);
}

/// 追加一个两位大写十六进制字节（以空格分隔）
fn push_hex_byte(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    if !out.is_empty() {
        out.push(' ');
    }
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0F) as usize] as char);
}

/// 字节序列 → 可读且可逆的文本表示（`text_to_hex` 可原样还原字节）
fn push_bytes_as_text(out: &mut String, bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\\' => {
                out.push_str("\\\\");
                i += 1;
                continue;
            }
            b'\n' => {
                out.push_str("\\n");
                i += 1;
                continue;
            }
            b'\r' => {
                out.push_str("\\r");
                i += 1;
                continue;
            }
            b'\t' => {
                out.push_str("\\t");
                i += 1;
                continue;
            }
            0x20..=0x7E => {
                out.push(b as char);
                i += 1;
                continue;
            }
            _ => {}
        }
        // 合法 UTF-8 多字节字符（中文等）直接展示；非法字节与不可打印字节用 \xNN
        if let Some(ch) = decode_utf8_char(&bytes[i..]) {
            if !ch.is_control() {
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }
        }
        push_escape_byte(out, b);
        i += 1;
    }
}

/// 尝试从字节串起始位置解码一个合法 UTF-8 字符
fn decode_utf8_char(bytes: &[u8]) -> Option<char> {
    let len = match bytes.first()? {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    if bytes.len() < len {
        return None;
    }
    std::str::from_utf8(&bytes[..len]).ok()?.chars().next()
}

fn push_escape_byte(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push_str("\\x");
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0F) as usize] as char);
}

#[cfg(test)]
mod tests {
    use super::{convert_value, hex_to_bytes, hex_to_text, text_to_hex, validate_hex_input};

    #[test]
    /// 测试十六进制字符串到字节的转换功能
    /// 包括空字符串、有效十六进制字符串和大小写不敏感的测试
    fn test_hex_to_bytes() {
        // 测试空字符串
        assert_eq!(hex_to_bytes(""), Vec::<u8>::new());

        // 测试有效的十六进制字符串
        assert_eq!(hex_to_bytes("48656c6c6f"), b"Hello");
        assert_eq!(hex_to_bytes("48656c6c6f20576f726c64"), b"Hello World");
        assert_eq!(hex_to_bytes("00010203"), &[0x00, 0x01, 0x02, 0x03]);

        // 测试大小写不敏感
        assert_eq!(hex_to_bytes("48656C6C6F"), b"Hello");
        assert_eq!(hex_to_bytes("48656c6c6f"), b"Hello");
    }

    #[test]
    /// 测试空白混排与非法字符的配对语义
    fn test_hex_to_bytes_whitespace_and_invalid() {
        // 空白跳过不占配对位
        assert_eq!(hex_to_bytes("48 65 6C 6C 6F"), b"Hello");
        assert_eq!(hex_to_bytes("48\n65\r6C\t6F"), &[0x48, 0x65, 0x6C, 0x6F]);
        // 非法字符占一个配对位,与旧实现一致: "4g86" → "4g" 丢弃 + "86"
        assert_eq!(hex_to_bytes("4g86"), &[0x86]);
        // 奇数长度: 尾部未配对数字丢弃
        assert_eq!(hex_to_bytes("486"), &[0x48]);
        // 空白/纯非法输入
        assert_eq!(hex_to_bytes("   \n\t\r"), Vec::<u8>::new());
        assert_eq!(hex_to_bytes("zzzz"), Vec::<u8>::new());
    }

    #[test]
    /// 多字节 UTF-8 字符(如中文)混入时应安全跳过而非 panic
    fn test_hex_to_bytes_with_multibyte_chars() {
        // 中文逐个占配对位(与旧实现的非法字符语义一致)
        assert_eq!(hex_to_bytes("你好"), Vec::<u8>::new());
        assert_eq!(hex_to_bytes("4你8"), Vec::<u8>::new());
        // 空白 + 中文 + hex 混排
        assert_eq!(hex_to_bytes("48 中 65 文 6C6C6F"), b"Hello");
    }

    #[test]
    /// 测试十六进制输入的验证功能
    /// 包括空字符串、有效十六进制字符串和无效十六进制字符串的测试
    fn test_validate_hex_input() {
        // 测试空字符串
        assert!(validate_hex_input(""));

        // 测试有效的十六进制字符串
        assert!(validate_hex_input("48656c6c6f"));
        assert!(validate_hex_input("48656C6C6F"));
        assert!(validate_hex_input("00010203"));

        // 测试无效的十六进制字符串
        assert!(!validate_hex_input("invalid"));
        assert!(!validate_hex_input("48656c6c6")); // 奇数长度
        assert!(!validate_hex_input("48656c6c6g")); // 包含非十六进制字符
    }

    #[test]
    /// 测试含变量占位符的十六进制验证
    fn test_validate_hex_input_with_variables() {
        // 变量占位符应被跳过，仅验证 hex 部分
        assert!(validate_hex_input("50494E47${seq}"));
        assert!(validate_hex_input("${seq}"));
        assert!(validate_hex_input("50494E47${worker_id}${seq}"));
        // 含变量时，非变量部分奇数长度也可通过(变量输出长度运行时确定)
        assert!(validate_hex_input("50494E4${seq}"));
        // 非变量部分含非法字符仍应失败
        assert!(!validate_hex_input("50494E4G${seq}"));
        // 未闭合的变量占位符: $ 后面没有完整 ${...}，$ 不是 hex 字符
        assert!(!validate_hex_input("50494E47$"));
    }

    #[test]
    /// hex 转义序列(\xNN / \n / \r / \t / \\)各自成字节,与旧行为兼容
    fn test_hex_to_bytes_with_escapes() {
        assert_eq!(hex_to_bytes("\\x41\\x42"), b"AB");
        assert_eq!(hex_to_bytes("\\x41"), b"A");
        assert_eq!(hex_to_bytes("41 42 \\x6C\\x6C\\x6F"), b"ABllo");
        assert_eq!(hex_to_bytes("61\\x0A\\x62"), &[0x61, 0x0A, 0x62]);
        assert_eq!(hex_to_bytes("\\n\\r\\t\\\\"), &[0x0A, 0x0D, 0x09, 0x5C]);
        // 真实换行仍是空白,不产生字节
        assert_eq!(hex_to_bytes("61\n62"), &[0x61, 0x62]);
        // 不完整转义: 反斜杠按非法字符处理,后续 hex 正常解析(与旧行为一致)
        assert_eq!(hex_to_bytes("\\xG1"), Vec::<u8>::new());
        assert_eq!(hex_to_bytes("41\\xG1"), &[0x41]);
    }

    #[test]
    /// 文本 → hex: UTF-8 逐字节编码、大写输出、变量占位符原样保留
    fn test_text_to_hex() {
        assert_eq!(text_to_hex(""), "");
        assert_eq!(text_to_hex("ok"), "6F 6B");
        assert_eq!(text_to_hex("Hello"), "48 65 6C 6C 6F");
        // 真实换行/制表/反斜杠
        assert_eq!(text_to_hex("a\nb"), "61 0A 62");
        assert_eq!(text_to_hex("a\tb\\c"), "61 09 62 5C 63");
        // 转义形式与字节形式等价(可逆)
        assert_eq!(text_to_hex("\\x00\\xFF"), "00 FF");
        assert_eq!(text_to_hex("\\n\\r\\t\\\\"), "0A 0D 09 5C");
        // 中文按 UTF-8 逐字节
        assert_eq!(text_to_hex("你好"), "E4 BD A0 E5 A5 BD");
        // 变量占位符整体保留
        assert_eq!(text_to_hex("${seq}ping"), "${seq} 70 69 6E 67");
        assert_eq!(text_to_hex("${seq}"), "${seq}");
        // 非法转义按字面反斜杠编码
        assert_eq!(text_to_hex("\\xG1"), "5C 78 47 31");
    }

    #[test]
    /// hex → 文本: 可打印直出、具名转义、其余 \xNN(大写)、变量保留
    fn test_hex_to_text() {
        assert_eq!(hex_to_text(""), "");
        assert_eq!(hex_to_text("6F 6B"), "ok");
        assert_eq!(hex_to_text("48656C6C6F"), "Hello");
        assert_eq!(hex_to_text("00 FF"), "\\x00\\xFF");
        assert_eq!(hex_to_text("0A 0D 09"), "\\n\\r\\t");
        assert_eq!(hex_to_text("5C"), "\\\\");
        // 中文 UTF-8 字节直接还原为字符
        assert_eq!(hex_to_text("E4 BD A0 E5 A5 BD"), "你好");
        // 合法 UTF-8 之外的字节用 \xNN
        assert_eq!(hex_to_text("C3 28"), "\\xC3(");
        // 变量占位符保留(穿插在字节之间)
        assert_eq!(hex_to_text("${seq} 70 69"), "${seq}pi");
        assert_eq!(hex_to_text("70 ${seq} 69"), "p${seq}i");
    }

    #[test]
    /// 往返一致性: hex → 文本 → hex 字节级相等(本方案相对 NetAssist/XCOM 的核心卖点)。
    /// 注意比的是字节，不是字符串：`text_to_hex` 输出统一为「两位一组、空格分隔」，
    /// 原串的 `50494E47` 这类紧凑写法会被重新分组。
    fn test_round_trip_hex_text_hex() {
        let cases = [
            "6F 6B",
            "00 FF",
            "00 01 02 03",
            "61 0A 62",
            "5C 78",
            "E4 BD A0 E5 A5 BD",
            "FF FE FD",
            "20 7E 7F",
            "50494E47 ${seq} 0D 0A",
            "50494E47",
        ];
        for hex in cases {
            let text = hex_to_text(hex);
            let back = text_to_hex(&text);
            assert_eq!(
                hex_to_bytes(&back),
                hex_to_bytes(hex),
                "round trip failed for {hex:?} -> {text:?} -> {back:?}"
            );
        }
    }

    #[test]
    /// 往返一致性: 文本 → hex → 文本(可打印内容原样还原)
    fn test_round_trip_text_hex_text() {
        assert_eq!(hex_to_text(&text_to_hex("ok")), "ok");
        assert_eq!(hex_to_text(&text_to_hex("你好")), "你好");
        assert_eq!(hex_to_text(&text_to_hex("${seq}ping")), "${seq}ping");
        // 转义形式进入 → hex → 文本,等价于原始可打印文本
        assert_eq!(hex_to_text(&text_to_hex("a\\nb")), "a\\nb");
    }

    #[test]
    /// 文本 → hex 内容是 hex 模式的发送输入，`hex_to_bytes` 取到的字节
    /// 必须与直接发送该文本一致（文本→hex 是编码，不是仅展示）。
    fn test_text_to_hex_feeds_hex_to_bytes() {
        for (text, bytes) in [
            ("ok", b"ok".to_vec()),
            ("a\nb", vec![0x61, 0x0A, 0x62]),
            ("a\tb\\c", vec![0x61, 0x09, 0x62, 0x5C, 0x63]),
            ("\\x00\\xFF", vec![0x00, 0xFF]),
            ("你好", "你好".as_bytes().to_vec()),
        ] {
            assert_eq!(
                hex_to_bytes(&text_to_hex(text)),
                bytes,
                "text {text:?} -> {:?}",
                text_to_hex(text)
            );
        }
        // `${...}` 是运行时变量占位符，发送前由变量替换处理；
        // 未替换时 hex 解析按非法字符跳过，其余字节仍与文本一致
        assert_eq!(hex_to_bytes(&text_to_hex("${seq}ping")), b"ping");
    }

    #[test]
    /// 模式切换转换: 失败(方向不匹配/非法 hex)返回 None,调用方不动内容
    fn test_convert_value() {
        assert_eq!(convert_value("ok", "text", "hex").as_deref(), Some("6F 6B"));
        assert_eq!(convert_value("6F 6B", "hex", "text").as_deref(), Some("ok"));
        assert_eq!(
            convert_value("00 FF", "hex", "text").as_deref(),
            Some("\\x00\\xFF")
        );
        // 同一模式不转换
        assert_eq!(convert_value("ok", "text", "text"), None);
        // hex 内容非法: 不转换(历史遗留内容不被破坏)
        assert_eq!(convert_value("ok", "hex", "text"), None);
        assert_eq!(convert_value("48656c6c6", "hex", "text"), None);
        // 空内容
        assert_eq!(convert_value("", "text", "hex").as_deref(), Some(""));
        assert_eq!(convert_value("", "hex", "text").as_deref(), Some(""));
    }
}
