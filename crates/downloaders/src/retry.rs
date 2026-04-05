//! Retry logic with exponential backoff.

use std::future::Future;
use std::time::Duration;
use tracing::warn;

/// Configuration for retry behaviour with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
}

impl RetryConfig {
    /// Default configuration for full-file downloads: 10 retries, 1s initial, 60s max, 2x backoff.
    pub fn default_download() -> Self {
        Self {
            max_retries: 10,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
        }
    }

    /// Default configuration for fragment downloads: 10 retries, 0.5s initial, 30s max, 2x backoff.
    pub fn default_fragment() -> Self {
        Self {
            max_retries: 10,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
        }
    }
}

/// Execute `operation` with retries according to `config`.
///
/// On each failure the delay is multiplied by `backoff_factor`, capped at `max_delay`.
/// If all retries are exhausted the last error is returned.
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, operation: F) -> anyhow::Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let mut delay = config.initial_delay;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                if attempt == config.max_retries {
                    return Err(err.context(format!(
                        "operation failed after {} retries",
                        config.max_retries
                    )));
                }
                warn!(
                    attempt = attempt + 1,
                    max = config.max_retries,
                    "retryable error: {err:#}, retrying in {delay:?}"
                );
                tokio::time::sleep(delay).await;
                let next = Duration::from_secs_f64(delay.as_secs_f64() * config.backoff_factor);
                delay = next.min(config.max_delay);
            }
        }
    }

    unreachable!("loop should have returned")
}
