use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;
use tracing::{debug, info};

use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

const VIEW_API: &str = "https://api.bilibili.com/x/web-interface/view";
const PLAYURL_API: &str = "https://api.bilibili.com/x/player/playurl";

/// Known video quality IDs to human-readable labels.
fn quality_label(qn: u64) -> &'static str {
    match qn {
        127 => "8K",
        126 => "Dolby Vision",
        125 => "HDR",
        120 => "4K",
        116 => "1080P60",
        112 => "1080P+",
        80 => "1080P",
        74 => "720P60",
        64 => "720P",
        32 => "480P",
        16 => "360P",
        6 => "240P",
        _ => "unknown",
    }
}

/// Known audio quality IDs.
fn audio_quality_label(qn: u64) -> &'static str {
    match qn {
        30280 => "320kbps",
        30232 => "128kbps",
        30216 => "64kbps",
        30250 => "Dolby Atmos",
        30251 => "Hi-Res",
        _ => "unknown",
    }
}

pub struct BilibiliExtractor;

impl BilibiliExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract BV ID from URL.
    fn extract_bvid(url: &str) -> Option<String> {
        let re = Regex::new(r"(?:bilibili\.com/video/|b23\.tv/)(BV[a-zA-Z0-9]+)").ok()?;
        re.captures(url).and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
    }

    /// Fetch video info to get the CID (required for playurl API).
    async fn fetch_video_info(
        client: &HttpClient,
        bvid: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!("{VIEW_API}?bvid={bvid}");
        let resp: serde_json::Value = client.get_json(&url).await?;
        let code = resp["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = resp["message"].as_str().unwrap_or("unknown error");
            anyhow::bail!("Bilibili API error {code}: {msg}");
        }
        Ok(resp["data"].clone())
    }

    /// Fetch playurl data with DASH support (fnval=4048 enables DASH+HDR+8K+AV1+DolbyAtmos).
    async fn fetch_playurl(
        client: &HttpClient,
        bvid: &str,
        cid: u64,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!(
            "{PLAYURL_API}?bvid={bvid}&cid={cid}&fnval=4048&fnver=0&fourk=1&qn=127"
        );
        let resp: serde_json::Value = client.get_json(&url).await?;
        let code = resp["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = resp["message"].as_str().unwrap_or("unknown error");
            anyhow::bail!("Bilibili playurl API error {code}: {msg}");
        }
        Ok(resp["data"].clone())
    }

    /// Parse DASH video and audio streams into Format entries.
    fn parse_dash_formats(data: &serde_json::Value) -> Vec<Format> {
        let mut formats = Vec::new();

        if let Some(dash) = data["dash"].as_object() {
            // Video streams
            if let Some(videos) = dash.get("video").and_then(|v| v.as_array()) {
                for v in videos {
                    let id = v["id"].as_u64().unwrap_or(0);
                    let base_url = v["baseUrl"].as_str().or(v["base_url"].as_str()).unwrap_or("");
                    let codecs = v["codecs"].as_str().unwrap_or("unknown");
                    let width = v["width"].as_u64().map(|w| w as u32);
                    let height = v["height"].as_u64().map(|h| h as u32);
                    let fps = v["frameRate"].as_str()
                        .or(v["frame_rate"].as_str())
                        .and_then(|f| f.parse::<f64>().ok());
                    let bandwidth = v["bandwidth"].as_u64().unwrap_or(0);

                    if !base_url.is_empty() {
                        let mut http_headers = HashMap::new();
                        http_headers.insert(
                            "Referer".to_string(),
                            "https://www.bilibili.com/".to_string(),
                        );

                        formats.push(Format {
                            format_id: format!("dash-video-{id}-{codecs}"),
                            format_note: Some(quality_label(id).to_string()),
                            ext: "mp4".to_string(),
                            url: Some(base_url.to_string()),
                            protocol: Protocol::Https,
                            width,
                            height,
                            fps,
                            vcodec: Some(codecs.to_string()),
                            acodec: Some("none".to_string()),
                            tbr: Some(bandwidth as f64 / 1000.0),
                            http_headers,
                            ..Default::default()
                        });
                    }
                }
            }

            // Audio streams
            if let Some(audios) = dash.get("audio").and_then(|a| a.as_array()) {
                for a in audios {
                    let id = a["id"].as_u64().unwrap_or(0);
                    let base_url = a["baseUrl"].as_str().or(a["base_url"].as_str()).unwrap_or("");
                    let codecs = a["codecs"].as_str().unwrap_or("unknown");
                    let bandwidth = a["bandwidth"].as_u64().unwrap_or(0);

                    if !base_url.is_empty() {
                        let mut http_headers = HashMap::new();
                        http_headers.insert(
                            "Referer".to_string(),
                            "https://www.bilibili.com/".to_string(),
                        );

                        formats.push(Format {
                            format_id: format!("dash-audio-{id}"),
                            format_note: Some(audio_quality_label(id).to_string()),
                            ext: "m4a".to_string(),
                            url: Some(base_url.to_string()),
                            protocol: Protocol::Https,
                            vcodec: Some("none".to_string()),
                            acodec: Some(codecs.to_string()),
                            abr: Some(bandwidth as f64 / 1000.0),
                            http_headers,
                            ..Default::default()
                        });
                    }
                }
            }
        }

        // Fallback: FLV/MP4 durl
        if formats.is_empty() {
            if let Some(durls) = data["durl"].as_array() {
                for (i, durl) in durls.iter().enumerate() {
                    let url_str = durl["url"].as_str().unwrap_or("");
                    let size = durl["size"].as_u64();
                    if !url_str.is_empty() {
                        let mut http_headers = HashMap::new();
                        http_headers.insert(
                            "Referer".to_string(),
                            "https://www.bilibili.com/".to_string(),
                        );

                        formats.push(Format {
                            format_id: format!("flv-{i}"),
                            format_note: Some("FLV fallback".to_string()),
                            ext: "flv".to_string(),
                            url: Some(url_str.to_string()),
                            protocol: Protocol::Https,
                            filesize: size,
                            http_headers,
                            ..Default::default()
                        });
                    }
                }
            }
        }

        formats
    }
}

impl Default for BilibiliExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoExtractor for BilibiliExtractor {
    fn name(&self) -> &str {
        "Bilibili"
    }

    fn key(&self) -> &str {
        "Bilibili"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[
            r"(?:https?://)?(?:www\.)?bilibili\.com/video/BV[a-zA-Z0-9]+",
            r"(?:https?://)?b23\.tv/BV[a-zA-Z0-9]+",
        ]
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            let bvid = Self::extract_bvid(url)
                .context("could not extract Bilibili BV ID from URL")?;
            info!(bvid = %bvid, "extracting Bilibili video");

            // 1. Get video info (to obtain CID)
            let video_info = Self::fetch_video_info(client, &bvid).await?;

            let cid = video_info["cid"]
                .as_u64()
                .or_else(|| {
                    // Try first page's cid
                    video_info["pages"]
                        .as_array()
                        .and_then(|pages| pages.first())
                        .and_then(|p| p["cid"].as_u64())
                })
                .context("could not find CID in video info")?;

            debug!(cid = cid, "found CID");

            let title = video_info["title"].as_str().map(|s| s.to_string());
            let description = video_info["desc"].as_str().map(|s| s.to_string());
            let uploader = video_info["owner"]["name"].as_str().map(|s| s.to_string());
            let uploader_id = video_info["owner"]["mid"].as_i64().map(|m| m.to_string());
            let duration = video_info["duration"].as_f64();
            let view_count = video_info["stat"]["view"].as_u64();
            let like_count = video_info["stat"]["like"].as_u64();

            let mut thumbnails = Vec::new();
            if let Some(pic) = video_info["pic"].as_str() {
                thumbnails.push(Thumbnail {
                    url: pic.to_string(),
                    id: Some("cover".to_string()),
                    width: None,
                    height: None,
                    preference: Some(1),
                    resolution: None,
                });
            }
            let thumbnail = thumbnails.first().map(|t| t.url.clone());

            // 2. Fetch playurl with DASH
            let playurl_data = Self::fetch_playurl(client, &bvid, cid).await?;
            let formats = Self::parse_dash_formats(&playurl_data);

            info!(format_count = formats.len(), "parsed Bilibili formats");

            let info = InfoDict {
                id: bvid.clone(),
                title: title.clone(),
                fulltitle: title,
                ext: "mp4".to_string(),
                url: None,
                webpage_url: Some(format!("https://www.bilibili.com/video/{bvid}")),
                original_url: Some(url.to_string()),
                display_id: Some(bvid),
                description,
                uploader,
                uploader_id,
                uploader_url: None,
                channel: None,
                channel_id: None,
                channel_url: None,
                duration,
                view_count,
                like_count,
                comment_count: None,
                upload_date: video_info["pubdate"].as_i64().map(|ts| {
                    chrono_timestamp_to_date(ts)
                }),
                timestamp: video_info["pubdate"].as_i64(),
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
                extractor: "bilibili".to_string(),
                extractor_key: "Bilibili".to_string(),
                extra: HashMap::new(),
            };

            Ok(ExtractionResult::SingleVideo(Box::new(info)))
        })
    }
}

/// Convert a Unix timestamp to YYYYMMDD date string.
fn chrono_timestamp_to_date(ts: i64) -> String {
    // Simple conversion without chrono dependency
    // We'll format as YYYYMMDD from the timestamp
    let secs_per_day: i64 = 86400;
    let days_since_epoch = ts / secs_per_day;
    // Approximate date calculation
    let mut year = 1970i64;
    let mut remaining_days = days_since_epoch;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &dim in &days_in_months {
        if remaining_days < dim {
            break;
        }
        remaining_days -= dim;
        month += 1;
    }
    let day = remaining_days + 1;

    format!("{year:04}{month:02}{day:02}")
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bvid() {
        assert_eq!(
            BilibiliExtractor::extract_bvid("https://www.bilibili.com/video/BV1xx411c7mu"),
            Some("BV1xx411c7mu".to_string())
        );
        assert_eq!(
            BilibiliExtractor::extract_bvid("https://b23.tv/BV1xx411c7mu"),
            Some("BV1xx411c7mu".to_string())
        );
    }

    #[test]
    fn test_suitable() {
        let ext = BilibiliExtractor::new();
        assert!(ext.suitable("https://www.bilibili.com/video/BV1xx411c7mu"));
        assert!(ext.suitable("https://b23.tv/BV1xx411c7mu"));
        assert!(!ext.suitable("https://www.youtube.com/watch?v=abc"));
    }

    #[test]
    fn test_quality_labels() {
        assert_eq!(quality_label(120), "4K");
        assert_eq!(quality_label(80), "1080P");
        assert_eq!(quality_label(64), "720P");
    }

    #[test]
    fn test_timestamp_to_date() {
        // 2024-01-15 = roughly 1705276800
        let date = chrono_timestamp_to_date(1705276800);
        assert_eq!(date, "20240115");
    }
}
