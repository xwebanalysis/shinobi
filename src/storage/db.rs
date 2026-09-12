//! SQLite persistence layer.
//!
//! * PRAGMAs applied on every connection: `journal_mode=WAL`,
//!   `foreign_keys=ON`, `busy_timeout=5000`.
//! * All queries run on `tokio::task::spawn_blocking` so the async runtime is
//!   never blocked by SQLite.
//! * Schema versioning through the `schema_meta(version)` table with an
//!   idempotent migration helper.
//! * Full-database export/import/clear covering `jobs`, `deep_results` and
//!   `schedules`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::Value;
use tracing::info;

use crate::api::routes::{DeepResult, JobInfo, Schedule};

/// Current schema version. Bump it and add a migration step when changing the
/// schema.
pub const SCHEMA_VERSION: u32 = 1;

/// Export format marker understood by the TUI/scripts.
pub const EXPORT_VERSION: &str = "SHINOBI_DB_V1";

#[derive(Clone)]
pub struct DbStore {
    conn: Arc<Mutex<Connection>>,
}

impl DbStore {
    /// Opens (or creates) the database, applies PRAGMAs and migrations.
    ///
    /// This is intentionally synchronous: it is only called once at startup,
    /// before the async workers start serving requests.
    pub fn new(path: &str) -> Result<Self, String> {
        let exists = Path::new(path).exists();
        let conn = Connection::open(path).map_err(|e| format!("Failed to open DB: {}", e))?;

        configure(&conn)?;
        let version = migrate(&conn)?;

        if !exists {
            info!("Created new database at {} (schema v{})", path, version);
        } else {
            info!("Loaded database from {} (schema v{})", path, version);
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs a closure with the locked connection off the async runtime.
    async fn blocking<T, F>(&self, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| format!("DB mutex poisoned: {}", e))?;
            f(&guard)
        })
        .await
        .map_err(|e| format!("DB task join error: {}", e))?
    }

    // ── jobs ────────────────────────────────────────────────────────────

    pub async fn save_job(&self, job: JobInfo) -> Result<(), String> {
        self.blocking(move |conn| save_job_sync(conn, &job)).await
    }

    pub async fn load_jobs(&self) -> Result<Vec<JobInfo>, String> {
        self.blocking(load_jobs_sync).await
    }

    pub async fn delete_job(&self, id: String) -> Result<(), String> {
        self.blocking(move |conn| {
            conn.execute("DELETE FROM jobs WHERE id = ?1", rusqlite::params![id])
                .map_err(|e| format!("Failed to delete job: {}", e))?;
            conn.execute(
                "DELETE FROM deep_results WHERE job_id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| format!("Failed to delete job deep results: {}", e))?;
            Ok(())
        })
        .await
    }

    // ── deep results ────────────────────────────────────────────────────

    pub async fn save_deep_result(&self, result: DeepResult) -> Result<(), String> {
        self.blocking(move |conn| save_deep_result_sync(conn, &result))
            .await
    }

    pub async fn load_deep_results(&self) -> Result<Vec<DeepResult>, String> {
        self.blocking(load_deep_results_sync).await
    }

    pub async fn get_deep_result(&self, id: String) -> Result<DeepResult, String> {
        self.blocking(move |conn| get_deep_result_sync(conn, &id))
            .await
    }

    pub async fn delete_deep_result(&self, id: String) -> Result<(), String> {
        self.blocking(move |conn| {
            let affected = conn
                .execute(
                    "DELETE FROM deep_results WHERE id = ?1",
                    rusqlite::params![id],
                )
                .map_err(|e| format!("Failed to delete deep result: {}", e))?;
            if affected == 0 {
                return Err("Not found".into());
            }
            Ok(())
        })
        .await
    }

    pub async fn delete_deep_results_for_job(&self, job_id: String) -> Result<(), String> {
        self.blocking(move |conn| {
            conn.execute(
                "DELETE FROM deep_results WHERE job_id = ?1",
                rusqlite::params![job_id],
            )
            .map_err(|e| format!("Failed to delete deep results: {}", e))?;
            Ok(())
        })
        .await
    }

    pub async fn clear_deep_results(&self) -> Result<(), String> {
        self.blocking(|conn| {
            conn.execute("DELETE FROM deep_results", [])
                .map_err(|e| format!("Failed to clear deep results: {}", e))?;
            Ok(())
        })
        .await
    }

    // ── schedules ───────────────────────────────────────────────────────

    pub async fn save_schedule(&self, sched: Schedule) -> Result<(), String> {
        self.blocking(move |conn| save_schedule_sync(conn, &sched))
            .await
    }

    pub async fn load_schedules(&self) -> Result<Vec<Schedule>, String> {
        self.blocking(load_schedules_sync).await
    }

    pub async fn delete_schedule(&self, id: String) -> Result<(), String> {
        self.blocking(move |conn| {
            let affected = conn
                .execute("DELETE FROM schedules WHERE id = ?1", rusqlite::params![id])
                .map_err(|e| format!("Failed to delete schedule: {}", e))?;
            if affected == 0 {
                return Err("Not found".into());
            }
            Ok(())
        })
        .await
    }

    // ── export / import / clear ─────────────────────────────────────────

    pub async fn export_all(&self) -> Result<Value, String> {
        let jobs = self.load_jobs().await?;
        let deep_results = self.load_deep_results().await?;
        let schedules = self.load_schedules().await?;

        Ok(serde_json::json!({
            "version": EXPORT_VERSION,
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "jobs": jobs,
            "deep_results": deep_results,
            "schedules": schedules,
        }))
    }

    pub async fn import_all(&self, payload: ImportPayload) -> Result<ImportReport, String> {
        let mut report = ImportReport::default();
        for job in payload.jobs {
            self.save_job(job).await?;
            report.jobs += 1;
        }
        for result in payload.deep_results {
            self.save_deep_result(result).await?;
            report.deep_results += 1;
        }
        for sched in payload.schedules {
            self.save_schedule(sched).await?;
            report.schedules += 1;
        }
        Ok(report)
    }

    pub async fn clear_all(&self) -> Result<(), String> {
        self.blocking(|conn| {
            conn.execute_batch(
                "DELETE FROM jobs; DELETE FROM deep_results; DELETE FROM schedules;",
            )
            .map_err(|e| format!("Failed to clear database: {}", e))?;
            Ok(())
        })
        .await
    }

    /// Trivial connectivity probe used by `/api/health`.
    pub async fn ping(&self) -> bool {
        self.blocking(|conn| {
            conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
        .is_ok()
    }
}

/// Payload accepted by `/api/database/import`.
///
/// `jobs` is the only required field so old exports keep working; the other
/// collections default to empty.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ImportPayload {
    #[serde(default)]
    pub jobs: Vec<JobInfo>,
    #[serde(default)]
    pub deep_results: Vec<DeepResult>,
    #[serde(default)]
    pub schedules: Vec<Schedule>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportReport {
    pub jobs: usize,
    pub deep_results: usize,
    pub schedules: usize,
}

// ── schema setup ────────────────────────────────────────────────────────

fn configure(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("Failed to configure SQLite: {}", e))
}

/// Applies pending migrations idempotently and returns the resulting schema
/// version.
pub fn migrate(conn: &Connection) -> Result<u32, String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_meta (version INTEGER NOT NULL);")
        .map_err(|e| format!("Failed to create schema_meta: {}", e))?;

    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_meta",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| format!("Failed to read schema version: {}", e))?
        .max(0) as u32;

    if current < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
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
                files TEXT NOT NULL DEFAULT '[]',
                data TEXT
            );
            CREATE TABLE IF NOT EXISTS deep_results (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL DEFAULT '',
                url TEXT NOT NULL,
                structured_data TEXT,
                nlp_data TEXT,
                emails TEXT DEFAULT '[]',
                phones TEXT DEFAULT '[]',
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS schedules (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                interval_min INTEGER NOT NULL DEFAULT 60,
                config TEXT NOT NULL DEFAULT '{}',
                enabled INTEGER NOT NULL DEFAULT 1,
                last_run TEXT,
                next_run TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_deep_results_job_id
                ON deep_results(job_id);",
        )
        .map_err(|e| format!("Failed to create schema v1: {}", e))?;
    }

    // Idempotent column additions for databases created before v1.
    ensure_column(conn, "jobs", "files", "TEXT NOT NULL DEFAULT '[]'")?;

    conn.execute_batch("DELETE FROM schema_meta;")
        .map_err(|e| format!("Failed to update schema_meta: {}", e))?;
    conn.execute(
        "INSERT INTO schema_meta (version) VALUES (?1)",
        rusqlite::params![SCHEMA_VERSION as i64],
    )
    .map_err(|e| format!("Failed to write schema version: {}", e))?;

    Ok(SCHEMA_VERSION)
}

fn ensure_column(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<(), String> {
    if !has_column(conn, table, column)? {
        conn.execute_batch(&format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            table, column, ddl
        ))
        .map_err(|e| format!("Failed to add {}.{}: {}", table, column, e))?;
    }
    Ok(())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| format!("Failed to inspect {}: {}", table, e))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| format!("Failed to read table info: {}", e))?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let name: String = row.get(1).map_err(|e| e.to_string())?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

// ── sync helpers ────────────────────────────────────────────────────────

fn parse_json_vec(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

fn to_json_vec(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".into())
}

fn save_job_sync(conn: &Connection, job: &JobInfo) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO jobs
         (id, url, status, created_at, pages_scraped, files_downloaded, total_pages,
          current_url, errors, emails, phones, files, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            job.id,
            job.url,
            job.status,
            job.created_at,
            job.pages_scraped as i64,
            job.files_downloaded as i64,
            job.total_pages as i64,
            job.current_url,
            to_json_vec(&job.errors),
            to_json_vec(&job.emails),
            to_json_vec(&job.phones),
            to_json_vec(&job.files),
            Option::<String>::None,
        ],
    )
    .map_err(|e| format!("Failed to save job: {}", e))?;
    Ok(())
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobInfo> {
    let errors_str: String = row.get(8)?;
    let emails_str: String = row.get(9)?;
    let phones_str: String = row.get(10)?;
    let files_str: String = row.get(11)?;
    Ok(JobInfo {
        id: row.get(0)?,
        url: row.get(1)?,
        status: row.get(2)?,
        created_at: row.get(3)?,
        pages_scraped: row.get::<_, i64>(4)?.max(0) as usize,
        files_downloaded: row.get::<_, i64>(5)?.max(0) as usize,
        total_pages: row.get::<_, i64>(6)?.max(0) as usize,
        current_url: row.get(7)?,
        errors: parse_json_vec(&errors_str),
        emails: parse_json_vec(&emails_str),
        phones: parse_json_vec(&phones_str),
        files: parse_json_vec(&files_str),
    })
}

fn load_jobs_sync(conn: &Connection) -> Result<Vec<JobInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, url, status, created_at, pages_scraped, files_downloaded,
                    total_pages, current_url, errors, emails, phones, files
             FROM jobs ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Failed to prepare jobs query: {}", e))?;

    let rows = stmt
        .query_map([], job_from_row)
        .map_err(|e| format!("Failed to query jobs: {}", e))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read job row: {}", e))
}

fn save_deep_result_sync(conn: &Connection, result: &DeepResult) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO deep_results
         (id, job_id, url, structured_data, nlp_data, emails, phones, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            result.id,
            result.job_id,
            result.url,
            result.structured_data.as_ref().map(|v| v.to_string()),
            result.nlp_data.as_ref().map(|v| v.to_string()),
            to_json_vec(&result.extracted.emails),
            to_json_vec(&result.extracted.phones),
            result.created_at,
        ],
    )
    .map_err(|e| format!("Failed to save deep result: {}", e))?;
    Ok(())
}

fn deep_result_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeepResult> {
    let sd_str: Option<String> = row.get(3)?;
    let nd_str: Option<String> = row.get(4)?;
    let emails_str: String = row.get(5)?;
    let phones_str: String = row.get(6)?;
    Ok(DeepResult {
        id: row.get(0)?,
        job_id: row.get(1)?,
        url: row.get(2)?,
        structured_data: sd_str.and_then(|s| serde_json::from_str(&s).ok()),
        nlp_data: nd_str.and_then(|s| serde_json::from_str(&s).ok()),
        extracted: crate::scraper::extractor::ExtractedData {
            emails: parse_json_vec(&emails_str),
            phones: parse_json_vec(&phones_str),
        },
        created_at: row.get(7)?,
    })
}

const DEEP_COLUMNS: &str = "id, job_id, url, structured_data, nlp_data, emails, phones, created_at";

fn load_deep_results_sync(conn: &Connection) -> Result<Vec<DeepResult>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM deep_results ORDER BY created_at DESC",
            DEEP_COLUMNS
        ))
        .map_err(|e| format!("Failed to prepare deep results query: {}", e))?;
    let rows = stmt
        .query_map([], deep_result_from_row)
        .map_err(|e| format!("Failed to query deep results: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read deep result row: {}", e))
}

fn get_deep_result_sync(conn: &Connection, id: &str) -> Result<DeepResult, String> {
    conn.query_row(
        &format!("SELECT {} FROM deep_results WHERE id = ?1", DEEP_COLUMNS),
        rusqlite::params![id],
        deep_result_from_row,
    )
    .map_err(|_| "Not found".to_string())
}

fn save_schedule_sync(conn: &Connection, sched: &Schedule) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO schedules
         (id, url, interval_min, config, enabled, last_run, next_run, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            sched.id,
            sched.url,
            sched.interval_min as i64,
            serde_json::to_string(&sched.config).unwrap_or_default(),
            sched.enabled as i64,
            sched.last_run,
            sched.next_run,
            sched.created_at,
        ],
    )
    .map_err(|e| format!("Failed to save schedule: {}", e))?;
    Ok(())
}

fn load_schedules_sync(conn: &Connection) -> Result<Vec<Schedule>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, url, interval_min, config, enabled, last_run, next_run, created_at
             FROM schedules ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Failed to prepare schedules query: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            let config_str: String = row.get(3)?;
            Ok(Schedule {
                id: row.get(0)?,
                url: row.get(1)?,
                interval_min: row.get::<_, i64>(2)?.max(0) as u64,
                config: serde_json::from_str(&config_str).unwrap_or_default(),
                enabled: row.get::<_, i64>(4)? != 0,
                last_run: row.get(5)?,
                next_run: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| format!("Failed to query schedules: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read schedule row: {}", e))
}
