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
        SharedString::from("NetAssistant Dark")
    } else {
        SharedString::from("NetAssistant Light")
    };

    info!("=== 开始应用主题: {} ===", theme_name);

    // 主题已在 ThemeManager::init() 中注册到 ThemeRegistry，直接查找
    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        Theme::global_mut(cx).apply_config(&theme);
        info!("=== 主题已成功应用: {} ===", theme_name);
    } else {
        info!("主题 {} 未在 Registry 中找到", theme_name);
    }

    cx.refresh_windows();
    info!("=== 主题应用流程完成，UI已更新 ===");
}
