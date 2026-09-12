//! `sitemap.xml` parsing (works for both `<urlset>` and `<sitemapindex>`).

use regex::Regex;
use url::Url;

/// Extracts every `<loc>` value, handling optional CDATA and XML entities.
pub fn extract_locs(body: &str) -> Vec<String> {
    let re = Regex::new(r"(?is)<loc>\s*(?:<!\[CDATA\[(.*?)\]\]>|(.*?))\s*</loc>").unwrap();
    re.captures_iter(body)
        .filter_map(|caps| {
            caps.get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str().trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
        })
        .collect()
}

pub fn parse_sitemap(body: &str, base: &Url) -> Vec<Url> {
    extract_locs(body)
        .into_iter()
        .filter_map(|text| base.join(&text).ok())
        .filter(|url| url.scheme() == "http" || url.scheme() == "https")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com").unwrap()
    }

    #[test]
    fn parses_namespaced_urlset() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
          <url><loc>https://example.com/a</loc></url>
          <url><loc>https://example.com/b?x=1&amp;y=2</loc></url>
        </urlset>"#;
        let urls = parse_sitemap(xml, &base());
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].as_str(), "https://example.com/a");
        assert_eq!(urls[1].query(), Some("x=1&y=2"));
    }

    #[test]
    fn parses_sitemap_index_with_cdata() {
        let xml = r#"<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
          <sitemap><loc><![CDATA[https://example.com/sitemap-1.xml]]></loc></sitemap>
        </sitemapindex>"#;
        let urls = parse_sitemap(xml, &base());
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].as_str(), "https://example.com/sitemap-1.xml");
    }

    #[test]
    fn resolves_relative_locations_and_skips_junk() {
        let xml =
            "<urlset><url><loc>/relative</loc></url><url><loc>mailto:x@y.z</loc></url></urlset>";
        let urls = parse_sitemap(xml, &base());
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].as_str(), "https://example.com/relative");
    }

    #[test]
    fn empty_document_returns_empty() {
        assert!(parse_sitemap("", &base()).is_empty());
        assert!(extract_locs("<urlset></urlset>").is_empty());
    }
}
