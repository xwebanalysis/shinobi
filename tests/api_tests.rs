//! HTTP/API integration tests using the real router and a temporary SQLite DB.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use dashmap::DashMap;
use http_body_util::BodyExt;
use serde_json::Value;
use shinobi::api::routes::{create_router, AppState, JobInfo};
use shinobi::storage::db::DbStore;
use shinobi::storage::manager::StorageManager;
use tower::ServiceExt;

struct TestCtx {
    state: AppState,
    /// Keeps the temporary directory alive for the whole test.
    _dir: tempfile::TempDir,
}

fn test_ctx() -> TestCtx {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(
        dir.path().join("downloads").to_str().unwrap(),
    ));
    let db = Arc::new(DbStore::new(dir.path().join("test.db").to_str().unwrap()).unwrap());
    let state = AppState::new(
        storage,
        Arc::new(DashMap::new()),
        Arc::new(DashMap::new()),
        Some(db),
    );
    TestCtx { state, _dir: dir }
}

fn sample_job(id: &str, status: &str, files: Vec<String>) -> JobInfo {
    JobInfo {
        id: id.into(),
        url: "https://example.com".into(),
        status: status.into(),
        created_at: "2026-09-12T10:00:00Z".into(),
        pages_scraped: 2,
        files_downloaded: files.len(),
        total_pages: 5,
        current_url: Some("https://example.com/about".into()),
        errors: vec![],
        emails: vec!["a@example.com".into()],
        phones: vec!["+34 600 000 000".into()],
        files,
    }
}

async fn get(state: &AppState, uri: &str) -> (StatusCode, Value, axum::http::HeaderMap) {
    let app = create_router(state.clone());
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, headers)
}

async fn delete(state: &AppState, uri: &str) -> (StatusCode, Value) {
    let app = create_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn health_returns_standard_json() {
    let ctx = test_ctx();
    let (status, body, _) = get(&ctx.state, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "shinobi");
    assert_eq!(body["database"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn analyses_aliases_match_jobs_endpoints() {
    let ctx = test_ctx();
    ctx.state
        .jobs
        .insert("job-1".into(), sample_job("job-1", "completed", vec![]));

    let (status, analysis, _) = get(&ctx.state, "/analyses/job-1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(analysis["id"], "job-1");
    assert_eq!(analysis["tool"], "shinobi");
    assert_eq!(analysis["status"], "COMPLETED");
    assert_eq!(analysis["target"], "https://example.com");
    assert_eq!(analysis["analysis_type"], "crawler");
    assert_eq!(analysis["summary"]["total_items"], 2);

    let (status, job, _) = get(&ctx.state, "/jobs/job-1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(job["id"], "job-1");

    let (status, list, _) = get(&ctx.state, "/analyses").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["id"], "job-1");
}

#[tokio::test]
async fn analysis_export_supports_json_and_csv() {
    let ctx = test_ctx();
    ctx.state
        .jobs
        .insert("job-2".into(), sample_job("job-2", "completed", vec![]));

    let (status, _, headers) = get(&ctx.state, "/analyses/job-2/export?format=json").await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.contains("shinobi-analysis-job-2.json"));
    assert!(headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("application/json"));

    let app = create_router(ctx.state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/analyses/job-2/export?format=csv")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains(".csv"));
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let csv = String::from_utf8_lossy(&bytes);
    assert!(csv.starts_with("id,tool,status,target"));
    assert!(csv.contains("job-2"));
}

#[tokio::test]
async fn delete_analysis_removes_only_that_jobs_files() {
    let ctx = test_ctx();
    let storage = ctx.state.storage.clone();
    storage
        .save_file("example.com/job1.html", b"job1")
        .await
        .unwrap();
    storage
        .save_file("example.com/job2.html", b"job2")
        .await
        .unwrap();
    ctx.state.jobs.insert(
        "job-1".into(),
        sample_job("job-1", "completed", vec!["example.com/job1.html".into()]),
    );
    ctx.state.jobs.insert(
        "job-2".into(),
        sample_job("job-2", "completed", vec!["example.com/job2.html".into()]),
    );

    let (status, body) = delete(&ctx.state, "/analyses/job-1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed_files"], 1);

    assert!(storage.read_file("example.com/job1.html").await.is_err());
    assert!(storage.read_file("example.com/job2.html").await.is_ok());
    assert!(ctx.state.jobs.get("job-2").is_some());
    assert!(ctx.state.jobs.get("job-1").is_none());
}

#[tokio::test]
async fn job_zip_contains_only_that_jobs_files() {
    let ctx = test_ctx();
    let storage = ctx.state.storage.clone();
    storage
        .save_file("example.com/job1.html", b"<html>one</html>")
        .await
        .unwrap();
    storage
        .save_file("example.com/job2.html", b"<html>two</html>")
        .await
        .unwrap();
    storage
        .save_file("other.com/job1.html", b"<html>other</html>")
        .await
        .unwrap();
    ctx.state.jobs.insert(
        "job-1".into(),
        sample_job("job-1", "completed", vec!["example.com/job1.html".into()]),
    );

    let app = create_router(ctx.state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/jobs/job-1/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/zip"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    assert_eq!(archive.len(), 1);
    let mut names = Vec::new();
    for i in 0..archive.len() {
        names.push(archive.by_index(i).unwrap().name().to_string());
    }
    assert_eq!(names, vec!["job1.html"]);
}

#[tokio::test]
async fn export_job_is_job_scoped() {
    let ctx = test_ctx();
    let files = vec!["example.com/job1.html".to_string()];
    ctx.state.jobs.insert(
        "job-1".into(),
        sample_job("job-1", "completed", files.clone()),
    );

    let app = create_router(ctx.state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs/job-1/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["job"]["id"], "job-1");
    assert_eq!(body["files"], serde_json::json!(files));
    assert_eq!(body["analysis"]["tool"], "shinobi");
}

#[tokio::test]
async fn missing_analysis_returns_404() {
    let ctx = test_ctx();
    let (status, _, _) = get(&ctx.state, "/analyses/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sse_stream_emits_xwa_sdk_events() {
    let ctx = test_ctx();
    ctx.state
        .jobs
        .insert("job-sse".into(), sample_job("job-sse", "completed", vec![]));

    let app = create_router(ctx.state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/jobs/job-sse/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes);

    assert!(body.contains("event: analysis_started"), "{}", body);
    assert!(body.contains("event: analysis_completed"), "{}", body);
    assert!(body.contains("\"tool\":\"shinobi\""), "{}", body);
    assert!(body.contains("\"analysis_id\":\"job-sse\""), "{}", body);
    assert!(body.contains("\"type\":\"item_found\""), "{}", body);
    assert!(body.contains("event: item_found"), "{}", body);
}

#[tokio::test]
async fn legacy_database_export_is_importable() {
    let ctx = test_ctx();
    let db = ctx.state.db.as_ref().unwrap();
    db.save_job(sample_job(
        "job-db",
        "completed",
        vec!["example.com/a.html".into()],
    ))
    .await
    .unwrap();

    let (status, export, _) = get(&ctx.state, "/database/export").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export["version"], "SHINOBI_DB_V1");
    assert_eq!(export["jobs"].as_array().unwrap().len(), 1);
    assert!(export["schedules"].is_array());
    assert!(export["deep_results"].is_array());
}
