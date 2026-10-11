use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Theme;
use gpui_kit::component::Icon;
use gpui_kit::component::IconName;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::input::{Editor, EditorState, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;

use crate::app::{FileHashJob, NetAssistantApp};
use crate::core::toolbox::{
    SelectionTool, ToolCategory, compute_tool, resolve_tools, selection_tools,
};
use crate::core::toolbox_extra::{
    base64_decode, base64_encode, file_hash_algo, format_file_size, hash_md5, hash_sha1,
    hash_sha224, hash_sha256, hash_sha384, hash_sha512, json_minify, json_pretty,
    now_local_string, now_millis_string, parse_time_input, url_decode, url_encode, FILE_HASH_ALGOS,
    RelativeTime,
};
use chrono::Weekday;

use crate::custom_icons::CustomIconName;

/// 工具箱多行编辑器右键菜单：剪切 / 复制 / 粘贴 / 全选（干净四项，复用组件标准编辑 action）。
///
/// 粘贴在剪贴板无文本时置灰；剪切/复制/全选恒亮（无选区/空内容时为安全 no-op），
/// 与 InputWithMode 自绘菜单的置灰策略一致。菜单在右键时才构建，读到的是当下剪贴板。
fn editor_context_menu_builder() -> impl Fn(NativeMenu, &mut Window, &mut App) -> NativeMenu {
    |menu, _window, cx| {
        let clipboard_has_text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| !text.is_empty());
        menu.menu_with_disabled(
            t!("input_mode.cut").to_string(),
            false,
            Box::new(gpui_kit::base::input::Cut),
        )
        .menu_with_disabled(
            t!("input_mode.copy").to_string(),
            false,
            Box::new(gpui_kit::base::input::Copy),
        )
        .menu_with_disabled(
            t!("input_mode.paste").to_string(),
            !clipboard_has_text,
            Box::new(gpui_kit::base::input::Paste),
        )
        .menu(
            t!("input_mode.select_all").to_string(),
            Box::new(gpui_kit::base::input::SelectAll),
        )
    }
}

/// 工具箱标签页：左侧分组导航（纯切换，无「全部」聚合页）+ 右侧工具块工作区。
///
/// 工具块统一模型：标题（含输入模式徽标）+ 输入框 + 输出行（名称 + 值 + 复制）。
/// 转换/校验组复用 `core/toolbox.rs` 注册表（`resolve_tools` / `compute_tool`）；
/// 时间/JSON/编码组直接调用 `core/toolbox_extra.rs` 的纯函数。
/// 输出不缓存，render 现算（输入订阅触发重绘）；各组输入框存放在 `app` 上
/// （`toolbox_*_input`），互相独立、随 Tab 常驻保留。
pub struct ToolboxPanel<'a> {
    app: &'a NetAssistantApp,
}

/// 一条输出行的渲染数据
struct OutputRow {
    /// 元素 id（页面内唯一）
    id: &'static str,
    /// 行名称（如算法名）；空串则不渲染名称列
    name: String,
    /// 展示值；`is_error` 时为禁用原因文案
    value: String,
    is_error: bool,
    /// 复制到剪贴板的内容；空串时隐藏复制按钮
    copy_value: String,
    /// 值区域的元素 id；提供时值区域限高并纵向滚动（大输出场景，如 JSON）
    value_id: Option<&'static str>,
    /// 对比哈希命中标记：true → 行高亮并显示「匹配」徽章（仅摘要行参与）
    matched: bool,
}

impl OutputRow {
    /// 空输入占位行（无名称、无复制）
    fn placeholder(id: &'static str) -> Self {
        Self {
            id,
            name: String::new(),
            value: String::new(),
            is_error: false,
            copy_value: String::new(),
            value_id: None,
            matched: false,
        }
    }

    /// 正常结果行（可复制）
    fn result(id: &'static str, name: String, value: String) -> Self {
        Self {
            id,
            name,
            copy_value: value.clone(),
            value,
            is_error: false,
            value_id: None,
            matched: false,
        }
    }

    /// 禁用行：值位置展示禁用原因，不可复制
    fn error(id: &'static str, name: String, reason_key: &'static str) -> Self {
        Self {
            id,
            name,
            value: t!(reason_key).to_string(),
            is_error: true,
            copy_value: String::new(),
            value_id: None,
            matched: false,
        }
    }

    /// 标记为对比命中（与下方摘要逐行比对时使用）
    fn with_matched(mut self) -> Self {
        self.matched = true;
        self
    }
}

/// 哈希比对归一化：trim + 转小写 + 去除空白与「:」分隔符（容错 `AB:CD:…`、大写等粘贴格式）
fn normalize_hash(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ':')
        .collect()
}

/// 按 id 从注册表取静态工具描述（预置 id 均为编译期常量）
fn find_tool(id: &'static str) -> &'static SelectionTool {
    selection_tools()
        .iter()
        .find(|tool| tool.id == id)
        .expect("core/toolbox 注册表缺少预置工具")
}

impl<'a> ToolboxPanel<'a> {
    pub fn new(app: &'a NetAssistantApp) -> Self {
        Self { app }
    }

    pub fn render(self, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let theme = cx.theme().clone();

        div()
            .flex()
            .flex_1()
            .min_h_0()
            .bg(theme.background)
            .child(self.render_nav(cx))
            .child(self.render_workspace(cx))
    }

    /// 左侧分组导航：点击纯切换（无「全部」聚合页），选中组高亮
    fn render_nav(&self, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let theme = cx.theme().clone();

        let mut nav = div()
            .id("toolbox-nav")
            .w_40()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .overflow_y_scrollbar()
            .bg(theme.secondary)
            .border_r_1()
            .border_color(theme.border);

        for (category, title_key, nav_id) in [
            (
                ToolCategory::Convert,
                "toolbox.category_convert",
                "toolbox-nav-convert",
            ),
            (
                ToolCategory::Checksum,
                "toolbox.category_checksum",
                "toolbox-nav-checksum",
            ),
            (ToolCategory::Hash, "toolbox.category_hash", "toolbox-nav-hash"),
            (ToolCategory::Time, "toolbox.category_time", "toolbox-nav-time"),
            (ToolCategory::Json, "toolbox.category_json", "toolbox-nav-json"),
            (
                ToolCategory::Encoding,
                "toolbox.category_encoding",
                "toolbox-nav-encoding",
            ),
        ] {
            let is_active = self.app.toolbox_active_group == category;
            nav = nav.child(
                div()
                    .id(nav_id)
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .when(is_active, |item| {
                        item.bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .font_medium()
                    })
                    .when(!is_active, |item| {
                        item.text_color(theme.foreground)
                            .hover(|style| style.bg(theme.border))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp,
                                         _event: &MouseDownEvent,
                                         _window: &mut Window,
                                         cx: &mut Context<NetAssistantApp>| {
                            // 纯切换分组；各组输入互相独立，切换后内容保留
                            app.toolbox_active_group = category;
                            cx.notify();
                        }),
                    )
                    .child(t!(title_key).to_string()),
            );
        }
        nav
    }

    /// 右侧工作区：当前组的工具块集合（纵向滚动）
    fn render_workspace(&self, cx: &mut Context<NetAssistantApp>) -> impl IntoElement {
        let mut workspace = div()
            .id("toolbox-workspace")
            .flex_1()
            .min_h_0()
            .min_w_0() // 防止块内长内容把工作区撑出可视区（复制按钮被顶出屏幕）
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .overflow_y_scrollbar();

        match self.app.toolbox_active_group {
            ToolCategory::Convert => {
                // 两个独立输入的块：文本→Hex、Hex→文本（无方向歧义）
                workspace = workspace
                    .child(Self::render_convert_block(
                        find_tool("to_hex"),
                        &self.app.toolbox_convert_text_input,
                        cx,
                    ))
                    .child(Self::render_convert_block(
                        find_tool("to_text"),
                        &self.app.toolbox_convert_hex_input,
                        cx,
                    ));
            }
            ToolCategory::Checksum => {
                workspace = workspace.child(self.render_checksum_block(cx));
            }
            ToolCategory::Hash => {
                // 单输入块：摘要六行 + 分组小标题下的密码哈希三行（KDF 走缓存）
                workspace = workspace.child(self.render_hash_block(cx));
            }
            ToolCategory::Time => {
                // 双块布局（参考 tool.lu）：①时间戳→日期 ②日期→时间戳，两块输入
                // 默认预填当前时刻，块头「填入当前」随时刷新
                workspace = workspace
                    .child(self.render_ts_to_date_block(cx))
                    .child(self.render_date_to_ts_block(cx));
            }
            ToolCategory::Json => {
                workspace = workspace.child(self.render_json_block(cx));
            }
            ToolCategory::Encoding => {
                // 两个独立输入的块：Base64 编解码、URL 编解码（各一块）
                workspace = workspace
                    .child(self.render_base64_block(cx))
                    .child(self.render_url_block(cx));
            }
        }
        workspace
    }

    /// 转换组工具块：单输入单输出（文本↔Hex 各一块，方向由注册表决定）
    fn render_convert_block(
        tool: &'static SelectionTool,
        input_state: &Entity<EditorState>,
        cx: &Context<NetAssistantApp>,
    ) -> Div {
        let source = input_state.read(cx).value().to_string();
        let mode = tool.modes.first().copied().unwrap_or("text");

        let rows = if source.trim().is_empty() {
            vec![OutputRow::placeholder(tool.id)]
        } else {
            match resolve_tools(mode, &source, tool.category)
                .into_iter()
                .find(|state| state.tool.id == tool.id)
            {
                Some(state) if state.enabled => vec![OutputRow::result(
                    tool.id,
                    t!("toolbox.result").to_string(),
                    compute_tool(state.tool, &source),
                )],
                Some(state) => vec![OutputRow::error(
                    tool.id,
                    t!("toolbox.result").to_string(),
                    state.disabled_reason.unwrap_or("input_mode.hex_invalid"),
                )],
                None => vec![OutputRow::placeholder(tool.id)],
            }
        };

        Self::render_block(
            t!(tool.label_key).to_string(),
            mode,
            Self::editor_area(input_state, cx),
            rows,
            None,
            None,
            None,
            cx,
        )
    }

    /// 校验组工具块：hex 输入 → 六种算法即时输出（一输入多结果行）
    fn render_checksum_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        let input_state = &self.app.toolbox_checksum_input;
        let source = input_state.read(cx).value().to_string();
        let states = resolve_tools("hex", &source, ToolCategory::Checksum);

        // 可用性只取决于输入本身（与算法无关）：空输入给占位，整体非法给单条
        // 错误行（六行重复原因无意义），合法则六个算法各一行结果
        let rows = if source.trim().is_empty() {
            vec![OutputRow::placeholder("checksum")]
        } else if let Some(bad) = states.iter().find(|state| !state.enabled) {
            vec![OutputRow::error(
                "checksum",
                String::new(),
                bad.disabled_reason.unwrap_or("input_mode.hex_invalid"),
            )]
        } else {
            states
                .iter()
                .map(|state| {
                    OutputRow::result(
                        state.tool.id,
                        t!(state.tool.label_key).to_string(),
                        compute_tool(state.tool, &source),
                    )
                })
                .collect()
        };

        Self::render_block(
            t!("toolbox.checksum_title").to_string(),
            "hex",
            Self::editor_area(input_state, cx),
            rows,
            None,
            None,
            None,
            cx,
        )
    }

    /// 哈希组工具块：按源分流（active=文件模式「文件条 + 算法 chips + 已选算法摘要行」；
    /// 其余=文本模式「单输入 → 摘要六行 + 密码哈希三行」）
    fn render_hash_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        match &self.app.toolbox_file_hash {
            Some(job) if job.active => self.render_hash_file_block(job, cx),
            _ => self.render_hash_text_block(cx),
        }
    }

    /// Hash 块头模式分段切换 [文本|文件]（互斥高亮；JSON 块 美化|压缩 同款交互）
    fn hash_mode_switch(&self, file_active: bool, cx: &Context<NetAssistantApp>) -> AnyElement {
        let theme = cx.theme().clone();

        let segment = |id: &'static str, on: bool| {
            div()
                .id(id)
                .cursor_pointer()
                .px_2()
                .py_0p5()
                .text_xs()
                .font_medium()
                .text_color(if on { theme.primary } else { theme.muted_foreground })
                .when(on, |seg| seg.bg(theme.primary.opacity(0.1)))
                .hover(|style| style.text_color(theme.primary))
        };

        div()
            .flex()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(
                segment("hash-mode-text", !file_active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                            app.hide_hash_file(window, cx);
                        }),
                    )
                    .child(t!("toolbox.mode_text").to_string()),
            )
            .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
            .child(
                segment("hash-mode-file", file_active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                            app.on_hash_file_button(window, cx);
                        }),
                    )
                    .child(t!("toolbox.hash_file_button").to_string()),
            )
            .into_any_element()
    }

    /// Hash 块文本模式：单输入 → 摘要 6 行 + 密码哈希 3 行（原行为）
    fn render_hash_text_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        let input_state = &self.app.toolbox_hash_input;
        let source = input_state.read(cx).value().to_string();

        // 对比哈希值：归一化后与摘要行逐行比对（密码哈希行因随机盐不参与）
        let compare_target = normalize_hash(
            &self
                .app
                .toolbox_hash_compare_input
                .read(cx)
                .value()
                .to_string(),
        );

        // 哈希对任意输入都有合法结果，无禁用态；KDF 三行读缓存（随机盐只在输入
        // 变化时重新生成），缓存与当前输入不一致的瞬间（订阅未触发前）不渲染该组
        let mut rows = Vec::new();
        let mut mid_header = None;
        if source.trim().is_empty() {
            rows.push(OutputRow::placeholder("hash"));
        } else {
            rows.extend([
                OutputRow::result("hash-md5", "MD5".to_string(), hash_md5(&source)),
                OutputRow::result("hash-sha1", "SHA-1".to_string(), hash_sha1(&source)),
                OutputRow::result("hash-sha224", "SHA-224".to_string(), hash_sha224(&source)),
                OutputRow::result("hash-sha256", "SHA-256".to_string(), hash_sha256(&source)),
                OutputRow::result("hash-sha384", "SHA-384".to_string(), hash_sha384(&source)),
                OutputRow::result("hash-sha512", "SHA-512".to_string(), hash_sha512(&source)),
            ]);
            if !compare_target.is_empty() {
                rows = rows
                    .into_iter()
                    .map(|row| {
                        if normalize_hash(&row.value) == compare_target {
                            row.with_matched()
                        } else {
                            row
                        }
                    })
                    .collect();
            }
            // id/名称与 app.rs 订阅中的生成顺序一一对应
            let metas = [
                ("passhash-bcrypt", "bcrypt"),
                ("passhash-argon2id", "Argon2id"),
                ("passhash-pbkdf2", "PBKDF2-SHA256"),
            ];
            if let Some((cached_source, results)) = &self.app.toolbox_kdf_cache {
                if cached_source == &source {
                    mid_header = Some((rows.len(), t!("toolbox.passhash_group").to_string()));
                    rows.extend(metas.iter().zip(results.iter()).map(
                        |((id, name), result)| match result {
                            Ok(value) => OutputRow::result(id, name.to_string(), value.clone()),
                            Err(()) => {
                                OutputRow::error(id, name.to_string(), "toolbox.passhash_failed")
                            }
                        },
                    ));
                }
            }
        }

        // 对比输入框：源文本下方、摘要行上方（放大镜图标 + 单行输入）
        let theme = cx.theme().clone();
        let compare_extra = Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(36.0))
                .w_full()
                .px_2()
                .bg(theme.background)
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .child(
                    Icon::new(IconName::Search)
                        .size_3()
                        .text_color(theme.muted_foreground),
                )
                .child(
                    Input::new(&self.app.toolbox_hash_compare_input)
                        .cleanable(true)
                        .w_full()
                        .h_full()
                        .font_family("JetBrains Mono")
                        .bg(theme.background)
                        .border_0(),
                )
                .into_any_element(),
        );

        Self::render_block(
            t!("toolbox.hash_title").to_string(),
            "text",
            Self::editor_area(input_state, cx),
            rows,
            mid_header,
            Some(self.hash_mode_switch(false, cx)),
            compare_extra,
            cx,
        )
    }

    /// Hash 块文件模式：文件条（名称+大小+✕ 移除）+ 算法 chips +
    /// 已选算法摘要行（对比联动、复制复用统一行骨架；KDF 组不适用隐藏）
    fn render_hash_file_block(&self, job: &FileHashJob, cx: &Context<NetAssistantApp>) -> Div {
        let theme = cx.theme().clone();

        // 对比哈希值：归一化后与文件摘要行逐行比对
        let compare_target = normalize_hash(
            &self
                .app
                .toolbox_hash_compare_input
                .read(cx)
                .value()
                .to_string(),
        );

        // 行区：已选算法按 FILE_HASH_ALGOS 序；完成 → 值行（含对比命中），
        // 补算中 → 「计算中…」行；读取失败 → 单条 error 行展示 OS 原因
        let mut rows = Vec::new();
        if let Some(err) = &job.error {
            rows.push(OutputRow {
                id: "hash-file-error",
                name: String::new(),
                value: format!("{}（{}）", t!("toolbox.hash_file_failed"), err),
                is_error: true,
                copy_value: String::new(),
                value_id: None,
                matched: false,
            });
        } else {
            for id in &job.selected {
                let Some(algo) = file_hash_algo(id) else {
                    continue;
                };
                let row_id: &'static str = match *id {
                    "md5" => "hash-file-md5",
                    "sha1" => "hash-file-sha1",
                    "sha224" => "hash-file-sha224",
                    "sha256" => "hash-file-sha256",
                    "sha384" => "hash-file-sha384",
                    _ => "hash-file-sha512",
                };
                if let Some(hex) = job.done.get(id) {
                    let mut row = OutputRow::result(row_id, algo.label.to_string(), hex.clone());
                    if !compare_target.is_empty() && normalize_hash(hex) == compare_target {
                        row = row.with_matched();
                    }
                    rows.push(row);
                } else if job.computing.contains(id) {
                    rows.push(OutputRow {
                        id: row_id,
                        name: algo.label.to_string(),
                        value: t!("toolbox.hash_file_pending").to_string(),
                        is_error: false,
                        copy_value: String::new(),
                        value_id: None,
                        matched: false,
                    });
                }
            }
        }

        // 文件条：文件图标 + 名称（超长截断）+ 大小 + ✕ 移除（回文本模式）
        let filebar = div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(36.0))
            .w_full()
            .px_2()
            .bg(theme.background)
            .rounded_md()
            .border_1()
            .border_color(if job.error.is_some() { theme.danger } else { theme.primary })
            .child(
                Icon::new(CustomIconName::File)
                    .size_3()
                    .text_color(if job.error.is_some() { theme.danger } else { theme.primary }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(job.file_name.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format_file_size(job.size_bytes)),
            )
            .child(
                div()
                    .id("hash-file-clear")
                    .cursor_pointer()
                    .flex_shrink_0()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .hover(|style| style.text_color(theme.danger))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app: &mut NetAssistantApp, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<NetAssistantApp>| {
                            app.clear_hash_file(window, cx);
                        }),
                    )
                    .child("✕"),
            );

        // 算法 chips：勾选 = 计算并展示；取消勾选仅隐藏（缓存保留，重勾零成本）
        let chips = div()
            .flex()
            .items_center()
            .gap_1p5()
            .flex_wrap()
            .w_full()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("toolbox.hash_file_algos").to_string()),
            )
            .children(FILE_HASH_ALGOS.iter().map(|algo| {
                let on = job.selected.contains(&algo.id);
                let algo_id = algo.id;
                div()
                    .id(SharedString::from(format!("hash-algo-{}", algo.id)))
                    .cursor_pointer()
                    .px_2()
                    .py_0p5()
                    .rounded_full()
                    .text_xs()
                    .font_medium()
                    .text_color(if on { theme.primary } else { theme.muted_foreground })
                    .border_1()
                    .border_color(if on { theme.primary } else { theme.border })
                    .when(on, |chip| chip.bg(theme.primary.opacity(0.1)))
                    .hover(|style| style.border_color(theme.primary).text_color(theme.primary))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app: &mut NetAssistantApp,
                                         _event: &MouseDownEvent,
                                         _window: &mut Window,
                                         cx: &mut Context<NetAssistantApp>| {
                            app.toggle_hash_algo(algo_id, cx);
                        }),
                    )
                    .child(algo.label.to_string())
            }));

        // 对比输入框（与文本模式共用；文件模式支持按长度自动勾选算法）
        let compare_extra = Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(36.0))
                .w_full()
                .px_2()
                .bg(theme.background)
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .child(
                    Icon::new(IconName::Search)
                        .size_3()
                        .text_color(theme.muted_foreground),
                )
                .child(
                    Input::new(&self.app.toolbox_hash_compare_input)
                        .cleanable(true)
                        .w_full()
                        .h_full()
                        .font_family("JetBrains Mono")
                        .bg(theme.background)
                        .border_0(),
                )
                .into_any_element(),
        );

        // 输入区 = 文件条 + 算法 chips 纵向堆叠（替代文本编辑器）
        let input_area = div()
            .flex()
            .flex_col()
            .gap_1p5()
            .w_full()
            .child(filebar)
            .child(chips)
            .into_any_element();

        Self::render_block(
            t!("toolbox.hash_title").to_string(),
            "file",
            input_area,
            rows,
            None,
            Some(self.hash_mode_switch(true, cx)),
            compare_extra,
            cx,
        )
    }

    /// 时间组块①「时间戳 → 日期」：单行输入（10 位秒 / 13 位毫秒自动识别）→
    /// 本地时间 / UTC 时间两行；块头「填入当前」写入当前毫秒值
    fn render_ts_to_date_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        let input_state = &self.app.toolbox_ts_input;
        let source = input_state.read(cx).value().trim().to_string();

        let rows = if source.is_empty() {
            vec![OutputRow::placeholder("time-ts")]
        } else {
            match parse_time_input(&source) {
                Some(view) => vec![
                    OutputRow::result(
                        "time-local",
                        t!("toolbox.time_local").to_string(),
                        view.local,
                    ),
                    OutputRow::result("time-utc", t!("toolbox.time_utc").to_string(), view.utc),
                ],
                None => vec![OutputRow::error("time-ts", String::new(), "toolbox.time_invalid")],
            }
        };

        let fill_button = |listener_id: &'static str| {
            let theme = cx.theme().clone();
            div()
                .id(listener_id)
                .px_2()
                .py_1()
                .rounded_md()
                .text_xs()
                .font_medium()
                .cursor_pointer()
                .text_color(theme.primary)
                .border_1()
                .border_color(theme.border)
                .hover(|style| style.bg(theme.border))
        };

        Self::render_single_line_block(
            t!("toolbox.time_ts_to_date").to_string(),
            input_state,
            rows,
            Some(
                fill_button("toolbox-time-fill-ts").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(
                        |app: &mut NetAssistantApp,
                         _event: &MouseDownEvent,
                         window: &mut Window,
                         cx: &mut Context<NetAssistantApp>| {
                            let now = now_millis_string();
                            app.toolbox_ts_input.update(cx, |input, cx| {
                                input.set_value(now, window, cx);
                            });
                        },
                    ),
                )
                .child(t!("toolbox.time_fill_now").to_string())
                .into_any_element(),
            ),
            cx,
        )
    }

    /// 时间组块②「日期 → 时间戳」：单行输入（多格式日期文本）→ 秒级 / 毫秒级
    /// 两行 + 底部「星期 · 相对时间」小字；块头「填入当前」写入当前本地时间
    fn render_date_to_ts_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        let theme = cx.theme().clone();
        let input_state = &self.app.toolbox_date_input;
        let source = input_state.read(cx).value().trim().to_string();

        let rows = if source.is_empty() {
            vec![OutputRow::placeholder("time-date")]
        } else {
            match parse_time_input(&source) {
                Some(view) => vec![
                    OutputRow::result(
                        "time-sec",
                        t!("toolbox.time_ts_sec").to_string(),
                        view.ts_sec,
                    ),
                    OutputRow::result("time-ms", t!("toolbox.time_ts_ms").to_string(), view.ts_ms),
                ],
                None => {
                    vec![OutputRow::error("time-date", String::new(), "toolbox.time_invalid")]
                }
            }
        };

        let fill_button = |listener_id: &'static str| {
            let theme = cx.theme().clone();
            div()
                .id(listener_id)
                .px_2()
                .py_1()
                .rounded_md()
                .text_xs()
                .font_medium()
                .cursor_pointer()
                .text_color(theme.primary)
                .border_1()
                .border_color(theme.border)
                .hover(|style| style.bg(theme.border))
        };

        let mut block = Self::render_single_line_block(
            t!("toolbox.time_date_to_ts").to_string(),
            input_state,
            rows,
            Some(
                fill_button("toolbox-time-fill-date").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(
                        |app: &mut NetAssistantApp,
                         _event: &MouseDownEvent,
                         window: &mut Window,
                         cx: &mut Context<NetAssistantApp>| {
                            let now = now_local_string();
                            app.toolbox_date_input.update(cx, |input, cx| {
                                input.set_value(now, window, cx);
                            });
                        },
                    ),
                )
                .child(t!("toolbox.time_fill_now").to_string())
                .into_any_element(),
            ),
            cx,
        );
        if let Some(view) = parse_time_input(&source) {
            block = block.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{} · {}",
                        weekday_text(view.weekday),
                        relative_text(view.relative)
                    )),
            );
        }
        block
    }

    /// 时间组单行输入块骨架：与 render_block 同构（标题 + 输入 + 输出行），输入区
    /// 为 36px 高单行 InputState（无模式徽标）；header_extra 渲染在标题行右侧
    /// （「填入当前」按钮）
    fn render_single_line_block(
        title: String,
        input_state: &Entity<InputState>,
        rows: Vec<OutputRow>,
        header_extra: Option<AnyElement>,
        cx: &Context<NetAssistantApp>,
    ) -> Div {
        let theme = cx.theme().clone();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(title),
                    )
                    .when_some(header_extra, |header, extra| header.child(extra)),
            )
            .child(
                div()
                    .h(px(36.0))
                    .w_full()
                    .bg(theme.background)
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        Input::new(input_state)
                            .cleanable(true)
                            .w_full()
                            .h_full()
                            .px_2()
                            .font_family("JetBrains Mono")
                            .bg(theme.background)
                            .rounded_md()
                            .border_0(),
                    ),
            )
            .children(
                rows.into_iter()
                    .map(|row| Self::render_output_row(row, &theme).into_any_element()),
            )
    }

    /// JSON 组工具块：左右两栏——左输入、右输出（美化/压缩 chip 切换 + 复制）。
    /// 格式化结果普遍较高，两栏并排各自限高滚动，避免单列纵排撑高页面；
    /// 解析两级回退：标准 JSON → JSON5 宽松语法（无引号 key / 单引号 / 尾逗号）
    fn render_json_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        let theme = cx.theme().clone();
        let input_state = &self.app.toolbox_json_input;
        let source = input_state.read(cx).value().to_string();
        let pretty_mode = self.app.toolbox_json_pretty;

        // 输出即时计算：空 → 占位；非法（含 JSON5 也无法转换）→ 红字报错
        let (output, is_error) = if source.trim().is_empty() {
            (String::new(), false)
        } else {
            match if pretty_mode {
                json_pretty(&source)
            } else {
                json_minify(&source)
            } {
                Some(text) => (text, false),
                None => (t!("toolbox.json_invalid").to_string(), true),
            }
        };
        let copy_value = output.clone();

        let chip = |id: &'static str, label: String, active: bool, set_pretty: bool| {
            div()
                .id(id)
                .px_2()
                .py_0p5()
                .rounded_full()
                .text_xs()
                .font_medium()
                .cursor_pointer()
                .border_1()
                .border_color(if active { theme.primary } else { theme.border })
                .when(active, |c| c.bg(theme.primary))
                .text_color(if active {
                    theme.primary_foreground
                } else {
                    theme.muted_foreground
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app: &mut NetAssistantApp,
                                     _event: &MouseDownEvent,
                                     _window: &mut Window,
                                     cx: &mut Context<NetAssistantApp>| {
                        app.toolbox_json_pretty = set_pretty;
                        cx.notify();
                    }),
                )
                .child(label)
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            // 标题行
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(t!("toolbox.json_title").to_string()),
            )
            // 左右两栏
            .child(
                div()
                    .flex()
                    .gap_2()
                    .min_h_0()
                    // 左栏：输入
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("toolbox.json_input").to_string()),
                            )
                            .child(
                                div()
                                    .relative()
                                    .h(px(320.0))
                                    .w_full()
                                    .bg(theme.background)
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .child(
                                        Editor::new(input_state)
                                            .context_menu(editor_context_menu_builder())
                                            .w_full()
                                            .h_full()
                                            .p_2()
                                            .font_family("JetBrains Mono")
                                            .bg(theme.background)
                                            .rounded_md()
                                            .border_0(),
                                    )
                                    .child(
                                        Self::editor_clear_button(input_state)
                                            .absolute()
                                            .top_1()
                                            .right_1()
                                            .opacity(if source.is_empty() { 0.25 } else { 0.6 }),
                                    ),
                            ),
                    )
                    // 右栏：chip 切换 + 复制 + 输出
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(chip(
                                        "json-chip-pretty",
                                        t!("input_mode.json_pretty").to_string(),
                                        pretty_mode,
                                        true,
                                    ))
                                    .child(chip(
                                        "json-chip-minify",
                                        t!("input_mode.json_minify").to_string(),
                                        !pretty_mode,
                                        false,
                                    ))
                                    .child(
                                        Clipboard::new("json-copy")
                                            .value(copy_value.clone())
                                            .tooltip(t!("input_mode.copy").to_string()),
                                    ),
                            )
                            .child(if output.is_empty() {
                                // 空输入占位
                                div()
                                    .h(px(320.0))
                                    .w_full()
                                    .bg(theme.secondary)
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .p_2()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("toolbox.output_empty").to_string())
                                    .into_any_element()
                            } else {
                                div()
                                    .id("json-output")
                                    .h(px(320.0))
                                    .w_full()
                                    .bg(theme.secondary)
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .p_2()
                                    .overflow_y_scrollbar()
                                    .text_sm()
                                    .font_family("JetBrains Mono")
                                    .when(is_error, |pane| {
                                        pane.text_color(theme.danger).font_medium()
                                    })
                                    .when(!is_error, |pane| pane.text_color(theme.foreground))
                                    .child(output)
                                    .into_any_element()
                            }),
                    ),
            )
    }

    /// 编码组「Base64」块：文本/Base64 同时给出编码与解码结果（各一行，方向自取）
    fn render_base64_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        let input_state = &self.app.toolbox_base64_input;
        let source = input_state.read(cx).value().to_string();

        let rows = if source.trim().is_empty() {
            vec![OutputRow::placeholder("base64")]
        } else {
            let mut rows = vec![OutputRow::result(
                "base64-encode",
                t!("toolbox.enc_encode").to_string(),
                base64_encode(&source),
            )];
            match base64_decode(&source) {
                Ok(text) => rows.push(OutputRow::result(
                    "base64-decode",
                    t!("toolbox.enc_decode").to_string(),
                    text,
                )),
                Err(()) => rows.push(OutputRow::error(
                    "base64-decode",
                    t!("toolbox.enc_decode").to_string(),
                    "toolbox.b64_invalid",
                )),
            }
            rows
        };

        Self::render_block(
            t!("toolbox.b64_title").to_string(),
            "text",
            Self::editor_area(input_state, cx),
            rows,
            None,
            None,
            None,
            cx,
        )
    }

    /// 编码组「URL」块：文本/URL 编码同时给出编码与解码结果（各一行，方向自取）
    fn render_url_block(&self, cx: &Context<NetAssistantApp>) -> Div {
        let input_state = &self.app.toolbox_url_input;
        let source = input_state.read(cx).value().to_string();

        let rows = if source.trim().is_empty() {
            vec![OutputRow::placeholder("url")]
        } else {
            let mut rows = vec![OutputRow::result(
                "url-encode",
                t!("toolbox.enc_encode").to_string(),
                url_encode(&source),
            )];
            match url_decode(&source) {
                Ok(text) => rows.push(OutputRow::result(
                    "url-decode",
                    t!("toolbox.enc_decode").to_string(),
                    text,
                )),
                Err(()) => rows.push(OutputRow::error(
                    "url-decode",
                    t!("toolbox.enc_decode").to_string(),
                    "toolbox.url_invalid",
                )),
            }
            rows
        };

        Self::render_block(
            t!("toolbox.url_title").to_string(),
            "text",
            Self::editor_area(input_state, cx),
            rows,
            None,
            None,
            None,
            cx,
        )
    }

    /// 编辑器容器右上角的清空叉号（有内容时由调用方渲染）。
    ///
    /// 点击用 `replace_all("")` 清空：保留撤销历史（Ctrl+Z 可恢复）、清选区、
    /// 重置滚动，规避 `set_value` 导致的光标重置问题。
    fn editor_clear_button(input_state: &Entity<EditorState>) -> Button {
        let input = input_state.clone();
        Button::new(("editor-clear", input_state.entity_id()))
            .icon(Icon::new(IconName::Close))
            .xsmall()
            .text()
            .tab_stop(false)
            .on_click(move |_, window, cx| {
                input.update(cx, |state, cx| state.replace_all("", window, cx));
            })
    }

    /// 块输入区通用外观：80px 等宽编辑器容器（文本类工具块共用）
    ///
    /// 右键挂自定义剪贴板菜单；有内容时右上角叠清空叉号。
    fn editor_area(input_state: &Entity<EditorState>, cx: &Context<NetAssistantApp>) -> AnyElement {
        let theme = cx.theme().clone();
        let has_content = !input_state.read(cx).value().is_empty();
        div()
            .relative()
            .h(px(80.0))
            .w_full()
            .bg(theme.background)
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .child(
                Editor::new(input_state)
                    .context_menu(editor_context_menu_builder())
                    .w_full()
                    .h_full()
                    .p_2()
                    .font_family("JetBrains Mono")
                    .bg(theme.background)
                    .rounded_md()
                    .border_0(),
            )
            // 叉号常驻：有内容较明显，空时弱显（点击空内容为无害 no-op），保证功能可发现
            .child(
                Self::editor_clear_button(input_state)
                    .absolute()
                    .top_1()
                    .right_1()
                    .opacity(if has_content { 0.6 } else { 0.25 }),
            )
            .into_any_element()
    }

    /// 工具块统一骨架：标题行（含模式徽标与可选附加动作）+ 输入区 + 输出行列表
    ///
    /// - `mode`：输入模式徽标（"hex"/"file"/"text"），空串则不渲染（时间组无单一模式）
    /// - `mid_header`：在输出行下标处插入分组小标题（哈希块「摘要 / 密码哈希」分组）
    /// - `header_extra`：标题行右侧附加元素
    fn render_block(
        title: String,
        mode: &'static str,
        // 输入区：文本块为编辑器容器（editor_area），文件摘要块为文件条 + 算法 chips
        input_area: AnyElement,
        rows: Vec<OutputRow>,
        mid_header: Option<(usize, String)>,
        header_extra: Option<AnyElement>,
        // 输入区与行区之间的插槽（哈希块放「对比哈希值」输入框，其余组为 None）
        body_extra: Option<AnyElement>,
        cx: &Context<NetAssistantApp>,
    ) -> Div {
        let theme = cx.theme().clone();
        let (mode_text_key, mode_color) = if mode == "hex" {
            ("toolbox.mode_hex", theme.primary)
        } else if mode == "file" {
            ("toolbox.mode_file", theme.primary)
        } else {
            ("toolbox.mode_text", theme.muted_foreground)
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            // 标题行：块名 + 输入模式徽标 + 附加动作（右对齐）
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child(title),
                            )
                            .when(!mode.is_empty(), |header| {
                                header.child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(mode_color)
                                        .border_1()
                                        .border_color(theme.border)
                                        .child(t!(mode_text_key).to_string()),
                                )
                            }),
                    )
                    .when_some(header_extra, |header, extra| header.child(extra)),
            )
            // 输入区（文本块：编辑器容器；哈希文件块：文件条 + 算法 chips）
            .child(input_area)
            // 输入区下方的附加内容（哈希块：对比哈希值输入框）
            .when_some(body_extra, |block, extra| block.child(extra))
            // 输出行：名称 + 值（或禁用原因/占位）+ 复制；mid_header 在指定下标处
            // 插入分组小标题（哈希块区分「摘要」与「密码哈希」两组）
            .children({
                let mut children = Vec::new();
                for (idx, row) in rows.into_iter().enumerate() {
                    if let Some((at, label)) = &mid_header {
                        if idx == *at {
                            children.push(
                                div()
                                    .pt_1()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.muted_foreground)
                                    .child(label.clone())
                                    .into_any_element(),
                            );
                        }
                    }
                    children.push(Self::render_output_row(row, &theme).into_any_element());
                }
                children
            })
    }

    /// 输出行：名称 + 值（或禁用原因/占位）+ 复制（统一骨架与密码块共用）
    fn render_output_row(row: OutputRow, theme: &Theme) -> impl IntoElement {
        div()
            .id(row.id)
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            // 行宽以容器为准：不允许超长值把行撑出块外（复制按钮永远保留在行尾）
            .min_w_0()
            .max_w_full()
            .overflow_hidden()
            .rounded_md()
            .bg(theme.secondary)
            // 对比命中：绿边框 + 绿色淡底（hover 加深同系绿，不用灰色）
            .when(row.matched, |line| {
                line.border_1().border_color(theme.success)
            })
            .hover(|style| {
                if row.matched {
                    style.bg(theme.success.opacity(0.16))
                } else {
                    style.bg(theme.border)
                }
            })
            .when(!row.name.is_empty(), |line| {
                line.child(
                    div()
                        .w_24()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .font_medium()
                        .text_color(if row.matched {
                            theme.success
                        } else {
                            theme.muted_foreground
                        })
                        .when(row.matched, |name| {
                            name.child(Icon::new(IconName::Check).size_3())
                        })
                        .child(row.name.clone()),
                )
            })
            .child(if row.is_error {
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_medium()
                    .text_color(theme.danger)
                    .child(row.value)
                    .into_any_element()
            } else if row.value.is_empty() {
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("toolbox.output_empty").to_string())
                    .into_any_element()
            } else {
                // 携带 value_id 的行（大输出）限高并纵向滚动
                match row.value_id {
                    Some(value_id) => div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_sm()
                        .font_family("JetBrains Mono")
                        .text_color(theme.foreground)
                        .id(value_id)
                        .max_h(px(240.0))
                        .overflow_y_scrollbar()
                        .child(row.value)
                        .into_any_element(),
                    None => div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_sm()
                        .font_family("JetBrains Mono")
                        .text_color(theme.foreground)
                        .child(row.value)
                        .into_any_element(),
                }
            })
            // 对比命中徽章：行尾、复制图标之前
            .when(row.matched, |line| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_0p5()
                        .px_1p5()
                        .py_0p5()
                        .rounded_full()
                        .text_xs()
                        .font_medium()
                        .text_color(theme.success)
                        .bg(theme.success.opacity(0.12))
                        .child(Icon::new(IconName::Check).size_3())
                        .child(t!("toolbox.hash_match_badge").to_string()),
                )
            })
            .when(!row.copy_value.is_empty(), |line| {
                let copy_value = row.copy_value.clone();
                // flex_shrink_0 外壳：无论值多长，图标按钮的 20px 永远预留
                line.child(
                    div().flex_shrink_0().child(
                        Clipboard::new(row.id)
                            .value(copy_value)
                            .tooltip(t!("input_mode.copy").to_string()),
                    ),
                )
            })
    }
}

/// 星期 → 本地化文案
fn weekday_text(weekday: Weekday) -> String {
    let key = match weekday {
        Weekday::Mon => "toolbox.weekday_mon",
        Weekday::Tue => "toolbox.weekday_tue",
        Weekday::Wed => "toolbox.weekday_wed",
        Weekday::Thu => "toolbox.weekday_thu",
        Weekday::Fri => "toolbox.weekday_fri",
        Weekday::Sat => "toolbox.weekday_sat",
        Weekday::Sun => "toolbox.weekday_sun",
    };
    t!(key).to_string()
}

/// 相对当前时刻 → 本地化文案（如「刚刚 / 5 分钟前 / 3 小时后」）
fn relative_text(relative: RelativeTime) -> String {
    match relative {
        RelativeTime::Now => t!("toolbox.time_rel_now").to_string(),
        RelativeTime::Minute(n, false) => t!("toolbox.time_rel_min_ago", n = n).to_string(),
        RelativeTime::Minute(n, true) => t!("toolbox.time_rel_min_later", n = n).to_string(),
        RelativeTime::Hour(n, false) => t!("toolbox.time_rel_hour_ago", n = n).to_string(),
        RelativeTime::Hour(n, true) => t!("toolbox.time_rel_hour_later", n = n).to_string(),
        RelativeTime::Day(n, false) => t!("toolbox.time_rel_day_ago", n = n).to_string(),
        RelativeTime::Day(n, true) => t!("toolbox.time_rel_day_later", n = n).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::ToolboxPanel;
    use super::normalize_hash;
    use gpui_kit::component::Root;
    use gpui_kit::component::input::EditorState;
    use gpui_kit::{
        AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, Window, div, px,
    };

    /// 回归：编辑器右上角清空叉号有内容时渲染，点击可清空
    /// （规划 [plan-toolbox-input-menu-and-clear.md]）。
    ///
    /// 右键菜单为系统原生菜单（NativeMenu），headless 下不可断言，仅覆盖叉号链路；
    /// 按钮位置受主题尺寸影响，按「右上角区域步进试探命中」手法点击，
    /// 与 input_with_mode 右键菜单用例同一模式。
    #[gpui_kit::test]
    fn editor_clear_button_clears_value(cx: &mut TestAppContext) {
        use gpui_kit::VisualTestContext;

        struct ClearHost(Entity<EditorState>);
        impl Render for ClearHost {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                // 容器固定在 (50,50)，200x80：叉号按 top_1 right_1 落在右上角附近
                div()
                    .relative()
                    .mt(px(50.0))
                    .ml(px(50.0))
                    .w(px(200.0))
                    .h(px(80.0))
                    .child(
                        ToolboxPanel::editor_clear_button(&self.0)
                            .absolute()
                            .top_1()
                            .right_1(),
                    )
            }
        }

        cx.update(gpui_kit::component::init);
        let mut input_entity: Option<Entity<EditorState>> = None;
        let (_, mut cx) = cx.add_window_view(|window, cx| {
            let input = cx.new(|cx| EditorState::new(window, cx));
            input.update(cx, |state, cx| state.replace_all("abc", window, cx));
            let host = cx.new(|_| ClearHost(input.clone()));
            input_entity = Some(input.clone());
            Root::new(host, window, cx)
        });
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        draw(&mut cx);
        draw(&mut cx);

        let value = |cx: &mut VisualTestContext| {
            cx.update(|_window, cx| {
                input_entity
                    .as_ref()
                    .expect("编辑器实体应在窗口构建时创建")
                    .read(cx)
                    .value()
                    .to_string()
            })
        };
        assert_eq!(value(&mut cx), "abc", "前置：编辑器应有初始内容");

        // 叉号在容器 (250,50) 的右上角内侧，xsmall 按钮尺寸受主题影响，步进 4px 扫描
        let mut cleared = false;
        'outer: for dy in 0..28 {
            for dx in 0..28 {
                if (dx % 4 != 0) || (dy % 4 != 0) {
                    continue;
                }
                let at = gpui_kit::point(px(250.0 - dx as f32), px(50.0 + dy as f32));
                cx.simulate_click(at, Default::default());
                draw(&mut cx);
                if value(&mut cx).is_empty() {
                    cleared = true;
                    break 'outer;
                }
            }
        }
        assert!(
            cleared,
            "点击编辑器右上角叉号应清空内容（按钮未渲染或未命中）"
        );
    }

    #[test]
    fn normalize_hash_trims_and_lowercases() {
        assert_eq!(
            normalize_hash("  9A09257515C0B32FC6D8AA6F0FBAE605EFC6C0AF0D9F9996EA39AC3310ADDB2C  "),
            "9a09257515c0b32fc6d8aa6f0fbae605efc6c0af0d9f9996ea39ac3310addb2c"
        );
    }

    #[test]
    fn normalize_hash_strips_colon_separators() {
        assert_eq!(
            normalize_hash("36:5D:16:7F:44:E1:D0:93:7D:DC:F4:D3:D3:72:4F:D0"),
            normalize_hash("365d167f44e1d0937ddcf4d3d3724fd0")
        );
    }

    #[test]
    fn normalize_hash_empty_stays_empty() {
        assert_eq!(normalize_hash("   "), "");
        assert_eq!(normalize_hash(""), "");
    }

    #[test]
    fn normalize_hash_different_values_dont_match() {
        assert_ne!(
            normalize_hash("d41d8cd98f00b204e9800998ecf8427e"),
            normalize_hash("365d167f44e1d0937ddcf4d3d3724fd0")
        );
    }
}
