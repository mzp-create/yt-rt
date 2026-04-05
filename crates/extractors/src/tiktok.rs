use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

/// TikTok video extractor.
///
/// Extracts `__UNIVERSAL_DATA_FOR_REHYDRATION__` JSON from the page HTML.
pub struct TikTokExtractor;

impl TikTokExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract the video ID from a TikTok URL.
    fn extract_video_id(url: &str) -> Option<String> {
        let re = Regex::new(r"tiktok\.com/@[^/]+/video/(\d+)").ok()?;
        re.captures(url)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// Extract the rehydration JSON blob from page HTML.
    fn extract_rehydration_data(html: &str) -> Option<serde_json::Value> {
        let re = Regex::new(
            r#"<script[^>]*id="__UNIVERSAL_DATA_FOR_REHYDRATION__"[^>]*>(.*?)</script>"#,
        )
        .ok()?;
        let caps = re.captures(html)?;
        let json_str = caps.get(1)?.as_str();
        serde_json::from_str(json_str).ok()
    }
}

impl Default for TikTokExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for TikTokExtractor {
    fn name(&self) -> &str {
        "TikTok"
    }

    fn key(&self) -> &str {
        "TikTok"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"https?://(?:www\.)?tiktok\.com/@[^/]+/video/\d+",
            r"https?://vm\.tiktok\.com/[a-zA-Z0-9]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let video_id = Self::extract_video_id(url)
                .context("could not extract TikTok video ID from URL")?;
            info!(video_id = %video_id, "extracting TikTok video");

            // Fetch the page HTML
            let html = client
                .get_text(url)
                .await
                .context("failed to fetch TikTok page")?;

            debug!("fetched TikTok page for video {video_id}");

            // Extract rehydration data
            let data = Self::extract_rehydration_data(&html)
                .context("could not find __UNIVERSAL_DATA_FOR_REHYDRATION__ data")?;

            // Navigate to the video item data
            // The structure varies; try the common path
            let default_scope = &data["__DEFAULT_SCOPE__"];
            let item_module = &default_scope["webapp.video-detail"]["itemInfo"]["itemStruct"];

            let desc = item_module["desc"].as_str().unwrap_or("").to_string();
            let author_name = item_module["author"]["uniqueId"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let author_nickname = item_module["author"]["nickname"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let music_title = item_module["music"]["title"]
                .as_str()
                .map(|s| s.to_string());

            let stats = &item_module["stats"];
            let like_count = stats["diggCount"].as_u64();
            let view_count = stats["playCount"].as_u64();
            let comment_count = stats["commentCount"].as_u64();

            let duration = item_module["video"]["duration"].as_f64();
            let width = item_module["video"]["width"]
                .as_u64()
                .map(|w| w as u32);
            let height = item_module["video"]["height"]
                .as_u64()
                .map(|h| h as u32);

            // Extract video formats
            let mut formats = Vec::new();

            // playAddr - main video URL
            if let Some(play_url) = item_module["video"]["playAddr"].as_str() {
                formats.push(Format {
                    format_id: "watermarked".to_string(),
                    format_note: Some("watermarked".to_string()),
                    ext: "mp4".to_string(),
                    url: Some(play_url.to_string()),
                    protocol: Protocol::Https,
                    width,
                    height,
                    vcodec: Some("h264".to_string()),
                    acodec: Some("aac".to_string()),
                    ..Format::default()
                });
            }

            // downloadAddr - typically higher quality / no watermark
            if let Some(dl_url) = item_module["video"]["downloadAddr"].as_str() {
                formats.push(Format {
                    format_id: "download".to_string(),
                    format_note: Some("download".to_string()),
                    ext: "mp4".to_string(),
                    url: Some(dl_url.to_string()),
                    protocol: Protocol::Https,
                    width,
                    height,
                    vcodec: Some("h264".to_string()),
                    acodec: Some("aac".to_string()),
                    preference: Some(10),
                    ..Format::default()
                });
            }

            // bitrateInfo may contain additional formats
            if let Some(bitrate_info) = item_module["video"]["bitrateInfo"].as_array() {
                for (i, info) in bitrate_info.iter().enumerate() {
                    if let Some(play_url) = info["PlayAddr"]["UrlList"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                    {
                        let quality = info["GearName"].as_str().unwrap_or("unknown");
                        let bitrate = info["Bitrate"].as_u64().unwrap_or(0);
                        formats.push(Format {
                            format_id: format!("bitrate-{i}-{quality}"),
                            format_note: Some(quality.to_string()),
                            ext: "mp4".to_string(),
                            url: Some(play_url.to_string()),
                            protocol: Protocol::Https,
                            width,
                            height,
                            tbr: if bitrate > 0 {
                                Some(bitrate as f64 / 1000.0)
                            } else {
                                None
                            },
                            vcodec: Some("h264".to_string()),
                            acodec: Some("aac".to_string()),
                            ..Format::default()
                        });
                    }
                }
            }

            // Thumbnail
            let mut thumbnails = Vec::new();
            if let Some(cover) = item_module["video"]["cover"].as_str() {
                thumbnails.push(Thumbnail {
                    url: cover.to_string(),
                    id: Some("cover".to_string()),
                    width: None,
                    height: None,
                    preference: Some(0),
                    resolution: None,
                });
            }
            if let Some(origin_cover) = item_module["video"]["originCover"].as_str() {
                thumbnails.push(Thumbnail {
                    url: origin_cover.to_string(),
                    id: Some("origin_cover".to_string()),
                    width: None,
                    height: None,
                    preference: Some(1),
                    resolution: None,
                });
            }

            let timestamp = item_module["createTime"]
                .as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .or_else(|| item_module["createTime"].as_i64());

            let ext = formats
                .first()
                .map(|f| f.ext.clone())
                .unwrap_or_else(|| "mp4".to_string());

            let mut extra = HashMap::new();
            if let Some(music) = music_title {
                extra.insert(
                    "music".to_string(),
                    serde_json::Value::String(music),
                );
            }

            let info = InfoDict {
                id: video_id.clone(),
                title: Some(if desc.is_empty() {
                    format!("TikTok video {video_id}")
                } else {
                    desc.clone()
                }),
                fulltitle: Some(desc.clone()),
                ext,
                url: None,
                webpage_url: Some(url.to_string()),
                original_url: Some(url.to_string()),
                display_id: Some(video_id),
                description: Some(desc),
                uploader: Some(author_nickname),
                uploader_id: Some(author_name.clone()),
                uploader_url: Some(format!("https://www.tiktok.com/@{author_name}")),
                channel: None,
                channel_id: None,
                channel_url: None,
                duration,
                view_count,
                like_count,
                comment_count,
                upload_date: None,
                timestamp,
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
                extractor: "tiktok".to_string(),
                extractor_key: "TikTok".to_string(),
                extra,
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
            TikTokExtractor::extract_video_id(
                "https://www.tiktok.com/@user/video/7123456789012345678"
            ),
            Some("7123456789012345678".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = TikTokExtractor::new();
        assert!(ext.suitable("https://www.tiktok.com/@user/video/71234567890"));
        assert!(ext.suitable("https://vm.tiktok.com/ZMxyz123/"));
        assert!(!ext.suitable("https://youtube.com/watch?v=abc"));
    }
}
