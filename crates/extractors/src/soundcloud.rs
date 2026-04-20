use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info, warn};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

const API_V2_BASE: &str = "https://api-v2.soundcloud.com";

pub struct SoundCloudExtractor;

impl SoundCloudExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract client_id from the SoundCloud page JavaScript.
    /// Fetches the main page, finds script URLs, then searches for client_id in them.
    async fn extract_client_id(client: &HttpClient) -> anyhow::Result<String> {
        let page = client.get_text("https://soundcloud.com").await?;

        // Find script src URLs
        let script_re = Regex::new(r#"src="(https://a-v2\.sndcdn\.com/assets/[^"]+\.js)"#)?;
        let script_urls: Vec<String> = script_re
            .captures_iter(&page)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();

        debug!(count = script_urls.len(), "found SoundCloud script URLs");

        // Search for client_id in scripts (check last ones first, more likely)
        let client_id_re = Regex::new(r#"client_id[:=]["']([a-zA-Z0-9]{32})["']"#)?;
        for script_url in script_urls.iter().rev() {
            match client.get_text(script_url).await {
                Ok(js) => {
                    if let Some(caps) = client_id_re.captures(&js) {
                        if let Some(id) = caps.get(1) {
                            info!(client_id = id.as_str(), "found SoundCloud client_id");
                            return Ok(id.as_str().to_string());
                        }
                    }
                }
                Err(e) => {
                    warn!(url = script_url.as_str(), error = %e, "failed to fetch script");
                }
            }
        }

        anyhow::bail!("could not extract SoundCloud client_id from page scripts")
    }

    /// Resolve a SoundCloud URL to its API representation.
    async fn resolve_url(
        client: &HttpClient,
        url: &str,
        client_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let encoded_url = url::form_urlencoded::byte_serialize(url.as_bytes())
            .collect::<String>();
        let resolve_url = format!(
            "{API_V2_BASE}/resolve?url={encoded_url}&client_id={client_id}",
        );
        let data: serde_json::Value = client.get_json(&resolve_url).await?;
        Ok(data)
    }

    /// Fetch the actual stream URL from a transcoding URL.
    async fn fetch_stream_url(
        client: &HttpClient,
        transcoding_url: &str,
        client_id: &str,
    ) -> anyhow::Result<String> {
        let separator = if transcoding_url.contains('?') { "&" } else { "?" };
        let fetch_url = format!("{transcoding_url}{separator}client_id={client_id}");
        let data: serde_json::Value = client.get_json(&fetch_url).await?;
        let stream_url = data["url"]
            .as_str()
            .context("missing stream URL in transcoding response")?;
        Ok(stream_url.to_string())
    }

    /// Build formats from track transcodings.
    async fn build_formats(
        client: &HttpClient,
        track: &serde_json::Value,
        client_id: &str,
    ) -> Vec<Format> {
        let mut formats = Vec::new();

        let transcodings = match track["media"]["transcodings"].as_array() {
            Some(arr) => arr,
            None => return formats,
        };

        for (i, tc) in transcodings.iter().enumerate() {
            let tc_url = match tc["url"].as_str() {
                Some(u) => u,
                None => continue,
            };

            let preset = tc["preset"].as_str().unwrap_or("unknown");
            let protocol_str = tc["format"]["protocol"].as_str().unwrap_or("progressive");
            let mime = tc["format"]["mime_type"].as_str().unwrap_or("audio/mpeg");

            let (ext, acodec) = if mime.contains("opus") || preset.contains("opus") {
                ("opus".to_string(), "opus".to_string())
            } else if mime.contains("ogg") {
                ("ogg".to_string(), "vorbis".to_string())
            } else {
                ("mp3".to_string(), "mp3".to_string())
            };

            let protocol = if protocol_str == "hls" {
                Protocol::Hls
            } else {
                Protocol::Https
            };

            let abr = tc["quality"]
                .as_str()
                .and_then(|q| q.strip_suffix("kbps"))
                .and_then(|n| n.trim().parse::<f64>().ok());

            // Fetch the actual stream URL
            match Self::fetch_stream_url(client, tc_url, client_id).await {
                Ok(stream_url) => {
                    formats.push(Format {
                        format_id: format!("{preset}-{i}"),
                        format_note: Some(format!("{preset} ({protocol_str})")),
                        ext,
                        url: Some(stream_url),
                        protocol,
                        acodec: Some(acodec),
                        abr,
                        vcodec: Some("none".to_string()),
                        ..Default::default()
                    });
                }
                Err(e) => {
                    warn!(preset = preset, error = %e, "failed to fetch stream URL");
                }
            }
        }

        formats
    }
}

impl Default for SoundCloudExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for SoundCloudExtractor {
    fn name(&self) -> &str {
        "SoundCloud"
    }

    fn key(&self) -> &str {
        "SoundCloud"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"(?:https?://)?(?:www\.)?soundcloud\.com/[^/]+/[^/]+(?:/[^/]+)?",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            info!(url = url, "extracting SoundCloud track");

            // 1. Get client_id
            let client_id = Self::extract_client_id(client).await?;

            // 2. Resolve URL to API data
            let data = Self::resolve_url(client, url, &client_id).await?;

            // Check if this is a playlist/set
            let kind = data["kind"].as_str().unwrap_or("track");

            if kind == "playlist" {
                // Handle playlist
                let playlist_id = data["id"].as_i64().unwrap_or(0).to_string();
                let title = data["title"].as_str().map(|s| s.to_string());
                let description = data["description"].as_str().map(|s| s.to_string());
                let user = data["user"]["username"].as_str().map(|s| s.to_string());

                let mut entries = Vec::new();
                if let Some(tracks) = data["tracks"].as_array() {
                    for track in tracks {
                        if let Some(permalink) = track["permalink_url"].as_str() {
                            entries.push(PlaylistEntry::Url(permalink.to_string()));
                        }
                    }
                }

                let playlist = PlaylistInfo {
                    id: playlist_id,
                    title,
                    description,
                    webpage_url: Some(url.to_string()),
                    uploader: user,
                    uploader_id: None,
                    entries,
                    playlist_count: data["track_count"].as_u64(),
                    extractor: "soundcloud:playlist".to_string(),
                    extractor_key: "SoundCloudPlaylist".to_string(),
                };

                Ok(ExtractionResult::Playlist(playlist))
            } else {
                // Single track
                let track_id = data["id"].as_i64().unwrap_or(0).to_string();
                let title = data["title"].as_str().map(|s| s.to_string());
                let description = data["description"].as_str().map(|s| s.to_string());
                let uploader = data["user"]["username"].as_str().map(|s| s.to_string());
                let uploader_id = data["user"]["permalink"].as_str().map(|s| s.to_string());
                let duration_ms = data["duration"].as_f64();
                let duration = duration_ms.map(|ms| ms / 1000.0);

                let mut thumbnails = Vec::new();
                if let Some(artwork) = data["artwork_url"].as_str() {
                    // SoundCloud serves -large by default; replace with -t500x500 for HD
                    let hd_url = artwork.replace("-large", "-t500x500");
                    thumbnails.push(Thumbnail {
                        url: hd_url.clone(),
                        id: Some("artwork".to_string()),
                        width: Some(500),
                        height: Some(500),
                        preference: Some(1),
                        resolution: Some("500x500".to_string()),
                    });
                    thumbnails.push(Thumbnail {
                        url: artwork.to_string(),
                        id: Some("artwork_small".to_string()),
                        width: Some(100),
                        height: Some(100),
                        preference: Some(0),
                        resolution: Some("100x100".to_string()),
                    });
                }
                let thumbnail = thumbnails.first().map(|t| t.url.clone());

                let formats = Self::build_formats(client, &data, &client_id).await;

                // Build extra metadata (genre, album, track, etc.)
                let mut extra = HashMap::new();
                if let Some(genre) = data["genre"].as_str() {
                    if !genre.is_empty() {
                        extra.insert("genre".to_string(), serde_json::Value::String(genre.to_string()));
                    }
                }
                if let Some(label) = data["label_name"].as_str() {
                    if !label.is_empty() {
                        extra.insert("label".to_string(), serde_json::Value::String(label.to_string()));
                    }
                }
                if let Some(release_date) = data["release_date"].as_str() {
                    if !release_date.is_empty() {
                        extra.insert("release_date".to_string(), serde_json::Value::String(release_date.to_string()));
                    }
                }

                let info = InfoDict {
                    id: track_id,
                    title: title.clone(),
                    fulltitle: title,
                    ext: "mp3".to_string(),
                    url: None,
                    webpage_url: data["permalink_url"].as_str().map(|s| s.to_string()),
                    original_url: Some(url.to_string()),
                    display_id: data["permalink"].as_str().map(|s| s.to_string()),
                    description,
                    uploader: uploader.clone(),
                    uploader_id,
                    uploader_url: data["user"]["permalink_url"].as_str().map(|s| s.to_string()),
                    channel: uploader,
                    channel_id: None,
                    channel_url: None,
                    duration,
                    view_count: data["playback_count"].as_u64(),
                    like_count: data["likes_count"].as_u64(),
                    comment_count: data["comment_count"].as_u64(),
                    upload_date: data["created_at"].as_str().and_then(|s| {
                        // "2024-01-15T12:00:00Z" -> "20240115"
                        s.get(..10).map(|d| d.replace('-', ""))
                    }),
                    timestamp: None,
                    age_limit: None,
                    categories: Vec::new(),
                    tags: data["tag_list"]
                        .as_str()
                        .map(|t| t.split_whitespace().map(|s| s.to_string()).collect())
                        .unwrap_or_default(),
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
                    extractor: "soundcloud".to_string(),
                    extractor_key: "SoundCloud".to_string(),
                    extra,
                };

                Ok(ExtractionResult::SingleVideo(Box::new(info)))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suitable() {
        let ext = SoundCloudExtractor::new();
        assert!(ext.suitable("https://soundcloud.com/user/track-name"));
        assert!(ext.suitable("https://soundcloud.com/user/sets/playlist"));
        assert!(!ext.suitable("https://www.youtube.com/watch?v=abc"));
    }
}
