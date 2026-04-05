use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use serde_json::json;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

const TWITCH_CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";
const GQL_URL: &str = "https://gql.twitch.tv/gql";

pub struct TwitchExtractor;

impl TwitchExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract VOD ID from twitch.tv/videos/ID URLs.
    fn extract_vod_id(url: &str) -> Option<String> {
        let re = Regex::new(r"twitch\.tv/videos/(\d+)").ok()?;
        re.captures(url).and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
    }

    /// Extract clip slug from clip URLs.
    fn extract_clip_slug(url: &str) -> Option<String> {
        // clips.twitch.tv/SLUG or twitch.tv/USER/clip/SLUG
        let patterns = [
            r"clips\.twitch\.tv/([A-Za-z0-9_-]+)",
            r"twitch\.tv/[^/]+/clip/([A-Za-z0-9_-]+)",
        ];
        for pat in &patterns {
            if let Ok(re) = Regex::new(pat) {
                if let Some(caps) = re.captures(url) {
                    return caps.get(1).map(|m| m.as_str().to_string());
                }
            }
        }
        None
    }

    /// Fetch VOD playback access token via GQL.
    async fn fetch_vod_token(
        client: &HttpClient,
        vod_id: &str,
    ) -> anyhow::Result<(String, String, serde_json::Value)> {
        let body = json!({
            "operationName": "PlaybackAccessToken",
            "variables": {
                "isLive": false,
                "login": "",
                "isVod": true,
                "vodID": vod_id,
                "playerType": "embed"
            },
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "0828119ded1c13477966434e15800ff57ddacf13ba1911c129dc2200705b0712"
                }
            }
        });

        let resp = client
            .request(reqwest::Method::POST, GQL_URL)
            .header("Client-ID", TWITCH_CLIENT_ID)
            .json(&body)
            .send()
            .await?;
        let data: serde_json::Value = resp.json().await?;

        let token_obj = &data["data"]["videoPlaybackAccessToken"];
        let token = token_obj["value"]
            .as_str()
            .context("missing token value")?
            .to_string();
        let sig = token_obj["signature"]
            .as_str()
            .context("missing token signature")?
            .to_string();

        // Also fetch video metadata
        let meta_body = json!([{
            "operationName": "VideoMetadata",
            "variables": { "channelLogin": "", "videoID": vod_id },
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "cb3b1eb2f2d2b2f65b8389ba446ec89d6fa94f4a09c7f1d4d3b1843c5f33ef70"
                }
            }
        }]);

        let meta_resp = client
            .request(reqwest::Method::POST, GQL_URL)
            .header("Client-ID", TWITCH_CLIENT_ID)
            .json(&meta_body)
            .send()
            .await?;
        let meta: serde_json::Value = meta_resp.json().await?;
        let video = meta
            .as_array()
            .and_then(|a| a.first())
            .map(|v| v["data"]["video"].clone())
            .unwrap_or(serde_json::Value::Null);

        Ok((token, sig, video))
    }

    /// Fetch clip info via GQL.
    async fn fetch_clip_info(
        client: &HttpClient,
        slug: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let body = json!([{
            "operationName": "VideoAccessToken_Clip",
            "variables": { "slug": slug },
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "36b89d2507fce29e5ca551df756d27c1cfe079e2609642b4390aa4c35796eb11"
                }
            }
        }]);

        let resp = client
            .request(reqwest::Method::POST, GQL_URL)
            .header("Client-ID", TWITCH_CLIENT_ID)
            .json(&body)
            .send()
            .await?;
        let data: serde_json::Value = resp.json().await?;
        let clip = data
            .as_array()
            .and_then(|a| a.first())
            .map(|v| v["data"]["clip"].clone())
            .unwrap_or(serde_json::Value::Null);
        Ok(clip)
    }

    /// Parse HLS master playlist into Format entries.
    fn parse_m3u8_formats(manifest: &str, manifest_url: &str) -> Vec<Format> {
        let mut formats = Vec::new();
        let lines: Vec<&str> = manifest.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if line.starts_with("#EXT-X-STREAM-INF:") {
                // Parse resolution and bandwidth
                let attrs = line.trim_start_matches("#EXT-X-STREAM-INF:");
                let bandwidth = Self::parse_attr(attrs, "BANDWIDTH")
                    .and_then(|v| v.parse::<f64>().ok());
                let resolution = Self::parse_attr(attrs, "RESOLUTION");
                let video_name = Self::parse_attr(attrs, "VIDEO")
                    .unwrap_or_else(|| "unknown".to_string());

                let (width, height) = resolution
                    .as_deref()
                    .and_then(|r| {
                        let parts: Vec<&str> = r.split('x').collect();
                        if parts.len() == 2 {
                            Some((parts[0].parse::<u32>().ok()?, parts[1].parse::<u32>().ok()?))
                        } else {
                            None
                        }
                    })
                    .unwrap_or((0, 0));

                if i + 1 < lines.len() {
                    let stream_url = lines[i + 1].trim();
                    if !stream_url.is_empty() && !stream_url.starts_with('#') {
                        formats.push(Format {
                            format_id: video_name.clone(),
                            format_note: resolution.clone(),
                            ext: "mp4".to_string(),
                            url: Some(stream_url.to_string()),
                            manifest_url: Some(manifest_url.to_string()),
                            protocol: Protocol::Hls,
                            width: if width > 0 { Some(width) } else { None },
                            height: if height > 0 { Some(height) } else { None },
                            tbr: bandwidth.map(|b| b / 1000.0),
                            vcodec: Some("avc1".to_string()),
                            acodec: Some("aac".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
            i += 1;
        }
        formats
    }

    fn parse_attr(attrs: &str, key: &str) -> Option<String> {
        let search = format!("{key}=");
        attrs.split(',').find_map(|part| {
            let trimmed = part.trim();
            if trimmed.starts_with(&search) {
                Some(trimmed[search.len()..].trim_matches('"').to_string())
            } else {
                None
            }
        })
    }
}

impl Default for TwitchExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for TwitchExtractor {
    fn name(&self) -> &str {
        "Twitch"
    }

    fn key(&self) -> &str {
        "Twitch"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"(?:https?://)?(?:www\.)?twitch\.tv/videos/\d+",
            r"(?:https?://)?(?:www\.)?twitch\.tv/[^/]+/clip/[A-Za-z0-9_-]+",
            r"(?:https?://)?clips\.twitch\.tv/[A-Za-z0-9_-]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            // Determine if this is a VOD or a clip
            if let Some(vod_id) = Self::extract_vod_id(url) {
                info!(vod_id = %vod_id, "extracting Twitch VOD");

                let (token, sig, video_meta) = Self::fetch_vod_token(client, &vod_id).await?;

                let encoded_token = url::form_urlencoded::byte_serialize(token.as_bytes())
                    .collect::<String>();
                let manifest_url = format!(
                    "https://usher.ttvnw.net/vod/{vod_id}.m3u8?sig={sig}&token={encoded_token}&allow_source=true&allow_audio_only=true",
                );

                let manifest = client.get_text(&manifest_url).await?;
                debug!(lines = manifest.lines().count(), "fetched HLS manifest");

                let formats = Self::parse_m3u8_formats(&manifest, &manifest_url);
                let title = video_meta["title"].as_str().map(|s| s.to_string());
                let uploader = video_meta["owner"]["displayName"].as_str().map(|s| s.to_string());
                let duration = video_meta["lengthSeconds"].as_f64();

                let info = InfoDict {
                    id: vod_id.clone(),
                    title: title.clone(),
                    fulltitle: title,
                    ext: "mp4".to_string(),
                    webpage_url: Some(format!("https://www.twitch.tv/videos/{vod_id}")),
                    original_url: Some(url.to_string()),
                    display_id: Some(vod_id),
                    uploader,
                    duration,
                    formats,
                    extractor: "twitch".to_string(),
                    extractor_key: "Twitch".to_string(),
                    ..default_info_dict()
                };

                Ok(ExtractionResult::SingleVideo(Box::new(info)))
            } else if let Some(slug) = Self::extract_clip_slug(url) {
                info!(slug = %slug, "extracting Twitch clip");

                let clip = Self::fetch_clip_info(client, &slug).await?;
                let title = clip["title"].as_str().map(|s| s.to_string());
                let uploader = clip["broadcaster"]["displayName"].as_str().map(|s| s.to_string());
                let duration = clip["durationSeconds"].as_f64();

                let mut formats = Vec::new();
                if let Some(qualities) = clip["videoQualities"].as_array() {
                    for q in qualities {
                        let quality = q["quality"].as_str().unwrap_or("unknown");
                        let source_url = q["sourceURL"].as_str().unwrap_or("");
                        if !source_url.is_empty() {
                            formats.push(Format {
                                format_id: quality.to_string(),
                                format_note: Some(format!("{quality}p")),
                                ext: "mp4".to_string(),
                                url: Some(source_url.to_string()),
                                protocol: Protocol::Https,
                                height: quality.parse().ok(),
                                vcodec: Some("avc1".to_string()),
                                acodec: Some("aac".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }

                let info = InfoDict {
                    id: slug.clone(),
                    title: title.clone(),
                    fulltitle: title,
                    ext: "mp4".to_string(),
                    webpage_url: Some(format!("https://clips.twitch.tv/{slug}")),
                    original_url: Some(url.to_string()),
                    display_id: Some(slug),
                    uploader,
                    duration,
                    formats,
                    extractor: "twitch:clip".to_string(),
                    extractor_key: "TwitchClip".to_string(),
                    ..default_info_dict()
                };

                Ok(ExtractionResult::SingleVideo(Box::new(info)))
            } else {
                anyhow::bail!("could not determine Twitch URL type: {url}");
            }
        })
    }
}

/// Helper to create an InfoDict with sensible defaults for non-YouTube extractors.
fn default_info_dict() -> InfoDict {
    InfoDict {
        id: String::new(),
        title: None,
        fulltitle: None,
        ext: "mp4".to_string(),
        url: None,
        webpage_url: None,
        original_url: None,
        display_id: None,
        description: None,
        uploader: None,
        uploader_id: None,
        uploader_url: None,
        channel: None,
        channel_id: None,
        channel_url: None,
        duration: None,
        view_count: None,
        like_count: None,
        comment_count: None,
        upload_date: None,
        timestamp: None,
        age_limit: None,
        categories: Vec::new(),
        tags: Vec::new(),
        is_live: None,
        was_live: None,
        live_status: None,
        release_timestamp: None,
        formats: Vec::new(),
        requested_formats: None,
        subtitles: HashMap::new(),
        automatic_captions: HashMap::new(),
        thumbnails: Vec::new(),
        thumbnail: None,
        chapters: Vec::new(),
        playlist: None,
        playlist_id: None,
        playlist_title: None,
        playlist_index: None,
        n_entries: None,
        extractor: String::new(),
        extractor_key: String::new(),
        extra: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_vod_id() {
        assert_eq!(
            TwitchExtractor::extract_vod_id("https://www.twitch.tv/videos/123456789"),
            Some("123456789".to_string())
        );
    }

    #[test]
    fn test_extract_clip_slug() {
        assert_eq!(
            TwitchExtractor::extract_clip_slug("https://clips.twitch.tv/AwesomeClipSlug"),
            Some("AwesomeClipSlug".to_string())
        );
        assert_eq!(
            TwitchExtractor::extract_clip_slug("https://www.twitch.tv/user/clip/MyClip-123"),
            Some("MyClip-123".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = TwitchExtractor::new();
        assert!(ext.suitable("https://www.twitch.tv/videos/123456789"));
        assert!(ext.suitable("https://clips.twitch.tv/SomeClipSlug"));
        assert!(ext.suitable("https://www.twitch.tv/user/clip/ClipSlug"));
        assert!(!ext.suitable("https://www.youtube.com/watch?v=abc"));
    }
}
