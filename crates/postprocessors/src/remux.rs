use std::path::Path;
use std::sync::Arc;

use tracing::info;

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Remuxes the downloaded file to a different container format without
/// re-encoding.
pub struct RemuxPP {
    ffmpeg: Arc<FFmpeg>,
    target_format: String,
}

impl RemuxPP {
    pub fn new(ffmpeg: Arc<FFmpeg>, target_format: String) -> Self {
        Self {
            ffmpeg,
            target_format,
        }
    }
}

impl PostProcessor for RemuxPP {
    fn name(&self) -> &str {
        "remux"
    }

    fn run(
        &self,
        _info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let current_ext = filepath
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if current_ext == self.target_format {
            return Ok(PostProcessorResult {
                filepath: filepath.to_path_buf(),
                info_modified: false,
            });
        }

        let output_path = filepath.with_extension(&self.target_format);
        let ffmpeg = self.ffmpeg.clone();
        let fmt = self.target_format.clone();

        tokio::runtime::Handle::current().block_on(async {
            ffmpeg.remux(filepath, &output_path, &fmt).await
        })?;

        let _ = std::fs::remove_file(filepath);
        info!(output = %output_path.display(), format = %self.target_format, "remuxed");

        Ok(PostProcessorResult {
            filepath: output_path,
            info_modified: false,
        })
    }
}
