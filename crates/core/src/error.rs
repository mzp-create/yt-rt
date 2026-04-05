use thiserror::Error;

#[derive(Debug, Error)]
pub enum YtDlpError {
    #[error("extraction error for {url}: {message}")]
    ExtractionError { url: String, message: String },

    #[error("download error for {url}: {message}")]
    DownloadError { url: String, message: String },

    #[error("post-processing error: {message}")]
    PostProcessingError { message: String },

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("format selection error: {0}")]
    FormatSelectionError(String),

    #[error("authentication error: {0}")]
    AuthenticationError(String),

    #[error("geo-restriction error: {0}")]
    GeoRestrictionError(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
}

pub type Result<T> = std::result::Result<T, YtDlpError>;
