/// 十六进制转换工具函数
///
/// 单次遍历,无中间全串分配: 空白字符跳过不占配对位,两位合法 hex 合成一个字节,
/// 非法字符占一个配对位并丢弃未配对的半字节(与旧实现逐对解析的语义一致)。
/// 按 char 边界处理,输入含多字节 UTF-8 字符(如中文)时安全跳过,不会 panic。
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut pending: Option<u8> = None;

    for c in hex.chars() {
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

#[cfg(test)]
mod tests {
    use super::{hex_to_bytes, validate_hex_input};

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
}
