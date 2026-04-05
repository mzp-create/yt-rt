use yt_dlp_core::types::{Format, Protocol};

use super::types::YtFormat;

/// Convert a YouTube API format to our core Format type.
pub fn parse_youtube_format(yt_fmt: &YtFormat) -> Format {
    let (vcodec, acodec) = parse_mime_codecs(&yt_fmt.mime_type);
    let ext = mime_to_ext(&yt_fmt.mime_type);
    let protocol = detect_protocol(yt_fmt);

    let is_video_only = vcodec.is_some()
        && vcodec.as_deref() != Some("none")
        && acodec.as_deref() == Some("none");
    let is_audio_only = acodec.is_some()
        && acodec.as_deref() != Some("none")
        && vcodec.as_deref() == Some("none");

    Format {
        format_id: yt_fmt.itag.to_string(),
        format_note: yt_fmt
            .quality_label
            .clone()
            .or_else(|| yt_fmt.quality.clone()),
        ext,
        url: yt_fmt.url.clone(),
        protocol,
        width: yt_fmt.width,
        height: yt_fmt.height,
        fps: yt_fmt.fps,
        vcodec,
        acodec,
        tbr: yt_fmt.bitrate.map(|b| b as f64 / 1000.0),
        abr: if is_audio_only {
            yt_fmt.average_bitrate.map(|b| b as f64 / 1000.0)
        } else {
            None
        },
        vbr: if is_video_only {
            yt_fmt.average_bitrate.map(|b| b as f64 / 1000.0)
        } else {
            None
        },
        asr: yt_fmt
            .audio_sample_rate
            .as_ref()
            .and_then(|s| s.parse().ok()),
        audio_channels: yt_fmt.audio_channels,
        filesize: yt_fmt
            .content_length
            .as_ref()
            .and_then(|s| s.parse().ok()),
        ..Default::default()
    }
}

/// Parse a mime type such as `video/mp4; codecs="avc1.640028, mp4a.40.2"` into
/// `(Option<vcodec>, Option<acodec>)`.
///
/// For `video/*` types with a single codec, acodec is set to `"none"`.
/// For `audio/*` types with a single codec, vcodec is set to `"none"`.
fn parse_mime_codecs(mime: &str) -> (Option<String>, Option<String>) {
    let is_video = mime.starts_with("video/");
    let is_audio = mime.starts_with("audio/");

    // Extract codecs from the `codecs="..."` portion.
    let codecs: Vec<&str> = if let Some(start) = mime.find("codecs=\"") {
        let after = &mime[start + 8..];
        if let Some(end) = after.find('"') {
            after[..end]
                .split(',')
                .map(|c| c.trim())
                .filter(|c| !c.is_empty())
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    match codecs.len() {
        0 => {
            // No codec info -- infer from mime type.
            if is_video {
                (Some("unknown".to_string()), Some("unknown".to_string()))
            } else if is_audio {
                (Some("none".to_string()), Some("unknown".to_string()))
            } else {
                (None, None)
            }
        }
        1 => {
            let codec = codecs[0].to_string();
            if is_video {
                // Single codec in video container -> video-only stream.
                (Some(codec), Some("none".to_string()))
            } else if is_audio {
                // Audio container.
                (Some("none".to_string()), Some(codec))
            } else {
                (Some(codec), None)
            }
        }
        _ => {
            // Two or more codecs: first is video, second is audio.
            let vcodec = codecs[0].to_string();
            let acodec = codecs[1].to_string();
            (Some(vcodec), Some(acodec))
        }
    }
}

/// Map a mime type string to a file extension.
fn mime_to_ext(mime: &str) -> String {
    let base = mime.split(';').next().unwrap_or(mime).trim();
    match base {
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/3gpp" => "3gp",
        "video/x-flv" => "flv",
        "audio/mp4" => "m4a",
        "audio/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        _ => "unknown",
    }
    .to_string()
}

/// Detect the download protocol from the format's URL.
fn detect_protocol(yt_fmt: &YtFormat) -> Protocol {
    match &yt_fmt.url {
        Some(u) if u.contains(".m3u8") => Protocol::Hls,
        Some(u) if u.contains("/manifest/") => Protocol::Dash,
        _ => Protocol::Https,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mime_codecs_video_audio() {
        let (v, a) = parse_mime_codecs(r#"video/mp4; codecs="avc1.640028, mp4a.40.2""#);
        assert_eq!(v.as_deref(), Some("avc1.640028"));
        assert_eq!(a.as_deref(), Some("mp4a.40.2"));
    }

    #[test]
    fn test_parse_mime_codecs_video_only() {
        let (v, a) = parse_mime_codecs(r#"video/webm; codecs="vp9""#);
        assert_eq!(v.as_deref(), Some("vp9"));
        assert_eq!(a.as_deref(), Some("none"));
    }

    #[test]
    fn test_parse_mime_codecs_audio_only() {
        let (v, a) = parse_mime_codecs(r#"audio/mp4; codecs="mp4a.40.2""#);
        assert_eq!(v.as_deref(), Some("none"));
        assert_eq!(a.as_deref(), Some("mp4a.40.2"));
    }

    #[test]
    fn test_parse_mime_codecs_no_codecs() {
        let (v, a) = parse_mime_codecs("video/mp4");
        assert_eq!(v.as_deref(), Some("unknown"));
        assert_eq!(a.as_deref(), Some("unknown"));
    }

    #[test]
    fn test_mime_to_ext() {
        assert_eq!(mime_to_ext("video/mp4; codecs=\"avc1\""), "mp4");
        assert_eq!(mime_to_ext("audio/mp4; codecs=\"mp4a\""), "m4a");
        assert_eq!(mime_to_ext("video/webm"), "webm");
        assert_eq!(mime_to_ext("audio/webm; codecs=\"opus\""), "webm");
    }

    #[test]
    fn test_parse_youtube_format_basic() {
        let yt_fmt = YtFormat {
            itag: 22,
            url: Some("https://example.com/video".to_string()),
            signature_cipher: None,
            cipher: None,
            mime_type: r#"video/mp4; codecs="avc1.64001F, mp4a.40.2""#.to_string(),
            bitrate: Some(2_500_000),
            width: Some(1280),
            height: Some(720),
            content_length: Some("50000000".to_string()),
            quality: Some("hd720".to_string()),
            quality_label: Some("720p".to_string()),
            fps: Some(30.0),
            audio_quality: Some("AUDIO_QUALITY_MEDIUM".to_string()),
            audio_sample_rate: Some("44100".to_string()),
            audio_channels: Some(2),
            average_bitrate: Some(2_000_000),
            approx_duration_ms: Some("300000".to_string()),
            last_modified: None,
            projection_type: None,
            color_info: None,
            init_range: None,
            index_range: None,
        };

        let fmt = parse_youtube_format(&yt_fmt);
        assert_eq!(fmt.format_id, "22");
        assert_eq!(fmt.ext, "mp4");
        assert_eq!(fmt.width, Some(1280));
        assert_eq!(fmt.height, Some(720));
        assert_eq!(fmt.vcodec.as_deref(), Some("avc1.64001F"));
        assert_eq!(fmt.acodec.as_deref(), Some("mp4a.40.2"));
        assert_eq!(fmt.format_note.as_deref(), Some("720p"));
        assert_eq!(fmt.asr, Some(44100));
        assert_eq!(fmt.filesize, Some(50000000));
    }
}
