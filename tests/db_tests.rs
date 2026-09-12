//! Database CRUD, migration, export/import and clear tests.

use serde_json::json;
use shinobi::api::routes::{DeepResult, JobInfo, Schedule};
use shinobi::scraper::extractor::ExtractedData;
use shinobi::storage::db::DbStore;

fn job(id: &str) -> JobInfo {
    JobInfo {
        id: id.into(),
        url: "https://example.com".into(),
        status: "completed".into(),
        created_at: "2026-09-12T10:00:00Z".into(),
        pages_scraped: 3,
        files_downloaded: 2,
        total_pages: 10,
        current_url: Some("https://example.com/about".into()),
        errors: vec!["warn".into()],
        emails: vec!["a@example.com".into()],
        phones: vec!["+34 600 000 000".into()],
        files: vec!["example.com/index.html".into()],
    }
}

fn deep(id: &str, job_id: &str) -> DeepResult {
    DeepResult {
        id: id.into(),
        job_id: job_id.into(),
        url: "https://example.com".into(),
        structured_data: Some(json!({"json-ld": {"@type": "WebSite"}})),
        nlp_data: None,
        extracted: ExtractedData {
            emails: vec!["deep@example.com".into()],
            phones: vec![],
        },
        created_at: "2026-09-12T10:05:00Z".into(),
    }
}

fn schedule(id: &str) -> Schedule {
    Schedule {
        id: id.into(),
        url: "https://example.com".into(),
        interval_min: 30,
        config: json!({"depth": 1}),
        enabled: true,
        last_run: None,
        next_run: "2026-09-12T11:00:00Z".into(),
        created_at: "2026-09-12T10:00:00Z".into(),
    }
}

#[tokio::test]
async fn job_crud_roundtrip_including_files() {
    let dir = tempfile::tempdir().unwrap();
    let db = DbStore::new(dir.path().join("test.db").to_str().unwrap()).unwrap();

    db.save_job(job("j1")).await.unwrap();
    let loaded = db.load_jobs().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].files, vec!["example.com/index.html"]);
    assert_eq!(loaded[0].emails, vec!["a@example.com"]);

    db.delete_job("j1".into()).await.unwrap();
    assert!(db.load_jobs().await.unwrap().is_empty());
}

#[tokio::test]
async fn deep_result_crud_and_cascade_delete() {
    let dir = tempfile::tempdir().unwrap();
    let db = DbStore::new(dir.path().join("test.db").to_str().unwrap()).unwrap();

    db.save_deep_result(deep("d1", "j1")).await.unwrap();
    db.save_deep_result(deep("d2", "j2")).await.unwrap();
    assert_eq!(db.load_deep_results().await.unwrap().len(), 2);
    assert!(db.get_deep_result("d1".into()).await.is_ok());

    db.delete_deep_results_for_job("j1".into()).await.unwrap();
    let remaining = db.load_deep_results().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "d2");

    assert!(db.delete_deep_result("d2".into()).await.is_ok());
    assert!(db.delete_deep_result("d2".into()).await.is_err());
}

#[tokio::test]
async fn deleting_job_also_deletes_its_deep_results() {
    let dir = tempfile::tempdir().unwrap();
    let db = DbStore::new(dir.path().join("test.db").to_str().unwrap()).unwrap();
    db.save_job(job("j1")).await.unwrap();
    db.save_job(job("j2")).await.unwrap();
    db.save_deep_result(deep("d1", "j1")).await.unwrap();
    db.save_deep_result(deep("d2", "j2")).await.unwrap();

    db.delete_job("j1".into()).await.unwrap();
    assert_eq!(db.load_jobs().await.unwrap().len(), 1);
    assert_eq!(db.load_deep_results().await.unwrap().len(), 1);
}

#[tokio::test]
async fn schedule_crud() {
    let dir = tempfile::tempdir().unwrap();
    let db = DbStore::new(dir.path().join("test.db").to_str().unwrap()).unwrap();

    db.save_schedule(schedule("s1")).await.unwrap();
    let loaded = db.load_schedules().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].interval_min, 30);
    assert_eq!(loaded[0].config["depth"], 1);

    db.delete_schedule("s1".into()).await.unwrap();
    assert!(db.load_schedules().await.unwrap().is_empty());
}

#[tokio::test]
async fn export_import_clear_covers_all_tables() {
    let dir = tempfile::tempdir().unwrap();
    let db = DbStore::new(dir.path().join("source.db").to_str().unwrap()).unwrap();
    db.save_job(job("j1")).await.unwrap();
    db.save_deep_result(deep("d1", "j1")).await.unwrap();
    db.save_schedule(schedule("s1")).await.unwrap();

    let export = db.export_all().await.unwrap();
    assert_eq!(export["version"], "SHINOBI_DB_V1");
    assert_eq!(export["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(export["deep_results"].as_array().unwrap().len(), 1);
    assert_eq!(export["schedules"].as_array().unwrap().len(), 1);

    let payload: shinobi::storage::db::ImportPayload =
        serde_json::from_value(export).expect("export must be importable");
    let target = DbStore::new(dir.path().join("target.db").to_str().unwrap()).unwrap();
    let report = target.import_all(payload).await.unwrap();
    assert_eq!(report.jobs, 1);
    assert_eq!(report.deep_results, 1);
    assert_eq!(report.schedules, 1);

    target.clear_all().await.unwrap();
    assert!(target.load_jobs().await.unwrap().is_empty());
    assert!(target.load_deep_results().await.unwrap().is_empty());
    assert!(target.load_schedules().await.unwrap().is_empty());
}

#[tokio::test]
async fn migrations_are_idempotent_and_ping_works() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");

    {
        let db = DbStore::new(path.to_str().unwrap()).unwrap();
        assert!(db.ping().await);
    }
    // Opening an existing database must not fail or duplicate schema rows.
    let db = DbStore::new(path.to_str().unwrap()).unwrap();
    assert!(db.ping().await);
    db.save_job(job("after-reopen")).await.unwrap();
    assert_eq!(db.load_jobs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn legacy_database_without_files_column_is_upgraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");

    // Simulate a pre-v1 database.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE jobs (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                created_at TEXT NOT NULL,
                pages_scraped INTEGER DEFAULT 0,
                files_downloaded INTEGER DEFAULT 0,
                total_pages INTEGER DEFAULT 0,
                current_url TEXT,
                errors TEXT DEFAULT '[]',
                emails TEXT DEFAULT '[]',
                phones TEXT DEFAULT '[]',
                data TEXT
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs (id, url, status, created_at) VALUES ('old', 'https://old.example', 'completed', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    }

    let db = DbStore::new(path.to_str().unwrap()).unwrap();
    let jobs = db.load_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].files.is_empty());
    db.save_job(job("new")).await.unwrap();
    assert_eq!(db.load_jobs().await.unwrap().len(), 2);
}
