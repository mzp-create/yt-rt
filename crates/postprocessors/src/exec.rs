use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context};
use tracing::info;

use crate::{PostProcessor, PostProcessorResult};
use yt_dlp_core::types::InfoDict;

/// Runs an arbitrary shell command after downloading, with `{}` replaced by the
/// file path.
pub struct ExecPP {
    command_template: String,
}

impl ExecPP {
    pub fn new(command_template: String) -> Self {
        Self { command_template }
    }
}

impl PostProcessor for ExecPP {
    fn name(&self) -> &str {
        "exec"
    }

    fn run(
        &self,
        _info: &InfoDict,
        filepath: &Path,
    ) -> anyhow::Result<PostProcessorResult> {
        let filepath_str = filepath
            .to_str()
            .context("filepath is not valid UTF-8")?;

        let cmd = self.command_template.replace("{}", filepath_str);
        info!(cmd = %cmd, "executing post-download command");

        let status = if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", &cmd]).status()
        } else {
            Command::new("sh").args(["-c", &cmd]).status()
        }
        .context("failed to execute command")?;

        if !status.success() {
            bail!(
                "exec command failed with exit code: {}",
                status.code().unwrap_or(-1)
            );
        }

        Ok(PostProcessorResult {
            filepath: filepath.to_path_buf(),
            info_modified: false,
        })
    }
}
