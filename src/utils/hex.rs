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
pub fn validate_hex_input(input: &str) -> bool {
    let cleaned = input
        .replace(" ", "")
        .replace("\n", "")
        .replace("\r", "")
        .replace("\t", "");
    if cleaned.is_empty() {
        return true;
    }
    if cleaned.len() % 2 != 0 {
        return false;
    }
    cleaned.chars().all(|c| c.is_ascii_hexdigit())
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
}
