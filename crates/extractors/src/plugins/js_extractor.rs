use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use anyhow::Context;
use regex::Regex;

use yt_dlp_core::types::{ExtractionResult, InfoDict};
use yt_dlp_jsinterp::JsInterpreter;
use yt_dlp_networking::client::HttpClient;

use crate::InfoExtractor;

/// An extractor implemented in JavaScript, executed via the Boa engine.
///
/// Plugin JS files must define the following globals:
///
/// ```js
/// var EXTRACTOR_NAME = "ExampleSite";
/// var EXTRACTOR_KEY  = "ExampleSite";
/// var SUITABLE_URLS  = ["https?://(?:www\\.)?example\\.com/video/([a-zA-Z0-9]+)"];
///
/// function extract(url) {
///     // Return a JSON string with InfoDict-like structure
///     return JSON.stringify({ id: "abc", title: "Example", formats: [] });
/// }
/// ```
pub struct JsExtractor {
    name: String,
    key: String,
    url_patterns: Vec<String>,
    source_code: String,
}

impl JsExtractor {
    /// Load a JS extractor from a file on disk.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("reading plugin {}", path.display()))?;
        Self::from_source(source)
    }

    /// Create from JS source code.
    pub fn from_source(source: String) -> anyhow::Result<Self> {
        // Execute the JS to extract metadata (name, key, url patterns).
        let mut js = JsInterpreter::new();
        js.load(&source)?;

        let name = js
            .execute("EXTRACTOR_NAME")
            .unwrap_or_else(|_| "Unknown".to_string());
        let key = js
            .execute("EXTRACTOR_KEY")
            .unwrap_or_else(|_| name.clone());

        // Get URL patterns as a JSON array.
        let patterns_json = js.execute("JSON.stringify(SUITABLE_URLS)")?;
        let url_patterns: Vec<String> = serde_json::from_str(&patterns_json)
            .context("SUITABLE_URLS must be a JSON array of strings")?;

        Ok(Self {
            name,
            key,
            url_patterns,
            source_code: source,
        })
    }
}

impl InfoExtractor for JsExtractor {
    fn name(&self) -> &str {
        &self.name
    }

    fn key(&self) -> &str {
        &self.key
    }

    fn suitable_urls(&self) -> &[&str] {
        // We cannot return &[&str] for dynamically loaded patterns.
        // The `suitable()` override below handles URL matching instead.
        &[]
    }

    fn suitable(&self, url: &str) -> bool {
        self.url_patterns.iter().any(|pattern| {
            Regex::new(pattern).map_or(false, |re| re.is_match(url))
        })
    }

    fn extract<'a>(
        &'a self,
        url: &'a str,
        _client: &'a HttpClient,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<ExtractionResult>> + Send + 'a>> {
        Box::pin(async move {
            // Create a fresh interpreter for each extraction (isolation).
            let mut js = JsInterpreter::new();
            js.load(&self.source_code)?;

            // Call the extract(url) function defined by the plugin.
            let result_json = js.call_function("extract", &[url])?;

            // Parse the result as an InfoDict.
            let info: InfoDict = serde_json::from_str(&result_json)
                .context("plugin returned invalid InfoDict JSON")?;

            Ok(ExtractionResult::SingleVideo(Box::new(info)))
        })
    }
}
