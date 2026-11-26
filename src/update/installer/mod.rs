//! 更新安装：下载后预置（stage），退出时替换（apply）。
//!
//! - Windows：交给 PowerShell 守卫进程，等本进程退出后静默运行 Inno 安装包；
//! - macOS：下载后即解压并校验到 `.<App>.update-<version>`，退出时两次 rename 完成替换；
//! - Linux：下载后复制到 AppImage 同目录，退出时一次 rename 覆盖。
//!
//! mac / Linux 的预置都在下载完成后完成，退出时只做 rename，失败可回滚；
//! 下次启动由 `clean_up` 清理 `.old-*` 和低版本的 `.update-*`。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// 待安装的更新包（下载完成后产生）。
#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub version: String,
    pub package: PathBuf,
}

/// 已预置、等待安装的更新。
#[derive(Debug, Clone)]
pub struct Staged {
    pub version: String,
    /// 新文件：Windows 为安装包；macOS 为新 `.app`；Linux 为新 AppImage。
    pub path: PathBuf,
    /// 要替换的目标：macOS 的 `.app`、Linux 的 AppImage；Windows 为 `None`。
    #[allow(dead_code)] // Windows 构建只读 `path`
    pub target: Option<PathBuf>,
}

/// 进程级待安装队列。
///
/// 不放在 `NetAssistantApp` 里：Windows 版 gpui 在窗口关闭时可能跳过 `App::shutdown`，
/// 用全局状态保证 `main` 在事件循环结束后一定能拿到。
static PENDING_INSTALL: OnceLock<Mutex<Option<Staged>>> = OnceLock::new();

/// 用户是否点了「立即重启并更新」。
///
/// 开发构建只在显式请求时才安装，避免每次关窗口都装一遍、重启后又是旧版本、再关再装的循环。
static EXPLICIT_INSTALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 记录"用户主动要求立刻安装"。
pub fn request_install_now() {
    EXPLICIT_INSTALL.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn pending_slot() -> &'static Mutex<Option<Staged>> {
    PENDING_INSTALL.get_or_init(|| Mutex::new(None))
}

/// 便宜的安装形态预检查：只读路径，不做网络/解压。
///
/// 用于在下载开始前就判定"这台机器能不能自动更新"，
/// 避免便携版 / dmg 用户白白下载几十 MB。
pub fn auto_update_supported() -> Result<(), InstallError> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe()
            .map_err(|e| InstallError::Failed(format!("获取当前程序路径失败: {e}")))?;
        if windows::is_portable_build(&exe) {
            return Err(InstallError::Unsupported(
                "当前是便携版运行，请手动下载安装包".to_string(),
            ));
        }
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        return macos::check_install_form();
    }
    #[cfg(target_os = "linux")]
    {
        return linux::check_install_form();
    }
    #[allow(unreachable_code)]
    Err(InstallError::Unsupported(
        "当前平台不支持自动更新".to_string(),
    ))
}

/// 手动下载时优先使用的资产键：按安装形态给出更合适的包。
///
/// 例如 Windows 便携版应下载便携 zip（而不是安装包），Linux 非 AppImage
/// 形态应下载 tar.gz。清单里没有对应键时调用方回退到标准资产。
pub fn manual_asset_key() -> Option<&'static str> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().ok()?;
        if windows::is_portable_build(&exe) {
            return Some("windows-x86_64-portable");
        }
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        if linux::check_install_form().is_err() {
            return Some("linux-x86_64-tar-gz");
        }
        return None;
    }
    #[allow(unreachable_code)]
    None
}

/// 下载完成后预置更新（解压 / 复制 / 校验）。
///
/// 阻塞调用（会解压几十 MB），调用方应放到阻塞线程池。
pub fn stage(plan: &InstallPlan) -> Result<Staged, InstallError> {
    #[cfg(target_os = "windows")]
    {
        // 便携版没有卸载器，装 Inno 包会额外产生一份安装，改为手动下载
        if let Ok(exe) = std::env::current_exe() {
            if windows::is_portable_build(&exe) {
                return Err(InstallError::Unsupported(
                    "当前是便携版运行，请手动下载安装包".to_string(),
                ));
            }
        }
        return Ok(Staged {
            version: plan.version.clone(),
            path: plan.package.clone(),
            target: None,
        });
    }
    #[cfg(target_os = "macos")]
    {
        return macos::stage(plan);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::stage(plan);
    }
    #[allow(unreachable_code)]
    Err(InstallError::Unsupported(
        "当前平台不支持自动更新".to_string(),
    ))
}

/// 预置成功后登记，退出时由 `install_pending` 安装。
pub fn set_pending(staged: Staged) {
    if let Ok(mut slot) = pending_slot().lock() {
        *slot = Some(staged);
    }
}

fn take_pending() -> Option<Staged> {
    pending_slot().lock().ok().and_then(|mut slot| slot.take())
}

/// 在 `app.run` 返回之后调用，把已预置的更新换上去。
///
/// 不在退出钩子里做：Windows 上创建进程可能泵消息循环、重入 GPUI，
/// 所以要等事件循环彻底结束。可重复调用（第二次取到 `None` 直接返回）。
pub fn install_pending() {
    let explicit = EXPLICIT_INSTALL.swap(false, std::sync::atomic::Ordering::SeqCst);
    // 开发构建：普通退出不安装。否则 debug 版（版本号低于清单）每次关闭都会装一次，
    // 重启后又拉起旧版本，形成"关一次装一次"的循环。只能用「立即重启并更新」触发。
    if cfg!(debug_assertions) && !explicit {
        if pending_slot().lock().map(|slot| slot.is_some()).unwrap_or(false) {
            log::info!("[update] 开发构建跳过退出时安装（点击「立即重启并更新」可强制执行）");
        }
        return;
    }

    if let Some(staged) = take_pending() {
        // 回执先落盘：新版本启动时据此确认更新是否成功
        crate::update::record_applied(env!("APP_VERSION"), &staged.version);
        match apply(&staged) {
            Ok(()) => log::info!("[update] 已安装 v{}", staged.version),
            Err(e) => {
                log::error!("[update] 安装失败: {e}");
                eprintln!("[update] 安装失败: {e}");
            }
        }
    } else {
        log::debug!("[update] 没有待安装的更新");
    }
}

/// 启动时清理上次安装留下的临时文件。
pub fn clean_up(current: &str) {
    #[cfg(target_os = "macos")]
    {
        macos::clean_up(current);
    }
    #[cfg(target_os = "linux")]
    {
        linux::clean_up(current);
    }
    let _ = current;
}

fn apply(staged: &Staged) -> Result<(), InstallError> {
    #[cfg(target_os = "windows")]
    {
        return windows::apply(staged);
    }
    #[cfg(target_os = "macos")]
    {
        return macos::apply(staged);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::apply(staged);
    }
    #[allow(unreachable_code)]
    Err(InstallError::Unsupported(
        "当前平台不支持自动更新".to_string(),
    ))
}

#[derive(Debug)]
pub enum InstallError {
    /// 当前安装形态不支持自动更新（如 deb / tar.gz / 非 .app 运行）。
    Unsupported(String),
    /// 预置或安装失败。
    Failed(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Unsupported(msg) | InstallError::Failed(msg) => f.write_str(msg),
        }
    }
}
