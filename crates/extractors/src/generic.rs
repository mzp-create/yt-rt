use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::{bail, Context};
use scraper::{Html, Selector};
use tracing::{debug, warn};
use url::Url;

use crate::InfoExtractor;
use yt_dlp_core::types::*;
use yt_dlp_networking::client::HttpClient;

pub struct GenericExtractor;

impl GenericExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl InfoExtractor for GenericExtractor {
    fn name(&self) -> &str {
        "Generic"
    }

    fn key(&self) -> &str {
        "Generic"
    }

    fn suitable_urls(&self) -> &[&str] {
        &[]
    }

    /// The generic extractor matches ANY http/https URL as a last resort.
    fn suitable(&self, url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            // Check if it's a direct media URL first (no need to fetch HTML)
            if is_direct_media_url(url) {
                debug!("detected direct media URL: {}", url);
                return extract_direct_media(url);
            }

            // Fetch the page
            let html_text = client
                .get_text(url)
                .await
                .context("failed to fetch page")?;
            let base_url = Url::parse(url)?;

            // Parse HTML and extract all results synchronously.
            // scraper::Html is !Send so we must not hold it across .await.
            let extraction = {
                let document = Html::parse_document(&html_text);
                extract_from_document(&document, &base_url)?
            };

            match extraction {
                DocExtraction::Found(info) => {
                    return Ok(ExtractionResult::SingleVideo(Box::new(info)));
                }
                DocExtraction::NeedOembed(oembed_url) => {
                    // Fetch oEmbed endpoint (async) and parse
                    if let Some(info) = fetch_oembed(&oembed_url, &base_url, client).await? {
                        debug!("extracted via oEmbed");
                        return Ok(ExtractionResult::SingleVideo(Box::new(info)));
                    }
                }
                DocExtraction::NotFound => {}
            }

            bail!("no media found on page: {}", url)
        })
    }
}

// ---------------------------------------------------------------------------
// Direct media detection
// ---------------------------------------------------------------------------

const MEDIA_EXTENSIONS: &[&str] = &[
    ".mp4", ".webm", ".m3u8", ".mpd", ".mp3", ".m4a", ".ogg", ".wav", ".flac", ".avi", ".mkv",
    ".mov", ".ts", ".f4v", ".f4m",
];

fn is_direct_media_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    let path = lower.split('?').next().unwrap_or(&lower);
    MEDIA_EXTENSIONS.iter().any(|ext| path.ends_with(ext))
}

fn extract_direct_media(url: &str) -> anyhow::Result<ExtractionResult> {
    let parsed = Url::parse(url)?;
    let path = parsed.path();
    let filename = path.rsplit('/').next().unwrap_or("media");
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("mp4")
        .to_lowercase();

    let protocol = if ext == "m3u8" {
        Protocol::Hls
    } else if ext == "mpd" {
        Protocol::Dash
    } else if url.starts_with("http://") {
        Protocol::Http
    } else {
        Protocol::Https
    };

    let format = Format {
        format_id: "direct".to_string(),
        ext: ext.clone(),
        url: Some(url.to_string()),
        protocol,
        ..Default::default()
    };

    let info = InfoDict {
        id: filename.to_string(),
        title: Some(filename.to_string()),
        ext,
        url: Some(url.to_string()),
        webpage_url: Some(url.to_string()),
        formats: vec![format],
        extractor: "Generic".to_string(),
        extractor_key: "Generic".to_string(),
        ..default_info_dict()
    };

    Ok(ExtractionResult::SingleVideo(Box::new(info)))
}

// ---------------------------------------------------------------------------
// Synchronous document extraction (scraper::Html is !Send)
// ---------------------------------------------------------------------------

/// Result of synchronous HTML parsing -- either a found InfoDict, an oEmbed URL
/// that needs async fetching, or nothing found.
enum DocExtraction {
    Found(InfoDict),
    NeedOembed(String),
    NotFound,
}

/// Parse the HTML document synchronously and try all non-async strategies.
/// If oEmbed is discovered, return the endpoint URL for async fetching.
fn extract_from_document(doc: &Html, base_url: &Url) -> anyhow::Result<DocExtraction> {
    // 1. Discover oEmbed endpoint URL (synchronous part only)
    let oembed_url = discover_oembed_url(doc, base_url)?;

    // 2. Open Graph video tags
    if let Some(info) = extract_og_video(doc, base_url)? {
        debug!("extracted via Open Graph video tags");
        return Ok(DocExtraction::Found(info));
    }

    // 3. Twitter player card
    if let Some(info) = extract_twitter_player(doc, base_url)? {
        debug!("extracted via Twitter player card");
        return Ok(DocExtraction::Found(info));
    }

    // 4. HTML5 video/audio tags
    if let Some(info) = extract_html5_media(doc, base_url)? {
        debug!("extracted via HTML5 video/audio tags");
        return Ok(DocExtraction::Found(info));
    }

    // 5. Iframe embeds pointing to known video hosts
    if let Some(info) = extract_iframe_embeds(doc, base_url)? {
        debug!("extracted via iframe embed");
        return Ok(DocExtraction::Found(info));
    }

    // 6. If we found an oEmbed URL, signal it for async fetching
    if let Some(url) = oembed_url {
        return Ok(DocExtraction::NeedOembed(url));
    }

    Ok(DocExtraction::NotFound)
}

// ---------------------------------------------------------------------------
// oEmbed discovery
// ---------------------------------------------------------------------------

/// Synchronously discover the oEmbed endpoint URL from link tags.
fn discover_oembed_url(doc: &Html, base_url: &Url) -> anyhow::Result<Option<String>> {
    let sel = Selector::parse("link[type='application/json+oembed'], link[type='text/xml+oembed']")
        .expect("valid selector");

    let link_el = match doc.select(&sel).next() {
        Some(el) => el,
        None => return Ok(None),
    };

    let href = match link_el.value().attr("href") {
        Some(h) => h,
        None => return Ok(None),
    };

    let link_type = link_el.value().attr("type").unwrap_or("");

    // We only handle JSON oEmbed for now
    if link_type.contains("xml") {
        warn!("XML oEmbed not yet supported, skipping");
        return Ok(None);
    }

    let oembed_url = resolve_url(base_url, href)?;
    debug!("found oEmbed endpoint: {}", oembed_url);
    Ok(Some(oembed_url))
}

/// Fetch oEmbed JSON endpoint and build an InfoDict from it.
async fn fetch_oembed(
    oembed_url: &str,
    base_url: &Url,
    client: &HttpClient,
) -> anyhow::Result<Option<InfoDict>> {
    let oembed_json: serde_json::Value = match client.get_json(oembed_url).await {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to fetch oEmbed endpoint: {}", e);
            return Ok(None);
        }
    };

    let oembed_type = oembed_json
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if oembed_type != "video" && oembed_type != "rich" {
        return Ok(None);
    }

    // Try to extract a video URL from the oEmbed "html" field (usually an iframe)
    let video_url = if let Some(html_field) = oembed_json.get("html").and_then(|v| v.as_str()) {
        extract_iframe_src_from_html(html_field)
    } else {
        oembed_json
            .get("url")
            .and_then(|v| v.as_str())
            .map(String::from)
    };

    let video_url = match video_url {
        Some(u) => u,
        None => return Ok(None),
    };

    let title = oembed_json
        .get("title")
        .and_then(|v| v.as_str())
        .map(String::from);
    let author = oembed_json
        .get("author_name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let thumbnail = oembed_json
        .get("thumbnail_url")
        .and_then(|v| v.as_str())
        .map(String::from);
    let width = oembed_json
        .get("width")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let height = oembed_json
        .get("height")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let format = Format {
        format_id: "oembed".to_string(),
        ext: guess_ext_from_url(&video_url),
        url: Some(video_url.clone()),
        width,
        height,
        ..Default::default()
    };

    let thumbnails = thumbnail
        .iter()
        .map(|u| Thumbnail {
            url: u.clone(),
            id: Some("oembed".to_string()),
            width: None,
            height: None,
            preference: None,
            resolution: None,
        })
        .collect();

    let info = InfoDict {
        id: id_from_url(base_url),
        title,
        ext: guess_ext_from_url(&video_url),
        url: Some(video_url),
        webpage_url: Some(base_url.to_string()),
        original_url: Some(base_url.to_string()),
        uploader: author,
        formats: vec![format],
        thumbnails,
        extractor: "Generic".to_string(),
        extractor_key: "Generic".to_string(),
        ..default_info_dict()
    };

    Ok(Some(info))
}

fn extract_iframe_src_from_html(html: &str) -> Option<String> {
    let doc = Html::parse_fragment(html);
    let sel = Selector::parse("iframe[src]").expect("valid selector");
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("src"))
        .map(String::from)
}

// ---------------------------------------------------------------------------
// Open Graph video tags
// ---------------------------------------------------------------------------

fn extract_og_video(doc: &Html, base_url: &Url) -> anyhow::Result<Option<InfoDict>> {
    let video_url = get_meta_property(doc, "og:video:secure_url")
        .or_else(|| get_meta_property(doc, "og:video:url"))
        .or_else(|| get_meta_property(doc, "og:video"));

    let video_url = match video_url {
        Some(u) => resolve_url(base_url, &u)?,
        None => return Ok(None),
    };

    let title = get_meta_property(doc, "og:title");
    let description = get_meta_property(doc, "og:description");
    let thumbnail_url = get_meta_property(doc, "og:image");
    let site_name = get_meta_property(doc, "og:site_name");
    let video_type = get_meta_property(doc, "og:video:type");
    let width = get_meta_property(doc, "og:video:width").and_then(|s| s.parse::<u32>().ok());
    let height = get_meta_property(doc, "og:video:height").and_then(|s| s.parse::<u32>().ok());

    let ext = video_type
        .as_deref()
        .and_then(mime_to_ext)
        .unwrap_or_else(|| guess_ext_from_url(&video_url));

    let format = Format {
        format_id: "og_video".to_string(),
        ext: ext.clone(),
        url: Some(video_url.clone()),
        width,
        height,
        ..Default::default()
    };

    let thumbnails = thumbnail_url
        .iter()
        .map(|u| Thumbnail {
            url: u.clone(),
            id: Some("og_image".to_string()),
            width: None,
            height: None,
            preference: None,
            resolution: None,
        })
        .collect();

    let info = InfoDict {
        id: id_from_url(base_url),
        title,
        description,
        ext,
        url: Some(video_url),
        webpage_url: Some(base_url.to_string()),
        original_url: Some(base_url.to_string()),
        channel: site_name,
        formats: vec![format],
        thumbnails,
        extractor: "Generic".to_string(),
        extractor_key: "Generic".to_string(),
        ..default_info_dict()
    };

    Ok(Some(info))
}

// ---------------------------------------------------------------------------
// Twitter player card
// ---------------------------------------------------------------------------

fn extract_twitter_player(doc: &Html, base_url: &Url) -> anyhow::Result<Option<InfoDict>> {
    let stream_url = get_meta_name(doc, "twitter:player:stream");
    let player_url = get_meta_name(doc, "twitter:player");

    let video_url = match stream_url.or(player_url) {
        Some(u) => resolve_url(base_url, &u)?,
        None => return Ok(None),
    };

    let title = get_meta_name(doc, "twitter:title");
    let description = get_meta_name(doc, "twitter:description");
    let thumbnail_url = get_meta_name(doc, "twitter:image");
    let width = get_meta_name(doc, "twitter:player:width").and_then(|s| s.parse::<u32>().ok());
    let height = get_meta_name(doc, "twitter:player:height").and_then(|s| s.parse::<u32>().ok());
    let content_type = get_meta_name(doc, "twitter:player:stream:content_type");

    let ext = content_type
        .as_deref()
        .and_then(mime_to_ext)
        .unwrap_or_else(|| guess_ext_from_url(&video_url));

    let format = Format {
        format_id: "twitter_player".to_string(),
        ext: ext.clone(),
        url: Some(video_url.clone()),
        width,
        height,
        ..Default::default()
    };

    let thumbnails = thumbnail_url
        .iter()
        .map(|u| Thumbnail {
            url: u.clone(),
            id: Some("twitter_image".to_string()),
            width: None,
            height: None,
            preference: None,
            resolution: None,
        })
        .collect();

    let info = InfoDict {
        id: id_from_url(base_url),
        title,
        description,
        ext,
        url: Some(video_url),
        webpage_url: Some(base_url.to_string()),
        original_url: Some(base_url.to_string()),
        formats: vec![format],
        thumbnails,
        extractor: "Generic".to_string(),
        extractor_key: "Generic".to_string(),
        ..default_info_dict()
    };

    Ok(Some(info))
}

// ---------------------------------------------------------------------------
// HTML5 <video>/<audio>/<source> tags
// ---------------------------------------------------------------------------

fn extract_html5_media(doc: &Html, base_url: &Url) -> anyhow::Result<Option<InfoDict>> {
    let mut formats: Vec<Format> = Vec::new();

    // <video src="...">
    let video_sel = Selector::parse("video[src]").expect("valid selector");
    for el in doc.select(&video_sel) {
        if let Some(src) = el.value().attr("src") {
            let resolved = resolve_url(base_url, src)?;
            formats.push(Format {
                format_id: format!("html5_video_{}", formats.len()),
                ext: guess_ext_from_url(&resolved),
                url: Some(resolved),
                width: el
                    .value()
                    .attr("width")
                    .and_then(|s| s.parse::<u32>().ok()),
                height: el
                    .value()
                    .attr("height")
                    .and_then(|s| s.parse::<u32>().ok()),
                ..Default::default()
            });
        }
    }

    // <video><source src="..." type="..."></video> and standalone <source> tags
    let source_sel = Selector::parse("video source[src], audio source[src]").expect("valid selector");
    for el in doc.select(&source_sel) {
        if let Some(src) = el.value().attr("src") {
            let resolved = resolve_url(base_url, src)?;
            let mime = el.value().attr("type");
            let ext = mime
                .and_then(mime_to_ext)
                .unwrap_or_else(|| guess_ext_from_url(&resolved));
            formats.push(Format {
                format_id: format!("html5_source_{}", formats.len()),
                ext,
                url: Some(resolved),
                ..Default::default()
            });
        }
    }

    // <audio src="...">
    let audio_sel = Selector::parse("audio[src]").expect("valid selector");
    for el in doc.select(&audio_sel) {
        if let Some(src) = el.value().attr("src") {
            let resolved = resolve_url(base_url, src)?;
            formats.push(Format {
                format_id: format!("html5_audio_{}", formats.len()),
                ext: guess_ext_from_url(&resolved),
                url: Some(resolved),
                vcodec: Some("none".to_string()),
                ..Default::default()
            });
        }
    }

    if formats.is_empty() {
        return Ok(None);
    }

    // Get page title for metadata
    let title = get_page_title(doc);
    let ext = formats[0].ext.clone();
    let url = formats[0].url.clone();

    let info = InfoDict {
        id: id_from_url(base_url),
        title,
        ext,
        url,
        webpage_url: Some(base_url.to_string()),
        original_url: Some(base_url.to_string()),
        formats,
        extractor: "Generic".to_string(),
        extractor_key: "Generic".to_string(),
        ..default_info_dict()
    };

    Ok(Some(info))
}

// ---------------------------------------------------------------------------
// Iframe embeds
// ---------------------------------------------------------------------------

/// Known embed URL patterns that likely contain video content.
const KNOWN_EMBED_PATTERNS: &[&str] = &[
    "youtube.com/embed/",
    "youtube-nocookie.com/embed/",
    "player.vimeo.com/video/",
    "dailymotion.com/embed/",
    "facebook.com/plugins/video",
    "instagram.com/p/",
    "tiktok.com/embed/",
    "twitch.tv/embed",
    "streamable.com/e/",
    "rumble.com/embed/",
    "bitchute.com/embed/",
];

fn extract_iframe_embeds(doc: &Html, base_url: &Url) -> anyhow::Result<Option<InfoDict>> {
    let iframe_sel = Selector::parse("iframe[src]").expect("valid selector");

    for el in doc.select(&iframe_sel) {
        let src = match el.value().attr("src") {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };

        let resolved = resolve_url(base_url, src)?;

        let is_known = KNOWN_EMBED_PATTERNS
            .iter()
            .any(|pattern| resolved.contains(pattern));

        if !is_known {
            continue;
        }

        let title = get_page_title(doc);
        let width = el
            .value()
            .attr("width")
            .and_then(|s| s.parse::<u32>().ok());
        let height = el
            .value()
            .attr("height")
            .and_then(|s| s.parse::<u32>().ok());

        let format = Format {
            format_id: "iframe_embed".to_string(),
            ext: "mp4".to_string(),
            url: Some(resolved.clone()),
            width,
            height,
            ..Default::default()
        };

        let info = InfoDict {
            id: id_from_url(base_url),
            title,
            ext: "mp4".to_string(),
            url: Some(resolved),
            webpage_url: Some(base_url.to_string()),
            original_url: Some(base_url.to_string()),
            formats: vec![format],
            extractor: "Generic".to_string(),
            extractor_key: "Generic".to_string(),
            ..default_info_dict()
        };

        return Ok(Some(info));
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get `<meta property="...">` content value.
fn get_meta_property(doc: &Html, property: &str) -> Option<String> {
    let selector_str = format!("meta[property='{property}']");
    let sel = Selector::parse(&selector_str).ok()?;
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get `<meta name="...">` content value.
fn get_meta_name(doc: &Html, name: &str) -> Option<String> {
    let selector_str = format!("meta[name='{name}']");
    let sel = Selector::parse(&selector_str).ok()?;
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get the `<title>` text of the page.
fn get_page_title(doc: &Html) -> Option<String> {
    let sel = Selector::parse("title").expect("valid selector");
    doc.select(&sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Resolve a possibly-relative URL against a base URL.
fn resolve_url(base: &Url, href: &str) -> anyhow::Result<String> {
    let resolved = base.join(href).context("failed to resolve relative URL")?;
    Ok(resolved.to_string())
}

/// Derive a simple ID from the URL path.
fn id_from_url(url: &Url) -> String {
    let path = url.path().trim_matches('/');
    if path.is_empty() {
        url.host_str().unwrap_or("unknown").to_string()
    } else {
        // Use last path segment, stripping extension
        let segment = path.rsplit('/').next().unwrap_or(path);
        let id = segment.split('.').next().unwrap_or(segment);
        id.to_string()
    }
}

/// Guess file extension from a URL.
fn guess_ext_from_url(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    let filename = path.rsplit('/').next().unwrap_or("");
    if let Some(ext) = filename.rsplit('.').next() {
        let ext_lower = ext.to_lowercase();
        if MEDIA_EXTENSIONS
            .iter()
            .any(|e| e.trim_start_matches('.') == ext_lower)
        {
            return ext_lower;
        }
    }
    "mp4".to_string()
}

/// Map a MIME type to a file extension.
fn mime_to_ext(mime: &str) -> Option<String> {
    let mime_lower = mime.to_lowercase();
    let ext = match mime_lower.as_str() {
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/ogg" => "ogg",
        "video/x-flv" => "flv",
        "video/x-matroska" => "mkv",
        "video/quicktime" => "mov",
        "video/x-msvideo" => "avi",
        "video/3gpp" => "3gp",
        "application/x-mpegURL" | "application/vnd.apple.mpegurl" => "m3u8",
        "application/dash+xml" => "mpd",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/webm" => "webm",
        _ => return None,
    };
    Some(ext.to_string())
}

/// Create a default `InfoDict` with all fields set to empty/default values.
fn default_info_dict() -> InfoDict {
    InfoDict {
        id: String::new(),
        title: None,
        fulltitle: None,
        ext: String::from("mp4"),
        url: None,
        webpage_url: None,
        original_url: None,
        display_id: None,
        description: None,
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
        formats: Vec::new(),
        requested_formats: None,
        subtitles: HashMap::new(),
        automatic_captions: HashMap::new(),
        thumbnails: Vec::new(),
        thumbnail: None,
        chapters: Vec::new(),
        playlist: None,
        playlist_id: None,
        playlist_title: None,
        playlist_index: None,
        n_entries: None,
        extractor: String::from("Generic"),
        extractor_key: String::from("Generic"),
        extra: HashMap::new(),
    }
}
