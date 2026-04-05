use anyhow::bail;
use serde::Deserialize;

const GITHUB_REPO: &str = "user/yt-dlp-rs";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[allow(dead_code)]
    size: u64,
}

/// Check for updates by querying the GitHub releases API.
pub async fn check_update() -> anyhow::Result<()> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "yt-dlp-rs")
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!("failed to check for updates: HTTP {}", resp.status());
    }

    let release: GithubRelease = resp.json().await?;
    let latest = release.tag_name.trim_start_matches('v');

    if latest == CURRENT_VERSION {
        println!("yt-dlp-rs is up to date (v{CURRENT_VERSION})");
    } else {
        println!("Update available: v{CURRENT_VERSION} -> v{latest}");
        println!("Release: {}", release.html_url);

        // Find asset for current platform
        let target = current_target();
        if let Some(asset) = release.assets.iter().find(|a| a.name.contains(&target)) {
            println!("Download: {}", asset.browser_download_url);
            println!("\nTo update, download the binary and replace the current executable.");
        } else {
            println!("No prebuilt binary found for {target}");
            println!("Build from source: cargo install yt-dlp-rs");
        }
    }
    Ok(())
}

fn current_target() -> String {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };
    format!("{os}-{arch}")
}
