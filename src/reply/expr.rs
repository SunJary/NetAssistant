// 单表达式引擎（L1 纯逻辑，零依赖自研递归下降）
//
// 覆盖 docs/plan-reply-rules-expr.md §5 的文法，**有意省略**赋值、语句块、循环、
// 字符串字面量与用户变量（决策 E-9 / 大纲 §7）：那属于"完整脚本语言"，
// 等于自研一个小型解释器（1500+ 行）并长期维护其语义边界。
//
// 表达式已覆盖 NetAssist 手册 §7 的全部实战示例所需的计算能力：
//   ${= rx.u16be(2) + 1 }
//   ${= if(rx.u8(1) == 3, 1, 0) }
//   ${= bits(rx.u16be(2), 10, 4) }
//
// 数值统一按 `i64` wrapping 语义（不 panic —— panic 会杀死网络任务，见 §8.2）。

use crate::reply::frame::RxContext;
use crate::reply::model::Cmp;

/// 表达式的抽象语法树
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// 整数字面量（十进制 / `0x` / `0b`）
    Int(i64),
    /// 一元运算
    Unary { op: UnaryOp, expr: Box<Expr> },
    /// 二元运算
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// 三元 `cond ? a : b`
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// 函数调用（含 `rx.u16be(2)` 这类命名空间调用）
    Call { name: String, args: Vec<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    fn from_cmp(cmp: Cmp) -> Self {
        match cmp {
            Cmp::Eq => BinaryOp::Eq,
            Cmp::Ne => BinaryOp::Ne,
            Cmp::Gt => BinaryOp::Gt,
            Cmp::Ge => BinaryOp::Ge,
            Cmp::Lt => BinaryOp::Lt,
            Cmp::Le => BinaryOp::Le,
        }
    }
}

/// 表达式解析 / 求值错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    /// 词法或语法错误（含位置，便于 UI 指出"第 N 个字符"）
    Syntax { message: String, position: usize },
    /// 未知函数名（保存期拒绝）
    UnknownFunction(String),
    /// 参数个数不符（保存期拒绝）
    BadArity {
        name: String,
        expected: String,
        got: usize,
    },
    /// 嵌套过深（解析期拒绝，避免运行期栈溢出）
    TooDeep,
    /// 取帧失败（运行期：越界等）
    Frame(String),
}

impl ExprError {
    pub fn describe(&self) -> String {
        match self {
            ExprError::Syntax { message, position } => {
                format!("语法错误（第 {} 字符）: {}", position, message)
            }
            ExprError::UnknownFunction(name) => format!("未知函数: {}", name),
            ExprError::BadArity {
                name,
                expected,
                got,
            } => format!(
                "函数 {} 参数个数不符: 期望 {}, 实际 {}",
                name, expected, got
            ),
            ExprError::TooDeep => "表达式嵌套过深".to_string(),
            ExprError::Frame(msg) => msg.clone(),
        }
    }
}

/// 表达式最大嵌套深度（解析期拒绝，运行期不会遇到）
pub const MAX_EXPR_DEPTH: usize = 32;

// ============================================================================
// 词法
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Int(i64),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
    Question,
    Colon,
}

fn tokenize(source: &str) -> Result<Vec<(Token, usize)>, ExprError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        // 数字：十进制 / 0x / 0b（决策 E-10：协议调试大量用十六进制常量）
        if c.is_ascii_digit() {
            let mut text = String::new();
            if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                text.push_str("0x");
                i += 2;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    text.push(chars[i]);
                    i += 1;
                }
                let value = i64::from_str_radix(&text[2..], 16).map_err(|_| ExprError::Syntax {
                    message: format!("非法十六进制常量 {}", text),
                    position: start,
                })?;
                tokens.push((Token::Int(value), start));
                continue;
            }
            if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'b' || chars[i + 1] == 'B') {
                text.push_str("0b");
                i += 2;
                while i < chars.len() && (chars[i] == '0' || chars[i] == '1') {
                    text.push(chars[i]);
                    i += 1;
                }
                let value = i64::from_str_radix(&text[2..], 2).map_err(|_| ExprError::Syntax {
                    message: format!("非法二进制常量 {}", text),
                    position: start,
                })?;
                tokens.push((Token::Int(value), start));
                continue;
            }
            while i < chars.len() && chars[i].is_ascii_digit() {
                text.push(chars[i]);
                i += 1;
            }
            let value = text.parse::<i64>().map_err(|_| ExprError::Syntax {
                message: format!("整数溢出: {}", text),
                position: start,
            })?;
            tokens.push((Token::Int(value), start));
            continue;
        }
        // 标识符（含 `.` 以便支持 `rx.u16be` 这类命名空间）
        if c.is_alphabetic() || c == '_' {
            let mut name = String::new();
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                name.push(chars[i]);
                i += 1;
            }
            tokens.push((Token::Ident(name), start));
            continue;
        }
        // 双字符运算符优先
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        let matched_two = match two.as_str() {
            "==" => Some("=="),
            "!=" => Some("!="),
            "<=" => Some("<="),
            ">=" => Some(">="),
            "&&" => Some("&&"),
            "||" => Some("||"),
            _ => None,
        };
        if let Some(op) = matched_two {
            tokens.push((Token::Op(op), start));
            i += 2;
            continue;
        }
        let single = match c {
            '+' => Some("+"),
            '-' => Some("-"),
            '*' => Some("*"),
            '/' => Some("/"),
            '%' => Some("%"),
            '<' => Some("<"),
            '>' => Some(">"),
            '!' => Some("!"),
            _ => None,
        };
        if let Some(op) = single {
            tokens.push((Token::Op(op), start));
            i += 1;
            continue;
        }
        match c {
            '(' => {
                tokens.push((Token::LParen, start));
                i += 1;
            }
            ')' => {
                tokens.push((Token::RParen, start));
                i += 1;
            }
            ',' => {
                tokens.push((Token::Comma, start));
                i += 1;
            }
            '?' => {
                tokens.push((Token::Question, start));
                i += 1;
            }
            ':' => {
                tokens.push((Token::Colon, start));
                i += 1;
            }
            other => {
                return Err(ExprError::Syntax {
                    message: format!("无法识别的字符 '{}'", other),
                    position: start,
                });
            }
        }
    }
    Ok(tokens)
}

// ============================================================================
// 语法（递归下降，按 §5.2 文法的优先级层级）
// ============================================================================

struct Parser {
    tokens: Vec<(Token, usize)>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn position(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|(_, p)| *p)
            .unwrap_or_else(|| self.tokens.last().map(|(_, p)| *p + 1).unwrap_or(0))
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Token::Op(o)) if *o == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// 吃掉一个关键字标识符（如 `and` / `or`）
    fn eat_keyword(&mut self, keyword: &str) -> bool {
        // 注意：不能用 `matches!(..., if { self.pos += 1; true })` —— 守卫里改 self.pos
        // 会与 `self.peek()` 的借用冲突。这里显式判定后再推进。
        let matched = matches!(self.peek(), Some(Token::Ident(name)) if name == keyword);
        if matched {
            self.pos += 1;
        }
        matched
    }

    fn expect(&mut self, token: &Token, what: &str) -> Result<(), ExprError> {
        if self.peek() == Some(token) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ExprError::Syntax {
                message: format!("期望 {}", what),
                position: self.position(),
            })
        }
    }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            return Err(ExprError::TooDeep);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// expr := ternary
    fn parse_expr(&mut self) -> Result<Expr, ExprError> {
        self.enter()?;
        let result = self.parse_ternary();
        self.leave();
        result
    }

    fn parse_ternary(&mut self) -> Result<Expr, ExprError> {
        let cond = self.parse_or()?;
        if matches!(self.peek(), Some(Token::Question)) {
            self.pos += 1;
            let then = self.parse_expr()?;
            self.expect(&Token::Colon, "':'")?;
            let otherwise = self.parse_expr()?;
            return Ok(Expr::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
            });
        }
        Ok(cond)
    }

    fn parse_or(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_and()?;
        loop {
            let matched = self.eat_op("||") || self.eat_keyword("or");
            if !matched {
                break;
            }
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_equality()?;
        loop {
            let matched = self.eat_op("&&") || self.eat_keyword("and");
            if !matched {
                break;
            }
            let right = self.parse_equality()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_relational()?;
        loop {
            let op = if self.eat_op("==") {
                BinaryOp::Eq
            } else if self.eat_op("!=") {
                BinaryOp::Ne
            } else {
                break;
            };
            let right = self.parse_relational()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = if self.eat_op("<=") {
                BinaryOp::Le
            } else if self.eat_op(">=") {
                BinaryOp::Ge
            } else if self.eat_op("<") {
                BinaryOp::Lt
            } else if self.eat_op(">") {
                BinaryOp::Gt
            } else {
                break;
            };
            let right = self.parse_additive()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = if self.eat_op("+") {
                BinaryOp::Add
            } else if self.eat_op("-") {
                BinaryOp::Sub
            } else {
                break;
            };
            let right = self.parse_multiplicative()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = if self.eat_op("*") {
                BinaryOp::Mul
            } else if self.eat_op("/") {
                BinaryOp::Div
            } else if self.eat_op("%") {
                BinaryOp::Rem
            } else {
                break;
            };
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        self.enter()?;
        let result = (|| {
            if self.eat_op("-") {
                return Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(self.parse_unary()?),
                });
            }
            if self.eat_op("!") {
                return Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary()?),
                });
            }
            self.parse_primary()
        })();
        self.leave();
        result
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        match self.peek().cloned() {
            Some(Token::Int(v)) => {
                self.pos += 1;
                Ok(Expr::Int(v))
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen, "')'")?;
                Ok(inner)
            }
            Some(Token::Ident(name)) => {
                self.pos += 1;
                if self.peek() == Some(&Token::LParen) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if self.peek() != Some(&Token::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek(), Some(Token::Comma)) {
                                self.pos += 1;
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(&Token::RParen, "')'")?;
                    Ok(Expr::Call { name, args })
                } else {
                    // 裸标识符不是合法 primary（有意不支持用户变量，决策 E-9）
                    Err(ExprError::Syntax {
                        message: format!("'{}' 不是合法的表达式项（本轮不支持用户变量）", name),
                        position: self.position(),
                    })
                }
            }
            other => Err(ExprError::Syntax {
                message: format!("意外的记号 {:?}", other),
                position: self.position(),
            }),
        }
    }
}

/// 解析表达式源码
pub fn parse(source: &str) -> Result<Expr, ExprError> {
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Err(ExprError::Syntax {
            message: "表达式为空".to_string(),
            position: 0,
        });
    }
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
    };
    let expr = parser.parse_expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(ExprError::Syntax {
            message: "表达式尾部有多余内容".to_string(),
            position: parser.position(),
        });
    }
    Ok(expr)
}

/// 校验一棵已解析的 AST（渲染期对预编译 AST 复查，避免保存期漏检时运行期 panic）
pub fn validate_expr(expr: &Expr) -> Result<(), ExprError> {
    check_functions(expr)
}

/// 递归检查函数名与参数个数（白名单，见 §5.3）
fn check_functions(expr: &Expr) -> Result<(), ExprError> {
    match expr {
        Expr::Int(_) => Ok(()),
        Expr::Unary { expr, .. } => check_functions(expr),
        Expr::Binary { left, right, .. } => {
            check_functions(left)?;
            check_functions(right)
        }
        Expr::Ternary {
            cond,
            then,
            otherwise,
        } => {
            check_functions(cond)?;
            check_functions(then)?;
            check_functions(otherwise)
        }
        Expr::Call { name, args } => {
            for arg in args {
                check_functions(arg)?;
            }
            let spec =
                function_spec(name).ok_or_else(|| ExprError::UnknownFunction(name.clone()))?;
            if !spec.arity.accepts(args.len()) {
                return Err(ExprError::BadArity {
                    name: name.clone(),
                    expected: spec.arity.describe(),
                    got: args.len(),
                });
            }
            Ok(())
        }
    }
}

/// 是否引用接收帧（决定渲染时是否需要 `RxFrame` 上下文）
///
/// **校验类函数也算**：`crc16_modbus(0, 6)` 不带任何参数却作用于接收帧，
/// 漏掉它们会让"需要帧上下文"的判断失效，进而把变量渲染成空。
pub fn needs_rx(expr: &Expr) -> bool {
    match expr {
        Expr::Int(_) => false,
        Expr::Unary { expr, .. } => needs_rx(expr),
        Expr::Binary { left, right, .. } => needs_rx(left) || needs_rx(right),
        Expr::Ternary {
            cond,
            then,
            otherwise,
        } => needs_rx(cond) || needs_rx(then) || needs_rx(otherwise),
        Expr::Call { name, args } => {
            name.starts_with("rx.") || is_frame_function(name) || args.iter().any(needs_rx)
        }
    }
}

/// 该函数是否作用于接收帧（无 rx 上下文时必然求值失败）
pub fn is_frame_function(name: &str) -> bool {
    matches!(
        name,
        "xor"
            | "sum8"
            | "lrc"
            | "crc16_modbus"
            | "crc16modbus"
            | "crc16_ccitt"
            | "crc16ccitt"
            | "crc32"
            | "calc"
    )
}

// ============================================================================
// 函数白名单
// ============================================================================

/// 允许的参数个数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    Exact(usize),
    Range(usize, usize),
}

impl Arity {
    fn accepts(self, n: usize) -> bool {
        match self {
            Arity::Exact(k) => n == k,
            Arity::Range(lo, hi) => n >= lo && n <= hi,
        }
    }

    fn describe(self) -> String {
        match self {
            Arity::Exact(k) => k.to_string(),
            Arity::Range(lo, hi) => format!("{}..{}", lo, hi),
        }
    }
}

/// 函数说明
#[derive(Debug, Clone, Copy)]
pub struct FunctionSpec {
    pub arity: Arity,
}

/// 函数白名单（首批，见 §5.3）
pub fn function_spec(name: &str) -> Option<FunctionSpec> {
    let spec = |arity: Arity| Some(FunctionSpec { arity });
    // 接收帧标量取值：与变量语法共用同一取值层
    for accessor in [
        "rx.u8", "rx.i8", "rx.u16be", "rx.u16le", "rx.i16be", "rx.i16le", "rx.u32be", "rx.u32le",
        "rx.i32be", "rx.i32le", "rx.u64be", "rx.u64le", "rx.i64be", "rx.i64le",
    ] {
        if name == accessor {
            return spec(Arity::Exact(1));
        }
    }
    match name {
        "rx.len" => spec(Arity::Exact(0)),
        "bits" => spec(Arity::Exact(3)),
        "min" => spec(Arity::Exact(2)),
        "max" => spec(Arity::Exact(2)),
        "abs" => spec(Arity::Exact(1)),
        "clamp" => spec(Arity::Exact(3)),
        "if" => spec(Arity::Exact(3)),
        // 校验：作用域为**接收帧**（与 ${rx.crc*} 一致）。
        // 两种拼写都接受：`crc16_modbus`（与 UI/持久化一致）与 `crc16modbus`
        // （与变量语法 `${crc16modbus:...}` 一致）—— 用户在两处看到的名字必须都能用。
        "crc16_modbus" | "crc16modbus" => spec(Arity::Exact(2)),
        "crc16_ccitt" | "crc16ccitt" => spec(Arity::Exact(2)),
        "crc32" => spec(Arity::Exact(2)),
        "xor" => spec(Arity::Exact(2)),
        "sum8" => spec(Arity::Exact(2)),
        "lrc" => spec(Arity::Exact(2)),
        // 参数化 CRC（F-44 的表达式形态）
        "calc" => spec(Arity::Range(7, 7)),
        _ => None,
    }
}

// ============================================================================
// 求值
// ============================================================================

/// 求值表达式。`rx` 为 `None` 时任何引用接收帧的函数都会失败（渲染层据此原样保留）。
pub fn eval(expr: &Expr, rx: Option<&RxContext<'_>>) -> Result<i64, ExprError> {
    match expr {
        Expr::Int(v) => Ok(*v),
        Expr::Unary { op, expr } => {
            let v = eval(expr, rx)?;
            Ok(match op {
                UnaryOp::Neg => v.wrapping_neg(),
                UnaryOp::Not => {
                    if v == 0 {
                        1
                    } else {
                        0
                    }
                }
            })
        }
        Expr::Binary { op, left, right } => {
            // 短路求值：`a && b` 在 a 为 0 时不求值 b（b 可能引用越界字段）
            match op {
                BinaryOp::And => {
                    let l = eval(left, rx)?;
                    if l == 0 {
                        return Ok(0);
                    }
                    return Ok(if eval(right, rx)? != 0 { 1 } else { 0 });
                }
                BinaryOp::Or => {
                    let l = eval(left, rx)?;
                    if l != 0 {
                        return Ok(1);
                    }
                    return Ok(if eval(right, rx)? != 0 { 1 } else { 0 });
                }
                _ => {}
            }
            let l = eval(left, rx)?;
            let r = eval(right, rx)?;
            Ok(match op {
                BinaryOp::Add => l.wrapping_add(r),
                BinaryOp::Sub => l.wrapping_sub(r),
                BinaryOp::Mul => l.wrapping_mul(r),
                BinaryOp::Div => {
                    if r == 0 {
                        // 除零：结果为 0 + warn（不 panic，不中断渲染，见 §5.5）
                        log::warn!("[expr] 除零，结果按 0 处理");
                        0
                    } else {
                        l.wrapping_div(r)
                    }
                }
                BinaryOp::Rem => {
                    if r == 0 {
                        log::warn!("[expr] 取模零，结果按 0 处理");
                        0
                    } else {
                        l.wrapping_rem(r)
                    }
                }
                BinaryOp::Eq => i64::from(l == r),
                BinaryOp::Ne => i64::from(l != r),
                BinaryOp::Lt => i64::from(l < r),
                BinaryOp::Le => i64::from(l <= r),
                BinaryOp::Gt => i64::from(l > r),
                BinaryOp::Ge => i64::from(l >= r),
                BinaryOp::And | BinaryOp::Or => unreachable!("已在上面短路处理"),
            })
        }
        Expr::Ternary {
            cond,
            then,
            otherwise,
        } => {
            // 三元同样短路：只求值被选中的分支
            if eval(cond, rx)? != 0 {
                eval(then, rx)
            } else {
                eval(otherwise, rx)
            }
        }
        Expr::Call { name, args } => eval_call(name, args, rx),
    }
}

fn eval_call(name: &str, args: &[Expr], rx: Option<&RxContext<'_>>) -> Result<i64, ExprError> {
    let spec = function_spec(name).ok_or_else(|| ExprError::UnknownFunction(name.to_string()))?;
    if !spec.arity.accepts(args.len()) {
        return Err(ExprError::BadArity {
            name: name.to_string(),
            expected: spec.arity.describe(),
            got: args.len(),
        });
    }

    // 标量取值类：`rx.u16be(2)`
    if let Some(accessor) = name.strip_prefix("rx.") {
        if accessor == "len" {
            return Ok(rx
                .map(|c| c.len() as i64)
                .ok_or_else(|| ExprError::Frame("无接收帧上下文，无法取 rx.len".to_string()))?);
        }
        let offset = eval(&args[0], rx)?;
        if offset < 0 {
            return Err(ExprError::Frame(format!("负偏移 {}", offset)));
        }
        let ctx =
            rx.ok_or_else(|| ExprError::Frame(format!("无接收帧上下文，无法取 rx.{}", accessor)))?;
        return ctx
            .get_scalar(accessor, offset as usize)
            .map_err(|e| ExprError::Frame(e.describe()));
    }

    // 其余函数：先求值参数
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        values.push(eval(arg, rx)?);
    }

    match name {
        "bits" => {
            let (value, start, width) = (values[0], values[1], values[2]);
            if start < 0 || width <= 0 || start + width > 64 {
                return Err(ExprError::Frame(format!(
                    "位域参数非法: start={} width={}",
                    start, width
                )));
            }
            let mask = if width >= 64 {
                u64::MAX
            } else {
                (1u64 << width) - 1
            };
            Ok(((value as u64 >> start) & mask) as i64)
        }
        "min" => Ok(values[0].min(values[1])),
        "max" => Ok(values[0].max(values[1])),
        "abs" => Ok(values[0].wrapping_abs()),
        "clamp" => Ok(values[0].clamp(values[1], values[2])),
        "if" => Ok(if values[0] != 0 { values[1] } else { values[2] }),
        "xor" | "sum8" | "lrc" | "crc16_modbus" | "crc16modbus" | "crc16_ccitt" | "crc16ccitt"
        | "crc32" => {
            let ctx =
                rx.ok_or_else(|| ExprError::Frame(format!("无接收帧上下文，无法调用 {}()", name)))?;
            let (offset, len) = (values[0], values[1]);
            if offset < 0 || len < 0 {
                return Err(ExprError::Frame("校验区间参数为负".to_string()));
            }
            let bytes = ctx
                .checksum_by_name(name, offset as usize, len as usize, false)
                .map_err(|e| ExprError::Frame(e.describe()))?;
            // 结果按大端解释为整数（与 ${rx.crc*} 的默认输出一致）
            let mut value: i64 = 0;
            for b in bytes {
                value = (value << 8) | b as i64;
            }
            Ok(value)
        }
        "calc" => {
            use crate::core::crc::{CrcParams, crc_generic};
            let ctx =
                rx.ok_or_else(|| ExprError::Frame("无接收帧上下文，无法调用 calc()".to_string()))?;
            let (poly, init, xorout, refin, refout, offset, len) = (
                values[0], values[1], values[2], values[3], values[4], values[5], values[6],
            );
            if offset < 0 || len < 0 {
                return Err(ExprError::Frame("校验区间参数为负".to_string()));
            }
            // 宽度按 poly 的最高有效位推断（8/16/32/64），与 RevEng 的宽度概念一致
            let width = infer_crc_width(poly);
            let params = CrcParams {
                poly: poly as u64,
                init: init as u64,
                xorout: xorout as u64,
                refin: refin != 0,
                refout: refout != 0,
                width,
            };
            let slice = ctx
                .slice(offset as usize, Some(len as usize))
                .map_err(|e| ExprError::Frame(e.describe()))?;
            Ok(crc_generic(slice, params) as i64)
        }
        other => Err(ExprError::UnknownFunction(other.to_string())),
    }
}

/// 由多项式推断 CRC 宽度（最高有效位的位置 + 1，向上取到 8/16/32/64）
fn infer_crc_width(poly: i64) -> u8 {
    let bits = 64 - (poly as u64).leading_zeros();
    match bits {
        0..=8 => 8,
        9..=16 => 16,
        17..=32 => 32,
        _ => 64,
    }
}

// 便于其它模块从 Cmp 构造表达式比较
impl From<Cmp> for BinaryOp {
    fn from(cmp: Cmp) -> Self {
        BinaryOp::from_cmp(cmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reply::frame::RxFrame;
    use std::sync::Arc;
    fn frame(bytes: &[u8]) -> Arc<RxFrame> {
        Arc::new(RxFrame::new(
            bytes.to_vec(),
            "127.0.0.1:1234".parse().unwrap(),
        ))
    }

    fn eval_src(source: &str, bytes: &[u8]) -> Result<i64, ExprError> {
        let expr = parse(source)?;
        check_functions(&expr)?;
        let f = frame(bytes);
        let ctx = RxContext::new(&f);
        eval(&expr, Some(&ctx))
    }

    /// 运算符优先级：`1 + 2 * 3` 必须是 7 而非 9
    #[test]
    fn test_precedence() {
        assert_eq!(eval_src("1 + 2 * 3", &[]), Ok(7));
        assert_eq!(eval_src("(1 + 2) * 3", &[]), Ok(9));
        assert_eq!(eval_src("2 * 3 % 4", &[]), Ok(2));
        assert_eq!(eval_src("10 - 2 - 3", &[]), Ok(5));
        assert_eq!(eval_src("1 + 2 == 3", &[]), Ok(1));
        assert_eq!(eval_src("1 < 2 && 3 > 2", &[]), Ok(1));
        assert_eq!(eval_src("1 > 2 || 3 > 2", &[]), Ok(1));
        assert_eq!(eval_src("!0", &[]), Ok(1));
        assert_eq!(eval_src("-5 + 2", &[]), Ok(-3));
    }

    /// 数字格式：十进制 / 0x / 0b（决策 E-10）
    #[test]
    fn test_number_formats() {
        assert_eq!(eval_src("0xFF", &[]), Ok(255));
        assert_eq!(eval_src("0b1010", &[]), Ok(10));
        assert_eq!(eval_src("0x10 + 1", &[]), Ok(17));
    }

    /// 三元与 `if()` 必须等价（对齐 NetAssist 的 `if` 写法）
    #[test]
    fn test_ternary_and_if_function() {
        assert_eq!(eval_src("1 ? 10 : 20", &[]), Ok(10));
        assert_eq!(eval_src("0 ? 10 : 20", &[]), Ok(20));
        assert_eq!(eval_src("if(1, 10, 20)", &[]), Ok(10));
        assert_eq!(eval_src("if(0, 10, 20)", &[]), Ok(20));
        // 嵌套
        assert_eq!(eval_src("if(1, if(0, 1, 2), 3)", &[]), Ok(2));
    }

    /// 接收帧取值：与 `${rx.*}` 共用取值层，语义必须一致
    #[test]
    fn test_rx_accessors() {
        let data = [0x01u8, 0x03, 0x00, 0x02, 0xFF];
        assert_eq!(eval_src("rx.u8(0)", &data), Ok(0x01));
        assert_eq!(eval_src("rx.u16be(2)", &data), Ok(0x0002));
        assert_eq!(eval_src("rx.u16le(0)", &data), Ok(0x0301));
        assert_eq!(eval_src("rx.i8(4)", &data), Ok(-1));
        assert_eq!(eval_src("rx.len()", &data), Ok(5));
        // 越界 → Err（不 panic）
        assert!(eval_src("rx.u32be(3)", &data).is_err());
        assert!(eval_src("rx.u8(99)", &data).is_err());
    }

    /// 位域提取（决策 E-12）：从 bit `start` 起取 `width` 位（LSB 为 bit 0）
    #[test]
    fn test_bits() {
        // 0x8A5F = 1000_1010_0101_1111b；bit 10..14 = 0b0010 = 2
        assert_eq!(eval_src("bits(0x8A5F, 10, 4)", &[]), Ok(2));
        assert_eq!(eval_src("bits(0b1111, 0, 4)", &[]), Ok(15));
        assert_eq!(eval_src("bits(0xFF, 4, 4)", &[]), Ok(15));
        assert_eq!(eval_src("bits(0xFF00, 8, 8)", &[]), Ok(0xFF));
        // 非法参数
        assert!(eval_src("bits(1, 60, 8)", &[]).is_err());
        assert!(eval_src("bits(1, -1, 4)", &[]).is_err());
        assert!(eval_src("bits(1, 0, 0)", &[]).is_err());
    }

    /// 数学函数
    #[test]
    fn test_math_functions() {
        assert_eq!(eval_src("min(3, 7)", &[]), Ok(3));
        assert_eq!(eval_src("max(3, 7)", &[]), Ok(7));
        assert_eq!(eval_src("abs(-9)", &[]), Ok(9));
        assert_eq!(eval_src("clamp(15, 0, 10)", &[]), Ok(10));
        assert_eq!(eval_src("clamp(-5, 0, 10)", &[]), Ok(0));
        assert_eq!(eval_src("clamp(5, 0, 10)", &[]), Ok(5));
    }

    /// 校验函数作用域是**接收帧**
    #[test]
    fn test_checksum_functions() {
        let modbus = [0x01u8, 0x03, 0x00, 0x00, 0x00, 0x02];
        assert_eq!(eval_src("crc16_modbus(0, 6)", &modbus), Ok(0x0BC4));
        assert_eq!(eval_src("crc16modbus(0, 6)", &modbus), Ok(0x0BC4));
        assert_eq!(eval_src("xor(0, 3)", &modbus), Ok(0x02));
        assert_eq!(eval_src("sum8(0, 3)", &modbus), Ok(0x04));
        assert_eq!(eval_src("lrc(0, 2)", &modbus), Ok(0xFC));
        assert!(eval_src("crc32(0, 6)", &modbus).is_ok());
        // 越界
        assert!(eval_src("crc16_modbus(0, 99)", &modbus).is_err());
    }

    /// `calc()`：参数化 CRC，等价于标准变体（F-44 的表达式形态）
    #[test]
    fn test_calc() {
        let data = b"123456789";
        // CRC16/MODBUS 的参数
        assert_eq!(
            eval_src("calc(0x8005, 0xFFFF, 0, 1, 1, 0, 9)", data),
            Ok(0x4B37)
        );
        // CRC16/CCITT-FALSE 的参数
        assert_eq!(
            eval_src("calc(0x1021, 0xFFFF, 0, 0, 0, 0, 9)", data),
            Ok(0x29B1)
        );
    }

    /// 组合：`rx.u16be(2) + 1`、`if(rx.u8(1) == 3, 4, 0)` —— 覆盖 NetAssist §7 的实例
    #[test]
    fn test_netassist_scenarios() {
        let modbus = [0x01u8, 0x03, 0x00, 0x00, 0x00, 0x02];
        assert_eq!(eval_src("rx.u16be(2) + 1", &modbus), Ok(1));
        assert_eq!(eval_src("if(rx.u8(1) == 3, 4, 0)", &modbus), Ok(4));
        assert_eq!(eval_src("if(rx.u8(1) == 6, 4, 0)", &modbus), Ok(0));
        assert_eq!(eval_src("rx.u8(0) * 256 + rx.u8(1)", &modbus), Ok(0x0103));
    }

    /// 除零不 panic，结果 0（§5.5）
    #[test]
    fn test_division_by_zero() {
        assert_eq!(eval_src("1 / 0", &[]), Ok(0));
        assert_eq!(eval_src("1 % 0", &[]), Ok(0));
    }

    /// 数值溢出按 i64 wrapping（行为可预测，不 panic）
    #[test]
    fn test_wrapping_overflow() {
        assert_eq!(
            eval_src("9223372036854775807 + 1", &[]),
            Ok(i64::MIN),
            "溢出必须 wrapping 而非 panic"
        );
    }

    /// 短路：`0 && (1/0)` 不得因除零而报错（右项根本不求值）
    #[test]
    fn test_short_circuit_evaluation() {
        assert_eq!(eval_src("0 && (1 / 0)", &[]), Ok(0));
        assert_eq!(eval_src("1 || (1 / 0)", &[]), Ok(1));
        // 三元只求值被选中的分支
        assert_eq!(eval_src("1 ? 5 : (1 / 0)", &[]), Ok(5));
        assert_eq!(eval_src("0 ? (1 / 0) : 7", &[]), Ok(7));
    }

    /// 未知函数与参数个数不符在保存期被拒（UI 据此禁用保存）
    #[test]
    fn test_validation_rejects_unknown_and_bad_arity() {
        // 生产路径：先解析再校验 AST（与渲染期一致）
        let validate = |source: &str| parse(source).and_then(|expr| validate_expr(&expr));
        assert!(matches!(
            validate("send(1)"),
            Err(ExprError::UnknownFunction(_))
        ));
        assert!(matches!(
            validate("min(1)"),
            Err(ExprError::BadArity { .. })
        ));
        assert!(matches!(
            validate("rx.len(1)"),
            Err(ExprError::BadArity { .. })
        ));
        assert!(matches!(
            validate("bits(1,2)"),
            Err(ExprError::BadArity { .. })
        ));
        // 合法表达式通过
        assert!(validate("rx.u16be(2) + 1").is_ok());
        assert!(validate("if(rx.u8(1) == 3, 4, 0)").is_ok());
        assert!(validate("crc16_modbus(0, 6)").is_ok());
        assert!(validate("calc(0x8005, 0xFFFF, 0, 1, 1, 0, 9)").is_ok());
    }

    /// 语法错误：位置信息必须给出（UI 用来高亮）
    #[test]
    fn test_syntax_errors_have_position() {
        for bad in ["1 +", "(1", ")", "1 2", "", "1 ? 2", "foo", "1 @ 2"] {
            let err = parse(bad).unwrap_err();
            match err {
                ExprError::Syntax { .. } => {}
                other => panic!("{:?} 应为语法错误, 实际 {:?}", bad, other),
            }
            assert!(!parse(bad).unwrap_err().describe().is_empty());
        }
    }

    /// 嵌套过深在解析期被拒（运行期不会栈溢出）
    #[test]
    fn test_depth_limit() {
        let deep = format!(
            "{}1{}",
            "(".repeat(MAX_EXPR_DEPTH + 5),
            ")".repeat(MAX_EXPR_DEPTH + 5)
        );
        assert!(matches!(parse(&deep), Err(ExprError::TooDeep)));
        // 未超限的嵌套正常
        let ok = format!("{}1{}", "(".repeat(5), ")".repeat(5));
        assert_eq!(eval_src(&ok, &[]), Ok(1));
    }

    /// 无接收帧上下文时引用 rx.* 必须失败（渲染层据此原样保留变量）
    #[test]
    fn test_no_rx_context_fails() {
        let expr = parse("rx.u16be(0)").unwrap();
        assert!(matches!(eval(&expr, None), Err(ExprError::Frame(_))));
        // 不引用帧的表达式在没有上下文时仍可求值
        let plain = parse("1 + 1").unwrap();
        assert_eq!(eval(&plain, None), Ok(2));
    }

    /// needs_rx 探测（快路径判断）
    #[test]
    fn test_needs_rx_detection() {
        assert!(!needs_rx(&parse("1 + 2").unwrap()));
        assert!(!needs_rx(&parse("min(1, max(2, 3))").unwrap()));
        assert!(needs_rx(&parse("rx.u16be(2)").unwrap()));
        assert!(needs_rx(&parse("rx.len()").unwrap()));
        assert!(needs_rx(&parse("1 + rx.u8(0)").unwrap()));
        assert!(needs_rx(&parse("if(rx.u8(0), 1, 2)").unwrap()));
        // 校验函数也依赖帧
        assert!(needs_rx(&parse("crc16_modbus(0, 6)").unwrap()));
    }

    /// `BinaryOp` 可由 `Cmp` 构造（保持两者语义一致）
    #[test]
    fn test_binary_op_from_cmp() {
        assert_eq!(BinaryOp::from(Cmp::Ge), BinaryOp::Ge);
    }

    /// `and` / `or` 关键字与 `&&` / `||` 等价（对齐 C 系习惯与文字写法）
    #[test]
    fn test_word_operators() {
        assert_eq!(eval_src("1 and 1", &[]), Ok(1));
        assert_eq!(eval_src("0 or 1", &[]), Ok(1));
        assert_eq!(eval_src("0 and 1", &[]), Ok(0));
    }

    /// `infer_crc_width` 推断正确
    #[test]
    fn test_infer_crc_width() {
        assert_eq!(infer_crc_width(0x07), 8);
        assert_eq!(infer_crc_width(0x8005), 16);
        assert_eq!(infer_crc_width(0x1021), 16);
        assert_eq!(infer_crc_width(0x04C11DB7), 32);
        assert_eq!(infer_crc_width(0x1EDC6F41), 32);
    }
}
