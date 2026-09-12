//! Regex-based email/phone extraction used in "fast" mode.

use regex::Regex;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtractedData {
    pub emails: Vec<String>,
    pub phones: Vec<String>,
}

pub fn extract_all(body: &str, _base_url: &str) -> ExtractedData {
    ExtractedData {
        emails: extract_emails(body),
        phones: extract_phones(body),
    }
}

fn extract_emails(body: &str) -> Vec<String> {
    let re = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    let mut results: Vec<String> = re
        .find_iter(body)
        .map(|m| m.as_str().to_lowercase())
        .collect();
    results.sort();
    results.dedup();
    results
}

/// Captures complete phone numbers (not just a prefix group) and keeps only
/// plausible ones: 7 to 15 digits after stripping separators.
fn extract_phones(body: &str) -> Vec<String> {
    let re = Regex::new(
        r"(?:\+\d{1,3}[\s.\-]?)?(?:\(\d{1,5}\)[\s.\-]?|\d{1,4}[\s.\-]?)\d{3,4}[\s.\-]?\d{3,4}",
    )
    .unwrap();

    let mut results: Vec<String> = re
        .find_iter(body)
        .map(|m| m.as_str().trim().to_string())
        .filter(|candidate| {
            let digits = candidate.chars().filter(|c| c.is_ascii_digit()).count();
            (7..=15).contains(&digits)
        })
        .collect();
    results.sort();
    results.dedup();
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_full_email_list() {
        let data = extract_all("Contact a@example.com or B.C+tag@sub.example.org.", "");
        assert_eq!(
            data.emails,
            vec!["a@example.com", "b.c+tag@sub.example.org"]
        );
    }

    #[test]
    fn captures_complete_phone_numbers() {
        let data = extract_all("Call (555) 123-4567 or +34 612 345 678. Short: 12-34.", "");
        assert!(
            data.phones.iter().any(|p| p.contains("(555) 123-4567")),
            "{:?}",
            data.phones
        );
        assert!(
            data.phones.iter().any(|p| p.contains("+34 612 345 678")),
            "{:?}",
            data.phones
        );
    }

    #[test]
    fn rejects_too_short_numbers() {
        let data = extract_all("Ref 12-34 and id 123456.", "");
        assert!(data.phones.is_empty(), "{:?}", data.phones);
    }

    #[test]
    fn deduplicates_results() {
        let data = extract_all("a@b.co a@b.co", "");
        assert_eq!(data.emails.len(), 1);
    }
}
