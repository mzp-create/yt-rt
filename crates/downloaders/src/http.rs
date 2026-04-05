//! HTTP/HTTPS file downloader with resume, rate limiting, and retry support.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{bail, Context};
use futures::StreamExt;
use reqwest::header;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use yt_dlp_core::progress::{DownloadProgress, ProgressReporter};
use yt_dlp_core::types::{Format, Protocol};
use yt_dlp_networking::client::HttpClient;

use crate::rate_limiter::RateLimiter;
use crate::retry::{with_retry, RetryConfig};
use crate::{DownloadOptions, Downloader};

/// Downloads files over HTTP/HTTPS with resume, rate limiting, and retry support.
pub struct HttpDownloader {
    client: Arc<HttpClient>,
    options: DownloadOptions,
}

impl HttpDownloader {
    pub fn new(client: Arc<HttpClient>, options: DownloadOptions) -> Self {
        Self { client, options }
    }

    /// The actual download implementation.
    async fn do_download(
        &self,
        format: &Format,
        output_path: &Path,
        progress: &dyn ProgressReporter,
    ) -> anyhow::Result<()> {
        let url = format
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("format has no URL"))?;

        let part_path = output_path.with_extension(
            format!(
                "{}.part",
                output_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
            ),
        );

        let filename = output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download")
            .to_string();

        let retry_config = RetryConfig {
            max_retries: self.options.retries,
            ..RetryConfig::default_download()
        };

        let client = self.client.clone();
        let options = self.options.clone();
        let url = url.to_string();
        let http_headers = format.http_headers.clone();
        let part_path_clone = part_path.clone();
        let filename_clone = filename.clone();

        with_retry(&retry_config, || {
            let client = client.clone();
            let url = url.clone();
            let http_headers = http_headers.clone();
            let part_path = part_path_clone.clone();
            let filename = filename_clone.clone();
            let buffer_size = options.buffer_size;
            let rate_limit = options.rate_limit;
            let resume = options.resume;

            async move {
                // Check existing file size for resume
                let existing_size = if resume {
                    tokio::fs::metadata(&part_path)
                        .await
                        .map(|m| m.len())
                        .unwrap_or(0)
                } else {
                    0
                };

                // Build the request
                let inner = client.inner();
                let mut req = inner.get(&url);

                // Apply format-specific headers
                for (k, v) in &http_headers {
                    req = req.header(k.as_str(), v.as_str());
                }

                // Range header for resume
                if existing_size > 0 {
                    debug!(existing_size, "resuming download");
                    req = req.header(header::RANGE, format!("bytes={existing_size}-"));
                }

                let response = req.send().await.context("HTTP request failed")?;
                let status = response.status();

                if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
                    bail!("HTTP error: {status}");
                }

                let is_partial = status == reqwest::StatusCode::PARTIAL_CONTENT;

                // Determine total size
                let content_length = response
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());

                let total_bytes = if is_partial {
                    content_length.map(|cl| cl + existing_size)
                } else {
                    content_length
                };

                // Open file for writing (append if resuming, create otherwise)
                let mut file = if is_partial && existing_size > 0 {
                    tokio::fs::OpenOptions::new()
                        .append(true)
                        .open(&part_path)
                        .await
                        .context("failed to open part file for append")?
                } else {
                    tokio::fs::File::create(&part_path)
                        .await
                        .context("failed to create part file")?
                };

                let mut downloaded = if is_partial { existing_size } else { 0u64 };

                let mut rate_limiter = rate_limit.map(RateLimiter::new);

                // Stream body in chunks
                let mut stream = response.bytes_stream();
                let mut buf = Vec::with_capacity(buffer_size);

                while let Some(chunk_result) = stream.next().await {
                    let chunk = chunk_result.context("error reading response body")?;
                    buf.extend_from_slice(&chunk);

                    // Flush when buffer is full enough
                    if buf.len() >= buffer_size {
                        file.write_all(&buf).await.context("write to part file failed")?;
                        downloaded += buf.len() as u64;
                        buf.clear();

                        // Rate limiting
                        if let Some(ref mut limiter) = rate_limiter {
                            limiter.acquire(buffer_size).await;
                        }

                        // Report progress
                        progress.report_download_progress(&DownloadProgress {
                            downloaded_bytes: downloaded,
                            total_bytes,
                            speed: None,
                            eta: None,
                            fragment_index: None,
                            fragment_count: None,
                            filename: filename.clone(),
                        });
                    }
                }

                // Flush remaining buffer
                if !buf.is_empty() {
                    file.write_all(&buf).await.context("write to part file failed")?;
                    downloaded += buf.len() as u64;

                    if let Some(ref mut limiter) = rate_limiter {
                        limiter.acquire(buf.len()).await;
                    }

                    progress.report_download_progress(&DownloadProgress {
                        downloaded_bytes: downloaded,
                        total_bytes,
                        speed: None,
                        eta: None,
                        fragment_index: None,
                        fragment_count: None,
                        filename: filename.clone(),
                    });
                }

                file.flush().await?;
                drop(file);

                Ok(())
            }
        })
        .await?;

        // Rename .part file to final destination
        tokio::fs::rename(&part_path, output_path)
            .await
            .context("failed to rename part file to final output")?;

        progress.finish();
        info!(path = %output_path.display(), "download complete");

        Ok(())
    }
}

impl Downloader for HttpDownloader {
    fn name(&self) -> &str {
        "http"
    }

    fn can_handle(&self, format: &Format) -> bool {
        matches!(format.protocol, Protocol::Http | Protocol::Https)
    }

    fn download<'a>(
        &'a self,
        format: &'a Format,
        output_path: &'a Path,
        progress: &'a dyn ProgressReporter,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(self.do_download(format, output_path, progress))
    }
}
