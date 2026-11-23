// 工具箱「随机」组的纯计算逻辑。
//
// 与 UI 解耦：参数 → 生成结果快照，全部可在无头环境单测。
// 随机源统一使用操作系统 CSPRNG（getrandom / SysRng），字符抽样使用拒绝采样，
// 避免 `% n` 取模偏差；生成结果不缓存到磁盘，也不写日志。

use uuid::Uuid;

/// 单次生成的长度上限（防止误输入超大值卡住界面）
pub const MAX_LENGTH: usize = 1024;
/// 单次生成的数量上限
pub const MAX_COUNT: usize = 100;

/// 随机类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomKind {
    /// 标准 UUID v4（36 位，带横线）
    Uuid,
    /// UUID v4 的无横线形式（32 位 hex）
    UuidSimple,
    /// 指定长度的十六进制串
    Hex,
    /// 指定长度的十进制数字串
    Decimal,
    /// 按点选字符集生成（随机密码）
    Custom,
    /// SM2 密钥对（私钥 + 未压缩公钥）
    Sm2,
    /// SM4 密钥 + IV
    Sm4,
}

impl RandomKind {
    /// 该类型的默认长度；定长类型返回固定长度
    pub fn default_length(self) -> usize {
        match self {
            RandomKind::Uuid => 36,
            RandomKind::UuidSimple => 32,
            RandomKind::Hex => 32,
            RandomKind::Decimal => 16,
            RandomKind::Custom => 16,
            RandomKind::Sm2 => 64,
            RandomKind::Sm4 => 32,
        }
    }

    /// 长度是否固定（UUID / 密钥类不可调）
    pub fn is_fixed_length(self) -> bool {
        matches!(
            self,
            RandomKind::Uuid | RandomKind::UuidSimple | RandomKind::Sm2 | RandomKind::Sm4
        )
    }

    /// 是否使用自定义字符集
    pub fn uses_charset(self) -> bool {
        self == RandomKind::Custom
    }
}

/// 自定义字符集点选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharsetOptions {
    /// 数字 0-9
    pub digits: bool,
    /// 小写 a-z
    pub lower: bool,
    /// 大写 A-Z
    pub upper: bool,
    /// 符号 !@#$%^&*()-_=+[]{};:,.<>?/~
    pub symbols: bool,
    /// 排除易混淆字符 0 O o 1 l I | ` ' "
    pub exclude_ambiguous: bool,
}

impl Default for CharsetOptions {
    fn default() -> Self {
        Self::password()
    }
}

const DIGITS: &str = "0123456789";
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>?/~";
const AMBIGUOUS: &[char] = &['0', 'O', 'o', '1', 'l', 'I', '|', '`', '\'', '"'];

impl CharsetOptions {
    /// 随机密码默认：四类全选 + 排除易混淆
    pub fn password() -> Self {
        Self {
            digits: true,
            lower: true,
            upper: true,
            symbols: true,
            exclude_ambiguous: true,
        }
    }

    /// 字母数字默认：数字 + 小写 + 大写 + 排除易混淆
    pub fn alnum() -> Self {
        Self {
            digits: true,
            lower: true,
            upper: true,
            symbols: false,
            exclude_ambiguous: true,
        }
    }

    /// 是否至少选中一类字符
    pub fn any_selected(&self) -> bool {
        self.digits || self.lower || self.upper || self.symbols
    }

    /// 已选类别（每类过滤易混淆字符后），空类别会被剔除
    fn selected_classes(&self) -> Vec<Vec<char>> {
        let mut classes = Vec::new();
        if self.digits {
            classes.push(self.filter(DIGITS));
        }
        if self.lower {
            classes.push(self.filter(LOWER));
        }
        if self.upper {
            classes.push(self.filter(UPPER));
        }
        if self.symbols {
            classes.push(self.filter(SYMBOLS));
        }
        classes.retain(|class| !class.is_empty());
        classes
    }

    /// 单个类别 → 字符集合（按排除易混淆配置过滤）
    fn filter(&self, source: &str) -> Vec<char> {
        source
            .chars()
            .filter(|c| !self.exclude_ambiguous || !AMBIGUOUS.contains(c))
            .collect()
    }

    /// 全部已选类别的并集（去重、保持类别顺序）；空集返回 Err
    pub fn effective_charset(&self) -> Result<Vec<char>, RandomError> {
        let mut union = Vec::new();
        for class in self.selected_classes() {
            for c in class {
                if !union.contains(&c) {
                    union.push(c);
                }
            }
        }
        if union.is_empty() {
            return Err(RandomError::EmptyCharset);
        }
        Ok(union)
    }
}

/// 生成参数
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomParams {
    pub kind: RandomKind,
    /// 目标字符数（定长类型忽略）
    pub length: usize,
    /// 生成数量（1..=MAX_COUNT）
    pub count: usize,
    /// 十六进制是否大写（仅 Hex 生效）
    pub uppercase: bool,
    pub charset: CharsetOptions,
}

impl RandomParams {
    /// 参数校验；错误在生成前返回，不消耗熵
    pub fn validate(&self) -> Result<(), RandomError> {
        if self.count == 0 || self.count > MAX_COUNT {
            return Err(RandomError::InvalidCount);
        }
        if !self.kind.is_fixed_length() && (self.length == 0 || self.length > MAX_LENGTH) {
            return Err(RandomError::InvalidLength);
        }
        if self.kind.uses_charset() && !self.charset.any_selected() {
            return Err(RandomError::EmptyCharset);
        }
        Ok(())
    }
}

/// 行标签（UI 负责本地化）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowLabel {
    None,
    PrivateKey,
    PublicKey,
    Key,
    Iv,
}

/// 一条输出行
#[derive(Debug, Clone)]
pub struct RandomRow {
    /// 所属生成项序号（0 基）；批量时 UI 展示 #1..#N
    pub group: usize,
    pub label: RowLabel,
    pub value: String,
}

/// 一次生成的快照
#[derive(Debug, Clone)]
pub struct RandomResult {
    pub rows: Vec<RandomRow>,
    /// 本次生成项数（决定 UI 是否显示序号列）
    pub count: usize,
    /// 「全部复制」内容：所有值按换行拼接
    pub flat: String,
}

/// 生成错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomError {
    EmptyCharset,
    InvalidLength,
    InvalidCount,
    CryptoFailure,
}

/// 生成入口：参数校验 → 按类型产生行 → 汇总快照
pub fn generate(params: &RandomParams) -> Result<RandomResult, RandomError> {
    params.validate()?;

    let mut source = RandomSource::new();
    let mut rows = Vec::with_capacity(params.count);

    match params.kind {
        RandomKind::Uuid | RandomKind::UuidSimple => {
            let simple = params.kind == RandomKind::UuidSimple;
            for group in 0..params.count {
                rows.push(RandomRow {
                    group,
                    label: RowLabel::None,
                    value: uuid_string(&mut source, simple)?,
                });
            }
        }
        RandomKind::Hex => {
            for group in 0..params.count {
                rows.push(RandomRow {
                    group,
                    label: RowLabel::None,
                    value: hex_string(&mut source, params.length, params.uppercase)?,
                });
            }
        }
        RandomKind::Decimal => {
            for group in 0..params.count {
                rows.push(RandomRow {
                    group,
                    label: RowLabel::None,
                    value: decimal_string(&mut source, params.length)?,
                });
            }
        }
        RandomKind::Custom => {
            let classes = params.charset.selected_classes();
            // 先求并集，空集在这里报错（validate 已挡住全不选，防排除后为空）
            let union = params.charset.effective_charset()?;
            for group in 0..params.count {
                rows.push(RandomRow {
                    group,
                    label: RowLabel::None,
                    value: custom_string(&mut source, params.length, &classes, &union)?,
                });
            }
        }
        RandomKind::Sm2 => {
            for group in 0..params.count {
                let (private, public) = sm2_keypair()?;
                rows.push(RandomRow {
                    group,
                    label: RowLabel::PrivateKey,
                    value: private,
                });
                rows.push(RandomRow {
                    group,
                    label: RowLabel::PublicKey,
                    value: public,
                });
            }
        }
        RandomKind::Sm4 => {
            for group in 0..params.count {
                let (key, iv) = sm4_key_iv()?;
                rows.push(RandomRow {
                    group,
                    label: RowLabel::Key,
                    value: key,
                });
                rows.push(RandomRow {
                    group,
                    label: RowLabel::Iv,
                    value: iv,
                });
            }
        }
    }

    let flat = rows
        .iter()
        .map(|row| row.value.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(RandomResult {
        rows,
        count: params.count,
        flat,
    })
}

// ============== 系统熵源缓冲 ==============

/// 4KB 熵缓冲：批量生成只在耗尽时发起系统调用，避免逐字符调用。
struct RandomSource {
    buf: [u8; 4096],
    pos: usize,
}

impl RandomSource {
    fn new() -> Self {
        Self {
            buf: [0u8; 4096],
            pos: 4096,
        }
    }

    fn refill(&mut self) -> Result<(), RandomError> {
        getrandom::fill(&mut self.buf).map_err(|_| RandomError::CryptoFailure)?;
        self.pos = 0;
        Ok(())
    }

    fn fill(&mut self, out: &mut [u8]) -> Result<(), RandomError> {
        let mut written = 0;
        while written < out.len() {
            if self.pos == self.buf.len() {
                self.refill()?;
            }
            let n = (out.len() - written).min(self.buf.len() - self.pos);
            out[written..written + n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
            self.pos += n;
            written += n;
        }
        Ok(())
    }

    fn byte(&mut self) -> Result<u8, RandomError> {
        if self.pos == self.buf.len() {
            self.refill()?;
        }
        let byte = self.buf[self.pos];
        self.pos += 1;
        Ok(byte)
    }

    fn next_u32(&mut self) -> Result<u32, RandomError> {
        let mut bytes = [0u8; 4];
        self.fill(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// 无偏取 [0, upper) 的整数（拒绝采样）
    fn below(&mut self, upper: u32) -> Result<u32, RandomError> {
        if upper <= 1 {
            return Ok(0);
        }
        let zone = u32::MAX - (u32::MAX % upper);
        loop {
            let value = self.next_u32()?;
            if value < zone {
                return Ok(value % upper);
            }
        }
    }
}

// ============== 各类型生成 ==============

fn uuid_string(source: &mut RandomSource, simple: bool) -> Result<String, RandomError> {
    let mut bytes = [0u8; 16];
    source.fill(&mut bytes)?;
    // RFC 4122：版本位 = 4，变体位 = 10xx
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let uuid = Uuid::from_bytes(bytes);
    Ok(if simple {
        uuid.simple().to_string()
    } else {
        uuid.to_string()
    })
}

fn hex_string(
    source: &mut RandomSource,
    length: usize,
    uppercase: bool,
) -> Result<String, RandomError> {
    const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";
    const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
    let table = if uppercase { HEX_UPPER } else { HEX_LOWER };
    let mut out = String::with_capacity(length);
    for _ in 0..length {
        out.push(table[(source.byte()? & 0x0f) as usize] as char);
    }
    Ok(out)
}

fn decimal_string(source: &mut RandomSource, length: usize) -> Result<String, RandomError> {
    let mut out = String::with_capacity(length);
    for _ in 0..length {
        out.push((b'0' + source.below(10)? as u8) as char);
    }
    Ok(out)
}

/// 自定义字符集：长度足够时每个已选类别至少出现 1 次，再 Fisher–Yates 洗牌
fn custom_string(
    source: &mut RandomSource,
    length: usize,
    classes: &[Vec<char>],
    union: &[char],
) -> Result<String, RandomError> {
    if union.is_empty() {
        return Err(RandomError::EmptyCharset);
    }

    let mut out: Vec<char> = Vec::with_capacity(length);
    if length >= classes.len() {
        for class in classes {
            out.push(class[source.below(class.len() as u32)? as usize]);
        }
    }
    while out.len() < length {
        out.push(union[source.below(union.len() as u32)? as usize]);
    }
    for i in (1..out.len()).rev() {
        let j = source.below((i + 1) as u32)? as usize;
        out.swap(i, j);
    }
    Ok(out.into_iter().collect())
}

/// SM2 密钥对：私钥 32 字节，公钥为 SEC1 未压缩点（04 || X || Y，65 字节）
fn sm2_keypair() -> Result<(String, String), RandomError> {
    use sm2::elliptic_curve::Generate;
    use sm2::elliptic_curve::common::getrandom::SysRng;
    use sm2::elliptic_curve::sec1::ToSec1Point;

    let secret = sm2::SecretKey::try_generate_from_rng(&mut SysRng)
        .map_err(|_| RandomError::CryptoFailure)?;
    let private = hex::encode(secret.to_bytes());
    let public = hex::encode(secret.public_key().to_sec1_point(false).as_bytes());
    Ok((private, public))
}

/// SM4 密钥与 IV：各 16 字节
fn sm4_key_iv() -> Result<(String, String), RandomError> {
    let mut key = [0u8; 16];
    let mut iv = [0u8; 16];
    getrandom::fill(&mut key).map_err(|_| RandomError::CryptoFailure)?;
    getrandom::fill(&mut iv).map_err(|_| RandomError::CryptoFailure)?;
    Ok((hex::encode(key), hex::encode(iv)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(kind: RandomKind, length: usize, count: usize) -> RandomParams {
        RandomParams {
            kind,
            length,
            count,
            uppercase: false,
            charset: CharsetOptions::password(),
        }
    }

    fn generate_one(kind: RandomKind, length: usize) -> String {
        let result = generate(&params(kind, length, 1)).expect("生成失败");
        assert_eq!(result.rows.len(), 1);
        result.rows[0].value.clone()
    }

    #[test]
    fn test_uuid_shape() {
        let value = generate_one(RandomKind::Uuid, 36);
        assert_eq!(value.len(), 36);
        let groups: Vec<&str> = value.split('-').collect();
        assert_eq!(groups.len(), 5);
        assert_eq!(
            groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        // 版本位固定 4；变体位 8/9/a/b
        assert_eq!(&value[14..15], "4");
        assert!(matches!(&value[19..20], "8" | "9" | "a" | "b"));
        assert!(value.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_uuid_simple_shape() {
        let value = generate_one(RandomKind::UuidSimple, 32);
        assert_eq!(value.len(), 32);
        assert!(!value.contains('-'));
        assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
        // 与标准 UUID 相同：版本位/变体位
        assert_eq!(&value[12..13], "4");
        assert!(matches!(&value[16..17], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn test_hex_length_and_case() {
        let lower = generate_one(RandomKind::Hex, 64);
        assert_eq!(lower.len(), 64);
        assert!(lower.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!lower.chars().any(|c| c.is_ascii_uppercase()));

        let mut upper_params = params(RandomKind::Hex, 64, 1);
        upper_params.uppercase = true;
        let upper = generate(&upper_params).unwrap().rows[0].value.clone();
        assert_eq!(upper.len(), 64);
        assert!(upper.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!upper.chars().any(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn test_decimal_charset() {
        let value = generate_one(RandomKind::Decimal, 32);
        assert_eq!(value.len(), 32);
        assert!(value.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_custom_guarantees_selected_classes() {
        let mut p = params(RandomKind::Custom, 64, 1);
        p.charset = CharsetOptions {
            digits: true,
            lower: true,
            upper: false,
            symbols: false,
            exclude_ambiguous: true,
        };
        let value = generate(&p).unwrap().rows[0].value.clone();
        assert_eq!(value.len(), 64);
        let allowed: Vec<char> = "23456789abcdefghijkmnopqrstuvwxyz".chars().collect();
        assert!(value.chars().all(|c| allowed.contains(&c)));
        assert!(value.chars().any(|c| c.is_ascii_digit()));
        assert!(value.chars().any(|c| c.is_ascii_alphabetic()));
        // 排除易混淆：0/1 不应出现
        assert!(!value.contains('0'));
        assert!(!value.contains('1'));
    }

    #[test]
    fn test_custom_short_length_still_in_charset() {
        // 长度小于类别数：不保证每类都有，但字符必须全部来自所选并集
        let mut p = params(RandomKind::Custom, 2, 1);
        p.charset = CharsetOptions {
            digits: true,
            lower: true,
            upper: true,
            symbols: true,
            exclude_ambiguous: false,
        };
        let value = generate(&p).unwrap().rows[0].value.clone();
        assert_eq!(value.chars().count(), 2);
        let union = p.charset.effective_charset().unwrap();
        assert!(value.chars().all(|c| union.contains(&c)));
    }

    #[test]
    fn test_count_and_flat() {
        let result = generate(&params(RandomKind::UuidSimple, 32, 5)).unwrap();
        assert_eq!(result.count, 5);
        assert_eq!(result.rows.len(), 5);
        for (i, row) in result.rows.iter().enumerate() {
            assert_eq!(row.group, i);
            assert_eq!(row.label, RowLabel::None);
        }
        assert_eq!(result.flat.lines().count(), 5);
        assert_eq!(
            result.flat.lines().collect::<Vec<_>>(),
            result
                .rows
                .iter()
                .map(|row| row.value.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_invalid_params() {
        // 长度越界
        assert_eq!(
            generate(&params(RandomKind::Hex, 0, 1)).unwrap_err(),
            RandomError::InvalidLength
        );
        assert_eq!(
            generate(&params(RandomKind::Hex, MAX_LENGTH + 1, 1)).unwrap_err(),
            RandomError::InvalidLength
        );
        // 数量越界
        assert_eq!(
            generate(&params(RandomKind::Hex, 32, 0)).unwrap_err(),
            RandomError::InvalidCount
        );
        assert_eq!(
            generate(&params(RandomKind::Hex, 32, MAX_COUNT + 1)).unwrap_err(),
            RandomError::InvalidCount
        );
        // 自定义字符集全不选
        let mut p = params(RandomKind::Custom, 16, 1);
        p.charset = CharsetOptions {
            digits: false,
            lower: false,
            upper: false,
            symbols: false,
            exclude_ambiguous: true,
        };
        assert_eq!(generate(&p).unwrap_err(), RandomError::EmptyCharset);
    }

    #[test]
    fn test_fixed_kind_ignores_length() {
        // 定长类型不校验 length（UI 禁用长度输入）
        assert!(generate(&params(RandomKind::Uuid, 0, 1)).is_ok());
        assert!(generate(&params(RandomKind::Sm4, 0, 1)).is_ok());
    }

    #[test]
    fn test_sm2_keypair_shape() {
        let result = generate(&params(RandomKind::Sm2, 64, 2)).unwrap();
        assert_eq!(result.count, 2);
        assert_eq!(result.rows.len(), 4);
        for pair in result.rows.chunks(2) {
            assert_eq!(pair[0].label, RowLabel::PrivateKey);
            assert_eq!(pair[1].label, RowLabel::PublicKey);
            assert_eq!(pair[0].value.len(), 64); // 32 字节私钥
            assert_eq!(pair[1].value.len(), 130); // 65 字节未压缩公钥
            assert!(pair[1].value.starts_with("04"));
            assert!(pair[0].value.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(pair[1].value.chars().all(|c| c.is_ascii_hexdigit()));
        }
        // 两次生成不应相同
        assert_ne!(result.rows[0].value, result.rows[2].value);
    }

    #[test]
    fn test_sm4_key_and_iv_shape() {
        let result = generate(&params(RandomKind::Sm4, 32, 2)).unwrap();
        assert_eq!(result.rows.len(), 4);
        for pair in result.rows.chunks(2) {
            assert_eq!(pair[0].label, RowLabel::Key);
            assert_eq!(pair[1].label, RowLabel::Iv);
            assert_eq!(pair[0].value.len(), 32);
            assert_eq!(pair[1].value.len(), 32);
            assert!(pair[0].value.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(pair[1].value.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_ne!(result.rows[0].value, result.rows[2].value);
        assert_ne!(result.rows[0].value, result.rows[1].value);
    }
}
