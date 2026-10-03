// 接收帧快照与 `rx.*` 取值层（L1 纯逻辑，不依赖 gpui / 不依赖网络层）
//
// 设计要点（见 docs/plan-reply-rules.md §5.1 与 docs/plan-reply-rules-expr.md §3）：
//   RxFrame    一次解码结果的**不可变快照**，用 `Arc` 在规则求值、变量渲染、
//              调试面板之间共享，避免为每条规则重复 clone 帧字节。
//   RxContext  变量取值的**统一入口**：`${rx.u16be:2}`、`${rx.raw:0:4}`、
//              `${rx.crc16modbus:0:6}` 全部落到这里。独立于渲染层存在，
//              因此调试面板可以用任意字节数组构造它做预演，不需要真的收包。
//   RxError    取值失败原因。渲染层据此决定"原样保留 vs 渲染为空 + warn"。

use crate::core::checksum::ChecksumAlgorithm;
use std::net::SocketAddr;
use std::sync::Arc;

/// 帧来自哪条路径（残帧判定直接消费本枚举）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOrigin {
    /// 解码器正常切出的完整帧
    Decoded,
    /// 静默超时被 `force_flush` 取走的半帧
    ForceFlushed,
    /// 连接结束时最后一次 `force_flush`
    EofFlushed,
}

impl FrameOrigin {
    /// 是否为"可能不完整"的帧 —— 残帧不参与规则匹配（[`crate::reply::matcher::evaluate`]）。
    /// 半截数据即便"碰巧"命中条件，回出的应答也与对端真实请求不对应。
    pub fn is_partial(self) -> bool {
        !matches!(self, FrameOrigin::Decoded)
    }
}

/// 帧元信息（目前只有来源标记）
///
/// 分帧序位（同一次 read 里的第几帧、一批拆出几帧）曾是 F-21 展示层的预留字段，
/// 因展示层未落地、也没有任何消费方而删除；F-21 实施时随消费方一起加回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMeta {
    /// 帧来自哪条路径 —— 残帧判定依赖它（非 `Decoded` 即不参与规则匹配）
    pub origin: FrameOrigin,
}

impl FrameMeta {
    /// 正常解码出的单帧
    pub fn decoded() -> Self {
        Self {
            origin: FrameOrigin::Decoded,
        }
    }
}

/// 一次解码结果的不可变快照。
///
/// `bytes` 是**解码后**的帧（已按解码器语义切好），规则匹配与 `rx.*` 取值都针对它。
#[derive(Debug)]
pub struct RxFrame {
    /// 帧原始字节。用 `Arc<[u8]>`：与同一帧产生的 `Message` 共享缓冲（P-6），
    /// 避免网络层为了"既跑规则又进消息明细"而复制整帧。
    pub bytes: Arc<[u8]>,
    /// 来源地址（服务端=对端；客户端=远端服务端；UDP=数据报来源）
    pub source: SocketAddr,
    /// 帧元信息（残帧判定用）
    pub meta: FrameMeta,
}

impl RxFrame {
    pub fn new(bytes: Vec<u8>, source: SocketAddr, meta: FrameMeta) -> Self {
        Self::from_shared(Arc::from(bytes), source, meta)
    }

    /// 由共享缓冲构造（P-6）：调用方已持有 `Arc<[u8]>` 时不再复制
    pub fn from_shared(bytes: Arc<[u8]>, source: SocketAddr, meta: FrameMeta) -> Self {
        Self {
            bytes,
            source,
            meta,
        }
    }

    /// 正常解码单帧的便捷构造（测试用；`127.0.0.1:0` 作为占位来源）
    #[cfg(test)]
    pub fn for_test(bytes: Vec<u8>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::new(
            bytes,
            "127.0.0.1:12345".parse().expect("合法测试地址"),
            FrameMeta::decoded(),
        ))
    }
}

/// 取值失败的原因（用于 warn 日志与调试面板的"实际值"列）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RxError {
    /// 偏移 + 宽度超出帧长
    OutOfRange {
        offset: usize,
        width: usize,
        len: usize,
    },
    /// 区间非法（起始 > 结束，或长度算术溢出）
    BadRange { start: usize, len: usize },
    /// 未知算法名
    UnknownAlgorithm(String),
    /// 未知的取值标识（如拼错的 `u17be`）
    UnknownAccessor(String),
}

impl RxError {
    /// 面向用户的简短说明（调试面板展示）
    pub fn describe(&self) -> String {
        match self {
            RxError::OutOfRange { offset, width, len } => {
                format!("越界: 偏移 {} 宽 {} 而帧长 {}", offset, width, len)
            }
            RxError::BadRange { start, len } => format!("区间非法: {}..{}", start, start + len),
            RxError::UnknownAlgorithm(name) => format!("未知校验算法: {}", name),
            RxError::UnknownAccessor(name) => format!("未知取值: {}", name),
        }
    }
}

/// 字节区间取值的呈现方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteView {
    /// 原始字节
    Raw,
    /// 大写 hex 文本（空格分隔）
    Hex,
    /// ASCII 文本（不可打印字符替换为 `.`）
    Ascii,
    /// Base64 文本
    Base64,
}

impl ByteView {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "raw" => Some(ByteView::Raw),
            "hex" => Some(ByteView::Hex),
            "ascii" => Some(ByteView::Ascii),
            "base64" => Some(ByteView::Base64),
            _ => None,
        }
    }
}

/// 变量取值的统一入口：所有 `rx.*` 与 `rx.crc*` 的语义都在这里。
///
/// 零拷贝（只持 `&RxFrame`），无堆分配 —— 热路径每帧构造一次的成本可忽略。
pub struct RxContext<'a> {
    frame: &'a RxFrame,
}

impl<'a> RxContext<'a> {
    pub fn new(frame: &'a RxFrame) -> Self {
        Self { frame }
    }

    /// 帧总字节数（`${rx.len}`）
    pub fn len(&self) -> usize {
        self.frame.bytes.len()
    }

    /// 帧字节（只读）
    pub fn bytes(&self) -> &[u8] {
        &self.frame.bytes
    }

    /// 来源地址 `IP:port`（`${rx.src}`）
    pub fn source(&self) -> String {
        self.frame.source.to_string()
    }

    /// 来源地址原值（`SocketAddr` 是 `Copy`，零堆分配 —— 匹配器 `From` 谓词用，P-5）
    pub fn source_addr(&self) -> SocketAddr {
        self.frame.source
    }

    /// 来源 IP（`${rx.src_ip}`）
    pub fn source_ip(&self) -> String {
        self.frame.source.ip().to_string()
    }

    /// 来源端口（`${rx.port}`）
    pub fn source_port(&self) -> u16 {
        self.frame.source.port()
    }

    /// 取字节区间 `[offset, offset+len)`；`len == None` 表示到帧尾。
    ///
    /// 空帧 + `raw`(off=0, len=None) 返回 Ok(空) —— `${rx.raw}` 在空帧上输出空串，
    /// 不是错误。
    pub fn slice(&self, offset: usize, len: Option<usize>) -> Result<&[u8], RxError> {
        let total = self.frame.bytes.len();
        if offset > total {
            return Err(RxError::OutOfRange {
                offset,
                width: len.unwrap_or(0),
                len: total,
            });
        }
        let end = match len {
            None => total,
            Some(l) => offset.checked_add(l).ok_or(RxError::BadRange {
                start: offset,
                len: l,
            })?,
        };
        if end > total {
            return Err(RxError::OutOfRange {
                offset,
                width: len.unwrap_or(0),
                len: total,
            });
        }
        Ok(&self.frame.bytes[offset..end])
    }

    /// 按访问标识读取标量。
    ///
    /// 支持：`u8` / `u16be` / `u16le` / `u32be` / `u32le` / `u64be` / `u64le`
    /// 以及有符号的 `i8` / `i16be` / … （决策 E-4/E-5：字节序与符号编码进名字）。
    pub fn get_scalar(&self, accessor: &str, offset: usize) -> Result<i64, RxError> {
        let (signed, width, little) = parse_scalar_accessor(accessor)
            .ok_or_else(|| RxError::UnknownAccessor(accessor.to_string()))?;
        let raw = self.slice(offset, Some(width))?;
        Ok(read_scalar(raw, width, little, signed))
    }

    /// 读取字节区间并按视图转换（`raw` / `hex` / `ascii` / `base64`）
    pub fn get_bytes(
        &self,
        kind: ByteView,
        offset: usize,
        len: Option<usize>,
    ) -> Result<Vec<u8>, RxError> {
        let slice = self.slice(offset, len)?;
        Ok(match kind {
            ByteView::Raw => slice.to_vec(),
            ByteView::Hex => format_hex(slice, true).into_bytes(),
            ByteView::Ascii => ascii_view(slice).into_bytes(),
            ByteView::Base64 => base64_encode(slice).into_bytes(),
        })
    }

    /// 对 `[offset, offset+len)` 计算校验值（`${rx.crc16modbus:0:6}`）。
    ///
    /// `little == true` 时反转多字节结果（决策 E-7）。
    pub fn checksum(
        &self,
        algorithm: ChecksumAlgorithm,
        offset: usize,
        len: usize,
        little: bool,
    ) -> Result<Vec<u8>, RxError> {
        let slice = self.slice(offset, Some(len))?;
        Ok(algorithm.compute_with_endian(slice, little))
    }

    /// 按算法名计算校验（匹配器 `ChecksumValid` 谓词用；未知算法名返回 Err）
    pub fn checksum_by_name(
        &self,
        algorithm: &str,
        offset: usize,
        len: usize,
        little: bool,
    ) -> Result<Vec<u8>, RxError> {
        let algo = ChecksumAlgorithm::parse(algorithm)
            .ok_or_else(|| RxError::UnknownAlgorithm(algorithm.to_string()))?;
        self.checksum(algo, offset, len, little)
    }
}

/// 解析标量访问标识 → (是否有符号, 字节数, 是否小端)
pub fn parse_scalar_accessor(accessor: &str) -> Option<(bool, usize, bool)> {
    let (signed, rest) = match accessor.strip_prefix('u') {
        Some(rest) => (false, rest),
        None => (true, accessor.strip_prefix('i')?),
    };
    let (digits, little) = match rest.strip_suffix("be") {
        Some(d) => (d, false),
        None => match rest.strip_suffix("le") {
            Some(d) => (d, true),
            None => (rest, false),
        },
    };
    let width = match digits {
        "8" => 1,
        "16" => 2,
        "32" => 4,
        "64" => 8,
        _ => return None,
    };
    // 单字节没有字节序差异；`u8le` 之类非标准写法直接拒绝，避免静默接受拼写错误
    if width == 1 && (rest.ends_with("be") || rest.ends_with("le")) {
        return None;
    }
    Some((signed, width, little))
}

/// 从字节读取标量（越界由调用方保证）
fn read_scalar(raw: &[u8], width: usize, little: bool, signed: bool) -> i64 {
    let mut value: u64 = 0;
    if little {
        for (i, &b) in raw.iter().enumerate() {
            value |= (b as u64) << (8 * i);
        }
    } else {
        for &b in raw {
            value = (value << 8) | b as u64;
        }
    }
    if signed {
        // 按宽度做符号扩展（如 i16 的 0xFFFF → -1）
        let shift = 64 - width * 8;
        ((value << shift) as i64) >> shift
    } else {
        value as i64
    }
}

/// 大写 hex 文本；`spaced` 为 true 时字节间以空格分隔
pub fn format_hex(bytes: &[u8], spaced: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * if spaced { 3 } else { 2 });
    for (i, b) in bytes.iter().enumerate() {
        if spaced && i > 0 {
            out.push(' ');
        }
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

/// ASCII 视图：可打印字符原样，其余替换为 `.`（NetAssist 的经典行为）
pub fn ascii_view(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7F).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}

/// 标准 Base64 编码（不引入依赖；协议调试场景数据量很小）
pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn frame(bytes: &[u8]) -> Arc<RxFrame> {
        RxFrame::for_test(bytes.to_vec())
    }

    fn assert_out_of_range(result: Result<i64, RxError>) {
        assert!(
            matches!(result, Err(RxError::OutOfRange { .. })),
            "应越界失败, 实际: {:?}",
            result
        );
    }

    /// 标量矩阵：宽度 × 字节序 × 符号，锁定 `RxContext::get_scalar` 的全部形态
    #[test]
    fn test_scalar_matrix() {
        let f = frame(&[0x01, 0x02, 0x03, 0x04, 0xFF, 0xFE, 0x7F, 0x80]);
        let rx = RxContext::new(&f);

        assert_eq!(rx.get_scalar("u8", 0), Ok(0x01));
        assert_eq!(rx.get_scalar("u8", 4), Ok(0xFF));
        assert_eq!(rx.get_scalar("i8", 4), Ok(-1));
        assert_eq!(rx.get_scalar("i8", 6), Ok(0x7F));
        assert_eq!(rx.get_scalar("i8", 7), Ok(-128));

        assert_eq!(rx.get_scalar("u16be", 0), Ok(0x0102));
        assert_eq!(rx.get_scalar("u16le", 0), Ok(0x0201));
        assert_eq!(rx.get_scalar("i16be", 4), Ok(-2));
        assert_eq!(rx.get_scalar("i16le", 4), Ok(0xFEFF_u16 as i16 as i64));

        assert_eq!(rx.get_scalar("u32be", 0), Ok(0x01020304));
        assert_eq!(rx.get_scalar("u32le", 0), Ok(0x04030201));
        assert_eq!(rx.get_scalar("i32be", 4), Ok(0xFFFE_7F80_u32 as i32 as i64));

        assert_eq!(
            rx.get_scalar("u64be", 0),
            Ok(0x01020304_FFFE7F80_u64 as i64)
        );
        assert_eq!(
            rx.get_scalar("i64be", 0),
            Ok(0x01020304_FFFE7F80_u64 as i64)
        );
    }

    /// 越界一律 Err（绝不 panic：调试工具崩了比不工作更糟）
    #[test]
    fn test_scalar_out_of_range() {
        let f = frame(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        let rx = RxContext::new(&f);
        // 帧长 8: 偏移 7 起读 u16 需要第 8 字节 → 越界
        assert_out_of_range(rx.get_scalar("u16be", 7));
        // 偏移正好等于帧长
        assert_out_of_range(rx.get_scalar("u8", 8));
        // 超出很多
        assert_out_of_range(rx.get_scalar("u32be", 99));
        // 偏移 6 起读 u16 正常
        assert_eq!(rx.get_scalar("u16be", 6), Ok(0x0708));
    }

    /// 空帧上的行为（边界表：`${rx.u8:0}` 越界、`${rx.len}` = 0）
    #[test]
    fn test_empty_frame() {
        let f = frame(&[]);
        let rx = RxContext::new(&f);
        assert_eq!(rx.len(), 0);
        assert_out_of_range(rx.get_scalar("u8", 0));
        // raw 整帧在空帧上输出空串，而非错误
        assert_eq!(rx.get_bytes(ByteView::Raw, 0, None), Ok(Vec::new()));
        // CRC16 空输入 = 0xFFFF（与 checksum.rs 既有断言一致）
        assert_eq!(
            rx.checksum(ChecksumAlgorithm::Crc16Modbus, 0, 0, false),
            Ok(vec![0xFF, 0xFF])
        );
    }

    /// 未知访问标识必须被拒绝（拼错的 `u17be` 不能被静默当成别的宽度）
    #[test]
    fn test_unknown_accessor_rejected() {
        assert_eq!(parse_scalar_accessor("u17be"), None);
        assert_eq!(parse_scalar_accessor("u8le"), None);
        assert_eq!(parse_scalar_accessor("x16be"), None);
        assert_eq!(parse_scalar_accessor("u16"), Some((false, 2, false)));
        assert_eq!(parse_scalar_accessor("i32le"), Some((true, 4, true)));

        let f = frame(&[1, 2, 3, 4]);
        let rx = RxContext::new(&f);
        assert_eq!(
            rx.get_scalar("u17be", 0),
            Err(RxError::UnknownAccessor("u17be".to_string()))
        );
    }

    /// 区间视图：raw / hex / ascii / base64 及各自边界
    #[test]
    fn test_byte_views() {
        let f = frame(b"AT+CSQ\r\n");
        let rx = RxContext::new(&f);

        assert_eq!(rx.get_bytes(ByteView::Raw, 0, Some(3)), Ok(b"AT+".to_vec()));
        assert_eq!(
            rx.get_bytes(ByteView::Hex, 0, Some(3)),
            Ok(b"41 54 2B".to_vec())
        );
        assert_eq!(
            rx.get_bytes(ByteView::Ascii, 0, Some(8)),
            Ok(b"AT+CSQ..".to_vec()),
            "不可打印字符应替换为 ."
        );
        assert_eq!(
            rx.get_bytes(ByteView::Base64, 0, Some(3)),
            Ok(b"QVQr".to_vec())
        );
        // len 省略 = 到帧尾（帧尾是 \r\n 两字节）
        assert_eq!(rx.get_bytes(ByteView::Raw, 6, None), Ok(b"\r\n".to_vec()));
        // 单字节到帧尾
        assert_eq!(rx.get_bytes(ByteView::Raw, 7, None), Ok(b"\n".to_vec()));
        // 零长度区间合法
        assert_eq!(rx.get_bytes(ByteView::Raw, 3, Some(0)), Ok(Vec::new()));
        // 起点越界
        assert!(rx.get_bytes(ByteView::Raw, 99, None).is_err());
        // 长度超出帧尾
        assert!(rx.get_bytes(ByteView::Raw, 0, Some(99)).is_err());
    }

    /// Base64 编码对标准向量（含补位）正确
    #[test]
    fn test_base64_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0x01, 0x03, 0x00, 0x00]), "AQMAAA==");
    }

    /// 元信息取值
    #[test]
    fn test_meta_accessors() {
        let f = RxFrame::new(
            vec![1, 2, 3],
            "192.168.1.7:5000".parse().unwrap(),
            FrameMeta::decoded(),
        );
        let rx = RxContext::new(&f);
        assert_eq!(rx.len(), 3);
        assert_eq!(rx.source(), "192.168.1.7:5000");
        assert_eq!(rx.source_ip(), "192.168.1.7");
        assert_eq!(rx.source_port(), 5000);
    }

    /// 校验取值：6 种算法 + 字节序，与 toolbox 既有断言交叉验证
    #[test]
    fn test_checksum_accessors() {
        // Modbus 读保持寄存器请求帧（不含 CRC）
        let f = frame(&[0x01, 0x03, 0x00, 0x00, 0x00, 0x02]);
        let rx = RxContext::new(&f);

        assert_eq!(
            rx.checksum(ChecksumAlgorithm::Crc16Modbus, 0, 6, false),
            Ok(vec![0x0B, 0xC4])
        );
        assert_eq!(
            rx.checksum(ChecksumAlgorithm::Crc16Modbus, 0, 6, true),
            Ok(vec![0xC4, 0x0B]),
            "Modbus RTU 线上为小端"
        );
        assert_eq!(
            rx.checksum(ChecksumAlgorithm::Xor, 0, 3, false),
            Ok(vec![0x02])
        );
        assert_eq!(
            rx.checksum(ChecksumAlgorithm::Sum8, 0, 3, false),
            Ok(vec![0x04])
        );
        assert_eq!(
            rx.checksum(ChecksumAlgorithm::Lrc, 0, 2, false),
            Ok(vec![0xFC])
        );
        assert_eq!(
            rx.checksum(ChecksumAlgorithm::Crc32, 0, 6, false)
                .map(|v| v.len()),
            Ok(4)
        );
        assert_eq!(
            rx.checksum_by_name("crc16_modbus", 0, 6, false),
            Ok(vec![0x0B, 0xC4])
        );
        assert_eq!(
            rx.checksum_by_name("crc16modbus", 0, 6, true),
            Ok(vec![0xC4, 0x0B])
        );
        // 未知算法名 → Err
        assert_eq!(
            rx.checksum_by_name("sha256", 0, 6, false),
            Err(RxError::UnknownAlgorithm("sha256".to_string()))
        );
        // 区间越界 → Err
        assert!(
            rx.checksum(ChecksumAlgorithm::Crc16Modbus, 0, 99, false)
                .is_err()
        );
    }

    /// 帧来源标记：静默强刷的半帧必须可区分（残帧闸的语义基础）
    #[test]
    fn test_frame_origin_partial() {
        assert!(!FrameOrigin::Decoded.is_partial());
        assert!(FrameOrigin::ForceFlushed.is_partial());
        assert!(FrameOrigin::EofFlushed.is_partial());
    }

    /// hex 文本格式：默认无分隔、spaced 时空格分隔
    #[test]
    fn test_format_hex() {
        assert_eq!(format_hex(&[0x01, 0xAB], true), "01 AB");
        assert_eq!(format_hex(&[0x01, 0xAB], false), "01AB");
        assert_eq!(format_hex(&[], true), "");
        assert_eq!(ascii_view(&[0x41, 0x00, 0x7F, 0x80]), "A...");
    }
}
