use clap::Parser;
use std::path::PathBuf;
use yt_dlp_core::config::{
    AuthConfig, Config, DownloadConfig, FormatSelectionConfig, GeneralConfig, NetworkConfig,
    OutputConfig, PostProcessingConfig, SubtitleConfig,
};

/// yt-dlp-rs: A Rust rewrite of yt-dlp — a feature-rich command-line audio/video downloader.
///
/// Usage: yt-dlp-rs [OPTIONS] [URLS]...
#[derive(Parser, Debug)]
#[command(
    name = "yt-dlp-rs",
    version,
    about = "A feature-rich command-line audio/video downloader",
    long_about = "yt-dlp-rs is a Rust rewrite of yt-dlp, a feature-rich command-line audio/video downloader.\n\nSupports hundreds of sites. Run --list-extractors to see the full list.",
    after_help = "See 'yt-dlp-rs --help' for more information on a specific option group."
)]
pub struct Cli {
    /// URLs to download
    #[arg(value_name = "URL")]
    pub urls: Vec<String>,

    #[command(flatten)]
    pub general: GeneralArgs,

    #[command(flatten)]
    pub network: NetworkArgs,

    #[command(flatten)]
    pub geo: GeoArgs,

    #[command(flatten)]
    pub video_selection: VideoSelectionArgs,

    #[command(flatten)]
    pub auth: AuthArgs,

    #[command(flatten)]
    pub format: FormatArgs,

    #[command(flatten)]
    pub subtitle: SubtitleArgs,

    #[command(flatten)]
    pub download: DownloadArgs,

    #[command(flatten)]
    pub workaround: WorkaroundArgs,

    #[command(flatten)]
    pub verbosity: VerbosityArgs,

    #[command(flatten)]
    pub filesystem: FilesystemArgs,

    #[command(flatten)]
    pub thumbnail: ThumbnailArgs,

    #[command(flatten)]
    pub postprocessing: PostProcessingArgs,
}

// ---------------------------------------------------------------------------
// General Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "General Options")]
pub struct GeneralArgs {
    /// Print program version and exit
    #[arg(long = "update", help = "Check for program updates")]
    pub update: bool,

    /// List all supported extractors
    #[arg(long)]
    pub list_extractors: bool,

    /// Print descriptions of all supported extractors
    #[arg(long)]
    pub extractor_descriptions: bool,

    /// Dump extracted info as JSON without downloading
    #[arg(long)]
    pub dump_json: bool,

    /// Print a field value using an output template (can be specified multiple times)
    #[arg(long = "print", value_name = "TEMPLATE")]
    pub print_template: Vec<String>,

    /// Increase verbosity (can repeat: -vv)
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress output
    #[arg(short, long)]
    pub quiet: bool,

    /// Suppress warning messages
    #[arg(long)]
    pub no_warnings: bool,

    /// Do not download the video, just print information
    #[arg(short, long)]
    pub simulate: bool,

    /// Do not download the video (alias for --simulate when used without --print/--dump-json)
    #[arg(long)]
    pub skip_download: bool,

    /// Continue on download errors; do not abort on unavailable videos in a playlist
    #[arg(short, long)]
    pub ignore_errors: bool,

    /// Abort downloading of further videos if an error occurs
    #[arg(long)]
    pub abort_on_error: bool,

    /// Do not overwrite any files
    #[arg(long)]
    pub no_overwrites: bool,

    /// Generate shell completions for the given shell (bash, zsh, fish, powershell, elvish)
    #[arg(long, value_name = "SHELL")]
    pub generate_completions: Option<clap_complete::Shell>,

    /// Dump a man page (roff format) to stdout
    #[arg(long)]
    pub dump_man_page: bool,
}

// ---------------------------------------------------------------------------
// Network Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Network Options")]
pub struct NetworkArgs {
    /// Use the specified HTTP/HTTPS/SOCKS proxy (e.g., socks5://127.0.0.1:1080)
    #[arg(long, value_name = "URL")]
    pub proxy: Option<String>,

    /// Time to wait before giving up on a connection, in seconds
    #[arg(long, value_name = "SECONDS")]
    pub socket_timeout: Option<u64>,

    /// Client-side IP address to bind to
    #[arg(long, value_name = "IP")]
    pub source_address: Option<String>,

    /// Make all connections via IPv4
    #[arg(short = '4', long)]
    pub force_ipv4: bool,

    /// Make all connections via IPv6
    #[arg(short = '6', long)]
    pub force_ipv6: bool,

    /// Impersonate a specific client for requests (e.g., chrome, firefox)
    #[arg(long, value_name = "CLIENT")]
    pub impersonate: Option<String>,
}

// ---------------------------------------------------------------------------
// Geo-Restriction Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Geo-Restriction Options")]
pub struct GeoArgs {
    /// Use this proxy to verify the IP address for some geo-restricted sites
    #[arg(long, value_name = "URL")]
    pub geo_verification_proxy: Option<String>,

    /// How to fake X-Forwarded-For HTTP header to try to bypass geo restriction
    #[arg(long, value_name = "VALUE")]
    pub xff: Option<String>,
}

// ---------------------------------------------------------------------------
// Video Selection Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Video Selection Options")]
pub struct VideoSelectionArgs {
    /// Playlist index to start downloading from (default: 1)
    #[arg(long, value_name = "N")]
    pub playlist_start: Option<u64>,

    /// Playlist index to stop downloading at (default: last)
    #[arg(long, value_name = "N")]
    pub playlist_end: Option<u64>,

    /// Specific playlist items to download (e.g., "1,3-5,7")
    #[arg(long, value_name = "SPEC")]
    pub playlist_items: Option<String>,

    /// Download only videos whose title matches the given regex
    #[arg(long, value_name = "REGEX")]
    pub match_title: Option<String>,

    /// Skip videos whose title matches the given regex
    #[arg(long, value_name = "REGEX")]
    pub reject_title: Option<String>,

    /// Abort download if filesize is smaller than SIZE (e.g., 50k or 44.6M)
    #[arg(long, value_name = "SIZE")]
    pub min_filesize: Option<String>,

    /// Abort download if filesize is larger than SIZE (e.g., 50k or 44.6M)
    #[arg(long, value_name = "SIZE")]
    pub max_filesize: Option<String>,

    /// Download only videos uploaded on this date (YYYYMMDD or date expression)
    #[arg(long, value_name = "DATE")]
    pub date: Option<String>,

    /// Download only videos uploaded on or before this date
    #[arg(long, value_name = "DATE")]
    pub datebefore: Option<String>,

    /// Download only videos uploaded on or after this date
    #[arg(long, value_name = "DATE")]
    pub dateafter: Option<String>,

    /// Generic video filter (e.g., "duration > 60")
    #[arg(long, value_name = "FILTER")]
    pub match_filter: Vec<String>,

    /// Stop downloading when a video matches the given filter expression
    #[arg(long, value_name = "FILTER")]
    pub break_match_filters: Vec<String>,

    /// Download only the video if a URL refers to a video and a playlist
    #[arg(long)]
    pub no_playlist: bool,

    /// Download the playlist if a URL refers to a video and a playlist
    #[arg(long)]
    pub yes_playlist: bool,

    /// Download only videos suitable for the given age
    #[arg(long, value_name = "YEARS")]
    pub age_limit: Option<u8>,

    /// Download only videos not listed in the archive file; record downloaded video IDs
    #[arg(long, value_name = "FILE")]
    pub download_archive: Option<PathBuf>,

    /// Stop the download process when encountering a file already in the archive
    #[arg(long)]
    pub break_on_existing: bool,

    /// Reset per-URL state (like --break-on-existing) for each input URL
    #[arg(long)]
    pub break_per_url: bool,

    /// Maximum number of videos to download
    #[arg(long, value_name = "NUMBER")]
    pub max_downloads: Option<u64>,
}

// ---------------------------------------------------------------------------
// Authentication Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Authentication Options")]
pub struct AuthArgs {
    /// Login with this account ID
    #[arg(short, long, value_name = "USERNAME")]
    pub username: Option<String>,

    /// Account password (if omitted, will be asked interactively)
    #[arg(short, long, value_name = "PASSWORD")]
    pub password: Option<String>,

    /// Two-factor authentication code
    #[arg(short = '2', long, value_name = "CODE")]
    pub twofactor: Option<String>,

    /// Use .netrc authentication data
    #[arg(long)]
    pub netrc: bool,

    /// Location of the .netrc file
    #[arg(long, value_name = "PATH")]
    pub netrc_location: Option<PathBuf>,

    /// Path to client certificate file in PEM format
    #[arg(long, value_name = "FILE")]
    pub client_certificate: Option<PathBuf>,

    /// Path to private key file for client certificate
    #[arg(long, value_name = "FILE")]
    pub client_certificate_key: Option<PathBuf>,

    /// Password for client certificate private key
    #[arg(long, value_name = "PASSWORD")]
    pub client_certificate_password: Option<String>,
}

// ---------------------------------------------------------------------------
// Format Selection Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Format Selection Options")]
pub struct FormatArgs {
    /// Video format code (e.g., "bestvideo+bestaudio/best")
    #[arg(short, long, value_name = "FORMAT")]
    pub format: Option<String>,

    /// Sort the formats by the given fields (e.g., "res,ext")
    #[arg(long, value_name = "SORTORDER")]
    pub format_sort: Vec<String>,

    /// Force user-specified sort order to have precedence over all fields
    #[arg(long)]
    pub format_sort_force: bool,

    /// Prefer video formats with free containers (webm) over same-quality non-free ones
    #[arg(long)]
    pub prefer_free_formats: bool,

    /// Container format to use when merging audio/video (e.g., mp4, mkv, webm)
    #[arg(long, value_name = "FORMAT")]
    pub merge_output_format: Option<String>,

    /// Audio format to convert to when using -x (e.g., mp3, m4a, opus)
    #[arg(long, value_name = "FORMAT")]
    pub audio_format: Option<String>,

    /// Audio quality when converting (0 is best, 10 is worst; or a specific bitrate like 128K)
    #[arg(long, value_name = "QUALITY")]
    pub audio_quality: Option<String>,

    /// Remux the video into another container if necessary (e.g., mp4)
    #[arg(long, value_name = "FORMAT")]
    pub remux_video: Option<String>,

    /// Re-encode the video into another format if necessary (e.g., mp4)
    #[arg(long, value_name = "FORMAT")]
    pub recode_video: Option<String>,

    /// Allow multiple video streams to be merged into a single file
    #[arg(long)]
    pub video_multistreams: bool,

    /// Do not allow multiple video streams (default)
    #[arg(long)]
    pub no_video_multistreams: bool,

    /// Allow multiple audio streams to be merged into a single file
    #[arg(long)]
    pub audio_multistreams: bool,

    /// Do not allow multiple audio streams (default)
    #[arg(long)]
    pub no_audio_multistreams: bool,
}

// ---------------------------------------------------------------------------
// Subtitle Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Subtitle Options")]
pub struct SubtitleArgs {
    /// Write subtitle files
    #[arg(long)]
    pub write_subs: bool,

    /// Write automatically generated subtitle files
    #[arg(long)]
    pub write_auto_subs: bool,

    /// Download all available subtitles
    #[arg(long)]
    pub all_subs: bool,

    /// Languages of the subtitles to download (comma-separated, e.g., "en,es")
    #[arg(long, value_name = "LANGS")]
    pub sub_langs: Option<String>,

    /// Subtitle format preference (e.g., "srt", "ass/srt/best")
    #[arg(long, value_name = "FORMAT")]
    pub sub_format: Option<String>,
}

// ---------------------------------------------------------------------------
// Download Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Download Options")]
pub struct DownloadArgs {
    /// Number of fragments of a DASH/HLS video to download concurrently (default: 1)
    #[arg(long, value_name = "N", default_value = "1")]
    pub concurrent_fragments: u32,

    /// Maximum download rate in bytes per second (e.g., 50K or 4.2M)
    #[arg(long, value_name = "RATE")]
    pub limit_rate: Option<String>,

    /// Number of retries for a download (default: 10)
    #[arg(long, value_name = "N", default_value = "10")]
    pub retries: u32,

    /// Number of retries for a fragment (default: 10)
    #[arg(long, value_name = "N", default_value = "10")]
    pub fragment_retries: u32,

    /// Size of download buffer (e.g., 1024 or 16K)
    #[arg(long, value_name = "SIZE")]
    pub buffer_size: Option<String>,

    /// Use the specified external downloader (e.g., aria2c, curl)
    #[arg(long, value_name = "NAME")]
    pub external_downloader: Option<String>,

    /// Give these arguments to the external downloader
    #[arg(long, value_name = "ARGS")]
    pub external_downloader_args: Option<String>,
}

// ---------------------------------------------------------------------------
// Workaround Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Workaround Options")]
pub struct WorkaroundArgs {
    /// Specify a custom user agent
    #[arg(long, value_name = "UA")]
    pub user_agent: Option<String>,

    /// Specify a custom HTTP referer
    #[arg(long, value_name = "URL")]
    pub referer: Option<String>,

    /// Specify additional HTTP headers (can be specified multiple times)
    #[arg(long, value_name = "FIELD:VALUE")]
    pub add_headers: Vec<String>,

    /// Number of seconds to sleep between requests during data extraction
    #[arg(long, value_name = "SECONDS")]
    pub sleep_interval: Option<f64>,

    /// Maximum number of seconds to sleep (used with --sleep-interval for random sleep)
    #[arg(long, value_name = "SECONDS")]
    pub max_sleep_interval: Option<f64>,

    /// Number of seconds to sleep before each subtitle download
    #[arg(long, value_name = "SECONDS")]
    pub sleep_subtitles: Option<f64>,
}

// ---------------------------------------------------------------------------
// Verbosity / Simulation Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Verbosity / Simulation Options")]
pub struct VerbosityArgs {
    /// Write downloaded intermediary pages to files in the current directory
    #[arg(long)]
    pub dump_pages: bool,

    /// Write downloaded intermediary pages to numbered files in the current directory
    #[arg(long)]
    pub write_pages: bool,

    /// Display sent and received HTTP traffic
    #[arg(long)]
    pub print_traffic: bool,

    /// Show progress bar
    #[arg(long)]
    pub progress: bool,

    /// Do not show progress bar
    #[arg(long)]
    pub no_progress: bool,

    /// Output progress bar as new lines
    #[arg(long)]
    pub newline: bool,

    /// Display progress in console titlebar
    #[arg(long)]
    pub console_title: bool,
}

// ---------------------------------------------------------------------------
// Filesystem Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Filesystem Options")]
pub struct FilesystemArgs {
    /// Output filename template (e.g., "%(title)s.%(ext)s")
    #[arg(short, long, value_name = "TEMPLATE")]
    pub output: Option<String>,

    /// Placeholder for unavailable template fields (default: "NA")
    #[arg(long, value_name = "TEXT")]
    pub output_na_placeholder: Option<String>,

    /// File paths for different output types (e.g., "subtitle:/path/to/dir")
    #[arg(short = 'P', long = "paths", value_name = "TYPE:PATH")]
    pub paths: Vec<String>,

    /// Restrict filenames to ASCII characters and avoid "&" and spaces
    #[arg(long)]
    pub restrict_filenames: bool,

    /// Force filenames to be Windows-compatible
    #[arg(long)]
    pub windows_filenames: bool,

    /// Limit the filename length (excluding extension) to the specified number of characters
    #[arg(long, value_name = "LENGTH")]
    pub trim_filenames: Option<u32>,

    /// Netscape-format cookies file to read cookies from and dump cookies to
    #[arg(long, value_name = "FILE")]
    pub cookies: Option<PathBuf>,

    /// Browser to extract cookies from (e.g., "chrome", "firefox")
    #[arg(long, value_name = "BROWSER")]
    pub cookies_from_browser: Option<String>,

    /// Location of the cache directory
    #[arg(long, value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,

    /// Disable filesystem caching
    #[arg(long)]
    pub no_cache_dir: bool,

    /// Delete all filesystem cache files
    #[arg(long)]
    pub rm_cache_dir: bool,

    /// Write video description to a .description file
    #[arg(long)]
    pub write_description: bool,

    /// Write video metadata to a .info.json file
    #[arg(long)]
    pub write_info_json: bool,

    /// Retrieve video comments to be placed in the .info.json file
    #[arg(long)]
    pub write_comments: bool,
}

// ---------------------------------------------------------------------------
// Thumbnail Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Thumbnail Options")]
pub struct ThumbnailArgs {
    /// Write thumbnail image to disk
    #[arg(long)]
    pub write_thumbnail: bool,

    /// Write all thumbnail image formats to disk
    #[arg(long)]
    pub write_all_thumbnails: bool,

    /// Convert thumbnails to given format (e.g., jpg, png, webp)
    #[arg(long, value_name = "FORMAT")]
    pub convert_thumbnails: Option<String>,
}

// ---------------------------------------------------------------------------
// Post-Processing Options
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(next_help_heading = "Post-Processing Options")]
pub struct PostProcessingArgs {
    /// Extract audio from video and convert to best audio-only format
    #[arg(short = 'x', long)]
    pub extract_audio: bool,

    /// Audio format to convert to when using -x (e.g., mp3, m4a, opus, flac)
    #[arg(long = "pp-audio-format", value_name = "FORMAT")]
    pub pp_audio_format: Option<String>,

    /// Audio quality for conversion (0 best, 10 worst; or specific bitrate like 192K)
    #[arg(long = "pp-audio-quality", value_name = "QUALITY")]
    pub pp_audio_quality: Option<String>,

    /// Embed subtitles in the video (only for mp4, webm, and mkv)
    #[arg(long)]
    pub embed_subs: bool,

    /// Embed thumbnail in the video as cover art
    #[arg(long)]
    pub embed_thumbnail: bool,

    /// Embed metadata (title, artist, date, etc.) in the file
    #[arg(long)]
    pub embed_metadata: bool,

    /// Embed chapter markers in the video
    #[arg(long)]
    pub embed_chapters: bool,

    /// Embed the infojson as an attachment to mkv/mka video files
    #[arg(long)]
    pub embed_info_json: bool,

    /// Parse additional metadata from other fields (FROM:TO format)
    #[arg(long, value_name = "FROM:TO")]
    pub parse_metadata: Vec<String>,

    /// SponsorBlock categories to mark in the video (comma-separated)
    #[arg(long, value_name = "CATS")]
    pub sponsorblock_mark: Option<String>,

    /// SponsorBlock categories to remove from the video (comma-separated)
    #[arg(long, value_name = "CATS")]
    pub sponsorblock_remove: Option<String>,

    /// Location of the ffmpeg binary; either the path to the binary or its containing directory
    #[arg(long, value_name = "PATH")]
    pub ffmpeg_location: Option<PathBuf>,

    /// Execute a command on the downloaded file (use {} as placeholder for filename)
    #[arg(long, value_name = "CMD")]
    pub exec: Option<String>,

    /// Convert subtitles to the given format (e.g., srt, ass, lrc)
    #[arg(long, value_name = "FORMAT")]
    pub convert_subs: Option<String>,
}

// ---------------------------------------------------------------------------
// Impl
// ---------------------------------------------------------------------------

impl Cli {
    /// Parse command-line arguments. Wraps `clap::Parser::parse()`.
    pub fn parse_args() -> Self {
        <Self as Parser>::parse()
    }

    /// Convert the parsed CLI arguments into a `yt_dlp_core::config::Config`.
    pub fn to_config(&self) -> Config {
        Config {
            general: GeneralConfig {
                verbose: self.general.verbose > 0,
                quiet: self.general.quiet,
                simulate: self.general.simulate || self.general.dump_json,
                skip_download: self.general.skip_download,
                print_json: self.general.dump_json,
                no_warnings: self.general.no_warnings,
                ignore_errors: self.general.ignore_errors,
                abort_on_error: self.general.abort_on_error,
                no_overwrites: self.general.no_overwrites,
            },
            network: NetworkConfig {
                proxy: self.network.proxy.clone(),
                socket_timeout: self.network.socket_timeout,
                source_address: self.network.source_address.clone(),
                force_ipv4: self.network.force_ipv4,
                force_ipv6: self.network.force_ipv6,
                impersonate: self.network.impersonate.clone(),
            },
            download: DownloadConfig {
                rate_limit: None, // TODO: parse human-readable rate string
                retries: self.download.retries,
                fragment_retries: self.download.fragment_retries,
                concurrent_fragments: self.download.concurrent_fragments,
                buffer_size: None, // TODO: parse human-readable size string
                external_downloader: self.download.external_downloader.clone(),
                external_downloader_args: Default::default(),
            },
            output: OutputConfig {
                output_template: self
                    .filesystem
                    .output
                    .clone()
                    .unwrap_or_else(|| "%(title)s [%(id)s].%(ext)s".to_string()),
                output_dir: None,
                restrict_filenames: self.filesystem.restrict_filenames,
                windows_filenames: self.filesystem.windows_filenames,
                paths: self.parse_paths(),
                cookies_file: self.filesystem.cookies.clone(),
                cookies_from_browser: self.filesystem.cookies_from_browser.clone(),
                cache_dir: self.filesystem.cache_dir.clone(),
                no_cache: self.filesystem.no_cache_dir,
            },
            auth: AuthConfig {
                username: self.auth.username.clone(),
                password: self.auth.password.clone(),
                twofactor: self.auth.twofactor.clone(),
                netrc: self.auth.netrc,
                netrc_location: self.auth.netrc_location.clone(),
                client_certificate: self.auth.client_certificate.clone(),
                client_certificate_key: self.auth.client_certificate_key.clone(),
                client_certificate_password: self.auth.client_certificate_password.clone(),
            },
            format_selection: FormatSelectionConfig {
                format: self.format.format.clone(),
                format_sort: self.format.format_sort.clone(),
                format_sort_force: self.format.format_sort_force,
                prefer_free_formats: self.format.prefer_free_formats,
                merge_output_format: self.format.merge_output_format.clone(),
                audio_format: self
                    .format
                    .audio_format
                    .clone()
                    .or_else(|| self.postprocessing.pp_audio_format.clone()),
                audio_quality: self
                    .format
                    .audio_quality
                    .clone()
                    .or_else(|| self.postprocessing.pp_audio_quality.clone()),
                remux_video: self.format.remux_video.clone(),
                recode_video: self.format.recode_video.clone(),
            },
            subtitle: SubtitleConfig {
                write_subtitles: self.subtitle.write_subs,
                write_auto_subtitles: self.subtitle.write_auto_subs,
                all_subtitles: self.subtitle.all_subs,
                subtitle_languages: self
                    .subtitle
                    .sub_langs
                    .as_deref()
                    .map(|s| s.split(',').map(|l| l.trim().to_string()).collect())
                    .unwrap_or_default(),
                subtitle_format: self.subtitle.sub_format.clone(),
            },
            postprocessing: PostProcessingConfig {
                extract_audio: self.postprocessing.extract_audio,
                embed_subtitles: self.postprocessing.embed_subs,
                embed_thumbnail: self.postprocessing.embed_thumbnail,
                embed_metadata: self.postprocessing.embed_metadata,
                embed_chapters: self.postprocessing.embed_chapters,
                embed_info_json: self.postprocessing.embed_info_json,
                sponsorblock_mark: self
                    .postprocessing
                    .sponsorblock_mark
                    .as_deref()
                    .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
                    .unwrap_or_default(),
                sponsorblock_remove: self
                    .postprocessing
                    .sponsorblock_remove
                    .as_deref()
                    .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
                    .unwrap_or_default(),
                ffmpeg_location: self.postprocessing.ffmpeg_location.clone(),
                exec_cmd: self.postprocessing.exec.clone(),
            },
        }
    }

    /// Parse `--paths TYPE:PATH` arguments into a HashMap.
    fn parse_paths(&self) -> std::collections::HashMap<String, PathBuf> {
        let mut map = std::collections::HashMap::new();
        for entry in &self.filesystem.paths {
            if let Some((type_key, path)) = entry.split_once(':') {
                map.insert(type_key.to_string(), PathBuf::from(path));
            }
        }
        map
    }
}
