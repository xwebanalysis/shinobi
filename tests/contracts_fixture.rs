//! Serialization checks against the minimal xwa-sdk fixtures.

use shinobi::contracts::{Analysis, AnalysisStatus, Event, EventType, Tool};

#[test]
fn event_fixture_deserializes_and_roundtrips() {
    let raw = include_str!("fixtures/xwa_event.json");
    let event: Event = serde_json::from_str(raw).expect("fixture must parse");
    assert_eq!(event.seq, 4);
    assert_eq!(event.event_type, EventType::ItemFound);
    assert_eq!(event.tool, Tool::Shinobi);
    assert_eq!(event.analysis_id, "3f9c2a7e-1b2c-4d5e-8f90-abcdef012345");
    assert_eq!(
        event.payload.as_ref().unwrap()["value"],
        "contact@example.com"
    );

    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["type"], "item_found");
    assert_eq!(value["tool"], "shinobi");
    assert!(value.get("event_type").is_none());
}

#[test]
fn analysis_fixture_maps_to_envelope_fields() {
    let raw = include_str!("fixtures/xwa_analysis.json");
    let analysis: Analysis = serde_json::from_str(raw).expect("fixture must parse");
    assert_eq!(analysis.tool, Tool::Shinobi);
    assert_eq!(analysis.status, AnalysisStatus::Completed);
    assert_eq!(analysis.analysis_type.as_deref(), Some("crawler"));
    let summary = analysis.summary.unwrap();
    assert_eq!(summary.total_items, Some(2));
    assert_eq!(summary.by_category.get("emails"), Some(&1));
}

#[test]
fn shinobi_event_constructor_is_consistent() {
    let event = Event::shinobi(1, EventType::AnalysisProgress, "job-x", None);
    assert_eq!(event.tool, Tool::Shinobi);
    assert_eq!(event.analysis_id, "job-x");
    assert_eq!(event.event_type.as_str(), "analysis_progress");
}
