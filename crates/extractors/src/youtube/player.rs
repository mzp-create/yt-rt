use anyhow::{bail, Context};
use regex::Regex;
use yt_dlp_networking::client::HttpClient;

/// Extracted player.js information
pub struct PlayerInfo {
    pub player_url: String,
    pub player_js: String,
    pub signature_timestamp: u64,
}

/// Fetch the YouTube watch page and extract the player.js URL
pub async fn extract_player_url(
    client: &HttpClient,
    video_id: &str,
) -> anyhow::Result<String> {
    let watch_url = format!(
        "https://www.youtube.com/watch?v={video_id}&bpctr=9999999999&has_verified=1"
    );
    let html = client
        .get_text(&watch_url)
        .await
        .context("failed to fetch watch page")?;

    // Extract player URL from the watch page
    // Pattern: "jsUrl":"/s/player/HASH/player_ias.vflset/en_US/base.js"
    let re = Regex::new(r#""jsUrl"\s*:\s*"([^"]*?/base\.js)""#)?;
    if let Some(caps) = re.captures(&html) {
        let path = caps.get(1).unwrap().as_str();
        if path.starts_with("http") {
            return Ok(path.to_string());
        }
        return Ok(format!("https://www.youtube.com{path}"));
    }

    // Fallback: look for player URL in script src
    let re2 = Regex::new(r#"src="([^"]*?/base\.js)""#)?;
    if let Some(caps) = re2.captures(&html) {
        let path = caps.get(1).unwrap().as_str();
        if path.starts_with("http") {
            return Ok(path.to_string());
        }
        return Ok(format!("https://www.youtube.com{path}"));
    }

    bail!("could not find player.js URL in watch page")
}

/// Fetch player.js and extract the signature timestamp
pub async fn fetch_player(
    client: &HttpClient,
    player_url: &str,
) -> anyhow::Result<PlayerInfo> {
    let player_js = client
        .get_text(player_url)
        .await
        .context("failed to fetch player.js")?;

    // Extract signature timestamp (signatureTimestamp or sts)
    let sts = extract_signature_timestamp(&player_js).unwrap_or(0);

    Ok(PlayerInfo {
        player_url: player_url.to_string(),
        player_js,
        signature_timestamp: sts,
    })
}

/// Extract signatureTimestamp from player.js
fn extract_signature_timestamp(js: &str) -> Option<u64> {
    // Pattern: signatureTimestamp:12345 or sts:12345
    let re = Regex::new(r"(?:signatureTimestamp|sts)\s*:\s*(\d{5,})").ok()?;
    re.captures(js)?.get(1)?.as_str().parse().ok()
}

/// Try to extract initial player response from the watch page HTML (embedded JSON)
pub fn extract_initial_player_response(html: &str) -> Option<serde_json::Value> {
    let re = Regex::new(r"var\s+ytInitialPlayerResponse\s*=\s*(\{.+?\})\s*;").ok()?;
    if let Some(caps) = re.captures(html) {
        return serde_json::from_str(caps.get(1)?.as_str()).ok();
    }
    // Try alternative pattern
    let re2 = Regex::new(r"ytInitialPlayerResponse\s*=\s*(\{.+?\})\s*;").ok()?;
    if let Some(caps) = re2.captures(html) {
        return serde_json::from_str(caps.get(1)?.as_str()).ok();
    }
    None
}
