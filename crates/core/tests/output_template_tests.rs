use std::collections::HashMap;
use yt_dlp_core::output_template::{render_template, sanitize_filename};
use yt_dlp_core::types::InfoDict;

fn make_test_info() -> InfoDict {
    InfoDict {
        id: "dQw4w9WgXcQ".to_string(),
        title: Some("Never Gonna Give You Up".to_string()),
        fulltitle: Some("Rick Astley - Never Gonna Give You Up".to_string()),
        ext: "mp4".to_string(),
        url: None,
        webpage_url: Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string()),
        original_url: None,
        display_id: None,
        description: Some("The official music video".to_string()),
        uploader: Some("Rick Astley".to_string()),
        uploader_id: Some("RickAstleyVEVO".to_string()),
        uploader_url: None,
        channel: Some("Rick Astley".to_string()),
        channel_id: Some("UCuAXFkgsw1L7xaCfnd5JJOw".to_string()),
        channel_url: None,
        duration: Some(212.0),
        view_count: Some(1_500_000_000),
        like_count: None,
        comment_count: None,
        upload_date: Some("20091025".to_string()),
        timestamp: None,
        age_limit: None,
        categories: vec![],
        tags: vec![],
        is_live: None,
        was_live: None,
        live_status: None,
        release_timestamp: None,
        formats: vec![],
        requested_formats: None,
        subtitles: HashMap::new(),
        automatic_captions: HashMap::new(),
        thumbnails: vec![],
        thumbnail: None,
        chapters: vec![],
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

#[test]
fn test_simple_fields() {
    let info = make_test_info();
    let result = render_template("%(title)s", &info).unwrap();
    assert_eq!(result, "Never Gonna Give You Up");

    let result = render_template("%(id)s", &info).unwrap();
    assert_eq!(result, "dQw4w9WgXcQ");

    let result = render_template("%(ext)s", &info).unwrap();
    assert_eq!(result, "mp4");
}

#[test]
fn test_combined_template() {
    let info = make_test_info();
    let result = render_template("%(title)s [%(id)s].%(ext)s", &info).unwrap();
    assert_eq!(result, "Never Gonna Give You Up [dQw4w9WgXcQ].mp4");
}

#[test]
fn test_missing_field_uses_na() {
    let info = make_test_info();
    // playlist is None, so resolve_field returns "" and the default kicks in
    let result = render_template("%(playlist|NA)s", &info).unwrap();
    assert_eq!(result, "NA");
}

#[test]
fn test_missing_field_no_default_is_empty() {
    let info = make_test_info();
    // like_count is None, no default specified -> empty string
    let result = render_template("likes: %(like_count)s", &info).unwrap();
    assert_eq!(result, "likes: ");
}

#[test]
fn test_truncation() {
    let info = make_test_info();
    let result = render_template("%(title).10s.%(ext)s", &info).unwrap();
    assert_eq!(result, "Never Gonn.mp4");
}

#[test]
fn test_default_value() {
    let info = make_test_info();
    let result = render_template("%(playlist|no_playlist)s - %(title)s", &info).unwrap();
    assert_eq!(result, "no_playlist - Never Gonna Give You Up");
}

#[test]
fn test_default_value_not_used_when_field_present() {
    let info = make_test_info();
    let result = render_template("%(title|fallback)s", &info).unwrap();
    assert_eq!(result, "Never Gonna Give You Up");
}

#[test]
fn test_date_formatting() {
    let info = make_test_info();
    let result = render_template("%(upload_date>%Y-%m-%d)s", &info).unwrap();
    assert_eq!(result, "2009-10-25");
}

#[test]
fn test_date_formatting_year_only() {
    let info = make_test_info();
    let result = render_template("%(upload_date>%Y)s", &info).unwrap();
    assert_eq!(result, "2009");
}

#[test]
fn test_nested_fields() {
    let info = make_test_info();
    let channel = render_template("%(channel)s", &info).unwrap();
    assert_eq!(channel, "Rick Astley");

    let uploader = render_template("%(uploader)s", &info).unwrap();
    assert_eq!(uploader, "Rick Astley");
}

#[test]
fn test_sanitize_removes_slashes() {
    assert_eq!(sanitize_filename("a/b\\c"), "a_b_c");
}

#[test]
fn test_sanitize_removes_null_bytes() {
    assert_eq!(sanitize_filename("test\0file"), "test_file");
}

#[test]
fn test_sanitize_removes_special_chars() {
    assert_eq!(sanitize_filename("a:b*c?d\"e<f>g|h"), "a_b_c_d_e_f_g_h");
}

#[test]
fn test_sanitize_trims_trailing_dots_and_spaces() {
    assert_eq!(sanitize_filename("file..."), "file");
    assert_eq!(sanitize_filename("file   "), "file");
    assert_eq!(sanitize_filename("file. . ."), "file");
}

#[test]
fn test_sanitize_preserves_normal_name() {
    assert_eq!(sanitize_filename("normal name"), "normal name");
    assert_eq!(
        sanitize_filename("My Video [dQw4w9WgXcQ]"),
        "My Video [dQw4w9WgXcQ]"
    );
}

#[test]
fn test_complex_template() {
    let info = make_test_info();
    let result =
        render_template("%(uploader)s/%(title)s [%(id)s].%(ext)s", &info).unwrap();
    assert_eq!(
        result,
        "Rick Astley/Never Gonna Give You Up [dQw4w9WgXcQ].mp4"
    );
}

#[test]
fn test_literal_text_preserved() {
    let info = make_test_info();
    let result = render_template("no templates here", &info).unwrap();
    assert_eq!(result, "no templates here");
}

#[test]
fn test_extra_field() {
    let mut info = make_test_info();
    info.extra.insert(
        "custom_field".to_string(),
        serde_json::Value::String("custom_value".to_string()),
    );
    let result = render_template("%(custom_field)s", &info).unwrap();
    assert_eq!(result, "custom_value");
}
