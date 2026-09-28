//! 行解析(纯逻辑 + 单测)。
//!
//! 文本 → 任务项列表; 错误在创建任务前反馈, 不产生半成品任务。
//! 空行一律跳过(无配置开关): 0 字节消息在 TCP 上无任何可见效果,
//! UDP 上虽能发出 0 长度数据报但无实际意义。
//! 行尾 `\r` 被剥离(兼容 CRLF); 行尾符由连接级 `send_trailer` 决定是否补。

use super::model::TaskItem;
use crate::utils::message_vars::CompiledTemplate;
use std::sync::Arc;

/// 任务项上限(文件大小上限沿用既有 1 MiB)
pub const MAX_TASK_ITEMS: usize = 50_000;

/// 非法行最多反馈的条数(UI 列出前 N 条即可)
pub const MAX_REPORTED_INVALID_LINES: usize = 50;

/// 行解析错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineParseError {
    /// 无有效行(全为空行)
    Empty,
    /// 超过任务项上限
    TooMany { count: usize, max: usize },
    /// hex 模式下存在非法行(1-based 行号, 原文)
    InvalidHexLines(Vec<(usize, String)>),
}

impl std::fmt::Display for LineParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineParseError::Empty => write!(f, "no valid lines"),
            LineParseError::TooMany { count, max } => {
                write!(f, "too many lines: {} > {}", count, max)
            }
            LineParseError::InvalidHexLines(_) => write!(f, "invalid hex lines"),
        }
    }
}

/// 文本 → 任务项列表。
///
/// - 按 `\n` 切分, 去掉行尾 `\r`(兼容 CRLF)
/// - 空行恒定跳过
/// - hex 模式逐行用 `validate_hex_input` 校验(支持 `${...}` 变量占位符)
/// - 每行预编译模板(含 `${seq}` 时每轮 `fetch_add`, 与周期发送语义一致)
pub fn parse_lines(text: &str, hex_mode: bool) -> Result<Vec<TaskItem>, LineParseError> {
    let mut items: Vec<TaskItem> = Vec::new();
    let mut invalid: Vec<(usize, String)> = Vec::new();

    for (idx, raw_line) in text.split('\n').enumerate() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        if hex_mode && !crate::utils::hex::validate_hex_input(line) {
            if invalid.len() < MAX_REPORTED_INVALID_LINES {
                invalid.push((idx + 1, line.to_string()));
            }
            continue;
        }
        items.push(TaskItem {
            index: items.len(),
            raw: line.to_string(),
            compiled: Arc::new(CompiledTemplate::new(line)),
        });
        if items.len() > MAX_TASK_ITEMS {
            return Err(LineParseError::TooMany {
                count: items.len(),
                max: MAX_TASK_ITEMS,
            });
        }
    }

    if !invalid.is_empty() {
        return Err(LineParseError::InvalidHexLines(invalid));
    }
    if items.is_empty() {
        return Err(LineParseError::Empty);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// CRLF 兼容: 行尾 \r 被剥离, 不进入消息内容
    fn test_crlf_stripped() {
        let items = parse_lines("AT\r\nATI\r\n", false).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].raw, "AT");
        assert_eq!(items[1].raw, "ATI");
    }

    #[test]
    /// 空行恒定跳过; 全为空行报 Empty
    fn test_empty_lines_skipped() {
        let items = parse_lines("a\n\n\nb\n", false).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].raw, "a");
        assert_eq!(items[1].raw, "b");
        assert!(matches!(
            parse_lines("\n\n\r\n", false),
            Err(LineParseError::Empty)
        ));
        assert!(matches!(parse_lines("", false), Err(LineParseError::Empty)));
    }

    #[test]
    /// index 连续、从 0 开始(空行被跳过后仍连续)
    fn test_index_sequential() {
        let items = parse_lines("a\n\nb\nc", false).unwrap();
        assert_eq!(
            items.iter().map(|i| i.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    /// hex 模式: 合法行通过, 奇数长度/非法字符行报错并带 1-based 行号
    fn test_hex_invalid_lines() {
        let err = parse_lines("41 42\n48656c6c6\n41G2\n43", true).unwrap_err();
        match err {
            LineParseError::InvalidHexLines(lines) => {
                assert_eq!(lines.len(), 2);
                assert_eq!(lines[0].0, 2);
                assert_eq!(lines[0].1, "48656c6c6");
                assert_eq!(lines[1].0, 3);
                assert_eq!(lines[1].1, "41G2");
            }
            other => panic!("expected InvalidHexLines, got {:?}", other),
        }
    }

    #[test]
    /// hex 模式含 ${...} 变量的行视为合法(长度运行时确定)
    fn test_hex_with_variables_ok() {
        let items = parse_lines("50494E47${seq}\n${uuid}", true).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    /// 文本模式不做 hex 校验, 任意内容都合法
    fn test_text_mode_accepts_anything() {
        let items = parse_lines("hello world\n中文 zzz", false).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    /// 超过上限报 TooMany
    fn test_too_many_items() {
        let text = (0..(MAX_TASK_ITEMS + 1))
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(matches!(
            parse_lines(&text, false),
            Err(LineParseError::TooMany { .. })
        ));
    }

    #[test]
    /// 每行预编译模板: needs_seq 与内容一致
    fn test_compiled_template_per_line() {
        let items = parse_lines("req-${seq}\nplain", false).unwrap();
        assert!(items[0].compiled.needs_seq());
        assert!(!items[1].compiled.needs_seq());
    }
}
