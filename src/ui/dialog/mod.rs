mod add_client;
mod decoder_selection;
mod favorite_list;
mod favorite_remark;
mod new_connection;
mod port_limit_help;
mod stress_config;
mod variable_picker;

pub use add_client::open_add_client_dialog;
pub use decoder_selection::{open_decoder_selection_dialog, DecoderSelectionDialogState};
pub use favorite_list::FavoriteListPanel;
pub use favorite_remark::open_favorite_remark_dialog;
pub use new_connection::open_new_connection_dialog;
pub use stress_config::{StressConfigDialog, StressConfigDialogState};

use gpui::{px, Pixels, Window};

/// 对话框标准最大高度：窗口高度 × 0.8，永不超过窗口高度 - 32px，下限 240。
///
/// Dialog 自带 top 偏移(视口/10)，公式已留足余量；
/// 最小窗口 600×300 时结果 = 240，保证标题与底部按钮始终在窗口内。
///
/// 用 min/max 链而非 clamp：clamp(min > max) 会 panic，而链式写法无前置条件，
/// 异常 bounds（如 DPI 缩放导致的极端值）下也能安全退化到下限 240。
pub fn dialog_height(window: &Window) -> Pixels {
    let win_h = (window.bounds().size.height / px(1.0)) as f32;
    px((win_h * 0.8).min(win_h - 32.0).max(240.0))
}

/// 内容区滚动上限：dialog_height 预留标题 + 底部按钮 + 上下内边距 + 间距的固定高度
/// （pt/pb 16×2 + 标题行 ~20 + 按钮行 ~32 + 间距 8×2 + 边框 2 ≈ 102，取 128 余量）。
///
/// 使用结构（经 `tests/dialog_layout.rs` 无头测试验证）——max_h 放外层普通 div 钳制可视区，
/// `overflow_y_scrollbar()` 放内层 div 且**自身不限高**：
///
/// ```ignore
/// div().max_h(dialog_content_max_height(window)).child(
///     div().overflow_y_scrollbar().child(表单),
/// )
/// ```
///
/// 不能把 max_h 直接放在滚动容器上：`overflow_y_scrollbar` 会把该元素保留为滚动跟踪元素，
/// 其自身高度被钳成可视区高度后，滚动机制认为内容未溢出，滚轮不响应；
/// 也不能不设上限依赖 Dialog 面板的 flex 收缩（面板高度是 auto，收缩不可靠）。
pub fn dialog_content_max_height(window: &Window) -> Pixels {
    dialog_height(window) - px(128.0)
}
