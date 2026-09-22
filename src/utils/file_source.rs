//! 文件数据源 —— 纯逻辑层
//!
//! 承载「从文件导入发送内容」的编码解析、上限校验与 hex 文本格式化，
//! 不依赖 gpui / 项目 UI 类型，可独立无头单测。
//! 设计与决策见 plans/plan-file-data-source.md。

/// 导入上限：1 MiB。
///
/// gpui-component 的 `SYNC_PARSE_MAX_BYTES`（256 KiB）只决定代码编辑器模式下
/// 前台同步高亮的时机：超过它时改写编辑树并转为带去抖的后台解析
/// （`input/mode.rs`），主线程不会被阻塞。因此上限可放宽到 1 MiB，
/// 无需额外的"关闭高亮"降级逻辑。
pub const MAX_IMPORT_BYTES: usize = 1024 * 1024;

/// 文件编码选项（导入对话框内三选一）
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FileEncoding {
    #[default]
    Utf8,
    Gbk,
    /// 系统 ANSI 代码页（Windows 取 GetACP）
    Ansi,
}

impl FileEncoding {
    /// 原始字节 → 文本。
    ///
    /// - UTF-8 严格解码，失败报 `InvalidUtf8`（不静默乱码）
    /// - GBK / ANSI 走 `encoding_rs` 有损解码，`Decoded::lossy` 标记是否出现替换字符
    /// - ANSI 代码页未识别时报 `UnsupportedAnsiCodePage`，不猜测
    /// - 非 Windows 无 ANSI 代码页：按 UTF-8 处理
    pub fn decode(self, bytes: &[u8]) -> Result<Decoded, FileSourceError> {
        match self {
            FileEncoding::Utf8 => match std::str::from_utf8(bytes) {
                Ok(text) => Ok(Decoded {
                    text: text.to_string(),
                    lossy: false,
                }),
                Err(_) => Err(FileSourceError::InvalidUtf8),
            },
            FileEncoding::Gbk => Ok(decode_lossy(encoding_rs::GBK, bytes)),
            FileEncoding::Ansi => match ansi_code_page() {
                Some(cp) => match encoding_for_code_page(cp) {
                    Some(encoding) => Ok(decode_lossy(encoding, bytes)),
                    None => Err(FileSourceError::UnsupportedAnsiCodePage(cp)),
                },
                // 非 Windows：无系统代码页概念，退回严格 UTF-8
                None => match std::str::from_utf8(bytes) {
                    Ok(text) => Ok(Decoded {
                        text: text.to_string(),
                        lossy: false,
                    }),
                    Err(_) => Err(FileSourceError::InvalidUtf8),
                },
            },
        }
    }
}

/// 解码结果
pub struct Decoded {
    pub text: String,
    /// 解码时出现过替换字符（GBK/ANSI 可能为 true，UTF-8 恒 false）
    pub lossy: bool,
}

/// 导入过程中的失败原因（文案由 UI 层按 `t!()` 映射）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSourceError {
    TooLarge { size: u64, limit: usize },
    Empty,
    InvalidUtf8,
    UnsupportedAnsiCodePage(u32),
    ReadFailed(String),
}

/// 校验导入字节数：空文件与超限在此拒绝。
/// 上层在读取前用文件元数据调用一次即可，避免读入超大文件。
pub fn validate_len(size: u64) -> Result<(), FileSourceError> {
    if size == 0 {
        Err(FileSourceError::Empty)
    } else if size > MAX_IMPORT_BYTES as u64 {
        Err(FileSourceError::TooLarge {
            size,
            limit: MAX_IMPORT_BYTES,
        })
    } else {
        Ok(())
    }
}

/// 字节数 → 人类可读大小（1024 进制，单位 B / KB / MB / GB）。
///
/// 小于 1024 用整数 + B；其余小于 10 保留一位小数（如 "1.5 MB"），
/// 否则取整（如 "256 KB"），避免常见量级出现无意义的小数。
pub fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let n = bytes as f64;
    let (value, unit) = if n < KIB {
        return format!("{bytes} B");
    } else if n < KIB * KIB {
        (n / KIB, "KB")
    } else if n < KIB * KIB * KIB {
        (n / (KIB * KIB), "MB")
    } else {
        (n / (KIB * KIB * KIB), "GB")
    };
    if value < 10.0 {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

/// 原始字节 → "AB CD EF" 形式的 hex 文本（空格分隔两位一组、大写），供 hex 模式回填。
///
/// 与 `crate::utils::hex::hex_to_bytes` 互为逆：解析后字节与原文件一致。
pub fn bytes_to_hex_text(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

/// 系统 ANSI 代码页（Windows 取 GetACP）；非 Windows 返回 None。
#[cfg(target_os = "windows")]
pub fn ansi_code_page() -> Option<u32> {
    // SAFETY: GetACP 无参数、无内存写入，恒安全调用。
    let cp = unsafe { winapi::um::winnls::GetACP() } as u32;
    if cp == 0 { None } else { Some(cp) }
}

/// 系统 ANSI 代码页（Windows 取 GetACP）；非 Windows 返回 None。
#[cfg(not(target_os = "windows"))]
pub fn ansi_code_page() -> Option<u32> {
    None
}

/// 代码页 → encoding_rs 编码；未识别返回 None（宁可报错也不猜）
fn encoding_for_code_page(cp: u32) -> Option<&'static encoding_rs::Encoding> {
    use encoding_rs::*;
    Some(match cp {
        874 => WINDOWS_874,
        932 => SHIFT_JIS,
        936 => GBK,
        949 => EUC_KR,
        950 => BIG5,
        1250 => WINDOWS_1250,
        1251 => WINDOWS_1251,
        1252 => WINDOWS_1252,
        1253 => WINDOWS_1253,
        1254 => WINDOWS_1254,
        1255 => WINDOWS_1255,
        1256 => WINDOWS_1256,
        1257 => WINDOWS_1257,
        1258 => WINDOWS_1258,
        54936 => GB18030,
        _ => return None,
    })
}

/// 有损解码：encoding_rs 的替换字符策略与浏览器一致
fn decode_lossy(encoding: &'static encoding_rs::Encoding, bytes: &[u8]) -> Decoded {
    let (text, _, had_errors) = encoding.decode(bytes);
    Decoded {
        text: text.into_owned(),
        lossy: had_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_strict_rejects_invalid() {
        assert_eq!(
            FileEncoding::Utf8.decode(b"hello").map(|d| d.text),
            Ok("hello".to_string())
        );
        // 非法 UTF-8（GBK 编码的中文）必须报错而非静默乱码
        let gbk = b"\xC4\xE3\xBA\xC3"; // "你好" 的 GBK 编码
        assert_eq!(
            FileEncoding::Utf8.decode(gbk).err(),
            Some(FileSourceError::InvalidUtf8)
        );
    }

    #[test]
    fn gbk_decodes_chinese() {
        let gbk = b"\xC4\xE3\xBA\xC3";
        let decoded = FileEncoding::Gbk.decode(gbk).unwrap();
        assert_eq!(decoded.text, "你好");
        assert!(!decoded.lossy);
    }

    #[test]
    fn code_page_mapping() {
        assert_eq!(encoding_for_code_page(936), Some(encoding_rs::GBK));
        assert_eq!(encoding_for_code_page(950), Some(encoding_rs::BIG5));
        assert_eq!(encoding_for_code_page(932), Some(encoding_rs::SHIFT_JIS));
        assert_eq!(
            encoding_for_code_page(1252),
            Some(encoding_rs::WINDOWS_1252)
        );
        // 未识别代码页 → None，不猜测
        assert_eq!(encoding_for_code_page(42), None);
    }

    #[test]
    fn validate_len_rejects_empty_and_oversize() {
        assert_eq!(validate_len(0), Err(FileSourceError::Empty));
        assert_eq!(validate_len(1), Ok(()));
        assert_eq!(validate_len(MAX_IMPORT_BYTES as u64), Ok(()));
        assert_eq!(
            validate_len(MAX_IMPORT_BYTES as u64 + 1),
            Err(FileSourceError::TooLarge {
                size: MAX_IMPORT_BYTES as u64 + 1,
                limit: MAX_IMPORT_BYTES,
            })
        );
    }

    #[test]
    fn format_size_is_human_readable() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(256 * 1024), "256 KB");
        assert_eq!(format_size(MAX_IMPORT_BYTES as u64), "1.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024 / 2), "1.5 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn hex_text_formatting() {
        assert_eq!(bytes_to_hex_text(&[]), "");
        assert_eq!(bytes_to_hex_text(&[0x00]), "00");
        assert_eq!(bytes_to_hex_text(&[0xAB, 0xCD, 0xEF]), "AB CD EF");
        assert_eq!(bytes_to_hex_text(b"Hi"), "48 69");
        // 与 hex_to_bytes 互逆
        let bytes: Vec<u8> = (0..=255u8).collect();
        assert_eq!(
            crate::utils::hex::hex_to_bytes(&bytes_to_hex_text(&bytes)),
            bytes
        );
    }
}