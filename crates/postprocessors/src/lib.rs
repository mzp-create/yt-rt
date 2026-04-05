pub mod ffmpeg;
pub mod merge;
pub mod audio;
pub mod remux;
pub mod metadata;
pub mod subtitles;
pub mod thumbnail;
pub mod chapters;
pub mod sponsorblock;
pub mod exec;
pub mod move_files;
pub mod info_json;

use yt_dlp_core::types::InfoDict;

use std::path::{Path, PathBuf};

// Re-export post-processor implementations for convenience.
pub use audio::AudioExtractPP;
pub use chapters::ChapterEmbedPP;
pub use exec::ExecPP;
pub use ffmpeg::FFmpeg;
pub use info_json::InfoJsonPP;
pub use merge::MergePostProcessor;
pub use metadata::MetadataEmbedPP;
pub use move_files::MoveFilesPP;
pub use remux::RemuxPP;
pub use sponsorblock::SponsorBlockPP;
pub use subtitles::SubtitleEmbedPP;
pub use thumbnail::ThumbnailEmbedPP;

/// Trait for post-processing downloaded files.
pub trait PostProcessor: Send + Sync {
    fn name(&self) -> &str;

    /// Run the post-processor. Returns the (possibly modified) output path.
    fn run(&self, info: &InfoDict, filepath: &Path) -> anyhow::Result<PostProcessorResult>;
}

/// Result of a single post-processor invocation.
pub struct PostProcessorResult {
    pub filepath: PathBuf,
    pub info_modified: bool,
}

/// Runs a chain of post-processors in sequence.
pub struct PostProcessorChain {
    processors: Vec<Box<dyn PostProcessor>>,
}

impl PostProcessorChain {
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    pub fn add(&mut self, pp: Box<dyn PostProcessor>) {
        self.processors.push(pp);
    }

    pub fn is_empty(&self) -> bool {
        self.processors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.processors.len()
    }

    /// Execute every post-processor in order, threading the filepath through.
    pub fn run_all(&self, info: &InfoDict, filepath: &Path) -> anyhow::Result<PathBuf> {
        let mut current_path = filepath.to_path_buf();
        for pp in &self.processors {
            tracing::info!("Running post-processor: {}", pp.name());
            let result = pp.run(info, &current_path)?;
            current_path = result.filepath;
        }
        Ok(current_path)
    }
}

impl Default for PostProcessorChain {
    fn default() -> Self {
        Self::new()
    }
}
