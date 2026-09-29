// 统一"选区工具"注册表
//
// 把「转换（hex ↔ 文本）」与「校验（各算法）」描述成一组 SelectionTool，供输入框
// 右键菜单渲染，并作为将来独立小工具入口（任意 source）的扩展接缝：拿到一段字符串
// 即可用 `resolve_tools` 求可用态、用 `compute_tool` 出结果，与是否来自输入框无关。

use super::checksum::ChecksumAlgorithm;
use crate::utils::hex::{hex_to_bytes, hex_to_text, text_to_hex, validate_hex_input};

/// 工具类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    Convert,
    Checksum,
}

/// 一个选区工具（静态描述，无运行时状态）。
///
/// - `modes`：该工具展示适用的输入模式（"text" / "hex"）
/// - `algo`：校验工具携带的算法；转换工具为 None
#[derive(Debug, Clone, Copy)]
pub struct SelectionTool {
    pub id: &'static str,
    pub category: ToolCategory,
    pub label_key: &'static str,
    pub modes: &'static [&'static str],
    pub algo: Option<ChecksumAlgorithm>,
}

impl SelectionTool {
    /// 计算：输入 source，返回展示文本（转换结果或校验值大写 hex）。
    /// 具体操作由工具自身（id / algo）决定，与输入模式无关。
    pub fn compute(&self, source: &str) -> String {
        match self.category {
            ToolCategory::Convert => match self.id {
                "to_hex" => text_to_hex(source),
                _ => hex_to_text(source),
            },
            ToolCategory::Checksum => {
                let algo = self.algo.expect("校验工具必须携带算法");
                format_bytes(&algo.compute(&hex_to_bytes(source)))
            }
        }
    }
}

/// 一次求值后的工具可用态
#[derive(Debug, Clone)]
pub struct ToolState {
    pub tool: &'static SelectionTool,
    pub enabled: bool,
    /// 禁用原因对应的 i18n key（需本地化后展示）
    pub disabled_reason: Option<&'static str>,
}

/// 全量工具注册
pub fn selection_tools() -> &'static [SelectionTool] {
    &[
        SelectionTool {
            id: "to_hex",
            category: ToolCategory::Convert,
            label_key: "input_mode.to_hex",
            modes: &["text"],
            algo: None,
        },
        SelectionTool {
            id: "to_text",
            category: ToolCategory::Convert,
            label_key: "input_mode.to_text",
            modes: &["hex"],
            algo: None,
        },
        SelectionTool {
            id: "checksum_xor",
            category: ToolCategory::Checksum,
            label_key: "input_mode.checksum_xor",
            modes: &["hex"],
            algo: Some(ChecksumAlgorithm::Xor),
        },
        SelectionTool {
            id: "checksum_sum8",
            category: ToolCategory::Checksum,
            label_key: "input_mode.checksum_sum8",
            modes: &["hex"],
            algo: Some(ChecksumAlgorithm::Sum8),
        },
        SelectionTool {
            id: "checksum_lrc",
            category: ToolCategory::Checksum,
            label_key: "input_mode.checksum_lrc",
            modes: &["hex"],
            algo: Some(ChecksumAlgorithm::Lrc),
        },
        SelectionTool {
            id: "checksum_crc16_modbus",
            category: ToolCategory::Checksum,
            label_key: "input_mode.checksum_crc16_modbus",
            modes: &["hex"],
            algo: Some(ChecksumAlgorithm::Crc16Modbus),
        },
        SelectionTool {
            id: "checksum_crc16_ccitt",
            category: ToolCategory::Checksum,
            label_key: "input_mode.checksum_crc16_ccitt",
            modes: &["hex"],
            algo: Some(ChecksumAlgorithm::Crc16Ccitt),
        },
        SelectionTool {
            id: "checksum_crc32",
            category: ToolCategory::Checksum,
            label_key: "input_mode.checksum_crc32",
            modes: &["hex"],
            algo: Some(ChecksumAlgorithm::Crc32),
        },
    ]
}

/// 判断某工具在 (mode, source) 下是否可用；不可用返回禁用原因 i18n key。
fn enabled(tool: &SelectionTool, source: &str) -> Result<(), &'static str> {
    match tool.category {
        ToolCategory::Convert => {
            if tool.id == "to_text" && !validate_hex_input(source) {
                return Err("input_mode.hex_invalid");
            }
            Ok(())
        }
        ToolCategory::Checksum => {
            if source.trim().is_empty() {
                return Err("input_mode.checksum_empty");
            }
            if source.contains("${") {
                return Err("input_mode.checksum_var");
            }
            if !validate_hex_input(source) {
                return Err("input_mode.hex_invalid");
            }
            Ok(())
        }
    }
}

/// 按 mode / source / category 求值生成可用态列表（已按 mode 过滤适用工具）。
pub fn resolve_tools(mode: &str, source: &str, category: ToolCategory) -> Vec<ToolState> {
    selection_tools()
        .iter()
        .filter(|t| t.category == category && t.modes.contains(&mode))
        .map(|tool| {
            let result = enabled(tool, source);
            ToolState {
                tool,
                enabled: result.is_ok(),
                disabled_reason: result.err(),
            }
        })
        .collect()
}

/// 对一个工具执行计算，返回展示文本。
pub fn compute_tool(tool: &SelectionTool, source: &str) -> String {
    tool.compute(source)
}

/// 字节序列 → "XX XX" 大写 hex（与 hex 编辑器展示一致）
fn format_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_compute_from_hex() {
        // Modbus RTU 帧 01 03 00 00 00 02 → CRC16-Modbus 值 = 0x0BC4，
        // 按 D6 大端展示为 "0B C4"（线上字节序为低字节在前 C4 0B）
        let tool = selection_tools()
            .iter()
            .find(|t| t.id == "checksum_crc16_modbus")
            .unwrap();
        assert_eq!(tool.compute("01 03 00 00 00 02"), "0B C4");
    }

    #[test]
    fn test_convert_compute() {
        let to_hex = selection_tools().iter().find(|t| t.id == "to_hex").unwrap();
        assert_eq!(to_hex.compute("ok"), "6F 6B");
        let to_text = selection_tools().iter().find(|t| t.id == "to_text").unwrap();
        assert_eq!(to_text.compute("6F 6B"), "ok");
    }

    #[test]
    fn test_resolve_convert_modes() {
        // text 模式只出 to_hex；hex 模式只出 to_text
        let text_only = resolve_tools("text", "ok", ToolCategory::Convert);
        let to_text_res = resolve_tools("hex", "6F 6B", ToolCategory::Convert);
        assert_eq!(text_only.len(), 1);
        assert_eq!(text_only[0].tool.id, "to_hex");
        assert_eq!(to_text_res.len(), 1);
        assert_eq!(to_text_res[0].tool.id, "to_text");
    }

    #[test]
    fn test_resolve_checksum_modes_and_enable() {
        // hex 模式出 6 个校验工具
        let hex_ok = resolve_tools("hex", "01 03 00 00 00 02", ToolCategory::Checksum);
        assert_eq!(hex_ok.len(), 6);
        assert!(hex_ok.iter().all(|t| t.enabled));

        // text 模式不出校验
        assert!(resolve_tools("text", "abc", ToolCategory::Checksum).is_empty());

        // 非法 hex
        let bad = resolve_tools("hex", "nothex", ToolCategory::Checksum);
        assert_eq!(bad[0].disabled_reason, Some("input_mode.hex_invalid"));

        // 含变量
        let var = resolve_tools("hex", "01 03 ${seq}", ToolCategory::Checksum);
        assert_eq!(var[0].disabled_reason, Some("input_mode.checksum_var"));

        // 空源 / 纯空白源：无内容可校验
        let empty = resolve_tools("hex", "", ToolCategory::Checksum);
        assert_eq!(empty[0].disabled_reason, Some("input_mode.checksum_empty"));
        let blank = resolve_tools("hex", "   \n", ToolCategory::Checksum);
        assert_eq!(blank[0].disabled_reason, Some("input_mode.checksum_empty"));
    }
}