// 从文件导入发送内容 —— 导入对话框
//
// 交互：发送区「打开文件」→ 本对话框（选文件 → 选编码 → 预览）→ 确定 → 回填发送输入框。
// 回填会覆盖现有草稿，故必须有「确定」这一步二次确认；编码选择也需要 UI 承载。
// 打开/关闭沿用 gpui_component 的命令式对话框惯例（见 stress_config.rs）。
//
// 状态挂在 `app.import_file_dialog`，内容闭包每帧从 app 读取最新状态；
// 对话框 builder 每帧重建，故底部「确定」按钮的可用态也能随状态实时刷新。

use std::path::PathBuf;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::Disableable as _;
use gpui_component::StyledExt;
use gpui_component::Theme;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::DialogFooter;
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;

use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::config::connection::TrailerKind;
use crate::send_task::{
    IntervalHandle, LineParseError, MAX_TASK_ITEMS, SendTaskConfig, TaskKind, parse_lines,
};
use crate::utils::file_source::{FileEncoding, FileSourceError, bytes_to_hex_text, format_size};
use crate::utils::hex::validate_hex_input;

use super::dialog_height;

/// 预览最多展示行数 / 字符数。
///
/// 预览只是给用户确认「编码对不对」，无需完整内容（完整内容在确认时由缓存字节重新生成）；
/// 且 hex 模式下完整文本可达 ~512K 字符，直接渲染会拖垮 UI，故在此截断。
const PREVIEW_MAX_LINES: usize = 8;
const PREVIEW_MAX_CHARS: usize = 2000;

/// 「发送方式」二选一: 整个文件发送(现状) / 逐行发送(新建后台任务)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSendMode {
    WholeFile,
    ByLines,
}

/// 逐行模式的行统计预览(仅展示; 创建任务时由 `parse_lines` 重新解析)
#[derive(Debug, Clone, Default)]
pub struct LinePreview {
    /// 有效行数(空行与非法行不计)
    pub valid: usize,
    /// 跳过的空行数
    pub skipped_blank: usize,
    /// hex 模式非法行(前 5 条; 1-based 行号, 原文)
    pub invalid: Vec<(usize, String)>,
    /// 整批级错误(无可发送行 / 超上限)
    pub parse_error: Option<String>,
}

/// 「从文件打开」对话框状态（打开时创建，取消/确定后由 app 置 None）
pub struct ImportFileDialogState {
    pub tab_id: String,
    /// 是否处于 hex 模式（决定是否显示编码选择，以及回填形态）
    pub hex_mode: bool,
    pub path: Option<PathBuf>,
    pub size: Option<u64>,
    /// 已读入的原始字节（≤1 MiB），切换编码时复用它重新解码，不重复 IO
    pub bytes: Option<Arc<Vec<u8>>>,
    pub encoding: FileEncoding,
    /// 预览（已截断，仅用于展示）；完整发送内容在确认时由 bytes + encoding 重新生成
    pub preview: Option<SharedString>,
    /// 预览是否发生有损替换（用于 amber 提示）
    pub lossy: bool,
    /// 当前错误提示（过大/空文件/UTF-8 非法/代码页不支持/读失败），Some 时禁止确定
    pub error: Option<String>,
    pub reading: bool,

    // ===== 发送方式（整个文件 / 逐行发送）=====
    pub send_mode: ImportSendMode,
    /// 逐行: 每条之间的间隔(ms)
    pub interval_input: Entity<InputState>,
    /// 逐行: 是否循环整批
    pub loop_enabled: bool,
    /// 逐行: 最多轮次(留空 = 无限)
    pub max_rounds_input: Entity<InputState>,
    /// 逐行: 行统计预览(切换模式/编码/文件时刷新)
    pub line_preview: Option<LinePreview>,
}

impl ImportFileDialogState {
    pub fn new(
        tab_id: String,
        hex_mode: bool,
        window: &mut Window,
        cx: &mut Context<NetAssistantApp>,
    ) -> Self {
        let interval_input = cx.new(|cx| InputState::new(window, cx));
        interval_input.update(cx, |input, cx| {
            input.set_value("1000".to_string(), window, cx);
        });
        let max_rounds_input = cx.new(|cx| InputState::new(window, cx));
        Self {
            tab_id,
            hex_mode,
            path: None,
            size: None,
            bytes: None,
            encoding: FileEncoding::default(),
            preview: None,
            lossy: false,
            error: None,
            reading: false,
            send_mode: ImportSendMode::WholeFile,
            interval_input,
            loop_enabled: false,
            max_rounds_input,
            line_preview: None,
        }
    }

    /// 是否可确认（已成功生成预览且不在读取中；逐行模式还需行参数合法）
    pub fn can_confirm(&self, cx: &App) -> bool {
        let base =
            self.bytes.is_some() && self.preview.is_some() && self.error.is_none() && !self.reading;
        if !base {
            return false;
        }
        match self.send_mode {
            ImportSendMode::WholeFile => true,
            // 逐行: 间隔必须能解析为数值; 行统计存在且无整批错误/非法行, 且有可发送行
            ImportSendMode::ByLines => {
                self.interval_ms(cx).is_some()
                    && self
                        .line_preview
                        .as_ref()
                        .map(|p| p.parse_error.is_none() && p.invalid.is_empty() && p.valid > 0)
                        .unwrap_or(false)
            }
        }
    }

    /// 逐行间隔(ms); 输入为空/非法时返回 None
    pub fn interval_ms(&self, cx: &App) -> Option<u64> {
        self.interval_input
            .read(cx)
            .value()
            .trim()
            .parse::<u64>()
            .ok()
    }

    /// 最多轮次; 留空/非法 → None(无限); 仅循环开启时有意义
    pub fn max_rounds(&self, cx: &App) -> Option<u32> {
        if !self.loop_enabled {
            return None;
        }
        self.max_rounds_input
            .read(cx)
            .value()
            .trim()
            .parse::<u32>()
            .ok()
    }

    /// 记录读取结果并生成预览
    pub fn set_loaded(&mut self, path: PathBuf, size: u64, bytes: Vec<u8>) {
        self.path = Some(path);
        self.size = Some(size);
        self.bytes = Some(Arc::new(bytes));
        self.error = None;
        self.lossy = false;
        self.reading = false;
        self.refresh_preview();
    }

    /// 记录失败（过大/空文件/读取失败）
    pub fn set_error(&mut self, err: &FileSourceError) {
        self.preview = None;
        self.lossy = false;
        self.reading = false;
        self.error = Some(error_text(err));
        self.line_preview = None;
    }

    /// 依当前 hex_mode / encoding 从缓存字节重新生成预览（切换编码时复用，不重复 IO）
    pub fn refresh_preview(&mut self) {
        let Some(bytes) = self.bytes.as_ref() else {
            return;
        };
        self.error = None;
        self.lossy = false;
        if self.hex_mode {
            // hex 模式按原始字节导入，不涉及编码
            self.preview = Some(SharedString::from(preview_head(&bytes_to_hex_text(bytes))));
        } else {
            match self.encoding.decode(bytes) {
                Ok(decoded) => {
                    self.lossy = decoded.lossy;
                    self.preview = Some(SharedString::from(preview_head(&decoded.text)));
                }
                Err(err) => {
                    self.preview = None;
                    self.error = Some(error_text(&err));
                }
            }
        }
        // 编码/文件变化会改变行内容, 同步刷新逐行统计
        self.refresh_line_preview();
    }

    /// 刷新逐行模式的行统计（切换发送方式 / 编码 / 文件时调用）。
    ///
    /// 统计口径与 `parse_lines` 一致: 按 `\n` 切分、剥离行尾 `\r`、空行跳过、
    /// hex 模式逐行校验。此处仅作展示与可确认性判断, 创建任务时仍由 `parse_lines` 重新解析。
    pub fn refresh_line_preview(&mut self) {
        self.line_preview = None;
        if self.send_mode != ImportSendMode::ByLines {
            return;
        }
        let Some(text) = self.full_text() else {
            return;
        };
        let mut stats = LinePreview::default();
        let mut too_many = false;
        for (idx, raw_line) in text.split('\n').enumerate() {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            if line.is_empty() {
                stats.skipped_blank += 1;
                continue;
            }
            if self.hex_mode && !validate_hex_input(line) {
                if stats.invalid.len() < 5 {
                    stats.invalid.push((idx + 1, line.to_string()));
                }
                continue;
            }
            stats.valid += 1;
            if stats.valid > MAX_TASK_ITEMS {
                too_many = true;
                break;
            }
        }
        if too_many {
            stats.parse_error =
                Some(t!("import_file.err_too_many", max = MAX_TASK_ITEMS).to_string());
        } else if stats.valid == 0 && stats.invalid.is_empty() {
            stats.parse_error = Some(t!("import_file.err_empty").to_string());
        }
        self.line_preview = Some(stats);
    }

    /// 逐行模式确认时构建任务配置; 解析失败返回 UI 文案。
    pub fn build_send_config(
        &self,
        cx: &App,
        start_immediately: bool,
    ) -> Result<SendTaskConfig, String> {
        let text = self
            .full_text()
            .ok_or_else(|| t!("import_file.empty").to_string())?;
        let items = parse_lines(&text, self.hex_mode).map_err(|e| line_parse_error_text(&e))?;
        let interval_ms = self.interval_ms(cx).unwrap_or(1000);
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("import_file.no_file").to_string());
        Ok(SendTaskConfig {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            kind: TaskKind::SendByLines { items },
            interval: IntervalHandle::new(interval_ms),
            loop_enabled: self.loop_enabled,
            max_rounds: self.max_rounds(cx),
            hex_mode: self.hex_mode,
            start_immediately,
            hidden: false,
        })
    }

    /// 生成要回填到发送输入框的完整内容（确认时调用）
    pub fn full_text(&self) -> Option<String> {
        let bytes = self.bytes.as_ref()?;
        if self.hex_mode {
            return Some(bytes_to_hex_text(bytes));
        }
        self.encoding.decode(bytes).ok().map(|decoded| decoded.text)
    }
}

/// 结尾追加字符的展示标签（无 / LF / CRLF）
pub fn trailer_label(kind: TrailerKind) -> String {
    match kind {
        TrailerKind::None => t!("connection_tab.trailer_none").to_string(),
        TrailerKind::Lf => "LF".to_string(),
        TrailerKind::CrLf => "CRLF".to_string(),
    }
}

/// 逐行模式只读提示行: 结尾追加跟随发送区(连接级)配置
pub fn trailer_hint_text(kind: TrailerKind) -> String {
    t!("import_file.trailer_hint", label = trailer_label(kind)).to_string()
}

/// `parse_lines` 错误 → 本地化文案
pub fn line_parse_error_text(err: &LineParseError) -> String {
    match err {
        LineParseError::Empty => t!("import_file.err_empty").to_string(),
        LineParseError::TooMany { max, .. } => {
            t!("import_file.err_too_many", max = max).to_string()
        }
        LineParseError::InvalidHexLines(_) => t!("import_file.err_invalid_hex").to_string(),
    }
}

/// 错误 → 本地化文案
pub fn error_text(err: &FileSourceError) -> String {
    match err {
        FileSourceError::TooLarge { size, limit } => t!(
            "import_file.too_large",
            size = format_size(*size),
            limit = format_size(*limit as u64)
        )
        .to_string(),
        FileSourceError::Empty => t!("import_file.empty").to_string(),
        FileSourceError::InvalidUtf8 => t!("import_file.invalid_utf8").to_string(),
        FileSourceError::UnsupportedAnsiCodePage(cp) => {
            t!("import_file.unsupported_cp", cp = cp).to_string()
        }
        FileSourceError::ReadFailed(msg) => t!("import_file.read_failed", msg = msg).to_string(),
    }
}

/// 预览截断：最多 PREVIEW_MAX_LINES 行 / PREVIEW_MAX_CHARS 字符
fn preview_head(text: &str) -> String {
    let mut out = String::new();
    let mut lines = 0usize;
    let mut count = 0usize;
    for ch in text.chars() {
        if lines >= PREVIEW_MAX_LINES || count >= PREVIEW_MAX_CHARS {
            break;
        }
        if ch == '\n' {
            lines += 1;
        }
        out.push(ch);
        count += 1;
    }
    out
}

/// 打开导入对话框（命令式，由 Root 管理层叠）
///
/// keyboard 保持关闭：Enter 不应直接确认（避免误覆盖草稿），关闭方式为
/// 取消按钮 / X / 点击蒙层。
pub fn open_import_file_dialog(
    app: WeakEntity<NetAssistantApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        dialog
            .title(t!("import_file.title").to_string())
            .w(px(520.0))
            .max_h(dialog_height(window))
            .keyboard(false)
            // X 按钮 / 蒙层关闭时同步清理状态
            .on_cancel({
                let app = app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.import_file_dialog = None;
                        cx.notify();
                    });
                    true
                }
            })
            .footer(render_footer(&app, cx))
            .content({
                let app = app.clone();
                move |content, window, cx| {
                    let Some(entity) = app.upgrade() else {
                        return content;
                    };
                    content.child(render_body(&entity, window, cx))
                }
            })
    });
}

/// 渲染对话框主体
fn render_body(app: &Entity<NetAssistantApp>, _window: &Window, cx: &App) -> Div {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let Some(s) = state.import_file_dialog.as_ref() else {
        return div();
    };

    let file_label = match s.path.as_ref().and_then(|p| p.file_name()) {
        Some(name) => name.to_string_lossy().to_string(),
        None => t!("import_file.no_file").to_string(),
    };
    let size_label = s
        .size
        .map(|n| t!("import_file.size", size = format_size(n)).to_string());

    let mut file_row = div().flex().flex_col().min_w_0().child(
        div()
            .text_sm()
            .text_color(theme.foreground)
            .child(file_label),
    );
    if let Some(size_label) = size_label {
        file_row = file_row.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(size_label),
        );
    }

    let browse = {
        let entity = app.clone();
        let label = if s.reading {
            t!("import_file.reading").to_string()
        } else {
            t!("import_file.browse").to_string()
        };
        div()
            .id("import-file-browse")
            .flex_shrink_0()
            .px_3()
            .py_1()
            .bg(theme.secondary)
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(theme.secondary_hover))
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(theme.secondary_foreground)
                    .child(label),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                entity.update(cx, |app, cx| app.pick_import_file(window, cx));
            })
    };

    let mut body = div()
        .flex()
        .flex_col()
        .gap_3()
        .px_6()
        .pb_4()
        // 文件选择行
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(browse)
                .child(file_row),
        );

    // 编码选择（hex 模式隐藏，改为按原始字节导入的提示）
    if s.hex_mode {
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(t!("import_file.hex_raw_hint").to_string()),
        );
    } else {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(t!("import_file.encoding").to_string()),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(encoding_chip(app, s.encoding, FileEncoding::Utf8, &theme))
                        .child(encoding_chip(app, s.encoding, FileEncoding::Gbk, &theme))
                        .child(encoding_chip(app, s.encoding, FileEncoding::Ansi, &theme)),
                ),
        );
    }

    // 预览
    if let Some(preview) = s.preview.clone() {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(t!("import_file.preview").to_string()),
                )
                .child(
                    div().max_h(px(160.0)).child(
                        div()
                            .id("import-file-preview")
                            .overflow_y_scrollbar()
                            .p_2()
                            .bg(theme.border)
                            .rounded_md()
                            .font_family("JetBrains Mono")
                            .text_xs()
                            .text_color(theme.foreground)
                            .child(preview),
                    ),
                ),
        );
    }

    // 有损解码提示（允许导入，仅提示）
    if s.lossy {
        body = body.child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(theme.warning)
                .child(t!("import_file.lossy").to_string()),
        );
    }

    // 发送方式：整个文件发送（现状）/ 逐行发送（新建后台任务）
    body = body.child(
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(t!("import_file.send_mode").to_string()),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(mode_chip(
                        app,
                        s.send_mode,
                        ImportSendMode::WholeFile,
                        &theme,
                    ))
                    .child(mode_chip(app, s.send_mode, ImportSendMode::ByLines, &theme)),
            ),
    );

    // 逐行参数区：间隔 / 循环 / 最多轮次 + 行统计 + 结尾提示
    if s.send_mode == ImportSendMode::ByLines {
        let mut params = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(theme.secondary)
            .rounded_md()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    // 间隔(ms)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(t!("import_file.line_interval").to_string()),
                    )
                    .child(
                        div()
                            .w_20()
                            .h_7()
                            .bg(theme.background)
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                Input::new(&s.interval_input)
                                    .w_full()
                                    .h_full()
                                    .bg(theme.background)
                                    .rounded_md()
                                    .border_0()
                                    .text_center(),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("ms".to_string()),
                    )
                    // 循环发送
                    .child({
                        let entity = app.clone();
                        div()
                            .ml_2()
                            .flex()
                            .items_center()
                            .gap_1()
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                entity.update(cx, |app, cx| {
                                    if let Some(d) = app.import_file_dialog.as_mut() {
                                        d.loop_enabled = !d.loop_enabled;
                                    }
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .w_4()
                                    .h_4()
                                    .border_1()
                                    .border_color(theme.border)
                                    .rounded(px(4.))
                                    .when(s.loop_enabled, |this| {
                                        this.bg(theme.primary)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.primary_foreground)
                                                    .font_bold()
                                                    .child("✓"),
                                            )
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("import_file.loop_send").to_string()),
                            )
                    }),
            );

        // 最多轮次（仅循环开启时可编辑；留空 = 无限）
        if s.loop_enabled {
            params = params.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(t!("import_file.max_rounds").to_string()),
                    )
                    .child(
                        div()
                            .w_20()
                            .h_7()
                            .bg(theme.background)
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                Input::new(&s.max_rounds_input)
                                    .w_full()
                                    .h_full()
                                    .bg(theme.background)
                                    .rounded_md()
                                    .border_0()
                                    .text_center(),
                            ),
                    ),
            );
        }

        // 行统计预览
        if let Some(p) = s.line_preview.as_ref() {
            if let Some(err) = p.parse_error.clone() {
                params = params.child(div().text_xs().text_color(theme.danger).child(err));
            } else {
                params = params.child(
                    div().text_xs().text_color(theme.foreground).child(
                        t!(
                            "import_file.lines_summary",
                            n = p.valid,
                            skipped = p.skipped_blank
                        )
                        .to_string(),
                    ),
                );
            }
            if !p.invalid.is_empty() {
                let mut list = div().flex().flex_col().gap_1().child(
                    div()
                        .text_xs()
                        .text_color(theme.warning)
                        .child(t!("import_file.invalid_lines_title").to_string()),
                );
                for (line_no, raw) in p.invalid.iter() {
                    list = list.child(
                        div()
                            .font_family("JetBrains Mono")
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("{}: {}", line_no, raw)),
                    );
                }
                params = params.child(list);
            }
        }

        // 结尾追加（只读，跟随发送区连接级配置）
        let trailer_kind = state
            .connection_tabs
            .get(&s.tab_id)
            .map(|t| t.send_trailer_setting.get())
            .unwrap_or(TrailerKind::None);
        params = params.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(trailer_hint_text(trailer_kind)),
        );

        body = body.child(params);
    }

    // 错误行
    if let Some(err) = s.error.clone() {
        body = body.child(
            div()
                .p_2()
                .rounded_md()
                .bg(theme.danger.opacity(0.12))
                .border_1()
                .border_color(theme.danger)
                .child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(theme.foreground)
                        .child(err),
                ),
        );
    }

    body
}

/// 渲染编码分段按钮
fn encoding_chip(
    app: &Entity<NetAssistantApp>,
    current: FileEncoding,
    encoding: FileEncoding,
    theme: &Theme,
) -> Div {
    let selected = current == encoding;
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |d| {
            d.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!selected, |d| {
            d.bg(theme.border).text_color(theme.foreground)
        })
        .child(
            div()
                .text_sm()
                .font_medium()
                .child(encoding_label(encoding)),
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                if let Some(s) = app.import_file_dialog.as_mut() {
                    if s.encoding != encoding {
                        s.encoding = encoding;
                        // 复用已读入字节重新解码，不重复 IO
                        s.refresh_preview();
                    }
                }
                cx.notify();
            });
        })
}

/// 编码选项的本地化标签
fn encoding_label(encoding: FileEncoding) -> String {
    match encoding {
        FileEncoding::Utf8 => t!("import_file.enc_utf8").to_string(),
        FileEncoding::Gbk => t!("import_file.enc_gbk").to_string(),
        FileEncoding::Ansi => t!("import_file.enc_ansi").to_string(),
    }
}

/// 渲染「发送方式」分段按钮
fn mode_chip(
    app: &Entity<NetAssistantApp>,
    current: ImportSendMode,
    mode: ImportSendMode,
    theme: &Theme,
) -> Div {
    let selected = current == mode;
    let entity = app.clone();
    div()
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |d| {
            d.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .when(!selected, |d| {
            d.bg(theme.border).text_color(theme.foreground)
        })
        .child(div().text_sm().font_medium().child(mode_label(mode)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |app, cx| {
                if let Some(d) = app.import_file_dialog.as_mut() {
                    if d.send_mode != mode {
                        d.send_mode = mode;
                        // 切换发送方式后重算逐行统计
                        d.refresh_line_preview();
                    }
                }
                cx.notify();
            });
        })
}

/// 发送方式的本地化标签
fn mode_label(mode: ImportSendMode) -> String {
    match mode {
        ImportSendMode::WholeFile => t!("import_file.mode_whole").to_string(),
        ImportSendMode::ByLines => t!("import_file.mode_by_lines").to_string(),
    }
}

/// 渲染底部操作按钮
fn render_footer(app: &WeakEntity<NetAssistantApp>, cx: &App) -> DialogFooter {
    // 可用态随状态实时刷新（对话框 builder 每帧重建）
    let (can_confirm, mode) = app
        .upgrade()
        .and_then(|entity| {
            entity
                .read(cx)
                .import_file_dialog
                .as_ref()
                .map(|s| (s.can_confirm(cx), s.send_mode))
        })
        .unwrap_or((false, ImportSendMode::WholeFile));

    let app_cancel = app.clone();
    let mut footer = DialogFooter::new().child(
        Button::new("import-file-cancel")
            .outline()
            .label(t!("import_file.cancel").to_string())
            .on_click(move |_, window, cx| {
                let _ = app_cancel.update(cx, |app, cx| {
                    app.import_file_dialog = None;
                    cx.notify();
                });
                window.close_dialog(cx);
            }),
    );

    match mode {
        // 整个文件发送: 沿用现状（确定 → 回填发送框）
        ImportSendMode::WholeFile => {
            let app_ok = app.clone();
            let ok = Button::new("import-file-ok")
                .primary()
                .label(t!("import_file.confirm").to_string());
            let ok = if can_confirm {
                ok.on_click(move |_, window, cx| {
                    let _ = app_ok.update(cx, |app, cx| app.confirm_import_file(window, cx));
                    window.close_dialog(cx);
                })
            } else {
                ok.disabled(true)
            };
            footer = footer.child(ok);
        }
        // 逐行发送: 创建任务（暂停待启动）/ 创建并立即开始
        ImportSendMode::ByLines => {
            let app_create = app.clone();
            let create = Button::new("import-file-create-task")
                .outline()
                .label(t!("import_file.create_task").to_string());
            let create = if can_confirm {
                create.on_click(move |_, window, cx| {
                    // 失败时保持对话框打开以展示错误(不创建半成品任务)
                    if app_create
                        .update(cx, |app, cx| app.confirm_import_file_as_task(false, cx))
                        .unwrap_or(false)
                    {
                        window.close_dialog(cx);
                    }
                })
            } else {
                create.disabled(true)
            };

            let app_start = app.clone();
            let start = Button::new("import-file-create-start")
                .primary()
                .label(t!("import_file.create_and_start").to_string());
            let start = if can_confirm {
                start.on_click(move |_, window, cx| {
                    // 失败时保持对话框打开以展示错误(不创建半成品任务)
                    if app_start
                        .update(cx, |app, cx| app.confirm_import_file_as_task(true, cx))
                        .unwrap_or(false)
                    {
                        window.close_dialog(cx);
                    }
                })
            } else {
                start.disabled(true)
            };
            footer = footer.child(create).child(start);
        }
    }
    footer
}
