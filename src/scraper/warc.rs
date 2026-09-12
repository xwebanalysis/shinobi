//! WARC 1.0 writer with valid record identifiers.
//!
//! Each response record carries a unique `WARC-Record-ID` (`<urn:uuid:…>`), a
//! reference to the file-level `warcinfo` record via `WARC-Warcinfo-ID` and a
//! `WARC-Payload-Digest` of the stored body.

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn new_record_id() -> String {
    format!("<urn:uuid:{}>", Uuid::new_v4())
}

fn payload_digest(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    format!("sha256:{}", hex)
}

#[derive(Clone)]
pub struct WarcRecord {
    pub target_uri: String,
    pub date: String,
    pub content_type: String,
    pub body: Vec<u8>,
    pub record_id: String,
}

impl WarcRecord {
    pub fn new(url: &str, content_type: &str, body: &[u8]) -> Self {
        Self {
            target_uri: url.to_string(),
            date: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            content_type: content_type.to_string(),
            body: body.to_vec(),
            record_id: new_record_id(),
        }
    }

    pub fn to_warc_bytes(&self) -> Vec<u8> {
        self.build(None)
    }

    pub fn to_warc_bytes_with_info(&self, warcinfo_id: &str) -> Vec<u8> {
        self.build(Some(warcinfo_id))
    }

    fn build(&self, warcinfo_id: Option<&str>) -> Vec<u8> {
        let http_headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
            self.content_type,
            self.body.len()
        );
        let block = [http_headers.as_bytes(), &self.body].concat();

        let mut headers = format!(
            "WARC/1.0\r\n\
             WARC-Type: response\r\n\
             WARC-Record-ID: {}\r\n\
             WARC-Date: {}\r\n\
             WARC-Target-URI: {}\r\n\
             WARC-Payload-Digest: {}\r\n\
             Content-Type: application/http; msgtype=response\r\n\
             Content-Length: {}\r\n",
            self.record_id,
            self.date,
            self.target_uri,
            payload_digest(&self.body),
            block.len()
        );
        if let Some(info_id) = warcinfo_id {
            headers.push_str(&format!("WARC-Warcinfo-ID: {}\r\n", info_id));
        }
        headers.push_str("\r\n");

        [headers.as_bytes(), &block, b"\r\n\r\n"].concat()
    }
}

fn warcinfo_bytes(record_id: &str, filename: &str, response_count: usize) -> Vec<u8> {
    let date = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let body = format!(
        "software: shinobi/{}\r\nformat: WARC File Format 1.0\r\nisPartOf: shinobi\r\nrecords: {}\r\n",
        env!("CARGO_PKG_VERSION"),
        response_count
    );
    let header = format!(
        "WARC/1.0\r\n\
         WARC-Type: warcinfo\r\n\
         WARC-Record-ID: {}\r\n\
         WARC-Date: {}\r\n\
         WARC-Filename: {}\r\n\
         WARC-Payload-Digest: {}\r\n\
         Content-Type: application/warc-fields\r\n\
         Content-Length: {}\r\n\
         \r\n",
        record_id,
        date,
        filename,
        payload_digest(body.as_bytes()),
        body.len()
    );
    [header.as_bytes(), body.as_bytes(), b"\r\n\r\n"].concat()
}

/// Serializes a complete `.warc` file: one `warcinfo` record followed by every
/// response record.
pub fn create_warc_file(records: &[WarcRecord]) -> Vec<u8> {
    if records.is_empty() {
        return Vec::new();
    }
    let warcinfo_id = new_record_id();
    let filename = format!("shinobi-{}.warc", Utc::now().format("%Y%m%d%H%M%S"));
    let mut data = warcinfo_bytes(&warcinfo_id, &filename, records.len());
    for record in records {
        data.extend_from_slice(&record.to_warc_bytes_with_info(&warcinfo_id));
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> WarcRecord {
        WarcRecord::new("https://example.com/", "text/html", b"<html>hello</html>")
    }

    #[test]
    fn record_has_valid_ids_and_length() {
        let record = sample_record();
        let bytes = record.to_warc_bytes();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("WARC/1.0\r\n"));
        assert!(text.contains("WARC-Type: response\r\n"));
        assert!(text.contains(&format!("WARC-Record-ID: {}", record.record_id)));
        assert!(record.record_id.starts_with("<urn:uuid:"));
        let body_len = b"<html>hello</html>".len();
        assert!(text.contains(&format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n",
            body_len
        )));
        assert!(text.contains("WARC-Payload-Digest: sha256:"));
    }

    #[test]
    fn file_starts_with_warcinfo_and_links_records() {
        let records = vec![sample_record(), sample_record()];
        let bytes = create_warc_file(&records);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("WARC/1.0\r\n"));
        assert!(text.contains("WARC-Type: warcinfo\r\n"));
        assert!(text.contains("WARC-Filename: shinobi-"));
        let info_id = text
            .lines()
            .find_map(|l| l.strip_prefix("WARC-Record-ID: "))
            .unwrap()
            .to_string();
        let linked = text
            .matches(&format!("WARC-Warcinfo-ID: {}", info_id))
            .count();
        assert_eq!(linked, 2);
        assert!(text.matches("WARC-Type: response").count() == 2);
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn empty_input_produces_empty_file() {
        assert!(create_warc_file(&[]).is_empty());
    }

    #[test]
    fn record_ids_are_unique() {
        let a = sample_record();
        let b = sample_record();
        assert_ne!(a.record_id, b.record_id);
    }
}
