use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Represents a single downloadable format (audio, video, or combined).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Format {
    pub format_id: String,
    pub format_note: Option<String>,
    pub ext: String,
    pub url: Option<String>,
    pub manifest_url: Option<String>,
    pub protocol: Protocol,
    // Video
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub vbr: Option<f64>,
    pub dynamic_range: Option<String>,
    // Audio
    pub acodec: Option<String>,
    pub abr: Option<f64>,
    pub asr: Option<u32>,
    pub audio_channels: Option<u8>,
    // Size
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub tbr: Option<f64>,
    // Quality
    pub quality: Option<f64>,
    pub preference: Option<i32>,
    pub language: Option<String>,
    // HTTP specific
    pub http_headers: HashMap<String, String>,
    pub cookies: Option<String>,
    // Fragment-based
    pub fragments: Option<Vec<Fragment>>,
    // Container
    pub container: Option<String>,
    // Flags
    pub is_dash_periods: bool,
}

impl Default for Format {
    fn default() -> Self {
        Self {
            format_id: String::new(),
            format_note: None,
            ext: String::from("unknown"),
            url: None,
            manifest_url: None,
            protocol: Protocol::default(),
            width: None,
            height: None,
            fps: None,
            vcodec: None,
            vbr: None,
            dynamic_range: None,
            acodec: None,
            abr: None,
            asr: None,
            audio_channels: None,
            filesize: None,
            filesize_approx: None,
            tbr: None,
            quality: None,
            preference: None,
            language: None,
            http_headers: HashMap::new(),
            cookies: None,
            fragments: None,
            container: None,
            is_dash_periods: false,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Resolution part
        if let Some(h) = self.height {
            write!(f, "{h}p")?;
            if let Some(fps) = self.fps {
                let fps_int = fps as u32;
                if fps_int > 30 {
                    write!(f, "{fps_int}")?;
                }
            }
        } else if self.acodec.is_some() && self.vcodec.as_deref().map_or(true, |v| v == "none") {
            write!(f, "audio only")?;
        } else {
            write!(f, "unknown")?;
        }

        // Extension
        write!(f, " {}", self.ext)?;

        // Codecs
        let has_video = self.vcodec.as_deref().is_some_and(|v| v != "none");
        let has_audio = self.acodec.as_deref().is_some_and(|a| a != "none");

        if has_video || has_audio {
            write!(f, " (")?;
            if has_video {
                write!(f, "{}", self.vcodec.as_deref().unwrap())?;
            }
            if has_video && has_audio {
                write!(f, "+")?;
            }
            if has_audio {
                write!(f, "{}", self.acodec.as_deref().unwrap())?;
            }
            write!(f, ")")?;
        }

        // File size
        if let Some(size) = self.filesize.or(self.filesize_approx) {
            let display = format_bytes(size);
            write!(f, " {display}")?;
        }

        Ok(())
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1}GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.1}MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1}KiB", b / KIB)
    } else {
        format!("{bytes}B")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    pub url: Option<String>,
    pub path: Option<String>,
    pub duration: Option<f64>,
    pub filesize: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Http,
    Https,
    Hls,
    HlsNative,
    Dash,
    Rtmp,
    Rtsp,
    Websocket,
    Mhtml,
    F4m,
    Ism,
    #[serde(other)]
    Other,
}

impl Default for Protocol {
    fn default() -> Self {
        Self::Https
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Hls => "hls",
            Self::HlsNative => "hls_native",
            Self::Dash => "dash",
            Self::Rtmp => "rtmp",
            Self::Rtsp => "rtsp",
            Self::Websocket => "websocket",
            Self::Mhtml => "mhtml",
            Self::F4m => "f4m",
            Self::Ism => "ism",
            Self::Other => "other",
        };
        write!(f, "{label}")
    }
}

/// Subtitle track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtitle {
    pub ext: String,
    pub url: Option<String>,
    pub data: Option<String>,
    pub name: Option<String>,
}

/// Thumbnail image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thumbnail {
    pub url: String,
    pub id: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub preference: Option<i32>,
    pub resolution: Option<String>,
}

/// Chapter marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub start_time: f64,
    pub end_time: Option<f64>,
    pub title: Option<String>,
}

/// Core info dict -- the central data structure representing an extracted video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoDict {
    pub id: String,
    pub title: Option<String>,
    pub fulltitle: Option<String>,
    pub ext: String,
    pub url: Option<String>,
    pub webpage_url: Option<String>,
    pub original_url: Option<String>,
    pub display_id: Option<String>,
    pub description: Option<String>,
    pub uploader: Option<String>,
    pub uploader_id: Option<String>,
    pub uploader_url: Option<String>,
    pub channel: Option<String>,
    pub channel_id: Option<String>,
    pub channel_url: Option<String>,
    pub duration: Option<f64>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub upload_date: Option<String>,
    pub timestamp: Option<i64>,
    pub age_limit: Option<u8>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub is_live: Option<bool>,
    pub was_live: Option<bool>,
    pub live_status: Option<String>,
    pub release_timestamp: Option<i64>,
    // Formats
    pub formats: Vec<Format>,
    pub requested_formats: Option<Vec<Format>>,
    // Subtitles: lang_code -> list of subtitle options
    pub subtitles: HashMap<String, Vec<Subtitle>>,
    pub automatic_captions: HashMap<String, Vec<Subtitle>>,
    // Thumbnails
    pub thumbnails: Vec<Thumbnail>,
    pub thumbnail: Option<String>,
    // Chapters
    pub chapters: Vec<Chapter>,
    // Playlist
    pub playlist: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub playlist_index: Option<u64>,
    pub n_entries: Option<u64>,
    // Extractor
    pub extractor: String,
    pub extractor_key: String,
    // Extra metadata
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Represents a playlist or channel containing multiple entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub webpage_url: Option<String>,
    pub uploader: Option<String>,
    pub uploader_id: Option<String>,
    pub entries: Vec<PlaylistEntry>,
    pub playlist_count: Option<u64>,
    pub extractor: String,
    pub extractor_key: String,
}

/// A single entry within a playlist (may be a URL to resolve later, or a full InfoDict).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaylistEntry {
    Url(String),
    Info(Box<InfoDict>),
}

/// The result of extraction -- either a single video or a playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtractionResult {
    SingleVideo(Box<InfoDict>),
    Playlist(PlaylistInfo),
}
