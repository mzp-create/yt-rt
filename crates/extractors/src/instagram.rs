use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

/// Instagram video/reel extractor.
///
/// Extracts embedded JSON data from the page or uses the graphql endpoint.
pub struct InstagramExtractor;

impl InstagramExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract shortcode from Instagram URLs.
    fn extract_shortcode(url: &str) -> Option<String> {
        let re = Regex::new(
            r"instagram\.com/(?:p|reel|tv)/([a-zA-Z0-9_-]+)",
        )
        .ok()?;
        re.captures(url)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// Try to extract the embedded shared data JSON from the page HTML.
    fn extract_shared_data(html: &str) -> Option<serde_json::Value> {
        // Try window._sharedData pattern
        let re = Regex::new(r"window\._sharedData\s*=\s*(\{.+?\});</script>").ok()?;
        if let Some(caps) = re.captures(html) {
            if let Some(json_str) = caps.get(1) {
                if let Ok(val) = serde_json::from_str(json_str.as_str()) {
                    return Some(val);
                }
            }
        }
        None
    }

    /// Try to extract __additionalDataLoaded JSON from the page HTML.
    fn extract_additional_data(html: &str) -> Option<serde_json::Value> {
        let re =
            Regex::new(r"window\.__additionalDataLoaded\s*\([^,]*,\s*(\{.+?\})\s*\)").ok()?;
        if let Some(caps) = re.captures(html) {
            if let Some(json_str) = caps.get(1) {
                if let Ok(val) = serde_json::from_str(json_str.as_str()) {
                    return Some(val);
                }
            }
        }
        None
    }
}

impl Default for InstagramExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for InstagramExtractor {
    fn name(&self) -> &str {
        "Instagram"
    }

    fn key(&self) -> &str {
        "Instagram"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"https?://(?:www\.)?instagram\.com/(?:p|reel|tv)/[a-zA-Z0-9_-]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let shortcode = Self::extract_shortcode(url)
                .context("could not extract Instagram shortcode from URL")?;
            info!(shortcode = %shortcode, "extracting Instagram video");

            // Fetch the page
            let html = client
                .get_text(url)
                .await
                .context("failed to fetch Instagram page")?;

            debug!("fetched Instagram page for shortcode {shortcode}");

            // Try to extract media data from embedded JSON
            let media = Self::extract_shared_data(&html)
                .and_then(|data| {
                    let media = data["entry_data"]["PostPage"][0]["graphql"]["shortcode_media"]
                        .clone();
                    if media.is_object() {
                        Some(media)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    Self::extract_additional_data(&html).and_then(|data| {
                        let media = data["graphql"]["shortcode_media"].clone();
                        if media.is_object() {
                            Some(media)
                        } else {
                            // Try alternate path
                            let media = data["items"][0].clone();
                            if media.is_object() {
                                Some(media)
                            } else {
                                None
                            }
                        }
                    })
                })
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

            // Extract metadata
            let caption = media["edge_media_to_caption"]["edges"][0]["node"]["text"]
                .as_str()
                .or_else(|| media["caption"]["text"].as_str())
                .unwrap_or("")
                .to_string();

            let owner_username = media["owner"]["username"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let owner_full_name = media["owner"]["full_name"]
                .as_str()
                .unwrap_or("")
                .to_string();

            let like_count = media["edge_media_preview_like"]["count"].as_u64();
            let comment_count = media["edge_media_to_comment"]["count"].as_u64();
            let timestamp = media["taken_at_timestamp"].as_i64();
            let view_count = media["video_view_count"].as_u64();

            // Extract video formats
            let mut formats = Vec::new();

            if let Some(video_url) = media["video_url"].as_str() {
                let width = media["dimensions"]["width"]
                    .as_u64()
                    .map(|w| w as u32);
                let height = media["dimensions"]["height"]
                    .as_u64()
                    .map(|h| h as u32);

                formats.push(Format {
                    format_id: "main".to_string(),
                    ext: "mp4".to_string(),
                    url: Some(video_url.to_string()),
                    protocol: Protocol::Https,
                    width,
                    height,
                    vcodec: Some("h264".to_string()),
                    acodec: Some("aac".to_string()),
                    ..Format::default()
                });
            }

            // Check for multiple video versions in video_versions
            if let Some(versions) = media["video_versions"].as_array() {
                for (i, ver) in versions.iter().enumerate() {
                    if let Some(ver_url) = ver["url"].as_str() {
                        let w = ver["width"].as_u64().map(|v| v as u32);
                        let h = ver["height"].as_u64().map(|v| v as u32);
                        formats.push(Format {
                            format_id: format!("version-{i}"),
                            ext: "mp4".to_string(),
                            url: Some(ver_url.to_string()),
                            protocol: Protocol::Https,
                            width: w,
                            height: h,
                            vcodec: Some("h264".to_string()),
                            acodec: Some("aac".to_string()),
                            ..Format::default()
                        });
                    }
                }
            }

            // Thumbnails
            let mut thumbnails = Vec::new();
            if let Some(thumb_url) = media["display_url"]
                .as_str()
                .or_else(|| media["thumbnail_src"].as_str())
            {
                thumbnails.push(Thumbnail {
                    url: thumb_url.to_string(),
                    id: Some("0".to_string()),
                    width: None,
                    height: None,
                    preference: None,
                    resolution: None,
                });
            }

            let title = if caption.is_empty() {
                format!("Instagram post by {owner_username}")
            } else {
                // Use first line of caption as title
                caption.lines().next().unwrap_or("").to_string()
            };

            let ext = formats
                .first()
                .map(|f| f.ext.clone())
                .unwrap_or_else(|| "mp4".to_string());

            let info = InfoDict {
                id: shortcode.clone(),
                title: Some(title),
                fulltitle: None,
                ext,
                url: None,
                webpage_url: Some(format!("https://www.instagram.com/p/{shortcode}/")),
                original_url: Some(url.to_string()),
                display_id: Some(shortcode),
                description: Some(caption),
                uploader: Some(owner_full_name),
                uploader_id: Some(owner_username.clone()),
                uploader_url: Some(format!(
                    "https://www.instagram.com/{owner_username}/"
                )),
                channel: None,
                channel_id: None,
                channel_url: None,
                duration: None,
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
                extractor: "instagram".to_string(),
                extractor_key: "Instagram".to_string(),
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
    fn test_extract_shortcode() {
        assert_eq!(
            InstagramExtractor::extract_shortcode(
                "https://www.instagram.com/p/CaBcDeFgHiJ/"
            ),
            Some("CaBcDeFgHiJ".to_string())
        );
        assert_eq!(
            InstagramExtractor::extract_shortcode(
                "https://www.instagram.com/reel/CaBcDeFgHiJ/"
            ),
            Some("CaBcDeFgHiJ".to_string())
        );
        assert_eq!(
            InstagramExtractor::extract_shortcode(
                "https://www.instagram.com/tv/CaBcDeFgHiJ/"
            ),
            Some("CaBcDeFgHiJ".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = InstagramExtractor::new();
        assert!(ext.suitable("https://www.instagram.com/p/CaBcDeFgHiJ/"));
        assert!(ext.suitable("https://www.instagram.com/reel/CaBcDeFgHiJ/"));
        assert!(ext.suitable("https://instagram.com/tv/CaBcDeFgHiJ/"));
        assert!(!ext.suitable("https://youtube.com/watch?v=abc"));
    }
}
