use serde::Deserialize;

/// YouTube client types for innertube API
#[derive(Debug, Clone)]
pub struct InnertubeClient {
    pub client_name: &'static str,
    pub client_version: &'static str,
    pub api_key: &'static str,
    pub user_agent: &'static str,
}

/// Pre-configured clients
pub const WEB_CLIENT: InnertubeClient = InnertubeClient {
    client_name: "WEB",
    client_version: "2.20240530.02.00",
    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
    user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
};

pub const ANDROID_CLIENT: InnertubeClient = InnertubeClient {
    client_name: "ANDROID",
    client_version: "19.29.37",
    api_key: "AIzaSyA8eiZmM1FaDVjRy-df2KTyQ_vz_yYM39w",
    user_agent: "com.google.android.youtube/19.29.37 (Linux; U; Android 14) gzip",
};

pub const IOS_CLIENT: InnertubeClient = InnertubeClient {
    client_name: "IOS",
    client_version: "19.29.1",
    api_key: "AIzaSyB-63vPrdThhKuerbB2N_l7Kwwcxj6yUAc",
    user_agent: "com.google.ios.youtube/19.29.1 (iPhone16,2; U; CPU iOS 17_5_1 like Mac OS X;)",
};

pub const TV_EMBED_CLIENT: InnertubeClient = InnertubeClient {
    client_name: "TVHTML5_SIMPLY_EMBEDDED_PLAYER",
    client_version: "2.0",
    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
    user_agent: "Mozilla/5.0 (SMART-TV; LINUX; Tizen 6.5)",
};

/// Streaming data from innertube response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingData {
    pub formats: Option<Vec<YtFormat>>,
    pub adaptive_formats: Option<Vec<YtFormat>>,
    pub expires_in_seconds: Option<String>,
    pub hls_manifest_url: Option<String>,
    pub dash_manifest_url: Option<String>,
}

/// YouTube format from API response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtFormat {
    pub itag: u32,
    pub url: Option<String>,
    pub signature_cipher: Option<String>,
    pub cipher: Option<String>,
    pub mime_type: String,
    pub bitrate: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub content_length: Option<String>,
    pub quality: Option<String>,
    pub quality_label: Option<String>,
    pub fps: Option<f64>,
    pub audio_quality: Option<String>,
    pub audio_sample_rate: Option<String>,
    pub audio_channels: Option<u8>,
    pub average_bitrate: Option<u64>,
    pub approx_duration_ms: Option<String>,
    pub last_modified: Option<String>,
    pub projection_type: Option<String>,
    pub color_info: Option<serde_json::Value>,
    pub init_range: Option<Range>,
    pub index_range: Option<Range>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: String,
    pub end: String,
}

/// Video details from innertube response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetails {
    pub video_id: String,
    pub title: String,
    pub length_seconds: Option<String>,
    pub channel_id: Option<String>,
    pub short_description: Option<String>,
    pub thumbnail: Option<ThumbnailList>,
    pub view_count: Option<String>,
    pub author: Option<String>,
    pub is_live_content: Option<bool>,
    pub is_live: Option<bool>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThumbnailList {
    pub thumbnails: Vec<ThumbnailItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailItem {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Microformat data for extra metadata
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroformatRenderer {
    pub publish_date: Option<String>,
    pub upload_date: Option<String>,
    pub category: Option<String>,
    pub owner_channel_name: Option<String>,
    pub external_channel_id: Option<String>,
    pub available_countries: Option<Vec<String>>,
    pub is_unlisted: Option<bool>,
}

/// Full innertube /player response (partial -- only the fields we need)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerResponse {
    pub streaming_data: Option<StreamingData>,
    pub video_details: Option<VideoDetails>,
    pub playability_status: Option<PlayabilityStatus>,
    pub microformat: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayabilityStatus {
    pub status: String,
    pub reason: Option<String>,
    pub playable_in_embed: Option<bool>,
    pub live_streamability: Option<serde_json::Value>,
}
