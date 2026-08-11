use log::debug;
use serde::Deserialize;
use std::time::Duration;

const GITHUB_OWNER: &str = "SunJary";
const GITHUB_REPO: &str = "NetAssistant";
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// GitHub API 响应结构
#[derive(Deserialize)]
struct GitHubRepo {
    stargazers_count: u32,
}

/// 解析 semver tag，如 `v1.2.3` → `(1, 2, 3)`
pub fn parse_semver(tag: &str) -> Option<(u32, u32, u32)> {
    let tag = tag.trim_start_matches('v');
    let parts: Vec<&str> = tag.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].split('-').next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// 比较版本号，返回 true 表示 latest > current
pub fn is_newer_version(current: &str, latest: &str) -> bool {
    let cur = match parse_semver(current) {
        Some(v) => v,
        None => return false,
    };
    let new = match parse_semver(latest) {
        Some(v) => v,
        None => return false,
    };
    new > cur
}

/// 判断是否应该提示更新
/// - 当前版本无法解析为 semver（如开发版日期号 20260808）→ 有最新版本即提示
/// - 当前版本可解析 → 比较 semver 版本号
pub fn should_show_update(current: &str, latest: &str) -> bool {
    if parse_semver(current).is_none() {
        return true;
    }
    is_newer_version(current, latest)
}

/// 检查最新版本号
///
/// 请求 HTML 重定向端点 `https://github.com/{owner}/{repo}/releases/latest`，
/// 跟随 302 重定向，从最终 URL `/releases/tag/v1.2.3` 解析 tag。
/// 不受 API 限流影响。
pub async fn check_latest_version() -> Option<String> {
    let url = format!(
        "https://github.com/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );

    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .ok()?;

    let resp = client.get(&url).send().await.ok()?;
    let final_url = resp.url().to_string();
    debug!("[update_checker] releases/latest 最终 URL: {}", final_url);

    // 从 /releases/tag/v1.2.3 解析 tag
    if let Some(tag_pos) = final_url.find("/releases/tag/") {
        let tag = &final_url[tag_pos + "/releases/tag/".len()..];
        let tag = tag.split('/').next().unwrap_or(tag);
        if !tag.is_empty() {
            return Some(tag.to_string());
        }
    }

    None
}

/// 获取 GitHub star 数
///
/// 请求 API 端点 `GET https://api.github.com/repos/{owner}/{repo}`，
/// 解析 `stargazers_count`。每次启动仅消耗 1 次 API 配额。
pub async fn fetch_star_count() -> Option<u32> {
    let url = format!(
        "https://api.github.com/repos/{}/{}",
        GITHUB_OWNER, GITHUB_REPO
    );

    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .ok()?;

    let resp = client
        .get(&url)
        .header(
            "User-Agent",
            format!("NetAssistant/{}", env!("APP_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        debug!("[update_checker] GitHub API 返回状态码: {}", resp.status());
        return None;
    }

    let body = resp.text().await.ok()?;
    let repo: GitHubRepo = serde_json::from_str(&body).ok()?;
    Some(repo.stargazers_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("v1.10.0"), Some((1, 10, 0)));
        assert_eq!(parse_semver("v1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(parse_semver("v1.2"), None);
        assert_eq!(parse_semver("invalid"), None);
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("v1.0.0", "v1.0.1"));
        assert!(is_newer_version("v1.9.0", "v1.10.0"));
        assert!(is_newer_version("v1.0.0", "v2.0.0"));
        assert!(!is_newer_version("v1.0.0", "v1.0.0"));
        assert!(!is_newer_version("v1.2.0", "v1.0.0"));
        assert!(!is_newer_version("invalid", "v1.0.0"));
    }

    #[test]
    fn test_should_show_update_dev_version() {
        // 开发版日期号 → 任何最新版本都提示
        assert!(should_show_update("20260808", "v1.0.0"));
        assert!(should_show_update("20260808", "v0.0.1"));
    }

    #[test]
    fn test_should_show_update_release_version() {
        // 发布版 → 正常 semver 比较
        assert!(should_show_update("v1.0.0", "v1.0.1"));
        assert!(!should_show_update("v1.2.0", "v1.0.0"));
        assert!(!should_show_update("v1.0.0", "v1.0.0"));
    }
}
