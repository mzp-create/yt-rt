use yt_dlp_core::format_selection::select_formats;
use yt_dlp_core::types::{Format, Protocol};

fn make_video_format(id: &str, height: u32, vcodec: &str, ext: &str) -> Format {
    Format {
        format_id: id.to_string(),
        ext: ext.to_string(),
        height: Some(height),
        vcodec: Some(vcodec.to_string()),
        acodec: Some("none".to_string()),
        protocol: Protocol::Https,
        ..Default::default()
    }
}

fn make_audio_format(id: &str, abr: f64, acodec: &str, ext: &str) -> Format {
    Format {
        format_id: id.to_string(),
        ext: ext.to_string(),
        vcodec: Some("none".to_string()),
        acodec: Some(acodec.to_string()),
        abr: Some(abr),
        protocol: Protocol::Https,
        ..Default::default()
    }
}

fn make_combined_format(
    id: &str,
    height: u32,
    vcodec: &str,
    acodec: &str,
    ext: &str,
) -> Format {
    Format {
        format_id: id.to_string(),
        ext: ext.to_string(),
        height: Some(height),
        vcodec: Some(vcodec.to_string()),
        acodec: Some(acodec.to_string()),
        protocol: Protocol::Https,
        ..Default::default()
    }
}

#[test]
fn test_best_selects_highest_combined() {
    let formats = vec![
        make_combined_format("360p", 360, "h264", "aac", "mp4"),
        make_combined_format("1080p", 1080, "h264", "aac", "mp4"),
        make_combined_format("720p", 720, "h264", "aac", "mp4"),
    ];
    let result = select_formats(&formats, "best").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].format_id, "1080p");
}

#[test]
fn test_worst_selects_lowest() {
    let formats = vec![
        make_combined_format("360p", 360, "h264", "aac", "mp4"),
        make_combined_format("1080p", 1080, "h264", "aac", "mp4"),
        make_combined_format("720p", 720, "h264", "aac", "mp4"),
    ];
    let result = select_formats(&formats, "worst").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].format_id, "360p");
}

#[test]
fn test_bestvideo_video_only() {
    let formats = vec![
        make_video_format("v360", 360, "h264", "mp4"),
        make_video_format("v1080", 1080, "h264", "mp4"),
        make_audio_format("a128", 128.0, "aac", "m4a"),
        make_combined_format("c720", 720, "h264", "aac", "mp4"),
    ];
    let result = select_formats(&formats, "bestvideo").unwrap();
    assert_eq!(result.len(), 1);
    // Should pick video-only format with highest resolution
    assert_eq!(result[0].format_id, "v1080");
}

#[test]
fn test_bestaudio_audio_only() {
    let formats = vec![
        make_video_format("v1080", 1080, "h264", "mp4"),
        make_audio_format("a64", 64.0, "opus", "webm"),
        make_audio_format("a128", 128.0, "aac", "m4a"),
        make_audio_format("a256", 256.0, "aac", "m4a"),
        make_combined_format("c720", 720, "h264", "aac", "mp4"),
    ];
    let result = select_formats(&formats, "bestaudio").unwrap();
    assert_eq!(result.len(), 1);
    // Should pick audio-only format with highest abr
    assert_eq!(result[0].format_id, "a256");
}

#[test]
fn test_bestvideo_plus_bestaudio_merge() {
    let formats = vec![
        make_video_format("v360", 360, "h264", "mp4"),
        make_video_format("v1080", 1080, "h264", "mp4"),
        make_audio_format("a64", 64.0, "opus", "webm"),
        make_audio_format("a128", 128.0, "aac", "m4a"),
    ];
    let result = select_formats(&formats, "bestvideo+bestaudio").unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].format_id, "v1080");
    assert_eq!(result[1].format_id, "a128");
}

#[test]
fn test_format_by_id() {
    let formats = vec![
        make_video_format("137", 1080, "h264", "mp4"),
        make_audio_format("140", 128.0, "aac", "m4a"),
        make_combined_format("18", 360, "h264", "aac", "mp4"),
    ];
    let result = select_formats(&formats, "140").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].format_id, "140");
}

#[test]
fn test_filter_height_le() {
    let formats = vec![
        make_combined_format("360p", 360, "h264", "aac", "mp4"),
        make_combined_format("720p", 720, "h264", "aac", "mp4"),
        make_combined_format("1080p", 1080, "h264", "aac", "mp4"),
        make_combined_format("4k", 2160, "h264", "aac", "mp4"),
    ];
    let result = select_formats(&formats, "best[height<=1080]").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].format_id, "1080p");
}

#[test]
fn test_filter_ext_eq() {
    let formats = vec![
        make_combined_format("webm_1080", 1080, "vp9", "opus", "webm"),
        make_combined_format("mp4_1080", 1080, "h264", "aac", "mp4"),
        make_combined_format("webm_720", 720, "vp9", "opus", "webm"),
    ];
    let result = select_formats(&formats, "best[ext=mp4]").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].format_id, "mp4_1080");
}

#[test]
fn test_filter_vcodec_ne_none() {
    let formats = vec![
        make_video_format("v1080", 1080, "h264", "mp4"),
        make_audio_format("a128", 128.0, "aac", "m4a"),
        make_combined_format("c720", 720, "h264", "aac", "mp4"),
    ];
    // Selecting best where vcodec != none should exclude audio-only formats
    let result = select_formats(&formats, "best[vcodec!=none]").unwrap();
    assert_eq!(result.len(), 1);
    // Should pick the one with highest video quality among formats with video
    assert_eq!(result[0].format_id, "v1080");
}

#[test]
fn test_fallback_chain() {
    // Only audio formats, so bestvideo+bestaudio fails, falls back to "best"
    let formats = vec![
        make_audio_format("a64", 64.0, "opus", "webm"),
        make_audio_format("a128", 128.0, "aac", "m4a"),
    ];
    let result = select_formats(&formats, "bestvideo+bestaudio/best").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].format_id, "a128");
}

#[test]
fn test_empty_format_string_errors() {
    let formats = vec![make_combined_format("c720", 720, "h264", "aac", "mp4")];
    let result = select_formats(&formats, "");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("empty format string"),
        "unexpected error: {err_msg}"
    );
}

#[test]
fn test_no_matching_format_errors() {
    let formats = vec![make_combined_format("c720", 720, "h264", "aac", "mp4")];
    let result = select_formats(&formats, "best[height>=4320]");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no matching format"),
        "unexpected error: {err_msg}"
    );
}

#[test]
fn test_multiple_filters() {
    let formats = vec![
        make_combined_format("mp4_720", 720, "h264", "aac", "mp4"),
        make_combined_format("webm_1080", 1080, "vp9", "opus", "webm"),
        make_combined_format("mp4_1080", 1080, "h264", "aac", "mp4"),
        make_combined_format("mp4_4k", 2160, "h264", "aac", "mp4"),
    ];
    let result = select_formats(&formats, "best[ext=mp4][height<=1080]").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].format_id, "mp4_1080");
}

#[test]
fn test_worst_video_plus_worst_audio() {
    let formats = vec![
        make_video_format("v360", 360, "h264", "mp4"),
        make_video_format("v1080", 1080, "h264", "mp4"),
        make_audio_format("a64", 64.0, "opus", "webm"),
        make_audio_format("a256", 256.0, "aac", "m4a"),
    ];
    let result = select_formats(&formats, "worstvideo+worstaudio").unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].format_id, "v360");
    assert_eq!(result[1].format_id, "a64");
}
