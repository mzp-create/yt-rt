use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::info;

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Merges separately downloaded video and audio streams into a single file.
pub struct MergePostProcessor {
    ffmpeg: Arc<FFmpeg>,
    /// Preferred output container (e.g. "mkv", "mp4"). `None` keeps the video extension.
    output_format: Option<String>,
}

impl MergePostProcessor {
    pub fn new(ffmpeg: Arc<FFmpeg>, output_format: Option<String>) -> Self {
        Self {
            ffmpeg,
            output_format,
        }
    }

    /// Locate the separate video/audio part files that the downloader left.
    ///
    /// Convention: `<stem>.fVIDEO.ext` and `<stem>.fAUDIO.ext` sit next to the
    /// final target path.
    fn find_part_files(filepath: &Path, info: &InfoDict) -> Option<(PathBuf, PathBuf)> {
        let parent = filepath.parent()?;
        let stem = filepath.file_stem()?.to_str()?;

        // Look at requested_formats for the format ids.
        let requested = info.requested_formats.as_ref()?;
        if requested.len() < 2 {
            return None;
        }

        let video_fmt = &requested[0];
        let audio_fmt = &requested[1];

        let video_path = parent.join(format!("{stem}.f{}.{}", video_fmt.format_id, video_fmt.ext));
        let audio_path = parent.join(format!("{stem}.f{}.{}", audio_fmt.format_id, audio_fmt.ext));

        if video_path.exists() && audio_path.exists() {
            Some((video_path, audio_path))
        } else {
            None
        }
    }
}

impl PostProcessor for MergePostProcessor {
    fn name(&self) -> &str {
        "merge"
    }

    fn run(
        &self,
        info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let (video_path, audio_path) = match Self::find_part_files(filepath, info) {
            Some(pair) => pair,
            None => {
                // Nothing to merge -- pass through.
                return Ok(PostProcessorResult {
                    filepath: filepath.to_path_buf(),
                    info_modified: false,
                });
            }
        };

        let output_path = filepath.to_path_buf();
        let fmt = self.output_format.as_deref();
        let ffmpeg = self.ffmpeg.clone();

        tokio::runtime::Handle::current().block_on(async {
            ffmpeg
                .merge_streams(&video_path, &audio_path, &output_path, fmt)
                .await
        })?;

        // Clean up part files.
        let _ = std::fs::remove_file(&video_path);
        let _ = std::fs::remove_file(&audio_path);
        info!(output = %output_path.display(), "merge complete");

        Ok(PostProcessorResult {
            filepath: output_path,
            info_modified: false,
        })
    }
}
