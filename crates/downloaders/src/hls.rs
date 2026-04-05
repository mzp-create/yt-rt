//! HLS (HTTP Live Streaming) downloader.
//!
//! Downloads m3u8 playlists, resolves segments, handles AES-128 encryption,
//! byte-range requests, and concurrent fragment downloading.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use m3u8_rs::{KeyMethod, MediaSegment, Playlist};
use reqwest::Method;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use url::Url;

use yt_dlp_core::progress::{DownloadProgress, ProgressReporter};
use yt_dlp_core::types::Format;
use yt_dlp_networking::client::HttpClient;

/// Downloads media delivered via the HLS (m3u8) protocol.
pub struct HlsDownloader {
    client: Arc<HttpClient>,
    concurrent_fragments: usize,
    fragment_retries: u32,
}

/// Describes the encryption state for a run of segments.
#[derive(Clone)]
struct EncryptionInfo {
    key: Vec<u8>,
    iv: [u8; 16],
}

/// A resolved segment ready for download.
struct ResolvedSegment {
    index: usize,
    url: String,
    byte_range: Option<(u64, u64)>, // (offset, length)
    encryption: Option<EncryptionInfo>,
}

impl HlsDownloader {
    /// Create a new HLS downloader.
    ///
    /// * `client` - Shared HTTP client.
    /// * `concurrent_fragments` - Maximum number of segments downloaded in parallel.
    /// * `fragment_retries` - How many times to retry a failed segment download.
    pub fn new(
        client: Arc<HttpClient>,
        concurrent_fragments: usize,
        fragment_retries: u32,
    ) -> Self {
        Self {
            client,
            concurrent_fragments,
            fragment_retries,
        }
    }

    /// Download the HLS stream described by `format` into `output_path`.
    pub async fn download(
        &self,
        format: &Format,
        output_path: &Path,
        progress: &dyn ProgressReporter,
    ) -> anyhow::Result<()> {
        let manifest_url = format
            .manifest_url
            .as_deref()
            .or(format.url.as_deref())
            .context("HLS format has no manifest_url or url")?;

        tracing::info!(url = %manifest_url, "fetching HLS manifest");

        let media_playlist = self.resolve_media_playlist(manifest_url, format).await?;
        let (base_url, segments) = media_playlist;

        let total_segments = segments.len() as u32;
        tracing::info!(total_segments, "resolved HLS media playlist");

        // Resolve segments: compute full URLs, encryption info, byte-ranges.
        let resolved = self
            .resolve_segments(&base_url, &segments)
            .await
            .context("failed to resolve HLS segments")?;

        // Create a temp directory next to the output for segment files.
        let temp_dir = output_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!(
                ".hls-tmp-{}",
                output_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "out".into())
            ));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .context("failed to create HLS temp directory")?;

        let download_result = self
            .download_segments(&resolved, &temp_dir, total_segments, progress)
            .await;

        // On error, still attempt cleanup.
        if let Err(e) = &download_result {
            tracing::error!(error = %e, "HLS segment download failed");
        }

        // Concatenate segments into the final output file.
        if download_result.is_ok() {
            self.concatenate_segments(&resolved, &temp_dir, output_path)
                .await
                .context("failed to concatenate HLS segments")?;
        }

        // Cleanup temp directory (best-effort).
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

        download_result
    }

    // ── Manifest resolution ─────────────────────────────────────────────

    /// Fetches the manifest and, if it is a master playlist, selects the best-matching
    /// variant and fetches its media playlist. Returns the base URL and segments.
    async fn resolve_media_playlist(
        &self,
        manifest_url: &str,
        format: &Format,
    ) -> anyhow::Result<(Url, Vec<MediaSegment>)> {
        let manifest_text = self
            .client
            .get_text(manifest_url)
            .await
            .context("failed to fetch HLS manifest")?;

        let base = Url::parse(manifest_url).context("invalid HLS manifest URL")?;

        let playlist = m3u8_rs::parse_playlist(manifest_text.as_bytes());
        let (_, playlist) = playlist.map_err(|e| anyhow::anyhow!("m3u8 parse error: {e}"))?;

        match playlist {
            Playlist::MediaPlaylist(mp) => Ok((base, mp.segments)),
            Playlist::MasterPlaylist(master) => {
                let variant = self.select_variant(&master.variants, format);
                let variant_url = base
                    .join(&variant.uri)
                    .context("failed to resolve variant URL")?;

                tracing::debug!(
                    bandwidth = variant.bandwidth,
                    uri = %variant_url,
                    "selected HLS variant"
                );

                let variant_text = self
                    .client
                    .get_text(variant_url.as_str())
                    .await
                    .context("failed to fetch HLS variant playlist")?;

                let variant_playlist = m3u8_rs::parse_playlist(variant_text.as_bytes());
                let (_, variant_playlist) = variant_playlist
                    .map_err(|e| anyhow::anyhow!("variant m3u8 parse error: {e}"))?;

                match variant_playlist {
                    Playlist::MediaPlaylist(mp) => Ok((variant_url, mp.segments)),
                    _ => bail!("expected a media playlist from variant URL"),
                }
            }
        }
    }

    /// Pick the variant stream that best matches the format's resolution or bandwidth.
    fn select_variant<'a>(
        &self,
        variants: &'a [m3u8_rs::VariantStream],
        format: &Format,
    ) -> &'a m3u8_rs::VariantStream {
        // If the format specifies a height, try to match resolution.
        if let Some(target_height) = format.height {
            if let Some(v) = variants.iter().find(|v| {
                v.resolution
                    .as_ref()
                    .map_or(false, |r| r.height == target_height as u64)
            }) {
                return v;
            }
        }

        // If the format specifies tbr (total bitrate in kbps), match by bandwidth.
        if let Some(tbr) = format.tbr {
            let target_bw = (tbr * 1000.0) as u64;
            if let Some(v) = variants.iter().min_by_key(|v| {
                (v.bandwidth as i64 - target_bw as i64).unsigned_abs()
            }) {
                return v;
            }
        }

        // Fallback: pick the highest bandwidth variant.
        variants
            .iter()
            .max_by_key(|v| v.bandwidth)
            .expect("master playlist has no variants")
    }

    // ── Segment resolution ──────────────────────────────────────────────

    /// Resolve all segments to absolute URLs, byte-ranges, and encryption info.
    async fn resolve_segments(
        &self,
        base_url: &Url,
        segments: &[MediaSegment],
    ) -> anyhow::Result<Vec<ResolvedSegment>> {
        let mut resolved = Vec::with_capacity(segments.len());
        let mut current_encryption: Option<EncryptionInfo> = None;
        // Track the running byte offset for byte-range segments without explicit offset.
        let mut running_offset: u64 = 0;

        for (index, seg) in segments.iter().enumerate() {
            // Handle encryption key changes.
            if let Some(ref key) = seg.key {
                current_encryption = self
                    .resolve_encryption(key, base_url, index)
                    .await
                    .with_context(|| format!("failed to resolve encryption for segment {index}"))?;
            }

            let segment_url = base_url
                .join(&seg.uri)
                .with_context(|| format!("failed to resolve segment URL: {}", seg.uri))?;

            // Handle EXT-X-BYTERANGE.
            let byte_range = seg.byte_range.as_ref().map(|br| {
                let length = br.length as u64;
                let offset = br.offset.map(|o| o as u64).unwrap_or(running_offset);
                running_offset = offset + length;
                (offset, length)
            });

            resolved.push(ResolvedSegment {
                index,
                url: segment_url.to_string(),
                byte_range,
                encryption: current_encryption.clone(),
            });
        }

        Ok(resolved)
    }

    /// Resolve an EXT-X-KEY tag into an EncryptionInfo, fetching the key if needed.
    async fn resolve_encryption(
        &self,
        key: &m3u8_rs::Key,
        base_url: &Url,
        segment_index: usize,
    ) -> anyhow::Result<Option<EncryptionInfo>> {
        match key.method {
            KeyMethod::None => Ok(None),
            KeyMethod::AES128 => {
                let key_uri = key
                    .uri
                    .as_deref()
                    .context("AES-128 key has no URI")?;

                let key_url = base_url.join(key_uri).context("invalid AES-128 key URL")?;

                tracing::debug!(url = %key_url, "fetching AES-128 key");

                let key_bytes = self
                    .client
                    .get_bytes(key_url.as_str())
                    .await
                    .context("failed to download AES-128 key")?;

                if key_bytes.len() != 16 {
                    bail!(
                        "AES-128 key has unexpected length: {} (expected 16)",
                        key_bytes.len()
                    );
                }

                let mut key_arr = [0u8; 16];
                key_arr.copy_from_slice(&key_bytes);

                // IV: explicit from playlist, or default to segment sequence number.
                let iv = if let Some(ref iv_hex) = key.iv {
                    parse_hex_iv(iv_hex).context("invalid IV hex string")?
                } else {
                    let mut iv = [0u8; 16];
                    let idx_bytes = (segment_index as u128).to_be_bytes();
                    iv.copy_from_slice(&idx_bytes);
                    iv
                };

                Ok(Some(EncryptionInfo {
                    key: key_arr.to_vec(),
                    iv,
                }))
            }
            _ => {
                tracing::warn!(method = ?key.method, "unsupported HLS encryption method, treating as unencrypted");
                Ok(None)
            }
        }
    }

    // ── Downloading ─────────────────────────────────────────────────────

    /// Download all resolved segments concurrently into the temp directory.
    async fn download_segments(
        &self,
        segments: &[ResolvedSegment],
        temp_dir: &Path,
        total_segments: u32,
        progress: &dyn ProgressReporter,
    ) -> anyhow::Result<()> {
        let semaphore = Arc::new(Semaphore::new(self.concurrent_fragments));
        let downloaded_bytes = Arc::new(AtomicU64::new(0));
        let completed_segments = Arc::new(AtomicU64::new(0));
        let total_bytes_estimate = Arc::new(AtomicU64::new(0));

        let mut tasks = FuturesUnordered::new();

        for seg in segments {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .context("semaphore closed")?;

            let client = self.client.clone();
            let url = seg.url.clone();
            let byte_range = seg.byte_range;
            let encryption = seg.encryption.clone();
            let index = seg.index;
            let retries = self.fragment_retries;
            let seg_path = temp_dir.join(format!("seg_{index:06}"));
            let dl_bytes = downloaded_bytes.clone();
            let total_est = total_bytes_estimate.clone();
            let completed = completed_segments.clone();

            tasks.push(tokio::spawn(async move {
                let result = download_single_segment(
                    &client, &url, byte_range, encryption, &seg_path, retries,
                )
                .await;

                drop(permit);

                match &result {
                    Ok(bytes_written) => {
                        dl_bytes.fetch_add(*bytes_written, Ordering::Relaxed);
                        total_est.fetch_add(*bytes_written, Ordering::Relaxed);
                        completed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        tracing::error!(segment = index, error = %e, "segment download failed");
                    }
                }

                result.map(|b| (index, b))
            }));
        }

        while let Some(join_result) = tasks.next().await {
            let result = join_result.context("segment task panicked")?;
            result?;

            let current_bytes = downloaded_bytes.load(Ordering::Relaxed);
            let current_completed = completed_segments.load(Ordering::Relaxed) as u32;

            // Estimate total bytes from average segment size.
            let avg = if current_completed > 0 {
                current_bytes / current_completed as u64
            } else {
                0
            };
            let estimated_total = avg * total_segments as u64;

            progress.report_download_progress(&DownloadProgress {
                downloaded_bytes: current_bytes,
                total_bytes: if estimated_total > 0 {
                    Some(estimated_total)
                } else {
                    None
                },
                speed: None,
                eta: None,
                fragment_index: Some(current_completed),
                fragment_count: Some(total_segments),
                filename: String::new(),
            });
        }

        Ok(())
    }

    // ── Concatenation ───────────────────────────────────────────────────

    /// Concatenate downloaded segment files into the final output.
    async fn concatenate_segments(
        &self,
        segments: &[ResolvedSegment],
        temp_dir: &Path,
        output_path: &Path,
    ) -> anyhow::Result<()> {
        tracing::info!(output = %output_path.display(), "concatenating HLS segments");

        let mut out_file = tokio::fs::File::create(output_path)
            .await
            .context("failed to create output file")?;

        for seg in segments {
            let seg_path = temp_dir.join(format!("seg_{:06}", seg.index));
            let data = tokio::fs::read(&seg_path)
                .await
                .with_context(|| format!("failed to read segment file {}", seg_path.display()))?;
            out_file
                .write_all(&data)
                .await
                .context("failed to write to output file")?;
        }

        out_file.flush().await?;
        tracing::info!("HLS concatenation complete");
        Ok(())
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Download a single segment with retry logic.
async fn download_single_segment(
    client: &HttpClient,
    url: &str,
    byte_range: Option<(u64, u64)>,
    encryption: Option<EncryptionInfo>,
    output_path: &Path,
    max_retries: u32,
) -> anyhow::Result<u64> {
    let mut last_error = None;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
            tracing::debug!(attempt, delay_ms = delay.as_millis(), url, "retrying segment");
            tokio::time::sleep(delay).await;
        }

        match try_download_segment(client, url, byte_range, &encryption).await {
            Ok(data) => {
                tokio::fs::write(output_path, &data)
                    .await
                    .context("failed to write segment to temp file")?;
                return Ok(data.len() as u64);
            }
            Err(e) => {
                tracing::warn!(attempt, url, error = %e, "segment download attempt failed");
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("segment download failed with no attempts")))
}

/// Attempt to download and optionally decrypt a segment.
async fn try_download_segment(
    client: &HttpClient,
    url: &str,
    byte_range: Option<(u64, u64)>,
    encryption: &Option<EncryptionInfo>,
) -> anyhow::Result<Vec<u8>> {
    let mut req = client.request(Method::GET, url);

    if let Some((offset, length)) = byte_range {
        let end = offset + length - 1;
        req = req.header("Range", format!("bytes={offset}-{end}"));
    }

    let resp = req.send().await.context("segment HTTP request failed")?;

    if !resp.status().is_success() {
        bail!("segment returned HTTP {}", resp.status());
    }

    let data = resp.bytes().await.context("failed to read segment body")?;
    let mut data = data.to_vec();

    // Decrypt if AES-128 encryption is active.
    if let Some(enc) = &encryption {
        data = decrypt_aes128(&enc.key, &enc.iv, &data)
            .context("AES-128 decryption failed")?;
    }

    Ok(data)
}

/// Decrypt data using AES-128-CBC with PKCS7 padding.
fn decrypt_aes128(key: &[u8], iv: &[u8; 16], data: &[u8]) -> anyhow::Result<Vec<u8>> {
    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

    let decryptor = Aes128CbcDec::new_from_slices(key, iv)
        .map_err(|e| anyhow::anyhow!("AES init error: {e}"))?;

    let mut buf = data.to_vec();
    let decrypted = decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| anyhow::anyhow!("AES decrypt error: {e}"))?;

    Ok(decrypted.to_vec())
}

/// Parse a hex IV string (with or without 0x prefix) into a 16-byte array.
fn parse_hex_iv(hex_str: &str) -> anyhow::Result<[u8; 16]> {
    let hex_str = hex_str.strip_prefix("0x").or(hex_str.strip_prefix("0X")).unwrap_or(hex_str);

    if hex_str.len() != 32 {
        bail!("IV hex string has unexpected length: {} (expected 32)", hex_str.len());
    }

    let mut iv = [0u8; 16];
    for i in 0..16 {
        iv[i] = u8::from_str_radix(&hex_str[i * 2..i * 2 + 2], 16)
            .with_context(|| format!("invalid hex byte in IV at position {i}"))?;
    }

    Ok(iv)
}
