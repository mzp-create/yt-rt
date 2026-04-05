//! Downloaders crate — HTTP, fragment-based, and external download support.

pub mod dash;
pub mod external;
pub mod fragment;
pub mod hls;
pub mod http;
pub mod manager;
pub mod rate_limiter;
pub mod retry;

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use yt_dlp_core::progress::ProgressReporter;
use yt_dlp_core::types::Format;
use yt_dlp_networking::client::HttpClient;

use crate::http::HttpDownloader;

/// Options that govern download behaviour (rate limiting, retries, concurrency, etc.).
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// Maximum download speed in bytes per second. `None` means unlimited.
    pub rate_limit: Option<u64>,
    /// Number of retries for a full-file download.
    pub retries: u32,
    /// Number of retries for each individual fragment.
    pub fragment_retries: u32,
    /// Maximum number of fragments to download concurrently.
    pub concurrent_fragments: u32,
    /// Size of the in-memory write buffer in bytes.
    pub buffer_size: usize,
    /// Whether to attempt resuming a partially downloaded file.
    pub resume: bool,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            rate_limit: None,
            retries: 10,
            fragment_retries: 10,
            concurrent_fragments: 1,
            buffer_size: 8 * 1024, // 8 KiB
            resume: true,
        }
    }
}

/// Trait for downloading a single format to disk.
///
/// Uses `Pin<Box<dyn Future>>` so the trait is object-safe and can be stored as
/// `dyn Downloader`.
pub trait Downloader: Send + Sync {
    fn name(&self) -> &str;

    fn can_handle(&self, format: &Format) -> bool;

    fn download<'a>(
        &'a self,
        format: &'a Format,
        output_path: &'a Path,
        progress: &'a dyn ProgressReporter,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

/// Registry that selects the appropriate downloader for a given format.
pub struct DownloaderRegistry {
    downloaders: Vec<Box<dyn Downloader>>,
}

impl DownloaderRegistry {
    /// Create a new registry pre-populated with the built-in HTTP downloader.
    pub fn new(client: Arc<HttpClient>, options: DownloadOptions) -> Self {
        let http = HttpDownloader::new(client, options);
        Self {
            downloaders: vec![Box::new(http)],
        }
    }

    /// Create an empty registry with no downloaders registered.
    pub fn empty() -> Self {
        Self {
            downloaders: Vec::new(),
        }
    }

    /// Register an additional downloader.
    pub fn register(&mut self, downloader: Box<dyn Downloader>) {
        self.downloaders.push(downloader);
    }

    /// Find the first downloader that can handle the given format.
    pub fn find_downloader(&self, format: &Format) -> Option<&dyn Downloader> {
        self.downloaders
            .iter()
            .find(|d| d.can_handle(format))
            .map(|d| d.as_ref())
    }
}
