use std::time::Instant;
use yt_dlp_downloaders::rate_limiter::RateLimiter;

#[tokio::test]
async fn test_rate_limiter_throttles() {
    let mut limiter = RateLimiter::new(1000); // 1000 bytes/sec

    // First acquire consumes the initial burst tokens, so we drain them first
    limiter.acquire(1000).await;

    let start = Instant::now();
    // Now the bucket is empty -- this should sleep ~1 second to refill
    limiter.acquire(1000).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() >= 800,
        "expected >= 800ms of throttling, got {}ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn test_rate_limiter_no_throttle_under_limit() {
    let mut limiter = RateLimiter::new(10_000); // 10KB/sec

    let start = Instant::now();
    // Acquire a small amount that fits within initial burst
    limiter.acquire(100).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 100,
        "should not throttle small requests within burst, took {}ms",
        elapsed.as_millis()
    );
}
