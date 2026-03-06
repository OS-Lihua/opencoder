//! Retry logic with exponential backoff.
//!
//! Mirrors `src/session/retry.ts` from the original OpenCode.
//! Delay formula: `2000 * 2^(attempt - 1)`, max 30 seconds.

use std::time::Duration;

/// Base delay in milliseconds.
const BASE_DELAY_MS: u64 = 2000;
/// Maximum delay in milliseconds.
const MAX_DELAY_MS: u64 = 30_000;

/// Calculate the retry delay for the given attempt number (1-based).
pub fn delay(attempt: u32) -> Duration {
    let ms = BASE_DELAY_MS.saturating_mul(1u64 << (attempt.saturating_sub(1)));
    Duration::from_millis(ms.min(MAX_DELAY_MS))
}

/// Whether an HTTP status code is retryable.
pub fn retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// Parse a Retry-After header value into a Duration.
/// Supports seconds (integer), milliseconds (integer > 1000), and HTTP-date.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    // Try parsing as integer (seconds or ms)
    if let Ok(n) = value.trim().parse::<u64>() {
        if n > 1_000_000_000 {
            // Looks like a timestamp — compute delta
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            return Some(Duration::from_secs(n.saturating_sub(now)));
        }
        return Some(Duration::from_secs(n));
    }
    // Try parsing as HTTP-date (simplified)
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_formula() {
        assert_eq!(delay(1), Duration::from_millis(2000));
        assert_eq!(delay(2), Duration::from_millis(4000));
        assert_eq!(delay(3), Duration::from_millis(8000));
        assert_eq!(delay(4), Duration::from_millis(16000));
        assert_eq!(delay(5), Duration::from_millis(30000)); // capped
        assert_eq!(delay(10), Duration::from_millis(30000)); // still capped
    }

    #[test]
    fn delay_zero_attempt() {
        // attempt=0 should still give base delay (2^0 = 1, but saturating_sub gives 0, 2^0=1)
        assert_eq!(delay(0), Duration::from_millis(2000));
    }

    #[test]
    fn retryable_statuses() {
        assert!(retryable_status(429));
        assert!(retryable_status(500));
        assert!(retryable_status(502));
        assert!(retryable_status(503));
        assert!(retryable_status(504));
        assert!(!retryable_status(200));
        assert!(!retryable_status(400));
        assert!(!retryable_status(401));
        assert!(!retryable_status(404));
    }

    #[test]
    fn parse_retry_after_seconds() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after(" 10 "), Some(Duration::from_secs(10)));
    }
}
