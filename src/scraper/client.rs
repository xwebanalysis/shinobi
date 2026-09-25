//! HTTP client with polite scheduling, proxy rotation and retries.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::config::ScrapeConfig;
use crate::scraper::anti_block::{self, ProxyRotator};
use crate::scraper::throttle;

/// Effective per-domain delay: the configured rate limit (or delay) never goes
/// below the `Crawl-delay` declared in robots.txt.
pub fn effective_delay_ms(rate_limit_ms: u64, delay_ms: u64, crawl_delay_ms: u64) -> u64 {
    let configured = if rate_limit_ms > 0 {
        rate_limit_ms
    } else {
        delay_ms
    };
    configured.max(crawl_delay_ms)
}

pub struct ScrapeClient {
    pub client: Client,
    /// One pre-built client per proxy so requests can rotate per call.
    proxy_clients: Vec<Client>,
    pub config: Arc<ScrapeConfig>,
    domain_timers: Mutex<HashMap<String, tokio::time::Instant>>,
    proxies: ProxyRotator,
    crawl_delay_ms: AtomicU64,
}

fn build_client(config: &ScrapeConfig, proxy: Option<&str>) -> Result<Client, String> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .gzip(true)
        .brotli(true)
        .pool_max_idle_per_host(anti_block::clamp_concurrency(config.concurrency).max(2))
        .tcp_keepalive(Duration::from_secs(30))
        .cookie_store(true);

    if config.auth_mode == "basic"
        && !config.auth_username.is_empty()
        && !config.auth_password.is_empty()
    {
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("{}:{}", config.auth_username, config.auth_password),
        );
        if let Ok(auth_val) = reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded))
        {
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(reqwest::header::AUTHORIZATION, auth_val);
            builder = builder.default_headers(h);
        }
    }

    if !config.user_agent_rotation {
        builder = builder.user_agent(anti_block::random_user_agent());
    }

    if let Some(proxy_url) = proxy {
        match reqwest::Proxy::all(proxy_url) {
            Ok(p) => builder = builder.proxy(p),
            Err(e) => warn!("Invalid proxy {}: {}", proxy_url, e),
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

impl ScrapeClient {
    pub fn new(config: Arc<ScrapeConfig>) -> Result<Self, String> {
        let client = build_client(&config, None)?;

        let proxies = if config.use_proxies {
            ProxyRotator::new(config.proxy_list.clone())
        } else {
            ProxyRotator::default()
        };

        let mut proxy_clients = Vec::new();
        if !proxies.is_empty() {
            for proxy in &config.proxy_list {
                if proxy.trim().is_empty() {
                    continue;
                }
                match build_client(&config, Some(proxy)) {
                    Ok(c) => proxy_clients.push(c),
                    Err(e) => warn!("Skipping proxy {}: {}", proxy, e),
                }
            }
        }

        Ok(Self {
            client,
            proxy_clients,
            config,
            domain_timers: Mutex::new(HashMap::new()),
            proxies,
            crawl_delay_ms: AtomicU64::new(0),
        })
    }

    fn request_client(&self) -> &Client {
        if self.proxy_clients.is_empty() {
            return &self.client;
        }
        let idx = self.proxies.current_index() % self.proxy_clients.len();
        &self.proxy_clients[idx]
    }

    /// Applies the `Crawl-delay` found in robots.txt (milliseconds).
    pub fn set_crawl_delay_ms(&self, ms: u64) {
        self.crawl_delay_ms.store(ms, Ordering::Relaxed);
    }

    /// Number of proxies configured (0 when disabled/empty).
    pub fn proxy_count(&self) -> usize {
        self.proxies.len()
    }

    async fn wait_for_slot(&self, url: &str) {
        let Some(host) = url.split('/').nth(2) else {
            return;
        };
        let base = effective_delay_ms(
            self.config.rate_limit,
            self.config.delay_ms,
            self.crawl_delay_ms.load(Ordering::Relaxed),
        );
        let wait_target = anti_block::jitter_ms(base);

        let mut timers = self.domain_timers.lock().await;
        let domain = host.to_string();
        if let Some(last) = timers.get(&domain) {
            let elapsed = last.elapsed().as_millis() as u64;
            if elapsed < wait_target {
                let wait = wait_target - elapsed;
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
        }
        timers.insert(domain, tokio::time::Instant::now());
    }

    pub async fn get(&self, url: &str, attempt: u32) -> Result<reqwest::Response, String> {
        // Round-robin proxy for this request.
        let mut req = self.request_client().get(url);

        if self.config.user_agent_rotation {
            req = req.header("User-Agent", anti_block::random_user_agent());
        }

        for (k, v) in anti_block::random_headers() {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(&v),
            ) {
                req = req.header(name, val);
            }
        }

        self.wait_for_slot(url).await;

        // Process-wide cross-job throttle (SHINOBI_GLOBAL_RPS). Composes with
        // the per-domain delay above: a request needs both its domain slot and
        // a global token before it is sent.
        throttle::global_throttle().acquire().await;

        let response = match req.send().await {
            Ok(response) => response,
            Err(e) => {
                if !self.proxies.is_empty() {
                    warn!("Request failed via proxy, rotating: {}", e);
                    let _ = self.proxies.mark_failed();
                }
                return Err(if attempt + 1 < self.config.retry_count.max(1) {
                    format!("Request failed (attempt {}): {}", attempt + 1, e)
                } else {
                    format!("Request failed after {} attempts: {}", attempt + 1, e)
                });
            }
        };

        let status = response.status();
        if status.is_success() {
            Ok(response)
        } else if status.as_u16() == 429 || status.as_u16() == 503 {
            // Rate limited: rotate to the next proxy before the next attempt.
            if !self.proxies.is_empty() {
                let _ = self.proxies.mark_failed();
            }
            Err(format!("Rate limited (HTTP {})", status))
        } else if status.is_server_error() {
            Err(format!("Server error (HTTP {})", status))
        } else if status.is_client_error() {
            Err(format!("Client error (HTTP {})", status))
        } else {
            Ok(response)
        }
    }

    pub async fn get_with_retry(&self, url: &str) -> Result<reqwest::Response, String> {
        let attempts = self.config.retry_count.max(1);
        let mut last_err = String::new();
        for attempt in 0..attempts {
            match self.get(url, attempt).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    last_err = e.clone();
                    let is_rate_limited =
                        e.contains("Rate limited") || e.contains("429") || e.contains("503");
                    if attempt + 1 < attempts {
                        let base = if is_rate_limited { 5000 } else { 1000 };
                        let wait = anti_block::backoff_ms(attempt, base);
                        info!(
                            "Retrying {} in {}ms (attempt {}/{})",
                            url,
                            wait,
                            attempt + 2,
                            attempts
                        );
                        tokio::time::sleep(Duration::from_millis(wait)).await;
                    }
                }
            }
        }
        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_delay_respects_crawl_delay() {
        assert_eq!(effective_delay_ms(0, 1000, 0), 1000);
        assert_eq!(effective_delay_ms(500, 1000, 0), 500);
        assert_eq!(effective_delay_ms(500, 1000, 1500), 1500);
        assert_eq!(effective_delay_ms(0, 200, 0), 200);
    }
}
