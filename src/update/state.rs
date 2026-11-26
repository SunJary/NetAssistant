//! 更新状态：供 UI 渲染与退出钩子读取。

use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdatePhase {
    /// 没有检查过，或检查完成且已是最新。
    #[default]
    Idle,
    /// 发现新版本，尚未开始下载。
    Available { version: String, size: u64 },
    /// 正在下载。
    Downloading {
        version: String,
        received: u64,
        total: u64,
    },
    /// 下载并校验完成，等待退出时安装。
    Ready { version: String, package: PathBuf },
    /// 检查或下载失败。
    Failed { message: String },
}

#[derive(Debug, Default)]
pub struct UpdateState {
    pub phase: UpdatePhase,
    /// 当前版本的下载信息，手动下载时使用。
    pub asset: Option<crate::update::manifest::Asset>,
    /// 手动下载完成的安装包路径。
    pub downloaded_package: Option<PathBuf>,
    /// 是否自动弹出标题栏提示。
    pub show_tooltip: bool,
    /// 更新对话框是否打开。
    pub dialog_open: bool,
    /// 当前平台没有自动更新产物，只能手动下载。
    pub manual_only: bool,
    /// 启动时的上次更新结果提示（成功?, 版本）。
    ///
    /// 不能在 `NetAssistantApp::new` 里直接弹通知：那时 gpui-component 的 Root
    /// 还没挂到窗口上，`push_notification` 会 panic。改为首帧渲染时消费。
    pub applied_notice: Option<(bool, String)>,
}

impl UpdateState {
    /// 有更新但还没准备好（红点状态）。
    pub fn is_available(&self) -> bool {
        matches!(
            self.phase,
            UpdatePhase::Available { .. } | UpdatePhase::Downloading { .. }
        )
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.phase, UpdatePhase::Ready { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.phase, UpdatePhase::Failed { .. })
    }

    /// 是否有任何与更新相关的状态需要展示（红点 / 绿点）。
    pub fn has_update(&self) -> bool {
        self.is_available() || self.is_ready() || self.is_failed()
    }

    pub fn ready_package(&self) -> Option<(String, PathBuf)> {
        match &self.phase {
            UpdatePhase::Ready { version, package } => Some((version.clone(), package.clone())),
            _ => None,
        }
    }
}
