//! 十六进制编辑器组件
//!
//! 三层结构（见 plans/plan-hex-editor.md）：
//! - `core`：纯逻辑（模型/解析/序列化/状态机），不依赖 gpui，可独立提取
//! - `widget`：GPUI 渲染与输入事件，主题色注入，不依赖 gpui_component/项目类型
//! - `adapter`：项目适配层（InputState 桥接、i18n、展开对话框、文件导入）

pub mod adapter;
pub mod core;
pub mod widget;

pub use widget::HexEditorState;
