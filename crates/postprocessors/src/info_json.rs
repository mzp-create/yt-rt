use std::path::Path;

use anyhow::Context;
use tracing::info;

use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Writes the `InfoDict` as a `.info.json` file next to the media file.
pub struct InfoJsonPP;

impl InfoJsonPP {
    pub fn new() -> Self {
        Self
    }
}

impl PostProcessor for InfoJsonPP {
    fn name(&self) -> &str {
        "write_info_json"
    }

    fn run(
        &self,
        info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let json_path = filepath.with_extension("info.json");

        let json = serde_json::to_string_pretty(info)
            .context("failed to serialise InfoDict")?;
        std::fs::write(&json_path, json)
            .with_context(|| format!("failed to write {}", json_path.display()))?;

        info!(path = %json_path.display(), "wrote info.json");

        Ok(PostProcessorResult {
            filepath: filepath.to_path_buf(),
            info_modified: false,
        })
    }
}
