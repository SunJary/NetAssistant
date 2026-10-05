// 通用消息变量引擎(文本 / hex 通用)
//
// 支持变量(在文本层替换, hex 模式下替换后再 hex_to_bytes):
//   ${seq}            递增序号(由调用方提供计数器)
//   ${worker_id}      当前 worker 编号(压测专属, 普通消息上下文传 None → 原样保留)
//   ${counter}        当前 worker 本地计数(压测专属, 同上)
//   ${timestamp}      当前毫秒时间戳(Unix epoch, 既有语义不变)
//   ${timestamp_s}    当前秒时间戳(Unix epoch)
//   ${date}           本地日期 %Y-%m-%d
//   ${time}           本地时间 %H:%M:%S
//   ${datetime}       本地日期时间 %Y-%m-%d %H:%M:%S
//   ${datetime_ms}    本地日期时间(含毫秒)
//   ${iso}            RFC3339(带时区偏移)
//   ${utc}            UTC ISO 8601(以 Z 结尾)
//   ${time:格式}      任意 strftime 格式(格式非法则原样保留)
//   ${uuid}           随机 UUID v4(既有语义不变)
//   ${random:min:max} [min,max] 闭区间随机整数(既有语义不变)
//
// 未知变量(如 ${foo})原样保留; 无 } 结尾的 ${ 也原样保留。
//
// hex 模式下:
//   - 数值变量(seq/worker_id/counter/timestamp/timestamp_s/random)输出零填充偶数长度十六进制
//   - uuid 输出 32 字符无连字符十六进制
//   - 文本类变量(时间/日期等)按 UTF-8 字节输出为大写十六进制
//
// 一条消息内所有时间变量共用同一个 `now`, 避免 ${datetime} 与 ${timestamp} 跨毫秒不一致。

use chrono::{DateTime, Local, SecondsFormat, Utc};
use log::warn;
use std::sync::Arc;
use uuid::Uuid;

use crate::core::checksum::ChecksumAlgorithm;
use crate::reply::frame::{ByteView, RxContext, RxFrame};

/// 单次渲染的上下文
///
/// `worker_id` / `counter` / `seq` 用 `Option` 表达"当前场景是否适用":
/// - 普通消息: 三者均可为 None, 对应变量按"未知变量"原样保留(不误导成 0)
/// - 压测: 三者均为 Some, 行为与既有实现逐字一致
///
/// `rx` 是本轮新增的**接收帧上下文**: 普通发送/压测为 None(既有调用点一行不用改),
/// 规则应答为 Some(见 `RenderContext::for_reply`)。用 `Option<Arc<RxFrame>>` 而非
/// 新参数, 是为了让 `render()` 签名完全不变 —— 这是"零破坏"的关键
/// (决策见 docs/plan-reply-rules-expr.md §思路)。
#[derive(Debug, Clone)]
pub struct RenderContext {
    /// 同一条消息共享的时间基准
    pub now: DateTime<Local>,
    /// 压测 worker 编号(普通消息传 None → ${worker_id} 原样保留)
    pub worker_id: Option<usize>,
    /// 压测 worker 本地计数(普通消息传 None → ${counter} 原样保留)
    pub counter: Option<u64>,
    /// 递增序号(不消费序号时传 None; 与 CompiledTemplate::needs_seq 配合)
    pub seq: Option<u64>,
    /// 接收帧上下文(普通发送为 None, 规则应答为 Some)
    pub rx: Option<Arc<RxFrame>>,
}

impl RenderContext {
    /// 构造普通消息上下文: 取一次当前时间, 压测专属变量不适用
    ///
    /// `seq` 仅在模板含 `${seq}` 时由调用方 `fetch_add` 后传入。
    pub fn common(seq: Option<u64>) -> Self {
        Self {
            now: Local::now(),
            worker_id: None,
            counter: None,
            seq,
            rx: None,
        }
    }

    /// 压测 worker 上下文(含 worker_id / counter), 既有行为逐字保留
    pub fn for_worker(
        now: DateTime<Local>,
        worker_id: Option<usize>,
        counter: Option<u64>,
        seq: Option<u64>,
    ) -> Self {
        Self {
            now,
            worker_id,
            counter,
            seq,
            rx: None,
        }
    }

    /// 规则应答上下文: 带接收帧快照
    ///
    /// 传 `Arc` 而非引用: `render` 可能被跨线程调用, 且调试面板需要长期持有帧快照。
    pub fn for_reply(seq: Option<u64>, frame: Arc<RxFrame>) -> Self {
        Self {
            rx: Some(frame),
            ..Self::common(seq)
        }
    }
}

/// 预编译的模板段
#[derive(Debug, Clone, PartialEq)]
pub enum VarSegment {
    /// 字面量文本
    Literal(String),
    /// ${timestamp} Unix 毫秒
    Timestamp,
    /// ${timestamp_s} Unix 秒
    TimestampSecs,
    /// ${date} 本地日期
    Date,
    /// ${time} 本地时间
    Time,
    /// ${datetime} 本地日期时间
    DateTime,
    /// ${datetime_ms} 本地日期时间(含毫秒)
    DateTimeMs,
    /// ${iso} RFC3339(带时区偏移)
    Iso,
    /// ${utc} UTC ISO 8601
    Utc,
    /// ${time:格式} 任意 strftime(已预检合法)
    TimeFormat(String),
    /// ${uuid} UUID v4
    Uuid,
    /// ${random:min:max} 预解析的闭区间
    Random(i64, i64),
    /// ${seq}
    Seq,
    /// ${worker_id}
    WorkerId,
    /// ${counter}
    Counter,
    /// ${rx.<accessor>:off[:len]} 接收帧标量/字节区间取值
    RxAccess {
        accessor: String,
        offset: usize,
        len: Option<usize>,
        /// 原始变量名(如 `rx.raw:0:2`): 无上下文时按**原样**保留,
        /// 而不是用解析后的字段重新拼写 —— 否则 `${rx.raw}` 会变成 `${rx.raw:0}`,
        /// 用户看到的"未生效提示"与他写的不一致。
        source: String,
    },
    /// ${rx.len} / ${rx.src} / ${rx.src_ip} / ${rx.port}
    RxMeta(RxMetaKind),
    /// ${<算法>:off:len[:le]} 生成型校验, 作用于**模板自身已渲染出的字节**
    ///
    /// 这是 F-32「发送前自动填充校验位」的核心: 发送框里写
    /// `01 03 00 00 00 02 ${crc16modbus:0:6:le}` 即可自动算出 `C4 0B`。
    GenChecksum {
        algorithm: ChecksumAlgorithm,
        offset: usize,
        len: usize,
        little: bool,
    },
    /// ${= 表达式 } 单表达式(见 src/reply/expr.rs)
    Expr(Box<crate::reply::expr::Expr>),
    /// 未知变量 / 非法格式, 原样保留 ${name}
    Unknown(String),
}

/// `${rx.*}` 的元信息取值种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxMetaKind {
    /// 帧总字节数
    Len,
    /// 来源地址 IP:port
    Src,
    /// 来源 IP
    SrcIp,
    /// 来源端口
    Port,
}

impl RxMetaKind {
    pub fn name(self) -> &'static str {
        match self {
            RxMetaKind::Len => "len",
            RxMetaKind::Src => "src",
            RxMetaKind::SrcIp => "src_ip",
            RxMetaKind::Port => "port",
        }
    }
}

/// 预编译的模板
///
/// 解析一次, 拆分为段(Literal / 变量), 避免每包重复执行 `char_indices().collect()`
/// 和字符串搜索。
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledTemplate {
    segments: Vec<VarSegment>,
    /// 原始模板长度(用于调用方预分配输出缓冲)
    template_len: usize,
}

impl CompiledTemplate {
    /// 从模板字符串构造预编译模板
    pub fn new(template: &str) -> Self {
        let template_len = template.len();
        // 快速路径: 无变量
        if !template.contains("${") {
            return Self {
                segments: vec![VarSegment::Literal(template.to_string())],
                template_len,
            };
        }

        let chars: Vec<(usize, char)> = template.char_indices().collect();
        let mut segments = Vec::new();
        let mut ci = 0;
        let mut literal_start = 0;

        while ci < chars.len() {
            let (_, ch) = chars[ci];
            if ch == '$' && ci + 1 < chars.len() && chars[ci + 1].1 == '{' {
                let after_brace_byte = chars[ci + 1].0 + chars[ci + 1].1.len_utf8();
                if let Some(close_rel) = template[after_brace_byte..].find('}') {
                    // 先冲刷已积累的字面量
                    if literal_start < chars[ci].0 {
                        segments.push(VarSegment::Literal(
                            template[literal_start..chars[ci].0].to_string(),
                        ));
                    }
                    let var_name = &template[after_brace_byte..after_brace_byte + close_rel];
                    segments.push(parse_segment(var_name));
                    let close_byte = after_brace_byte + close_rel + 1;
                    literal_start = close_byte;
                    ci = chars.partition_point(|(p, _)| *p < close_byte);
                    continue;
                }
            }
            ci += 1;
        }
        // 冲刷尾部字面量
        if literal_start < template.len() {
            segments.push(VarSegment::Literal(template[literal_start..].to_string()));
        }

        Self {
            segments,
            template_len,
        }
    }

    /// 原始模板长度(用于调用方预分配缓冲)
    pub fn template_len(&self) -> usize {
        self.template_len
    }

    /// 模板是否含 `${seq}`
    ///
    /// 调用方据此决定是否 `fetch_add`, 避免"只发 ${uuid} 也吃掉一个序号"。
    pub fn needs_seq(&self) -> bool {
        self.segments.iter().any(|s| matches!(s, VarSegment::Seq))
    }

    /// 模板是否引用接收帧(`rx.*` / 表达式里的 `rx.*` / 校验函数)
    ///
    /// 规则应答据此省掉一次 `Arc<RxFrame>` 构造与原子递增 —— 不含 `rx.*` 的
    /// 规则在洪泛下完全不付这份成本(决策见 plan-reply-rules-expr.md §4.3)。
    pub fn needs_rx(&self) -> bool {
        self.segments.iter().any(|s| match s {
            VarSegment::RxAccess { .. } | VarSegment::RxMeta(_) => true,
            // 表达式可能引用 rx.* 或校验函数（二者都依赖接收帧）
            VarSegment::Expr(expr) => crate::reply::expr::needs_rx(expr),
            _ => false,
        })
    }

    /// 全部段(只读)。供调试面板列出"这条模板里有哪些变量"。
    pub fn segments(&self) -> &[VarSegment] {
        &self.segments
    }

    /// 渲染到给定的 String 缓冲(调用方负责 clear + 预分配)
    pub fn render(&self, ctx: &RenderContext, hex_mode: bool, out: &mut String) {
        // uuid 每条消息只生成一次(仅当模板含 ${uuid} 时才产生随机数开销)
        let uuid = if self.segments.iter().any(|s| matches!(s, VarSegment::Uuid)) {
            Some(Uuid::new_v4())
        } else {
            None
        };

        for seg in &self.segments {
            match seg {
                VarSegment::Literal(s) => out.push_str(s),
                VarSegment::Timestamp => push_i64(out, ctx.now.timestamp_millis(), hex_mode),
                VarSegment::TimestampSecs => push_i64(out, ctx.now.timestamp(), hex_mode),
                VarSegment::Date => {
                    push_text(out, &ctx.now.format("%Y-%m-%d").to_string(), hex_mode)
                }
                VarSegment::Time => {
                    push_text(out, &ctx.now.format("%H:%M:%S").to_string(), hex_mode)
                }
                VarSegment::DateTime => push_text(
                    out,
                    &ctx.now.format("%Y-%m-%d %H:%M:%S").to_string(),
                    hex_mode,
                ),
                VarSegment::DateTimeMs => push_text(
                    out,
                    &ctx.now.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                    hex_mode,
                ),
                VarSegment::Iso => push_text(
                    out,
                    &ctx.now.to_rfc3339_opts(SecondsFormat::Secs, false),
                    hex_mode,
                ),
                VarSegment::Utc => push_text(
                    out,
                    &ctx.now
                        .with_timezone(&Utc)
                        .to_rfc3339_opts(SecondsFormat::Secs, true),
                    hex_mode,
                ),
                VarSegment::TimeFormat(fmt) => {
                    push_text(out, &ctx.now.format(fmt).to_string(), hex_mode)
                }
                VarSegment::Uuid => {
                    if let Some(uuid) = &uuid {
                        if hex_mode {
                            out.push_str(&uuid.simple().to_string().to_uppercase())
                        } else {
                            out.push_str(&uuid.to_string())
                        }
                    }
                }
                VarSegment::Random(min, max) => {
                    let span = (*max - *min) as u64 + 1;
                    let val = *min + (random_u64() % span) as i64;
                    push_i64(out, val, hex_mode);
                }
                // 上下文不适用的变量按"未知变量"原样保留(不静默渲染成 0)
                VarSegment::Seq => match ctx.seq {
                    Some(v) => push_u64(out, v, hex_mode),
                    None => out.push_str("${seq}"),
                },
                VarSegment::WorkerId => match ctx.worker_id {
                    Some(v) => push_u64(out, v as u64, hex_mode),
                    None => out.push_str("${worker_id}"),
                },
                VarSegment::Counter => match ctx.counter {
                    Some(v) => push_u64(out, v, hex_mode),
                    None => out.push_str("${counter}"),
                },
                // ===== 新增: 接收帧取值 =====
                //
                // 无接收上下文时按"未知变量"原样保留(决策 E-2): 静默渲染成空会掩盖
                // 配置错误(旧发送框里误写 ${rx.x} 会静默发出空内容), 保留字面量让用户
                // 一眼看出"这个变量没生效"。
                VarSegment::RxAccess {
                    accessor,
                    offset,
                    len,
                    source,
                } => match &ctx.rx {
                    None => out.push_str(&format!("${{{}}}", source)),
                    Some(frame) => {
                        let rx = RxContext::new(frame);
                        match resolve_rx_access(&rx, accessor, *offset, *len) {
                            Ok(RxValue::Scalar(v)) => push_i64(out, v, hex_mode),
                            Ok(RxValue::Bytes(b)) => push_bytes(out, &b, hex_mode),
                            // 越界: 渲染为空 + warn(决策 E-3)。
                            // 与 E-2 区分: 上下文不对是配置问题, 越界是"帧比预期短"的数据问题;
                            // 空串让应答帧结构保持(长度位不会多出 rx.u16be:99 这种文本)。
                            Err(e) => warn!("[rx] ${{{}}} 取值失败: {}", source, e.describe()),
                        }
                    }
                },
                VarSegment::RxMeta(kind) => match &ctx.rx {
                    None => out.push_str(&render_source(seg)),
                    Some(frame) => {
                        let rx = RxContext::new(frame);
                        match kind {
                            RxMetaKind::Len => push_u64(out, rx.len() as u64, hex_mode),
                            RxMetaKind::Src => push_text(out, &rx.source(), hex_mode),
                            RxMetaKind::SrcIp => push_text(out, &rx.source_ip(), hex_mode),
                            RxMetaKind::Port => push_u64(out, rx.source_port() as u64, hex_mode),
                        }
                    }
                },
                // ===== 新增: 生成型校验(读 out 自身, 流式语义) =====
                VarSegment::GenChecksum {
                    algorithm,
                    offset,
                    len,
                    little,
                } => {
                    // hex 模式: out 里是大写 hex 字符(可能含空格), 需还原为字节;
                    //           out 是"半成品", 末尾必须落在完整字节边界上 —— 见
                    //           保存期校验的奇偶性检查。
                    // text 模式: out 就是 UTF-8 字节流。
                    let bytes = if hex_mode {
                        crate::utils::hex::hex_to_bytes(out)
                    } else {
                        out.as_bytes().to_vec()
                    };
                    match bytes.get(*offset..offset.saturating_add(*len)) {
                        Some(slice) => {
                            let value = algorithm.compute_with_endian(slice, *little);
                            push_bytes(out, &value, hex_mode);
                        }
                        None => warn!(
                            "[gen] {} 校验区间越界: {}..{} (已渲染 {} 字节)",
                            algorithm.name(),
                            offset,
                            offset + len,
                            bytes.len()
                        ),
                    }
                }
                // ===== 新增: 单表达式 =====
                VarSegment::Expr(expr) => {
                    let rx_ctx = ctx.rx.as_ref().map(|f| RxContext::new(f));
                    if let Err(e) = crate::reply::expr::validate_expr(expr) {
                        warn!("[expr] 表达式不可用: {}", e.describe());
                        out.push_str(&render_source(seg));
                        continue;
                    }
                    match crate::reply::expr::eval(expr, rx_ctx.as_ref()) {
                        Ok(v) => push_i64(out, v, hex_mode),
                        Err(e) => warn!("[expr] 求值失败: {}", e.describe()),
                    }
                }
                VarSegment::Unknown(s) => out.push_str(s),
            }
        }
    }
}

/// `rx.*` 取值的结果(标量或字节区间)
enum RxValue {
    Scalar(i64),
    Bytes(Vec<u8>),
}

/// 解析一个 `rx.<accessor>` 取值
fn resolve_rx_access(
    rx: &RxContext<'_>,
    accessor: &str,
    offset: usize,
    len: Option<usize>,
) -> Result<RxValue, crate::reply::frame::RxError> {
    // 校验取值(由 parse_rx_segment 编码为 `__checksum:<算法>:<字节序>`)
    if let Some(spec) = accessor.strip_prefix("__checksum:") {
        let mut fields = spec.split(':');
        let algorithm = fields.next().unwrap_or_default();
        let little = fields.next() == Some("le");
        return rx
            .checksum_by_name(algorithm, offset, len.unwrap_or(0), little)
            .map(RxValue::Bytes);
    }
    // 标量访问标识(不含冒号)走标量路径; 其余走字节区间路径
    if crate::reply::frame::parse_scalar_accessor(accessor).is_some() {
        return rx.get_scalar(accessor, offset).map(RxValue::Scalar);
    }
    match ByteView::parse(accessor) {
        Some(view) => rx.get_bytes(view, offset, len).map(RxValue::Bytes),
        None => Err(crate::reply::frame::RxError::UnknownAccessor(
            accessor.to_string(),
        )),
    }
}

/// 字节区间输出: hex 模式下已经是 hex 文本(rx.hex)或原始字节, 需分别处理
fn push_bytes(out: &mut String, bytes: &[u8], hex_mode: bool) {
    if hex_mode {
        // 生成型校验与校验取值给的是真实字节 → 输出为无分隔大写 hex
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for b in bytes {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    } else {
        // 文本模式按 Latin-1 直出: 保证 raw 变量的目标是"字节级原样回显"
        // (用户若想构造应答帧应当用 hex 模式 + ${rx.raw})
        for &b in bytes {
            out.push(b as char);
        }
    }
}

/// 把一个变量段还原为源码写法(日志诊断用)
fn render_source(seg: &VarSegment) -> String {
    match seg {
        VarSegment::RxAccess { source, .. } => format!("${{{}}}", source),
        VarSegment::RxMeta(kind) => format!("${{rx.{}}}", kind.name()),
        VarSegment::GenChecksum {
            algorithm,
            offset,
            len,
            little,
        } => {
            let suffix = if *little { ":le" } else { "" };
            format!(
                "${{{algo}:{offset}:{len}{suffix}}}",
                algo = algorithm.compact_name()
            )
        }
        VarSegment::Expr(_) => "${= ... }".to_string(),
        VarSegment::Unknown(s) => s.clone(),
        other => format!("{:?}", other),
    }
}

/// 解析 `${rx.<accessor>[:off[:len]]}`
fn parse_rx_segment(var_name: &str) -> VarSegment {
    let body = &var_name["rx.".len()..];
    let parts: Vec<&str> = body.split(':').map(|p| p.trim()).collect();
    let accessor = parts[0];

    // 元信息: 不带参数
    if parts.len() == 1 {
        match accessor {
            "len" => return VarSegment::RxMeta(RxMetaKind::Len),
            "src" => return VarSegment::RxMeta(RxMetaKind::Src),
            "src_ip" => return VarSegment::RxMeta(RxMetaKind::SrcIp),
            "port" => return VarSegment::RxMeta(RxMetaKind::Port),
            "" => return VarSegment::Unknown(format!("${{{}}}", var_name)),
            _ => {}
        }
    }

    // 校验取值: ${rx.crc16modbus:off:len[:le]}
    if let Some(algo) = ChecksumAlgorithm::parse(accessor) {
        if parts.len() < 3 || parts.len() > 4 {
            return VarSegment::Unknown(format!("${{{}}}", var_name));
        }
        let (Ok(offset), Ok(len)) = (parts[1].parse::<usize>(), parts[2].parse::<usize>()) else {
            return VarSegment::Unknown(format!("${{{}}}", var_name));
        };
        let little = match parts.get(3) {
            None => false,
            Some(&"le") => true,
            Some(&"be") => false,
            Some(_) => return VarSegment::Unknown(format!("${{{}}}", var_name)),
        };
        return VarSegment::RxAccess {
            accessor: format!(
                "__checksum:{}:{}",
                algo.name(),
                if little { "le" } else { "be" }
            ),
            offset,
            len: Some(len),
            source: var_name.to_string(),
        };
    }

    // 标量 / 字节区间取值
    let scalar = crate::reply::frame::parse_scalar_accessor(accessor).is_some();
    let view = ByteView::parse(accessor).is_some();
    if !scalar && !view {
        return VarSegment::Unknown(format!("${{{}}}", var_name));
    }
    let offset = match parts.get(1) {
        None | Some(&"") => 0,
        Some(text) => match text.parse::<usize>() {
            Ok(v) => v,
            Err(_) => return VarSegment::Unknown(format!("${{{}}}", var_name)),
        },
    };
    let len = match parts.get(2) {
        None | Some(&"") => None,
        Some(text) => match text.parse::<usize>() {
            Ok(v) => Some(v),
            Err(_) => return VarSegment::Unknown(format!("${{{}}}", var_name)),
        },
    };
    if parts.len() > 3 {
        return VarSegment::Unknown(format!("${{{}}}", var_name));
    }
    // 标量取值不接受 len
    if scalar && len.is_some() {
        return VarSegment::Unknown(format!("${{{}}}", var_name));
    }
    VarSegment::RxAccess {
        accessor: accessor.to_string(),
        offset,
        len,
        source: var_name.to_string(),
    }
}

/// 解析生成型校验变量 `${<算法>:off:len[:le]}`
fn parse_gen_checksum(var_name: &str) -> Option<VarSegment> {
    let parts: Vec<&str> = var_name.split(':').map(|p| p.trim()).collect();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let algorithm = ChecksumAlgorithm::parse(parts[0])?;
    let offset = parts[1].parse::<usize>().ok()?;
    let len = parts[2].parse::<usize>().ok()?;
    let little = match parts.get(3) {
        None => false,
        Some(&"le") => true,
        Some(&"be") => false,
        Some(_) => return None,
    };
    Some(VarSegment::GenChecksum {
        algorithm,
        offset,
        len,
        little,
    })
}

/// 将变量名解析为预编译段
///
/// 解析顺序: 既有精确名 → `${= 表达式 }` → `${rx.*}` → 生成型校验 → `${time:}` /
/// `${random:}` → 未知变量原样保留。
///
/// **既有变量的语义一个都不改** —— 这是"零破坏"的硬标准(既有 30+ 个测试
/// 一行不改即通过)。
fn parse_segment(var_name: &str) -> VarSegment {
    match var_name {
        "seq" => VarSegment::Seq,
        "worker_id" => VarSegment::WorkerId,
        "counter" => VarSegment::Counter,
        "timestamp" => VarSegment::Timestamp,
        "timestamp_s" => VarSegment::TimestampSecs,
        "date" => VarSegment::Date,
        "time" => VarSegment::Time,
        "datetime" => VarSegment::DateTime,
        "datetime_ms" => VarSegment::DateTimeMs,
        "iso" => VarSegment::Iso,
        "utc" => VarSegment::Utc,
        "uuid" => VarSegment::Uuid,
        // ${= 表达式 }: 显式 `=` 前缀避免与"变量名含冒号"歧义(决策 E-8)
        _ if var_name.starts_with('=') => {
            let source = var_name[1..].trim();
            match crate::reply::expr::parse(source) {
                Ok(expr) => match crate::reply::expr::validate_expr(&expr) {
                    Ok(()) => VarSegment::Expr(Box::new(expr)),
                    Err(_) => VarSegment::Unknown(format!("${{{}}}", var_name)),
                },
                Err(_) => VarSegment::Unknown(format!("${{{}}}", var_name)),
            }
        }
        // ${rx.*}
        _ if var_name.starts_with("rx.") => parse_rx_segment(var_name),
        _ if var_name.starts_with("time:") => {
            let fmt = &var_name["time:".len()..];
            // 空格式按 ${time} 默认处理
            if fmt.is_empty() {
                return VarSegment::Time;
            }
            if is_valid_strftime(fmt) {
                VarSegment::TimeFormat(fmt.to_string())
            } else {
                VarSegment::Unknown(format!("${{{}}}", var_name))
            }
        }
        _ if var_name.starts_with("random:") => {
            let params = &var_name["random:".len()..];
            let parts: Vec<&str> = params.split(':').collect();
            if parts.len() != 2 {
                return VarSegment::Unknown(format!("${{{}}}", var_name));
            }
            match (
                parts[0].trim().parse::<i64>(),
                parts[1].trim().parse::<i64>(),
            ) {
                (Ok(min), Ok(max)) if min <= max => VarSegment::Random(min, max),
                _ => VarSegment::Unknown(format!("${{{}}}", var_name)),
            }
        }
        _ => match parse_gen_checksum(var_name) {
            Some(seg) => seg,
            None => VarSegment::Unknown(format!("${{{}}}", var_name)),
        },
    }
}

/// 预检 strftime 格式串是否合法
///
/// chrono 的 `DelayedFormat` 对非法指示符(如 `%Q`)会在格式化时 panic,
/// 因此必须在编译期用 `StrftimeItems` 预检, 命中 `Item::Error` 即视为未知变量原样保留。
fn is_valid_strftime(fmt: &str) -> bool {
    use chrono::format::{Item, StrftimeItems};
    !StrftimeItems::new(fmt).any(|item| matches!(item, Item::Error))
}

/// 追加数值: hex 模式输出偶数长度大写十六进制, 否则十进制文本
fn push_u64(out: &mut String, v: u64, hex_mode: bool) {
    if hex_mode {
        out.push_str(&format_hex_u64(v))
    } else {
        out.push_str(&v.to_string())
    }
}

fn push_i64(out: &mut String, v: i64, hex_mode: bool) {
    push_u64(out, v as u64, hex_mode)
}

/// 追加文本: hex 模式按 UTF-8 字节输出为大写十六进制, 否则原样文本
fn push_text(out: &mut String, text: &str, hex_mode: bool) {
    if hex_mode {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for b in text.as_bytes() {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    } else {
        out.push_str(text);
    }
}

/// 将 u64 格式化为偶数长度大写十六进制字符串(如 0→"00", 10→"0A", 256→"0100")
fn format_hex_u64(v: u64) -> String {
    let hex = format!("{:X}", v);
    if hex.len() % 2 != 0 {
        format!("0{}", hex)
    } else {
        hex
    }
}

/// 用 std RandomState 产生一个伪随机 u64
///
/// 非加密用途(仅为报文变化), 避免引入 rand 依赖。
fn random_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_text(template: &str, ctx: &RenderContext) -> String {
        let compiled = CompiledTemplate::new(template);
        let mut out = String::new();
        compiled.render(ctx, false, &mut out);
        out
    }

    fn render_hex(template: &str, ctx: &RenderContext) -> String {
        let compiled = CompiledTemplate::new(template);
        let mut out = String::new();
        compiled.render(ctx, true, &mut out);
        out
    }

    fn common_ctx() -> RenderContext {
        RenderContext::common(Some(0))
    }

    #[test]
    fn test_no_variable_fast_path() {
        let compiled = CompiledTemplate::new("hello world");
        assert!(!compiled.needs_seq());
        assert_eq!(render_text("hello world", &common_ctx()), "hello world");
    }

    #[test]
    fn test_seq_needs_seq_and_render() {
        let compiled = CompiledTemplate::new("req-${seq}");
        assert!(compiled.needs_seq());
        assert_eq!(
            render_text("req-${seq}", &RenderContext::common(Some(0))),
            "req-0"
        );
        assert_eq!(
            render_text("req-${seq}", &RenderContext::common(Some(1))),
            "req-1"
        );
        // 不含 ${seq} 的模板不消费序号
        assert!(!CompiledTemplate::new("id=${uuid}").needs_seq());
    }

    #[test]
    fn test_seq_none_preserved() {
        // 传 None 时 ${seq} 原样保留(不静默渲染成 0)
        assert_eq!(
            render_text("${seq}", &RenderContext::common(None)),
            "${seq}"
        );
    }

    #[test]
    fn test_worker_id_and_counter() {
        let ctx = RenderContext {
            now: Local::now(),
            worker_id: Some(7),
            counter: Some(1),
            seq: Some(0),
            rx: None,
        };
        assert_eq!(render_text("w${worker_id}-c${counter}", &ctx), "w7-c1");
    }

    #[test]
    fn test_worker_id_counter_none_preserved() {
        // 普通消息上下文: 压测专属变量原样保留, 不渲染成 0
        assert_eq!(
            render_text("${worker_id}${counter}", &common_ctx()),
            "${worker_id}${counter}"
        );
    }

    #[test]
    fn test_timestamp_and_secs_share_now() {
        let ctx = common_ctx();
        let out = render_text("${timestamp}|${timestamp_s}", &ctx);
        let (ms, secs) = out.split_once('|').unwrap();
        assert_eq!(
            ms.parse::<i64>().unwrap() / 1000,
            secs.parse::<i64>().unwrap()
        );
    }

    #[test]
    fn test_time_presets_shape() {
        let ctx = common_ctx();
        assert_eq!(render_text("${date}", &ctx).len(), 10);
        assert_eq!(render_text("${time}", &ctx).len(), 8);
        assert_eq!(render_text("${datetime}", &ctx).len(), 19);
        assert!(render_text("${datetime_ms}", &ctx).len() >= 23);
        assert!(
            render_text("${iso}", &ctx).ends_with("+08:00")
                || render_text("${iso}", &ctx).contains('T')
        );
        assert!(render_text("${utc}", &ctx).ends_with('Z'));
    }

    #[test]
    fn test_time_custom_format() {
        let ctx = RenderContext::common(Some(0));
        let out = render_text("${time:%Y/%m/%d}", &ctx);
        // 与 ${date} 的连字符格式对照: 分隔符应被替换为 /
        assert_eq!(out.len(), 10);
        assert!(out.contains('/'));
        // 空格式回退到 ${time} 默认
        assert_eq!(render_text("${time:}", &ctx).len(), 8);
    }

    #[test]
    fn test_invalid_time_format_preserved() {
        // 非法指示符(%Q)必须原样保留而非 panic
        let ctx = common_ctx();
        assert_eq!(render_text("${time:%Q}", &ctx), "${time:%Q}");
    }

    #[test]
    fn test_text_var_hex_encoding() {
        // hex 模式下文本类变量按 UTF-8 字节输出为大写 hex
        let ctx = RenderContext {
            now: Local::now(),
            worker_id: None,
            counter: None,
            seq: None,
            rx: None,
        };
        let out = render_hex("${date}", &ctx);
        // "YYYY-MM-DD" → 10 字节 → 20 个 hex 字符
        assert_eq!(out.len(), 20);
        assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(out, out.to_uppercase());
    }

    #[test]
    fn test_uuid_is_valid_format() {
        let out = render_text("id=${uuid}", &common_ctx());
        assert!(Uuid::parse_str(&out[3..]).is_ok(), "应生成合法 UUID");
    }

    #[test]
    fn test_uuid_hex_no_hyphens() {
        // hex 模式下 uuid 输出为纯 32 字符十六进制, 可被 hex_to_bytes 解析为 16 字节
        let out = render_hex("0000${uuid}", &common_ctx());
        let uuid_hex = &out[4..];
        assert_eq!(uuid_hex.len(), 32, "uuid hex 应为 32 字符: {}", uuid_hex);
        assert!(uuid_hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!uuid_hex.contains('-'));
        let bytes = crate::utils::hex::hex_to_bytes(&out);
        assert_eq!(bytes.len(), 18, "前缀 2 字节 + uuid 16 字节");
    }

    #[test]
    fn test_uuid_text_keeps_hyphens() {
        let out = render_text("${uuid}", &common_ctx());
        assert!(out.contains('-'), "文本模式 uuid 应保留连字符: {}", out);
    }

    #[test]
    fn test_random_in_range() {
        for _ in 0..100 {
            let out = render_text("${random:1:10}", &common_ctx());
            let n: i64 = out.parse().unwrap();
            assert!((1..=10).contains(&n));
        }
    }

    #[test]
    fn test_random_equal_min_max() {
        assert_eq!(render_text("${random:5:5}", &common_ctx()), "5");
    }

    #[test]
    fn test_unknown_variable_preserved() {
        assert_eq!(
            render_text("v=${unknown_var}", &common_ctx()),
            "v=${unknown_var}"
        );
    }

    #[test]
    fn test_malformed_random_preserved() {
        let ctx = common_ctx();
        assert_eq!(render_text("${random:abc:5}", &ctx), "${random:abc:5}");
        assert_eq!(render_text("${random:1}", &ctx), "${random:1}");
        assert_eq!(render_text("${random:5:1}", &ctx), "${random:5:1}");
    }

    #[test]
    fn test_unclosed_brace_preserved() {
        assert_eq!(render_text("x=${seq y", &common_ctx()), "x=${seq y");
    }

    #[test]
    fn test_hex_numeric_even_length() {
        let ctx = RenderContext {
            now: Local::now(),
            worker_id: Some(12),
            counter: None,
            seq: Some(0),
            rx: None,
        };
        let out = render_hex("4142${worker_id}${seq}", &ctx);
        assert_eq!(out, "41420C00");
        assert_eq!(
            crate::utils::hex::hex_to_bytes(&out),
            vec![0x41, 0x42, 0x0C, 0x00]
        );
    }

    #[test]
    fn test_hex_seq_values() {
        for (v, expected) in [(10u64, "0A"), (255, "FF"), (256, "0100")] {
            let ctx = RenderContext::common(Some(v));
            assert_eq!(render_hex("${seq}", &ctx), expected);
        }
    }

    #[test]
    fn test_hex_random_even_length() {
        for _ in 0..100 {
            let out = render_hex("${random:0:255}", &common_ctx());
            assert_eq!(out.len(), 2, "0-255 hex 应为 2 字符: {}", out);
            assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn test_multibyte_literal_safe() {
        // 模板含中文等多字节字符时按 char 边界解析, 不 panic 且字面量完整保留
        let out = render_text("中文-${random:1:1}-文", &common_ctx());
        assert_eq!(out, "中文-1-文");
    }

    // ========================================================================
    // 本轮新增: ${rx.*} / 生成型校验 / 表达式
    // ========================================================================

    /// 构造带接收帧的渲染上下文(规则应答侧)
    fn reply_ctx(bytes: &[u8], seq: Option<u64>) -> RenderContext {
        RenderContext::for_reply(seq, crate::reply::RxFrame::for_test(bytes.to_vec()))
    }

    /// `${rx.*}` 标量取值(与 `${seq}` 同构: text 十进制 / hex 偶数长度大写)
    #[test]
    fn test_rx_scalar_values() {
        let ctx = reply_ctx(&[0x01, 0x03, 0x00, 0x02], Some(0));
        assert_eq!(render_text("${rx.u8:0}", &ctx), "1");
        assert_eq!(render_text("${rx.u8:3}", &ctx), "2");
        assert_eq!(render_text("${rx.u16be:2}", &ctx), "2");
        assert_eq!(render_text("${rx.u16le:0}", &ctx), "769");
        assert_eq!(render_hex("${rx.u8:0}", &ctx), "01");
        // 数值变量的 hex 输出是"最短偶数长度"，不按位宽补零（与既有 ${seq} 一致）
        assert_eq!(render_hex("${rx.u16be:2}", &ctx), "02");
        assert_eq!(render_hex("${rx.u16be:0}", &ctx), "0103");
        // 偏移省略 = 0
        assert_eq!(render_text("${rx.u16be}", &ctx), "259");
    }

    /// `${rx}` 元信息
    #[test]
    fn test_rx_meta_values() {
        let ctx = reply_ctx(&[0xAA, 0xBB], None);
        assert_eq!(render_text("${rx.len}", &ctx), "2");
        assert_eq!(render_hex("${rx.len}", &ctx), "02");
        // RxFrame::for_test 的来源是 127.0.0.1:12345
        assert_eq!(render_text("${rx.src}", &ctx), "127.0.0.1:12345");
        assert_eq!(render_text("${rx.src_ip}", &ctx), "127.0.0.1");
        assert_eq!(render_text("${rx.port}", &ctx), "12345");
        assert_eq!(render_hex("${rx.port}", &ctx), "3039");
    }

    /// `${rx.raw}` / `hex` / `ascii` / `base64` 四种字节视图
    #[test]
    fn test_rx_byte_views() {
        let ctx = reply_ctx(b"AT+CSQ", None);
        // 文本模式: raw 按 Latin-1 直出(保证"字节级原样回显")
        assert_eq!(render_text("${rx.raw}", &ctx), "AT+CSQ");
        assert_eq!(render_text("${rx.raw:0:2}", &ctx), "AT");
        // hex 模式: raw 输出无分隔大写 hex
        assert_eq!(render_hex("${rx.raw:0:2}", &ctx), "4154");
        // hex 视图在两种模式下都是大写 hex 文本(hex 模式再编码一次)
        assert_eq!(render_text("${rx.hex:0:2}", &ctx), "41 54");
        assert_eq!(render_text("${rx.ascii:0:6}", &ctx), "AT+CSQ");
        assert_eq!(render_text("${rx.base64:0:3}", &ctx), "QVQr");
        // 到帧尾
        assert_eq!(render_text("${rx.raw:3}", &ctx), "CSQ");
    }

    /// `${rx.<算法>:off:len[:le]}` 作用于**接收帧**
    #[test]
    fn test_rx_checksum_variables() {
        // Modbus 读保持寄存器请求帧
        let ctx = reply_ctx(&[0x01, 0x03, 0x00, 0x00, 0x00, 0x02, 0xC4, 0x0B], None);
        assert_eq!(render_hex("${rx.crc16modbus:0:6}", &ctx), "0BC4");
        assert_eq!(render_hex("${rx.crc16modbus:0:6:le}", &ctx), "C40B");
        assert_eq!(render_hex("${rx.xor:0:3}", &ctx), "02");
        assert_eq!(render_hex("${rx.sum8:0:3}", &ctx), "04");
        assert_eq!(render_hex("${rx.lrc:0:2}", &ctx), "FC");
        assert_eq!(render_hex("${rx.crc32:0:6}", &ctx).len(), 8);
    }

    /// 生成型校验: `${crc16modbus:off:len:le}` 作用于**模板自身已渲染出的字节**
    ///
    /// 这是 F-32「发送前自动填充校验位」的核心用法 —— 用户不需要手算 CRC 再粘贴。
    /// 注意模板的字面量（含空格）原样保留，校验字节追加在末尾。
    #[test]
    fn test_generated_checksum_modbus_rtu_frame() {
        let ctx = common_ctx();
        // Modbus RTU 读保持寄存器: 6 字节头 + CRC(小端)
        assert_eq!(
            render_hex("01 03 00 00 00 02 ${crc16modbus:0:6:le}", &ctx),
            "01 03 00 00 00 02 C40B",
            "必须与手算的 Modbus CRC 一致"
        );
        // 默认大端
        assert_eq!(
            render_hex("01 03 00 00 00 02 ${crc16modbus:0:6}", &ctx),
            "01 03 00 00 00 02 0BC4"
        );
        // 无分隔写法（不变量 2: hex 解析对空白与大小写容错）
        assert_eq!(
            render_hex("010300000002${crc16modbus:0:6:le}", &ctx),
            "010300000002C40B"
        );
        // 文字面量的空格不影响字节解析（不变量 2）
        assert_eq!(
            render_hex("01 03 00 00 00 02${crc16modbus:0:6:le}", &ctx),
            "01 03 00 00 00 02C40B"
        );
        // 累积和 / 异或 / LRC
        assert_eq!(render_hex("010300${sum8:0:3}", &ctx), "01030004");
        assert_eq!(render_hex("010300${xor:0:3}", &ctx), "01030002");
        assert_eq!(render_hex("0103${lrc:0:2}", &ctx), "0103FC");
    }

    /// 生成型校验的区间越界: 渲染为空 + warn(不 panic、不破坏后续段)
    #[test]
    fn test_generated_checksum_out_of_range() {
        let ctx = common_ctx();
        // 已渲染 2 字节却要求覆盖 6 → 越界, 该变量渲染为空, 前面的字面量原样保留
        assert_eq!(render_hex("0103${crc16modbus:0:6:le}", &ctx), "0103");
        assert_eq!(render_hex("01 03 ${crc16modbus:0:6:le}", &ctx), "01 03 ");
    }

    /// `${= 表达式 }`(决策 E-8/E-9/E-10)
    #[test]
    fn test_expression_variables() {
        let ctx = reply_ctx(&[0x01, 0x03, 0x00, 0x02], Some(0));
        assert_eq!(render_text("${= 1 + 2 }", &ctx), "3");
        assert_eq!(render_text("${= 0xFF }", &ctx), "255");
        assert_eq!(render_text("${= rx.u16be(2) + 1 }", &ctx), "3");
        assert_eq!(render_text("${= if(rx.u8(1) == 3, 4, 0) }", &ctx), "4");
        assert_eq!(render_text("${= bits(0x8A5F, 10, 4) }", &ctx), "2");
        // 无 rx 上下文的普通模板里, 不引用帧的表达式照样可用
        assert_eq!(render_text("${= 2 * 3 }", &common_ctx()), "6");
        // hex 模式按数值输出
        assert_eq!(render_hex("${= 10 }", &ctx), "0A");
    }

    // ===== 决策 E-2 / E-3: 上下文与越界的区别 =====

    /// 无接收帧上下文时 `${rx.*}` **原样保留**(E-2): 静默渲染成空会掩盖配置错误
    #[test]
    fn test_rx_variables_preserved_without_context() {
        let ctx = common_ctx();
        assert_eq!(render_text("${rx.raw}", &ctx), "${rx.raw}");
        assert_eq!(render_text("${rx.u16be:2}", &ctx), "${rx.u16be:2}");
        assert_eq!(render_text("${rx.len}", &ctx), "${rx.len}");
        assert_eq!(render_text("${rx.src}", &ctx), "${rx.src}");
        assert_eq!(
            render_text("${rx.crc16modbus:0:6}", &ctx),
            "${rx.crc16modbus:0:6}"
        );
        assert_eq!(render_text("v=${rx.u8:0}", &ctx), "v=${rx.u8:0}");
    }

    /// 有上下文但**越界**时渲染为空(E-3): 帧比预期短是数据问题, 空串保持应答帧结构
    #[test]
    fn test_rx_out_of_range_renders_empty() {
        let ctx = reply_ctx(&[0x01], None);
        assert_eq!(render_text("A${rx.u16be:0}B", &ctx), "AB");
        assert_eq!(render_text("A${rx.raw:5}C", &ctx), "AC");
        assert_eq!(render_text("A${rx.hex:0:9}D", &ctx), "AD");
    }

    /// 空帧上的行为(边界表)
    #[test]
    fn test_rx_empty_frame() {
        let ctx = reply_ctx(&[], None);
        assert_eq!(render_text("${rx.raw}", &ctx), "");
        assert_eq!(render_text("${rx.len}", &ctx), "0");
        // 空帧取 1 字节 → 越界 → 空串
        assert_eq!(render_text("[${rx.u8:0}]", &ctx), "[]");
        // 空输入 CRC16 = 0xFFFF（与 checksum 既有断言一致）
        assert_eq!(render_hex("${rx.crc16modbus:0:0}", &ctx), "FFFF");
    }

    /// 未知 accessor 按未知变量原样保留(与 `${unknown}` 一致)
    ///
    /// 注意 `${rx.u8}`（省略偏移）是**合法**写法，等价于 `${rx.u8:0}` ——
    /// 这是有意保留的便利写法，不是拼写错误。
    #[test]
    fn test_unknown_rx_accessor_preserved() {
        for template in [
            "${rx.u17be:0}",
            "${rx.foo:0}",
            "${rx.u8:0:2}",
            "${rx.raw:0:1:2}",
            "${rx.u8le:0}",
        ] {
            assert_eq!(
                render_text(template, &reply_ctx(&[1, 2, 3, 4], None)),
                template,
                "非法语法必须原样保留: {}",
                template
            );
        }
        // 省略偏移 = 0（便利写法）
        assert_eq!(render_text("${rx.u8}", &reply_ctx(&[7, 8], None)), "7");
    }

    /// 非法表达式按未知变量原样保留(未知函数 / 参数个数不符 / 语法错)
    #[test]
    fn test_invalid_expression_preserved() {
        for template in [
            "${= send(1) }",
            "${= min(1) }",
            "${= 1 + }",
            "${= foo }",
            "${= }",
        ] {
            assert_eq!(
                render_text(template, &common_ctx()),
                template,
                "非法表达式必须原样保留: {}",
                template
            );
        }
    }

    /// 非法生成型校验名按未知变量保留(不能把 `${crc99:0:6}` 当成合法变量)
    #[test]
    fn test_invalid_gen_checksum_preserved() {
        for template in [
            "${crc99:0:6}",
            "${crc16modbus:0}",
            "${crc16modbus:a:b}",
            "${crc16modbus:0:6:xx}",
        ] {
            assert_eq!(render_text(template, &common_ctx()), template);
        }
    }

    /// `needs_seq` / `needs_rx` 探测(快路径判断)
    #[test]
    fn test_needs_detection() {
        assert!(CompiledTemplate::new("${rx.u8:0}").needs_rx());
        assert!(CompiledTemplate::new("${rx.len}").needs_rx());
        assert!(CompiledTemplate::new("${= rx.u8(0) }").needs_rx());
        assert!(!CompiledTemplate::new("${seq}").needs_rx());
        assert!(!CompiledTemplate::new("hello").needs_rx());

        // 既有探测不受影响
        assert!(CompiledTemplate::new("${seq}").needs_seq());
        assert!(!CompiledTemplate::new("${uuid}").needs_seq());
    }

    /// `${rx.*}` 与 `${seq}` 共存: 序号只在模板含它时消费
    #[test]
    fn test_rx_with_seq() {
        let ctx = reply_ctx(&[0x01], Some(7));
        assert_eq!(render_text("${rx.u8:0}-${seq}", &ctx), "1-7");
        let compiled = CompiledTemplate::new("${rx.u8:0}");
        assert!(!compiled.needs_seq(), "不含 seq 的模板不得消费序号");
    }
}
