use crate::config::connection::DecoderConfig;
use bytes::BytesMut;
use log::debug;
use tokio_util::codec::{BytesCodec, Decoder, Encoder, LengthDelimitedCodec};

/// 扩展的解码器trait，支持强制刷新缓冲区
pub trait ExtendedDecoder: Decoder<Item = BytesMut, Error = std::io::Error> + Send + Sync {
    /// 强制刷新缓冲区，返回所有待处理数据
    fn force_flush(&mut self) -> Option<BytesMut>;
}

/// 原始数据解码器类型别名
pub type BytesDecoder = BytesCodec;

impl ExtendedDecoder for BytesDecoder {
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
    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "LineToBytesMutDecoder: 强制刷新缓冲区: {:?}, 长度: {}",
                String::from_utf8_lossy(&self.pending_data),
                self.pending_data.len()
            );
            Some(self.pending_data.split_to(self.pending_data.len()))
        } else {
            None
        }
    }
}

impl Decoder for LineToBytesMutDecoder {
    type Item = BytesMut;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 查找完整的行
        let search_start = 0;
        while let Some(newline_pos) = src[search_start..].iter().position(|&b| b == b'\n') {
            // 计算换行符在整个src中的位置
            let absolute_pos = search_start + newline_pos;

            // 提取完整的行（包括换行符）
            let mut line = src.split_to(absolute_pos + 1);

            // 移除行尾的\r（如果有）
            let line = if line.len() > 1 && line[line.len() - 2] == b'\r' {
                line.split_to(line.len() - 2) // 移除\r\n
            } else {
                line.split_to(line.len() - 1) // 移除\n
            };

            debug!(
                "LineToBytesMutDecoder: 解码出完整行: {:?}, 长度: {}",
                String::from_utf8_lossy(&line),
                line.len()
            );

            // 返回完整行
            return Ok(Some(line));
        }

        // 如果没有完整的行，检查是否有剩余数据
        if !src.is_empty() {
            // 将新数据添加到待处理数据中
            self.pending_data.extend_from_slice(src);
            src.clear();

            // 暂时不返回，等待可能的后续数据
            return Ok(None);
        }

        // 没有数据可返回
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
    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "LengthDelimitedToBytesMutDecoder: 强制刷新缓冲区: {:?}, 长度: {}",
                String::from_utf8_lossy(&self.pending_data),
                self.pending_data.len()
            );
            Some(self.pending_data.split_to(self.pending_data.len()))
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
            debug!("FixedLengthDecoder: 解码出帧, 长度: {}", frame.len());
            Ok(Some(frame))
        } else {
            Ok(None)
        }
    }
}

impl ExtendedDecoder for FixedLengthDecoder {
    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "FixedLengthDecoder: 强制刷新缓冲区, 长度: {}",
                self.pending_data.len()
            );
            Some(self.pending_data.split_to(self.pending_data.len()))
        } else {
            None
        }
    }
}

/// JSON 流式解码器
/// 基于 serde_json::StreamDeserializer, 支持无分隔符拼接的 JSON 流(如 {"1":"1"}{"1":"1"}),
/// 每解析出一个完整 JSON 值即切出一帧(保留该值的原始字节, 供上层展示)。
/// 数据不完整时等待更多数据; 连接断开/超时由 force_flush 兜底返回残留字节。
struct JsonDecoder {
    pending_data: BytesMut,
}

impl JsonDecoder {
    fn new() -> Self {
        Self {
            pending_data: BytesMut::new(),
        }
    }
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

        if self.pending_data.is_empty() {
            return Ok(None);
        }

        // 在内部作用域完成解析, 借用结束后再修改 pending_data
        // 解析目标为 serde_json::Value, 仅用于确定 JSON 值边界, 不关心具体内容
        let parse_result: Result<Option<usize>, std::io::Error> = {
            let mut stream = serde_json::Deserializer::from_slice(&self.pending_data)
                .into_iter::<serde_json::Value>();
            match stream.next() {
                Some(Ok(_)) => Ok(Some(stream.byte_offset())),
                // 数据不完整(如半个 JSON 值), 等待后续数据
                Some(Err(e)) if e.is_eof() => Ok(None),
                // 真正的语法错误
                Some(Err(e)) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )),
                None => Ok(None),
            }
        };

        match parse_result {
            Ok(Some(offset)) => {
                let frame = self.pending_data.split_to(offset);
                debug!(
                    "JsonDecoder: 解码出一帧, 长度: {}, 内容: {:?}",
                    frame.len(),
                    String::from_utf8_lossy(&frame)
                );
                Ok(Some(frame))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
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
    fn force_flush(&mut self) -> Option<BytesMut> {
        if !self.pending_data.is_empty() {
            debug!(
                "JsonDecoder: 强制刷新缓冲区, 长度: {}, 内容: {:?}",
                self.pending_data.len(),
                String::from_utf8_lossy(&self.pending_data)
            );
            Some(self.pending_data.split_to(self.pending_data.len()))
        } else {
            None
        }
    }
}
