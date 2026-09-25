//! Process-wide request throttle shared by every active scrape job.
//!
//! The per-domain delay (`rate_limit`/`delay_ms` + jitter), robots.txt
//! `Crawl-delay` and exponential backoff are unchanged. This guard adds one
//! more layer on top: a single token bucket for the whole process, so N
//! concurrent jobs can never emit more than `SHINOBI_GLOBAL_RPS` requests per
//! second to scrape targets.
//!
//! * `SHINOBI_GLOBAL_RPS` — requests/second ceiling (default 5). `0` disables
//!   the throttle entirely.
//! * The bucket is lazily initialised once and shared via
//!   [`global_throttle()`], so it covers every job in the process.
//! * Only *target* fetches are throttled (pages, robots.txt, sitemap.xml and
//!   the JS-renderer path). Local sidecar calls (extractor on localhost,
//!   webhooks) are not scraping traffic and are intentionally excluded.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tracing::debug;

/// Requests-per-second ceiling applied when `SHINOBI_GLOBAL_RPS` is unset.
pub const DEFAULT_GLOBAL_RPS: f64 = 5.0;

/// Environment variable controlling the process-wide rate limit (`0` = off).
pub const GLOBAL_RPS_ENV: &str = "SHINOBI_GLOBAL_RPS";

/// Token-bucket decision policy with an injected clock, so the rate math is
/// unit-testable without sleeping (same pattern as the scheduler tests).
pub struct ThrottlePolicy {
    rate_per_sec: f64,
    /// One second worth of tokens (min 1) so an idle process may emit a short
    /// burst, then settles at exactly `rate_per_sec`.
    capacity: f64,
    tokens: f64,
    /// Monotonic millisecond counter of the last refill (epoch is arbitrary;
    /// only deltas matter).
    last_refill_ms: u64,
}

impl ThrottlePolicy {
    pub fn new(rate_per_sec: f64) -> Self {
        let rate = rate_per_sec.max(0.0);
        let capacity = rate.max(1.0);
        Self {
            rate_per_sec: rate,
            capacity,
            tokens: capacity,
            last_refill_ms: 0,
        }
    }

    /// `true` when every request is admitted immediately.
    pub fn disabled(&self) -> bool {
        self.rate_per_sec <= 0.0
    }

    /// Requests/second ceiling (0 when disabled).
    pub fn rate_per_sec(&self) -> f64 {
        self.rate_per_sec
    }

    /// Consumes one token at clock time `now_ms`, returning how long the
    /// caller must wait before sending. Pure function: no system clock, no IO.
    ///
    /// When the bucket is empty the token that will arrive during the wait is
    /// reserved immediately (`tokens` may go slightly negative), otherwise the
    /// waiter and the next caller would both consume the same refill and the
    /// effective rate would double.
    pub fn acquire(&mut self, now_ms: u64) -> Duration {
        if self.disabled() {
            return Duration::ZERO;
        }
        let elapsed_secs = now_ms.saturating_sub(self.last_refill_ms) as f64 / 1000.0;
        self.tokens = (self.tokens + elapsed_secs * self.rate_per_sec).min(self.capacity);
        self.last_refill_ms = now_ms;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Duration::ZERO
        } else {
            // Wait long enough for one token, reserving it right away.
            let wait_ms = ((1.0 - self.tokens) / self.rate_per_sec * 1000.0).ceil() as u64;
            self.tokens -= 1.0;
            Duration::from_millis(wait_ms)
        }
    }
}

/// Process-wide limiter: one instance is created lazily and shared by every
/// [`crate::scraper::client::ScrapeClient`], regardless of job.
#[derive(Clone)]
pub struct GlobalThrottle {
    inner: Arc<Inner>,
}

struct Inner {
    rate_per_sec: f64,
    policy: Mutex<ThrottlePolicy>,
}

impl GlobalThrottle {
    /// Builds the throttle from `SHINOBI_GLOBAL_RPS` (default 5; 0 disables).
    pub fn from_env() -> Self {
        let rate = std::env::var(GLOBAL_RPS_ENV)
            .ok()
            .and_then(|value| parse_rate(&value))
            .unwrap_or(DEFAULT_GLOBAL_RPS);
        Self::new(rate)
    }

    pub fn new(rate_per_sec: f64) -> Self {
        let rate = rate_per_sec.max(0.0);
        Self {
            inner: Arc::new(Inner {
                rate_per_sec: rate,
                policy: Mutex::new(ThrottlePolicy::new(rate)),
            }),
        }
    }

    pub fn rate_per_sec(&self) -> f64 {
        self.inner.rate_per_sec
    }

    pub fn disabled(&self) -> bool {
        self.inner.rate_per_sec <= 0.0
    }

    /// Blocks until a global token is available. No-op when disabled.
    ///
    /// Single-shot: [`ThrottlePolicy::acquire`] reserves the token up front
    /// and returns the exact wait until it is available, so after sleeping the
    /// caller may proceed immediately (re-checking here would reserve a second
    /// token and starve the bucket forever).
    pub async fn acquire(&self) {
        if self.disabled() {
            return;
        }
        let wait = {
            let mut policy = self.inner.policy.lock().await;
            policy.acquire(now_ms())
        };
        if !wait.is_zero() {
            debug!("global throttle: holding request for {:?}", wait);
            tokio::time::sleep(wait).await;
        }
    }
}

/// Lazily-initialised process-wide throttle shared by all jobs.
pub fn global_throttle() -> &'static GlobalThrottle {
    static THROTTLE: std::sync::OnceLock<GlobalThrottle> = std::sync::OnceLock::new();
    THROTTLE.get_or_init(GlobalThrottle::from_env)
}

/// Parses the env value: a finite, non-negative rate, or `None` for invalid
/// input (callers then fall back to the default).
fn parse_rate(value: &str) -> Option<f64> {
    let rate = value.trim().parse::<f64>().ok()?;
    if rate.is_nan() || rate.is_infinite() {
        return None;
    }
    Some(rate.max(0.0))
}

/// Monotonic millisecond counter anchored to process start. The epoch is
/// irrelevant (only deltas matter), so no wall-clock drift concerns.
fn now_ms() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_holds_rate_over_time() {
        // 5 rps -> one request every 200 ms after the initial burst.
        let mut policy = ThrottlePolicy::new(5.0);
        let mut now = 1_000u64;

        // Burst headroom: capacity (5) tokens are available immediately.
        for _ in 0..5 {
            assert_eq!(policy.acquire(now), Duration::ZERO);
        }

        // 6th request must wait one full interval.
        let wait = policy.acquire(now);
        assert_eq!(wait, Duration::from_millis(200));

        // After the wait elapses the reserved token is spent; steady state
        // keeps one interval between requests.
        now += 200;
        assert_eq!(policy.acquire(now), Duration::from_millis(200));
        now += 200;
        assert_eq!(policy.acquire(now), Duration::from_millis(200));
    }

    #[test]
    fn steady_state_allows_exactly_rate_per_second() {
        // 10 rps: 100 requests over 10 s -> 90 waits of exactly 100 ms plus
        // the 10-token initial burst.
        let mut policy = ThrottlePolicy::new(10.0);
        let mut now = 0u64;
        let mut waits = 0usize;
        for i in 0..100u64 {
            let wait = policy.acquire(now);
            if wait.is_zero() {
                assert!(i < 10, "unexpected free token at request {i}");
            } else {
                waits += 1;
                assert_eq!(wait, Duration::from_millis(100));
                now += 100;
            }
        }
        assert_eq!(waits, 90);
    }

    #[test]
    fn zero_rate_disables_the_throttle() {
        let mut policy = ThrottlePolicy::new(0.0);
        for _ in 0..1_000 {
            assert_eq!(policy.acquire(42), Duration::ZERO);
        }
    }

    #[test]
    fn bucket_refills_after_idle_time() {
        let mut policy = ThrottlePolicy::new(2.0);
        // Consume the full 2-token burst.
        assert_eq!(policy.acquire(0), Duration::ZERO);
        assert_eq!(policy.acquire(0), Duration::ZERO);
        // Third request at the same instant has to wait 500 ms.
        assert_eq!(policy.acquire(0), Duration::from_millis(500));
        // Let 2 seconds of idle time pass: bucket refills to capacity (2).
        assert_eq!(policy.acquire(2_000), Duration::ZERO);
        assert_eq!(policy.acquire(2_000), Duration::ZERO);
        assert_eq!(policy.acquire(2_000), Duration::from_millis(500));
    }

    #[test]
    fn factory_reports_rate_and_disabled_state() {
        let active = GlobalThrottle::new(5.0);
        assert_eq!(active.rate_per_sec(), 5.0);
        assert!(!active.disabled());

        let disabled = GlobalThrottle::new(0.0);
        assert!(disabled.disabled());
        assert_eq!(disabled.rate_per_sec(), 0.0);

        // Negative input clamps to disabled instead of panicking.
        assert!(GlobalThrottle::new(-1.0).disabled());
    }

    #[test]
    fn env_value_parsing_accepts_only_finite_non_negative_rates() {
        assert_eq!(parse_rate("5"), Some(5.0));
        assert_eq!(parse_rate(" 2.5 "), Some(2.5));
        assert_eq!(parse_rate("0"), Some(0.0));
        // Negative clamps to disabled, garbage yields None (default fallback).
        assert_eq!(parse_rate("-3"), Some(0.0));
        assert_eq!(parse_rate(""), None);
        assert_eq!(parse_rate("abc"), None);
        assert_eq!(parse_rate("NaN"), None);
        assert_eq!(parse_rate("inf"), None);
    }

    #[tokio::test]
    async fn disabled_throttle_acquires_instantly() {
        let throttle = GlobalThrottle::new(0.0);
        let start = Instant::now();
        for _ in 0..100 {
            throttle.acquire().await;
        }
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "disabled throttle must not block"
        );
    }

    #[tokio::test]
    async fn async_throttle_enforces_rate_in_real_time() {
        // 20 rps: 25 sequential requests -> 20-token burst + 5 * 50 ms.
        let throttle = GlobalThrottle::new(20.0);
        let start = Instant::now();
        for _ in 0..25 {
            throttle.acquire().await;
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(240),
            "25 requests at 20/s should take >= 240 ms, took {elapsed:?}"
        );
    }
}
