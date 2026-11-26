//! 应用内自动更新。
//!
//! 一期范围（见 `plans/plan-auto-update.md`）：
//! - 清单来自 GitHub Release 资产 `update.json`（固定地址 `releases/latest/download/`）；
//! - 只信任 GitHub 域名，下载后校验 SHA-256；
//! - 后台下载，应用退出时执行安装；
//! - 覆盖 Windows x64 / macOS universal / Linux x86_64 AppImage，其余平台降级为手动下载。

pub mod downloader;
pub mod installer;
pub mod manifest;
pub mod state;
pub mod verify;

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 更新清单固定地址：始终保持指向最新的正式版本。
pub const MANIFEST_URL: &str =
    "https://github.com/SunJary/NetAssistant/releases/latest/download/update.json";

/// 手动下载页（自动更新不可用时使用）。
pub const RELEASES_PAGE: &str = "https://github.com/SunJary/NetAssistant/releases/latest";

/// 清单请求超时：国内访问 GitHub 冷连接握手可能超过 5 秒，留足余量。
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 拉取并解析更新清单。
pub async fn fetch_manifest() -> Result<manifest::UpdateManifest, String> {
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let resp = client
        .get(MANIFEST_URL)
        .header("User-Agent", format!("NetAssistant/{}", env!("APP_VERSION")))
        .send()
        .await
        .map_err(|e| format!("请求更新清单失败: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("更新清单返回状态码 {}", resp.status()));
    }

    if !verify::is_trusted_redirect(resp.url().as_str()) {
        return Err("更新清单地址不在信任范围内".to_string());
    }

    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("读取更新清单失败: {e}"))?;
    debug!("[update] 更新清单长度: {} 字节", body.len());

    manifest::UpdateManifest::parse(&body)
}

/// 安装前写下的回执：记录"从哪个版本升到哪个版本"。
#[derive(Debug, Serialize, Deserialize)]
pub struct AppliedUpdate {
    pub from: String,
    pub to: String,
}

fn applied_path() -> PathBuf {
    downloader::update_dir().join("applied.json")
}

/// 启动安装前调用：记录本次更新，供新版本启动时确认结果。
pub fn record_applied(from: &str, to: &str) {
    let data = AppliedUpdate {
        from: from.to_string(),
        to: to.to_string(),
    };
    let path = applied_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec(&data)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
    {
        Ok(()) => debug!("[update] 已记录安装回执: {from} -> {to}"),
        Err(e) => warn!("[update] 写安装回执失败: {e}"),
    }
}

/// 读取并删除安装回执（读到即消费，避免重复提示）。
pub fn take_applied() -> Option<AppliedUpdate> {
    let path = applied_path();
    let bytes = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    serde_json::from_slice(&bytes).ok()
}
