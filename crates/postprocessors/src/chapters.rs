use std::path::Path;
use std::sync::Arc;

use tracing::{debug, info};

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Embeds chapter markers from the `InfoDict` into the media container.
pub struct ChapterEmbedPP {
    ffmpeg: Arc<FFmpeg>,
}

impl ChapterEmbedPP {
    pub fn new(ffmpeg: Arc<FFmpeg>) -> Self {
        Self { ffmpeg }
    }
}

impl PostProcessor for ChapterEmbedPP {
    fn name(&self) -> &str {
        "embed_chapters"
    }

    fn run(
        &self,
        info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        if info.chapters.is_empty() {
            debug!("no chapters to embed");
            return Ok(PostProcessorResult {
                filepath: filepath.to_path_buf(),
                info_modified: false,
            });
        }

        let chapters: Vec<(f64, Option<f64>, &str)> = info
            .chapters
            .iter()
            .map(|ch| {
                (
                    ch.start_time,
                    ch.end_time,
                    ch.title.as_deref().unwrap_or(""),
                )
            })
            .collect();

        let tmp_path = filepath.with_extension("tmp.chap");
        let ffmpeg = self.ffmpeg.clone();

        tokio::runtime::Handle::current().block_on(async {
            ffmpeg
                .embed_chapters(filepath, &tmp_path, &chapters)
                .await
        })?;

        std::fs::rename(&tmp_path, filepath)?;
        info!(count = chapters.len(), "embedded chapters");

        Ok(PostProcessorResult {
            filepath: filepath.to_path_buf(),
            info_modified: false,
        })
    }
}
