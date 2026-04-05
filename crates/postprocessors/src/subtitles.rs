use std::path::Path;
use std::sync::Arc;

use tracing::{info, debug};

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Embeds subtitle files that sit alongside the video into the container.
pub struct SubtitleEmbedPP {
    ffmpeg: Arc<FFmpeg>,
}

impl SubtitleEmbedPP {
    pub fn new(ffmpeg: Arc<FFmpeg>) -> Self {
        Self { ffmpeg }
    }
}

impl PostProcessor for SubtitleEmbedPP {
    fn name(&self) -> &str {
        "embed_subtitles"
    }

    fn run(
        &self,
        info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let parent = filepath.parent().unwrap_or(Path::new("."));
        let stem = filepath
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        // Collect subtitle files matching <stem>.<lang>.<ext>.
        let mut sub_files = Vec::new();
        let sub_extensions = ["srt", "ass", "vtt"];

        for lang in info.subtitles.keys() {
            for ext in &sub_extensions {
                let candidate = parent.join(format!("{stem}.{lang}.{ext}"));
                if candidate.exists() {
                    sub_files.push(candidate);
                }
            }
        }

        if sub_files.is_empty() {
            debug!("no subtitle files found to embed");
            return Ok(PostProcessorResult {
                filepath: filepath.to_path_buf(),
                info_modified: false,
            });
        }

        let tmp_path = filepath.with_extension("tmp.subs");
        let refs: Vec<&Path> = sub_files.iter().map(|p| p.as_path()).collect();

        let ffmpeg = self.ffmpeg.clone();
        tokio::runtime::Handle::current().block_on(async {
            ffmpeg
                .embed_subtitles(filepath, &refs, &tmp_path)
                .await
        })?;

        std::fs::rename(&tmp_path, filepath)?;
        info!(count = sub_files.len(), "embedded subtitles");

        Ok(PostProcessorResult {
            filepath: filepath.to_path_buf(),
            info_modified: false,
        })
    }
}
