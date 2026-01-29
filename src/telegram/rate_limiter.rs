//! Rate limiter for Telegram API calls.
//!
//! Implements a simple rate limiter to avoid triggering Telegram's
//! flood wait errors when updating the profile bio.

use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Rate limiter that enforces minimum intervals between operations.
#[derive(Debug)]
pub struct RateLimiter {
    /// Minimum duration between allowed operations.
    min_interval: Duration,

    /// Last time an operation was performed.
    last_operation: Mutex<Option<Instant>>,
}

impl RateLimiter {
    /// Creates a new rate limiter with the specified minimum interval.
    #[must_use]
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last_operation: Mutex::new(None),
        }
    }

    /// Creates a rate limiter from seconds.
    #[must_use]
    pub fn from_secs(secs: u64) -> Self {
        Self::new(Duration::from_secs(secs))
    }

    /// Waits until an operation is allowed, then marks the operation as performed.
    ///
    /// Returns the duration waited (0 if no wait was needed).
    pub async fn wait_and_acquire(&self) -> Duration {
        let mut last = self.last_operation.lock().await;

        let wait_duration = if let Some(last_time) = *last {
            let elapsed = last_time.elapsed();
            if elapsed < self.min_interval {
                self.min_interval - elapsed
            } else {
                Duration::ZERO
            }
        } else {
            Duration::ZERO
        };

        if !wait_duration.is_zero() {
            debug!(
                "Rate limiter: waiting {:?} before next operation",
                wait_duration
            );
            tokio::time::sleep(wait_duration).await;
        }

        *last = Some(Instant::now());
        wait_duration
    }

    /// Checks if an operation is currently allowed without blocking.
    pub async fn is_allowed(&self) -> bool {
        let last = self.last_operation.lock().await;
        match *last {
            Some(last_time) => last_time.elapsed() >= self.min_interval,
            None => true,
        }
    }

    /// Marks an operation as just performed (non-blocking).
    pub async fn mark_used(&self) {
        let mut last = self.last_operation.lock().await;
        *last = Some(Instant::now());
    }

    /// Returns the time remaining until the next operation is allowed.
    pub async fn time_until_allowed(&self) -> Duration {
        let last = self.last_operation.lock().await;
        match *last {
            Some(last_time) => {
                let elapsed = last_time.elapsed();
                if elapsed >= self.min_interval {
                    Duration::ZERO
                } else {
                    self.min_interval - elapsed
                }
            }
            None => Duration::ZERO,
        }
    }

    /// Handles a flood wait error from Telegram by updating the wait time.
    pub async fn handle_flood_wait(&self, wait_seconds: u32) {
        warn!(
            "Received flood wait from Telegram: {} seconds",
            wait_seconds
        );
        // We'll need to wait at least this long before the next operation
        tokio::time::sleep(Duration::from_secs(u64::from(wait_seconds))).await;

        // Mark as just performed so the rate limiter knows to wait
        let mut last = self.last_operation.lock().await;
        *last = Some(Instant::now());
    }

    /// Resets the rate limiter, allowing immediate operation.
    pub async fn reset(&self) {
        let mut last = self.last_operation.lock().await;
        *last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_first_operation() {
        let limiter = RateLimiter::from_secs(1);
        assert!(limiter.is_allowed().await);

        let waited = limiter.wait_and_acquire().await;
        assert_eq!(waited, Duration::ZERO);
    }

    #[tokio::test]
    async fn test_rate_limiter_subsequent_operation() {
        let limiter = RateLimiter::new(Duration::from_millis(100));

        // First operation
        limiter.wait_and_acquire().await;

        // Should not be immediately allowed
        assert!(!limiter.is_allowed().await);

        // Time until allowed should be positive
        let remaining = limiter.time_until_allowed().await;
        assert!(remaining > Duration::ZERO);
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let limiter = RateLimiter::new(Duration::from_secs(60));

        limiter.wait_and_acquire().await;
        assert!(!limiter.is_allowed().await);

        limiter.reset().await;
        assert!(limiter.is_allowed().await);
    }
}
