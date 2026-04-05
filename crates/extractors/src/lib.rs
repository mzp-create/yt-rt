pub mod bilibili;
pub mod dailymotion;
pub mod facebook;
pub mod generic;
pub mod instagram;
pub mod plugins;
pub mod reddit;
pub mod soundcloud;
pub mod tiktok;
pub mod twitch;
pub mod twitter;
pub mod vimeo;
pub mod youtube;

use yt_dlp_core::types::ExtractionResult;
use yt_dlp_networking::client::HttpClient;

use std::future::Future;
use std::pin::Pin;

/// Base trait all extractors must implement.
///
/// Uses `Pin<Box<dyn Future>>` for `extract` so that the trait remains object-safe
/// and can be used as `dyn InfoExtractor`.
pub trait InfoExtractor: Send + Sync {
    /// Human-readable name of the extractor.
    fn name(&self) -> &str;

    /// Unique key for the extractor.
    fn key(&self) -> &str;

    /// Regex patterns that match URLs this extractor handles.
    fn suitable_urls(&self) -> &[&str];

    /// Check if this extractor can handle the given URL.
    fn suitable(&self, url: &str) -> bool {
        self.suitable_urls().iter().any(|pattern| {
            regex::Regex::new(pattern).map_or(false, |re| re.is_match(url))
        })
    }

    /// Extract info from the URL.
    fn extract<'a>(
        &'a self,
        url: &'a str,
        client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>>;
}

/// Registry of all available extractors.
pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn InfoExtractor>>,
}

impl ExtractorRegistry {
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }

    pub fn register(&mut self, extractor: Box<dyn InfoExtractor>) {
        self.extractors.push(extractor);
    }

    /// Find the first extractor suitable for the URL.
    pub fn find_extractor(&self, url: &str) -> Option<&dyn InfoExtractor> {
        self.extractors
            .iter()
            .find(|e| e.suitable(url))
            .map(|e| e.as_ref())
    }

    pub fn list_extractors(&self) -> Vec<(&str, &str)> {
        self.extractors
            .iter()
            .map(|e| (e.key(), e.name()))
            .collect()
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create default registry with all built-in extractors.
pub fn create_default_registry() -> ExtractorRegistry {
    let mut registry = ExtractorRegistry::new();

    // Built-in extractors (site-specific before generic)
    registry.register(Box::new(youtube::extractor::YoutubeExtractor::new()));
    registry.register(Box::new(twitter::TwitterExtractor::new()));
    registry.register(Box::new(reddit::RedditExtractor::new()));
    registry.register(Box::new(vimeo::VimeoExtractor::new()));
    registry.register(Box::new(tiktok::TikTokExtractor::new()));
    registry.register(Box::new(instagram::InstagramExtractor::new()));
    registry.register(Box::new(twitch::TwitchExtractor::new()));
    registry.register(Box::new(dailymotion::DailymotionExtractor::new()));
    registry.register(Box::new(soundcloud::SoundCloudExtractor::new()));
    registry.register(Box::new(bilibili::BilibiliExtractor::new()));
    registry.register(Box::new(facebook::FacebookExtractor::new()));

    // Load JS plugins
    let loader = plugins::loader::PluginLoader::new();
    for plugin in loader.load_all() {
        registry.register(Box::new(plugin));
    }

    // Generic extractor MUST be last -- it matches any http/https URL
    registry.register(Box::new(generic::GenericExtractor::new()));

    registry
}
