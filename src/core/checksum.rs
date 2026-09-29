// 校验算法纯函数（不依赖 gpui）
//
// 覆盖二进制帧/私有协议调试的常用校验。仅用于"对给定字节计算校验值"展示，
// 不涉及发送帧填充或接收侧验算（那些属于另一特性）。

/// 校验算法
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// 计算校验值，返回 1（XOR/Sum8/LRC）、2（CRC16）或 4（CRC32）字节，按大端放置。
    pub fn compute(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            ChecksumAlgorithm::Xor => vec![bytes.iter().fold(0u8, |acc, &b| acc ^ b)],
            ChecksumAlgorithm::Sum8 => vec![bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))],
            ChecksumAlgorithm::Lrc => {
                let sum = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
                vec![(!sum).wrapping_add(1)]
            }
            ChecksumAlgorithm::Crc16Modbus => {
                let crc = crc16_modbus(bytes);
                vec![(crc >> 8) as u8, crc as u8]
            }
            ChecksumAlgorithm::Crc16Ccitt => {
                let crc = crc16_ccitt_false(bytes);
                vec![(crc >> 8) as u8, crc as u8]
            }
            ChecksumAlgorithm::Crc32 => {
                let crc = crc32_ieee(bytes);
                vec![
                    (crc >> 24) as u8,
                    (crc >> 16) as u8,
                    (crc >> 8) as u8,
                    crc as u8,
                ]
            }
        }
    }
}

/// CRC16/MODBUS：反射模式，多项式反射为 0xA001。
/// 检查值 "123456789" → 0x4B37。
fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if crc & 0x0001 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// CRC16/CCITT-FALSE：非反射，多项式 0x1021。
/// 检查值 "123456789" → 0x29B1。
fn crc16_ccitt_false(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// CRC32/ISO-HDLC：反射，多项式反射为 0xEDB88320，xorout 0xFFFFFFFF。
/// 检查值 "123456789" → 0xCBF43926。
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg() as u32;
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 统一检查字节串 "123456789"（标准 CRC 检查向量），锁定算法参数。
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

    #[test]
    fn test_xor_sum8_lrc() {
        // 0x01 ^ 0x03 ^ 0x00 = 0x02
        assert_eq!(ChecksumAlgorithm::Xor.compute(&[0x01, 0x03, 0x00]), vec![0x02]);
        // 求和
        assert_eq!(ChecksumAlgorithm::Sum8.compute(&[0x01, 0x03, 0x00]), vec![0x04]);
        // 进位置低 8 位
        assert_eq!(
            ChecksumAlgorithm::Sum8.compute(&[0xFF, 0x01]),
            vec![0x00]
        );
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
        assert_eq!(
            ChecksumAlgorithm::Crc16Ccitt.compute(&[]),
            vec![0xFF, 0xFF]
        );
        // CRC32 空 = 0x00000000
        assert_eq!(
            ChecksumAlgorithm::Crc32.compute(&[]),
            vec![0x00, 0x00, 0x00, 0x00]
        );
    }
}