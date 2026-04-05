use crate::types::InfoDict;
use regex::Regex;

/// Result of applying filters to a video.
#[derive(Debug, PartialEq)]
pub enum FilterResult {
    /// The video passes all filters.
    Accept,
    /// The video should be skipped, with a reason.
    Reject(String),
    /// Stop processing further videos (for --break-match-filters / --break-on-existing).
    Break(String),
}

/// Configuration for all video filters.
#[derive(Debug, Clone, Default)]
pub struct FilterConfig {
    pub match_title: Option<String>,
    pub reject_title: Option<String>,
    pub age_limit: Option<u8>,
    pub date: Option<String>,
    pub datebefore: Option<String>,
    pub dateafter: Option<String>,
    pub match_filters: Vec<String>,
    pub break_match_filters: Vec<String>,
    pub min_filesize: Option<u64>,
    pub max_filesize: Option<u64>,
}

/// Apply all configured filters to an InfoDict.
pub fn apply_filters(info: &InfoDict, filters: &FilterConfig) -> FilterResult {
    // --match-title
    if let Some(ref pattern) = filters.match_title {
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
            if let Some(title) = &info.title {
                if !re.is_match(title) {
                    return FilterResult::Reject(format!(
                        "title '{}' doesn't match pattern: {pattern}",
                        title
                    ));
                }
            }
        }
    }

    // --reject-title
    if let Some(ref pattern) = filters.reject_title {
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
            if let Some(title) = &info.title {
                if re.is_match(title) {
                    return FilterResult::Reject(format!(
                        "title '{}' matches reject pattern: {pattern}",
                        title
                    ));
                }
            }
        }
    }

    // --age-limit
    if let Some(limit) = filters.age_limit {
        if let Some(age) = info.age_limit {
            if age > limit {
                return FilterResult::Reject(format!("age limit {age} exceeds {limit}"));
            }
        }
    }

    // --date, --datebefore, --dateafter
    if let Some(ref date) = info.upload_date {
        if let Some(ref target) = filters.date {
            if date != target {
                return FilterResult::Reject(format!("date {date} != {target}"));
            }
        }
        if let Some(ref before) = filters.datebefore {
            if date.as_str() > before.as_str() {
                return FilterResult::Reject(format!("date {date} after {before}"));
            }
        }
        if let Some(ref after) = filters.dateafter {
            if date.as_str() < after.as_str() {
                return FilterResult::Reject(format!("date {date} before {after}"));
            }
        }
    }

    // --min-filesize / --max-filesize (check against format filesizes)
    if filters.min_filesize.is_some() || filters.max_filesize.is_some() {
        // Use the first format's filesize as a proxy, or the sum of requested formats
        let total_size: Option<u64> = if let Some(ref requested) = info.requested_formats {
            let sizes: Vec<u64> = requested
                .iter()
                .filter_map(|f| f.filesize.or(f.filesize_approx))
                .collect();
            if sizes.is_empty() {
                None
            } else {
                Some(sizes.iter().sum())
            }
        } else {
            info.formats
                .first()
                .and_then(|f| f.filesize.or(f.filesize_approx))
        };

        if let Some(size) = total_size {
            if let Some(min) = filters.min_filesize {
                if size < min {
                    return FilterResult::Reject(format!(
                        "filesize {size} is smaller than minimum {min}"
                    ));
                }
            }
            if let Some(max) = filters.max_filesize {
                if size > max {
                    return FilterResult::Reject(format!(
                        "filesize {size} is larger than maximum {max}"
                    ));
                }
            }
        }
    }

    // --match-filter (generic field comparison)
    for filter_expr in &filters.match_filters {
        if !evaluate_match_filter(info, filter_expr) {
            return FilterResult::Reject(format!("doesn't match filter: {filter_expr}"));
        }
    }

    // --break-match-filters
    for filter_expr in &filters.break_match_filters {
        if evaluate_match_filter(info, filter_expr) {
            return FilterResult::Break(format!("matches break filter: {filter_expr}"));
        }
    }

    FilterResult::Accept
}

/// Parse a human-readable file size string like "50k", "44.6M", "1G" into bytes.
pub fn parse_filesize(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_part, suffix) = if s.ends_with(|c: char| c.is_ascii_alphabetic()) {
        let idx = s.len() - 1;
        // Handle two-char suffixes like "KB", "MB", etc.
        let (num, suf) = if s.len() >= 2
            && s.as_bytes()[s.len() - 2].is_ascii_alphabetic()
        {
            (&s[..s.len() - 2], &s[s.len() - 2..])
        } else {
            (&s[..idx], &s[idx..])
        };
        (num.trim(), suf)
    } else {
        (s, "")
    };

    let num: f64 = num_part.parse().ok()?;
    let multiplier: f64 = match suffix.to_uppercase().as_str() {
        "" => 1.0,
        "B" => 1.0,
        "K" | "KB" => 1024.0,
        "M" | "MB" => 1024.0 * 1024.0,
        "G" | "GB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((num * multiplier) as u64)
}

/// Evaluate a single match filter expression.
///
/// Supports expressions like:
/// - `"duration > 60"` — numeric comparison
/// - `"like_count >= 100"` — numeric comparison
/// - `"is_live"` — boolean field must be true
/// - `"!is_live"` — boolean field must be false/absent
///
/// Uses serde_json to get field values dynamically from the InfoDict.
fn evaluate_match_filter(info: &InfoDict, expr: &str) -> bool {
    let expr = expr.trim();

    // Boolean checks: "is_live" or "!is_live"
    if !expr.contains('>') && !expr.contains('<') && !expr.contains('=') && !expr.contains('!') {
        return get_field_bool(info, expr);
    }
    if expr.starts_with('!') && !expr.contains('>') && !expr.contains('<') && !expr.contains('=')
    {
        return !get_field_bool(info, &expr[1..]);
    }

    // Parse "field OP value" expressions
    // Supported operators: >, <, >=, <=, ==, !=
    static OPS: &[&str] = &[">=", "<=", "!=", "==", ">", "<"];

    for op in OPS {
        if let Some(idx) = expr.find(op) {
            let field = expr[..idx].trim();
            let value_str = expr[idx + op.len()..].trim();
            return evaluate_comparison(info, field, op, value_str);
        }
    }

    // Couldn't parse — treat as passing
    true
}

fn evaluate_comparison(info: &InfoDict, field: &str, op: &str, value_str: &str) -> bool {
    let json_val = get_field_value(info, field);

    match json_val {
        Some(serde_json::Value::Number(n)) => {
            let field_num = n.as_f64().unwrap_or(0.0);
            if let Ok(target) = value_str.parse::<f64>() {
                match op {
                    ">" => field_num > target,
                    "<" => field_num < target,
                    ">=" => field_num >= target,
                    "<=" => field_num <= target,
                    "==" => (field_num - target).abs() < f64::EPSILON,
                    "!=" => (field_num - target).abs() >= f64::EPSILON,
                    _ => true,
                }
            } else {
                false
            }
        }
        Some(serde_json::Value::String(s)) => match op {
            "==" => s == value_str,
            "!=" => s != value_str,
            ">" => s.as_str() > value_str,
            "<" => s.as_str() < value_str,
            ">=" => s.as_str() >= value_str,
            "<=" => s.as_str() <= value_str,
            _ => true,
        },
        Some(serde_json::Value::Null) | None => {
            // Field is absent or null; comparisons against absent fields fail
            // except for != which succeeds (field is "not equal" to any value)
            op == "!="
        }
        _ => true,
    }
}

/// Get a JSON value for a named field from InfoDict via serde_json serialization.
fn get_field_value(info: &InfoDict, field: &str) -> Option<serde_json::Value> {
    // Serialize the whole InfoDict to a JSON Value, then look up the field.
    // This is not the most efficient approach, but it's simple and correct.
    let value = serde_json::to_value(info).ok()?;
    if let serde_json::Value::Object(map) = value {
        map.get(field).cloned()
    } else {
        None
    }
}

/// Check if a boolean field is true.
fn get_field_bool(info: &InfoDict, field: &str) -> bool {
    match get_field_value(info, field) {
        Some(serde_json::Value::Bool(b)) => b,
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::InfoDict;

    fn make_info(title: &str, duration: f64, upload_date: &str) -> InfoDict {
        InfoDict {
            id: "test123".to_string(),
            title: Some(title.to_string()),
            ext: "mp4".to_string(),
            duration: Some(duration),
            upload_date: Some(upload_date.to_string()),
            extractor: "TestExtractor".to_string(),
            extractor_key: "Test".to_string(),
            ..default_info()
        }
    }

    fn default_info() -> InfoDict {
        InfoDict {
            id: String::new(),
            title: None,
            fulltitle: None,
            ext: "mp4".to_string(),
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
            categories: vec![],
            tags: vec![],
            is_live: None,
            was_live: None,
            live_status: None,
            release_timestamp: None,
            formats: vec![],
            requested_formats: None,
            subtitles: Default::default(),
            automatic_captions: Default::default(),
            thumbnails: vec![],
            thumbnail: None,
            chapters: vec![],
            playlist: None,
            playlist_id: None,
            playlist_title: None,
            playlist_index: None,
            n_entries: None,
            extractor: String::new(),
            extractor_key: String::new(),
            extra: Default::default(),
        }
    }

    #[test]
    fn test_accept_when_no_filters() {
        let info = make_info("Test Video", 120.0, "20240101");
        let filters = FilterConfig::default();
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);
    }

    #[test]
    fn test_match_title_accepts() {
        let info = make_info("My Cool Video", 120.0, "20240101");
        let filters = FilterConfig {
            match_title: Some("cool".to_string()),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);
    }

    #[test]
    fn test_match_title_rejects() {
        let info = make_info("My Cool Video", 120.0, "20240101");
        let filters = FilterConfig {
            match_title: Some("boring".to_string()),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }

    #[test]
    fn test_reject_title() {
        let info = make_info("Boring Lecture Part 5", 120.0, "20240101");
        let filters = FilterConfig {
            reject_title: Some("boring".to_string()),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }

    #[test]
    fn test_reject_title_no_match_accepts() {
        let info = make_info("Fun Tutorial", 120.0, "20240101");
        let filters = FilterConfig {
            reject_title: Some("boring".to_string()),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);
    }

    #[test]
    fn test_age_limit_accepts() {
        let mut info = make_info("Test", 120.0, "20240101");
        info.age_limit = Some(13);
        let filters = FilterConfig {
            age_limit: Some(18),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);
    }

    #[test]
    fn test_age_limit_rejects() {
        let mut info = make_info("Test", 120.0, "20240101");
        info.age_limit = Some(18);
        let filters = FilterConfig {
            age_limit: Some(13),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }

    #[test]
    fn test_date_exact_match() {
        let info = make_info("Test", 120.0, "20240115");
        let filters = FilterConfig {
            date: Some("20240115".to_string()),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);
    }

    #[test]
    fn test_date_exact_mismatch() {
        let info = make_info("Test", 120.0, "20240115");
        let filters = FilterConfig {
            date: Some("20240120".to_string()),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }

    #[test]
    fn test_datebefore() {
        let info = make_info("Test", 120.0, "20240115");
        let accept = FilterConfig {
            datebefore: Some("20240120".to_string()),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &accept), FilterResult::Accept);

        let reject = FilterConfig {
            datebefore: Some("20240110".to_string()),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &reject), FilterResult::Reject(_)));
    }

    #[test]
    fn test_dateafter() {
        let info = make_info("Test", 120.0, "20240115");
        let accept = FilterConfig {
            dateafter: Some("20240110".to_string()),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &accept), FilterResult::Accept);

        let reject = FilterConfig {
            dateafter: Some("20240120".to_string()),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &reject), FilterResult::Reject(_)));
    }

    #[test]
    fn test_match_filter_duration() {
        let info = make_info("Test", 120.0, "20240101");

        let filters = FilterConfig {
            match_filters: vec!["duration > 60".to_string()],
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);

        let filters = FilterConfig {
            match_filters: vec!["duration > 200".to_string()],
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }

    #[test]
    fn test_match_filter_comparison_operators() {
        let mut info = make_info("Test", 120.0, "20240101");
        info.view_count = Some(1000);

        // >=
        let filters = FilterConfig {
            match_filters: vec!["view_count >= 1000".to_string()],
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);

        // <=
        let filters = FilterConfig {
            match_filters: vec!["view_count <= 999".to_string()],
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));

        // ==
        let filters = FilterConfig {
            match_filters: vec!["view_count == 1000".to_string()],
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);

        // !=
        let filters = FilterConfig {
            match_filters: vec!["view_count != 1000".to_string()],
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }

    #[test]
    fn test_break_match_filter() {
        let info = make_info("Test", 120.0, "20240101");
        let filters = FilterConfig {
            break_match_filters: vec!["duration > 60".to_string()],
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Break(_)));
    }

    #[test]
    fn test_boolean_filter() {
        let mut info = make_info("Test", 120.0, "20240101");
        info.is_live = Some(true);

        let filters = FilterConfig {
            match_filters: vec!["is_live".to_string()],
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);

        let filters = FilterConfig {
            match_filters: vec!["!is_live".to_string()],
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }

    #[test]
    fn test_parse_filesize() {
        assert_eq!(parse_filesize("1024"), Some(1024));
        assert_eq!(parse_filesize("50k"), Some(50 * 1024));
        assert_eq!(parse_filesize("50K"), Some(50 * 1024));
        assert_eq!(parse_filesize("10M"), Some(10 * 1024 * 1024));
        assert_eq!(parse_filesize("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_filesize("1.5M"), Some((1.5 * 1024.0 * 1024.0) as u64));
        assert_eq!(parse_filesize(""), None);
        assert_eq!(parse_filesize("abc"), None);
    }

    #[test]
    fn test_min_max_filesize() {
        use crate::types::Format;

        let mut info = make_info("Test", 120.0, "20240101");
        info.formats = vec![Format {
            format_id: "best".to_string(),
            filesize: Some(10 * 1024 * 1024), // 10 MiB
            ..Default::default()
        }];

        // min_filesize: 5M -> accept (10M > 5M)
        let filters = FilterConfig {
            min_filesize: Some(5 * 1024 * 1024),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);

        // min_filesize: 20M -> reject (10M < 20M)
        let filters = FilterConfig {
            min_filesize: Some(20 * 1024 * 1024),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));

        // max_filesize: 20M -> accept (10M < 20M)
        let filters = FilterConfig {
            max_filesize: Some(20 * 1024 * 1024),
            ..Default::default()
        };
        assert_eq!(apply_filters(&info, &filters), FilterResult::Accept);

        // max_filesize: 5M -> reject (10M > 5M)
        let filters = FilterConfig {
            max_filesize: Some(5 * 1024 * 1024),
            ..Default::default()
        };
        assert!(matches!(apply_filters(&info, &filters), FilterResult::Reject(_)));
    }
}
