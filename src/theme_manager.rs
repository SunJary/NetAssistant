use gpui::{App, SharedString};
use gpui_component::{Theme, ThemeRegistry};
use std::path::PathBuf;
use log::info;

pub struct ThemeManager {
}

impl ThemeManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&mut self, cx: &mut App) {
        info!("初始化主题系统...");
        
        // 默认使用浅色主题作为后备
        let theme_name = SharedString::from("Custom Light");

        info!("使用主题: {}", theme_name);

        // 使用绝对路径确保能找到主题目录
        let themes_path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("themes");

        info!("主题目录: {:?}", themes_path);

        if let Err(err) = ThemeRegistry::watch_dir(themes_path, cx, move |cx| {
            // 先获取可用主题列表
            let available_themes: Vec<_> = ThemeRegistry::global(cx)
                .themes()
                .keys()
                .cloned()
                .collect();
            info!("可用主题: {:?}", available_themes);

            // 尝试使用指定的主题
            if let Some(theme) = ThemeRegistry::global(cx)
                .themes()
                .get(&theme_name)
                .cloned()
            {
                // 创建一个新的作用域来避免借用冲突
                {
                    Theme::global_mut(cx).apply_config(&theme);
                }
                info!("主题已应用: {}", theme_name);
            } else {
                info!("主题 {} 未找到，使用默认主题", theme_name);
                // 尝试使用第一个可用的主题
                if !available_themes.is_empty() {
                    let first_theme_name = available_themes[0].clone();
                    if let Some(theme) = ThemeRegistry::global(cx)
                        .themes()
                        .get(&first_theme_name)
                        .cloned()
                    {
                        // 创建一个新的作用域来避免借用冲突
                        {
                            Theme::global_mut(cx).apply_config(&theme);
                        }
                        info!("使用默认主题: {}", first_theme_name);
                    }
                }
            }
        }) {
            info!("无法监听主题目录: {}", err);
        }
    }
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}
