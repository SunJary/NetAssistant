//! Linux：只支持 AppImage。
//!
//! 1. `stage`：把新 AppImage 复制到原文件同目录（`.<name>.update-<version>`）并加上执行权限；
//! 2. `apply`：一次 rename 覆盖原文件，然后拉起新版本；
//! 3. `clean_up`：下次启动删除低版本的 `.update-*` 残留。
//!
//! deb / tar.gz 安装形态一律降级为手动下载。

use super::{InstallError, InstallPlan, Staged};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn update_prefix(name: &str) -> String {
    format!(".{name}.update-")
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// 便宜的安装形态预检查：只解析路径，不读写文件。
pub fn check_install_form() -> Result<(), InstallError> {
    if current_appimage().is_some() {
        Ok(())
    } else {
        Err(InstallError::Unsupported(
            "当前不是 AppImage 运行方式，请手动下载安装".to_string(),
        ))
    }
}

pub fn stage(plan: &InstallPlan) -> Result<Staged, InstallError> {
    if !plan.package.is_file() {
        return Err(InstallError::Failed(format!(
            "更新包不存在: {}",
            plan.package.display()
        )));
    }

    let target = current_appimage().ok_or_else(|| {
        InstallError::Unsupported("当前不是 AppImage 运行方式，请手动下载安装".to_string())
    })?;
    let name = file_name(&target);
    let staged_path = target.with_file_name(format!("{}{}", update_prefix(&name), plan.version));

    std::fs::copy(&plan.package, &staged_path).map_err(|e| {
        InstallError::Failed(format!("复制更新包失败（{e}），请检查所在目录的写入权限"))
    })?;
    std::fs::set_permissions(&staged_path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| InstallError::Failed(format!("设置执行权限失败: {e}")))?;

    Ok(Staged {
        version: plan.version.clone(),
        path: staged_path,
        target: Some(target),
    })
}

pub fn apply(staged: &Staged) -> Result<(), InstallError> {
    let target = staged
        .target
        .as_ref()
        .ok_or_else(|| InstallError::Failed("缺少安装目标路径".to_string()))?;
    if !staged.path.is_file() {
        return Err(InstallError::Failed(format!(
            "预置的更新不存在: {}",
            staged.path.display()
        )));
    }

    std::fs::set_permissions(&staged.path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| InstallError::Failed(format!("设置执行权限失败: {e}")))?;

    // 同目录 rename 是原子的；失败时退回"复制覆盖"
    if let Err(rename_error) = std::fs::rename(&staged.path, target) {
        std::fs::copy(&staged.path, target).map_err(|e| {
            InstallError::Failed(format!("替换 AppImage 失败（{rename_error}；{e}）"))
        })?;
        let _ = std::fs::remove_file(&staged.path);
    }
    let _ = std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755));

    // 开发构建不自动重启：重启用的是调试版二进制，会再次发现更新，容易陷入循环
    if cfg!(debug_assertions) {
        log::info!("[update] 开发构建不自动重启，请手动打开新版本");
        return Ok(());
    }
    Command::new(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| InstallError::Failed(format!("重新启动应用失败: {e}")))
}

pub fn clean_up(current: &str) {
    let Some(target) = current_appimage() else {
        return;
    };
    let Some(parent) = target.parent() else {
        return;
    };
    let prefix = update_prefix(&file_name(&target));
    let current_version = crate::update::verify::parse_semver(current);

    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(suffix) = name.strip_prefix(&prefix) else {
            continue;
        };
        let stale = match (crate::update::verify::parse_semver(suffix), current_version) {
            (Some(version), Some(current)) => version <= current,
            _ => true,
        };
        if stale {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// AppImage 运行时通过 `$APPIMAGE` 指明自身路径；兼容直接执行 `.AppImage` 的情况。
fn current_appimage() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let name = exe.file_name()?.to_string_lossy().to_string();
    if name.ends_with(".AppImage") {
        Some(exe)
    } else {
        None
    }
}
