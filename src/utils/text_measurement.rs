use log::{debug, info};

use gpui::{Pixels, SharedString, TextStyle, Window, px};
use gpui_component::PixelsExt;

/// 文本测量工具类，提供基于GPUI TextSystem的精确文本尺寸计算功能
pub struct TextMeasurement {
    default_font_size: Pixels,
}

impl TextMeasurement {
    /// 创建新的文本测量实例
    pub fn new() -> Self {
        Self {
            default_font_size: px(14.0),
        }
    }

    /// 创建带有自定义字体大小的文本测量实例
    pub fn with_font_size(font_size: Pixels) -> Self {
        Self {
            default_font_size: font_size,
        }
    }

    /// 计算完整消息在指定宽度下的高度
    /// 
    /// # 参数
    /// - `window`: GPUI窗口实例，用于访问文本系统
    /// - `content`: 要测量的文本内容
    /// - `width`: 文本容器的最大宽度
    /// - `font_size`: 可选的字体大小，默认使用工具类配置的大小
    /// 
    /// # 返回值
    /// 计算得到的完整消息高度（包含文本、头部、内边距和间距）
    pub fn calculate_text_height(
        &self,
        window: &Window,
        content: &str,
        width: Pixels,
        font_size: Option<Pixels>,
    ) -> Pixels {
        let actual_font_size = font_size.unwrap_or(self.default_font_size);
        
        // 创建文本样式并设置字体大小和换行属性
        let mut text_style = TextStyle::default();
        text_style.font_size = actual_font_size.into();
        text_style.white_space = gpui::WhiteSpace::Normal;
        
        // 创建TextRun
        let text = SharedString::new(content);
        let run = text_style.to_run(text.len());
        
        // 动态计算需要减去的内边距值
        let padding_to_subtract = if width < px(250.0) {
            px(60.0) // 宽度小于130px时，减去更大的内边距值
        } else if width < px(300.0) {
            px(40.0)
        } else {
            px(20.0)
        };
        
        // 使用GPUI的文本系统进行精确测量，减去计算得到的内边距
        let measured_width = width * 0.9 - padding_to_subtract;
        
        // 打印测量开始日志
        debug!("文本测量开始: 文本长度={}, 容器宽度={:.2}px, 测量宽度={:.2}px, 字体大小={:.2}px",
              content.len(), width.as_f32(), measured_width.as_f32(), actual_font_size.as_f32());
        
        let wrapped_lines = window.text_system().shape_text(
            text,
            actual_font_size,
            &[run],
            Some(measured_width),
            None
        ).expect("文本测量失败");
        
        // 计算总高度：遍历每个WrappedLine，累加其size方法返回的高度
        let line_height = text_style.line_height_in_pixels(window.rem_size());
        let mut total_height = px(0.0);
        for line in &wrapped_lines {
            total_height += line.size(line_height).height;
        }
        
        
        // 计算完整消息高度（包含所有布局元素）
        let outer_gap = px(4.);
        let header_height = px(20.);
        let gap_between_header_and_content = px(4.);
        let content_padding_top = px(12.);
        let content_padding_bottom = px(12.);
        
        let complete_message_height = outer_gap + header_height + gap_between_header_and_content + content_padding_top + total_height + content_padding_bottom;
        
        // 打印测量结束日志
        debug!("文本测量结束: 完整消息高度={:.2}px, 文本行数={}", 
              complete_message_height.as_f32(),
              wrapped_lines.len());
        
        complete_message_height
    }

   
}



