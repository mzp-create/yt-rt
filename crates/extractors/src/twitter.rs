use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

/// Twitter/X video extractor.
///
/// Uses the syndication API which does not require authentication.
pub struct TwitterExtractor;

impl TwitterExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract tweet ID from various Twitter/X URL formats.
    fn extract_tweet_id(url: &str) -> Option<String> {
        let re = Regex::new(
            r"(?:twitter\.com|x\.com)/[^/]+/status/(\d+)",
        )
        .ok()?;
        re.captures(url)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    }
}

impl Default for TwitterExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for TwitterExtractor {
    fn name(&self) -> &str {
        "Twitter"
    }

    fn key(&self) -> &str {
        "Twitter"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"https?://(?:www\.)?(?:twitter\.com|x\.com)/[^/]+/status/\d+",
            r"https?://t\.co/[a-zA-Z0-9]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let tweet_id = Self::extract_tweet_id(url)
                .context("could not extract tweet ID from URL")?;
            info!(tweet_id = %tweet_id, "extracting Twitter video");

            // Use the syndication endpoint (no auth required)
            let api_url = format!(
                "https://cdn.syndication.twimg.com/tweet-result?id={tweet_id}&token=x"
            );
            let data: serde_json::Value = client
                .get_json(&api_url)
                .await
                .context("failed to fetch tweet data from syndication API")?;

            debug!("fetched syndication data for tweet {tweet_id}");

            let text = data["text"].as_str().unwrap_or("");
            let author = data["user"]["name"].as_str().unwrap_or("Unknown");
            let screen_name = data["user"]["screen_name"].as_str().unwrap_or("");

            // Extract video formats from mediaDetails
            let mut formats = Vec::new();
            if let Some(media_details) = data["mediaDetails"].as_array() {
                for media in media_details {
                    if let Some(variants) = media["video_info"]["variants"].as_array() {
                        for (i, variant) in variants.iter().enumerate() {
                            let content_type =
                                variant["content_type"].as_str().unwrap_or("");
                            let video_url = variant["url"].as_str().unwrap_or("");
                            if video_url.is_empty() {
                                continue;
                            }

                            let bitrate = variant["bitrate"].as_u64().unwrap_or(0);
                            let ext = if content_type.contains("mp4") {
                                "mp4"
                            } else if content_type.contains("mpegURL") {
                                "m3u8"
                            } else {
                                "mp4"
                            };

                            let protocol = if ext == "m3u8" {
                                Protocol::Hls
                            } else {
                                Protocol::Https
                            };

                            // Try to parse resolution from URL (e.g., /vid/1280x720/)
                            let (width, height) = extract_resolution_from_url(video_url);

                            formats.push(Format {
                                format_id: format!("{i}-{bitrate}"),
                                ext: ext.to_string(),
                                url: Some(video_url.to_string()),
                                protocol,
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
            }

            // Thumbnails
            let mut thumbnails = Vec::new();
            if let Some(thumb_url) = data["mediaDetails"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|m| m["media_url_https"].as_str())
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

            let ext = formats
                .first()
                .map(|f| f.ext.clone())
                .unwrap_or_else(|| "mp4".to_string());

            let info = InfoDict {
                id: tweet_id.clone(),
                title: Some(truncate_text(text, 80)),
                fulltitle: Some(text.to_string()),
                ext,
                url: None,
                webpage_url: Some(url.to_string()),
                original_url: Some(url.to_string()),
                display_id: Some(tweet_id),
                description: Some(text.to_string()),
                uploader: Some(author.to_string()),
                uploader_id: Some(screen_name.to_string()),
                uploader_url: Some(format!("https://twitter.com/{screen_name}")),
                channel: None,
                channel_id: None,
                channel_url: None,
                duration: None,
                view_count: None,
                like_count: data["favorite_count"].as_u64(),
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
                extractor: "twitter".to_string(),
                extractor_key: "Twitter".to_string(),
                extra: HashMap::new(),
            };

            Ok(ExtractionResult::SingleVideo(Box::new(info)))
        })
    }
}

/// Try to extract resolution from a Twitter video URL like `/vid/1280x720/`.
fn extract_resolution_from_url(url: &str) -> (Option<u32>, Option<u32>) {
    if let Ok(re) = Regex::new(r"/(\d{2,4})x(\d{2,4})/") {
        if let Some(caps) = re.captures(url) {
            let w = caps.get(1).and_then(|m| m.as_str().parse().ok());
            let h = caps.get(2).and_then(|m| m.as_str().parse().ok());
            return (w, h);
        }
    }
    (None, None)
}

/// Truncate text to a maximum length, appending "..." if truncated.
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tweet_id() {
        assert_eq!(
            TwitterExtractor::extract_tweet_id(
                "https://twitter.com/user/status/1234567890"
            ),
            Some("1234567890".to_string())
        );
        assert_eq!(
            TwitterExtractor::extract_tweet_id(
                "https://x.com/elonmusk/status/9876543210"
            ),
            Some("9876543210".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = TwitterExtractor::new();
        assert!(ext.suitable("https://twitter.com/user/status/123456"));
        assert!(ext.suitable("https://x.com/user/status/123456"));
        assert!(!ext.suitable("https://youtube.com/watch?v=abc"));
    }
}
