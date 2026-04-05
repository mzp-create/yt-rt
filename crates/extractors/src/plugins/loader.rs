use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::InfoExtractor;

pub struct PluginLoader {
    search_dirs: Vec<PathBuf>,
}

impl PluginLoader {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        // ~/.config/yt-dlp-rs/plugins/extractors/
        if let Some(config) = dirs::config_dir() {
            dirs.push(config.join("yt-dlp-rs").join("plugins").join("extractors"));
        }
        // $YT_DLP_RS_PLUGIN_PATH
        if let Ok(path) = std::env::var("YT_DLP_RS_PLUGIN_PATH") {
            for p in path.split(':') {
                dirs.push(PathBuf::from(p));
            }
        }
        Self { search_dirs: dirs }
    }

    pub fn with_dirs(dirs: Vec<PathBuf>) -> Self {
        Self { search_dirs: dirs }
    }

    /// Discover all `.js` plugin files in the configured search directories.
    pub fn discover(&self) -> Vec<PathBuf> {
        let mut plugins = Vec::new();
        for dir in &self.search_dirs {
            if dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map_or(false, |e| e == "js") {
                            debug!(path = %path.display(), "found plugin");
                            plugins.push(path);
                        }
                    }
                }
            }
        }
        info!(count = plugins.len(), "discovered extractor plugins");
        plugins
    }

    /// Load all discovered plugins into `JsExtractor` instances.
    pub fn load_all(&self) -> Vec<super::js_extractor::JsExtractor> {
        self.discover()
            .into_iter()
            .filter_map(|path| {
                match super::js_extractor::JsExtractor::from_file(&path) {
                    Ok(ext) => {
                        info!(name = ext.name(), path = %path.display(), "loaded plugin");
                        Some(ext)
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "failed to load plugin");
                        None
                    }
                }
            })
            .collect()
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}
