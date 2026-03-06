//! Auto-update: check for new versions and upgrade.

use anyhow::Result;

#[allow(dead_code)]
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check if a newer version is available.
#[allow(dead_code)]
pub async fn check_version() -> Result<Option<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("opencoder-cli")
        .build()?;

    let resp = client
        .get("https://api.github.com/repos/anthropics/opencoder/releases/latest")
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let body: serde_json::Value = resp.json().await?;
    let tag = body["tag_name"].as_str().unwrap_or("");
    let latest = tag.strip_prefix('v').unwrap_or(tag);

    if latest.is_empty() {
        return Ok(None);
    }

    let current = semver::Version::parse(CURRENT_VERSION).ok();
    let remote = semver::Version::parse(latest).ok();

    match (current, remote) {
        (Some(c), Some(r)) if r > c => Ok(Some(latest.to_string())),
        _ => Ok(None),
    }
}

/// Print an update notification if a newer version exists.
#[allow(dead_code)]
pub async fn notify_if_available() {
    // Don't check in CI environments
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        return;
    }

    if let Ok(Some(version)) = check_version().await {
        eprintln!(
            "\x1b[33mA new version of opencoder is available: v{version} (current: v{CURRENT_VERSION})\x1b[0m"
        );
        eprintln!("\x1b[33mUpdate with: cargo install opencoder-cli\x1b[0m");
    }
}
