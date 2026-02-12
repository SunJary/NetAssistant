use gpui::SharedString;
use gpui_component::{IconNamed, IconName};

/// 自定义图标名称枚举，扩展了 gpui-component 的 IconName
#[derive(Clone)]
pub enum CustomIconName {
    // 包含所有 gpui-component 内置图标
    IconName(IconName),
    // 添加自定义图标
    FilePlusCorner,
}

impl From<IconName> for CustomIconName {
    fn from(value: IconName) -> Self {
        CustomIconName::IconName(value)
    }
}

impl IconNamed for CustomIconName {
    fn path(self) -> SharedString {
        match self {
            // 转发内置图标路径
            CustomIconName::IconName(icon_name) => icon_name.path(),
            // 自定义图标路径（使用与内置图标相同的格式）
            CustomIconName::FilePlusCorner => "icons/file-plus-corner.svg".into(),
        }
    }
}
