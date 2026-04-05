use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info, warn};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

pub struct FacebookExtractor;

impl FacebookExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract video ID from various Facebook URL patterns.
    fn extract_video_id(url: &str) -> Option<String> {
        let patterns = [
            // facebook.com/*/videos/ID
            r"facebook\.com/.+/videos/(\d+)",
            // facebook.com/watch/?v=ID
            r"facebook\.com/watch/?\?v=(\d+)",
            // facebook.com/video.php?v=ID
            r"facebook\.com/video\.php\?v=(\d+)",
            // facebook.com/reel/ID
            r"facebook\.com/reel/(\d+)",
            // fb.watch/SLUG -- slug is the ID in this case
            r"fb\.watch/([a-zA-Z0-9_-]+)",
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

    /// Extract video URLs from Facebook page source.
    /// Looks for playable_url and playable_url_quality_hd in page HTML/JS.
    fn extract_from_page_source(page: &str) -> Vec<Format> {
        let mut formats = Vec::new();

        // Pattern 1: "playable_url":"..." and "playable_url_quality_hd":"..."
        let sd_re = Regex::new(r#""playable_url"\s*:\s*"([^"]+)""#).ok();
        let hd_re = Regex::new(r#""playable_url_quality_hd"\s*:\s*"([^"]+)""#).ok();

        if let Some(re) = &sd_re {
            if let Some(caps) = re.captures(page) {
                if let Some(url_match) = caps.get(1) {
                    let raw_url = url_match.as_str();
                    let url = unescape_facebook_url(raw_url);
                    if !url.is_empty() {
                        formats.push(Format {
                            format_id: "sd".to_string(),
                            format_note: Some("SD".to_string()),
                            ext: "mp4".to_string(),
                            url: Some(url),
                            protocol: Protocol::Https,
                            quality: Some(1.0),
                            vcodec: Some("avc1".to_string()),
                            acodec: Some("aac".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        if let Some(re) = &hd_re {
            if let Some(caps) = re.captures(page) {
                if let Some(url_match) = caps.get(1) {
                    let raw_url = url_match.as_str();
                    let url = unescape_facebook_url(raw_url);
                    if !url.is_empty() {
                        formats.push(Format {
                            format_id: "hd".to_string(),
                            format_note: Some("HD".to_string()),
                            ext: "mp4".to_string(),
                            url: Some(url),
                            protocol: Protocol::Https,
                            quality: Some(2.0),
                            height: Some(720),
                            vcodec: Some("avc1".to_string()),
                            acodec: Some("aac".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        // Pattern 2: "browser_native_sd_url":"..." and "browser_native_hd_url":"..."
        let native_sd_re = Regex::new(r#""browser_native_sd_url"\s*:\s*"([^"]+)""#).ok();
        let native_hd_re = Regex::new(r#""browser_native_hd_url"\s*:\s*"([^"]+)""#).ok();

        if let Some(re) = &native_sd_re {
            if let Some(caps) = re.captures(page) {
                if let Some(url_match) = caps.get(1) {
                    let url = unescape_facebook_url(url_match.as_str());
                    // Only add if we don't already have an SD format
                    if !url.is_empty() && !formats.iter().any(|f| f.format_id == "sd") {
                        formats.push(Format {
                            format_id: "sd".to_string(),
                            format_note: Some("SD".to_string()),
                            ext: "mp4".to_string(),
                            url: Some(url),
                            protocol: Protocol::Https,
                            quality: Some(1.0),
                            vcodec: Some("avc1".to_string()),
                            acodec: Some("aac".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        if let Some(re) = &native_hd_re {
            if let Some(caps) = re.captures(page) {
                if let Some(url_match) = caps.get(1) {
                    let url = unescape_facebook_url(url_match.as_str());
                    if !url.is_empty() && !formats.iter().any(|f| f.format_id == "hd") {
                        formats.push(Format {
                            format_id: "hd".to_string(),
                            format_note: Some("HD".to_string()),
                            ext: "mp4".to_string(),
                            url: Some(url),
                            protocol: Protocol::Https,
                            quality: Some(2.0),
                            height: Some(720),
                            vcodec: Some("avc1".to_string()),
                            acodec: Some("aac".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        // Pattern 3: DASH manifest URL
        let dash_re = Regex::new(r#""dash_manifest_url"\s*:\s*"([^"]+)""#).ok();
        if let Some(re) = &dash_re {
            if let Some(caps) = re.captures(page) {
                if let Some(url_match) = caps.get(1) {
                    let url = unescape_facebook_url(url_match.as_str());
                    if !url.is_empty() {
                        formats.push(Format {
                            format_id: "dash".to_string(),
                            format_note: Some("DASH manifest".to_string()),
                            ext: "mp4".to_string(),
                            manifest_url: Some(url),
                            protocol: Protocol::Dash,
                            ..Default::default()
                        });
                    }
                }
            }
        }

        formats
    }

    /// Extract metadata from page source (title, description, etc.).
    fn extract_metadata(page: &str) -> (Option<String>, Option<String>, Option<String>) {
        let title = Regex::new(r#"<title[^>]*>([^<]+)</title>"#)
            .ok()
            .and_then(|re| re.captures(page))
            .and_then(|c| c.get(1).map(|m| html_unescape(m.as_str())));

        // Try og:description
        let description = Regex::new(r#"property="og:description"\s+content="([^"]*)""#)
            .ok()
            .and_then(|re| re.captures(page))
            .and_then(|c| c.get(1).map(|m| html_unescape(m.as_str())));

        // Try og:image for thumbnail
        let thumbnail = Regex::new(r#"property="og:image"\s+content="([^"]*)""#)
            .ok()
            .and_then(|re| re.captures(page))
            .and_then(|c| c.get(1).map(|m| html_unescape(m.as_str())));

        (title, description, thumbnail)
    }
}

/// Unescape Facebook's escaped URLs (unicode escapes and backslash-escaped slashes).
fn unescape_facebook_url(raw: &str) -> String {
    let unescaped = raw
        .replace("\\/", "/")
        .replace("\\u0025", "%")
        .replace("\\u003C", "<")
        .replace("\\u003E", ">")
        .replace("\\u0026", "&");
    // Handle \\u00XX unicode escapes generically
    let re = Regex::new(r"\\u([0-9a-fA-F]{4})").ok();
    match re {
        Some(re) => re
            .replace_all(&unescaped, |caps: &regex::Captures| {
                let code = u32::from_str_radix(&caps[1], 16).unwrap_or(0);
                char::from_u32(code).unwrap_or('?').to_string()
            })
            .to_string(),
        None => unescaped,
    }
}

/// Basic HTML entity unescaping.
fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
}

impl Default for FacebookExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for FacebookExtractor {
    fn name(&self) -> &str {
        "Facebook"
    }

    fn key(&self) -> &str {
        "Facebook"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"(?:https?://)?(?:www\.)?facebook\.com/.+/videos/\d+",
            r"(?:https?://)?(?:www\.)?facebook\.com/watch/?\?v=\d+",
            r"(?:https?://)?(?:www\.)?facebook\.com/video\.php\?v=\d+",
            r"(?:https?://)?(?:www\.)?facebook\.com/reel/\d+",
            r"(?:https?://)?fb\.watch/[a-zA-Z0-9_-]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let video_id = Self::extract_video_id(url)
                .context("could not extract Facebook video ID from URL")?;
            info!(video_id = %video_id, "extracting Facebook video");

            // For fb.watch short URLs, we need to follow the redirect
            let actual_url = if url.contains("fb.watch") {
                let resp = client.get(url).await?;
                resp.url().to_string()
            } else {
                url.to_string()
            };

            // Fetch the page source
            let page = client.get_text(&actual_url).await?;
            debug!(page_len = page.len(), "fetched Facebook page");

            // Extract video formats
            let formats = Self::extract_from_page_source(&page);

            if formats.is_empty() {
                warn!("no video formats found in Facebook page source; may require authentication");
            }

            // Extract metadata
            let (title, description, thumbnail_url) = Self::extract_metadata(&page);

            let mut thumbnails = Vec::new();
            if let Some(ref thumb) = thumbnail_url {
                thumbnails.push(Thumbnail {
                    url: thumb.clone(),
                    id: Some("og_image".to_string()),
                    width: None,
                    height: None,
                    preference: Some(1),
                    resolution: None,
                });
            }

            let info = InfoDict {
                id: video_id,
                title: title.clone(),
                fulltitle: title,
                ext: "mp4".to_string(),
                url: None,
                webpage_url: Some(actual_url),
                original_url: Some(url.to_string()),
                display_id: None,
                description,
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
                formats,
                requested_formats: None,
                subtitles: HashMap::new(),
                automatic_captions: HashMap::new(),
                thumbnails,
                thumbnail: thumbnail_url,
                chapters: Vec::new(),
                playlist: None,
                playlist_id: None,
                playlist_title: None,
                playlist_index: None,
                n_entries: None,
                extractor: "facebook".to_string(),
                extractor_key: "Facebook".to_string(),
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
            FacebookExtractor::extract_video_id("https://www.facebook.com/user/videos/123456789"),
            Some("123456789".to_string())
        );
        assert_eq!(
            FacebookExtractor::extract_video_id("https://www.facebook.com/watch/?v=123456789"),
            Some("123456789".to_string())
        );
        assert_eq!(
            FacebookExtractor::extract_video_id("https://fb.watch/abcXYZ123/"),
            Some("abcXYZ123".to_string())
        );
        assert_eq!(
            FacebookExtractor::extract_video_id("https://www.facebook.com/reel/123456789"),
            Some("123456789".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = FacebookExtractor::new();
        assert!(ext.suitable("https://www.facebook.com/user/videos/123456789"));
        assert!(ext.suitable("https://www.facebook.com/watch/?v=123456789"));
        assert!(ext.suitable("https://fb.watch/abcXYZ123"));
        assert!(!ext.suitable("https://www.youtube.com/watch?v=abc"));
    }

    #[test]
    fn test_unescape_url() {
        let escaped = "https:\\/\\/video.xx.fbcdn.net\\/v\\/test";
        assert_eq!(
            unescape_facebook_url(escaped),
            "https://video.xx.fbcdn.net/v/test"
        );
    }

    #[test]
    fn test_extract_formats_from_page() {
        let page = r#"something "playable_url":"https:\/\/video.xx.fbcdn.net\/sd.mp4" more "playable_url_quality_hd":"https:\/\/video.xx.fbcdn.net\/hd.mp4" end"#;
        let formats = FacebookExtractor::extract_from_page_source(page);
        assert_eq!(formats.len(), 2);
        assert_eq!(formats[0].format_id, "sd");
        assert_eq!(formats[1].format_id, "hd");
    }
}
