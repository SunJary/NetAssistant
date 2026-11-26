//! GitHub 相关请求：目前只保留 star 数获取。
//!
//! 版本检查已迁移到 `crate::update`（读取 Release 资产 `update.json`）。

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
