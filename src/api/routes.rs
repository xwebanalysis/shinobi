//! REST API and SSE stream.
//!
//! Every persisted job is exposed both through the legacy `/api/jobs/*`
//! endpoints (used by the current UI) and through the semantic
//! `/api/analyses/*` aliases that return xwa-sdk `Analysis` envelopes. The SSE
//! stream emits xwa-sdk `Event` objects (`analysis_started`,
//! `analysis_progress`, `item_found`, `log`, `analysis_completed`,
//! `analysis_error`).

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::response::sse::Event;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Sse},
    routing::{delete, get, post},
    Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tokio::sync::{mpsc, Semaphore};
use uuid::Uuid;

use crate::config::ScrapeConfig;
use crate::contracts::{
    Analysis, AnalysisStatus, Error as ContractError, Event as ContractEvent, EventType, Summary,
    Tool,
};
use crate::scraper::anti_block;
use crate::scraper::downloader::{Downloader, ScrapeProgress};
use crate::storage::db::{DbStore, ImportPayload};
use crate::storage::manager::StorageManager;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<StorageManager>,
    pub jobs: Arc<DashMap<String, JobInfo>>,
    pub downloaders: Arc<DashMap<String, Arc<Downloader>>>,
    pub db: Option<Arc<DbStore>>,
    pub scrape_semaphore: Arc<Semaphore>,
    pub active_scrapes: Arc<AtomicUsize>,
}

impl AppState {
    pub fn new(
        storage: Arc<StorageManager>,
        jobs: Arc<DashMap<String, JobInfo>>,
        downloaders: Arc<DashMap<String, Arc<Downloader>>>,
        db: Option<Arc<DbStore>>,
    ) -> Self {
        Self {
            storage,
            jobs,
            downloaders,
            db,
            scrape_semaphore: Arc::new(Semaphore::new(anti_block::MAX_CONCURRENCY)),
            active_scrapes: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: String,
    pub url: String,
    pub status: String,
    pub created_at: String,
    pub pages_scraped: usize,
    pub files_downloaded: usize,
    pub total_pages: usize,
    pub current_url: Option<String>,
    pub errors: Vec<String>,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    /// Paths relative to `DATA_DIR` written by this job.
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ScrapeQuery {
    pub url: String,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub concurrency: Option<usize>,
    #[serde(default = "default_delay")]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub max_pages: Option<usize>,
    #[serde(default = "default_true")]
    pub same_domain_only: Option<bool>,
    #[serde(default)]
    pub download_assets: Option<bool>,
    #[serde(default = "default_true")]
    pub user_agent_rotation: Option<bool>,
    #[serde(default)]
    pub file_types: Option<Vec<String>>,
    #[serde(default)]
    pub javascript_rendering: Option<bool>,
    #[serde(default)]
    pub take_screenshots: Option<bool>,
    #[serde(default)]
    pub extract_emails: Option<bool>,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default = "default_true")]
    pub deduplicate: Option<bool>,
    #[serde(default = "default_true")]
    pub respect_robots_txt: Option<bool>,
    #[serde(default = "default_true")]
    pub rewrite_urls: Option<bool>,
    #[serde(default)]
    pub generate_index: Option<bool>,
    #[serde(default)]
    pub export_warc: Option<bool>,
    #[serde(default)]
    pub auth_username: Option<String>,
    #[serde(default)]
    pub auth_password: Option<String>,
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub rate_limit: Option<u64>,
    #[serde(default)]
    pub deep_mode: Option<bool>,
    #[serde(default)]
    pub extract_structured: Option<bool>,
    #[serde(default)]
    pub nlp_enabled: Option<bool>,
    #[serde(default)]
    pub custom_selectors: Option<Vec<String>>,
    #[serde(default = "default_export_format")]
    pub export_format: Option<String>,
    #[serde(default)]
    pub extractor_endpoint: Option<String>,
    #[serde(default)]
    pub proxy_list: Option<Vec<String>>,
    #[serde(default)]
    pub use_proxies: Option<bool>,
    #[serde(default = "default_retry_count")]
    pub retry_count: Option<u32>,
}

fn default_retry_count() -> Option<u32> {
    Some(3)
}

fn default_delay() -> Option<u64> {
    Some(1000)
}
fn default_true() -> Option<bool> {
    Some(true)
}
fn default_export_format() -> Option<String> {
    Some("json".into())
}

impl ScrapeQuery {
    /// Converts the request into a sanitized [`ScrapeConfig`].
    pub fn into_config(self) -> ScrapeConfig {
        ScrapeConfig {
            url: self.url,
            depth: self.depth.unwrap_or(2).min(10),
            concurrency: anti_block::clamp_concurrency(self.concurrency.unwrap_or(3)),
            delay_ms: self.delay_ms.unwrap_or(1000).clamp(0, 60_000),
            max_pages: self.max_pages.unwrap_or(100).clamp(1, 10_000),
            same_domain_only: self.same_domain_only.unwrap_or(true),
            download_assets: self.download_assets.unwrap_or(true),
            user_agent_rotation: self.user_agent_rotation.unwrap_or(true),
            file_types: self.file_types.unwrap_or_default(),
            javascript_rendering: self.javascript_rendering.unwrap_or(false),
            take_screenshots: self.take_screenshots.unwrap_or(false),
            extract_emails: self.extract_emails.unwrap_or(false),
            webhook_url: self.webhook_url.unwrap_or_default(),
            deduplicate: self.deduplicate.unwrap_or(true),
            respect_robots_txt: self.respect_robots_txt.unwrap_or(true),
            rewrite_urls: self.rewrite_urls.unwrap_or(true),
            generate_index: self.generate_index.unwrap_or(false),
            export_warc: self.export_warc.unwrap_or(false),
            auth_username: self.auth_username.unwrap_or_default(),
            auth_password: self.auth_password.unwrap_or_default(),
            auth_mode: self.auth_mode.unwrap_or_default(),
            rate_limit: self.rate_limit.unwrap_or(0),
            deep_mode: self.deep_mode.unwrap_or(false),
            extract_structured: self.extract_structured.unwrap_or(false),
            nlp_enabled: self.nlp_enabled.unwrap_or(false),
            custom_selectors: self.custom_selectors.unwrap_or_default(),
            export_format: self.export_format.unwrap_or_else(|| "json".into()),
            extractor_endpoint: self.extractor_endpoint.unwrap_or_default(),
            proxy_list: self.proxy_list.unwrap_or_default(),
            use_proxies: self.use_proxies.unwrap_or(false),
            retry_count: self.retry_count.unwrap_or(3),
        }
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/scrape", post(start_scrape))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/stream", get(stream_job))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/jobs/{id}", delete(delete_job))
        .route("/jobs/{id}/export", post(export_job))
        .route("/jobs/{id}/download", get(download_job_zip))
        .route("/analyses", get(list_analyses))
        .route("/analyses/{id}", get(get_analysis))
        .route("/analyses/{id}/export", get(export_analysis))
        .route("/analyses/{id}", delete(delete_analysis))
        .route("/database/export", get(export_database))
        .route("/database/import", post(import_database))
        .route("/health", get(health_check))
        .route("/files", get(list_files))
        .route("/files/{*path}", get(get_file))
        .route("/search", get(search_files))
        .route("/stats", get(get_stats))
        .route("/python/docs", get(python_docs_proxy))
        .route("/deep/scrape", post(start_deep_scrape))
        .route("/deep/batch", post(start_deep_batch))
        .route("/deep/crawl", post(start_deep_crawl))
        .route("/deep/crawl/{id}/status", get(get_deep_crawl_status))
        .route("/deep/crawl/{id}/results", get(get_deep_crawl_results))
        .route("/deep/crawl/{id}/cancel", post(cancel_deep_crawl))
        .route("/deep/results", get(list_deep_results))
        .route("/deep/results", delete(clear_deep_results))
        .route("/deep/results/{id}", get(get_deep_result))
        .route("/deep/results/{id}", delete(delete_deep_result))
        .route("/deep/results.csv", get(export_deep_csv))
        .route("/database/clear", post(clear_database))
        .route("/schedules", get(list_schedules).post(create_schedule))
        .route("/schedules/{id}", delete(delete_schedule))
        .with_state(state)
}

// ── scrape lifecycle ────────────────────────────────────────────────────

/// Creates the job record and starts the crawl. Shared by the HTTP handler and
/// the scheduler so both take exactly the same code path.
pub fn launch_scrape(state: &AppState, config: Arc<ScrapeConfig>) -> JobInfo {
    let job_id = Uuid::new_v4().to_string();
    let job_info = JobInfo {
        id: job_id.clone(),
        url: config.url.clone(),
        status: "queued".into(),
        created_at: Utc::now().to_rfc3339(),
        pages_scraped: 0,
        files_downloaded: 0,
        total_pages: config.max_pages,
        current_url: None,
        errors: Vec::new(),
        emails: Vec::new(),
        phones: Vec::new(),
        files: Vec::new(),
    };

    state.jobs.insert(job_id.clone(), job_info.clone());
    spawn_scrape(state.clone(), job_id, config);
    job_info
}

fn spawn_scrape(state: AppState, job_id: String, config: Arc<ScrapeConfig>) {
    let storage = state.storage.clone();
    let jobs = state.jobs.clone();
    let downloaders = state.downloaders.clone();
    let db = state.db.clone();
    let sem = state.scrape_semaphore.clone();
    let active = state.active_scrapes.clone();

    tokio::spawn(async move {
        let _permit = match sem.acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return,
        };
        active.fetch_add(1, Ordering::Relaxed);
        let (tx, mut rx) = mpsc::channel::<ScrapeProgress>(100);

        let downloader = match Downloader::new(config, storage).await {
            Ok(d) => Arc::new(d),
            Err(e) => {
                if let Some(mut job) = jobs.get_mut(&job_id) {
                    job.status = "failed".into();
                    job.errors.push(e);
                    if let Some(ref db) = db {
                        let snapshot = job.clone();
                        drop(job);
                        let _ = db.save_job(snapshot).await;
                    }
                }
                active.fetch_sub(1, Ordering::Relaxed);
                return;
            }
        };

        downloaders.insert(job_id.clone(), downloader.clone());
        {
            let mut job = match jobs.get_mut(&job_id) {
                Some(job) => job,
                None => return,
            };
            job.status = "running".into();
            let snapshot = job.clone();
            drop(job);
            if let Some(ref db) = db {
                let _ = db.save_job(snapshot).await;
            }
        }

        let dl = downloader.clone();
        let jid = job_id.clone();
        let j = jobs.clone();
        let dls = downloaders.clone();
        let db2 = db.clone();
        tokio::spawn(async move {
            dl.run(tx).await;
            if let Some(mut job) = j.get_mut(&jid) {
                if job.status != "cancelled" && job.status != "failed" {
                    job.status = "completed".into();
                }
                let snapshot = job.clone();
                drop(job);
                if let Some(ref db) = db2 {
                    let _ = db.save_job(snapshot).await;
                }
            }
            dls.remove(&jid);
        });

        // Throttled persistence: write on state transitions and at most once
        // every 2 seconds while the scrape is running.
        let mut last_saved = tokio::time::Instant::now() - Duration::from_secs(10);
        let mut last_status = String::new();
        while let Some(progress) = rx.recv().await {
            let snapshot = {
                let Some(mut job) = jobs.get_mut(&job_id) else {
                    break;
                };
                job.pages_scraped = progress.pages_scraped;
                job.files_downloaded = progress.files_downloaded;
                job.current_url = progress.current_url.clone();
                job.status = progress.status.clone();
                job.errors = progress.errors;
                job.emails = progress.emails;
                job.phones = progress.phones;
                job.files = progress.saved_files;
                job.clone()
            };

            let terminal = matches!(
                snapshot.status.as_str(),
                "completed" | "failed" | "cancelled"
            );
            if terminal
                || snapshot.status != last_status
                || last_saved.elapsed() >= Duration::from_secs(2)
            {
                if let Some(ref db) = db {
                    let _ = db.save_job(snapshot.clone()).await;
                }
                last_saved = tokio::time::Instant::now();
                last_status = snapshot.status.clone();
            }

            if let Some(deep) = progress.deep_extracted {
                let deep_result = DeepResult {
                    id: Uuid::new_v4().to_string(),
                    job_id: job_id.clone(),
                    url: snapshot.current_url.clone().unwrap_or_default(),
                    structured_data: deep.get("structured").cloned(),
                    nlp_data: deep.get("nlp").cloned(),
                    extracted: crate::scraper::extractor::ExtractedData {
                        emails: deep
                            .get("emails")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default(),
                        phones: deep
                            .get("phones")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default(),
                    },
                    created_at: Utc::now().to_rfc3339(),
                };
                if let Some(ref db) = db {
                    let _ = db.save_deep_result(deep_result).await;
                }
            }
        }
        active.fetch_sub(1, Ordering::Relaxed);
    });
}

async fn start_scrape(
    State(state): State<AppState>,
    Json(query): Json<ScrapeQuery>,
) -> Result<Json<JobInfo>, (StatusCode, Json<serde_json::Value>)> {
    if query.url.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "URL is required"})),
        ));
    }
    let config = Arc::new(query.into_config());
    Ok(Json(launch_scrape(&state, config)))
}

async fn download_job_zip(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let job = state
        .jobs
        .get(&id)
        .map(|j| j.value().clone())
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut buf = Vec::new();
    let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut added = 0usize;
    for path in &job.files {
        if let Ok(data) = state.storage.read_file(path).await {
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            let name = if name.is_empty() { path.clone() } else { name };
            let _ = zip_writer.start_file(name, options);
            let _ = std::io::Write::write_all(&mut zip_writer, &data);
            added += 1;
        }
    }
    if added == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    if zip_writer.finish().is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let short_id = id.split('-').next().unwrap_or(&id);
    let filename = format!("shinobi-job-{}.zip", short_id);
    let disposition = format!("attachment; filename=\"{}\"", filename);
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/zip"),
    );
    if let Ok(h) = axum::http::HeaderValue::from_str(&disposition) {
        headers.insert(axum::http::header::CONTENT_DISPOSITION, h);
    }
    Ok((headers, buf))
}

async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<FilesQuery>,
) -> Json<serde_json::Value> {
    let mut all: Vec<JobInfo> = state.jobs.iter().map(|e| e.value().clone()).collect();
    all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let total = all.len();
    let items: Vec<JobInfo> = all
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .collect();
    Json(json!({"items": items, "total": total, "offset": query.offset, "limit": query.limit}))
}

async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobInfo>, StatusCode> {
    state
        .jobs
        .get(&id)
        .map(|j| Json(j.value().clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn export_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let job = state.jobs.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let job = job.value().clone();
    Ok(Json(job_export_json(&job)))
}

fn job_export_json(job: &JobInfo) -> JsonValue {
    json!({
        "job": {
            "id": job.id,
            "url": job.url,
            "status": job.status,
            "created_at": job.created_at,
            "pages_scraped": job.pages_scraped,
            "files_downloaded": job.files_downloaded,
            "total_pages": job.total_pages,
        },
        "analysis": job_to_analysis(job),
        "files": job.files,
        "emails": job.emails,
        "phones": job.phones,
        "exported_at": Utc::now().to_rfc3339(),
    })
}

// ── SSE stream (xwa-sdk Event envelope) ─────────────────────────────────

async fn send_xwa_event(tx: &mpsc::Sender<Result<Event, Infallible>>, event: ContractEvent) {
    let name = event.event_type.as_str();
    let data = serde_json::to_string(&event).unwrap_or_default();
    let _ = tx.send(Ok(Event::default().event(name).data(data))).await;
}

fn event_stream(state: AppState, id: String) -> impl Stream<Item = Result<Event, Infallible>> {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);

    tokio::spawn(async move {
        let mut seq: i64 = 0;

        let Some(initial) = state.jobs.get(&id).map(|j| j.value().clone()) else {
            seq += 1;
            send_xwa_event(
                &tx,
                ContractEvent::shinobi(
                    seq,
                    EventType::AnalysisCompleted,
                    &id,
                    Some(json!({"status": "COMPLETED", "message": "job removed"})),
                ),
            )
            .await;
            return;
        };

        if let Some(error) = terminal_error(&initial) {
            seq += 1;
            send_xwa_event(
                &tx,
                ContractEvent::shinobi(
                    seq,
                    EventType::AnalysisError,
                    &id,
                    Some(serde_json::to_value(error).unwrap_or_default()),
                ),
            )
            .await;
            return;
        }

        seq += 1;
        send_xwa_event(
            &tx,
            ContractEvent::shinobi(
                seq,
                EventType::AnalysisStarted,
                &id,
                Some(json!({
                    "url": initial.url,
                    "total_pages": initial.total_pages,
                    "status": initial.status,
                })),
            ),
        )
        .await;

        // Start empty so a client that connects mid-scrape still receives the
        // items/errors accumulated so far as `item_found`/`log` events.
        let mut last_pages = initial.pages_scraped;
        let mut known_emails: HashSet<String> = HashSet::new();
        let mut known_phones: HashSet<String> = HashSet::new();
        let mut known_errors = 0usize;

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let Some(job) = state.jobs.get(&id).map(|j| j.value().clone()) else {
                seq += 1;
                send_xwa_event(
                    &tx,
                    ContractEvent::shinobi(
                        seq,
                        EventType::AnalysisCompleted,
                        &id,
                        Some(json!({"status": "COMPLETED", "message": "job removed"})),
                    ),
                )
                .await;
                break;
            };

            if job.pages_scraped != last_pages {
                let percent = if job.total_pages > 0 {
                    ((job.pages_scraped as f64 / job.total_pages as f64) * 100.0).min(100.0)
                } else {
                    0.0
                };
                seq += 1;
                send_xwa_event(
                    &tx,
                    ContractEvent::shinobi(
                        seq,
                        EventType::AnalysisProgress,
                        &id,
                        Some(json!({
                            "percent": percent.round(),
                            "pages_scraped": job.pages_scraped,
                            "files_downloaded": job.files_downloaded,
                            "total_pages": job.total_pages,
                            "current_url": job.current_url,
                        })),
                    ),
                )
                .await;

                seq += 1;
                send_xwa_event(
                    &tx,
                    ContractEvent::shinobi(
                        seq,
                        EventType::Log,
                        &id,
                        Some(json!({
                            "level": "info",
                            "message": format!(
                                "[PAGE] {}/{} {}",
                                job.pages_scraped,
                                job.total_pages,
                                job.current_url.clone().unwrap_or_default()
                            ),
                        })),
                    ),
                )
                .await;
                last_pages = job.pages_scraped;
            }

            for email in &job.emails {
                if known_emails.insert(email.clone()) {
                    seq += 1;
                    send_xwa_event(
                        &tx,
                        ContractEvent::shinobi(
                            seq,
                            EventType::ItemFound,
                            &id,
                            Some(json!({
                                "kind": "email",
                                "value": email,
                                "url": job.current_url,
                            })),
                        ),
                    )
                    .await;
                }
            }
            for phone in &job.phones {
                if known_phones.insert(phone.clone()) {
                    seq += 1;
                    send_xwa_event(
                        &tx,
                        ContractEvent::shinobi(
                            seq,
                            EventType::ItemFound,
                            &id,
                            Some(json!({
                                "kind": "phone",
                                "value": phone,
                                "url": job.current_url,
                            })),
                        ),
                    )
                    .await;
                }
            }

            for error in job.errors.iter().skip(known_errors) {
                seq += 1;
                send_xwa_event(
                    &tx,
                    ContractEvent::shinobi(
                        seq,
                        EventType::Log,
                        &id,
                        Some(json!({"level": "error", "message": error})),
                    ),
                )
                .await;
            }
            known_errors = job.errors.len();

            let terminal = matches!(job.status.as_str(), "completed" | "failed" | "cancelled");
            if terminal {
                seq += 1;
                match job.status.as_str() {
                    "failed" => {
                        send_xwa_event(
                            &tx,
                            ContractEvent::shinobi(
                                seq,
                                EventType::AnalysisError,
                                &id,
                                Some(
                                    serde_json::to_value(ContractError {
                                        code: "SCRAPE_FAILED".into(),
                                        message: if job.errors.is_empty() {
                                            "scrape failed".into()
                                        } else {
                                            job.errors.join("; ")
                                        },
                                        detail: Some(json!({"url": job.url})),
                                        retryable: false,
                                    })
                                    .unwrap_or_default(),
                                ),
                            ),
                        )
                        .await;
                    }
                    _ => {
                        send_xwa_event(
                            &tx,
                            ContractEvent::shinobi(
                                seq,
                                EventType::AnalysisCompleted,
                                &id,
                                Some(json!({
                                    "status": AnalysisStatus::from_job_status(&job.status),
                                    "pages_scraped": job.pages_scraped,
                                    "files_downloaded": job.files_downloaded,
                                    "emails": job.emails,
                                    "phones": job.phones,
                                    "files": job.files,
                                })),
                            ),
                        )
                        .await;
                    }
                }
                break;
            }
        }
    });

    tokio_stream::wrappers::ReceiverStream::new(rx)
}

fn terminal_error(job: &JobInfo) -> Option<ContractError> {
    if job.status != "failed" {
        return None;
    }
    Some(ContractError {
        code: "SCRAPE_FAILED".into(),
        message: if job.errors.is_empty() {
            "scrape failed".into()
        } else {
            job.errors.join("; ")
        },
        detail: Some(json!({"url": job.url})),
        retryable: false,
    })
}

async fn stream_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    if state.jobs.contains_key(&id) {
        Ok(Sse::new(event_stream(state, id)))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(downloader) = state.downloaders.get(&id) {
        downloader.cancel();
        if let Some(mut job) = state.jobs.get_mut(&id) {
            job.status = "cancelled".into();
        }
        Ok(Json(json!({"status": "cancelled"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// ── xwa-sdk Analysis aliases ────────────────────────────────────────────

/// Converts a job into the xwa-sdk `Analysis` envelope.
pub fn job_to_analysis(job: &JobInfo) -> Analysis {
    let status = AnalysisStatus::from_job_status(&job.status);
    let emails = job.emails.len() as i64;
    let phones = job.phones.len() as i64;
    let mut by_category = std::collections::BTreeMap::new();
    if emails > 0 {
        by_category.insert("emails".to_string(), emails);
    }
    if phones > 0 {
        by_category.insert("phones".to_string(), phones);
    }

    let error = terminal_error(job).map(|e| ContractError {
        message: e.message,
        ..e
    });

    Analysis {
        id: job.id.clone(),
        tool: Tool::Shinobi,
        target: job.url.clone(),
        status,
        created_at: job.created_at.clone(),
        tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        analysis_type: Some("crawler".into()),
        started_at: None,
        finished_at: None,
        error,
        summary: Some(Summary {
            total_items: Some(emails + phones),
            by_severity: Default::default(),
            by_category,
        }),
    }
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    pub format: Option<String>,
}

async fn list_analyses(
    State(state): State<AppState>,
    Query(query): Query<FilesQuery>,
) -> Json<serde_json::Value> {
    let mut all: Vec<Analysis> = state
        .jobs
        .iter()
        .map(|e| job_to_analysis(e.value()))
        .collect();
    all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let total = all.len();
    let items: Vec<Analysis> = all
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .collect();
    Json(json!({"items": items, "total": total, "offset": query.offset, "limit": query.limit}))
}

async fn get_analysis(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Analysis>, StatusCode> {
    state
        .jobs
        .get(&id)
        .map(|j| Json(job_to_analysis(j.value())))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn export_analysis(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ExportQuery>,
) -> Result<axum::response::Response, StatusCode> {
    let job = state
        .jobs
        .get(&id)
        .map(|j| j.value().clone())
        .ok_or(StatusCode::NOT_FOUND)?;
    let format = query.format.unwrap_or_else(|| "json".into()).to_lowercase();

    let (content_type, body, extension) = match format.as_str() {
        "csv" => ("text/csv; charset=utf-8", analysis_csv(&job), "csv"),
        _ => (
            "application/json",
            serde_json::to_string_pretty(&job_export_json(&job)).unwrap_or_default(),
            "json",
        ),
    };

    let disposition = format!(
        "attachment; filename=\"shinobi-analysis-{}.{}\"",
        id, extension
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(value) = axum::http::HeaderValue::from_str(content_type) {
        headers.insert(axum::http::header::CONTENT_TYPE, value);
    }
    if let Ok(value) = axum::http::HeaderValue::from_str(&disposition) {
        headers.insert(axum::http::header::CONTENT_DISPOSITION, value);
    }
    Ok((headers, body).into_response())
}

/// Minimal CSV export for one analysis (RFC 4180 quoting for list fields).
pub fn analysis_csv(job: &JobInfo) -> String {
    let mut csv = String::from(
        "id,tool,status,target,created_at,pages_scraped,files_downloaded,emails,phones,files\n",
    );
    csv.push_str(&format!(
        "{},{},{},{},{},{},{},{},{},{}\n",
        job.id,
        "shinobi",
        job.status,
        csv_escape(&job.url),
        job.created_at,
        job.pages_scraped,
        job.files_downloaded,
        csv_escape(&job.emails.join("; ")),
        csv_escape(&job.phones.join("; ")),
        csv_escape(&job.files.join("; ")),
    ));
    csv
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

async fn delete_analysis(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    delete_job_impl(&state, &id).await
}

// ── health / files / stats ──────────────────────────────────────────────

async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    let database = match &state.db {
        None => "disabled",
        Some(db) => {
            if db.ping().await {
                "ok"
            } else {
                "error"
            }
        }
    };
    Json(json!({
        "status": "ok",
        "service": "shinobi",
        "version": env!("CARGO_PKG_VERSION"),
        "database": database,
    }))
}

#[derive(Debug, Deserialize)]
pub struct FilesQuery {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

async fn list_files(
    State(state): State<AppState>,
    Query(query): Query<FilesQuery>,
) -> Json<serde_json::Value> {
    let mut all = state
        .storage
        .list_files(&query.prefix)
        .await
        .unwrap_or_default();
    all.sort_by_key(|a| std::cmp::Reverse(a.modified));
    let total = all.len();
    let items: Vec<_> = all
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .collect();
    Json(json!({"items": items, "total": total, "offset": query.offset, "limit": query.limit}))
}

async fn get_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    match state.storage.read_file(&path).await {
        Ok(data) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Ok(([(axum::http::header::CONTENT_TYPE, mime.to_string())], data))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn search_files(
    State(_state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<serde_json::Value> {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "downloads".into());
    let dir = std::path::Path::new(&data_dir);
    let mut results = Vec::new();

    if dir.exists() {
        let q = query.q.to_lowercase();
        let mut entries = Vec::new();
        if let Ok(mut read_dir) = tokio::fs::read_dir(dir).await {
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                if path.is_dir() || path.extension().map(|e| e == "zip").unwrap_or(false) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.contains(&q) {
                    if let Ok(meta) = entry.metadata().await {
                        entries.push(json!({
                            "path": path.strip_prefix(dir).map(|p| p.to_string_lossy()).unwrap_or_default().to_string(),
                            "size": meta.len(),
                            "modified": meta.modified().ok().and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(0),
                        }));
                    }
                }
            }
        }
        entries.sort_by(|a, b| {
            b.get("modified")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .cmp(&a.get("modified").and_then(|v| v.as_u64()).unwrap_or(0))
        });
        results = entries
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();
    }

    Json(json!({"items": results, "total": results.len(), "query": query.q}))
}

async fn get_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "downloads".into());
    let dir = std::path::Path::new(&data_dir);
    let mut disk_size: u64 = 0;
    let mut file_count: u64 = 0;

    if dir.exists() {
        if let Ok(mut read_dir) = tokio::fs::read_dir(dir).await {
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                if path.extension().map(|e| e == "zip").unwrap_or(false) {
                    continue;
                }
                if let Ok(meta) = entry.metadata().await {
                    if meta.is_file() {
                        file_count += 1;
                        disk_size += meta.len();
                    } else if meta.is_dir() {
                        disk_size += dir_size(&path).await;
                    }
                }
            }
        }
    }

    Json(json!({
        "jobs": state.jobs.len(),
        "active_scrapes": state.active_scrapes.load(Ordering::Relaxed),
        "files": file_count,
        "disk_size": disk_size,
        "disk_size_human": format_size_human(disk_size),
    }))
}

fn dir_size_sync(path: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    if let Ok(read_dir) = std::fs::read_dir(path) {
        for entry in read_dir.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total += dir_size_sync(&entry.path());
                } else {
                    total += meta.len();
                }
            }
        }
    }
    total
}

async fn dir_size(path: &std::path::Path) -> u64 {
    tokio::task::spawn_blocking({
        let path = path.to_owned();
        move || dir_size_sync(&path)
    })
    .await
    .unwrap_or(0)
}

fn format_size_human(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{}B", bytes);
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1}KB", bytes as f64 / 1024.0);
    }
    if bytes < 1024 * 1024 * 1024 {
        return format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0));
    }
    format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

// ── database ────────────────────────────────────────────────────────────

async fn export_database(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match &state.db {
        Some(db) => db
            .export_all()
            .await
            .map(Json)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn import_database(
    State(state): State<AppState>,
    Json(payload): Json<ImportPayload>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match &state.db {
        Some(db) => {
            let jobs_for_memory = payload.jobs.clone();
            match db.import_all(payload).await {
                Ok(report) => {
                    for job in jobs_for_memory {
                        state.jobs.insert(job.id.clone(), job);
                    }
                    Ok(Json(json!({
                        "imported": report.jobs + report.deep_results + report.schedules,
                        "jobs": report.jobs,
                        "deep_results": report.deep_results,
                        "schedules": report.schedules,
                    })))
                }
                Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

// ── deep results ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepResult {
    pub id: String,
    pub job_id: String,
    pub url: String,
    pub structured_data: Option<JsonValue>,
    pub nlp_data: Option<JsonValue>,
    pub extracted: crate::scraper::extractor::ExtractedData,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct DeepQuery {
    pub url: String,
    #[serde(default)]
    pub extract_structured: Option<bool>,
    #[serde(default)]
    pub nlp_enabled: Option<bool>,
    #[serde(default)]
    pub custom_selectors: Option<Vec<String>>,
}

fn extractor_endpoint() -> String {
    std::env::var("EXTRACTOR_URL").unwrap_or_else(|_| "http://localhost:9090".into())
}

fn deep_result_from_extractor(url: &str, result: &JsonValue, job_id: &str) -> DeepResult {
    DeepResult {
        id: Uuid::new_v4().to_string(),
        job_id: job_id.to_string(),
        url: url.to_string(),
        structured_data: result.get("structured").cloned(),
        nlp_data: result.get("nlp").cloned(),
        extracted: crate::scraper::extractor::ExtractedData {
            emails: result
                .get("emails")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            phones: result
                .get("phones")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        },
        created_at: Utc::now().to_rfc3339(),
    }
}

async fn start_deep_scrape(
    State(state): State<AppState>,
    Json(query): Json<DeepQuery>,
) -> Result<Json<DeepResult>, (StatusCode, Json<serde_json::Value>)> {
    if query.url.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "URL is required"})),
        ));
    }

    let client = reqwest::Client::new();
    let payload = json!({
        "url": query.url,
        "extract_structured": query.extract_structured.unwrap_or(true),
        "nlp_enabled": query.nlp_enabled.unwrap_or(false),
        "custom_selectors": query.custom_selectors.unwrap_or_default(),
    });

    let resp = match client
        .post(format!("{}/extract", extractor_endpoint()))
        .json(&payload)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Extractor unavailable: {}", e)})),
            ))
        }
    };

    let result: JsonValue = resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Invalid response: {}", e)})),
        )
    })?;

    let deep_result = deep_result_from_extractor(&query.url, &result, "");
    if let Some(ref db) = state.db {
        let _ = db.save_deep_result(deep_result.clone()).await;
    }
    Ok(Json(deep_result))
}

async fn list_deep_results(
    State(state): State<AppState>,
    Query(query): Query<FilesQuery>,
) -> Json<serde_json::Value> {
    let all = match &state.db {
        Some(db) => db.load_deep_results().await.unwrap_or_default(),
        None => Vec::new(),
    };
    let total = all.len();
    let items: Vec<DeepResult> = all
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .collect();
    Json(json!({"items": items, "total": total, "offset": query.offset, "limit": query.limit}))
}

async fn get_deep_result(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeepResult>, StatusCode> {
    match &state.db {
        Some(db) => db
            .get_deep_result(id)
            .await
            .map(Json)
            .map_err(|_| StatusCode::NOT_FOUND),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeepBatchQuery {
    pub urls: Vec<String>,
    #[serde(default)]
    pub extract_structured: Option<bool>,
    #[serde(default)]
    pub nlp_enabled: Option<bool>,
    #[serde(default)]
    pub custom_selectors: Option<Vec<String>>,
}

async fn start_deep_batch(
    State(state): State<AppState>,
    Json(query): Json<DeepBatchQuery>,
) -> Result<Json<Vec<DeepResult>>, (StatusCode, Json<serde_json::Value>)> {
    if query.urls.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "At least one URL is required"})),
        ));
    }

    let client = reqwest::Client::new();
    let endpoint = extractor_endpoint();
    let mut results = Vec::new();
    let cs = query.custom_selectors.clone().unwrap_or_default();

    for url in &query.urls {
        let payload = json!({
            "url": url,
            "extract_structured": query.extract_structured.unwrap_or(true),
            "nlp_enabled": query.nlp_enabled.unwrap_or(false),
            "custom_selectors": cs,
        });
        if let Ok(resp) = client
            .post(format!("{}/extract", endpoint))
            .json(&payload)
            .send()
            .await
        {
            if let Ok(result) = resp.json::<JsonValue>().await {
                let deep_result = deep_result_from_extractor(url, &result, "");
                if let Some(ref db) = state.db {
                    let _ = db.save_deep_result(deep_result.clone()).await;
                }
                results.push(deep_result);
            }
        }
    }

    Ok(Json(results))
}

async fn export_deep_csv(State(state): State<AppState>) -> Result<String, StatusCode> {
    let results = match &state.db {
        Some(db) => db.load_deep_results().await.unwrap_or_default(),
        None => return Err(StatusCode::NOT_FOUND),
    };

    let mut csv = String::from("id,url,created_at,emails,phones,has_structured,has_nlp\n");
    for r in &results {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            r.id,
            csv_escape(&r.url),
            r.created_at,
            csv_escape(&r.extracted.emails.join("; ")),
            csv_escape(&r.extracted.phones.join("; ")),
            if r.structured_data.is_some() {
                "yes"
            } else {
                "no"
            },
            if r.nlp_data.is_some() { "yes" } else { "no" },
        ));
    }
    Ok(csv)
}

#[derive(Debug, Deserialize)]
pub struct DeepCrawlQuery {
    pub url: String,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub max_pages: Option<usize>,
    #[serde(default)]
    pub extract_structured: Option<bool>,
    #[serde(default)]
    pub nlp_enabled: Option<bool>,
    #[serde(default)]
    pub custom_selectors: Option<Vec<String>>,
}

async fn start_deep_crawl(
    State(_state): State<AppState>,
    Json(query): Json<DeepCrawlQuery>,
) -> Result<Json<JsonValue>, (StatusCode, Json<serde_json::Value>)> {
    if query.url.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "URL is required"})),
        ));
    }

    let client = reqwest::Client::new();
    let payload = json!({
        "url": query.url,
        "depth": query.depth.unwrap_or(3),
        "max_pages": query.max_pages.unwrap_or(100),
        "same_domain": true,
        "download_assets": true,
        "file_types": [],
        "extract_structured": query.extract_structured.unwrap_or(true),
        "nlp_enabled": query.nlp_enabled.unwrap_or(false),
        "custom_selectors": query.custom_selectors.unwrap_or_default(),
    });

    match client
        .post(format!("{}/crawl", extractor_endpoint()))
        .json(&payload)
        .send()
        .await
    {
        Ok(r) => {
            let result: JsonValue = r
                .json()
                .await
                .unwrap_or_else(|_| json!({"error": "invalid response"}));
            Ok(Json(result))
        }
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Crawler unavailable: {}", e)})),
        )),
    }
}

async fn get_deep_crawl_status(Path(id): Path<String>) -> Result<Json<JsonValue>, StatusCode> {
    let client = reqwest::Client::new();
    match client
        .get(format!("{}/crawl/{}/status", extractor_endpoint(), id))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => Ok(Json(r.json().await.unwrap_or_default())),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_deep_crawl_results(Path(id): Path<String>) -> Result<Json<JsonValue>, StatusCode> {
    let client = reqwest::Client::new();
    match client
        .get(format!("{}/crawl/{}/results", extractor_endpoint(), id))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => Ok(Json(r.json().await.unwrap_or_default())),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn cancel_deep_crawl(Path(id): Path<String>) -> Result<Json<JsonValue>, StatusCode> {
    let client = reqwest::Client::new();
    match client
        .post(format!("{}/crawl/{}/cancel", extractor_endpoint(), id))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => Ok(Json(r.json().await.unwrap_or_default())),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    delete_job_impl(&state, &id).await
}

/// Removes a job and **only its own artifacts** (never the whole domain
/// directory, which may contain files from other jobs).
async fn delete_job_impl(
    state: &AppState,
    id: &str,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let job = state
        .jobs
        .get(id)
        .map(|j| j.value().clone())
        .ok_or(StatusCode::NOT_FOUND)?;

    state.jobs.remove(id);
    state.downloaders.remove(id);
    if let Some(ref db) = state.db {
        let _ = db.delete_job(id.to_string()).await;
    }
    for path in &job.files {
        let _ = state.storage.remove_file(path).await;
    }
    Ok(Json(
        json!({"status": "deleted", "removed_files": job.files.len()}),
    ))
}

async fn delete_deep_result(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match &state.db {
        Some(db) => {
            db.delete_deep_result(id)
                .await
                .map_err(|_| StatusCode::NOT_FOUND)?;
            Ok(Json(json!({"status": "deleted"})))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn clear_deep_results(State(state): State<AppState>) -> Json<serde_json::Value> {
    if let Some(ref db) = state.db {
        let _ = db.clear_deep_results().await;
    }
    Json(json!({"status": "cleared"}))
}

async fn clear_database(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.jobs.clear();
    state.downloaders.clear();
    if let Some(ref db) = state.db {
        let _ = db.clear_all().await;
    }
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "downloads".into());
    let dir = std::path::Path::new(&data_dir);
    if dir.exists() {
        let _ = tokio::fs::remove_dir_all(dir).await;
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    Json(json!({"status": "cleared"}))
}

// ── schedules + scheduler ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub url: String,
    pub interval_min: u64,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSchedule {
    pub url: String,
    pub interval_min: u64,
    pub config: Option<serde_json::Value>,
}

/// Pure decision used by the scheduler (and unit-tested with an injected
/// clock, no network).
pub fn schedule_is_due(sched: &Schedule, now: DateTime<Utc>) -> bool {
    if !sched.enabled {
        return false;
    }
    match DateTime::parse_from_rfc3339(&sched.next_run) {
        Ok(next) => next.with_timezone(&Utc) <= now,
        Err(_) => false,
    }
}

pub fn next_run_at(now: DateTime<Utc>, interval_min: u64) -> String {
    let minutes = interval_min.max(1) as i64;
    (now + chrono::Duration::minutes(minutes)).to_rfc3339()
}

/// Builds a [`ScrapeConfig`] from a stored schedule, falling back to the
/// schedule URL and clamping politeness limits.
pub fn schedule_config(sched: &Schedule) -> Arc<ScrapeConfig> {
    let mut config: ScrapeConfig = serde_json::from_value(sched.config.clone()).unwrap_or_default();
    if config.url.trim().is_empty() {
        config.url = sched.url.clone();
    }
    config.concurrency = anti_block::clamp_concurrency(config.concurrency);
    config.depth = config.depth.min(10);
    config.max_pages = config.max_pages.clamp(1, 10_000);
    Arc::new(config)
}

/// Scheduler loop: launches real scrapes for `enabled=1` schedules whose
/// `next_run` is in the past and advances `last_run`/`next_run`.
pub async fn scheduler_worker(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;

        let Some(db) = state.db.clone() else {
            continue;
        };
        let schedules = match db.load_schedules().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("scheduler: could not load schedules: {}", e);
                continue;
            }
        };

        let now = Utc::now();
        for sched in schedules.into_iter().filter(|s| schedule_is_due(s, now)) {
            let config = schedule_config(&sched);
            let job = launch_scrape(&state, config);
            tracing::info!(
                "scheduler: launched job {} for schedule {} ({})",
                job.id,
                sched.id,
                sched.url
            );
            let updated = Schedule {
                last_run: Some(now.to_rfc3339()),
                next_run: next_run_at(now, sched.interval_min),
                ..sched
            };
            if let Err(e) = db.save_schedule(updated).await {
                tracing::warn!("scheduler: could not update schedule: {}", e);
            }
        }
    }
}

async fn list_schedules(State(state): State<AppState>) -> Json<Vec<Schedule>> {
    match &state.db {
        Some(db) => Json(db.load_schedules().await.unwrap_or_default()),
        None => Json(Vec::new()),
    }
}

async fn create_schedule(
    State(state): State<AppState>,
    Json(query): Json<CreateSchedule>,
) -> Result<Json<Schedule>, (StatusCode, Json<serde_json::Value>)> {
    if query.url.trim().is_empty() || query.interval_min < 5 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Valid URL and interval >= 5 min required"})),
        ));
    }

    let next_run = next_run_at(Utc::now(), query.interval_min);
    let schedule = Schedule {
        id: Uuid::new_v4().to_string(),
        url: query.url,
        interval_min: query.interval_min,
        config: query.config.unwrap_or_else(|| json!({})),
        enabled: true,
        last_run: None,
        next_run,
        created_at: Utc::now().to_rfc3339(),
    };

    if let Some(ref db) = state.db {
        db.save_schedule(schedule.clone()).await.map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "could not save schedule"})),
            )
        })?;
    }

    Ok(Json(schedule))
}

async fn delete_schedule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match &state.db {
        Some(db) => {
            db.delete_schedule(id)
                .await
                .map_err(|_| StatusCode::NOT_FOUND)?;
            Ok(Json(json!({"status": "deleted"})))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn python_docs_proxy() -> Result<impl IntoResponse, StatusCode> {
    match reqwest::get(format!("{}/docs", extractor_endpoint())).await {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            Ok(([("Content-Type", "text/html; charset=utf-8")], body))
        }
        Err(_) => Err(StatusCode::BAD_GATEWAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule(next_run: &str, enabled: bool) -> Schedule {
        Schedule {
            id: "s1".into(),
            url: "https://example.com".into(),
            interval_min: 30,
            config: json!({}),
            enabled,
            last_run: None,
            next_run: next_run.to_string(),
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn due_decision_uses_injected_clock() {
        let now = DateTime::parse_from_rfc3339("2026-09-12T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert!(schedule_is_due(
            &schedule("2026-09-12T09:59:00Z", true),
            now
        ));
        assert!(!schedule_is_due(
            &schedule("2026-09-12T10:01:00Z", true),
            now
        ));
        assert!(!schedule_is_due(
            &schedule("2026-09-12T09:00:00Z", false),
            now
        ));
        assert!(!schedule_is_due(&schedule("not-a-date", true), now));
    }

    #[test]
    fn next_run_advances_by_interval() {
        let now = DateTime::parse_from_rfc3339("2026-09-12T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_at(now, 30);
        let parsed = DateTime::parse_from_rfc3339(&next).unwrap();
        assert_eq!(
            parsed.with_timezone(&Utc),
            now + chrono::Duration::minutes(30)
        );
    }

    #[test]
    fn schedule_config_uses_schedule_url_and_clamps() {
        let sched = schedule("2026-09-12T09:00:00Z", true);
        let config = schedule_config(&sched);
        assert_eq!(config.url, "https://example.com");
        assert!(config.concurrency <= anti_block::MAX_CONCURRENCY);

        let sched = Schedule {
            config: json!({"url": "https://other.example", "concurrency": 99, "max_pages": 0}),
            ..sched
        };
        let config = schedule_config(&sched);
        assert_eq!(config.url, "https://other.example");
        assert_eq!(config.concurrency, anti_block::MAX_CONCURRENCY);
        assert_eq!(config.max_pages, 1);
    }

    #[test]
    fn job_to_analysis_maps_status_and_summary() {
        let job = JobInfo {
            id: "j1".into(),
            url: "https://example.com".into(),
            status: "completed".into(),
            created_at: "2026-09-12T10:00:00Z".into(),
            pages_scraped: 3,
            files_downloaded: 2,
            total_pages: 10,
            current_url: None,
            errors: vec![],
            emails: vec!["a@b.c".into()],
            phones: vec!["+34 600 000 000".into()],
            files: vec!["example.com/index.html".into()],
        };
        let analysis = job_to_analysis(&job);
        assert_eq!(analysis.id, "j1");
        assert_eq!(analysis.status, AnalysisStatus::Completed);
        assert_eq!(analysis.target, "https://example.com");
        assert_eq!(analysis.summary.as_ref().unwrap().total_items, Some(2));
        assert_eq!(
            analysis.summary.as_ref().unwrap().by_category.get("emails"),
            Some(&1)
        );
    }

    #[test]
    fn failed_job_maps_to_error_analysis() {
        let job = JobInfo {
            id: "j2".into(),
            url: "https://example.com".into(),
            status: "failed".into(),
            created_at: "2026-09-12T10:00:00Z".into(),
            pages_scraped: 0,
            files_downloaded: 0,
            total_pages: 1,
            current_url: None,
            errors: vec!["boom".into()],
            emails: vec![],
            phones: vec![],
            files: vec![],
        };
        let analysis = job_to_analysis(&job);
        assert_eq!(analysis.status, AnalysisStatus::Error);
        assert_eq!(analysis.error.as_ref().unwrap().code, "SCRAPE_FAILED");
    }

    #[test]
    fn csv_export_escapes_separators() {
        let job = JobInfo {
            id: "j3".into(),
            url: "https://example.com/a,b".into(),
            status: "completed".into(),
            created_at: "2026-09-12T10:00:00Z".into(),
            pages_scraped: 1,
            files_downloaded: 1,
            total_pages: 1,
            current_url: None,
            errors: vec![],
            emails: vec!["a@b.c".into()],
            phones: vec![],
            files: vec![],
        };
        let csv = analysis_csv(&job);
        assert!(csv.contains("\"https://example.com/a,b\""));
        assert!(csv.starts_with("id,tool,status,target"));
    }
}
