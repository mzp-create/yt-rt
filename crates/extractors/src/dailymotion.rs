use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

const METADATA_URL: &str = "https://www.dailymotion.com/player/metadata/video";

pub struct DailymotionExtractor;

impl DailymotionExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract video ID from Dailymotion URL.
    /// Handles: dailymotion.com/video/XABCDEF, dai.ly/XABCDEF
    fn extract_video_id(url: &str) -> Option<String> {
        let patterns = [
            r"dailymotion\.com/video/([a-zA-Z0-9]+)",
            r"dai\.ly/([a-zA-Z0-9]+)",
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

    /// Parse HLS master playlist for Dailymotion quality variants.
    fn parse_hls_formats(manifest: &str, manifest_url: &str) -> Vec<Format> {
        let mut formats = Vec::new();
        let lines: Vec<&str> = manifest.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if line.starts_with("#EXT-X-STREAM-INF:") {
                let attrs = line.trim_start_matches("#EXT-X-STREAM-INF:");
                let bandwidth = parse_m3u8_attr(attrs, "BANDWIDTH")
                    .and_then(|v| v.parse::<f64>().ok());
                let resolution = parse_m3u8_attr(attrs, "RESOLUTION");
                let name = parse_m3u8_attr(attrs, "NAME")
                    .unwrap_or_else(|| "auto".to_string());

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
                            format_id: name.clone(),
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
}

impl Default for DailymotionExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_m3u8_attr(attrs: &str, key: &str) -> Option<String> {
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

impl InfoExtractor for DailymotionExtractor {
    fn name(&self) -> &str {
        "Dailymotion"
    }

    fn key(&self) -> &str {
        "Dailymotion"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"(?:https?://)?(?:www\.)?dailymotion\.com/video/[a-zA-Z0-9]+",
            r"(?:https?://)?dai\.ly/[a-zA-Z0-9]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let video_id = Self::extract_video_id(url)
                .context("could not extract Dailymotion video ID from URL")?;
            info!(video_id = %video_id, "extracting Dailymotion video");

            let metadata_url = format!("{METADATA_URL}/{video_id}");
            let meta: serde_json::Value = client.get_json(&metadata_url).await?;

            let title = meta["title"].as_str().map(|s| s.to_string());
            let description = meta["description"].as_str().map(|s| s.to_string());
            let owner_name = meta["owner"]["screenname"].as_str().map(|s| s.to_string());
            let owner_url = meta["owner"]["url"].as_str().map(|s| s.to_string());
            let duration = meta["duration"].as_f64();

            // Thumbnails
            let mut thumbnails = Vec::new();
            if let Some(poster_url) = meta["posters"]["720"].as_str() {
                thumbnails.push(Thumbnail {
                    url: poster_url.to_string(),
                    id: Some("poster_720".to_string()),
                    width: Some(1280),
                    height: Some(720),
                    preference: Some(1),
                    resolution: Some("1280x720".to_string()),
                });
            }
            let thumbnail = thumbnails.first().map(|t| t.url.clone());

            // Get HLS manifest from qualities
            let mut formats = Vec::new();

            if let Some(qualities) = meta["qualities"].as_object() {
                // auto[] contains HLS manifests
                if let Some(auto_list) = qualities.get("auto") {
                    if let Some(auto_arr) = auto_list.as_array() {
                        for entry in auto_arr {
                            let entry_type = entry["type"].as_str().unwrap_or("");
                            let entry_url = entry["url"].as_str().unwrap_or("");
                            if entry_type.contains("mpegURL") || entry_type.contains("m3u8") {
                                if !entry_url.is_empty() {
                                    debug!(manifest_url = entry_url, "fetching HLS manifest");
                                    match client.get_text(entry_url).await {
                                        Ok(manifest) => {
                                            let hls = Self::parse_hls_formats(&manifest, entry_url);
                                            formats.extend(hls);
                                        }
                                        Err(e) => {
                                            tracing::warn!(error = %e, "failed to fetch HLS manifest");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Also check numbered qualities like "480", "720" for direct MP4
                for (quality, sources) in qualities.iter() {
                    if quality == "auto" {
                        continue;
                    }
                    if let Some(sources_arr) = sources.as_array() {
                        for source in sources_arr {
                            let source_type = source["type"].as_str().unwrap_or("");
                            let source_url = source["url"].as_str().unwrap_or("");
                            if source_type.contains("mp4") && !source_url.is_empty() {
                                formats.push(Format {
                                    format_id: format!("http-{quality}"),
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
                }
            }

            let info = InfoDict {
                id: video_id.clone(),
                title: title.clone(),
                fulltitle: title,
                ext: "mp4".to_string(),
                url: None,
                webpage_url: Some(format!("https://www.dailymotion.com/video/{video_id}")),
                original_url: Some(url.to_string()),
                display_id: Some(video_id),
                description,
                uploader: owner_name,
                uploader_id: None,
                uploader_url: owner_url,
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
                is_live: None,
                was_live: None,
                live_status: None,
                release_timestamp: None,
                formats,
                requested_formats: None,
                subtitles: HashMap::new(),
                automatic_captions: HashMap::new(),
                thumbnails,
                thumbnail,
                chapters: Vec::new(),
                playlist: None,
                playlist_id: None,
                playlist_title: None,
                playlist_index: None,
                n_entries: None,
                extractor: "dailymotion".to_string(),
                extractor_key: "Dailymotion".to_string(),
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
            DailymotionExtractor::extract_video_id("https://www.dailymotion.com/video/x8abc12"),
            Some("x8abc12".to_string())
        );
        assert_eq!(
            DailymotionExtractor::extract_video_id("https://dai.ly/x8abc12"),
            Some("x8abc12".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = DailymotionExtractor::new();
        assert!(ext.suitable("https://www.dailymotion.com/video/x8abc12"));
        assert!(ext.suitable("https://dai.ly/x8abc12"));
        assert!(!ext.suitable("https://www.youtube.com/watch?v=abc"));
    }
}
