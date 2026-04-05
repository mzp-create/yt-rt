use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

/// Vimeo video extractor.
///
/// Fetches video config from the player embed endpoint.
pub struct VimeoExtractor;

impl VimeoExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract video ID from Vimeo URLs.
    fn extract_video_id(url: &str) -> Option<String> {
        let re = Regex::new(
            r"(?:vimeo\.com/|player\.vimeo\.com/video/)(\d+)",
        )
        .ok()?;
        re.captures(url)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    }
}

impl Default for VimeoExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for VimeoExtractor {
    fn name(&self) -> &str {
        "Vimeo"
    }

    fn key(&self) -> &str {
        "Vimeo"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"https?://(?:www\.)?vimeo\.com/(\d+)",
            r"https?://player\.vimeo\.com/video/(\d+)",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let video_id = Self::extract_video_id(url)
                .context("could not extract Vimeo video ID from URL")?;
            info!(video_id = %video_id, "extracting Vimeo video");

            // Fetch player config
            let config_url =
                format!("https://player.vimeo.com/video/{video_id}/config");
            let config: serde_json::Value = client
                .get_json(&config_url)
                .await
                .context("failed to fetch Vimeo config")?;

            debug!("fetched Vimeo config for video {video_id}");

            // Extract metadata from config
            let video = &config["video"];
            let title = video["title"].as_str().unwrap_or("").to_string();
            let description = video["description"].as_str().map(|s| s.to_string());
            let duration = video["duration"].as_f64();
            let width = video["width"].as_u64().map(|w| w as u32);
            let height = video["height"].as_u64().map(|h| h as u32);

            let owner = &video["owner"];
            let uploader = owner["name"].as_str().unwrap_or("").to_string();
            let uploader_url = owner["url"].as_str().map(|s| s.to_string());
            let uploader_id = owner["id"].as_u64().map(|id| id.to_string());

            // Extract formats
            let mut formats = Vec::new();
            let files = &config["request"]["files"];

            // Progressive (direct MP4) formats
            if let Some(progressive) = files["progressive"].as_array() {
                for p in progressive {
                    let fmt_url = p["url"].as_str().unwrap_or("");
                    if fmt_url.is_empty() {
                        continue;
                    }
                    let h = p["height"].as_u64().map(|v| v as u32);
                    let w = p["width"].as_u64().map(|v| v as u32);
                    let fps = p["fps"].as_f64();
                    let quality = p["quality"].as_str().unwrap_or("");

                    formats.push(Format {
                        format_id: format!("http-{quality}"),
                        format_note: Some(format!("{quality} direct")),
                        ext: "mp4".to_string(),
                        url: Some(fmt_url.to_string()),
                        protocol: Protocol::Https,
                        width: w,
                        height: h,
                        fps,
                        vcodec: Some("h264".to_string()),
                        acodec: Some("aac".to_string()),
                        ..Format::default()
                    });
                }
            }

            // DASH formats
            if let Some(dash) = files["dash"].as_object() {
                if let Some(cdns) = dash.get("cdns").and_then(|c| c.as_object()) {
                    for (cdn_name, cdn) in cdns {
                        if let Some(dash_url) = cdn["url"].as_str() {
                            formats.push(Format {
                                format_id: format!("dash-{cdn_name}"),
                                format_note: Some(format!("DASH ({cdn_name})")),
                                ext: "mp4".to_string(),
                                manifest_url: Some(dash_url.to_string()),
                                protocol: Protocol::Dash,
                                width,
                                height,
                                vcodec: Some("h264".to_string()),
                                acodec: Some("aac".to_string()),
                                ..Format::default()
                            });
                        }
                    }
                }
            }

            // HLS formats
            if let Some(hls) = files["hls"].as_object() {
                if let Some(cdns) = hls.get("cdns").and_then(|c| c.as_object()) {
                    for (cdn_name, cdn) in cdns {
                        if let Some(hls_url) = cdn["url"].as_str() {
                            formats.push(Format {
                                format_id: format!("hls-{cdn_name}"),
                                format_note: Some(format!("HLS ({cdn_name})")),
                                ext: "mp4".to_string(),
                                manifest_url: Some(hls_url.to_string()),
                                protocol: Protocol::Hls,
                                width,
                                height,
                                vcodec: Some("h264".to_string()),
                                acodec: Some("aac".to_string()),
                                ..Format::default()
                            });
                        }
                    }
                }
            }

            // Thumbnails
            let mut thumbnails = Vec::new();
            if let Some(thumbs) = video["thumbs"].as_object() {
                for (size, thumb_url) in thumbs {
                    if let Some(u) = thumb_url.as_str() {
                        thumbnails.push(Thumbnail {
                            url: u.to_string(),
                            id: Some(size.clone()),
                            width: None,
                            height: None,
                            preference: None,
                            resolution: Some(size.clone()),
                        });
                    }
                }
            }

            let ext = formats
                .first()
                .map(|f| f.ext.clone())
                .unwrap_or_else(|| "mp4".to_string());

            let info = InfoDict {
                id: video_id.clone(),
                title: Some(title),
                fulltitle: None,
                ext,
                url: None,
                webpage_url: Some(format!("https://vimeo.com/{video_id}")),
                original_url: Some(url.to_string()),
                display_id: Some(video_id),
                description,
                uploader: Some(uploader),
                uploader_id,
                uploader_url,
                channel: None,
                channel_id: None,
                channel_url: None,
                duration,
                view_count: None,
                like_count: None,
                comment_count: None,
                upload_date: None,
                timestamp: None,
                age_limit: None,
                categories: Vec::new(),
                tags: Vec::new(),
                is_live: Some(false),
                was_live: None,
                live_status: None,
                release_timestamp: None,
                formats,
                requested_formats: None,
                subtitles: HashMap::new(),
                automatic_captions: HashMap::new(),
                thumbnails,
                thumbnail: None,
                chapters: Vec::new(),
                playlist: None,
                playlist_id: None,
                playlist_title: None,
                playlist_index: None,
                n_entries: None,
                extractor: "vimeo".to_string(),
                extractor_key: "Vimeo".to_string(),
                extra: HashMap::new(),
            };

            Ok(ExtractionResult::SingleVideo(Box::new(info)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_video_id() {
        assert_eq!(
            VimeoExtractor::extract_video_id("https://vimeo.com/123456789"),
            Some("123456789".to_string())
        );
        assert_eq!(
            VimeoExtractor::extract_video_id(
                "https://player.vimeo.com/video/123456789"
            ),
            Some("123456789".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = VimeoExtractor::new();
        assert!(ext.suitable("https://vimeo.com/123456"));
        assert!(ext.suitable("https://player.vimeo.com/video/123456"));
        assert!(!ext.suitable("https://youtube.com/watch?v=abc"));
    }
}
