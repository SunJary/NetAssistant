use tokio_util::codec::{BytesCodec, LengthDelimitedCodec, Decoder, Encoder};
use bytes::{BytesMut};
use crate::config::connection::{DecoderConfig};
use log::{debug};

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
                debug!("CodecFactory: 使用LengthDelimited解码器，配置: {:?}", config);
                let length_delimited = LengthDelimitedDecoder::builder()
                    .max_frame_length(config.max_frame_length)
                    .length_field_offset(config.length_field_offset.into())
                    .length_field_length(config.length_field_length.into())
                    .length_adjustment(config.length_adjustment.try_into().unwrap_or(0))
                    .new_codec();
                Box::new(LengthDelimitedToBytesMutDecoder::new(length_delimited))
            }
            DecoderConfig::Json => {
                debug!("CodecFactory: 使用JSON解码器（基于BytesCodec）");
                // 对于JSON，我们直接使用BytesCodec
                Box::new(BytesDecoder::new())
            }
        }
    }
    
    /// 根据配置创建相应的encoder，返回Box<dyn Encoder<BytesMut, Error = std::io::Error>>
    pub fn create_encoder(config: &DecoderConfig) -> Box<dyn Encoder<BytesMut, Error = std::io::Error> + Send + Sync> {
        match config {
            DecoderConfig::Bytes => {
                Box::new(BytesDecoder::new())
            }
            DecoderConfig::LineBased => {
                // 将LinesCodec包装成输入BytesMut的Encoder
                Box::new(LineToBytesMutEncoder::new())
            }
            DecoderConfig::LengthDelimited(_) => {
                // 使用BytesEncoder作为默认编码器
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
            debug!("LineToBytesMutDecoder: 强制刷新缓冲区: {:?}, 长度: {}", String::from_utf8_lossy(&self.pending_data), self.pending_data.len());
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
            
            debug!("LineToBytesMutDecoder: 解码出完整行: {:?}, 长度: {}", String::from_utf8_lossy(&line), line.len());
            
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
            debug!("LineToBytesMutDecoder: decode_eof 返回剩余数据: {:?}, 长度: {}", String::from_utf8_lossy(&remaining), remaining.len());
            Ok(Some(remaining))
        } else if !self.pending_data.is_empty() {
            // 返回待处理数据
            debug!("LineToBytesMutDecoder: decode_eof 返回待处理数据: {:?}, 长度: {}", String::from_utf8_lossy(&self.pending_data), self.pending_data.len());
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
            debug!("LengthDelimitedToBytesMutDecoder: 强制刷新缓冲区: {:?}, 长度: {}", String::from_utf8_lossy(&self.pending_data), self.pending_data.len());
            Some(self.pending_data.split_to(self.pending_data.len()))
        } else {
            None
        }
    }
}


