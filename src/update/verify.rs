//! 版本比较与下载地址白名单。

/// 允许的 GitHub 域名（含 302 之后的对象存储域名）。
const TRUSTED_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "codeload.github.com",
];

/// 本项目 Release 资产路径前缀。
const RELEASES_PREFIX: &str = "/SunJary/NetAssistant/releases/";

/// 去掉 `v` 前缀与首尾空白。
pub fn normalize_version(v: &str) -> &str {
    let v = v.trim();
    v.strip_prefix('v')
        .or_else(|| v.strip_prefix('V'))
        .unwrap_or(v)
}

/// 开发版版本号：`build.rs` 缺省生成的 `%Y%m%d`（8 位纯数字）。
pub fn is_dev_version(v: &str) -> bool {
    let t = normalize_version(v);
    t.len() == 8 && t.bytes().all(|b| b.is_ascii_digit())
}

/// 解析 semver 的 major/minor/patch，忽略预发布与构建元数据。
pub fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let t = normalize_version(v);
    let core = t.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// `latest` 是否比 `current` 新。任一侧无法解析时返回 false。
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(cur), Some(new)) => new > cur,
        _ => false,
    }
}

/// 初始下载地址是否可信：必须是 https + GitHub 域名 + 本项目 Release 路径。
pub fn is_trusted_download_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if !TRUSTED_HOSTS.contains(&host) {
        return false;
    }
    if host == "github.com" {
        parsed.path().starts_with(RELEASES_PREFIX)
    } else {
        true
    }
}

/// 重定向后的最终地址是否仍可信（GitHub 会 302 到对象存储域名）。
pub fn is_trusted_redirect(url: &str) -> bool {
    is_trusted_download_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("v1.9.0", "v1.10.0"));
        assert!(is_newer("1.5.1", "1.6.0"));
        assert!(!is_newer("v1.6.0", "v1.6.0"));
        assert!(!is_newer("v1.6.0", "v1.5.9"));
        assert!(is_newer("v1.0.0", "2.0.0-beta.1"));
        assert!(!is_newer("20260808", "v1.6.0"));
    }

    #[test]
    fn dev_version_detection() {
        assert!(is_dev_version("20260808"));
        assert!(is_dev_version("v20260808"));
        assert!(!is_dev_version("1.6.0"));
    }

    #[test]
    fn trusted_urls() {
        assert!(is_trusted_download_url(
            "https://github.com/SunJary/NetAssistant/releases/download/v1.6.0/netassistant-windows-x86_64-setup.exe"
        ));
        assert!(!is_trusted_download_url(
            "https://github.com/other/NetAssistant/releases/download/v1.6.0/a.exe"
        ));
        assert!(!is_trusted_download_url(
            "http://github.com/SunJary/NetAssistant/releases/download/v1.6.0/a.exe"
        ));
        assert!(!is_trusted_download_url("https://example.com/a.exe"));
        assert!(is_trusted_download_url(
            "https://objects.githubusercontent.com/github-production-release-asset/1/2"
        ));
    }
}
