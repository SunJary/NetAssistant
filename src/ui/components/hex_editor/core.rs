//! 十六进制编辑器 —— 纯逻辑核心层
//!
//! 本模块不依赖 gpui / gpui_component / 项目内其他类型，只做数据变换，
//! 可独立无头单测；未来若提取为独立 crate，本文件可原样搬出。
//! 设计与布局规格见 plans/plan-hex-editor.md。

/// 单元格上限：超过后编辑器进入只读展示，防止超长内容拖垮 UI。
/// 值字符串本身不受影响（网络层仍读完整内容），界面会提示改用文本模式处理。
pub const MAX_CELLS: usize = 4096;

/// 半字节占位：`Cell::Byte.lo == HALF_EMPTY` 表示该字节只有高 4 位（半字节）。
/// 出现在奇数长度 hex 段（如 `50494E4${seq}`）或 insert 模式新输入中，
/// 序列化时只输出高位字符；运行时与变量输出拼接后由 hex_to_bytes 顺序解析。
pub const HALF_EMPTY: char = '\0';

/// 网格单元格。字节保留用户输入的大小写，未编辑的字符不做改写。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Byte { hi: char, lo: char },
    /// 不透明 token 占位（core 不解释内容，如项目侧的 `${seq}`），保存原文
    Token(String),
}

impl Cell {
    /// 光标可停留的 nibble 位置数：字节恒为 2（半字节的低位是可填写的空槽），token 为 1
    fn cursor_positions(&self) -> usize {
        match self {
            Cell::Byte { .. } => 2,
            Cell::Token(_) => 1,
        }
    }

    pub fn is_token(&self) -> bool {
        matches!(self, Cell::Token(_))
    }

    pub fn to_ascii_char(&self) -> Option<char> {
        match self {
            Cell::Byte { hi, lo } => {
                let hi = hi.to_digit(16)?;
                let lo = lo.to_digit(16)?;
                let b = (hi << 4) | lo;
                char::from_u32(b).filter(|c| c.is_ascii_graphic() || *c == ' ')
            }
            Cell::Token(_) => None,
        }
    }
}

/// 光标：单元格索引 + nibble 位置（0=高 4 位，1=低 4 位）。
/// `cell == cells.len()` 表示"内容末尾"的虚拟位置（继续键入会追加）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub cell: usize,
    pub nibble: usize,
}

/// 解析结果
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HexDoc {
    pub cells: Vec<Cell>,
    /// 超过 MAX_CELLS 被截断，编辑器只读展示
    pub truncated: bool,
}

/// 编辑器状态（纯数据）。`doc == None` 表示输入串解析失败（应回退文本渲染）。
#[derive(Debug, Clone, Default)]
pub struct State {
    pub doc: Option<HexDoc>,
    pub cursor: Option<Cursor>,
    /// 选区 [start, end) 单元格索引
    pub selection: Option<(usize, usize)>,
    /// 拖选起点（extend 移动时基于它扩展选区）
    pub selection_anchor: Option<usize>,
    /// false = 覆盖模式（默认），true = 插入模式
    pub insert_mode: bool,
}

impl State {
    /// 从输入串构建状态。解析失败时 doc 为 None。
    pub fn from_value(value: &str) -> Self {
        Self {
            doc: parse(value).ok(),
            ..Default::default()
        }
    }

    pub fn len(&self) -> Option<usize> {
        Some(self.doc.as_ref()?.cells.len())
    }

    fn cells(&self) -> Option<&Vec<Cell>> {
        Some(&self.doc.as_ref()?.cells)
    }

    fn cells_mut(&mut self) -> Option<&mut Vec<Cell>> {
        Some(&mut self.doc.as_mut()?.cells)
    }

    fn readonly(&self) -> bool {
        self.doc.as_ref().map(|d| d.truncated).unwrap_or(false)
    }

    /// 应用动作。返回 Some(新值字符串) 表示内容变化需要写回，None 表示仅状态变化。
    pub fn apply(&mut self, action: Action) -> Option<String> {
        match action {
            Action::Digit(c) => self.type_digit(c),
            Action::Ascii(c) => self.type_ascii(c),
            Action::Backspace => self.backspace(),
            Action::Delete => self.delete_at_cursor(),
            Action::InsertToggle => {
                self.insert_mode = !self.insert_mode;
                None
            }
            Action::Move { dir, extend } => {
                self.move_cursor(dir, extend);
                None
            }
            Action::Click { cell, nibble } => {
                self.click(cell, nibble);
                None
            }
            Action::DragTo(i) => {
                self.drag_to(i);
                None
            }
            Action::SelectAll => {
                self.select_all();
                None
            }
            Action::Clear => self.clear(),
            Action::Paste(cells) => self.paste(cells),
        }
    }

    /// 当前光标。无光标(从未点击/刚切换)时视作定位到首字节——与界面默认
    /// 高亮的首字节光标一致, 键入/导航从可见位置开始; 越界时钳制。
    fn resolved_cursor(&self) -> Option<Cursor> {
        let len = self.len()?;
        if len == 0 {
            return Some(Cursor { cell: 0, nibble: 0 });
        }
        Some(match self.cursor {
            Some(c) if c.cell < len => {
                let max = self.cells()?[c.cell].cursor_positions() - 1;
                Cursor { cell: c.cell, nibble: c.nibble.min(max) }
            }
            Some(_) => Cursor { cell: len, nibble: 0 }, // 显式虚拟末尾(导航可达)
            None => Cursor { cell: 0, nibble: 0 },      // 从未定位 → 首字节
        })
    }

    fn clamp_cursor(&mut self) {
        self.cursor = self.resolved_cursor();
    }

    /// 从某索引起向后找第一个字节单元格（键入数字时跳过 token）
    fn next_byte_cell(&self, from: usize) -> Option<usize> {
        self.cells()?
            .iter()
            .enumerate()
            .skip(from + 1)
            .find(|(_, c)| !c.is_token())
            .map(|(i, _)| i)
    }

    /// nibble 前进一格：字节内 0→1，1→下一格（末尾则到虚拟末尾）。
    /// token 只有 1 格，前进即离开。返回 None 表示已在虚拟末尾之外（不应发生）。
    fn advance(&self, cur: Cursor) -> Option<Cursor> {
        let cells = self.cells()?;
        let positions = cells.get(cur.cell)?.cursor_positions();
        if cur.nibble + 1 < positions {
            return Some(Cursor { cell: cur.cell, nibble: cur.nibble + 1 });
        }
        Some(Cursor { cell: cur.cell + 1, nibble: 0 })
    }

    fn type_digit(&mut self, c: char) -> Option<String> {
        if self.readonly() || !c.is_ascii_hexdigit() {
            return None;
        }
        let len = self.len()?;
        let mut cur = self.resolved_cursor()?;
        // token 上键入：跳到其后第一个字节单元格；其后无字节则视作末尾追加
        if cur.cell < len && self.cells()?[cur.cell].is_token() {
            cur = match self.next_byte_cell(cur.cell) {
                Some(i) => Cursor { cell: i, nibble: 0 },
                None => Cursor { cell: len, nibble: 0 },
            };
        }
        let d = c.to_ascii_lowercase();
        // 虚拟末尾：键入即追加（覆盖模式同样行为，与文本框"末尾输入即追加"直觉一致）
        if cur.cell >= len {
            self.cells_mut()?.push(Cell::Byte { hi: d, lo: HALF_EMPTY });
            self.cursor = Some(Cursor { cell: len, nibble: 1 });
            self.clear_selection();
            return Some(self.serialize_doc());
        }
        if self.insert_mode {
            // 插入模式：半字节空槽上键入 → 补全低位；否则新建半字节单元格，
            // 下一次键入补全（连续键入即逐字节追加）
            let is_half = matches!(self.cells()?[cur.cell], Cell::Byte { lo, .. } if lo == HALF_EMPTY);
            if is_half && cur.nibble == 1 {
                if let Cell::Byte { lo, .. } = &mut self.cells_mut()?[cur.cell] {
                    *lo = d;
                }
                self.cursor = self.advance(cur);
            } else {
                let insert_at = if cur.nibble == 0 { cur.cell } else { cur.cell + 1 };
                self.cells_mut()?.insert(insert_at, Cell::Byte { hi: d, lo: HALF_EMPTY });
                self.cursor = Some(Cursor { cell: insert_at, nibble: 1 });
            }
        } else {
            match &mut self.cells_mut()?[cur.cell] {
                Cell::Byte { hi, lo } => {
                    if cur.nibble == 0 { *hi = d } else { *lo = d }
                }
                Cell::Token(_) => return None,
            }
            self.cursor = self.advance(cur);
        }
        self.clear_selection();
        Some(self.serialize_doc())
    }

    fn type_ascii(&mut self, c: char) -> Option<String> {
        if self.readonly() || !(0x20..=0x7e).contains(&(c as u32)) {
            return None;
        }
        let len = self.len()?;
        let mut cur = self.resolved_cursor()?;
        if cur.cell >= len {
            // 末尾追加完整字节
            let [hi, lo] = char_to_hex_chars(c);
            self.cells_mut()?.push(Cell::Byte { hi, lo });
            self.cursor = Some(Cursor { cell: len + 1, nibble: 0 });
            self.clear_selection();
            return Some(self.serialize_doc());
        }
        if self.cells()?[cur.cell].is_token() {
            cur = Cursor { cell: self.next_byte_cell(cur.cell)?, nibble: 0 };
        }
        let [hi, lo] = char_to_hex_chars(c);
        if self.insert_mode {
            self.cells_mut()?.insert(cur.cell, Cell::Byte { hi, lo });
            self.cursor = Some(Cursor { cell: cur.cell + 1, nibble: 0 });
        } else {
            self.cells_mut()?[cur.cell] = Cell::Byte { hi, lo };
            self.cursor = self.advance(cur);
        }
        self.clear_selection();
        Some(self.serialize_doc())
    }

    fn backspace(&mut self) -> Option<String> {
        if self.readonly() {
            return None;
        }
        if self.delete_selection().is_some() {
            return Some(self.serialize_doc());
        }
        let cur = self.resolved_cursor()?;
        if cur.cell == 0 {
            return None;
        }
        self.cells_mut()?.remove(cur.cell - 1);
        self.cursor = Some(Cursor { cell: cur.cell - 1, nibble: 0 });
        Some(self.serialize_doc())
    }

    fn delete_at_cursor(&mut self) -> Option<String> {
        if self.readonly() {
            return None;
        }
        if self.delete_selection().is_some() {
            return Some(self.serialize_doc());
        }
        let len = self.len()?;
        let cur = self.resolved_cursor()?;
        if cur.cell >= len {
            return None;
        }
        self.cells_mut()?.remove(cur.cell);
        self.clamp_cursor();
        Some(self.serialize_doc())
    }

    fn delete_selection(&mut self) -> Option<()> {
        let (s, e) = self.selection?;
        let len = self.len()?;
        let (s, e) = (s.min(len), e.min(len));
        if s >= e {
            self.clear_selection();
            return None;
        }
        self.cells_mut()?.drain(s..e);
        self.cursor = Some(Cursor { cell: s.min(len - (e - s)), nibble: 0 });
        self.clear_selection();
        Some(())
    }

    fn move_cursor(&mut self, dir: MoveDir, extend: bool) {
        let Some(len) = self.len() else { return };
        let from = self.resolved_cursor().unwrap_or(Cursor { cell: 0, nibble: 0 });
        let next = match dir {
            MoveDir::Left => {
                if from.nibble > 0 {
                    Cursor { cell: from.cell, nibble: from.nibble - 1 }
                } else if from.cell > 0 {
                    let max = self
                        .cells()
                        .and_then(|c| c.get(from.cell - 1))
                        .map(|c| c.cursor_positions() - 1)
                        .unwrap_or(0);
                    Cursor { cell: from.cell - 1, nibble: max }
                } else {
                    from
                }
            }
            MoveDir::Right => self.advance(from).unwrap_or(from),
            MoveDir::Up { stride } if stride > 0 => {
                Cursor { cell: from.cell.saturating_sub(stride), nibble: 0 }
            }
            MoveDir::Down { stride } if stride > 0 => {
                Cursor { cell: (from.cell + stride).min(len), nibble: 0 }
            }
            MoveDir::Home { stride } if stride > 0 => {
                Cursor { cell: from.cell / stride * stride, nibble: 0 }
            }
            MoveDir::End { stride } if stride > 0 => {
                let row_end = ((from.cell / stride + 1) * stride).min(len);
                Cursor { cell: row_end, nibble: 0 }
            }
            // stride 为 0 属于调用方错误，忽略移动
            _ => from,
        };
        let next = Cursor { cell: next.cell.min(len), nibble: next.nibble };
        if extend {
            let anchor = *self.selection_anchor.get_or_insert(from.cell);
            let (s, e) = ordered_range(anchor, next.cell);
            self.selection = Some((s, e));
        } else {
            self.clear_selection();
        }
        self.cursor = Some(next);
    }

    fn click(&mut self, index: usize, nibble: usize) {
        let Some(len) = self.len() else { return };
        if len == 0 {
            self.cursor = Some(Cursor { cell: 0, nibble: 0 });
            self.selection_anchor = Some(0);
            self.selection = None;
            return;
        }
        // 空槽/内容末尾空位(index >= len)定位到虚拟末尾——键入即追加, 与文本框点击行尾一致
        let i = index.min(len);
        self.cursor = Some(Cursor { cell: i, nibble });
        self.selection_anchor = Some(i);
        self.selection = None;
    }

    fn drag_to(&mut self, index: usize) {
        let Some(len) = self.len() else { return };
        let i = index.min(len.saturating_sub(1));
        let anchor = self
            .selection_anchor
            .unwrap_or_else(|| self.cursor.map(|c| c.cell).unwrap_or(i));
        let (s, e) = ordered_range(anchor, i);
        self.selection = Some((s, e));
        self.cursor = Some(Cursor { cell: i, nibble: 0 });
    }

    fn select_all(&mut self) {
        let Some(len) = self.len() else { return };
        if len == 0 {
            return;
        }
        self.selection = Some((0, len));
        self.selection_anchor = Some(0);
        self.cursor = Some(Cursor { cell: 0, nibble: 0 });
    }

    fn clear(&mut self) -> Option<String> {
        if self.readonly() {
            return None;
        }
        let empty = self.len()? == 0;
        if let Some(doc) = self.doc.as_mut() {
            doc.cells.clear();
            doc.truncated = false;
        }
        self.cursor = None;
        self.clear_selection();
        if empty {
            None
        } else {
            Some(String::new())
        }
    }

    fn paste(&mut self, cells: Vec<Cell>) -> Option<String> {
        if self.readonly() || cells.is_empty() {
            return None;
        }
        let len = self.len()?;
        let (s, e) = match self.selection {
            Some((s, e)) => {
                let e = e.min(len);
                (s.min(e), e)
            }
            None => {
                let at = self.cursor.map(|c| c.cell.min(len)).unwrap_or(len);
                (at, at)
            }
        };
        let cells = if cells.len() > MAX_CELLS { cells[..MAX_CELLS].to_vec() } else { cells };
        let inserted = cells.len();
        let new_len = len - (e - s) + inserted;
        self.cells_mut()?.splice(s..e, cells);
        self.cursor = Some(Cursor { cell: (s + inserted).min(new_len), nibble: 0 });
        self.clear_selection();
        Some(self.serialize_doc())
    }

    fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_anchor = None;
    }

    fn serialize_doc(&self) -> String {
        match self.doc.as_ref() {
            Some(doc) => serialize(&doc.cells),
            None => String::new(),
        }
    }

    /// 复制当前选区（无选区时返回 None）
    pub fn selection_value(&self) -> Option<String> {
        let (s, e) = self.selection?;
        let cells = self.cells()?;
        let e = e.min(cells.len());
        Some(serialize(&cells[s.min(e)..e]))
    }

    /// 复制全部内容的 hex 串
    pub fn full_value(&self) -> String {
        self.serialize_doc()
    }

    /// (完整字节数, 半字节数, token 数)
    pub fn counts(&self) -> (usize, usize, usize) {
        let Some(doc) = self.doc.as_ref() else { return (0, 0, 0) };
        let mut full = 0;
        let mut half = 0;
        let mut tokens = 0;
        for cell in &doc.cells {
            match cell {
                Cell::Byte { lo, .. } if *lo == HALF_EMPTY => half += 1,
                Cell::Byte { .. } => full += 1,
                Cell::Token(_) => tokens += 1,
            }
        }
        (full, half, tokens)
    }

    /// 光标处字节偏移（状态栏展示用估算值）：
    /// 每个单元格（字节/半字节/token）按 1 字节计——token 的运行时输出长度
    /// 无法在编辑期确定，按常见情形（如 ${seq} 输出两位 hex）估为 1 字节。
    pub fn cursor_offset(&self) -> Option<usize> {
        self.cursor.map(|c| c.cell)
    }

    pub fn selection_len(&self) -> usize {
        self.selection.map(|(s, e)| e.saturating_sub(s)).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MoveDir {
    Left,
    Right,
    Up { stride: usize },
    Down { stride: usize },
    Home { stride: usize },
    End { stride: usize },
}

#[derive(Debug, Clone)]
pub enum Action {
    /// 键入一位 hex 数字
    Digit(char),
    /// 键入可打印 ASCII 字符（仅展开编辑器启用）：直接设置当前字节
    Ascii(char),
    Backspace,
    Delete,
    InsertToggle,
    Move { dir: MoveDir, extend: bool },
    /// 鼠标点击/定位到单元格（含 nibble 精度）
    Click { cell: usize, nibble: usize },
    /// 拖选经过单元格
    DragTo(usize),
    SelectAll,
    Clear,
    /// 粘贴：已由 parse_tolerant 转为字节单元格序列
    Paste(Vec<Cell>),
}

fn ordered_range(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b + 1) } else { (b, a + 1) }
}

fn char_to_hex_chars(c: char) -> [char; 2] {
    let mut buf = [0u8; 4];
    let s = c.encode_utf8(&mut buf);
    let byte = s.as_bytes()[0];
    let s = format!("{:02x}", byte);
    let mut chars = s.chars();
    [chars.next().unwrap(), chars.next().unwrap()]
}

/// 解析输入串为文档。失败时返回 Err(非法字符的字节索引)。
///
/// 规则（与 `crate::utils::hex::validate_hex_input` 语义对齐）：
/// - 空白字符跳过；连续 hex 字符两位一组构成字节，末尾落单为半字节
/// - `${...}` 视为 token（未闭合时消费到串尾，与 strip_variables 一致）
/// - 其余字符非法
pub fn parse(value: &str) -> Result<HexDoc, usize> {
    let mut cells: Vec<Cell> = Vec::new();
    let mut pending: Option<char> = None;
    let mut chars = value.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {}
            '$' => {
                if chars.peek().map(|(_, c)| *c) != Some('{') {
                    return Err(idx);
                }
                chars.next();
                let mut token = String::from("${");
                for (_, c) in chars.by_ref() {
                    token.push(c);
                    if c == '}' {
                        break;
                    }
                }
                // 奇数 hex 段在此收尾为半字节（如 `50494E4${seq}` 的 '4'）
                if let Some(hi) = pending.take() {
                    cells.push(Cell::Byte { hi, lo: HALF_EMPTY });
                }
                cells.push(Cell::Token(token));
            }
            c if c.is_ascii_hexdigit() => match pending.take() {
                None => pending = Some(c),
                Some(hi) => cells.push(Cell::Byte { hi, lo: c }),
            },
            _ => return Err(idx),
        }
    }
    if let Some(hi) = pending.take() {
        cells.push(Cell::Byte { hi, lo: HALF_EMPTY });
    }
    let truncated = cells.len() > MAX_CELLS;
    if truncated {
        cells.truncate(MAX_CELLS);
    }
    Ok(HexDoc { cells, truncated })
}

/// 序列化为两位一组、空格分隔的 hex 串（token 原样保留）。
/// `hex_to_bytes` 对空白不敏感，序列化结果与其兼容。
pub fn serialize(cells: &[Cell]) -> String {
    let mut out = String::with_capacity(cells.len() * 3);
    for cell in cells {
        if !out.is_empty() {
            out.push(' ');
        }
        match cell {
            Cell::Byte { hi, lo } => {
                out.push(*hi);
                if *lo != HALF_EMPTY {
                    out.push(*lo);
                }
            }
            Cell::Token(token) => out.push_str(token),
        }
    }
    out
}

/// 粘贴容错解析：剥离空格/逗号/分号/冒号/引号/下划线/连字符与 `0x`、`\x` 前缀，
/// 收集 hex 字符；其余字符报 Err(索引)。
pub fn parse_tolerant(text: &str) -> Result<Vec<Cell>, usize> {
    let mut chars_iter = text.char_indices().peekable();
    let mut digits: Vec<char> = Vec::new();
    while let Some((idx, ch)) = chars_iter.next() {
        match ch {
            ' ' | '\t' | '\n' | '\r' | ',' | ';' | ':' | '_' | '"' | '\'' | '-' => {}
            '0' if matches!(chars_iter.peek().map(|(_, c)| c), Some('x') | Some('X')) => {
                chars_iter.next();
            }
            '\\' if matches!(chars_iter.peek().map(|(_, c)| c), Some('x') | Some('X')) => {
                chars_iter.next();
            }
            c if c.is_ascii_hexdigit() => digits.push(c),
            _ => return Err(idx),
        }
    }
    let mut cells = Vec::with_capacity(digits.len().div_ceil(2));
    let mut iter = digits.into_iter();
    while let Some(hi) = iter.next() {
        match iter.next() {
            Some(lo) => cells.push(Cell::Byte { hi, lo }),
            None => cells.push(Cell::Byte { hi, lo: HALF_EMPTY }),
        }
    }
    Ok(cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(value: &str) -> HexDoc {
        parse(value).expect("parse ok")
    }

    fn state(value: &str) -> State {
        let s = State::from_value(value);
        assert!(s.doc.is_some(), "parse failed: {value}");
        s
    }

    #[test]
    fn parse_plain_bytes() {
        let d = doc("48656c6c6f");
        assert_eq!(d.cells.len(), 5);
        assert_eq!(d.cells[0], Cell::Byte { hi: '4', lo: '8' });
        assert!(!d.truncated);
    }

    #[test]
    fn parse_with_spaces_and_case() {
        let d = doc("50 49 4E 47");
        assert_eq!(d.cells.len(), 4);
        assert_eq!(d.cells[2], Cell::Byte { hi: '4', lo: 'E' });
    }

    #[test]
    fn parse_token_roundtrip() {
        let d = doc("50494E47${seq}");
        assert_eq!(d.cells.len(), 5);
        assert_eq!(d.cells[4], Cell::Token("${seq}".to_string()));
        assert_eq!(serialize(&d.cells), "50 49 4E 47 ${seq}");
    }

    #[test]
    fn parse_odd_segment_before_token() {
        // 奇数 hex 段 + token：合法（变量输出长度运行时确定）
        let d = doc("50494E4${seq}");
        assert_eq!(d.cells.len(), 5);
        assert_eq!(d.cells[3], Cell::Byte { hi: '4', lo: HALF_EMPTY });
    }

    #[test]
    fn parse_unclosed_token_consumes_rest() {
        let d = doc("${seq");
        assert_eq!(d.cells, vec![Cell::Token("${seq".to_string())]);
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse("48g5"), Err(2));
        assert_eq!(parse("48$"), Err(2));
        assert_eq!(parse("48{5}"), Err(2));
    }

    #[test]
    fn serialize_empty() {
        assert_eq!(serialize(&[]), "");
        assert_eq!(serialize(&doc("").cells), "");
    }

    #[test]
    fn type_digit_overwrite_advances() {
        let mut s = state("48 65");
        s.cursor = Some(Cursor { cell: 0, nibble: 0 });
        assert_eq!(s.apply(Action::Digit('A')), Some("a8 65".into()));
        assert_eq!(s.cursor, Some(Cursor { cell: 0, nibble: 1 }));
        assert_eq!(s.apply(Action::Digit('B')), Some("ab 65".into()));
        assert_eq!(s.cursor, Some(Cursor { cell: 1, nibble: 0 }));
    }

    #[test]
    fn type_digit_appends_at_end() {
        let mut s = state("48");
        s.cursor = Some(Cursor { cell: 1, nibble: 1 });
        assert_eq!(s.apply(Action::Digit('f')), Some("48 f".into()));
        assert_eq!(s.apply(Action::Digit('0')), Some("48 f0".into()));
        // 虚拟末尾继续键入 → 再追加
        assert_eq!(s.apply(Action::Digit('1')), Some("48 f0 1".into()));
    }

    #[test]
    fn type_digit_skips_token() {
        let mut s = state("50 49 4E 47 ${seq}");
        s.cursor = Some(Cursor { cell: 4, nibble: 0 });
        // token 上键入 → 跳到 token 后追加
        assert_eq!(s.apply(Action::Digit('a')), Some("50 49 4E 47 ${seq} a".into()));
    }

    #[test]
    fn type_digit_insert_mode() {
        let mut s = state("48 65");
        s.cursor = Some(Cursor { cell: 0, nibble: 0 });
        s.apply(Action::InsertToggle);
        assert_eq!(s.apply(Action::Digit('1')), Some("1 48 65".into()));
        assert_eq!(s.apply(Action::Digit('2')), Some("12 48 65".into()));
    }

    #[test]
    fn backspace_deletes_previous_cell() {
        let mut s = state("48 65 6C");
        s.cursor = Some(Cursor { cell: 2, nibble: 0 });
        assert_eq!(s.apply(Action::Backspace), Some("48 6C".into()));
        assert_eq!(s.cursor, Some(Cursor { cell: 1, nibble: 0 }));
        // 行首无效
        assert_eq!(s.apply(Action::Backspace), Some("6C".into()));
        assert_eq!(s.cursor, Some(Cursor { cell: 0, nibble: 0 }));
        assert_eq!(s.apply(Action::Backspace), None);
    }

    #[test]
    fn delete_removes_current_cell() {
        let mut s = state("48 65 6C");
        s.cursor = Some(Cursor { cell: 1, nibble: 0 });
        assert_eq!(s.apply(Action::Delete), Some("48 6C".into()));
        assert_eq!(s.cursor, Some(Cursor { cell: 1, nibble: 0 }));
    }

    #[test]
    fn delete_token_as_whole() {
        let mut s = state("50 ${seq} 2A");
        s.cursor = Some(Cursor { cell: 1, nibble: 0 });
        assert_eq!(s.apply(Action::Delete), Some("50 2A".into()));
    }

    #[test]
    fn navigation_with_stride() {
        let mut s = state("00 01 02 03 04 05 06 07 08");
        s.cursor = Some(Cursor { cell: 4, nibble: 1 });
        s.apply(Action::Move { dir: MoveDir::Up { stride: 8 }, extend: false });
        assert_eq!(s.cursor, Some(Cursor { cell: 0, nibble: 0 }));
        s.apply(Action::Move { dir: MoveDir::Down { stride: 8 }, extend: false });
        assert_eq!(s.cursor, Some(Cursor { cell: 8, nibble: 0 }));
        s.apply(Action::Move { dir: MoveDir::Left, extend: false });
        assert_eq!(s.cursor, Some(Cursor { cell: 7, nibble: 1 }));
        s.apply(Action::Move { dir: MoveDir::Home { stride: 8 }, extend: false });
        assert_eq!(s.cursor, Some(Cursor { cell: 0, nibble: 0 }));
        s.apply(Action::Move { dir: MoveDir::End { stride: 8 }, extend: false });
        assert_eq!(s.cursor, Some(Cursor { cell: 8, nibble: 0 }));
    }

    #[test]
    fn selection_and_shift_extend() {
        let mut s = state("00 01 02 03");
        s.cursor = Some(Cursor { cell: 0, nibble: 0 });
        s.apply(Action::Move { dir: MoveDir::Right, extend: true });
        s.apply(Action::Move { dir: MoveDir::Right, extend: true });
        assert_eq!(s.selection, Some((0, 2)));
        assert_eq!(s.selection_value(), Some("00 01".into()));
        // 删除选区
        assert_eq!(s.apply(Action::Delete), Some("02 03".into()));
        assert_eq!(s.selection, None);
    }

    #[test]
    fn drag_selection() {
        let mut s = state("00 01 02 03");
        s.apply(Action::Click { cell: 1, nibble: 0 });
        assert_eq!(s.selection_anchor, Some(1));
        s.apply(Action::DragTo(3));
        assert_eq!(s.selection, Some((1, 4)));
    }

    #[test]
    fn click_past_end_lands_virtual_end() {
        let mut s = state("48 65");
        // 点击内容末尾空位(cell == len) → 定位到虚拟末尾, 键入即追加
        s.apply(Action::Click { cell: 2, nibble: 0 });
        assert_eq!(s.cursor, Some(Cursor { cell: 2, nibble: 0 }));
        assert_eq!(s.apply(Action::Digit('a')), Some("48 65 a".into()));
        // 更远的空位同样钳制到虚拟末尾
        s.apply(Action::Click { cell: 9, nibble: 0 });
        assert_eq!(s.cursor, Some(Cursor { cell: 3, nibble: 0 }));
    }

    #[test]
    fn paste_replaces_selection() {
        let mut s = state("00 01 02 03");
        s.selection = Some((1, 3));
        let cells = parse_tolerant("aa bb cc").unwrap();
        assert_eq!(s.apply(Action::Paste(cells)), Some("00 aa bb cc 03".into()));
    }

    #[test]
    fn paste_tolerant_formats() {
        assert_eq!(
            parse_tolerant("0x48, 0x65\n\\x6C;7-6:_\"F\"").unwrap(),
            vec![
                Cell::Byte { hi: '4', lo: '8' },
                Cell::Byte { hi: '6', lo: '5' },
                Cell::Byte { hi: '6', lo: 'C' },
                Cell::Byte { hi: '7', lo: '6' },
                Cell::Byte { hi: 'F', lo: HALF_EMPTY },
            ]
        );
        assert_eq!(parse_tolerant("zz"), Err(0));
        // 空内容 → 空
        assert_eq!(parse_tolerant("  , ; ").unwrap(), vec![]);
    }

    #[test]
    fn clear_resets() {
        let mut s = state("48 65");
        assert_eq!(s.apply(Action::Clear), Some(String::new()));
        assert_eq!(s.len(), Some(0));
        assert_eq!(s.apply(Action::Clear), None);
    }

    #[test]
    fn truncated_is_readonly() {
        let mut s = State {
            doc: Some(HexDoc { cells: vec![Cell::Byte { hi: '4', lo: '8' }], truncated: true }),
            ..Default::default()
        };
        s.cursor = Some(Cursor { cell: 0, nibble: 0 });
        assert_eq!(s.apply(Action::Digit('a')), None);
        assert_eq!(s.apply(Action::Backspace), None);
        assert_eq!(s.apply(Action::Paste(parse_tolerant("11").unwrap())), None);
        // 导航仍可用（用于复制）
        s.apply(Action::Move { dir: MoveDir::Right, extend: true });
        assert_eq!(s.selection, Some((0, 1)));
    }

    #[test]
    fn type_ascii_sets_byte() {
        let mut s = state("48 65");
        s.cursor = Some(Cursor { cell: 0, nibble: 0 });
        assert_eq!(s.apply(Action::Ascii('Z')), Some("5a 65".into()));
        // 不可打印字符忽略
        assert_eq!(s.apply(Action::Ascii('\n')), None);
    }

    #[test]
    fn counts_and_offset() {
        let s = state("50 49 4E 47 ${seq} 2A");
        assert_eq!(s.counts(), (5, 0, 1));
        let mut s2 = state("50 49 4E 47 ${seq} 2A");
        s2.cursor = Some(Cursor { cell: 5, nibble: 0 });
        // 光标偏移估算: 每格按 1 字节计 (token 按常见输出 1 字节估)
        assert_eq!(s2.cursor_offset(), Some(5));
    }

    #[test]
    fn half_byte_roundtrip() {
        // 半字节只在 token 相邻处保留分组（变量边界语义）；纯 hex 串解析为
        // nibble 流后按两位一组规范化（与 hex_to_bytes 的流语义一致）
        let value = "50 4 4E";
        let d = doc(value);
        let serialized = serialize(&d.cells);
        assert_eq!(serialized, "50 44 E");
        // 序列化幂等
        assert_eq!(doc(&serialized).cells, d.cells);
        // nibble 流语义与 hex_to_bytes 一致: 5,0,4,4,E → (50)(44)(E 舍弃)
        let bytes = crate::utils::hex::hex_to_bytes(&serialized);
        assert_eq!(bytes, vec![0x50, 0x44]);
        // token 相邻的半字节分组稳定往返（变量输出与半字节拼接）
        let d = doc("50494E4${seq}");
        assert_eq!(serialize(&d.cells), "50 49 4E 4 ${seq}");
    }

    #[test]
    fn max_cells_truncation() {
        let long = "00".repeat(MAX_CELLS + 10);
        let d = doc(&long);
        assert!(d.truncated);
        assert_eq!(d.cells.len(), MAX_CELLS);
    }
}
