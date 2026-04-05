use std::path::Path;
use std::sync::Arc;

use tracing::{debug, info};

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Embeds a thumbnail image into the media file as cover art.
pub struct ThumbnailEmbedPP {
    ffmpeg: Arc<FFmpeg>,
}

impl ThumbnailEmbedPP {
    pub fn new(ffmpeg: Arc<FFmpeg>) -> Self {
        Self { ffmpeg }
    }
}

impl PostProcessor for ThumbnailEmbedPP {
    fn name(&self) -> &str {
        "embed_thumbnail"
    }

    fn run(
        &self,
        _info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let parent = filepath.parent().unwrap_or(Path::new("."));
        let stem = filepath
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        // Look for a thumbnail file next to the media.
        let thumb_extensions = ["jpg", "jpeg", "png", "webp"];
        let thumb_path = thumb_extensions
            .iter()
            .map(|ext| parent.join(format!("{stem}.{ext}")))
            .find(|p| p.exists());

        let thumb_path = match thumb_path {
            Some(p) => p,
            None => {
                debug!("no thumbnail file found to embed");
                return Ok(PostProcessorResult {
                    filepath: filepath.to_path_buf(),
                    info_modified: false,
                });
            }
        };

        let tmp_path = filepath.with_extension("tmp.thumb");
        let ffmpeg = self.ffmpeg.clone();

        tokio::runtime::Handle::current().block_on(async {
            ffmpeg
                .embed_thumbnail(filepath, &thumb_path, &tmp_path)
                .await
        })?;

        std::fs::rename(&tmp_path, filepath)?;
        info!(thumbnail = %thumb_path.display(), "embedded thumbnail");

        Ok(PostProcessorResult {
            filepath: filepath.to_path_buf(),
            info_modified: false,
        })
    }
}
