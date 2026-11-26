//! macOS：下载后把新 bundle 解压到安装目录旁边的隐藏目录，退出时两次 rename 替换。
//!
//! 1. `stage`：`ditto` 解压 → 校验 `CFBundleShortVersionString` → 校验签名（仅告警）；
//! 2. `apply`：旧包改名为 `.<App>.old-<id>`，新包改名为正式名，失败回滚；
//! 3. `clean_up`：下次启动删除 `.old-*` 和低版本的 `.update-*`。

use super::{InstallError, InstallPlan, Staged};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `.<App>.update-`：预置目录前缀。
fn update_prefix(name: &str) -> String {
    format!(".{name}.update-")
}

/// `.<App>.old-`：替换时旧包暂存前缀。
fn old_prefix(name: &str) -> String {
    format!(".{name}.old-")
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<String, InstallError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| InstallError::Failed(format!("运行 {program} 失败: {e}")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(InstallError::Failed(format!(
            "{program} 执行失败: {}",
            stderr.trim()
        )))
    }
}

/// 当前运行的 `.app`：`<App>.app/Contents/MacOS/<binary>`。
fn installed_bundle() -> Result<PathBuf, InstallError> {
    let exe = std::env::current_exe()
        .map_err(|e| InstallError::Failed(format!("获取当前程序路径失败: {e}")))?;
    let bundle = exe
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .ok_or_else(|| {
            InstallError::Unsupported("当前不是 .app 安装形态，请手动下载安装".to_string())
        })?;
    if bundle.to_string_lossy().contains("/AppTranslocation/") {
        return Err(InstallError::Unsupported(
            "应用正从隔离位置运行，请先把它拖到「应用程序」文件夹再更新".to_string(),
        ));
    }
    if bundle.to_string_lossy().starts_with("/Volumes/") {
        return Err(InstallError::Unsupported(
            "应用正在磁盘映像中运行，请先把它拖到「应用程序」文件夹再更新".to_string(),
        ));
    }
    Ok(bundle.to_path_buf())
}

/// 便宜的安装形态预检查：只解析路径，不读写文件。
pub fn check_install_form() -> Result<(), InstallError> {
    installed_bundle().map(|_| ())
}

fn bundle_name(bundle: &Path) -> String {
    bundle
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub fn stage(plan: &InstallPlan) -> Result<Staged, InstallError> {
    if !plan.package.is_file() {
        return Err(InstallError::Failed(format!(
            "更新包不存在: {}",
            plan.package.display()
        )));
    }

    let bundle = installed_bundle()?;
    let name = bundle_name(&bundle);
    let parent = bundle
        .parent()
        .ok_or_else(|| InstallError::Failed("无法定位应用所在目录".to_string()))?;
    let staging = parent.join(format!("{}{}", update_prefix(&name), plan.version));

    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging).map_err(|e| {
        InstallError::Failed(format!("无法在应用目录创建更新目录（{e}），请检查权限"))
    })?;

    let unpack = run(
        "/usr/bin/ditto",
        &[
            "-x".as_ref(),
            "-k".as_ref(),
            plan.package.as_os_str(),
            staging.as_os_str(),
        ],
    );
    if let Err(e) = unpack {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    let new_app = match std::fs::read_dir(&staging)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|e| e.to_str()) == Some("app"))
    {
        Some(path) => path,
        None => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(InstallError::Failed("更新包内没有找到 .app".to_string()));
        }
    };

    // 版本必须和清单一致，防止拿错包
    let version = run(
        "/usr/bin/plutil",
        &[
            "-extract".as_ref(),
            "CFBundleShortVersionString".as_ref(),
            "raw".as_ref(),
            "-o".as_ref(),
            "-".as_ref(),
            new_app.join("Contents/Info.plist").as_os_str(),
        ],
    );
    match version {
        Ok(found)
            if crate::update::verify::normalize_version(&found)
                == crate::update::verify::normalize_version(&plan.version) => {}
        Ok(found) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(InstallError::Failed(format!(
                "更新包版本不符（期望 {}，实际 {found}）",
                plan.version
            )));
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    }

    // 签名校验只做提示：本项目发布的是 ad-hoc 签名，失败不阻断更新
    if let Err(e) = run(
        "/usr/bin/codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            new_app.as_os_str(),
        ],
    ) {
        log::warn!("[update] 新包签名校验未通过（继续安装）: {e}");
    }

    Ok(Staged {
        version: plan.version.clone(),
        path: new_app,
        target: Some(bundle),
    })
}

pub fn apply(staged: &Staged) -> Result<(), InstallError> {
    let target = staged
        .target
        .as_ref()
        .ok_or_else(|| InstallError::Failed("缺少安装目标路径".to_string()))?;
    if !staged.path.exists() {
        return Err(InstallError::Failed(format!(
            "预置的更新不存在: {}",
            staged.path.display()
        )));
    }

    let name = bundle_name(target);
    let id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let aside = target.with_file_name(format!("{}{}", old_prefix(&name), id));

    std::fs::rename(target, &aside)
        .map_err(|e| InstallError::Failed(format!("暂存旧版本失败: {e}")))?;
    if let Err(e) = std::fs::rename(&staged.path, target) {
        let _ = std::fs::rename(&aside, target);
        return Err(InstallError::Failed(format!(
            "替换新版本失败（已回滚）: {e}"
        )));
    }
    // 预置目录此时已空
    if let Some(dir) = staged.path.parent() {
        let _ = std::fs::remove_dir(dir);
    }

    // 开发构建不自动重启：重启用的是调试版二进制，会再次发现更新，容易陷入循环
    if cfg!(debug_assertions) {
        log::info!("[update] 开发构建不自动重启，请手动打开新版本");
        return Ok(());
    }
    if let Err(e) = Command::new("/usr/bin/open").arg("-n").arg(target).spawn() {
        log::warn!("[update] 重新启动应用失败: {e}");
    }
    Ok(())
}

pub fn clean_up(current: &str) {
    let Ok(bundle) = installed_bundle() else {
        return;
    };
    let Some(parent) = bundle.parent() else {
        return;
    };
    let name = bundle_name(&bundle);
    let update = update_prefix(&name);
    let old = old_prefix(&name);

    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with(&old) {
            let _ = std::fs::remove_dir_all(&path);
            continue;
        }
        if let Some(suffix) = file_name.strip_prefix(&update) {
            let stale = match crate::update::verify::parse_semver(suffix) {
                Some(version) => {
                    version <= crate::update::verify::parse_semver(current).unwrap_or((0, 0, 0))
                }
                None => true,
            };
            if stale {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}
