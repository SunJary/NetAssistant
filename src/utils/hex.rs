/// 十六进制转换工具函数
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex
        .replace(" ", "")
        .replace("\n", "")
        .replace("\r", "")
        .replace("\t", "");
    let mut bytes = Vec::new();

    for i in (0..hex.len()).step_by(2) {
        if i + 1 < hex.len() {
            let byte_str = &hex[i..i + 2];
            if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
                bytes.push(byte);
            }
        }
    }

    bytes
}

/// 验证十六进制输入
///
/// 支持变量占位符 `${...}`: 含变量时仅验证非变量部分的字符合法性，
/// 跳过长度检查(变量输出长度在运行时才能确定)。
pub fn validate_hex_input(input: &str) -> bool {
    let cleaned = input
        .replace(" ", "")
        .replace("\n", "")
        .replace("\r", "")
        .replace("\t", "");
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
