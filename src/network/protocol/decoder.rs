use crate::config::connection::DecoderConfig;
use bytes::BytesMut;
use log::debug;
use tokio_util::codec::{BytesCodec, Decoder, Encoder, LengthDelimitedCodec};

/// 扩展的解码器trait，支持强制刷新缓冲区
pub trait ExtendedDecoder: Decoder<Item = BytesMut, Error = std::io::Error> + Send + Sync {
    /// 是否还有未成帧的残留数据(读循环据此决定是否启动静默计时)
    fn has_pending(&self) -> bool;

    /// 强制刷新缓冲区: 返回并**取走**全部残留数据(缓冲区随之清空)。
    ///
    /// 采用消费式而非 peek: peek 会让缓冲区只进不出 —— 每次静默都重复吐全量,
    /// 显示量按 O(n²) 增长、内存无限膨胀(见 plans/plan-tcp-decoder-half-frame-flush.md)。
    /// 代价是跨静默的半帧不再拼回: 靠分隔符/语法定界的解码器(换行符 / JSON)
    /// 下一条分隔符即自愈; 靠累计字节数对齐的解码器(固定长度 / 长度前缀)
    /// 被切开后会永久错位 —— 该取舍已确认接受。
    fn force_flush(&mut self) -> Option<BytesMut>;
}

/// 原始数据解码器类型别名
pub type BytesDecoder = BytesCodec;

impl ExtendedDecoder for BytesDecoder {
    fn has_pending(&self) -> bool {
        // BytesDecoder没有缓冲区
        false
    }

    fn force_flush(&mut self) -> Option<BytesMut> {
        // BytesDecoder没有缓冲区，总是返回None
        None
    }
}

/// 长度前缀解码器类型别名
pub type LengthDelimitedDecoder = LengthDelimitedCodec;

/// Codec工厂，用于根据配置生成相应的解码器
pub struct CodecFactory;

impl CodecFactory {
    /// 根据配置创建相应的decoder，返回Box<dyn ExtendedDecoder>
    pub fn create_decoder(config: &DecoderConfig) -> Box<dyn ExtendedDecoder> {
        debug!("CodecFactory: 创建解码器，配置: {:?}", config);

        match config {
            DecoderConfig::Bytes => {
                debug!("CodecFactory: 使用Bytes解码器");
                Box::new(BytesDecoder::new())
            }
            DecoderConfig::LineBased => {
                debug!("CodecFactory: 使用LineBased解码器");
                Box::new(LineToBytesMutDecoder::new())
            }
            DecoderConfig::LengthDelimited(config) => {
                debug!(
                    "CodecFactory: 使用LengthDelimited解码器，配置: {:?}",
                    config
                );
                // tokio-util 0.7 的 Builder 没有 length_field_includes_self 方法,
                // 通过调整 length_adjustment 来补偿: 长度字段包含自身时, 需减去长度字段本身的字节数
                let effective_adjustment = if config.length_field_is_including_length_field {
                    config.length_adjustment - config.length_field_length as i32
                } else {
                    config.length_adjustment
                };
                // 保留完整帧: 默认 tokio-util 会跳过 offset+长度字段, 仅返回载荷.
                // 保留完整帧时: num_skip=0(不跳过头部), 并把 offset+长度字段 加回 adjustment,
                // 使返回字节数 n = 完整帧长(offset+长度字段+载荷).
                let (final_adjustment, num_skip) = if config.length_field_keep_full_frame {
                    let adj = effective_adjustment
                        + config.length_field_offset as i32
                        + config.length_field_length as i32;
                    (adj, Some(0usize))
                } else {
                    (effective_adjustment, None)
                };
                let length_delimited = {
                    let mut builder = LengthDelimitedDecoder::builder();
                    builder
                        .max_frame_length(config.max_frame_length)
                        .length_field_offset(config.length_field_offset.into())
                        .length_field_length((config.length_field_length as usize).max(1).min(8))
                        .length_adjustment(final_adjustment.try_into().unwrap_or(0));
                    // 根据配置选择字节序: 默认大端, 配置为小端时切换
                    if config.length_field_is_little_endian {
                        builder.little_endian();
                    }
                    // 保留完整帧时显式设置 num_skip=0; 否则使用默认(offset+长度字段)
                    if let Some(skip) = num_skip {
                        builder.num_skip(skip);
                    }
                    builder.new_codec()
                };
                Box::new(LengthDelimitedToBytesMutDecoder::new(length_delimited))
            }
            DecoderConfig::FixedLength(frame_length) => {
                debug!(
                    "CodecFactory: 使用FixedLength解码器，帧长度: {}",
                    frame_length
                );
                Box::new(FixedLengthDecoder::new(*frame_length))
            }
            DecoderConfig::Json => {
                debug!("CodecFactory: 使用JSON解码器（基于serde_json StreamDeserializer）");
                Box::new(JsonDecoder::new())
            }
        }
    }

    /// 根据配置创建相应的encoder，返回Box<dyn Encoder<BytesMut, Error = std::io::Error>>
    pub fn create_encoder(
        config: &DecoderConfig,
    ) -> Box<dyn Encoder<BytesMut, Error = std::io::Error> + Send + Sync> {
        match config {
            DecoderConfig::Bytes => Box::new(BytesDecoder::new()),
            DecoderConfig::LineBased => {
                // 将LinesCodec包装成输入BytesMut的Encoder
                Box::new(LineToBytesMutEncoder::new())
            }
            DecoderConfig::LengthDelimited(_) => {
                // 使用BytesEncoder作为默认编码器
                Box::new(BytesDecoder::new())
            }
            DecoderConfig::FixedLength(_) => {
                // 固定长度解码只影响接收分帧，发送时原样输出
                Box::new(BytesDecoder::new())
            }
            DecoderConfig::Json => {
                // 对于JSON，我们直接使用BytesCodec
                Box::new(BytesDecoder::new())
            }
        }
    }
}

/// 自定义换行符解码器
/// 立即处理所有以换行符结尾的完整行，剩余数据暂存等待后续处理
struct LineToBytesMutDecoder {
    pending_data: BytesMut, // 没有换行符的待处理数据
}

impl LineToBytesMutDecoder {
    fn new() -> Self {
        Self {
            pending_data: BytesMut::new(),
        }
    }
}

impl ExtendedDecoder for LineToBytesMutDecoder {
    fn has_pending(&self) -> bool {
        !self.pending_data.is_empty()
    }

    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "LineToBytesMutDecoder: 强制刷新缓冲区: {:?}, 长度: {}",
                String::from_utf8_lossy(&self.pending_data),
                self.pending_data.len()
            );
            // 取走残留并清空缓冲区: 下次静默只吐新增数据, 不重复显示全量
            Some(std::mem::take(&mut self.pending_data))
        } else {
            None
        }
    }
}

impl Decoder for LineToBytesMutDecoder {
    type Item = BytesMut;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 先把新数据并入待处理缓冲区: 一行可能跨多次 read,
        // 只在 src 中查找换行符会把同一行拆成两条且顺序颠倒
        if !src.is_empty() {
            self.pending_data.extend_from_slice(src);
            src.clear();
        }

        // 查找完整的行
        if let Some(newline_pos) = self.pending_data.iter().position(|&b| b == b'\n') {
            // 提取完整的行（包括换行符）
            let mut line = self.pending_data.split_to(newline_pos + 1);

            // 移除行尾的\r（如果有）
            let line = if line.len() > 1 && line[line.len() - 2] == b'\r' {
                line.split_to(line.len() - 2) // 移除\r\n
            } else {
                line.split_to(line.len() - 1) // 移除\n
            };

            // 返回完整行
            return Ok(Some(line));
        }

        // 没有完整的行, 等待后续数据
        Ok(None)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 处理剩余数据
        if !src.is_empty() {
            let remaining = src.split_to(src.len());
            debug!(
                "LineToBytesMutDecoder: decode_eof 返回剩余数据: {:?}, 长度: {}",
                String::from_utf8_lossy(&remaining),
                remaining.len()
            );
            Ok(Some(remaining))
        } else if !self.pending_data.is_empty() {
            // 返回待处理数据
            debug!(
                "LineToBytesMutDecoder: decode_eof 返回待处理数据: {:?}, 长度: {}",
                String::from_utf8_lossy(&self.pending_data),
                self.pending_data.len()
            );
            Ok(Some(self.pending_data.split_to(self.pending_data.len())))
        } else {
            Ok(None)
        }
    }
}

/// 换行符编码器到BytesMut编码器的适配器
struct LineToBytesMutEncoder {
    // 不需要内部编码器，直接处理
}

impl LineToBytesMutEncoder {
    fn new() -> Self {
        Self {
            // 无内部状态
        }
    }
}

impl Encoder<BytesMut> for LineToBytesMutEncoder {
    type Error = std::io::Error;

    fn encode(&mut self, item: BytesMut, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // 直接将数据添加到目标缓冲区
        dst.extend_from_slice(&item);
        Ok(())
    }
}

/// 长度前缀解码器到BytesMut解码器的适配器
struct LengthDelimitedToBytesMutDecoder {
    inner: LengthDelimitedDecoder,
    pending_data: BytesMut, // 存储未完成的消息数据
}

impl LengthDelimitedToBytesMutDecoder {
    fn new(inner: LengthDelimitedDecoder) -> Self {
        Self {
            inner,
            pending_data: BytesMut::new(),
        }
    }
}

impl Decoder for LengthDelimitedToBytesMutDecoder {
    type Item = BytesMut;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 保存当前数据到pending_data
        if !src.is_empty() {
            self.pending_data.extend_from_slice(src);
            src.clear();
        }

        // 尝试解码
        match self.inner.decode(&mut self.pending_data) {
            Ok(Some(bytes)) => Ok(Some(BytesMut::from(bytes))),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl ExtendedDecoder for LengthDelimitedToBytesMutDecoder {
    fn has_pending(&self) -> bool {
        !self.pending_data.is_empty()
    }

    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "LengthDelimitedToBytesMutDecoder: 强制刷新缓冲区: {:?}, 长度: {}",
                String::from_utf8_lossy(&self.pending_data),
                self.pending_data.len()
            );
            // 取走残留并清空缓冲区: 下次静默只吐新增数据, 不重复显示全量
            Some(std::mem::take(&mut self.pending_data))
        } else {
            None
        }
    }
}

/// 固定长度解码器
/// 缓冲数据，每凑够 frame_length 字节切出一帧
struct FixedLengthDecoder {
    frame_length: usize,
    pending_data: BytesMut,
}

impl FixedLengthDecoder {
    fn new(frame_length: usize) -> Self {
        Self {
            frame_length: frame_length.max(1),
            pending_data: BytesMut::new(),
        }
    }
}

impl Decoder for FixedLengthDecoder {
    type Item = BytesMut;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 累积新数据
        if !src.is_empty() {
            self.pending_data.extend_from_slice(src);
            src.clear();
        }

        // 凑够一帧则切出(即使 src 为空, 也要检查 pending_data 中的剩余数据)
        if self.pending_data.len() >= self.frame_length {
            let frame = self.pending_data.split_to(self.frame_length);
            Ok(Some(frame))
        } else {
            Ok(None)
        }
    }
}

impl ExtendedDecoder for FixedLengthDecoder {
    fn has_pending(&self) -> bool {
        !self.pending_data.is_empty()
    }

    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "FixedLengthDecoder: 强制刷新缓冲区, 长度: {}",
                self.pending_data.len()
            );
            // 取走残留并清空缓冲区: 下次静默只吐新增数据, 不重复显示全量
            Some(std::mem::take(&mut self.pending_data))
        } else {
            None
        }
    }
}

/// JSON 流式解码器
/// 基于 serde_json::StreamDeserializer, 支持无分隔符拼接的 JSON 流(如 {"1":"1"}{"1":"1"}),
/// 每解析出一个完整 JSON 值即切出一帧(保留该值的原始字节, 供上层展示)。
/// 数据不完整时等待更多数据; 语法错误时跳到下一个可能的 JSON 起点重新同步,
/// 被跳过的非法字节**不丢弃**, 而是作为一条消息吐出(调试工具不应丢数据);
/// 对端静默/连接断开时由 force_flush 取走并清空残留(消费式, 见该 trait 方法说明)。
struct JsonDecoder {
    pending_data: BytesMut,
    /// 无法解析而被跳过的非法字节。
    ///
    /// 不变量: `decode` 的每条返回路径都会先把它吐空(见 decode 内的三处 return),
    /// 因此它在 `decode` 之外恒为空, 不参与 `has_pending` / `force_flush` 的判断。
    skipped: BytesMut,
}

impl JsonDecoder {
    fn new() -> Self {
        Self {
            pending_data: BytesMut::new(),
            skipped: BytesMut::new(),
        }
    }
}

/// 查找下一个"可能是 JSON 值起点"的字节位置。
///
/// 合法 JSON 值只可能以 `{ [ " - 数字 t f n` 开头, 其余字节不可能成为值的起点,
/// 可以整段跳过 —— 这只是加速(避免逐字节重试解析的 O(n²) 开销), 正确性仍由
/// 解析器兜底: 若猜到的位置也解析不了, 就继续往后跳。
/// 返回值至少为 1, 保证重同步一定能推进(否则会死循环)。
fn next_value_start(data: &[u8]) -> Option<usize> {
    // 从下标 1 开始找, 保证至少前进 1 字节(否则会死循环)
    data.get(1..).and_then(|rest| {
        rest.iter()
            .position(|&b| {
                matches!(
                    b,
                    b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
                )
            })
            .map(|i| i + 1)
    })
}

impl Decoder for JsonDecoder {
    type Item = BytesMut;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 累积新数据到 pending_data
        if !src.is_empty() {
            self.pending_data.extend_from_slice(src);
            src.clear();
        }

        loop {
            // 全是无法解析的字节: 作为一条消息吐出, 不静默丢弃
            if self.pending_data.is_empty() {
                if !self.skipped.is_empty() {
                    return Ok(Some(std::mem::take(&mut self.skipped)));
                }
                return Ok(None);
            }

            // 在内部作用域完成解析, 借用结束后再修改 pending_data。
            // 解析目标为 serde::de::IgnoredAny: 只定位 JSON 值的边界, 不构建对象树
            // (压测下省掉每条消息的 Value 分配)。
            let parse_result: Result<Option<usize>, serde_json::Error> = {
                let mut stream = serde_json::Deserializer::from_slice(&self.pending_data)
                    .into_iter::<serde::de::IgnoredAny>();
                match stream.next() {
                    Some(Ok(_)) => Ok(Some(stream.byte_offset())),
                    // 数据不完整(如半个 JSON 值), 等待后续数据
                    Some(Err(e)) if e.is_eof() => Ok(None),
                    // 真正的语法错误
                    Some(Err(e)) => Err(e),
                    None => Ok(None),
                }
            };

            match parse_result {
                Ok(Some(offset)) => {
                    // 先把之前跳过的非法字节吐出, 保证与原始流顺序一致
                    if !self.skipped.is_empty() {
                        return Ok(Some(std::mem::take(&mut self.skipped)));
                    }
                    let frame = self.pending_data.split_to(offset);
                    return Ok(Some(frame));
                }
                Ok(None) => {
                    // 数据不完整: 但已确认是垃圾的字节先吐出来, 不必陪它一起等
                    if !self.skipped.is_empty() {
                        return Ok(Some(std::mem::take(&mut self.skipped)));
                    }
                    return Ok(None);
                }
                Err(e) => {
                    // 跳到下一个可能的 JSON 起点重新同步: 否则一次坏数据会让后续每次
                    // decode 都立刻报错, 残留永远吐不出去, 整条连接永久失能。
                    // 跳过的字节先攒起来(而不是 split_to 丢弃), 稍后作为一条消息吐出。
                    let skip = next_value_start(&self.pending_data).unwrap_or(self.pending_data.len());
                    debug!("JsonDecoder: 跳过 {} 字节以重新同步: {}", skip, e);
                    let skipped = self.pending_data.split_to(skip);
                    self.skipped.extend_from_slice(&skipped);
                }
            }
        }
    }

    fn decode_eof(&mut self, _src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 流结束时返回残留数据(可能是不完整的 JSON)
        if !self.pending_data.is_empty() {
            let remaining = self.pending_data.split_to(self.pending_data.len());
            debug!(
                "JsonDecoder: decode_eof 返回残留数据, 长度: {}, 内容: {:?}",
                remaining.len(),
                String::from_utf8_lossy(&remaining)
            );
            Ok(Some(remaining))
        } else {
            Ok(None)
        }
    }
}

impl ExtendedDecoder for JsonDecoder {
    fn has_pending(&self) -> bool {
        !self.pending_data.is_empty()
    }

    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "JsonDecoder: 强制刷新缓冲区, 长度: {}, 内容: {:?}",
                self.pending_data.len(),
                String::from_utf8_lossy(&self.pending_data)
            );
            // 取走残留并清空缓冲区: 下次静默只吐新增数据, 不重复显示全量
            Some(std::mem::take(&mut self.pending_data))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection::LengthDelimitedConfig;

    /// 把一段字节喂给解码器(模拟一次 read), 返回本次切出的所有帧
    fn decode_all(decoder: &mut Box<dyn ExtendedDecoder>, data: &[u8]) -> Vec<Vec<u8>> {
        let mut src = BytesMut::from(data);
        let mut frames = Vec::new();
        while let Ok(Some(frame)) = decoder.decode(&mut src) {
            frames.push(frame.to_vec());
        }
        frames
    }

    fn v(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    fn json_decoder() -> Box<dyn ExtendedDecoder> {
        CodecFactory::create_decoder(&DecoderConfig::Json)
    }

    #[test]
    fn json_splits_multiple_values_in_one_read() {
        let mut d = json_decoder();
        let frames = decode_all(&mut d, br#"{"n":0}{"n":1}{"n":2}"#);
        assert_eq!(frames, vec![v(r#"{"n":0}"#), v(r#"{"n":1}"#), v(r#"{"n":2}"#)]);
        assert!(!d.has_pending());
    }

    #[test]
    fn json_joins_value_split_across_reads() {
        let mut d = json_decoder();
        assert!(decode_all(&mut d, br#"{"n":"#).is_empty());
        assert!(d.has_pending());
        assert_eq!(decode_all(&mut d, br#"1}"#), vec![v(r#"{"n":1}"#)]);
        assert!(!d.has_pending());
    }

    /// 核心回归: 强刷是消费式的 —— 残留被取走后缓冲区必须为空, 后续字节不再与前半截拼回
    #[test]
    fn json_consume_flush_resets_buffer() {
        let mut d = json_decoder();
        assert!(decode_all(&mut d, br#"{"b":2"#).is_empty());

        assert_eq!(d.force_flush().as_deref(), Some(&br#"{"b":2"#[..]));
        assert!(!d.has_pending(), "消费式强刷后缓冲区必须为空");

        // 前半截已被取走, 后续字节各自成帧(数字 3 与无法解析的 })
        assert_eq!(decode_all(&mut d, br#"3}"#), vec![v("3"), v("}")]);
        assert!(!d.has_pending());
    }

    /// 非法字节不应让解码器永久失能, 跳过重同步后仍能切出后续合法 JSON
    #[test]
    fn json_resyncs_after_invalid_bytes() {
        let mut d = json_decoder();
        assert_eq!(
            decode_all(&mut d, br#"}{"n":0}"#),
            vec![v("}"), v(r#"{"n":0}"#)]
        );
        assert!(!d.has_pending());
    }

    /// 非法字节不能被静默丢弃, 要作为一条消息吐出(调试工具不应丢数据)
    #[test]
    fn json_keeps_garbage_only_input() {
        let mut d = json_decoder();
        assert_eq!(
            decode_all(&mut d, "啊士大夫".as_bytes()),
            vec![v("啊士大夫")]
        );
        assert!(!d.has_pending());
    }

    /// 合法值之间夹杂的非法字节, 按原始流顺序分块吐出
    #[test]
    fn json_keeps_garbage_between_values() {
        let mut d = json_decoder();
        assert_eq!(
            decode_all(&mut d, "332让3 人".as_bytes()),
            vec![v("332让"), v("3"), v(" 人")]
        );
        assert!(!d.has_pending());
    }

    /// 非法字节在前, 合法 JSON 在后: 两块都要出来
    #[test]
    fn json_keeps_garbage_before_valid_value() {
        let mut d = json_decoder();
        assert_eq!(
            decode_all(&mut d, "啊{\"a\":1}".as_bytes()),
            vec![v("啊"), v(r#"{"a":1}"#)]
        );
        assert!(!d.has_pending());
    }

    /// 非法字节 + 未完成的 JSON: 垃圾先吐出, 半包仍留在缓冲区等静默强刷
    #[test]
    fn json_keeps_garbage_and_waits_for_half_frame() {
        let mut d = json_decoder();
        assert_eq!(decode_all(&mut d, "啊{\"a\":".as_bytes()), vec![v("啊")]);
        assert!(d.has_pending());
        assert_eq!(d.force_flush().as_deref(), Some(&br#"{"a":"#[..]));
    }

    /// 独立 bug: 一行跨两次 read 时必须拼成一条, 且顺序不能颠倒
    #[test]
    fn line_based_joins_line_split_across_reads() {
        let mut d = CodecFactory::create_decoder(&DecoderConfig::LineBased);
        assert!(decode_all(&mut d, b"abcdef").is_empty());
        assert!(d.has_pending());
        assert_eq!(decode_all(&mut d, b"gh\n"), vec![v("abcdefgh")]);
        assert!(!d.has_pending());
    }

    /// 消费式强刷后残留被取走: 换行符解码器靠分隔符自愈, 只吐静默后的新增数据
    #[test]
    fn line_based_consume_flush_resets_buffer() {
        let mut d = CodecFactory::create_decoder(&DecoderConfig::LineBased);
        assert!(decode_all(&mut d, b"partial").is_empty());
        assert_eq!(d.force_flush().as_deref(), Some(&b"partial"[..]));
        assert!(!d.has_pending(), "消费式强刷后缓冲区必须为空");

        // 旧残留已吐过, 只出新增部分(换行符分帧不依赖前缀, 自然自愈)
        assert_eq!(decode_all(&mut d, b"rest\n"), vec![v("rest")]);
        assert!(!d.has_pending());
    }

    /// 第二次静默只吐新增: 强刷消费掉缓冲区, 不会重复吐全量(O(n²) 膨胀的回归)
    #[test]
    fn consume_flush_emits_only_new_bytes() {
        let mut d = CodecFactory::create_decoder(&DecoderConfig::LineBased);
        assert!(decode_all(&mut d, b"aa").is_empty());
        assert_eq!(d.force_flush().as_deref(), Some(&b"aa"[..]));

        assert!(decode_all(&mut d, b"bb").is_empty());
        assert_eq!(d.force_flush().as_deref(), Some(&b"bb"[..]));

        // 无新增数据时不再返回任何东西
        assert!(d.force_flush().is_none());
    }

    #[test]
    fn line_based_splits_multiple_lines_and_trims_cr() {
        let mut d = CodecFactory::create_decoder(&DecoderConfig::LineBased);
        assert_eq!(decode_all(&mut d, b"a\r\nb\n"), vec![v("a"), v("b")]);
        assert!(!d.has_pending());
    }

    /// 消费式强刷的已知代价: 固定长度解码靠累计字节数对齐, 前缀被取走后永久错位
    #[test]
    fn fixed_length_consume_flush_shifts_alignment() {
        let mut d = CodecFactory::create_decoder(&DecoderConfig::FixedLength(4));
        assert_eq!(decode_all(&mut d, b"AAAABBBBCC"), vec![v("AAAA"), v("BBBB")]);
        assert_eq!(d.force_flush().as_deref(), Some(&b"CC"[..]));
        assert!(!d.has_pending(), "消费式强刷后缓冲区必须为空");

        // CC 已被取走, 不再参与拼帧: 后续字节只能按新边界重新对齐
        assert!(decode_all(&mut d, b"DD").is_empty());
        assert_eq!(decode_all(&mut d, b"DD"), vec![v("DDDD")]);
        assert!(!d.has_pending());
    }

    /// 消费式强刷的已知代价: 长度前缀被取走后, 后续帧要等下一个长度头才重新同步
    #[test]
    fn length_delimited_consume_flush_shifts_alignment() {
        let mut d = CodecFactory::create_decoder(&DecoderConfig::LengthDelimited(
            LengthDelimitedConfig::default(),
        ));
        // 帧 = 4 字节大端长度字段 + 载荷(默认保留完整帧)
        assert_eq!(
            decode_all(&mut d, &[0, 0, 0, 2, b'A', b'A', 0, 0, 0, 3]),
            vec![vec![0, 0, 0, 2, b'A', b'A']]
        );
        assert_eq!(d.force_flush().as_deref(), Some(&[0u8, 0, 0, 3][..]));
        assert!(!d.has_pending(), "消费式强刷后缓冲区必须为空");

        // 长度头已随残留一起吐掉, 载荷 BBB 失去归属, 只能等下一个长度头
        assert!(decode_all(&mut d, b"BBB").is_empty());
        assert!(d.has_pending());
    }

    #[test]
    fn bytes_decoder_has_no_pending() {
        let mut d = CodecFactory::create_decoder(&DecoderConfig::Bytes);
        assert!(!d.has_pending());
        assert!(d.force_flush().is_none());
    }
}
