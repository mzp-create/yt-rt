//! Fragment-based download manager for HLS/DASH segment downloading.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use yt_dlp_core::progress::{DownloadProgress, ProgressReporter};
use yt_dlp_core::types::Fragment;
use yt_dlp_networking::client::HttpClient;

use crate::retry::{with_retry, RetryConfig};
use crate::DownloadOptions;

/// Downloads a list of fragments concurrently and concatenates them into a single output file.
pub struct FragmentDownloader {
    client: Arc<HttpClient>,
    options: DownloadOptions,
}

impl FragmentDownloader {
    pub fn new(client: Arc<HttpClient>, options: DownloadOptions) -> Self {
        Self { client, options }
    }

    /// Download all `fragments` and concatenate them into `output_path`.
    ///
    /// Fragments are downloaded concurrently (limited by `options.concurrent_fragments`)
    /// and each fragment is retried according to `options.fragment_retries`.
    pub async fn download_fragments(
        &self,
        fragments: &[Fragment],
        output_path: &Path,
        progress: &dyn ProgressReporter,
    ) -> anyhow::Result<()> {
        let temp_dir = output_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                ".{}.fragments",
                output_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("download")
            ));

        tokio::fs::create_dir_all(&temp_dir)
            .await
            .context("failed to create fragment temp dir")?;

        let filename = output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download")
            .to_string();

        let fragment_count = fragments.len() as u32;
        let semaphore = Arc::new(Semaphore::new(self.options.concurrent_fragments as usize));

        let retry_config = RetryConfig {
            max_retries: self.options.fragment_retries,
            ..RetryConfig::default_fragment()
        };

        // Track total downloaded bytes across all fragments
        let total_downloaded = Arc::new(std::sync::atomic::AtomicU64::new(0));

        // Estimate total size from fragments that have filesize info
        let total_bytes_estimate: Option<u64> = {
            let known: u64 = fragments.iter().filter_map(|f| f.filesize).sum();
            if known > 0 {
                Some(known)
            } else {
                None
            }
        };

        // Download fragments concurrently
        let mut tasks = FuturesUnordered::new();

        for (index, fragment) in fragments.iter().enumerate() {
            let client = self.client.clone();
            let semaphore = semaphore.clone();
            let retry_config = retry_config.clone();
            let temp_dir = temp_dir.clone();
            let total_downloaded = total_downloaded.clone();
            let fragment_url = fragment.url.clone();

            tasks.push(async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .context("semaphore closed")?;

                let frag_path = temp_dir.join(format!("fragment_{index:06}"));

                let url = fragment_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("fragment {index} has no URL"))?;

                let url = url.to_string();
                let client = client.clone();
                let frag_path_inner = frag_path.clone();

                with_retry(&retry_config, || {
                    let client = client.clone();
                    let url = url.clone();
                    let frag_path = frag_path_inner.clone();

                    async move {
                        let response = client.get(&url).await?;

                        if !response.status().is_success() {
                            anyhow::bail!("HTTP {} for fragment", response.status());
                        }

                        let bytes = response.bytes().await?;
                        tokio::fs::write(&frag_path, &bytes)
                            .await
                            .context("failed to write fragment file")?;

                        Ok(bytes.len() as u64)
                    }
                })
                .await
                .map(|bytes_written| {
                    total_downloaded
                        .fetch_add(bytes_written, std::sync::atomic::Ordering::Relaxed);
                    (index, frag_path)
                })
            });
        }

        // Collect results, reporting progress as fragments finish
        let mut completed_fragments: Vec<(usize, PathBuf)> = Vec::with_capacity(fragments.len());

        while let Some(result) = tasks.next().await {
            let (index, frag_path) = result?;
            completed_fragments.push((index, frag_path));

            let downloaded = total_downloaded.load(std::sync::atomic::Ordering::Relaxed);
            progress.report_download_progress(&DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes: total_bytes_estimate,
                speed: None,
                eta: None,
                fragment_index: Some(completed_fragments.len() as u32),
                fragment_count: Some(fragment_count),
                filename: filename.clone(),
            });
        }

        // Sort by original index to concatenate in order
        completed_fragments.sort_by_key(|(idx, _)| *idx);

        // Concatenate all fragments into the output file
        let mut output_file = tokio::fs::File::create(output_path)
            .await
            .context("failed to create output file")?;

        for (_index, frag_path) in &completed_fragments {
            let data = tokio::fs::read(frag_path)
                .await
                .context("failed to read fragment file")?;
            output_file
                .write_all(&data)
                .await
                .context("failed to write to output file")?;
        }

        output_file.flush().await?;
        drop(output_file);

        // Clean up temp directory
        if let Err(e) = tokio::fs::remove_dir_all(&temp_dir).await {
            warn!(dir = %temp_dir.display(), "failed to clean up fragment temp dir: {e}");
        }

        progress.finish();
        info!(path = %output_path.display(), fragments = fragments.len(), "fragment download complete");

        Ok(())
    }
}
