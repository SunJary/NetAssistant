/// 十六进制转换工具函数
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex.replace(" ", "").replace("\n", "").replace("\r", "").replace("\t", "");
    let mut bytes = Vec::new();
    
    for i in (0..hex.len()).step_by(2) {
        if i + 1 < hex.len() {
            let byte_str = &hex[i..i+2];
            if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
                bytes.push(byte);
            }
        }
    }
    
    bytes
}

/// 验证十六进制输入
pub fn validate_hex_input(input: &str) -> bool {
    let cleaned = input.replace(" ", "").replace("\n", "").replace("\r", "").replace("\t", "");
    if cleaned.is_empty() {
        return true;
    }
    cleaned.chars().all(|c| c.is_ascii_hexdigit())
}