use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

/// Reddit video extractor.
///
/// Fetches JSON from Reddit's API by appending `.json` to the post URL.
pub struct RedditExtractor;

impl RedditExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract the canonical Reddit post path from various URL formats.
    fn extract_post_url(url: &str) -> Option<String> {
        let re = Regex::new(
            r"(?:https?://)?(?:(?:www|old|new)\.)?reddit\.com(/r/[^/]+/comments/[^/?#]+)",
        )
        .ok()?;
        re.captures(url)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// Extract post ID from the URL path.
    fn extract_post_id(url: &str) -> Option<String> {
        let re = Regex::new(r"/comments/([a-z0-9]+)").ok()?;
        re.captures(url)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    }
}

impl Default for RedditExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for RedditExtractor {
    fn name(&self) -> &str {
        "Reddit"
    }

    fn key(&self) -> &str {
        "Reddit"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"https?://(?:(?:www|old|new)\.)?reddit\.com/r/[^/]+/comments/[a-z0-9]+",
            r"https?://redd\.it/[a-z0-9]+",
            r"https?://v\.redd\.it/[a-z0-9]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let post_path = Self::extract_post_url(url)
                .context("could not extract Reddit post path from URL")?;
            let post_id = Self::extract_post_id(url)
                .context("could not extract Reddit post ID from URL")?;
            info!(post_id = %post_id, "extracting Reddit video");

            // Fetch JSON data by appending .json
            let json_url = format!("https://www.reddit.com{post_path}.json");
            let data: serde_json::Value = client
                .get_json(&json_url)
                .await
                .context("failed to fetch Reddit post JSON")?;

            debug!("fetched Reddit JSON for post {post_id}");

            // Navigate to the post data
            let post_data = &data[0]["data"]["children"][0]["data"];

            let title = post_data["title"].as_str().unwrap_or("").to_string();
            let author = post_data["author"].as_str().unwrap_or("").to_string();
            let subreddit = post_data["subreddit"].as_str().unwrap_or("").to_string();
            let score = post_data["score"].as_u64();
            let num_comments = post_data["num_comments"].as_u64();

            let mut formats = Vec::new();

            // Try to get reddit_video data
            let reddit_video = &post_data["media"]["reddit_video"];
            if reddit_video.is_object() {
                let duration = reddit_video["duration"].as_f64();
                let height = reddit_video["height"].as_u64().map(|h| h as u32);
                let width = reddit_video["width"].as_u64().map(|w| w as u32);

                // DASH manifest
                if let Some(dash_url) = reddit_video["dash_url"].as_str() {
                    formats.push(Format {
                        format_id: "dash".to_string(),
                        ext: "mp4".to_string(),
                        manifest_url: Some(dash_url.to_string()),
                        protocol: Protocol::Dash,
                        width,
                        height,
                        vcodec: Some("h264".to_string()),
                        acodec: Some("aac".to_string()),
                        format_note: Some("DASH".to_string()),
                        ..Format::default()
                    });
                }

                // HLS manifest
                if let Some(hls_url) = reddit_video["hls_url"].as_str() {
                    formats.push(Format {
                        format_id: "hls".to_string(),
                        ext: "mp4".to_string(),
                        manifest_url: Some(hls_url.to_string()),
                        protocol: Protocol::Hls,
                        width,
                        height,
                        vcodec: Some("h264".to_string()),
                        acodec: Some("aac".to_string()),
                        format_note: Some("HLS".to_string()),
                        ..Format::default()
                    });
                }

                // Direct fallback URL
                if let Some(fallback_url) = reddit_video["fallback_url"].as_str() {
                    formats.push(Format {
                        format_id: "fallback".to_string(),
                        ext: "mp4".to_string(),
                        url: Some(fallback_url.to_string()),
                        protocol: Protocol::Https,
                        width,
                        height,
                        vcodec: Some("h264".to_string()),
                        acodec: Some("none".to_string()),
                        format_note: Some("video only".to_string()),
                        ..Format::default()
                    });

                    // Audio is typically at a separate URL
                    let audio_url = fallback_url
                        .replace("DASH_720", "DASH_audio")
                        .replace("DASH_480", "DASH_audio")
                        .replace("DASH_360", "DASH_audio")
                        .replace("DASH_240", "DASH_audio")
                        .replace("DASH_1080", "DASH_audio");
                    if audio_url != fallback_url {
                        formats.push(Format {
                            format_id: "audio".to_string(),
                            ext: "m4a".to_string(),
                            url: Some(audio_url),
                            protocol: Protocol::Https,
                            vcodec: Some("none".to_string()),
                            acodec: Some("aac".to_string()),
                            format_note: Some("audio only".to_string()),
                            ..Format::default()
                        });
                    }
                }

                let _ = duration; // used below in info dict
            }

            // Thumbnail
            let mut thumbnails = Vec::new();
            if let Some(thumb_url) = post_data["thumbnail"].as_str() {
                if thumb_url.starts_with("http") {
                    thumbnails.push(Thumbnail {
                        url: thumb_url.to_string(),
                        id: Some("0".to_string()),
                        width: None,
                        height: None,
                        preference: None,
                        resolution: None,
                    });
                }
            }

            let duration = post_data["media"]["reddit_video"]["duration"].as_f64();
            let ext = formats
                .first()
                .map(|f| f.ext.clone())
                .unwrap_or_else(|| "mp4".to_string());

            let info = InfoDict {
                id: post_id.clone(),
                title: Some(title),
                fulltitle: None,
                ext,
                url: None,
                webpage_url: Some(format!("https://www.reddit.com{post_path}")),
                original_url: Some(url.to_string()),
                display_id: Some(post_id),
                description: post_data["selftext"].as_str().map(|s| s.to_string()),
                uploader: Some(author.clone()),
                uploader_id: Some(author),
                uploader_url: None,
                channel: Some(format!("r/{subreddit}")),
                channel_id: Some(subreddit.clone()),
                channel_url: Some(format!("https://www.reddit.com/r/{subreddit}")),
                duration,
                view_count: None,
                like_count: score,
                comment_count: num_comments,
                upload_date: None,
                timestamp: post_data["created_utc"].as_f64().map(|t| t as i64),
                age_limit: if post_data["over_18"].as_bool().unwrap_or(false) {
                    Some(18)
                } else {
                    Some(0)
                },
                categories: vec![subreddit],
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
                extractor: "reddit".to_string(),
                extractor_key: "Reddit".to_string(),
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
    fn test_extract_post_id() {
        assert_eq!(
            RedditExtractor::extract_post_id(
                "https://www.reddit.com/r/videos/comments/abc123/some_title/"
            ),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = RedditExtractor::new();
        assert!(ext.suitable("https://www.reddit.com/r/videos/comments/abc123/title/"));
        assert!(ext.suitable("https://old.reddit.com/r/test/comments/xyz789/post/"));
        assert!(!ext.suitable("https://twitter.com/user/status/123"));
    }
}
