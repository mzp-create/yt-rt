//! Download manager — high-level orchestrator that selects the right downloader
//! based on format protocol and manages the full download lifecycle.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context};
use tracing::info;

use yt_dlp_core::config::DownloadConfig;
use yt_dlp_core::progress::ProgressReporter;
use yt_dlp_core::types::{Format, InfoDict, Protocol};
use yt_dlp_networking::client::HttpClient;

use crate::dash::DashDownloader;
use crate::external::ExternalDownloader;
use crate::hls::HlsDownloader;
use crate::http::HttpDownloader;
use crate::DownloadOptions;

/// High-level download manager that dispatches to the right downloader.
pub struct DownloadManager {
    client: Arc<HttpClient>,
    options: DownloadOptions,
    external: Option<ExternalDownloader>,
}

impl DownloadManager {
    pub fn new(client: Arc<HttpClient>, config: &DownloadConfig) -> Self {
        let options = DownloadOptions {
            rate_limit: config.rate_limit,
            retries: config.retries,
            fragment_retries: config.fragment_retries,
            concurrent_fragments: config.concurrent_fragments,
            buffer_size: config.buffer_size.map(|b| b as usize).unwrap_or(8 * 1024),
            resume: true,
        };

        let external = config.external_downloader.as_ref().map(|name| {
            let args = config
                .external_downloader_args
                .get(name)
                .cloned()
                .unwrap_or_default();
            ExternalDownloader::new(name, args)
        });

        Self {
            client,
            options,
            external,
        }
    }

    /// Download a single format to the output path.
    pub async fn download_format(
        &self,
        format: &Format,
        output_path: &Path,
        progress: &dyn ProgressReporter,
    ) -> anyhow::Result<()> {
        // If an external downloader is configured, delegate to it
        if let Some(ext) = &self.external {
            if let Some(url) = &format.url {
                info!(url = url, "using external downloader");
                return ext
                    .download(url, output_path, &format.http_headers)
                    .await;
            }
        }

        match format.protocol {
            Protocol::Hls | Protocol::HlsNative => {
                info!("downloading via HLS");
                let hls = HlsDownloader::new(
                    self.client.clone(),
                    self.options.concurrent_fragments as usize,
                    self.options.fragment_retries,
                );
                hls.download(format, output_path, progress).await
            }
            Protocol::Dash => {
                info!("downloading via DASH");
                let dash = DashDownloader::new(
                    self.client.clone(),
                    self.options.concurrent_fragments as usize,
                    self.options.fragment_retries,
                );
                dash.download(format, output_path, progress).await
            }
            Protocol::Http | Protocol::Https | Protocol::Other => {
                info!("downloading via HTTP");
                let http = HttpDownloader::new(self.client.clone(), self.options.clone());
                use crate::Downloader;
                http.download(format, output_path, progress).await
            }
            ref proto => {
                bail!("unsupported download protocol: {proto}")
            }
        }
    }

    /// Download an InfoDict — handles both single formats and merged (video+audio).
    pub async fn download_info(
        &self,
        info: &InfoDict,
        output_dir: &Path,
        filename: &str,
        progress: &dyn ProgressReporter,
    ) -> anyhow::Result<PathBuf> {
        let requested = info
            .requested_formats
            .as_ref()
            .filter(|f| f.len() > 1);

        match requested {
            Some(formats) => {
                // Multiple formats to merge (e.g., bestvideo+bestaudio)
                info!(
                    count = formats.len(),
                    "downloading {} formats for merging",
                    formats.len()
                );

                let mut part_paths = Vec::new();
                for (i, fmt) in formats.iter().enumerate() {
                    let part_name = format!(
                        "{}.f{}.{}",
                        filename.rsplit_once('.').map(|(n, _)| n).unwrap_or(filename),
                        fmt.format_id,
                        fmt.ext
                    );
                    let part_path = output_dir.join(&part_name);

                    progress.report_extraction_progress(&format!(
                        "Downloading format {}/{}: {} ({})",
                        i + 1,
                        formats.len(),
                        fmt.format_id,
                        fmt.ext
                    ));

                    self.download_format(fmt, &part_path, progress)
                        .await
                        .with_context(|| format!("failed to download format {}", fmt.format_id))?;

                    part_paths.push(part_path);
                }

                let output_path = output_dir.join(filename);
                // Return the paths — merging will be handled by the post-processor (FFmpeg)
                // For now, just return the first part path
                // TODO: integrate with FFmpeg post-processor for merging
                info!(
                    parts = ?part_paths,
                    output = ?output_path,
                    "formats downloaded, merge pending"
                );
                Ok(part_paths.into_iter().next().unwrap_or(output_path))
            }
            None => {
                // Single format
                let format = info
                    .formats
                    .last()
                    .context("no formats available for download")?;
                let output_path = output_dir.join(filename);
                self.download_format(format, &output_path, progress).await?;
                Ok(output_path)
            }
        }
    }
}
