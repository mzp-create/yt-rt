use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub network: NetworkConfig,
    pub download: DownloadConfig,
    pub output: OutputConfig,
    pub auth: AuthConfig,
    pub format_selection: FormatSelectionConfig,
    pub postprocessing: PostProcessingConfig,
    pub subtitle: SubtitleConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeneralConfig {
    pub verbose: bool,
    pub quiet: bool,
    pub simulate: bool,
    pub skip_download: bool,
    pub print_json: bool,
    pub no_warnings: bool,
    pub ignore_errors: bool,
    pub abort_on_error: bool,
    pub no_overwrites: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    pub proxy: Option<String>,
    pub socket_timeout: Option<u64>,
    pub source_address: Option<String>,
    pub force_ipv4: bool,
    pub force_ipv6: bool,
    pub impersonate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadConfig {
    pub rate_limit: Option<u64>,
    pub retries: u32,
    pub fragment_retries: u32,
    pub concurrent_fragments: u32,
    pub buffer_size: Option<u64>,
    pub external_downloader: Option<String>,
    pub external_downloader_args: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub output_template: String,
    pub output_dir: Option<PathBuf>,
    pub restrict_filenames: bool,
    pub windows_filenames: bool,
    pub paths: HashMap<String, PathBuf>,
    pub cookies_file: Option<PathBuf>,
    pub cookies_from_browser: Option<String>,
    pub cache_dir: Option<PathBuf>,
    pub no_cache: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            output_template: "%(title)s [%(id)s].%(ext)s".to_string(),
            output_dir: None,
            restrict_filenames: false,
            windows_filenames: false,
            paths: HashMap::new(),
            cookies_file: None,
            cookies_from_browser: None,
            cache_dir: None,
            no_cache: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    pub username: Option<String>,
    pub password: Option<String>,
    pub twofactor: Option<String>,
    pub netrc: bool,
    pub netrc_location: Option<PathBuf>,
    pub client_certificate: Option<PathBuf>,
    pub client_certificate_key: Option<PathBuf>,
    pub client_certificate_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatSelectionConfig {
    pub format: Option<String>,
    pub format_sort: Vec<String>,
    pub format_sort_force: bool,
    pub prefer_free_formats: bool,
    pub merge_output_format: Option<String>,
    pub audio_format: Option<String>,
    pub audio_quality: Option<String>,
    pub remux_video: Option<String>,
    pub recode_video: Option<String>,
}

impl Default for FormatSelectionConfig {
    fn default() -> Self {
        Self {
            format: None,
            format_sort: vec![],
            format_sort_force: false,
            prefer_free_formats: false,
            merge_output_format: None,
            audio_format: None,
            audio_quality: None,
            remux_video: None,
            recode_video: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubtitleConfig {
    pub write_subtitles: bool,
    pub write_auto_subtitles: bool,
    pub all_subtitles: bool,
    pub subtitle_languages: Vec<String>,
    pub subtitle_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostProcessingConfig {
    pub extract_audio: bool,
    pub embed_subtitles: bool,
    pub embed_thumbnail: bool,
    pub embed_metadata: bool,
    pub embed_chapters: bool,
    pub embed_info_json: bool,
    pub sponsorblock_mark: Vec<String>,
    pub sponsorblock_remove: Vec<String>,
    pub ffmpeg_location: Option<PathBuf>,
    pub exec_cmd: Option<String>,
}

impl Config {
    /// Load config from file, merging with defaults.
    pub fn load(path: Option<&std::path::Path>) -> crate::error::Result<Self> {
        if let Some(path) = path {
            if path.exists() {
                let content = std::fs::read_to_string(path)?;
                let config: Config = toml::from_str(&content)
                    .map_err(|e| crate::error::YtDlpError::ConfigError(e.to_string()))?;
                return Ok(config);
            }
        }
        // Try default locations
        if let Some(config_dir) = dirs::config_dir() {
            let default_path = config_dir.join("yt-dlp-rs").join("config.toml");
            if default_path.exists() {
                let content = std::fs::read_to_string(default_path)?;
                let config: Config = toml::from_str(&content)
                    .map_err(|e| crate::error::YtDlpError::ConfigError(e.to_string()))?;
                return Ok(config);
            }
        }
        Ok(Config::default())
    }
}
