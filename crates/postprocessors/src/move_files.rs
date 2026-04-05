use std::path::{Path, PathBuf};

use anyhow::Context;
use tracing::info;

use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Moves the finished file to a target directory.
pub struct MoveFilesPP {
    target_dir: PathBuf,
}

impl MoveFilesPP {
    pub fn new(target_dir: PathBuf) -> Self {
        Self { target_dir }
    }
}

impl PostProcessor for MoveFilesPP {
    fn name(&self) -> &str {
        "move_files"
    }

    fn run(
        &self,
        _info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        std::fs::create_dir_all(&self.target_dir)
            .with_context(|| format!("failed to create target dir {}", self.target_dir.display()))?;

        let filename = filepath
            .file_name()
            .context("filepath has no filename")?;
        let dest = self.target_dir.join(filename);

        std::fs::rename(filepath, &dest).or_else(|_| {
            // rename can fail across filesystems -- fall back to copy + delete.
            std::fs::copy(filepath, &dest)?;
            std::fs::remove_file(filepath)?;
            Ok::<(), std::io::Error>(())
        })?;

        info!(from = %filepath.display(), to = %dest.display(), "moved file");

        Ok(PostProcessorResult {
            filepath: dest,
            info_modified: false,
        })
    }
}
