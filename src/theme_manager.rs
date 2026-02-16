use gpui::App;
use gpui_component::{Theme, ThemeSet};
use std::rc::Rc;
use log::info;

// 使用原始字符串字面量来避免Rust 2021的前缀语法问题
const NETASSISTANT_THEME: &str = include_str!("../themes/na-theme.json");

pub struct ThemeManager {
}

impl ThemeManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&mut self, cx: &mut App) {
        info!("初始化主题系统...");
        
        // 解析内嵌主题JSON
        let theme_set: ThemeSet = match serde_json::from_str(NETASSISTANT_THEME) {
            Ok(set) => set,
            Err(err) => {
                info!("解析内嵌主题失败: {}", err);
                // 如果解析失败，使用默认主题
                return;
            }
        };
        
        // 手动应用主题配置
        for theme in &theme_set.themes {
            // 应用主题到当前活动主题
            let theme_rc = Rc::new(theme.clone());
            Theme::global_mut(cx).apply_config(&theme_rc);
            info!("使用内嵌主题: {}", theme.name);
            break;
        }
    }
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}
