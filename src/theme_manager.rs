use gpui::App;
use gpui_component::{Theme, ThemeRegistry, ThemeSet};
use log::info;
use std::rc::Rc;

// 使用原始字符串字面量来避免Rust 2021的前缀语法问题
const NETASSISTANT_THEME: &str = include_str!("../themes/na-theme.json");

pub struct ThemeManager {}

impl ThemeManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&mut self, cx: &mut App) {
        info!("初始化主题系统...");

        // 注册内嵌主题到 ThemeRegistry，使后续 apply_theme 可直接从 Registry 查找
        if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(NETASSISTANT_THEME) {
            info!("注册内嵌主题失败: {}", err);
            return;
        }
        info!("=== 内嵌主题已注册到 ThemeRegistry ===");

        // 应用第一个主题作为初始主题
        if let Ok(theme_set) = serde_json::from_str::<ThemeSet>(NETASSISTANT_THEME) {
            if let Some(theme) = theme_set.themes.first() {
                let theme_rc = Rc::new(theme.clone());
                Theme::global_mut(cx).apply_config(&theme_rc);
                info!("使用内嵌主题: {}", theme.name);
            }
        }
    }
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}
