// 参数化 CRC 通用实现（不依赖 gpui）
//
// 背景：F-44「可配置校验算法参数」要求支持任意 CRC 变体
// （poly / init / xorout / refin / refout / 输出宽度）。
// 本模块用「一张通用实现 + 参数」替换原先分散在 checksum.rs 的三个硬编码
// 实现（crc16_modbus / crc16_ccitt_false / crc32_ieee）—— 代码更少、能力更强。
//
// 反射语义遵循 CRC 领域惯例（与 RevEng 目录一致）：
// `refin` / `refout` 成对出现时按"反射算法"处理，此时 `poly` 使用**正常形式**
// （如 CRC16/MODBUS 的 0x8005），实现内部自动反射为 0xA001。
// 这与多数教程里直接写 0xA001 的写法等价，但能统一表达 CCITT/XMODEM 等非反射变体。

/// 参数化 CRC 配置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrcParams {
    /// 生成多项式（正常形式，不含最高位）
    pub poly: u64,
    /// 寄存器初值
    pub init: u64,
    /// 结果异或值
    pub xorout: u64,
    /// 输入是否反射（每字节镜像）
    pub refin: bool,
    /// 输出是否反射
    pub refout: bool,
    /// 宽度：8 / 16 / 32 / 64
    pub width: u8,
}

impl CrcParams {
    /// 宽度掩码（如 width=16 → 0xFFFF）
    fn mask(&self) -> u64 {
        if self.width >= 64 {
            u64::MAX
        } else {
            (1u64 << self.width) - 1
        }
    }

    /// CRC16/MODBUS：poly 0x8005，init 0xFFFF，输入/输出均反射
    pub const CRC16_MODBUS: Self = Self {
        poly: 0x8005,
        init: 0xFFFF,
        xorout: 0x0000,
        refin: true,
        refout: true,
        width: 16,
    };

    /// CRC16/CCITT-FALSE：poly 0x1021，init 0xFFFF，无反射
    pub const CRC16_CCITT_FALSE: Self = Self {
        poly: 0x1021,
        init: 0xFFFF,
        xorout: 0x0000,
        refin: false,
        refout: false,
        width: 16,
    };

    /// CRC32/ISO-HDLC（zlib/PKZIP）：poly 0x04C11DB7，init 0xFFFFFFFF，
    /// 反射，xorout 0xFFFFFFFF
    pub const CRC32_ISO_HDLC: Self = Self {
        poly: 0x04C11DB7,
        init: 0xFFFF_FFFF,
        xorout: 0xFFFF_FFFF,
        refin: true,
        refout: true,
        width: 32,
    };
}

/// 按 8 位镜像一个值（只保留 width 位）
fn reflect(mut value: u64, width: u8) -> u64 {
    let mut out = 0u64;
    for _ in 0..width {
        out = (out << 1) | (value & 1);
        value >>= 1;
    }
    out
}

/// 参数化 CRC 计算，返回未截断之外的完整结果（已按 width 掩码与 xorout 处理）。
///
/// 逐位实现（非查表）：协议调试的帧都很短，且查表需要按参数建表（256×宽度），
/// 逐位实现代码更少、无状态、天然可单测。若将来用于大流量校验再引入查表缓存。
pub fn crc_generic(data: &[u8], params: CrcParams) -> u64 {
    let mask = params.mask();
    let width = params.width;

    // 反射算法统一走"右移 + 反射多项式"的镜像实现，避免每个字节做两轮镜像
    if params.refin {
        let poly_ref = reflect(params.poly, width);
        let mut crc = if params.refout {
            params.init & mask
        } else {
            reflect(params.init & mask, width)
        };
        for &b in data {
            crc ^= b as u64;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ poly_ref;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc &= mask;
        let out = if params.refout {
            crc
        } else {
            reflect(crc, width)
        };
        (out ^ params.xorout) & mask
    } else {
        let mut crc = params.init & mask;
        for &b in data {
            crc ^= (b as u64) << (width as u32 - 8);
            for _ in 0..8 {
                if crc & (1u64 << (width - 1)) != 0 {
                    crc = ((crc << 1) ^ params.poly) & mask;
                } else {
                    crc = (crc << 1) & mask;
                }
            }
        }
        (crc ^ params.xorout) & mask
    }
}

/// 计算 CRC 并按指定宽度输出**大端**字节序列。
///
/// 宽度 8/16/32/64 分别输出 1/2/4/8 字节。调用方若需要小端
/// （如 Modbus RTU 线上顺序）自行反转 —— 见
/// `ChecksumAlgorithm::compute_with_endian`。
pub fn crc_generic_bytes(data: &[u8], params: CrcParams) -> Vec<u8> {
    let value = crc_generic(data, params);
    let width_bytes = (params.width as usize + 7) / 8;
    let mut out = Vec::with_capacity(width_bytes);
    for i in (0..width_bytes).rev() {
        out.push(((value >> (i * 8)) & 0xFF) as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标准检查向量 "123456789"，锁定参数化实现与三个既有硬编码实现一致
    #[test]
    fn test_standard_check_values() {
        let data = b"123456789";
        assert_eq!(
            crc_generic(data, CrcParams::CRC16_MODBUS),
            0x4B37,
            "CRC16/MODBUS 检查值应为 0x4B37"
        );
        assert_eq!(
            crc_generic(data, CrcParams::CRC16_CCITT_FALSE),
            0x29B1,
            "CRC16/CCITT-FALSE 检查值应为 0x29B1"
        );
        assert_eq!(
            crc_generic(data, CrcParams::CRC32_ISO_HDLC),
            0xCBF4_3926,
            "CRC32/ISO-HDLC 检查值应为 0xCBF43926"
        );
    }

    /// 更多标准变体，证明"参数化"确实覆盖硬编码做不到的场景（F-44）
    #[test]
    fn test_other_standard_variants() {
        let data = b"123456789";
        // CRC16/ARC：poly 0x8005，init 0x0000，反射，无 xorout
        let arc = CrcParams {
            poly: 0x8005,
            init: 0x0000,
            xorout: 0x0000,
            refin: true,
            refout: true,
            width: 16,
        };
        assert_eq!(
            crc_generic(data, arc),
            0xBB3D,
            "CRC16/ARC 检查值应为 0xBB3D"
        );

        // CRC16/XMODEM：poly 0x1021，init 0x0000，无反射
        let xmodem = CrcParams {
            poly: 0x1021,
            init: 0x0000,
            xorout: 0x0000,
            refin: false,
            refout: false,
            width: 16,
        };
        assert_eq!(
            crc_generic(data, xmodem),
            0x31C3,
            "CRC16/XMODEM 检查值应为 0x31C3"
        );

        // CRC32C（Castagnoli）：poly 0x1EDC6F41，反射，xorout 0xFFFFFFFF
        let crc32c = CrcParams {
            poly: 0x1EDC6F41,
            init: 0xFFFF_FFFF,
            xorout: 0xFFFF_FFFF,
            refin: true,
            refout: true,
            width: 32,
        };
        assert_eq!(
            crc_generic(data, crc32c),
            0xE306_9283,
            "CRC-32C 检查值应为 0xE3069283"
        );
    }

    /// 空输入：CRC16/MODBUS 与 CCITT-FALSE 均为 init（0xFFFF），CRC32 为 0
    #[test]
    fn test_empty_input_is_init() {
        assert_eq!(crc_generic(&[], CrcParams::CRC16_MODBUS), 0xFFFF);
        assert_eq!(crc_generic(&[], CrcParams::CRC16_CCITT_FALSE), 0xFFFF);
        assert_eq!(crc_generic(&[], CrcParams::CRC32_ISO_HDLC), 0x0000_0000);
    }

    /// 字节序列输出为固定宽度的大端
    #[test]
    fn test_bytes_output_width_and_endian() {
        let data = b"123456789";
        assert_eq!(
            crc_generic_bytes(data, CrcParams::CRC16_MODBUS),
            vec![0x4B, 0x37]
        );
        assert_eq!(
            crc_generic_bytes(data, CrcParams::CRC32_ISO_HDLC),
            vec![0xCB, 0xF4, 0x39, 0x26]
        );
        // 单字节宽度（自造变体）输出 1 字节
        let crc8 = CrcParams {
            poly: 0x07,
            init: 0x00,
            xorout: 0x00,
            refin: false,
            refout: false,
            width: 8,
        };
        assert_eq!(crc_generic_bytes(b"A", crc8).len(), 1);
    }

    /// Modbus RTU 实际帧：01 03 00 00 00 02 的 CRC 为 0x0BC4，
    /// 线上小端顺序为 C4 0B（与规划 M2 验收项一致）
    #[test]
    fn test_modbus_rtu_frame_crc() {
        let frame = [0x01u8, 0x03, 0x00, 0x00, 0x00, 0x02];
        assert_eq!(crc_generic(&frame, CrcParams::CRC16_MODBUS), 0x0BC4);
        let mut be = crc_generic_bytes(&frame, CrcParams::CRC16_MODBUS);
        assert_eq!(be, vec![0x0B, 0xC4]);
        be.reverse();
        assert_eq!(be, vec![0xC4, 0x0B], "Modbus RTU 线上为小端");
    }

    /// 反射语义：refin=false 时 poly 用正常形式，结果应与对应的反射写法不同，
    /// 避免"两种反射配置被静默当成同一个算法"
    #[test]
    fn test_reflect_flags_change_result() {
        let reflected = CrcParams::CRC16_MODBUS;
        let not_reflected = CrcParams {
            refin: false,
            refout: false,
            ..CrcParams::CRC16_MODBUS
        };
        assert_ne!(
            crc_generic(b"123456789", reflected),
            crc_generic(b"123456789", not_reflected)
        );
    }

    /// 64 位宽度：CRC-64/XZ 标准检查值 + 输出为 8 字节大端
    #[test]
    fn test_width_64_crc64_xz() {
        let xz = CrcParams {
            poly: 0x42F0_E1EB_A9EA_3693,
            init: 0xFFFF_FFFF_FFFF_FFFF,
            xorout: 0xFFFF_FFFF_FFFF_FFFF,
            refin: true,
            refout: true,
            width: 64,
        };
        assert_eq!(
            crc_generic(b"123456789", xz),
            0x995D_C9BB_DF19_39FA,
            "CRC-64/XZ 检查值应为 0x995DC9BBDF1939FA"
        );
        assert_eq!(crc_generic_bytes(b"123456789", xz).len(), 8);
        // 空输入 = init ^ xorout = 0
        assert_eq!(crc_generic(&[], xz), 0);
    }
}
