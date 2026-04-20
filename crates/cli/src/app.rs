use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tracing::{debug, info, warn};

use yt_dlp_core::archive::DownloadArchive;
use yt_dlp_core::config::Config;
use yt_dlp_core::filters::{self, FilterConfig, FilterResult};
use yt_dlp_core::format_selection::select_formats;
use yt_dlp_core::output_template::{render_template, sanitize_filename};
use yt_dlp_core::progress::{IndicatifReporter, ProgressReporter, QuietReporter};
use yt_dlp_core::types::{ExtractionResult, InfoDict, PlaylistEntry};
use yt_dlp_downloaders::manager::DownloadManager;
use yt_dlp_extractors::create_default_registry;
use yt_dlp_networking::client::HttpClient;
use yt_dlp_postprocessors::{
    AudioExtractPP, ChapterEmbedPP, ExecPP, FFmpeg, InfoJsonPP, MetadataEmbedPP,
    PostProcessorChain, RemuxPP, SponsorBlockPP, SubtitleEmbedPP, ThumbnailEmbedPP,
};

use crate::args::Cli;

/// Main application entry point. Dispatches based on CLI flags.
pub async fn run(cli: Cli) -> Result<()> {
    // 1. Initialize tracing/logging based on verbosity level.
    init_tracing(&cli);

    debug!("Parsed CLI arguments: {:?}", cli);

    // --generate-completions SHELL: print shell completions and exit
    if let Some(shell) = cli.general.generate_completions {
        crate::completions::generate_completions(shell);
        return Ok(());
    }

    // --dump-man-page: print man page and exit
    if cli.general.dump_man_page {
        crate::manpage::generate_manpage()?;
        return Ok(());
    }

    // --update: check for updates via GitHub releases API
    if cli.general.update {
        crate::update::check_update().await?;
        return Ok(());
    }

    // --rm-cache-dir: remove cache directory
    if cli.filesystem.rm_cache_dir {
        remove_cache_dir(&cli)?;
        return Ok(());
    }

    // 2. Build the core config from CLI args.
    let config = cli.to_config();
    debug!("Resolved config: {:?}", config);

    // 3. Create HTTP client from network config.
    let http_client = Arc::new(HttpClient::new(&config.network)?);

    // 4. Create extractor registry.
    let registry = create_default_registry();

    // 5. Create download manager.
    let download_manager = DownloadManager::new(http_client.clone(), &config.download);

    // 6. Create progress reporter.
    let progress: Box<dyn ProgressReporter> = if config.general.quiet {
        Box::new(QuietReporter)
    } else {
        Box::new(IndicatifReporter::new())
    };

    // 7. Handle special commands.

    // --list-extractors
    if cli.general.list_extractors {
        for (key, name) in registry.list_extractors() {
            println!("{key}: {name}");
        }
        return Ok(());
    }

    // --extractor-descriptions
    if cli.general.extractor_descriptions {
        for (key, name) in registry.list_extractors() {
            println!("{key}: {name}");
        }
        return Ok(());
    }

    // 8. Require at least one URL for all remaining operations.
    if cli.urls.is_empty() {
        bail!(
            "No URLs provided. Use 'yt-dlp-rs --help' for usage information.\n\
             Usage: yt-dlp-rs [OPTIONS] URL [URL...]"
        );
    }

    // Determine simulation mode.
    let simulate = config.general.simulate || config.general.print_json;
    if simulate {
        info!("Running in simulation mode (no downloads will occur)");
    }

    // Load download archive if --download-archive was specified.
    let mut archive: Option<DownloadArchive> =
        if let Some(ref archive_path) = cli.video_selection.download_archive {
            let a = DownloadArchive::load(archive_path)
                .with_context(|| format!("failed to load download archive: {}", archive_path.display()))?;
            info!(entries = a.len(), path = %archive_path.display(), "loaded download archive");
            Some(a)
        } else {
            None
        };

    // Build filter config from CLI args.
    let filter_config = build_filter_config(&cli);

    // 9. Process each URL.
    let mut download_count = 0u64;
    let mut error_count = 0u64;
    let max_downloads = cli.video_selection.max_downloads;
    let break_on_existing = cli.video_selection.break_on_existing;
    let break_per_url = cli.video_selection.break_per_url;

    'url_loop: for (i, url) in cli.urls.iter().enumerate() {
        if let Some(max) = max_downloads {
            if download_count >= max {
                info!("Reached maximum download count ({max}), stopping.");
                break;
            }
        }

        // Reset break-on-existing state per URL if requested.
        let mut encountered_existing = false;
        let _ = break_per_url; // consumed below

        let index = i + 1;
        let total = cli.urls.len();
        if !config.general.quiet && total > 1 {
            println!("[{index}/{total}] Processing: {url}");
        }

        // Find extractor for this URL.
        let extractor = match registry.find_extractor(url) {
            Some(e) => e,
            None => {
                let msg = format!("No extractor found for URL: {url}");
                if config.general.ignore_errors {
                    warn!("{msg}");
                    eprintln!("ERROR: {msg}; skipping...");
                    error_count += 1;
                    continue;
                } else {
                    bail!("{msg}");
                }
            }
        };

        info!(extractor = extractor.key(), url = url, "extracting");

        // Extract info from the URL.
        let result = extractor.extract(url, &http_client).await;

        match result {
            Ok(extraction) => match extraction {
                ExtractionResult::SingleVideo(info) => {
                    match process_video(
                        &config,
                        &cli,
                        &download_manager,
                        &*progress,
                        *info,
                        &mut download_count,
                        &mut archive,
                        &filter_config,
                        break_on_existing,
                        &mut encountered_existing,
                    )
                    .await
                    {
                        Ok(ProcessOutcome::Downloaded | ProcessOutcome::Skipped) => {}
                        Ok(ProcessOutcome::BreakRequested) => {
                            info!("Break requested, stopping.");
                            break 'url_loop;
                        }
                        Err(e) => {
                            error_count += 1;
                            if config.general.abort_on_error {
                                return Err(e);
                            }
                            if config.general.ignore_errors {
                                warn!("Error processing {url}: {e}");
                                if !config.general.quiet {
                                    eprintln!("ERROR: {e}; skipping...");
                                }
                            } else {
                                return Err(e);
                            }
                        }
                    }
                }
                ExtractionResult::Playlist(playlist) => {
                    println!(
                        "[playlist] {} - {} entries",
                        playlist.title.as_deref().unwrap_or("Unknown"),
                        playlist.entries.len()
                    );
                    for entry in &playlist.entries {
                        if let Some(max) = max_downloads {
                            if download_count >= max {
                                info!("Reached maximum download count ({max}), stopping.");
                                break;
                            }
                        }

                        let process_result = match entry {
                            PlaylistEntry::Info(info) => {
                                process_video(
                                    &config,
                                    &cli,
                                    &download_manager,
                                    &*progress,
                                    *info.clone(),
                                    &mut download_count,
                                    &mut archive,
                                    &filter_config,
                                    break_on_existing,
                                    &mut encountered_existing,
                                )
                                .await
                            }
                            PlaylistEntry::Url(entry_url) => {
                                // Re-extract the entry URL through the registry
                                match registry.find_extractor(entry_url) {
                                    Some(entry_extractor) => {
                                        match entry_extractor.extract(entry_url, &http_client).await
                                        {
                                            Ok(ExtractionResult::SingleVideo(info)) => {
                                                process_video(
                                                    &config,
                                                    &cli,
                                                    &download_manager,
                                                    &*progress,
                                                    *info,
                                                    &mut download_count,
                                                    &mut archive,
                                                    &filter_config,
                                                    break_on_existing,
                                                    &mut encountered_existing,
                                                )
                                                .await
                                            }
                                            Ok(_) => {
                                                // Nested playlist — skip for now
                                                println!(
                                                    "  [playlist entry] {entry_url} (nested playlist, skipping)"
                                                );
                                                Ok(ProcessOutcome::Skipped)
                                            }
                                            Err(e) => Err(e.into()),
                                        }
                                    }
                                    None => {
                                        println!("  [playlist entry] {entry_url} (no extractor found, skipping)");
                                        error_count += 1;
                                        continue;
                                    }
                                }
                            }
                        };

                        match process_result {
                            Ok(ProcessOutcome::Downloaded | ProcessOutcome::Skipped) => {}
                            Ok(ProcessOutcome::BreakRequested) => {
                                info!("Break requested, stopping playlist processing.");
                                if break_per_url {
                                    break; // break out of playlist loop, continue with next URL
                                } else {
                                    break 'url_loop;
                                }
                            }
                            Err(e) => {
                                error_count += 1;
                                if config.general.abort_on_error {
                                    return Err(e);
                                }
                                if config.general.ignore_errors {
                                    warn!("Error: {e}");
                                    if !config.general.quiet {
                                        eprintln!("ERROR: {e}; skipping...");
                                    }
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
            },
            Err(e) => {
                error_count += 1;
                if config.general.abort_on_error {
                    return Err(e.into());
                }
                if config.general.ignore_errors {
                    warn!("Error processing {url}: {e}");
                    if !config.general.quiet {
                        eprintln!("ERROR: {url}: {e}; skipping...");
                    }
                } else {
                    return Err(e.into());
                }
            }
        }
    }

    if !config.general.quiet {
        if error_count > 0 {
            eprintln!(
                "Finished with {download_count} successful, {error_count} failed."
            );
        } else if !simulate && download_count > 0 {
            debug!("All {download_count} URL(s) processed successfully.");
        }
    }

    Ok(())
}

/// Outcome of processing a single video.
enum ProcessOutcome {
    /// The video was downloaded successfully.
    Downloaded,
    /// The video was skipped (archive hit, filter reject, simulate, etc.).
    Skipped,
    /// A break condition was triggered (--break-match-filters or --break-on-existing).
    BreakRequested,
}

/// Build a [`FilterConfig`] from the parsed CLI arguments.
fn build_filter_config(cli: &Cli) -> FilterConfig {
    let vs = &cli.video_selection;
    FilterConfig {
        match_title: vs.match_title.clone(),
        reject_title: vs.reject_title.clone(),
        age_limit: vs.age_limit,
        date: vs.date.clone(),
        datebefore: vs.datebefore.clone(),
        dateafter: vs.dateafter.clone(),
        match_filters: vs.match_filter.clone(),
        break_match_filters: vs.break_match_filters.clone(),
        min_filesize: vs
            .min_filesize
            .as_deref()
            .and_then(filters::parse_filesize),
        max_filesize: vs
            .max_filesize
            .as_deref()
            .and_then(filters::parse_filesize),
    }
}

/// Process a single video: check archive, apply filters, select formats, download, post-process.
#[allow(clippy::too_many_arguments)]
async fn process_video(
    config: &Config,
    cli: &Cli,
    download_manager: &DownloadManager,
    progress: &dyn ProgressReporter,
    info: InfoDict,
    download_count: &mut u64,
    archive: &mut Option<DownloadArchive>,
    filter_config: &FilterConfig,
    break_on_existing: bool,
    encountered_existing: &mut bool,
) -> Result<ProcessOutcome> {
    // Print title
    println!(
        "[{}] {}: {}",
        info.extractor,
        info.id,
        info.title.as_deref().unwrap_or("Unknown")
    );

    // Check download archive: skip if already downloaded.
    if let Some(arch) = &*archive {
        if arch.contains(&info.extractor_key, &info.id) {
            println!(
                "[download] {} has already been recorded in the archive",
                info.id
            );
            if break_on_existing {
                *encountered_existing = true;
                return Ok(ProcessOutcome::BreakRequested);
            }
            return Ok(ProcessOutcome::Skipped);
        }
    }

    // Apply video filters.
    match filters::apply_filters(&info, filter_config) {
        FilterResult::Accept => {}
        FilterResult::Reject(reason) => {
            info!(id = info.id, reason = %reason, "video rejected by filter");
            if !config.general.quiet {
                println!("[filter] Skipping {}: {reason}", info.id);
            }
            return Ok(ProcessOutcome::Skipped);
        }
        FilterResult::Break(reason) => {
            info!(id = info.id, reason = %reason, "break filter matched");
            if !config.general.quiet {
                println!("[filter] Stopping: {reason}");
            }
            return Ok(ProcessOutcome::BreakRequested);
        }
    }

    // --dump-json: print info dict as JSON and return
    if cli.general.dump_json {
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(ProcessOutcome::Skipped);
    }

    // --print TEMPLATE: print template values
    if !cli.general.print_template.is_empty() {
        for template in &cli.general.print_template {
            let rendered = render_template(template, &info)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{rendered}");
        }
        if cli.general.simulate || cli.general.skip_download {
            return Ok(ProcessOutcome::Skipped);
        }
    }

    // --simulate or --skip-download: just print format info
    if cli.general.simulate || cli.general.skip_download {
        for fmt in &info.formats {
            println!("  {}", fmt);
        }
        return Ok(ProcessOutcome::Skipped);
    }

    // Format selection
    let format_string = config
        .format_selection
        .format
        .as_deref()
        .unwrap_or("bestvideo+bestaudio/best");
    let selected = select_formats(&info.formats, format_string)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if selected.is_empty() {
        bail!("No formats matched format string: {format_string}");
    }

    // Print selected formats
    for fmt in &selected {
        println!("  [format] {} ({})", fmt.format_id, fmt);
    }

    // Build output filename
    let template = config.output.output_template.as_str();
    let filename = render_template(template, &info)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let filename = sanitize_filename(&filename);

    let default_dir = std::path::PathBuf::from(".");
    let output_dir = config.output.output_dir.as_deref().unwrap_or(&default_dir);

    // Create output directory if needed
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create output directory: {}", output_dir.display()))?;
    }

    // Create a modified InfoDict with requested_formats set
    let mut download_info = info.clone();
    if selected.len() > 1 {
        download_info.requested_formats =
            Some(selected.iter().map(|f| (*f).clone()).collect());
    }

    // Download
    let output_path = download_manager
        .download_info(&download_info, output_dir, &filename, progress)
        .await
        .with_context(|| format!("failed to download {}", info.id))?;

    progress.finish();

    println!(
        "[download] {} saved to {}",
        info.id,
        output_path.display()
    );

    // Record in the download archive after successful download.
    if let Some(arch) = archive {
        arch.record(&info.extractor_key, &info.id)
            .with_context(|| "failed to update download archive")?;
        debug!(id = info.id, "recorded in download archive");
    }

    // -----------------------------------------------------------------------
    // Pre-post-processing: write sidecar files
    // -----------------------------------------------------------------------

    // Download thumbnail if --write-thumbnail or --embed-thumbnail is set.
    // --embed-thumbnail needs the thumbnail file on disk to embed it.
    if cli.thumbnail.write_thumbnail || config.postprocessing.embed_thumbnail {
        write_thumbnail(&info, &output_path).await?;
    }

    // --write-description: write info.description to a .description file
    if cli.filesystem.write_description {
        write_description(&info, &output_path)?;
    }

    // -----------------------------------------------------------------------
    // Post-processing chain
    // -----------------------------------------------------------------------

    let chain = build_postprocessor_chain(config, cli.filesystem.write_info_json);

    let final_path = if !chain.is_empty() {
        info!(
            count = chain.len(),
            "running {} post-processor(s)",
            chain.len()
        );
        let result = chain
            .run_all(&info, &output_path)
            .with_context(|| "post-processing failed")?;
        if result != output_path {
            println!(
                "[postprocess] output moved to {}",
                result.display()
            );
        }
        result
    } else {
        output_path
    };

    if !config.general.quiet {
        println!(
            "[done] {} -> {}",
            info.id,
            final_path.display()
        );
    }

    *download_count += 1;

    Ok(ProcessOutcome::Downloaded)
}

/// Build a [`PostProcessorChain`] from the resolved [`Config`].
///
/// The order of post-processors mirrors yt-dlp's conventional ordering:
/// 1. Audio extraction (mutually exclusive with remux)
/// 2. Remux
/// 3. Metadata embed
/// 4. Subtitle embed
/// 5. Thumbnail embed
/// 6. Chapter embed
/// 7. SponsorBlock
/// 8. Info JSON
/// 9. Exec (always last)
fn build_postprocessor_chain(config: &Config, write_info_json: bool) -> PostProcessorChain {
    let pp = &config.postprocessing;
    let mut chain = PostProcessorChain::new();

    // Try to locate FFmpeg. Many post-processors need it.
    let ffmpeg = match FFmpeg::new(pp.ffmpeg_location.as_deref()) {
        Ok(ff) => Some(Arc::new(ff)),
        Err(e) => {
            // FFmpeg is only required if an ffmpeg-based PP is actually used.
            debug!("ffmpeg not available: {e}");
            None
        }
    };

    // Helper: get a clone of Arc<FFmpeg> or bail early from the block.
    macro_rules! need_ffmpeg {
        () => {
            match ffmpeg.clone() {
                Some(ff) => ff,
                None => {
                    warn!("skipping post-processor: ffmpeg not found");
                    return chain;
                }
            }
        };
    }

    // 1. Extract audio (-x / --extract-audio)
    if pp.extract_audio {
        let ff = need_ffmpeg!();
        let audio_format = config
            .format_selection
            .audio_format
            .clone()
            .unwrap_or_else(|| "m4a".to_string());
        chain.add(Box::new(AudioExtractPP::new(
            ff,
            audio_format,
            config.format_selection.audio_quality.clone(),
        )));
    }

    // 2. Remux (--remux-video FORMAT)
    if let Some(ref target) = config.format_selection.remux_video {
        if !pp.extract_audio {
            let ff = need_ffmpeg!();
            chain.add(Box::new(RemuxPP::new(ff, target.clone())));
        }
    }

    // 3. Embed metadata (--embed-metadata)
    if pp.embed_metadata {
        let ff = need_ffmpeg!();
        chain.add(Box::new(MetadataEmbedPP::new(ff)));
    }

    // 4. Embed subtitles (--embed-subs)
    if pp.embed_subtitles {
        let ff = need_ffmpeg!();
        chain.add(Box::new(SubtitleEmbedPP::new(ff)));
    }

    // 5. Embed thumbnail (--embed-thumbnail)
    if pp.embed_thumbnail {
        let ff = need_ffmpeg!();
        chain.add(Box::new(ThumbnailEmbedPP::new(ff)));
    }

    // 6. Embed chapters (--embed-chapters)
    if pp.embed_chapters {
        let ff = need_ffmpeg!();
        chain.add(Box::new(ChapterEmbedPP::new(ff)));
    }

    // 7. SponsorBlock removal (--sponsorblock-remove CATS)
    if !pp.sponsorblock_remove.is_empty() {
        let ff = need_ffmpeg!();
        chain.add(Box::new(SponsorBlockPP::new(
            ff,
            pp.sponsorblock_remove.clone(),
            pp.sponsorblock_mark.clone(),
        )));
    }

    // 8. Write info JSON (--write-info-json)
    if write_info_json {
        chain.add(Box::new(InfoJsonPP::new()));
    }

    // 9. Exec (--exec CMD) — always last
    if let Some(ref cmd) = pp.exec_cmd {
        chain.add(Box::new(ExecPP::new(cmd.clone())));
    }

    chain
}

/// Download the best thumbnail to a file next to the video.
async fn write_thumbnail(info: &InfoDict, output_path: &Path) -> Result<()> {
    // Prefer info.thumbnail, then fall back to the last entry in info.thumbnails.
    let url = info
        .thumbnail
        .as_deref()
        .or_else(|| info.thumbnails.last().map(|t| t.url.as_str()));

    let url = match url {
        Some(u) => u,
        None => {
            info!("No thumbnail URL available, skipping --write-thumbnail.");
            return Ok(());
        }
    };

    // Determine extension from URL or default to jpg.
    let ext = url
        .rsplit('/')
        .next()
        .and_then(|seg| {
            let seg = seg.split('?').next().unwrap_or(seg);
            seg.rsplit('.').next()
        })
        .and_then(|e| {
            let lower = e.to_lowercase();
            match lower.as_str() {
                "jpg" | "jpeg" | "png" | "webp" => Some(lower),
                _ => None,
            }
        })
        .unwrap_or_else(|| "jpg".to_string());

    let thumb_path = output_path.with_extension(&ext);

    info!(url = url, path = %thumb_path.display(), "downloading thumbnail");

    // Use a simple reqwest-free approach: tokio::process::Command with curl,
    // or if the networking client is available, use it.
    // For simplicity and to avoid adding dependencies, use tokio::process::Command.
    let status = tokio::process::Command::new("curl")
        .args(["-sL", "-o"])
        .arg(&thumb_path)
        .arg(url)
        .status()
        .await
        .context("failed to launch curl for thumbnail download")?;

    if !status.success() {
        warn!("Thumbnail download failed (curl exited with {status}), continuing.");
    } else {
        println!(
            "[thumbnail] saved to {}",
            thumb_path.display()
        );
    }

    Ok(())
}

/// Write the video description to a .description sidecar file.
fn write_description(info: &InfoDict, output_path: &Path) -> Result<()> {
    let desc = match &info.description {
        Some(d) => d,
        None => {
            info!("No description available, skipping --write-description.");
            return Ok(());
        }
    };

    let desc_path = output_path.with_extension("description");
    std::fs::write(&desc_path, desc)
        .with_context(|| format!("failed to write description to {}", desc_path.display()))?;
    println!(
        "[description] saved to {}",
        desc_path.display()
    );
    Ok(())
}

/// Initialize the tracing subscriber based on CLI verbosity flags.
fn init_tracing(cli: &Cli) {
    use tracing_subscriber::EnvFilter;

    let filter = if cli.general.quiet {
        "error"
    } else {
        match cli.general.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };

    // Allow RUST_LOG to override if set.
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_thread_ids(false)
        .init();
}

/// Remove the cache directory.
fn remove_cache_dir(cli: &Cli) -> Result<()> {
    let cache_dir = cli
        .filesystem
        .cache_dir
        .clone()
        .or_else(|| dirs::cache_dir().map(|d| d.join("yt-dlp-rs")));

    if let Some(dir) = cache_dir {
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
            println!("Removed cache directory: {}", dir.display());
        } else {
            println!("Cache directory does not exist: {}", dir.display());
        }
    } else {
        println!("No cache directory configured.");
    }

    Ok(())
}
