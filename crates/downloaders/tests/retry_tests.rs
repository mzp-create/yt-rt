use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use yt_dlp_downloaders::retry::{with_retry, RetryConfig};

#[tokio::test]
async fn test_retry_succeeds_on_third_attempt() {
    let attempts = AtomicU32::new(0);
    let config = RetryConfig {
        max_retries: 5,
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };
    let result = with_retry(&config, || {
        let n = attempts.fetch_add(1, Ordering::SeqCst);
        async move {
            if n < 2 {
                anyhow::bail!("simulated failure")
            }
            Ok(42)
        }
    })
    .await;
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_exhausts_retries() {
    let attempts = AtomicU32::new(0);
    let config = RetryConfig {
        max_retries: 3,
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };
    let result: anyhow::Result<i32> = with_retry(&config, || {
        attempts.fetch_add(1, Ordering::SeqCst);
        async { anyhow::bail!("always fails") }
    })
    .await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("failed after 3 retries"),
        "unexpected error: {err_msg}"
    );
    // initial attempt + 3 retries = 4 total attempts
    assert_eq!(attempts.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn test_retry_succeeds_immediately() {
    let attempts = AtomicU32::new(0);
    let config = RetryConfig {
        max_retries: 5,
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };
    let result = with_retry(&config, || {
        attempts.fetch_add(1, Ordering::SeqCst);
        async { Ok("success") }
    })
    .await;
    assert_eq!(result.unwrap(), "success");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
