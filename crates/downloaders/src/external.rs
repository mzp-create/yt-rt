//! External downloader delegation (aria2c, curl, wget).

use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;
use tracing::info;

/// Delegates downloads to an external program such as `aria2c`, `curl`, or `wget`.
pub struct ExternalDownloader {
    program: String,
    extra_args: Vec<String>,
}

impl ExternalDownloader {
    pub fn new(program: &str, extra_args: Vec<String>) -> Self {
        Self {
            program: program.to_string(),
            extra_args,
        }
    }

    /// Download `url` to `output_path`, passing the given HTTP headers.
    pub async fn download(
        &self,
        url: &str,
        output_path: &Path,
        headers: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        let output_str = output_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("output path is not valid UTF-8"))?;

        let mut cmd = match self.program.as_str() {
            "aria2c" => {
                let mut c = Command::new("aria2c");
                c.args(["-x", "16", "-s", "16", "--auto-file-renaming=false"]);
                c.args(["-o", output_str]);
                for (k, v) in headers {
                    c.args(["--header", &format!("{k}: {v}")]);
                }
                c.args(&self.extra_args);
                c.arg(url);
                c
            }
            "curl" => {
                let mut c = Command::new("curl");
                c.args(["-L", "-o", output_str]);
                for (k, v) in headers {
                    c.args(["--header", &format!("{k}: {v}")]);
                }
                c.args(&self.extra_args);
                c.arg(url);
                c
            }
            "wget" => {
                let mut c = Command::new("wget");
                c.args(["-O", output_str]);
                for (k, v) in headers {
                    c.args(["--header", &format!("{k}: {v}")]);
                }
                c.args(&self.extra_args);
                c.arg(url);
                c
            }
            other => {
                // Generic: just pass url and -o output
                let mut c = Command::new(other);
                c.args(&self.extra_args);
                c.args(["-o", output_str]);
                c.arg(url);
                c
            }
        };

        info!(program = %self.program, url = url, output = output_str, "launching external downloader");

        let status = cmd.status().await?;
        if !status.success() {
            anyhow::bail!(
                "external downloader '{}' exited with status {}",
                self.program,
                status
            );
        }
        Ok(())
    }
}
