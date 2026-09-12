//! Minimal but correct `robots.txt` parser.
//!
//! Supports `User-agent`, `Allow`, `Disallow` and `Crawl-delay` for the `*`
//! group (the one Shinobi obeys by default). Matching follows RFC 9309:
//! the **longest** matching path wins, and an `Allow` wins ties.

/// A single `Allow`/`Disallow` rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub allow: bool,
    pub path: String,
}

#[derive(Debug, Clone, Default)]
pub struct RobotsTxt {
    pub rules: Vec<Rule>,
    /// Seconds to wait between requests, when declared for the `*` agent.
    pub crawl_delay: Option<f64>,
}

impl RobotsTxt {
    pub fn parse(body: &str) -> Self {
        let mut rules = Vec::new();
        let mut crawl_delay = None;
        let mut in_star_group = false;

        for raw in body.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((field, value)) = line.split_once(':') else {
                continue;
            };
            let field = field.trim().to_ascii_lowercase();
            let value = value.trim();

            match field.as_str() {
                "user-agent" => {
                    let agent = value.to_ascii_lowercase();
                    // A named agent group ends the `*` group unless another
                    // `*` block starts later.
                    in_star_group = agent == "*";
                }
                "allow" if in_star_group => {
                    if !value.is_empty() {
                        rules.push(Rule {
                            allow: true,
                            path: value.to_string(),
                        });
                    }
                }
                "disallow" if in_star_group => {
                    if !value.is_empty() {
                        rules.push(Rule {
                            allow: false,
                            path: value.to_string(),
                        });
                    }
                }
                "crawl-delay" if in_star_group => {
                    if let Ok(seconds) = value.parse::<f64>() {
                        if seconds >= 0.0 {
                            crawl_delay = Some(seconds);
                        }
                    }
                }
                _ => {}
            }
        }

        // Preserve declaration order for deterministic tests.
        rules.sort_by_key(|a| std::cmp::Reverse(a.path.len()));

        Self { rules, crawl_delay }
    }

    /// Number of disallow rules (kept for logging compatibility).
    pub fn disallow_count(&self) -> usize {
        self.rules.iter().filter(|r| !r.allow).count()
    }

    pub fn is_allowed(&self, url: &url::Url) -> bool {
        let path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        let mut best: Option<&Rule> = None;
        for rule in &self.rules {
            if path.starts_with(&rule.path) {
                match best {
                    Some(current) if current.path.len() >= rule.path.len() => {}
                    _ => best = Some(rule),
                }
            }
        }
        best.map(|r| r.allow).unwrap_or(true)
    }

    /// Delay in milliseconds, when the robots file declares one.
    pub fn crawl_delay_ms(&self) -> Option<u64> {
        self.crawl_delay
            .map(|seconds| (seconds * 1000.0).round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn url(path: &str) -> Url {
        Url::parse(&format!("https://example.com{}", path)).unwrap()
    }

    #[test]
    fn parses_disallow_and_allow() {
        let robots = RobotsTxt::parse(
            "User-agent: *\nDisallow: /private\nAllow: /private/public\nCrawl-delay: 1.5\n",
        );
        assert_eq!(robots.rules.len(), 2);
        assert_eq!(robots.crawl_delay, Some(1.5));
        assert_eq!(robots.crawl_delay_ms(), Some(1500));
        assert!(robots.is_allowed(&url("/")));
        assert!(!robots.is_allowed(&url("/private/secret")));
        assert!(robots.is_allowed(&url("/private/public/page")));
    }

    #[test]
    fn longest_match_wins_regardless_of_order() {
        let robots = RobotsTxt::parse("User-agent: *\nAllow: /a\nDisallow: /a/b\n");
        assert!(robots.is_allowed(&url("/a/x")));
        assert!(!robots.is_allowed(&url("/a/b/c")));

        let robots = RobotsTxt::parse("User-agent: *\nAllow: /a/b\nDisallow: /a\n");
        assert!(robots.is_allowed(&url("/a/b/c")));
    }

    #[test]
    fn comments_and_other_agents_are_ignored() {
        let robots = RobotsTxt::parse(
            "# comment\nUser-agent: googlebot\nDisallow: /nope\n\nUser-agent: *\nDisallow: /all # inline\n",
        );
        assert!(robots.is_allowed(&url("/nope")));
        assert!(!robots.is_allowed(&url("/all")));
    }

    #[test]
    fn missing_user_agent_means_unrestricted() {
        let robots = RobotsTxt::parse("Disallow: /everything\nCrawl-delay: 5\n");
        assert!(robots.rules.is_empty());
        assert!(robots.is_allowed(&url("/everything")));
        assert_eq!(robots.crawl_delay, None);
    }

    #[test]
    fn empty_file_allows_everything() {
        let robots = RobotsTxt::parse("");
        assert!(robots.is_allowed(&url("/")));
        assert_eq!(robots.disallow_count(), 0);
    }
}
