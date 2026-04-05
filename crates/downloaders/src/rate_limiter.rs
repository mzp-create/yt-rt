//! Token bucket rate limiter for download speed throttling.

use tokio::time::Instant;

/// A token-bucket rate limiter that throttles throughput to a configured bytes-per-second rate.
pub struct RateLimiter {
    rate: u64,
    tokens: f64,
    last_check: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter that allows `bytes_per_second` throughput.
    pub fn new(bytes_per_second: u64) -> Self {
        Self {
            rate: bytes_per_second,
            tokens: bytes_per_second as f64,
            last_check: Instant::now(),
        }
    }

    /// Acquire permission to transfer `bytes` bytes, sleeping if necessary to
    /// stay within the configured rate.
    pub async fn acquire(&mut self, bytes: usize) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_check).as_secs_f64();
        self.last_check = now;

        // Replenish tokens based on elapsed time
        self.tokens += elapsed * self.rate as f64;
        // Cap tokens at the rate (1 second worth of burst)
        if self.tokens > self.rate as f64 {
            self.tokens = self.rate as f64;
        }

        self.tokens -= bytes as f64;

        // If we've gone negative, sleep to let tokens accumulate
        if self.tokens < 0.0 {
            let sleep_secs = -self.tokens / self.rate as f64;
            tokio::time::sleep(std::time::Duration::from_secs_f64(sleep_secs)).await;
            self.last_check = Instant::now();
            self.tokens = 0.0;
        }
    }
}
