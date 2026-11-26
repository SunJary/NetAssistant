//! 更新清单的数据结构。
//!
//! 格式与 shell-rs 的 `packaging/manifest.example.json` 保持同构，
//! 后续若把清单迁到 CDN，客户端不需要改协议。

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // schema / channel / published_at / notes_url 为协议字段，客户端暂不全部使用
pub struct UpdateManifest {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// 通道：一期只有 stable，字段保留用于以后扩展。
    #[serde(default)]
    pub channel: String,
    /// 版本号，不带 `v` 前缀。
    pub version: String,
    #[serde(default)]
    pub published_at: Option<String>,
    /// 更新说明（纯文本 / Markdown 均可）。客户端不使用：更新对话框只展示本地化提示。
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub notes_url: Option<String>,
    /// 平台键 → 安装包信息。
    #[serde(default)]
    pub assets: HashMap<String, Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    /// 下载地址，按顺序尝试，第一个成功即用。
    pub urls: Vec<String>,
    #[serde(default)]
    pub size: u64,
    /// 小写十六进制 SHA-256。
    pub sha256: String,
}

fn default_schema() -> u32 {
    1
}

impl UpdateManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let manifest: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("解析更新清单失败: {e}"))?;
        if manifest.version.trim().is_empty() {
            return Err("更新清单缺少版本号".to_string());
        }
        Ok(manifest)
    }

    /// 当前平台对应的安装包；`None` 表示该平台不支持自动更新。
    pub fn asset_for_current_platform(&self) -> Option<&Asset> {
        self.assets.get(current_platform_key())
    }
}

/// 当前平台的清单键。
///
/// 与 `release.yml` 产物对齐：
/// - `netassistant-windows-x86_64-setup.exe` → `windows-x86_64`
/// - `netassistant-macos-universal.zip` → `macos-universal`
/// - `netassistant-linux-x86_64.AppImage` → `linux-x86_64`
///
/// Windows ARM64、Linux ARM64 等目前没有自动更新产物，返回的键在清单里不存在，
/// 调用方据此降级为手动下载。
pub fn current_platform_key() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            "windows-x86_64"
        } else {
            "windows-aarch64"
        }
    } else if cfg!(target_os = "macos") {
        "macos-universal"
    } else if cfg!(target_arch = "x86_64") {
        "linux-x86_64"
    } else {
        "linux-aarch64"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "schema": 1,
        "channel": "stable",
        "version": "1.6.0",
        "assets": {
            "windows-x86_64": {
                "urls": ["https://github.com/SunJary/NetAssistant/releases/download/v1.6.0/netassistant-windows-x86_64-setup.exe"],
                "size": 123,
                "sha256": "abc"
            }
        }
    }"#;

    #[test]
    fn parse_manifest() {
        let m = UpdateManifest::parse(SAMPLE.as_bytes()).expect("应当解析成功");
        assert_eq!(m.version, "1.6.0");
        assert_eq!(m.schema, 1);
        assert!(m.assets.contains_key("windows-x86_64"));
    }

    #[test]
    fn reject_missing_version() {
        let bad = r#"{"assets": {}}"#;
        assert!(UpdateManifest::parse(bad.as_bytes()).is_err());
    }

    #[test]
    fn platform_key_is_known() {
        // 只验证返回值非空；具体平台由编译目标决定。
        assert!(!current_platform_key().is_empty());
    }
}
