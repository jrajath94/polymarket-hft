// Token bucket rate limiter for CLOB API calls.
//
// Polymarket rate limits: 500 orders/10s, 1500 book reads/10s, 9000 general/10s.
// We use a token bucket that refills continuously rather than windowed counting,
// which avoids the "burst at window boundary" problem.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use parking_lot::Mutex;

use crate::error::{AppError, Result};

/// Token bucket rate limiter.
pub struct RateLimiter {
    /// Max tokens in the bucket (= burst capacity)
    capacity: u32,
    /// Current token count (scaled by 1000 to avoid floats)
    tokens_x1000: Mutex<u64>,
    /// Refill rate: tokens per second * 1000
    refill_rate_x1000: u64,
    /// Last refill timestamp
    last_refill: Mutex<Instant>,
    /// Total requests made (for metrics)
    total_requests: AtomicU64,
    /// Total requests rejected (for metrics)
    total_rejected: AtomicU64,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// `max_per_window`: maximum requests allowed in the window
    /// `window_secs`: window duration (e.g., 10 for "500/10s")
    pub fn new(max_per_window: u32, window_secs: u32) -> Self {
        let refill_rate = (max_per_window as f64) / (window_secs as f64);
        Self {
            capacity: max_per_window,
            tokens_x1000: Mutex::new((max_per_window as u64) * 1000),
            refill_rate_x1000: (refill_rate * 1000.0) as u64,
            last_refill: Mutex::new(Instant::now()),
            total_requests: AtomicU64::new(0),
            total_rejected: AtomicU64::new(0),
        }
    }

    /// Try to acquire `n` tokens. Returns Ok(()) if allowed, Err if rate limited.
    pub fn try_acquire(&self, n: u32) -> Result<()> {
        self.refill();
        let needed = (n as u64) * 1000;

        let mut tokens = self.tokens_x1000.lock();
        if *tokens >= needed {
            *tokens -= needed;
            self.total_requests.fetch_add(n as u64, Ordering::Relaxed);
            Ok(())
        } else {
            self.total_rejected.fetch_add(n as u64, Ordering::Relaxed);
            Err(AppError::RateLimit(format!(
                "rate limit exceeded: need {} tokens, have {}",
                n,
                *tokens / 1000
            )))
        }
    }

    /// Refill tokens based on elapsed time.
    fn refill(&self) {
        let mut last = self.last_refill.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(*last);
        let elapsed_ms = elapsed.as_millis() as u64;

        if elapsed_ms > 0 {
            let new_tokens = (self.refill_rate_x1000 * elapsed_ms) / 1000;
            let mut tokens = self.tokens_x1000.lock();
            let max = (self.capacity as u64) * 1000;
            *tokens = (*tokens + new_tokens).min(max);
            *last = now;
        }
    }

    /// Time until `n` tokens will be available.
    pub fn time_until_available(&self, n: u32) -> Duration {
        self.refill();
        let needed = (n as u64) * 1000;
        let tokens = *self.tokens_x1000.lock();

        if tokens >= needed {
            Duration::ZERO
        } else {
            let deficit = needed - tokens;
            let ms = (deficit * 1000) / self.refill_rate_x1000;
            Duration::from_millis(ms + 1) // +1 to avoid rounding down
        }
    }

    /// Get metrics for monitoring.
    pub fn metrics(&self) -> (u64, u64) {
        (
            self.total_requests.load(Ordering::Relaxed),
            self.total_rejected.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_limiter_allows_within_capacity() {
        let limiter = RateLimiter::new(500, 10);
        // Should allow up to 500 requests immediately (full bucket)
        for _ in 0..500 {
            assert!(limiter.try_acquire(1).is_ok());
        }
    }

    #[test]
    fn test_limiter_rejects_over_capacity() {
        let limiter = RateLimiter::new(10, 10);
        // Drain all tokens
        for _ in 0..10 {
            limiter.try_acquire(1).unwrap();
        }
        // Next should be rejected
        assert!(limiter.try_acquire(1).is_err());
    }

    #[test]
    fn test_limiter_refills_over_time() {
        let limiter = RateLimiter::new(10, 10);
        // Drain all tokens
        for _ in 0..10 {
            limiter.try_acquire(1).unwrap();
        }
        assert!(limiter.try_acquire(1).is_err());

        // Wait for refill (1 token/sec at 10/10s rate)
        thread::sleep(Duration::from_millis(1100));
        assert!(limiter.try_acquire(1).is_ok());
    }

    #[test]
    fn test_limiter_batch_acquire() {
        let limiter = RateLimiter::new(100, 10);
        assert!(limiter.try_acquire(15).is_ok()); // Batch of 15
        assert!(limiter.try_acquire(85).is_ok()); // 85 more
        assert!(limiter.try_acquire(1).is_err()); // Over capacity
    }

    #[test]
    fn test_limiter_time_until_available() {
        let limiter = RateLimiter::new(10, 10);
        // Fresh limiter, should be available immediately
        assert_eq!(limiter.time_until_available(1), Duration::ZERO);

        // Drain all
        limiter.try_acquire(10).unwrap();
        let wait = limiter.time_until_available(1);
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_secs(2));
    }

    #[test]
    fn test_limiter_metrics() {
        let limiter = RateLimiter::new(5, 10);
        limiter.try_acquire(3).unwrap();
        limiter.try_acquire(2).unwrap();
        let _ = limiter.try_acquire(1); // Rejected

        let (total, rejected) = limiter.metrics();
        assert_eq!(total, 5);
        assert_eq!(rejected, 1);
    }

    #[test]
    fn test_limiter_never_exceeds_capacity() {
        let limiter = RateLimiter::new(10, 10);
        // Even after sleeping, should not accumulate more than capacity
        thread::sleep(Duration::from_millis(100));
        let mut count = 0;
        while limiter.try_acquire(1).is_ok() {
            count += 1;
            if count > 20 {
                break; // Safety: prevent infinite loop
            }
        }
        assert!(count <= 10);
    }
}
