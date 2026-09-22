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
use gpui_component::scroll::ScrollableElement;

use rust_i18n::t;

use crate::app::NetAssistantApp;
use crate::utils::file_source::{FileEncoding, FileSourceError, bytes_to_hex_text, format_size};

use super::dialog_height;

/// 预览最多展示行数 / 字符数。
///
/// 预览只是给用户确认「编码对不对」，无需完整内容（完整内容在确认时由缓存字节重新生成）；
/// 且 hex 模式下完整文本可达 ~512K 字符，直接渲染会拖垮 UI，故在此截断。
const PREVIEW_MAX_LINES: usize = 8;
const PREVIEW_MAX_CHARS: usize = 2000;

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
}

impl ImportFileDialogState {
    pub fn new(tab_id: String, hex_mode: bool) -> Self {
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
        }
    }

    /// 是否可确认（已成功生成预览且不在读取中）
    pub fn can_confirm(&self) -> bool {
        self.bytes.is_some() && self.preview.is_some() && self.error.is_none() && !self.reading
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
            return;
        }
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

    /// 生成要回填到发送输入框的完整内容（确认时调用）
    pub fn full_text(&self) -> Option<String> {
        let bytes = self.bytes.as_ref()?;
        if self.hex_mode {
            return Some(bytes_to_hex_text(bytes));
        }
        self.encoding.decode(bytes).ok().map(|decoded| decoded.text)
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

    let mut file_row = div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(
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
                        .child(encoding_chip(
                            app,
                            s.encoding,
                            FileEncoding::Utf8,
                            &theme,
                        ))
                        .child(encoding_chip(
                            app,
                            s.encoding,
                            FileEncoding::Gbk,
                            &theme,
                        ))
                        .child(encoding_chip(
                            app,
                            s.encoding,
                            FileEncoding::Ansi,
                            &theme,
                        )),
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
                    div()
                        .max_h(px(160.0))
                        .child(
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

/// 渲染底部操作按钮
fn render_footer(app: &WeakEntity<NetAssistantApp>, cx: &App) -> DialogFooter {
    // 确定按钮可用态随状态实时刷新（对话框 builder 每帧重建）
    let can_confirm = app
        .upgrade()
        .and_then(|entity| {
            entity
                .read(cx)
                .import_file_dialog
                .as_ref()
                .map(|s| s.can_confirm())
        })
        .unwrap_or(false);

    let app_cancel = app.clone();
    let app_ok = app.clone();
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

    let ok = Button::new("import-file-ok")
        .primary()
        .label(t!("import_file.confirm").to_string());
    // 无有效预览（未选文件 / 解码失败 / 读取中）时禁用确定
    let ok = if can_confirm {
        ok.on_click(move |_, window, cx| {
            let _ = app_ok.update(cx, |app, cx| app.confirm_import_file(window, cx));
            window.close_dialog(cx);
        })
    } else {
        ok.disabled(true)
    };
    footer = footer.child(ok);
    footer
}