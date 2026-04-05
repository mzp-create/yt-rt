use anyhow::{bail, Context};
use serde_json::json;
use yt_dlp_networking::client::HttpClient;

use super::types::*;

const INNERTUBE_BASE: &str = "https://www.youtube.com/youtubei/v1";

pub struct InnertubeApi<'a> {
    client: &'a HttpClient,
}

impl<'a> InnertubeApi<'a> {
    pub fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    /// Call /player endpoint to get streaming data and video details
    pub async fn player(
        &self,
        video_id: &str,
        innertube_client: &InnertubeClient,
    ) -> anyhow::Result<PlayerResponse> {
        let url = format!("{INNERTUBE_BASE}/player?key={}", innertube_client.api_key);
        let payload = json!({
            "videoId": video_id,
            "context": {
                "client": {
                    "clientName": innertube_client.client_name,
                    "clientVersion": innertube_client.client_version,
                    "hl": "en",
                    "gl": "US",
                }
            },
            "playbackContext": {
                "contentPlaybackContext": {
                    "signatureTimestamp": 0
                }
            }
        });

        let resp = self
            .client
            .inner()
            .post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", innertube_client.user_agent)
            .header("X-YouTube-Client-Name", innertube_client.client_name)
            .header(
                "X-YouTube-Client-Version",
                innertube_client.client_version,
            )
            .json(&payload)
            .send()
            .await
            .context("innertube player request failed")?;

        let body = resp.text().await?;
        let player_response: PlayerResponse =
            serde_json::from_str(&body).context("failed to parse player response")?;

        // Check playability
        if let Some(status) = &player_response.playability_status {
            match status.status.as_str() {
                "OK" | "LIVE_STREAM_OFFLINE" => {}
                "LOGIN_REQUIRED" => bail!("video requires login"),
                "UNPLAYABLE" => bail!(
                    "video is unplayable: {}",
                    status.reason.as_deref().unwrap_or("unknown reason")
                ),
                "ERROR" => bail!(
                    "video error: {}",
                    status.reason.as_deref().unwrap_or("unknown error")
                ),
                other => tracing::warn!(status = other, "unknown playability status"),
            }
        }

        Ok(player_response)
    }

    /// Call /player with signature timestamp for decrypted content
    pub async fn player_with_sts(
        &self,
        video_id: &str,
        innertube_client: &InnertubeClient,
        signature_timestamp: u64,
    ) -> anyhow::Result<PlayerResponse> {
        let url = format!("{INNERTUBE_BASE}/player?key={}", innertube_client.api_key);
        let payload = json!({
            "videoId": video_id,
            "context": {
                "client": {
                    "clientName": innertube_client.client_name,
                    "clientVersion": innertube_client.client_version,
                    "hl": "en",
                    "gl": "US",
                }
            },
            "playbackContext": {
                "contentPlaybackContext": {
                    "signatureTimestamp": signature_timestamp
                }
            }
        });

        let resp = self
            .client
            .inner()
            .post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", innertube_client.user_agent)
            .json(&payload)
            .send()
            .await?;

        Ok(resp.json().await?)
    }

    /// Browse endpoint for playlists, channels, search
    pub async fn browse(
        &self,
        browse_id: &str,
        params: Option<&str>,
        innertube_client: &InnertubeClient,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!("{INNERTUBE_BASE}/browse?key={}", innertube_client.api_key);
        let mut payload = json!({
            "browseId": browse_id,
            "context": {
                "client": {
                    "clientName": innertube_client.client_name,
                    "clientVersion": innertube_client.client_version,
                    "hl": "en",
                    "gl": "US",
                }
            }
        });
        if let Some(p) = params {
            payload["params"] = json!(p);
        }

        let resp = self
            .client
            .inner()
            .post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", innertube_client.user_agent)
            .json(&payload)
            .send()
            .await?;

        Ok(resp.json().await?)
    }

    /// Search endpoint
    pub async fn search(
        &self,
        query: &str,
        innertube_client: &InnertubeClient,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!("{INNERTUBE_BASE}/search?key={}", innertube_client.api_key);
        let payload = json!({
            "query": query,
            "context": {
                "client": {
                    "clientName": innertube_client.client_name,
                    "clientVersion": innertube_client.client_version,
                    "hl": "en",
                    "gl": "US",
                }
            }
        });

        let resp = self
            .client
            .inner()
            .post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", innertube_client.user_agent)
            .json(&payload)
            .send()
            .await?;

        Ok(resp.json().await?)
    }
}
