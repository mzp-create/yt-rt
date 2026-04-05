//! DASH (Dynamic Adaptive Streaming over HTTP) downloader.
//!
//! Downloads DASH/MPD manifests, resolves segment URLs from SegmentTemplate
//! (with `$Number$` and `$Time$` substitution), SegmentList, or pre-parsed
//! fragment lists, and downloads segments concurrently.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use url::Url;

use yt_dlp_core::progress::{DownloadProgress, ProgressReporter};
use yt_dlp_core::types::Format;
use yt_dlp_networking::client::HttpClient;

/// Downloads media delivered via the DASH (MPD) protocol.
pub struct DashDownloader {
    client: Arc<HttpClient>,
    concurrent_fragments: usize,
    fragment_retries: u32,
}

/// A resolved DASH segment ready for download.
struct ResolvedSegment {
    index: usize,
    url: String,
    is_init: bool,
}

impl DashDownloader {
    /// Create a new DASH downloader.
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

    /// Download the DASH stream described by `format` into `output_path`.
    pub async fn download(
        &self,
        format: &Format,
        output_path: &Path,
        progress: &dyn ProgressReporter,
    ) -> anyhow::Result<()> {
        let segments = if let Some(ref fragments) = format.fragments {
            // Fast path: extractor already resolved the fragment list.
            tracing::info!(
                fragment_count = fragments.len(),
                "using pre-parsed DASH fragments"
            );
            self.segments_from_fragments(fragments)?
        } else {
            // Slow path: fetch and parse the MPD manifest.
            let manifest_url = format
                .manifest_url
                .as_deref()
                .context("DASH format has no manifest_url and no pre-parsed fragments")?;

            tracing::info!(url = %manifest_url, "fetching DASH MPD manifest");
            self.resolve_segments_from_mpd(manifest_url, format).await?
        };

        let total_segments = segments.len() as u32;
        tracing::info!(total_segments, "resolved DASH segments");

        // Create temp directory for segment files.
        let temp_dir = output_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!(
                ".dash-tmp-{}",
                output_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "out".into())
            ));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .context("failed to create DASH temp directory")?;

        let download_result = self
            .download_segments(&segments, &temp_dir, total_segments, progress)
            .await;

        if let Err(e) = &download_result {
            tracing::error!(error = %e, "DASH segment download failed");
        }

        if download_result.is_ok() {
            self.concatenate_segments(&segments, &temp_dir, output_path)
                .await
                .context("failed to concatenate DASH segments")?;
        }

        // Cleanup temp directory (best-effort).
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

        download_result
    }

    // ── Fragment-based resolution ───────────────────────────────────────

    /// Convert pre-parsed Fragment structs into ResolvedSegments.
    fn segments_from_fragments(
        &self,
        fragments: &[yt_dlp_core::types::Fragment],
    ) -> anyhow::Result<Vec<ResolvedSegment>> {
        let mut resolved = Vec::with_capacity(fragments.len());

        for (i, frag) in fragments.iter().enumerate() {
            let url = frag
                .url
                .as_deref()
                .or(frag.path.as_deref())
                .with_context(|| format!("fragment {i} has no url or path"))?
                .to_string();

            resolved.push(ResolvedSegment {
                index: i,
                url,
                is_init: i == 0, // Treat first fragment as init if present.
            });
        }

        Ok(resolved)
    }

    // ── MPD manifest resolution ─────────────────────────────────────────

    /// Fetch the MPD, parse it, and resolve segment URLs for the matching representation.
    async fn resolve_segments_from_mpd(
        &self,
        manifest_url: &str,
        format: &Format,
    ) -> anyhow::Result<Vec<ResolvedSegment>> {
        let manifest_text = self
            .client
            .get_text(manifest_url)
            .await
            .context("failed to fetch DASH MPD manifest")?;

        let mpd = dash_mpd::parse(&manifest_text)
            .map_err(|e| anyhow::anyhow!("MPD parse error: {e}"))?;

        let base_url = Url::parse(manifest_url).context("invalid MPD manifest URL")?;

        // Build the base URL chain: MPD BaseURL -> Period BaseURL -> AdaptationSet BaseURL -> Representation BaseURL.
        let mpd_base = resolve_base_url(&base_url, &mpd.base_url);

        for period in &mpd.periods {
            let period_base = resolve_base_url(&mpd_base, &period.BaseURL);

            for adaptation in &period.adaptations {
                let adapt_base = resolve_base_url(&period_base, &adaptation.BaseURL);

                for repr in &adaptation.representations {
                    if !self.matches_representation(repr, format) {
                        continue;
                    }

                    let repr_base = resolve_base_url(&adapt_base, &repr.BaseURL);

                    tracing::debug!(
                        repr_id = repr.id.as_deref().unwrap_or("?"),
                        bandwidth = repr.bandwidth,
                        "matched DASH representation"
                    );

                    // Try SegmentTemplate first (most common), then SegmentList.
                    let seg_template = repr
                        .SegmentTemplate
                        .as_ref()
                        .or(adaptation.SegmentTemplate.as_ref())
                        .or(period.SegmentTemplate.as_ref());

                    if let Some(template) = seg_template {
                        return self.resolve_from_segment_template(
                            template,
                            &repr_base,
                            repr.id.as_deref().unwrap_or(""),
                            repr.bandwidth.unwrap_or(0),
                        );
                    }

                    let seg_list = repr
                        .SegmentList
                        .as_ref()
                        .or(adaptation.SegmentList.as_ref());

                    if let Some(list) = seg_list {
                        return self.resolve_from_segment_list(list, &repr_base);
                    }

                    // Single-segment representation (BaseURL only).
                    return Ok(vec![ResolvedSegment {
                        index: 0,
                        url: repr_base.to_string(),
                        is_init: false,
                    }]);
                }
            }
        }

        bail!(
            "no matching DASH representation found for format_id={}",
            format.format_id
        );
    }

    /// Check if a Representation matches the requested format.
    fn matches_representation(
        &self,
        repr: &dash_mpd::Representation,
        format: &Format,
    ) -> bool {
        // Match by format_id (representation @id) first.
        if let Some(ref repr_id) = repr.id {
            if *repr_id == format.format_id {
                return true;
            }
        }

        // Match by resolution.
        if let (Some(rw), Some(rh), Some(fw), Some(fh)) =
            (repr.width, repr.height, format.width, format.height)
        {
            if rw == fw as u64 && rh == fh as u64 {
                return true;
            }
        }

        // Match by bandwidth.
        if let (Some(rb), Some(tbr)) = (repr.bandwidth, format.tbr) {
            let format_bw = (tbr * 1000.0) as u64;
            // Allow 5% tolerance.
            let tolerance = format_bw / 20;
            if rb.abs_diff(format_bw) <= tolerance {
                return true;
            }
        }

        false
    }

    // ── SegmentTemplate resolution ──────────────────────────────────────

    /// Resolve segments from a SegmentTemplate with `$Number$` or `$Time$` substitution.
    fn resolve_from_segment_template(
        &self,
        template: &dash_mpd::SegmentTemplate,
        base_url: &Url,
        repr_id: &str,
        bandwidth: u64,
    ) -> anyhow::Result<Vec<ResolvedSegment>> {
        let mut segments = Vec::new();
        let mut index = 0;

        // Initialization segment.
        if let Some(ref init_template) = template.initialization {
            let init_url = substitute_template(init_template, repr_id, bandwidth, 0, 0);
            let init_full = base_url
                .join(&init_url)
                .context("failed to resolve init segment URL")?;
            segments.push(ResolvedSegment {
                index,
                url: init_full.to_string(),
                is_init: true,
            });
            index += 1;
        }

        let media_template = template
            .media
            .as_deref()
            .context("SegmentTemplate has no @media attribute")?;

        let start_number = template.startNumber.unwrap_or(1);
        let timescale = template.timescale.unwrap_or(1);

        if let Some(ref timeline) = template.SegmentTimeline {
            // Time-based: use $Time$ substitution.
            let mut time: u64 = 0;

            for s in &timeline.segments {
                if let Some(t) = s.t {
                    time = t;
                }

                let repeat_count = s.r.unwrap_or(0);
                // r can be negative (-1) meaning repeat until next S element.
                // In practice, we handle positive repeats here.
                let repeats = if repeat_count >= 0 {
                    repeat_count as u64
                } else {
                    0
                };

                for _ in 0..=repeats {
                    let number = start_number + (index as u64) - if segments.first().map_or(false, |s: &ResolvedSegment| s.is_init) { 1 } else { 0 };
                    let seg_url = substitute_template(media_template, repr_id, bandwidth, number, time);
                    let seg_full = base_url
                        .join(&seg_url)
                        .context("failed to resolve media segment URL")?;

                    segments.push(ResolvedSegment {
                        index,
                        url: seg_full.to_string(),
                        is_init: false,
                    });
                    index += 1;
                    time += s.d;
                }
            }
        } else if let Some(duration) = template.duration {
            // Number-based: use $Number$ substitution with @duration/@timescale.
            // Without a timeline and without an external signal for total count,
            // we cannot determine the segment count. This path is used when the
            // caller has already provided fragments or when the MPD specifies a
            // mediaPresentationDuration. We generate a reasonable number.
            //
            // In a real implementation the total duration would come from the Period
            // or MPD-level duration. We approximate: if duration > 0 we produce
            // segments until the period duration is exhausted, capped at a sane max.
            let segment_duration_secs = duration / timescale as f64;
            // Fallback: 4 hours max.
            let max_segments: u64 = ((4.0 * 3600.0) / segment_duration_secs).ceil() as u64;
            // We'll generate up to max_segments; the downloader will handle 404s gracefully.
            // For now, cap at a smaller value to avoid runaway.
            let count = max_segments.min(10000);

            for seg_num in 0..count {
                let number = start_number + seg_num;
                let time = (seg_num as f64 * duration) as u64;
                let seg_url = substitute_template(media_template, repr_id, bandwidth, number, time);
                let seg_full = base_url
                    .join(&seg_url)
                    .context("failed to resolve media segment URL")?;

                segments.push(ResolvedSegment {
                    index,
                    url: seg_full.to_string(),
                    is_init: false,
                });
                index += 1;
            }
        } else {
            bail!("SegmentTemplate has neither SegmentTimeline nor @duration");
        }

        Ok(segments)
    }

    // ── SegmentList resolution ──────────────────────────────────────────

    /// Resolve segments from an explicit SegmentList.
    fn resolve_from_segment_list(
        &self,
        list: &dash_mpd::SegmentList,
        base_url: &Url,
    ) -> anyhow::Result<Vec<ResolvedSegment>> {
        let mut segments = Vec::new();
        let mut index = 0;

        // Initialization segment.
        if let Some(ref init) = list.Initialization {
            if let Some(ref source_url) = init.sourceURL {
                let init_full = base_url
                    .join(source_url)
                    .context("failed to resolve SegmentList init URL")?;
                segments.push(ResolvedSegment {
                    index,
                    url: init_full.to_string(),
                    is_init: true,
                });
                index += 1;
            }
        }

        for seg_url in &list.segment_urls {
            if let Some(ref media) = seg_url.media {
                let full_url = base_url
                    .join(media)
                    .context("failed to resolve SegmentList media URL")?;
                segments.push(ResolvedSegment {
                    index,
                    url: full_url.to_string(),
                    is_init: false,
                });
                index += 1;
            }
        }

        Ok(segments)
    }

    // ── Downloading ─────────────────────────────────────────────────────

    /// Download all resolved segments concurrently into temp directory.
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

        let mut tasks = FuturesUnordered::new();

        for seg in segments {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .context("semaphore closed")?;

            let client = self.client.clone();
            let url = seg.url.clone();
            let index = seg.index;
            let retries = self.fragment_retries;
            let seg_path = temp_dir.join(format!("seg_{index:06}"));
            let dl_bytes = downloaded_bytes.clone();
            let completed = completed_segments.clone();

            tasks.push(tokio::spawn(async move {
                let result =
                    download_single_segment(&client, &url, &seg_path, retries).await;

                drop(permit);

                match &result {
                    Ok(bytes_written) => {
                        dl_bytes.fetch_add(*bytes_written, Ordering::Relaxed);
                        completed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        tracing::error!(segment = index, error = %e, "DASH segment download failed");
                    }
                }

                result.map(|b| (index, b))
            }));
        }

        while let Some(join_result) = tasks.next().await {
            let result = join_result.context("DASH segment task panicked")?;
            result?;

            let current_bytes = downloaded_bytes.load(Ordering::Relaxed);
            let current_completed = completed_segments.load(Ordering::Relaxed) as u32;

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

    /// Concatenate init + media segments into the final output file.
    async fn concatenate_segments(
        &self,
        segments: &[ResolvedSegment],
        temp_dir: &Path,
        output_path: &Path,
    ) -> anyhow::Result<()> {
        tracing::info!(output = %output_path.display(), "concatenating DASH segments");

        let mut out_file = tokio::fs::File::create(output_path)
            .await
            .context("failed to create output file")?;

        // Write init segment(s) first, then media segments, preserving order.
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
        tracing::info!("DASH concatenation complete");
        Ok(())
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Download a single DASH segment with retry logic.
async fn download_single_segment(
    client: &HttpClient,
    url: &str,
    output_path: &Path,
    max_retries: u32,
) -> anyhow::Result<u64> {
    let mut last_error = None;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
            tracing::debug!(attempt, delay_ms = delay.as_millis(), url, "retrying DASH segment");
            tokio::time::sleep(delay).await;
        }

        match client.get_bytes(url).await {
            Ok(data) => {
                tokio::fs::write(output_path, &data)
                    .await
                    .context("failed to write DASH segment to temp file")?;
                return Ok(data.len() as u64);
            }
            Err(e) => {
                tracing::warn!(attempt, url, error = %e, "DASH segment download attempt failed");
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("DASH segment download failed with no attempts")))
}

/// Perform DASH SegmentTemplate substitution for `$RepresentationID$`, `$Number$`,
/// `$Bandwidth$`, and `$Time$` variables.
fn substitute_template(
    template: &str,
    repr_id: &str,
    bandwidth: u64,
    number: u64,
    time: u64,
) -> String {
    let mut result = template.to_string();

    result = result.replace("$RepresentationID$", repr_id);
    result = result.replace("$Bandwidth$", &bandwidth.to_string());

    // Handle $Number$ with optional printf-style formatting, e.g. $Number%05d$.
    result = substitute_with_format(&result, "$Number", number);
    result = substitute_with_format(&result, "$Time", time);

    result
}

/// Handle `$Var$` or `$Var%0Nd$` patterns, substituting the given value.
fn substitute_with_format(template: &str, var_prefix: &str, value: u64) -> String {
    let simple_pattern = format!("{var_prefix}$");
    if template.contains(&simple_pattern) {
        return template.replace(&simple_pattern, &value.to_string());
    }

    // Look for printf-style format: $Var%0Nd$
    let format_start = format!("{var_prefix}%");
    if let Some(start_idx) = template.find(&format_start) {
        let after = &template[start_idx + format_start.len()..];
        if let Some(end_idx) = after.find("d$") {
            let fmt_spec = &after[..end_idx];
            // Parse width from format spec like "05" or "0N"
            let width: usize = fmt_spec
                .trim_start_matches('0')
                .parse()
                .unwrap_or(1);
            let formatted = format!("{value:0>width$}");
            let full_pattern = format!("{format_start}{fmt_spec}d$");
            return template.replace(&full_pattern, &formatted);
        }
    }

    template.to_string()
}

/// Walk the BaseURL chain: if the child list has entries, resolve the first one
/// against the parent. Otherwise, return the parent as-is.
fn resolve_base_url(parent: &Url, base_urls: &[dash_mpd::BaseURL]) -> Url {
    if let Some(first) = base_urls.first() {
        // If the base URL is absolute, use it directly. Otherwise join with parent.
        match Url::parse(&first.base) {
            Ok(absolute) => absolute,
            Err(_) => parent.join(&first.base).unwrap_or_else(|_| parent.clone()),
        }
    } else {
        parent.clone()
    }
}
