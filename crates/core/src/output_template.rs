//! Output filename template engine, porting yt-dlp's `%(field)s` syntax.
//!
//! Supports:
//! - `%(field)s` — basic field substitution
//! - `%(field).100s` — truncation to 100 characters
//! - `%(field>%Y)s` — date formatting (for date/timestamp fields)
//! - `%(field|default)s` — default value if field is missing

use crate::error::Result;
use crate::types::InfoDict;
use chrono::{DateTime, NaiveDate};
use regex::Regex;

/// Render an output template string using values from an `InfoDict`.
pub fn render_template(template: &str, info: &InfoDict) -> Result<String> {
    let re = Regex::new(r"%\(([^)]+)\)([.\d]*)s").expect("valid regex");

    let mut result = String::with_capacity(template.len());
    let mut last_end = 0;

    for cap in re.captures_iter(template) {
        let full_match = cap.get(0).unwrap();
        result.push_str(&template[last_end..full_match.start()]);

        let inner = &cap[1];
        let format_spec = &cap[2];

        // Parse inner: field_name, optional `>date_format`, optional `|default`
        let (field_name, date_format, default_value) = parse_inner(inner);

        // Resolve the field value
        let raw_value = resolve_field(info, &field_name);

        // Apply date formatting if requested
        let value = if let Some(date_fmt) = &date_format {
            apply_date_format(&raw_value, date_fmt)
        } else {
            raw_value
        };

        // Apply default if the value is empty
        let value = if value.is_empty() {
            default_value.unwrap_or_default()
        } else {
            value
        };

        // Apply truncation if specified (e.g. `.100`)
        let value = apply_truncation(&value, format_spec);

        result.push_str(&value);
        last_end = full_match.end();
    }

    result.push_str(&template[last_end..]);

    // Replace characters that are invalid in filenames
    Ok(result)
}

/// Sanitize a string for use as a filename, replacing problematic characters.
pub fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => out.push('_'),
            _ => out.push(ch),
        }
    }
    // Trim trailing dots and spaces (problematic on Windows)
    out.trim_end_matches(['.', ' ']).to_string()
}

/// Parse the inner part of a template field: `field_name>date_fmt|default`.
fn parse_inner(inner: &str) -> (String, Option<String>, Option<String>) {
    let (rest, default_value) = if let Some(idx) = inner.rfind('|') {
        (&inner[..idx], Some(inner[idx + 1..].to_string()))
    } else {
        (inner, None)
    };

    let (field_name, date_format) = if let Some(idx) = rest.find('>') {
        (rest[..idx].to_string(), Some(rest[idx + 1..].to_string()))
    } else {
        (rest.to_string(), None)
    };

    (field_name, date_format, default_value)
}

/// Resolve a field name to its string value from the InfoDict.
fn resolve_field(info: &InfoDict, field: &str) -> String {
    match field {
        "id" => info.id.clone(),
        "title" => info.title.clone().unwrap_or_default(),
        "fulltitle" => info.fulltitle.clone().unwrap_or_default(),
        "ext" => info.ext.clone(),
        "url" => info.url.clone().unwrap_or_default(),
        "webpage_url" => info.webpage_url.clone().unwrap_or_default(),
        "original_url" => info.original_url.clone().unwrap_or_default(),
        "display_id" => info.display_id.clone().unwrap_or_else(|| info.id.clone()),
        "description" => info.description.clone().unwrap_or_default(),
        "uploader" => info.uploader.clone().unwrap_or_default(),
        "uploader_id" => info.uploader_id.clone().unwrap_or_default(),
        "uploader_url" => info.uploader_url.clone().unwrap_or_default(),
        "channel" => info.channel.clone().unwrap_or_default(),
        "channel_id" => info.channel_id.clone().unwrap_or_default(),
        "channel_url" => info.channel_url.clone().unwrap_or_default(),
        "duration" => info.duration.map(|d| format!("{d}")).unwrap_or_default(),
        "view_count" => info.view_count.map(|v| v.to_string()).unwrap_or_default(),
        "like_count" => info.like_count.map(|v| v.to_string()).unwrap_or_default(),
        "comment_count" => info
            .comment_count
            .map(|v| v.to_string())
            .unwrap_or_default(),
        "upload_date" => info.upload_date.clone().unwrap_or_default(),
        "timestamp" => info.timestamp.map(|t| t.to_string()).unwrap_or_default(),
        "age_limit" => info.age_limit.map(|a| a.to_string()).unwrap_or_default(),
        "is_live" => info.is_live.map(|b| b.to_string()).unwrap_or_default(),
        "was_live" => info.was_live.map(|b| b.to_string()).unwrap_or_default(),
        "live_status" => info.live_status.clone().unwrap_or_default(),
        "playlist" => info.playlist.clone().unwrap_or_default(),
        "playlist_id" => info.playlist_id.clone().unwrap_or_default(),
        "playlist_title" => info.playlist_title.clone().unwrap_or_default(),
        "playlist_index" => info
            .playlist_index
            .map(|i| i.to_string())
            .unwrap_or_default(),
        "n_entries" => info.n_entries.map(|n| n.to_string()).unwrap_or_default(),
        "extractor" => info.extractor.clone(),
        "extractor_key" => info.extractor_key.clone(),
        "thumbnail" => info.thumbnail.clone().unwrap_or_default(),
        "release_timestamp" => info
            .release_timestamp
            .map(|t| t.to_string())
            .unwrap_or_default(),
        // Fall back to the extra HashMap
        other => info
            .extra
            .get(other)
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default(),
    }
}

/// Apply date formatting to a value. The value may be an upload_date (YYYYMMDD)
/// or a Unix timestamp.
fn apply_date_format(value: &str, fmt: &str) -> String {
    // Try parsing as YYYYMMDD
    if value.len() == 8 {
        if let Ok(date) = NaiveDate::parse_from_str(value, "%Y%m%d") {
            return date.format(fmt).to_string();
        }
    }
    // Try parsing as Unix timestamp
    if let Ok(ts) = value.parse::<i64>() {
        if let Some(dt) = DateTime::from_timestamp(ts, 0) {
            return dt.format(fmt).to_string();
        }
    }
    // Return original if we can't parse it
    value.to_string()
}

/// Apply truncation from a format spec like `.100`.
fn apply_truncation(value: &str, spec: &str) -> String {
    if spec.is_empty() {
        return value.to_string();
    }
    // Parse ".N" where N is the max length
    let spec = spec.strip_prefix('.').unwrap_or(spec);
    if let Ok(max_len) = spec.parse::<usize>() {
        if value.len() > max_len {
            return value.chars().take(max_len).collect();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_info() -> InfoDict {
        InfoDict {
            id: "dQw4w9WgXcQ".to_string(),
            title: Some("Never Gonna Give You Up".to_string()),
            fulltitle: None,
            ext: "mp4".to_string(),
            url: None,
            webpage_url: Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string()),
            original_url: None,
            display_id: None,
            description: None,
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
    fn test_basic_template() {
        let info = make_info();
        let result = render_template("%(title)s [%(id)s].%(ext)s", &info).unwrap();
        assert_eq!(result, "Never Gonna Give You Up [dQw4w9WgXcQ].mp4");
    }

    #[test]
    fn test_truncation() {
        let info = make_info();
        let result = render_template("%(title).10s.%(ext)s", &info).unwrap();
        assert_eq!(result, "Never Gonn.mp4");
    }

    #[test]
    fn test_default_value() {
        let info = make_info();
        let result = render_template("%(playlist|no_playlist)s - %(title)s", &info).unwrap();
        assert_eq!(result, "no_playlist - Never Gonna Give You Up");
    }

    #[test]
    fn test_date_formatting() {
        let info = make_info();
        let result = render_template("%(upload_date>%Y-%m-%d)s", &info).unwrap();
        assert_eq!(result, "2009-10-25");
    }

    #[test]
    fn test_date_year_only() {
        let info = make_info();
        let result = render_template("%(upload_date>%Y)s", &info).unwrap();
        assert_eq!(result, "2009");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("a/b:c*d"), "a_b_c_d");
        assert_eq!(sanitize_filename("file..."), "file");
        assert_eq!(sanitize_filename("normal name"), "normal name");
    }
}
