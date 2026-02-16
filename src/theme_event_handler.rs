use gpui::*;
use gpui_component::theme::{Theme, ThemeRegistry, ThemeSet};
use std::rc::Rc;
use log::info;

// 内嵌主题JSON
const NETASSISTANT_THEME: &str = include_str!("../themes/na-theme.json");

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




    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {

        Theme::global_mut(cx).apply_config(&theme);
        info!("=== 主题已成功应用: {} ===", theme_name);
    } else {
        info!("主题 {} 未找到，尝试从内嵌主题加载", theme_name);
        
        // 尝试从内嵌主题加载
        match serde_json::from_str::<ThemeSet>(NETASSISTANT_THEME) {
            Ok(theme_set) => {
                for theme in &theme_set.themes {
                    if theme.name == theme_name {

                        let theme_rc = Rc::new(theme.clone());
                        Theme::global_mut(cx).apply_config(&theme_rc);
                        info!("=== 内嵌主题已成功应用: {} ===", theme_name);
                        // 通知UI更新以应用新主题
                        cx.refresh_windows();

                        return;
                    }
                }
                
                // 如果还是找不到，回退到默认主题
                info!("内嵌主题中也未找到 {}, 回退到默认主题", theme_name);
            },
            Err(err) => {
                info!("解析内嵌主题失败: {}, 回退到默认主题", err);
            }
        }
        
        // 回退到默认主题
        let fallback_theme_name = if is_dark_mode {
            SharedString::from("Default Dark")
        } else {
            SharedString::from("Default Light")
        };
        
        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&fallback_theme_name).cloned() {

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
