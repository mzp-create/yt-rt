use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

const API_BASE: &str = "https://sponsor.api.videosponsorblock.com";

/// Fetches SponsorBlock segment data and removes or marks sponsor segments.
pub struct SponsorBlockPP {
    ffmpeg: Arc<FFmpeg>,
    /// Categories to completely remove (e.g. "sponsor", "selfpromo").
    remove_categories: Vec<String>,
    /// Categories to mark as chapters but keep.
    mark_categories: Vec<String>,
}

impl SponsorBlockPP {
    pub fn new(
        ffmpeg: Arc<FFmpeg>,
        remove_categories: Vec<String>,
        mark_categories: Vec<String>,
    ) -> Self {
        Self {
            ffmpeg,
            remove_categories,
            mark_categories,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Segment {
    segment: (f64, f64),
    category: String,
    #[serde(rename = "actionType")]
    #[allow(dead_code)]
    action_type: Option<String>,
}

impl PostProcessor for SponsorBlockPP {
    fn name(&self) -> &str {
        "sponsorblock"
    }

    fn run(
        &self,
        info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        if self.remove_categories.is_empty() && self.mark_categories.is_empty() {
            return Ok(PostProcessorResult {
                filepath: filepath.to_path_buf(),
                info_modified: false,
            });
        }

        let video_id = &info.id;
        let all_categories: Vec<&str> = self
            .remove_categories
            .iter()
            .chain(self.mark_categories.iter())
            .map(String::as_str)
            .collect();

        let categories_param = serde_json::to_string(&all_categories)?;
        let url = format!(
            "{API_BASE}/api/skipSegments?videoID={video_id}&categories={categories_param}"
        );

        // Fetch segments synchronously via the tokio runtime.
        let segments: Vec<Segment> = tokio::runtime::Handle::current().block_on(async {
            let resp = reqwest::get(&url).await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    r.json::<Vec<Segment>>().await.unwrap_or_default()
                }
                Ok(r) => {
                    debug!(status = %r.status(), "SponsorBlock returned non-success");
                    Vec::new()
                }
                Err(e) => {
                    warn!(error = %e, "failed to query SponsorBlock API");
                    Vec::new()
                }
            }
        });

        if segments.is_empty() {
            debug!("no SponsorBlock segments found for {video_id}");
            return Ok(PostProcessorResult {
                filepath: filepath.to_path_buf(),
                info_modified: false,
            });
        }

        info!(
            count = segments.len(),
            video_id = video_id,
            "fetched SponsorBlock segments"
        );

        // For now we only support *marking* segments as chapters.
        // Full segment removal requires complex cutting which is deferred.
        let chapters: Vec<(f64, Option<f64>, String)> = segments
            .iter()
            .map(|s| {
                let label = format!("[SponsorBlock: {}]", s.category);
                (s.segment.0, Some(s.segment.1), label)
            })
            .collect();

        if !chapters.is_empty() {
            let chapter_refs: Vec<(f64, Option<f64>, &str)> = chapters
                .iter()
                .map(|(s, e, t)| (*s, *e, t.as_str()))
                .collect();

            let tmp_path = filepath.with_extension("tmp.sb");
            let ffmpeg = self.ffmpeg.clone();

            tokio::runtime::Handle::current().block_on(async {
                ffmpeg
                    .embed_chapters(filepath, &tmp_path, &chapter_refs)
                    .await
            })?;

            std::fs::rename(&tmp_path, filepath)?;
        }

        Ok(PostProcessorResult {
            filepath: filepath.to_path_buf(),
            info_modified: false,
        })
    }
}
