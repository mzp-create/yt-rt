use std::path::Path;
use std::sync::Arc;

use tracing::info;

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Extracts audio from a downloaded media file (`-x` / `--extract-audio`).
pub struct AudioExtractPP {
    ffmpeg: Arc<FFmpeg>,
    audio_format: String,
    audio_quality: Option<String>,
}

impl AudioExtractPP {
    pub fn new(ffmpeg: Arc<FFmpeg>, audio_format: String, audio_quality: Option<String>) -> Self {
        Self {
            ffmpeg,
            audio_format,
            audio_quality,
        }
    }
}

impl PostProcessor for AudioExtractPP {
    fn name(&self) -> &str {
        "extract_audio"
    }

    fn run(
        &self,
        _info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let output_path = filepath.with_extension(&self.audio_format);
        let quality = self.audio_quality.as_deref();
        let ffmpeg = self.ffmpeg.clone();
        let fmt = self.audio_format.clone();

        tokio::runtime::Handle::current().block_on(async {
            ffmpeg
                .extract_audio(filepath, &output_path, &fmt, quality)
                .await
        })?;

        // Remove the original video file if it differs from the output.
        if filepath != output_path {
            let _ = std::fs::remove_file(filepath);
        }

        info!(output = %output_path.display(), format = %self.audio_format, "audio extracted");
        Ok(PostProcessorResult {
            filepath: output_path,
            info_modified: false,
        })
    }
}
