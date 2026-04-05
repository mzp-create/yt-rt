use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info, warn};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use super::format_parser::parse_youtube_format;
use super::innertube::InnertubeApi;
use super::player;
use super::signature::SignatureDecryptor;
use super::types::*;
use crate::InfoExtractor;

/// Main YouTube video extractor.
///
/// Handles URLs of the form:
/// - `https://www.youtube.com/watch?v=VIDEO_ID`
/// - `https://youtu.be/VIDEO_ID`
/// - `https://www.youtube.com/embed/VIDEO_ID`
/// - `https://www.youtube.com/v/VIDEO_ID`
/// - `https://www.youtube.com/shorts/VIDEO_ID`
/// - `https://www.youtube.com/live/VIDEO_ID`
pub struct YoutubeExtractor;

impl YoutubeExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract video ID from various YouTube URL formats.
    pub fn extract_video_id(url: &str) -> Option<String> {
        // Handle: youtube.com/watch?v=ID, youtu.be/ID, youtube.com/embed/ID,
        // youtube.com/v/ID, youtube.com/shorts/ID, youtube.com/live/ID
        // Also handle m.youtube.com variants
        let patterns = [
            r"(?:youtube\.com/watch\?.*v=|youtu\.be/|youtube\.com/embed/|youtube\.com/v/|youtube\.com/shorts/|youtube\.com/live/)([a-zA-Z0-9_-]{11})",
        ];
        for pat in &patterns {
            if let Ok(re) = Regex::new(pat) {
                if let Some(caps) = re.captures(url) {
                    return Some(caps.get(1)?.as_str().to_string());
                }
            }
        }
        // Maybe it's just a bare video ID
        let re = Regex::new(r"^[a-zA-Z0-9_-]{11}$").ok()?;
        if re.is_match(url) {
            return Some(url.to_string());
        }
        None
    }

    /// Try multiple innertube clients in order, returning the first successful
    /// PlayerResponse that contains streaming data.
    async fn fetch_player_response(
        api: &InnertubeApi<'_>,
        video_id: &str,
        signature_timestamp: u64,
    ) -> anyhow::Result<PlayerResponse> {
        let clients: &[(&str, &InnertubeClient)] = &[
            ("WEB", &WEB_CLIENT),
            ("ANDROID", &ANDROID_CLIENT),
            ("IOS", &IOS_CLIENT),
            ("TV_EMBED", &TV_EMBED_CLIENT),
        ];

        let mut last_error: Option<anyhow::Error> = None;

        for (name, innertube_client) in clients {
            debug!(client = name, "trying innertube client");

            let result = api
                .player_with_sts(video_id, innertube_client, signature_timestamp)
                .await;

            match result {
                Ok(resp) => {
                    // Check if we got usable streaming data
                    if let Some(ref sd) = resp.streaming_data {
                        let has_formats = sd
                            .formats
                            .as_ref()
                            .map_or(false, |f| !f.is_empty());
                        let has_adaptive = sd
                            .adaptive_formats
                            .as_ref()
                            .map_or(false, |f| !f.is_empty());

                        if has_formats || has_adaptive {
                            info!(client = name, "got streaming data");
                            return Ok(resp);
                        }
                    }

                    // Check playability status for hard errors
                    if let Some(ref status) = resp.playability_status {
                        match status.status.as_str() {
                            "LOGIN_REQUIRED" => {
                                last_error = Some(anyhow::anyhow!(
                                    "video requires login: {}",
                                    status.reason.as_deref().unwrap_or("unknown reason")
                                ));
                            }
                            "UNPLAYABLE" => {
                                last_error = Some(anyhow::anyhow!(
                                    "video is unplayable: {}",
                                    status.reason.as_deref().unwrap_or("unknown reason")
                                ));
                            }
                            "ERROR" => {
                                last_error = Some(anyhow::anyhow!(
                                    "video error: {}",
                                    status.reason.as_deref().unwrap_or("unknown error")
                                ));
                            }
                            _ => {
                                warn!(
                                    client = name,
                                    status = status.status.as_str(),
                                    "no streaming data from client"
                                );
                                last_error =
                                    Some(anyhow::anyhow!("no streaming data from {name} client"));
                            }
                        }
                    } else {
                        last_error =
                            Some(anyhow::anyhow!("no streaming data from {name} client"));
                    }
                }
                Err(e) => {
                    warn!(client = name, error = %e, "innertube request failed");
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("all innertube clients failed")))
    }

    /// Process all formats from streaming data: decipher signatures and transform
    /// n-parameters.
    fn process_formats(
        yt_formats: &[YtFormat],
        decryptor: &mut SignatureDecryptor,
    ) -> Vec<Format> {
        let mut formats = Vec::new();

        for yt_fmt in yt_formats {
            let mut fmt = parse_youtube_format(yt_fmt);

            // Determine the cipher string (could be in signature_cipher or cipher field)
            let cipher = yt_fmt
                .signature_cipher
                .as_deref()
                .or(yt_fmt.cipher.as_deref());

            // The base URL is either from the format directly or from the cipher
            let base_url = yt_fmt.url.as_deref().unwrap_or("");

            match decryptor.process_url(base_url, cipher) {
                Ok(final_url) => {
                    fmt.url = Some(final_url);
                }
                Err(e) => {
                    warn!(itag = yt_fmt.itag, error = %e, "failed to process format URL");
                    // Keep the original URL if available; skip entirely if not
                    if fmt.url.is_none() {
                        continue;
                    }
                }
            }

            formats.push(fmt);
        }

        formats
    }

    /// Build thumbnails from video details.
    fn build_thumbnails(details: &VideoDetails) -> Vec<Thumbnail> {
        let mut thumbnails = Vec::new();

        if let Some(ref thumb_list) = details.thumbnail {
            for (i, item) in thumb_list.thumbnails.iter().enumerate() {
                thumbnails.push(Thumbnail {
                    url: item.url.clone(),
                    id: Some(i.to_string()),
                    width: item.width,
                    height: item.height,
                    preference: Some(i as i32),
                    resolution: match (item.width, item.height) {
                        (Some(w), Some(h)) => Some(format!("{w}x{h}")),
                        _ => None,
                    },
                });
            }
        }

        thumbnails
    }

    /// Build the InfoDict from a PlayerResponse and processed formats.
    fn build_info_dict(
        video_id: &str,
        url: &str,
        player_resp: &PlayerResponse,
        formats: Vec<Format>,
    ) -> InfoDict {
        let details = player_resp.video_details.as_ref();

        let title = details.map(|d| d.title.clone());
        let description = details.and_then(|d| d.short_description.clone());
        let channel_id = details.and_then(|d| d.channel_id.clone());
        let author = details.and_then(|d| d.author.clone());
        let view_count: Option<u64> = details
            .and_then(|d| d.view_count.as_ref())
            .and_then(|s| s.parse().ok());
        let duration: Option<f64> = details
            .and_then(|d| d.length_seconds.as_ref())
            .and_then(|s| s.parse().ok());
        let is_live = details.and_then(|d| d.is_live);
        let tags = details
            .and_then(|d| d.keywords.clone())
            .unwrap_or_default();

        let thumbnails = details
            .map(|d| Self::build_thumbnails(d))
            .unwrap_or_default();

        let thumbnail = thumbnails.last().map(|t| t.url.clone());

        // Try to extract microformat data
        let microformat: Option<MicroformatRenderer> = player_resp
            .microformat
            .as_ref()
            .and_then(|m| m.get("playerMicroformatRenderer"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let upload_date = microformat.as_ref().and_then(|m| {
            m.upload_date
                .clone()
                .or_else(|| m.publish_date.clone())
        });
        let categories = microformat
            .as_ref()
            .and_then(|m| m.category.clone())
            .map(|c| vec![c])
            .unwrap_or_default();

        let channel_url = channel_id
            .as_ref()
            .map(|cid| format!("https://www.youtube.com/channel/{cid}"));

        // Determine the best extension from available formats
        let ext = formats
            .first()
            .map(|f| f.ext.clone())
            .unwrap_or_else(|| "mp4".to_string());

        InfoDict {
            id: video_id.to_string(),
            title: title.clone(),
            fulltitle: title,
            ext,
            url: None,
            webpage_url: Some(format!("https://www.youtube.com/watch?v={video_id}")),
            original_url: Some(url.to_string()),
            display_id: Some(video_id.to_string()),
            description,
            uploader: author.clone(),
            uploader_id: channel_id.clone(),
            uploader_url: channel_url.clone(),
            channel: author,
            channel_id,
            channel_url,
            duration,
            view_count,
            like_count: None,
            comment_count: None,
            upload_date,
            timestamp: None,
            age_limit: None,
            categories,
            tags,
            is_live,
            was_live: None,
            live_status: is_live.map(|live| {
                if live {
                    "is_live".to_string()
                } else {
                    "not_live".to_string()
                }
            }),
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
            extractor: "youtube".to_string(),
            extractor_key: "Youtube".to_string(),
            extra: HashMap::new(),
        }
    }
}

impl Default for YoutubeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for YoutubeExtractor {
    fn name(&self) -> &str {
        "YouTube"
    }

    fn key(&self) -> &str {
        "Youtube"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"(?:https?://)?(?:www\.|m\.)?youtube\.com/watch\?.*v=[a-zA-Z0-9_-]{11}",
            r"(?:https?://)?youtu\.be/[a-zA-Z0-9_-]{11}",
            r"(?:https?://)?(?:www\.|m\.)?youtube\.com/embed/[a-zA-Z0-9_-]{11}",
            r"(?:https?://)?(?:www\.|m\.)?youtube\.com/v/[a-zA-Z0-9_-]{11}",
            r"(?:https?://)?(?:www\.|m\.)?youtube\.com/shorts/[a-zA-Z0-9_-]{11}",
            r"(?:https?://)?(?:www\.|m\.)?youtube\.com/live/[a-zA-Z0-9_-]{11}",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            // 1. Extract video ID
            let video_id = Self::extract_video_id(url)
                .context("could not extract video ID from URL")?;
            info!(video_id = %video_id, "extracting YouTube video");

            // 2. Fetch the watch page and extract player.js URL
            let player_url = player::extract_player_url(client, &video_id)
                .await
                .context("failed to extract player URL")?;
            debug!(player_url = %player_url, "found player.js URL");

            // 3. Fetch player.js and extract signature timestamp
            let player_info = player::fetch_player(client, &player_url)
                .await
                .context("failed to fetch player.js")?;
            debug!(
                signature_timestamp = player_info.signature_timestamp,
                "extracted player info"
            );

            // 4. Set up signature decryptor from player.js
            let mut decryptor = SignatureDecryptor::new();
            if let Err(e) = decryptor.extract_functions(&player_info.player_js) {
                warn!(error = %e, "failed to extract signature functions from player.js");
            }

            // 5. Fetch player response via innertube API (tries multiple clients)
            let api = InnertubeApi::new(client);
            let player_resp = Self::fetch_player_response(
                &api,
                &video_id,
                player_info.signature_timestamp,
            )
            .await
            .context("failed to get player response from any innertube client")?;

            // 6. Extract and process formats from streaming data
            let mut all_formats = Vec::new();

            if let Some(ref streaming_data) = player_resp.streaming_data {
                // Combined (muxed) formats
                if let Some(ref fmts) = streaming_data.formats {
                    debug!(count = fmts.len(), "processing muxed formats");
                    let processed = Self::process_formats(fmts, &mut decryptor);
                    all_formats.extend(processed);
                }

                // Adaptive (separate audio/video) formats
                if let Some(ref adaptive) = streaming_data.adaptive_formats {
                    debug!(count = adaptive.len(), "processing adaptive formats");
                    let processed = Self::process_formats(adaptive, &mut decryptor);
                    all_formats.extend(processed);
                }
            }

            info!(
                video_id = %video_id,
                format_count = all_formats.len(),
                "extraction complete"
            );

            // 7. Build InfoDict and return
            let info = Self::build_info_dict(&video_id, url, &player_resp, all_formats);

            Ok(ExtractionResult::SingleVideo(Box::new(info)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_video_id_watch() {
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        assert_eq!(
            YoutubeExtractor::extract_video_id(url),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_short_url() {
        let url = "https://youtu.be/dQw4w9WgXcQ";
        assert_eq!(
            YoutubeExtractor::extract_video_id(url),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_embed() {
        let url = "https://www.youtube.com/embed/dQw4w9WgXcQ";
        assert_eq!(
            YoutubeExtractor::extract_video_id(url),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_shorts() {
        let url = "https://www.youtube.com/shorts/dQw4w9WgXcQ";
        assert_eq!(
            YoutubeExtractor::extract_video_id(url),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_live() {
        let url = "https://www.youtube.com/live/dQw4w9WgXcQ";
        assert_eq!(
            YoutubeExtractor::extract_video_id(url),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_bare() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_invalid() {
        assert_eq!(YoutubeExtractor::extract_video_id("not-a-url"), None);
        assert_eq!(YoutubeExtractor::extract_video_id(""), None);
    }

    #[test]
    fn test_extract_video_id_with_extra_params() {
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf";
        assert_eq!(
            YoutubeExtractor::extract_video_id(url),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_mobile() {
        let url = "https://m.youtube.com/watch?v=dQw4w9WgXcQ";
        assert_eq!(
            YoutubeExtractor::extract_video_id(url),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_suitable_urls() {
        let extractor = YoutubeExtractor::new();
        assert!(extractor.suitable("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(extractor.suitable("https://youtu.be/dQw4w9WgXcQ"));
        assert!(extractor.suitable("https://www.youtube.com/embed/dQw4w9WgXcQ"));
        assert!(extractor.suitable("https://www.youtube.com/shorts/dQw4w9WgXcQ"));
        assert!(!extractor.suitable("https://www.example.com/video"));
    }
}
