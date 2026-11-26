//! 更新包下载：断点续传、进度上报、SHA-256 校验、失败重试、旧版本缓存清理。

use super::manifest::Asset;
use super::verify;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// 整体请求超时：安装包 20–30MB，给足余量。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// 下载进度（供 UI 轮询）。
#[derive(Debug, Default)]
pub struct DownloadProgress {
    pub received: AtomicU64,
    pub total: AtomicU64,
}

impl DownloadProgress {
    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.received.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// 下载缓存目录：`<cache>/NetAssistant/updates/`。
pub fn update_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("NetAssistant")
        .join("updates")
}

/// 清理旧版本缓存：删除版本号低于当前版本的目录。
///
/// 在应用启动后调用；当前版本的目录保留，下次启动可直接复用安装包。
pub fn clean_old_versions(current: &str) {
    let Some(cur) = verify::parse_semver(current) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(update_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(version) = verify::parse_semver(name) else {
            continue;
        };
        if version < cur {
            match std::fs::remove_dir_all(&path) {
                Ok(()) => log::debug!("[update] 已清理旧版本缓存: {}", path.display()),
                Err(e) => log::debug!("[update] 清理旧版本缓存失败 {}: {e}", path.display()),
            }
        }
    }
}

/// 用户可感知的「下载」目录；取不到时退回主目录 / 临时目录。
pub fn download_target_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir)
}

/// 手动下载：保存到用户指定的目录，保留资产原始文件名。
pub async fn download_to(
    asset: &Asset,
    dir: &Path,
    progress: Arc<DownloadProgress>,
) -> Result<PathBuf, String> {
    download_into(asset, dir, progress).await
}

/// 依次尝试清单里的每个地址（整体最多两轮），成功后返回落盘路径。
///
/// 已存在且校验通过的安装包会被直接复用，不会重复下载。
pub async fn download_asset(
    asset: &Asset,
    version: &str,
    progress: Arc<DownloadProgress>,
) -> Result<PathBuf, String> {
    let dir = update_dir().join(verify::normalize_version(version));
    download_into(asset, &dir, progress).await
}

async fn download_into(
    asset: &Asset,
    dir: &Path,
    progress: Arc<DownloadProgress>,
) -> Result<PathBuf, String> {
    if asset.urls.is_empty() {
        return Err("更新清单没有为该平台提供下载地址".to_string());
    }

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("创建更新目录失败: {e}"))?;

    let file_name = asset
        .urls
        .iter()
        .find_map(|url| file_name_from_url(url))
        .unwrap_or_else(|| "netassistant-update.bin".to_string());
    let final_path = dir.join(&file_name);
    let part_path = dir.join(format!("{file_name}.part"));

    progress.total.store(asset.size, Ordering::Relaxed);

    // 1. 已下载过且校验通过：直接复用
    if tokio::fs::metadata(&final_path).await.is_ok() && verify_package(&final_path, asset).await {
        log::debug!("[update] 复用已校验的安装包: {}", final_path.display());
        progress.received.store(asset.size, Ordering::Relaxed);
        return Ok(final_path);
    }

    // 2. 上次已下载完整但没来得及改名：校验后直接转正
    if let Ok(meta) = tokio::fs::metadata(&part_path).await {
        let len = meta.len();
        if asset.size > 0 && len == asset.size && verify_package(&part_path, asset).await {
            tokio::fs::rename(&part_path, &final_path)
                .await
                .map_err(|e| format!("保存安装包失败: {e}"))?;
            log::debug!("[update] 校验并复用已完成的下载: {}", final_path.display());
            progress.received.store(len, Ordering::Relaxed);
            return Ok(final_path);
        }
        if asset.size > 0 && len > asset.size {
            let _ = tokio::fs::remove_file(&part_path).await;
        }
    }

    let mut errors: Vec<String> = Vec::new();
    for _round in 0..2 {
        for url in &asset.urls {
            if !verify::is_trusted_download_url(url) {
                errors.push(format!("{url}: 不在信任的下载域名内"));
                continue;
            }

            match try_download(url, &part_path, &final_path, asset, &progress).await {
                Ok(()) => return Ok(final_path),
                Err(e) => {
                    // 网络中断时保留 .part，下一轮/下次启动继续续传；
                    // 校验类错误会在内部删除损坏文件。
                    errors.push(format!("{url}: {e}"));
                }
            }
        }
    }

    Err(format!("下载更新包失败：{}", errors.join("；")))
}

async fn try_download(
    url: &str,
    part_path: &Path,
    final_path: &Path,
    asset: &Asset,
    progress: &DownloadProgress,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let existing = tokio::fs::metadata(part_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let mut request = client
        .get(url)
        .header("User-Agent", format!("NetAssistant/{}", env!("APP_VERSION")));
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }

    let mut resp = request
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // 本地片段已经和服务端不一致，丢掉重来
        let _ = tokio::fs::remove_file(part_path).await;
        return Err("续传位置无效（HTTP 416），已丢弃本地片段".to_string());
    }
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    if !verify::is_trusted_redirect(resp.url().as_str()) {
        return Err("重定向后的地址不在信任范围内".to_string());
    }

    let resuming = existing > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    if resuming {
        let start = content_range_start(&resp).ok_or_else(|| "服务器未返回有效的 Content-Range".to_string())?;
        if start != existing {
            let _ = tokio::fs::remove_file(part_path).await;
            return Err(format!("续传起点不符（期望 {existing}，实际 {start}）"));
        }
    }

    // 服务端忽略 Range（返回 200）时从头开始，truncate 掉旧片段。
    let mut received: u64 = if resuming { existing } else { 0 };
    let declared = resp.content_length().unwrap_or(0);
    let total = if asset.size > 0 {
        asset.size
    } else if resuming {
        existing + declared
    } else {
        declared
    };
    progress.total.store(total, Ordering::Relaxed);
    progress.received.store(received, Ordering::Relaxed);

    let mut file = if resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(part_path)
            .await
            .map_err(|e| format!("打开临时文件失败: {e}"))?
    } else {
        tokio::fs::File::create(part_path)
            .await
            .map_err(|e| format!("创建临时文件失败: {e}"))?
    };

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入临时文件失败: {e}"))?;
        received += chunk.len() as u64;
        progress.received.store(received, Ordering::Relaxed);
    }

    file.flush()
        .await
        .map_err(|e| format!("刷新临时文件失败: {e}"))?;
    drop(file);

    if asset.size > 0 && received != asset.size {
        return Err(format!(
            "文件大小不符（期望 {}，实际 {received}）",
            asset.size
        ));
    }

    let actual = sha256_hex(part_path).await?;
    if !actual.eq_ignore_ascii_case(asset.sha256.trim()) {
        // 内容损坏：丢掉片段，下一轮从头下
        let _ = tokio::fs::remove_file(part_path).await;
        return Err(format!(
            "SHA-256 校验失败（期望 {}，实际 {actual}）",
            asset.sha256
        ));
    }

    tokio::fs::rename(part_path, final_path)
        .await
        .map_err(|e| format!("保存安装包失败: {e}"))?;
    Ok(())
}

/// 文件大小与 SHA-256 是否都符合清单声明。
async fn verify_package(path: &Path, asset: &Asset) -> bool {
    if asset.size > 0 {
        match tokio::fs::metadata(path).await {
            Ok(meta) if meta.len() == asset.size => {}
            _ => return false,
        }
    }
    match sha256_hex(path).await {
        Ok(actual) => actual.eq_ignore_ascii_case(asset.sha256.trim()),
        Err(_) => false,
    }
}

/// 流式计算文件 SHA-256（放到阻塞线程池，避免卡住 tokio worker）。
async fn sha256_hex(path: &Path) -> Result<String, String> {
    use std::io::Read;

    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok::<String, std::io::Error>(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(|e| format!("校验任务异常: {e}"))?
    .map_err(|e| format!("读取安装包失败: {e}"))
}

/// 解析 `Content-Range: bytes 100-199/200` 的起始偏移。
fn content_range_start(resp: &reqwest::Response) -> Option<u64> {
    let value = resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    let range = value.trim().strip_prefix("bytes")?.trim();
    let start = range.split('-').next()?.trim();
    start.parse().ok()
}

/// 从下载地址提取安全的文件名。
fn file_name_from_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let segment = parsed.path_segments()?.next_back()?;
    let safe: String = segment
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if safe.is_empty() || safe == "." || safe == ".." {
        None
    } else {
        Some(safe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_file_name() {
        assert_eq!(
            file_name_from_url(
                "https://github.com/SunJary/NetAssistant/releases/download/v1.6.0/netassistant-windows-x86_64-setup.exe"
            ),
            Some("netassistant-windows-x86_64-setup.exe".to_string())
        );
        assert_eq!(
            file_name_from_url("https://github.com/a/../../etc/passwd"),
            Some("passwd".to_string())
        );
        assert_eq!(file_name_from_url("https://github.com/"), None);
    }
}
