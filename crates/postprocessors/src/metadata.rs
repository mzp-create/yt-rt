use std::path::Path;
use std::sync::Arc;

use tracing::info;

use crate::ffmpeg::FFmpeg;
use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Embeds metadata tags (title, artist, date, etc.) into the media file.
pub struct MetadataEmbedPP {
    ffmpeg: Arc<FFmpeg>,
}

impl MetadataEmbedPP {
    pub fn new(ffmpeg: Arc<FFmpeg>) -> Self {
        Self { ffmpeg }
    }
}

impl PostProcessor for MetadataEmbedPP {
    fn name(&self) -> &str {
        "embed_metadata"
    }

    fn run(
        &self,
        info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let mut pairs: Vec<(String, String)> = Vec::new();

        if let Some(ref title) = info.title {
            pairs.push(("title".into(), title.clone()));
        }
        if let Some(ref uploader) = info.uploader {
            pairs.push(("artist".into(), uploader.clone()));
        }
        if let Some(ref date) = info.upload_date {
            pairs.push(("date".into(), date.clone()));
        }
        if let Some(ref desc) = info.description {
            // Truncate very long descriptions to avoid issues.
            let truncated = if desc.len() > 4096 {
                format!("{}...", &desc[..4093])
            } else {
                desc.clone()
            };
            pairs.push(("description".into(), truncated));
            pairs.push(("comment".into(), desc.clone()));
        }
        if let Some(ref url) = info.webpage_url {
            pairs.push(("purl".into(), url.clone()));
        }
        if let Some(ref channel) = info.channel {
            pairs.push(("album_artist".into(), channel.clone()));
        }

        // Extra metadata fields (genre, label, etc.)
        for (key, value) in &info.extra {
            if let Some(s) = value.as_str() {
                if !s.is_empty() {
                    pairs.push((key.clone(), s.to_string()));
                }
            }
        }

        if pairs.is_empty() {
            return Ok(PostProcessorResult {
                filepath: filepath.to_path_buf(),
                info_modified: false,
            });
        }

        // FFmpeg cannot write metadata in-place, so write to a temp file and
        // rename.
        let tmp_path = filepath.with_extension("tmp.meta");
        let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let ffmpeg = self.ffmpeg.clone();
        tokio::runtime::Handle::current().block_on(async {
            ffmpeg.embed_metadata(filepath, &tmp_path, &refs).await
        })?;

        std::fs::rename(&tmp_path, filepath)?;
        info!(path = %filepath.display(), "embedded metadata");

        Ok(PostProcessorResult {
            filepath: filepath.to_path_buf(),
            info_modified: false,
        })
    }
}
