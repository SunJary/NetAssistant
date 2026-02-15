use gpui::*;
use gpui_component::theme::{Theme, ThemeRegistry};
use log::info;

impl Global for ThemeEventHandler {}

pub struct ThemeEventHandler {
    is_dark_mode: bool,
}

impl ThemeEventHandler {
    pub fn new() -> Self {
        Self {
            is_dark_mode: false,
        }
    }

    pub fn is_dark_mode(&self) -> bool {
        self.is_dark_mode
    }

    pub fn set_is_dark_mode(&mut self, is_dark: bool) {
        if self.is_dark_mode != is_dark {
            self.is_dark_mode = is_dark;
            info!(
                "系统主题变化，更新为: {}",
                if is_dark { "Dark" } else { "Light" }
            );
        }
    }

    pub fn toggle_theme(&mut self) {
        self.is_dark_mode = !self.is_dark_mode;
        info!(
            "手动切换主题: {}",
            if self.is_dark_mode { "Dark" } else { "Light" }
        );
    }
}

pub fn apply_theme(is_dark_mode: bool, cx: &mut App) {
    let theme_name = if is_dark_mode {
        SharedString::from("Custom Dark")
    } else {
        SharedString::from("Custom Light")
    };

    info!("=== 开始应用主题: {} ===", theme_name);

    // 打印可用主题列表
    let available_themes: Vec<_> = ThemeRegistry::global(cx)
        .themes()
        .keys()
        .cloned()
        .collect();
    info!("可用主题列表: {:?}", available_themes);

    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        info!("找到主题: {}, 开始应用配置", theme_name);
        Theme::global_mut(cx).apply_config(&theme);
        info!("=== 主题已成功应用: {} ===", theme_name);
    } else {
        info!("主题 {} 未找到", theme_name);
        // 如果自定义主题未找到，回退到默认主题
        let fallback_theme_name = if is_dark_mode {
            SharedString::from("Default Dark")
        } else {
            SharedString::from("Default Light")
        };
        
        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&fallback_theme_name).cloned() {
            info!("回退到默认主题: {}, 开始应用", fallback_theme_name);
            Theme::global_mut(cx).apply_config(&theme);
            info!("=== 默认主题已成功应用: {} ===", fallback_theme_name);
        } else {
            info!("默认主题 {} 也未找到", fallback_theme_name);
        }
    }
    
    // 通知UI更新以应用新主题
    cx.refresh_windows();
    info!("=== 主题应用流程完成，UI已更新 ===");
}
