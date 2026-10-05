// 校验算法纯函数（不依赖 gpui）
//
// 覆盖二进制帧/私有协议调试的常用校验。三个 CRC 实现均为
// [crc_generic](crate::core::crc::crc_generic) 的参数化调用 —— 既有的固定参数变体
// （CRC16/MODBUS、CRC16/CCITT-FALSE、CRC32/ISO-HDLC）只是参数表中的一个点，
// 因此任意 CRC 变体（F-44：自定义 poly/init/xorout/refin/refout）无需新代码。

use crate::core::crc::{CrcParams, crc_generic_bytes};
use serde::{Deserialize, Serialize};

/// 校验算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumAlgorithm {
    /// 异或：所有字节异或，1 字节
    Xor,
    /// 累加和：字节求和取低 8 位，1 字节
    Sum8,
    /// Modbus LRC：Sum8 取反加 1，1 字节
    Lrc,
    /// CRC16/MODBUS，poly 0x8005(反射 0xA001)，init 0xFFFF，输入/输出均反射，2 字节
    Crc16Modbus,
    /// CRC16/CCITT-FALSE，poly 0x1021，init 0xFFFF，无反射，2 字节
    Crc16Ccitt,
    /// CRC32/ISO-HDLC，poly 0x04C11DB7(反射 0xEDB88320)，init 0xFFFFFFFF，反射，xorout 0xFFFFFFFF，4 字节
    Crc32,
}

impl ChecksumAlgorithm {
    /// 全部算法（UI 下拉与变量选择器的唯一来源，顺序即展示顺序）
    pub const ALL: [ChecksumAlgorithm; 6] = [
        ChecksumAlgorithm::Xor,
        ChecksumAlgorithm::Sum8,
        ChecksumAlgorithm::Lrc,
        ChecksumAlgorithm::Crc16Modbus,
        ChecksumAlgorithm::Crc16Ccitt,
        ChecksumAlgorithm::Crc32,
    ];

    /// 稳定字符串标识（小写，与 `serde` 的 snake_case 一致）。
    ///
    /// 同时用于：持久化 JSON、校验工具注册表、日志诊断。
    pub fn name(self) -> &'static str {
        match self {
            ChecksumAlgorithm::Xor => "xor",
            ChecksumAlgorithm::Sum8 => "sum8",
            ChecksumAlgorithm::Lrc => "lrc",
            ChecksumAlgorithm::Crc16Modbus => "crc16_modbus",
            ChecksumAlgorithm::Crc16Ccitt => "crc16_ccitt",
            ChecksumAlgorithm::Crc32 => "crc32",
        }
    }

    /// 变量语法中使用的紧凑名（去掉下划线，与既有 toolbox 工具 id 习惯一致）
    pub fn compact_name(self) -> &'static str {
        match self {
            ChecksumAlgorithm::Crc16Modbus => "crc16modbus",
            ChecksumAlgorithm::Crc16Ccitt => "crc16ccitt",
            ChecksumAlgorithm::Xor => "xor",
            ChecksumAlgorithm::Sum8 => "sum8",
            ChecksumAlgorithm::Lrc => "lrc",
            ChecksumAlgorithm::Crc32 => "crc32",
        }
    }

    /// 从字符串标识解析；同时接受 snake_case 与紧凑两种写法（大小写不敏感）。
    ///
    /// 容错是必要的：用户在 UI 上看到 `crc16_modbus`，在变量语法里写
    /// `${rx.crc16modbus:...}`，两者都必须能用。
    pub fn parse(name: &str) -> Option<Self> {
        let normalized = name.trim().to_ascii_lowercase();
        let squeezed = normalized.replace(['_', '-'], "");
        Self::ALL.into_iter().find(|a| {
            a.name() == normalized || a.compact_name() == squeezed || a.compact_name() == normalized
        })
    }

    /// 计算校验值，返回 1（XOR/Sum8/LRC）、2（CRC16）或 4（CRC32）字节，按大端放置。
    pub fn compute(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            ChecksumAlgorithm::Xor => vec![bytes.iter().fold(0u8, |acc, &b| acc ^ b)],
            ChecksumAlgorithm::Sum8 => vec![bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))],
            ChecksumAlgorithm::Lrc => {
                let sum = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
                vec![(!sum).wrapping_add(1)]
            }
            ChecksumAlgorithm::Crc16Modbus => crc_generic_bytes(bytes, CrcParams::CRC16_MODBUS),
            ChecksumAlgorithm::Crc16Ccitt => crc_generic_bytes(bytes, CrcParams::CRC16_CCITT_FALSE),
            ChecksumAlgorithm::Crc32 => crc_generic_bytes(bytes, CrcParams::CRC32_ISO_HDLC),
        }
    }

    /// 计算校验值并按指定字节序输出。
    ///
    /// `little == true` 时反转多字节结果 —— Modbus RTU 线上顺序是小端
    /// （`01 03 00 00 00 02` 的 CRC 线上为 `C4 0B`，而 `compute` 给的是 `0B C4`）。
    /// 这个细节不显式建模必错，且症状隐晦（对端只报校验错）。
    pub fn compute_with_endian(self, bytes: &[u8], little: bool) -> Vec<u8> {
        let mut out = self.compute(bytes);
        if little && out.len() > 1 {
            out.reverse();
        }
        out
    }
}

impl std::fmt::Display for ChecksumAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for ChecksumAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("未知校验算法: {}", s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 统一检查字节串 "123456789"（标准 CRC 检查向量），锁定算法参数。
    ///
    /// 硬编码实现改写为 `crc_generic` 调用后，本测试是「行为零变化」的硬标准。
    #[test]
    fn test_crc_standard_check_values() {
        let data = b"123456789";
        assert_eq!(
            ChecksumAlgorithm::Crc16Modbus.compute(data),
            vec![0x4B, 0x37],
            "CRC16/MODBUS 应等于 0x4B37"
        );
        assert_eq!(
            ChecksumAlgorithm::Crc16Ccitt.compute(data),
            vec![0x29, 0xB1],
            "CRC16/CCITT-FALSE 应等于 0x29B1"
        );
        assert_eq!(
            ChecksumAlgorithm::Crc32.compute(data),
            vec![0xCB, 0xF4, 0x39, 0x26],
            "CRC32 应等于 0xCBF43926"
        );
    }

    /// 参数化实现与既有直接实现交叉验证：CRC16/MODBUS 对 Modbus 读寄存器帧
    /// 的期望值与 toolbox 既有断言一致（0x0BC4）
    #[test]
    fn test_crc_generic_matches_known_frame() {
        let frame = [0x01u8, 0x03, 0x00, 0x00, 0x00, 0x02];
        assert_eq!(
            ChecksumAlgorithm::Crc16Modbus.compute(&frame),
            vec![0x0B, 0xC4]
        );
        assert_eq!(
            ChecksumAlgorithm::Crc16Modbus.compute_with_endian(&frame, true),
            vec![0xC4, 0x0B],
            "Modbus RTU 线上为小端"
        );
        // 单字节算法不受字节序开关影响
        assert_eq!(
            ChecksumAlgorithm::Xor.compute_with_endian(&[0x01, 0x03], true),
            vec![0x02]
        );
    }

    #[test]
    fn test_xor_sum8_lrc() {
        // 0x01 ^ 0x03 ^ 0x00 = 0x02
        assert_eq!(
            ChecksumAlgorithm::Xor.compute(&[0x01, 0x03, 0x00]),
            vec![0x02]
        );
        // 求和
        assert_eq!(
            ChecksumAlgorithm::Sum8.compute(&[0x01, 0x03, 0x00]),
            vec![0x04]
        );
        // 进位置低 8 位
        assert_eq!(ChecksumAlgorithm::Sum8.compute(&[0xFF, 0x01]), vec![0x00]);
        // LRC = 两补；0xFF 两补为 0x01
        assert_eq!(ChecksumAlgorithm::Lrc.compute(&[0xFF]), vec![0x01]);
        assert_eq!(ChecksumAlgorithm::Lrc.compute(&[0x01, 0x03]), vec![0xFC]);
        // 空输入：XOR=0, Sum8=0, LRC=0
        assert_eq!(ChecksumAlgorithm::Xor.compute(&[]), vec![0x00]);
        assert_eq!(ChecksumAlgorithm::Sum8.compute(&[]), vec![0x00]);
        assert_eq!(ChecksumAlgorithm::Lrc.compute(&[]), vec![0x00]);
    }

    #[test]
    fn test_output_width() {
        assert_eq!(ChecksumAlgorithm::Xor.compute(&[1]).len(), 1);
        assert_eq!(ChecksumAlgorithm::Sum8.compute(&[1]).len(), 1);
        assert_eq!(ChecksumAlgorithm::Lrc.compute(&[1]).len(), 1);
        assert_eq!(ChecksumAlgorithm::Crc16Modbus.compute(&[1]).len(), 2);
        assert_eq!(ChecksumAlgorithm::Crc16Ccitt.compute(&[1]).len(), 2);
        assert_eq!(ChecksumAlgorithm::Crc32.compute(&[1]).len(), 4);
    }

    #[test]
    fn test_empty_single_byte_flows() {
        // CRC16(Crc16Modbus, 空) = 0xFFFF；CRC16(CCITT-FALSE, 空) = 0xFFFF
        assert_eq!(
            ChecksumAlgorithm::Crc16Modbus.compute(&[]),
            vec![0xFF, 0xFF]
        );
        assert_eq!(ChecksumAlgorithm::Crc16Ccitt.compute(&[]), vec![0xFF, 0xFF]);
        // CRC32 空 = 0x00000000
        assert_eq!(
            ChecksumAlgorithm::Crc32.compute(&[]),
            vec![0x00, 0x00, 0x00, 0x00]
        );
    }

    /// 名称解析必须同时接受 snake_case（UI/持久化）与紧凑写法（变量语法）
    #[test]
    fn test_name_parse_both_spellings() {
        assert_eq!(
            ChecksumAlgorithm::parse("crc16_modbus"),
            Some(ChecksumAlgorithm::Crc16Modbus)
        );
        assert_eq!(
            ChecksumAlgorithm::parse("crc16modbus"),
            Some(ChecksumAlgorithm::Crc16Modbus)
        );
        assert_eq!(
            ChecksumAlgorithm::parse("CRC16-CCITT"),
            Some(ChecksumAlgorithm::Crc16Ccitt)
        );
        assert_eq!(
            ChecksumAlgorithm::parse("crc32"),
            Some(ChecksumAlgorithm::Crc32)
        );
        assert_eq!(ChecksumAlgorithm::parse(" sha256 "), None);
        // name() / from_str 往返一致
        for algo in ChecksumAlgorithm::ALL {
            assert_eq!(ChecksumAlgorithm::parse(algo.name()), Some(algo));
            assert_eq!(algo.name().parse::<ChecksumAlgorithm>().ok(), Some(algo));
        }
    }

    /// 持久化用的字符串形态稳定（回复规则 JSON 里 algorithm 是裸字符串）
    #[test]
    fn test_serde_is_string() {
        let json = serde_json::to_string(&ChecksumAlgorithm::Crc16Modbus).unwrap();
        assert_eq!(json, "\"crc16_modbus\"");
        let back: ChecksumAlgorithm = serde_json::from_str("\"crc16_modbus\"").unwrap();
        assert_eq!(back, ChecksumAlgorithm::Crc16Modbus);
    }
}
