//! xwa-sdk contracts (v0.2.0) replicated locally with serde.
//!
//! This module mirrors the canonical JSON Schemas in `xwa-sdk/schemas`:
//! `Event`, `Analysis`, `Finding`, `Error` and `Summary`. Optional fields are
//! skipped when `None` and enums serialize with their schema spellings, so the
//! JSON produced by Shinobi can be consumed by any XWA module or by the Rust
//! binding (`xwa-sdk/bindings/rust`) without a runtime dependency.
//!
//! Source of truth:
//! `xwa-sdk/schemas/{event,analysis,finding,error}.json` and
//! `xwa-sdk/bindings/rust/src/lib.rs`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Producing module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Samurai,
    Shinobi,
    Tengu,
    Kensei,
    Kabuki,
    Yari,
    Musha,
    Azuma,
}

/// Analysis lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnalysisStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "COMPLETED")]
    Completed,
    #[serde(rename = "ERROR")]
    Error,
    #[serde(rename = "CANCELLED")]
    Cancelled,
}

impl AnalysisStatus {
    /// Maps a Shinobi job status (`queued`, `running`, `scraping`, `deep`,
    /// `completed`, `failed`, `cancelled`) onto the unified lifecycle states.
    pub fn from_job_status(status: &str) -> Self {
        match status {
            "queued" => Self::Pending,
            "running" | "scraping" | "deep" | "pending" => Self::Running,
            "completed" => Self::Completed,
            "failed" | "error" => Self::Error,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }
}

/// Streaming event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    AnalysisStarted,
    AnalysisProgress,
    ItemFound,
    AnalysisCompleted,
    AnalysisError,
    Log,
}

impl EventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnalysisStarted => "analysis_started",
            Self::AnalysisProgress => "analysis_progress",
            Self::ItemFound => "item_found",
            Self::AnalysisCompleted => "analysis_completed",
            Self::AnalysisError => "analysis_error",
            Self::Log => "log",
        }
    }
}

/// Unified severity scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Pass,
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Detection confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Structured failure attached to an analysis.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
    #[serde(default)]
    pub retryable: bool,
}

/// Aggregated result counts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_items: Option<i64>,
    #[serde(default)]
    pub by_severity: BTreeMap<String, i64>,
    #[serde(default)]
    pub by_category: BTreeMap<String, i64>,
}

/// Top-level unit of work produced by a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub id: String,
    pub tool: Tool,
    pub target: String,
    pub status: AnalysisStatus,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            id: String::new(),
            tool: Tool::Shinobi,
            target: String::new(),
            status: AnalysisStatus::Pending,
            created_at: String::new(),
            tool_version: None,
            analysis_type: None,
            started_at: None,
            finished_at: None,
            error: None,
            summary: None,
        }
    }
}

/// A single observation mapped to the unified severity scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub tool: Tool,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cvss_score: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_at: Option<String>,
}

/// Streaming envelope for live analysis progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: i64,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub tool: Tool,
    pub analysis_id: String,
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

impl Event {
    /// Builds a Shinobi event with `tool: "shinobi"`.
    pub fn shinobi(
        seq: i64,
        event_type: EventType,
        analysis_id: &str,
        payload: Option<Value>,
    ) -> Self {
        Self {
            seq,
            event_type,
            tool: Tool::Shinobi,
            analysis_id: analysis_id.to_string(),
            ts: chrono::Utc::now().to_rfc3339(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_serializes_with_schema_field_names() {
        let event = Event::shinobi(
            7,
            EventType::ItemFound,
            "job-1",
            Some(json!({"kind": "email", "value": "a@b.c"})),
        );
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["seq"], 7);
        assert_eq!(value["type"], "item_found");
        assert_eq!(value["tool"], "shinobi");
        assert_eq!(value["analysis_id"], "job-1");
        assert!(value.get("event_type").is_none());
        assert!(value["ts"].is_string());
        assert_eq!(value["payload"]["kind"], "email");
    }

    #[test]
    fn event_roundtrip() {
        let event = Event::shinobi(1, EventType::AnalysisStarted, "x", None);
        let json = serde_json::to_string(&event).unwrap();
        let restored: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
        assert!(!json.contains("payload"));
    }

    #[test]
    fn status_and_severity_spellings_match_schemas() {
        assert_eq!(
            serde_json::to_value(AnalysisStatus::Pending).unwrap(),
            "PENDING"
        );
        assert_eq!(
            serde_json::to_value(AnalysisStatus::Cancelled).unwrap(),
            "CANCELLED"
        );
        assert_eq!(
            serde_json::to_value(Severity::Critical).unwrap(),
            "critical"
        );
        assert_eq!(
            serde_json::to_value(EventType::AnalysisProgress).unwrap(),
            "analysis_progress"
        );
        assert_eq!(serde_json::to_value(Tool::Shinobi).unwrap(), "shinobi");
    }

    #[test]
    fn job_status_mapping() {
        assert_eq!(
            AnalysisStatus::from_job_status("queued"),
            AnalysisStatus::Pending
        );
        assert_eq!(
            AnalysisStatus::from_job_status("scraping"),
            AnalysisStatus::Running
        );
        assert_eq!(
            AnalysisStatus::from_job_status("deep"),
            AnalysisStatus::Running
        );
        assert_eq!(
            AnalysisStatus::from_job_status("completed"),
            AnalysisStatus::Completed
        );
        assert_eq!(
            AnalysisStatus::from_job_status("failed"),
            AnalysisStatus::Error
        );
        assert_eq!(
            AnalysisStatus::from_job_status("cancelled"),
            AnalysisStatus::Cancelled
        );
    }

    #[test]
    fn explicit_null_optional_fields_deserialize() {
        let finding: Finding = serde_json::from_value(json!({
            "tool": "shinobi",
            "severity": "info",
            "title": "t",
            "description": "d",
            "id": null,
            "confidence": null
        }))
        .unwrap();
        assert_eq!(finding.id, None);
        assert_eq!(finding.confidence, None);
    }

    #[test]
    fn summary_defaults_to_empty_maps() {
        let summary = Summary::default();
        let value = serde_json::to_value(&summary).unwrap();
        assert_eq!(value["by_severity"], json!({}));
        assert_eq!(value["by_category"], json!({}));
        assert!(value.get("total_items").is_none());
    }
}
