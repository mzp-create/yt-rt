//! Format selection engine, parsing and evaluating yt-dlp format strings
//! like `"bestvideo[height<=1080]+bestaudio/best"`.

use crate::error::{Result, YtDlpError};
use crate::types::Format;
use regex::Regex;

/// Select formats from the available list using a yt-dlp format string.
///
/// Returns the selected formats in order. When `+` (merge) is used, two formats
/// are returned (video then audio). When `/` (fallback) is used, the first
/// matching alternative is returned.
pub fn select_formats<'a>(formats: &'a [Format], format_string: &str) -> Result<Vec<&'a Format>> {
    let format_string = format_string.trim();
    if format_string.is_empty() {
        return Err(YtDlpError::FormatSelectionError(
            "empty format string".into(),
        ));
    }

    // Split on `/` for fallback alternatives
    let alternatives: Vec<&str> = format_string.split('/').collect();

    for alt in &alternatives {
        let alt = alt.trim();
        if alt.is_empty() {
            continue;
        }

        // Split on `+` for merge requests (e.g. bestvideo+bestaudio)
        let merge_parts: Vec<&str> = alt.split('+').collect();

        let mut result = Vec::new();
        let mut all_matched = true;

        for part in &merge_parts {
            let part = part.trim();
            if let Some(fmt) = select_single(formats, part)? {
                result.push(fmt);
            } else {
                all_matched = false;
                break;
            }
        }

        if all_matched && !result.is_empty() {
            return Ok(result);
        }
    }

    Err(YtDlpError::FormatSelectionError(format!(
        "no matching format found for '{format_string}'"
    )))
}

/// Parsed filter expression, e.g. `[height<=1080]`.
#[derive(Debug)]
struct Filter {
    field: String,
    op: FilterOp,
    value: String,
}

#[derive(Debug)]
enum FilterOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Select a single format specifier (keyword + optional filters).
fn select_single<'a>(formats: &'a [Format], spec: &str) -> Result<Option<&'a Format>> {
    let (keyword, filters) = parse_spec(spec)?;

    // Apply filters to narrow the candidate set
    let candidates: Vec<&Format> = formats
        .iter()
        .filter(|f| passes_all_filters(f, &filters))
        .collect();

    if candidates.is_empty() {
        return Ok(None);
    }

    match keyword.as_str() {
        "best" => Ok(Some(pick_best(&candidates, SortMode::Overall))),
        "worst" => Ok(Some(pick_worst(&candidates, SortMode::Overall))),
        "bestvideo" => {
            let video_only: Vec<&Format> = candidates
                .iter()
                .filter(|f| has_video(f) && !has_audio(f))
                .copied()
                .collect();
            let pool = if video_only.is_empty() {
                // Fall back to combined formats with video
                candidates
                    .iter()
                    .filter(|f| has_video(f))
                    .copied()
                    .collect::<Vec<_>>()
            } else {
                video_only
            };
            if pool.is_empty() {
                Ok(None)
            } else {
                Ok(Some(pick_best(&pool, SortMode::Video)))
            }
        }
        "worstvideo" => {
            let video_only: Vec<&Format> = candidates
                .iter()
                .filter(|f| has_video(f))
                .copied()
                .collect();
            if video_only.is_empty() {
                Ok(None)
            } else {
                Ok(Some(pick_worst(&video_only, SortMode::Video)))
            }
        }
        "bestaudio" => {
            let audio_only: Vec<&Format> = candidates
                .iter()
                .filter(|f| has_audio(f) && !has_video(f))
                .copied()
                .collect();
            let pool = if audio_only.is_empty() {
                candidates
                    .iter()
                    .filter(|f| has_audio(f))
                    .copied()
                    .collect::<Vec<_>>()
            } else {
                audio_only
            };
            if pool.is_empty() {
                Ok(None)
            } else {
                Ok(Some(pick_best(&pool, SortMode::Audio)))
            }
        }
        "worstaudio" => {
            let audio_only: Vec<&Format> = candidates
                .iter()
                .filter(|f| has_audio(f))
                .copied()
                .collect();
            if audio_only.is_empty() {
                Ok(None)
            } else {
                Ok(Some(pick_worst(&audio_only, SortMode::Audio)))
            }
        }
        // Treat as a literal format_id
        id => {
            let found = candidates.iter().find(|f| f.format_id == id).copied();
            Ok(found)
        }
    }
}

/// Parse a format specifier like `"bestvideo[height<=1080][ext=mp4]"` into
/// the keyword and a list of filters.
fn parse_spec(spec: &str) -> Result<(String, Vec<Filter>)> {
    let re = Regex::new(r"\[([a-zA-Z_]+)\s*(!=|<=|>=|<|>|=)\s*([^\]]+)\]").expect("valid regex");

    // Find where the first `[` is to split keyword from filters
    let keyword = spec
        .find('[')
        .map(|i| &spec[..i])
        .unwrap_or(spec)
        .trim()
        .to_string();

    let mut filters = Vec::new();
    for cap in re.captures_iter(spec) {
        let field = cap[1].to_string();
        let op = match &cap[2] {
            "=" => FilterOp::Eq,
            "!=" => FilterOp::Ne,
            "<" => FilterOp::Lt,
            "<=" => FilterOp::Le,
            ">" => FilterOp::Gt,
            ">=" => FilterOp::Ge,
            other => {
                return Err(YtDlpError::FormatSelectionError(format!(
                    "unknown filter operator: {other}"
                )));
            }
        };
        let value = cap[3].trim().to_string();
        filters.push(Filter { field, op, value });
    }

    Ok((keyword, filters))
}

/// Check whether a format passes all filter expressions.
fn passes_all_filters(format: &Format, filters: &[Filter]) -> bool {
    filters.iter().all(|f| passes_filter(format, f))
}

fn passes_filter(format: &Format, filter: &Filter) -> bool {
    let field_value = get_field_value(format, &filter.field);

    match field_value {
        FieldValue::None => {
            // If the field is absent, only `!= <value>` passes
            matches!(filter.op, FilterOp::Ne)
        }
        FieldValue::Str(s) => match filter.op {
            FilterOp::Eq => s == filter.value,
            FilterOp::Ne => s != filter.value,
            _ => {
                // Try numeric comparison for string fields
                if let (Ok(a), Ok(b)) = (s.parse::<f64>(), filter.value.parse::<f64>()) {
                    compare_f64(a, b, &filter.op)
                } else {
                    false
                }
            }
        },
        FieldValue::Num(n) => {
            if let Ok(v) = filter.value.parse::<f64>() {
                compare_f64(n, v, &filter.op)
            } else {
                // Compare as strings if the filter value is not numeric
                let s = n.to_string();
                match filter.op {
                    FilterOp::Eq => s == filter.value,
                    FilterOp::Ne => s != filter.value,
                    _ => false,
                }
            }
        }
    }
}

fn compare_f64(a: f64, b: f64, op: &FilterOp) -> bool {
    match op {
        FilterOp::Eq => (a - b).abs() < f64::EPSILON,
        FilterOp::Ne => (a - b).abs() >= f64::EPSILON,
        FilterOp::Lt => a < b,
        FilterOp::Le => a <= b,
        FilterOp::Gt => a > b,
        FilterOp::Ge => a >= b,
    }
}

enum FieldValue {
    None,
    Str(String),
    Num(f64),
}

fn get_field_value(format: &Format, field: &str) -> FieldValue {
    match field {
        "ext" => FieldValue::Str(format.ext.clone()),
        "format_id" => FieldValue::Str(format.format_id.clone()),
        "format_note" => match &format.format_note {
            Some(v) => FieldValue::Str(v.clone()),
            None => FieldValue::None,
        },
        "height" => format
            .height
            .map_or(FieldValue::None, |v| FieldValue::Num(v as f64)),
        "width" => format
            .width
            .map_or(FieldValue::None, |v| FieldValue::Num(v as f64)),
        "fps" => format.fps.map_or(FieldValue::None, FieldValue::Num),
        "vcodec" => match &format.vcodec {
            Some(v) => FieldValue::Str(v.clone()),
            None => FieldValue::Str("none".to_string()),
        },
        "acodec" => match &format.acodec {
            Some(v) => FieldValue::Str(v.clone()),
            None => FieldValue::Str("none".to_string()),
        },
        "vbr" => format.vbr.map_or(FieldValue::None, FieldValue::Num),
        "abr" => format.abr.map_or(FieldValue::None, FieldValue::Num),
        "tbr" => format.tbr.map_or(FieldValue::None, FieldValue::Num),
        "asr" => format
            .asr
            .map_or(FieldValue::None, |v| FieldValue::Num(v as f64)),
        "filesize" => format
            .filesize
            .map_or(FieldValue::None, |v| FieldValue::Num(v as f64)),
        "filesize_approx" => format
            .filesize_approx
            .map_or(FieldValue::None, |v| FieldValue::Num(v as f64)),
        "audio_channels" => format
            .audio_channels
            .map_or(FieldValue::None, |v| FieldValue::Num(v as f64)),
        "quality" => format.quality.map_or(FieldValue::None, FieldValue::Num),
        "preference" => format
            .preference
            .map_or(FieldValue::None, |v| FieldValue::Num(v as f64)),
        "language" => match &format.language {
            Some(v) => FieldValue::Str(v.clone()),
            None => FieldValue::None,
        },
        "protocol" => FieldValue::Str(format.protocol.to_string()),
        "dynamic_range" => match &format.dynamic_range {
            Some(v) => FieldValue::Str(v.clone()),
            None => FieldValue::None,
        },
        "container" => match &format.container {
            Some(v) => FieldValue::Str(v.clone()),
            None => FieldValue::None,
        },
        _ => FieldValue::None,
    }
}

fn has_video(f: &Format) -> bool {
    f.vcodec.as_deref().is_some_and(|v| v != "none") || f.height.is_some()
}

fn has_audio(f: &Format) -> bool {
    f.acodec.as_deref().is_some_and(|a| a != "none")
}

#[derive(Clone, Copy)]
enum SortMode {
    Overall,
    Video,
    Audio,
}

/// Compute a sort score for a format given the sort mode.
fn sort_score(f: &Format, mode: SortMode) -> f64 {
    match mode {
        SortMode::Overall => {
            let video = f.height.unwrap_or(0) as f64 * 1000.0
                + f.fps.unwrap_or(0.0)
                + f.vbr.unwrap_or(0.0) * 0.01;
            let audio = f.abr.unwrap_or(0.0) + f.asr.unwrap_or(0) as f64 * 0.001;
            let pref = f.preference.unwrap_or(0) as f64 * 10000.0;
            let quality = f.quality.unwrap_or(0.0) * 100.0;
            pref + quality + video + audio
        }
        SortMode::Video => {
            let pref = f.preference.unwrap_or(0) as f64 * 10000.0;
            let quality = f.quality.unwrap_or(0.0) * 100.0;
            pref + quality
                + f.height.unwrap_or(0) as f64 * 1000.0
                + f.fps.unwrap_or(0.0)
                + f.vbr.unwrap_or(0.0) * 0.01
        }
        SortMode::Audio => {
            let pref = f.preference.unwrap_or(0) as f64 * 10000.0;
            let quality = f.quality.unwrap_or(0.0) * 100.0;
            pref + quality + f.abr.unwrap_or(0.0) * 10.0 + f.asr.unwrap_or(0) as f64 * 0.01
        }
    }
}

fn pick_best<'a>(candidates: &[&'a Format], mode: SortMode) -> &'a Format {
    candidates
        .iter()
        .max_by(|a, b| {
            sort_score(a, mode)
                .partial_cmp(&sort_score(b, mode))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("candidates is non-empty")
}

fn pick_worst<'a>(candidates: &[&'a Format], mode: SortMode) -> &'a Format {
    candidates
        .iter()
        .min_by(|a, b| {
            sort_score(a, mode)
                .partial_cmp(&sort_score(b, mode))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("candidates is non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_format(
        id: &str,
        height: Option<u32>,
        vcodec: Option<&str>,
        acodec: Option<&str>,
        ext: &str,
    ) -> Format {
        Format {
            format_id: id.to_string(),
            ext: ext.to_string(),
            height,
            vcodec: vcodec.map(String::from),
            acodec: acodec.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn test_best_selects_highest_quality() {
        let formats = vec![
            make_format("360p", Some(360), Some("h264"), Some("aac"), "mp4"),
            make_format("1080p", Some(1080), Some("h264"), Some("aac"), "mp4"),
            make_format("720p", Some(720), Some("h264"), Some("aac"), "mp4"),
        ];
        let result = select_formats(&formats, "best").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].format_id, "1080p");
    }

    #[test]
    fn test_worst_selects_lowest_quality() {
        let formats = vec![
            make_format("360p", Some(360), Some("h264"), Some("aac"), "mp4"),
            make_format("1080p", Some(1080), Some("h264"), Some("aac"), "mp4"),
        ];
        let result = select_formats(&formats, "worst").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].format_id, "360p");
    }

    #[test]
    fn test_filter_height() {
        let formats = vec![
            make_format("360p", Some(360), Some("h264"), Some("aac"), "mp4"),
            make_format("1080p", Some(1080), Some("h264"), Some("aac"), "mp4"),
            make_format("4k", Some(2160), Some("h264"), Some("aac"), "mp4"),
        ];
        let result = select_formats(&formats, "best[height<=1080]").unwrap();
        assert_eq!(result[0].format_id, "1080p");
    }

    #[test]
    fn test_merge_bestvideo_bestaudio() {
        let formats = vec![
            make_format("vid", Some(1080), Some("h264"), Some("none"), "mp4"),
            make_format("aud", None, Some("none"), Some("aac"), "m4a"),
            make_format("combo", Some(720), Some("h264"), Some("aac"), "mp4"),
        ];
        let result = select_formats(&formats, "bestvideo+bestaudio").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].format_id, "vid");
        assert_eq!(result[1].format_id, "aud");
    }

    #[test]
    fn test_fallback() {
        // No formats match "bestvideo" at all (audio-only set),
        // so it falls back to "best"
        let formats = vec![make_format(
            "aud",
            None,
            Some("none"),
            Some("aac"),
            "m4a",
        )];
        let result = select_formats(&formats, "bestvideo+bestaudio/best").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].format_id, "aud");
    }

    #[test]
    fn test_format_id_literal() {
        let formats = vec![
            make_format("137", Some(1080), Some("h264"), Some("none"), "mp4"),
            make_format("140", None, Some("none"), Some("aac"), "m4a"),
        ];
        let result = select_formats(&formats, "137").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].format_id, "137");
    }

    #[test]
    fn test_ext_filter() {
        let formats = vec![
            make_format("webm", Some(1080), Some("vp9"), Some("opus"), "webm"),
            make_format("mp4", Some(1080), Some("h264"), Some("aac"), "mp4"),
        ];
        let result = select_formats(&formats, "best[ext=mp4]").unwrap();
        assert_eq!(result[0].format_id, "mp4");
    }
}
