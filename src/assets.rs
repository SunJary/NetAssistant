use gpui::AssetSource;
use gpui_component_assets::Assets as DefaultAssets;
use rust_embed::{RustEmbed};
use std::borrow::Cow;

// 自定义图标资源
#[derive(RustEmbed)]
#[folder = "assets/icons"]
pub struct CustomIcons;

// 组合自定义图标和默认图标
pub struct CustomAssets {
    default_assets: DefaultAssets,
}

impl CustomAssets {
    pub fn new() -> Self {
        Self {
            default_assets: DefaultAssets,
        }
    }
}

impl AssetSource for CustomAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        // 首先尝试从自定义图标中加载（处理两种路径格式）
        if let Some(data) = CustomIcons::get(path) {
            return Ok(Some(data.data));
        }
        // 尝试去除 "icons/" 前缀后查找
        if let Some(without_prefix) = path.strip_prefix("icons/") {
            if let Some(data) = CustomIcons::get(without_prefix) {
                return Ok(Some(data.data));
            }
        }
        // 如果自定义图标中没有，则从默认图标中加载
        self.default_assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        // 合并自定义图标和默认图标的列表
        let mut default_list = self.default_assets.list(path)?;
        let custom_list = CustomIcons::iter()
            .filter(|p: &Cow<'static, str>| p.starts_with(path))
            .map(|p| gpui::SharedString::from(p.clone()))
            .collect::<Vec<_>>();
        
        default_list.extend(custom_list);
        Ok(default_list)
    }
}
