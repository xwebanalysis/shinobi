//! Anti-blocking helpers: realistic headers, jittered delays, exponential
//! backoff with jitter and round-robin proxy rotation.
//!
//! Shinobi deliberately stays *polite*: concurrency is hard-capped, delays
//! carry ±20% jitter and retries use exponential backoff, so a target never
//! receives a synchronized burst and is unlikely to blacklist the crawler.

use std::sync::atomic::{AtomicUsize, Ordering};

use rand::RngExt;

/// Hard ceiling for concurrent page fetches, independent of user config.
pub const MAX_CONCURRENCY: usize = 3;

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36 Edg/125.0.0.0",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64; rv:127.0) Gecko/20100101 Firefox/127.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Mobile Safari/537.36",
];

/// Clamps a user-provided concurrency to `1..=MAX_CONCURRENCY`.
pub fn clamp_concurrency(requested: usize) -> usize {
    requested.clamp(1, MAX_CONCURRENCY)
}

/// Returns `base_ms` with ±20% random jitter (never overflows).
pub fn jitter_ms(base_ms: u64) -> u64 {
    if base_ms == 0 {
        return 0;
    }
    let spread = (base_ms / 5).max(1);
    let low = base_ms.saturating_sub(spread);
    let high = base_ms.saturating_add(spread);
    rand::rng().random_range(low..=high)
}

/// Next round-robin index for a proxy list of `len` entries.
pub fn next_proxy_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

/// Round-robin proxy selector with failure-driven rotation.
#[derive(Debug, Default)]
pub struct ProxyRotator {
    proxies: Vec<String>,
    index: AtomicUsize,
}

impl ProxyRotator {
    pub fn new(proxies: Vec<String>) -> Self {
        Self {
            proxies,
            index: AtomicUsize::new(0),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }

    pub fn len(&self) -> usize {
        self.proxies.len()
    }

    /// Index currently selected (0 when empty).
    pub fn current_index(&self) -> usize {
        if self.proxies.is_empty() {
            return 0;
        }
        self.index.load(Ordering::Relaxed) % self.proxies.len()
    }

    /// Proxy currently selected, if any.
    pub fn current(&self) -> Option<String> {
        self.proxies.get(self.current_index()).cloned()
    }

    /// Advances the cursor and returns the next proxy (if any).
    pub fn rotate(&self) -> Option<String> {
        if self.proxies.is_empty() {
            return None;
        }
        let next = next_proxy_index(self.index.load(Ordering::Relaxed), self.proxies.len());
        self.index.store(next, Ordering::Relaxed);
        self.proxies.get(next).cloned()
    }

    /// Marks the current proxy as failed and moves to the next one.
    pub fn mark_failed(&self) -> Option<String> {
        self.rotate()
    }
}

fn random_accept() -> &'static str {
    let accepts = [
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
    ];
    accepts[rand::rng().random_range(0..accepts.len())]
}

fn random_accept_language() -> &'static str {
    let langs = [
        "en-US,en;q=0.9,es;q=0.8",
        "en-GB,en;q=0.9,es;q=0.8",
        "en-US,en;q=0.9",
        "en-CA,en;q=0.9,fr;q=0.8",
        "es-ES,es;q=0.9,en;q=0.8",
        "de-DE,de;q=0.9,en;q=0.8",
        "fr-FR,fr;q=0.9,en;q=0.8",
        "pt-BR,pt;q=0.9,en;q=0.8",
    ];
    langs[rand::rng().random_range(0..langs.len())]
}

pub fn random_user_agent() -> String {
    USER_AGENTS[rand::rng().random_range(0..USER_AGENTS.len())].to_string()
}

pub fn random_headers() -> Vec<(String, String)> {
    let mut rng = rand::rng();
    vec![
        ("Accept".into(), random_accept().into()),
        ("Accept-Language".into(), random_accept_language().into()),
        ("Accept-Encoding".into(), "gzip, deflate, br".into()),
        ("Sec-Fetch-Dest".into(), "document".into()),
        ("Sec-Fetch-Mode".into(), "navigate".into()),
        (
            "Sec-Fetch-Site".into(),
            if rng.random_bool(0.7) {
                "none".into()
            } else {
                "cross-site".into()
            },
        ),
        ("Sec-Fetch-User".into(), "?1".into()),
        ("Upgrade-Insecure-Requests".into(), "1".into()),
        (
            "Sec-Ch-Ua".into(),
            format!(
                "\"Not)A;Brand\";v=\"99\", \"Google Chrome\";v=\"{}\", \"Chromium\";v=\"{}\"",
                rng.random_range(120..=126),
                rng.random_range(120..=126),
            ),
        ),
        ("Sec-Ch-Ua-Mobile".into(), "?0".into()),
        ("Sec-Ch-Ua-Platform".into(), {
            let platforms = ["\"Windows\"", "\"macOS\"", "\"Linux\""];
            platforms[rng.random_range(0..3)].into()
        }),
    ]
}

/// Exponential backoff (`base * 2^attempt`) with up to one extra `base` of
/// full jitter, capped so a single retry never sleeps for hours.
pub fn backoff_ms(attempt: u32, base_ms: u64) -> u64 {
    let exponential = base_ms.saturating_mul(1u64 << attempt.min(6));
    let jitter = rand::rng().random_range(0..=base_ms.max(1));
    exponential.saturating_add(jitter).min(120_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_is_hard_capped() {
        assert_eq!(clamp_concurrency(0), 1);
        assert_eq!(clamp_concurrency(1), 1);
        assert_eq!(clamp_concurrency(5), MAX_CONCURRENCY);
        assert_eq!(clamp_concurrency(usize::MAX), MAX_CONCURRENCY);
    }

    #[test]
    fn jitter_stays_within_20_percent() {
        for _ in 0..200 {
            let value = jitter_ms(1000);
            assert!((800..=1200).contains(&value), "unexpected jitter {value}");
        }
        assert_eq!(jitter_ms(0), 0);
    }

    #[test]
    fn proxy_rotation_cycles_round_robin() {
        let rotator = ProxyRotator::new(vec!["p1".into(), "p2".into(), "p3".into()]);
        assert_eq!(rotator.current().as_deref(), Some("p1"));
        assert_eq!(rotator.rotate().as_deref(), Some("p2"));
        assert_eq!(rotator.rotate().as_deref(), Some("p3"));
        assert_eq!(rotator.rotate().as_deref(), Some("p1"));
        assert_eq!(next_proxy_index(0, 0), 0);
    }

    #[test]
    fn proxy_mark_failed_advances() {
        let rotator = ProxyRotator::new(vec!["a".into(), "b".into()]);
        assert_eq!(rotator.mark_failed().as_deref(), Some("b"));
        assert_eq!(rotator.current().as_deref(), Some("b"));
    }

    #[test]
    fn empty_rotator_is_noop() {
        let rotator = ProxyRotator::default();
        assert!(rotator.is_empty());
        assert!(rotator.current().is_none());
        assert!(rotator.rotate().is_none());
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        let first = backoff_ms(0, 1000);
        let third = backoff_ms(2, 1000);
        assert!(first <= 2000);
        assert!(third >= 4000);
        assert!(backoff_ms(30, 1000) <= 120_000);
    }

    #[test]
    fn random_helpers_are_non_empty() {
        assert!(!random_user_agent().is_empty());
        let headers = random_headers();
        assert!(headers
            .iter()
            .any(|(name, _)| name == "User-Agent" || name == "Accept"));
    }
}
