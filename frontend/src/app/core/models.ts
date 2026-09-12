/**
 * Shared domain types for the Shinobi UI.
 *
 * REST payloads follow the legacy `/api/jobs/*` shapes; the SSE stream uses the
 * xwa-sdk `Event` envelope (see `events.ts` for the reducer).
 */

export interface ScrapeConfig {
  url: string;
  depth?: number;
  concurrency?: number;
  delay_ms?: number;
  max_pages?: number;
  same_domain_only?: boolean;
  download_assets?: boolean;
  user_agent_rotation?: boolean;
  javascript_rendering?: boolean;
  /** UI keeps a comma-separated string; ApiService converts it to string[]. */
  file_types?: string;
  respect_robots_txt?: boolean;
  deduplicate?: boolean;
  take_screenshots?: boolean;
  extract_emails?: boolean;
  webhook_url?: string;
  rewrite_urls?: boolean;
  generate_index?: boolean;
  export_warc?: boolean;
  auth_username?: string;
  auth_password?: string;
  auth_mode?: string;
  rate_limit?: number;
  deep_mode?: boolean;
  extract_structured?: boolean;
  nlp_enabled?: boolean;
  custom_selectors?: string[];
}

export interface JobInfo {
  id: string;
  url: string;
  status: string;
  created_at: string;
  pages_scraped: number;
  files_downloaded: number;
  total_pages: number;
  current_url: string | null;
  errors: string[];
  emails: string[];
  phones: string[];
  /** Paths relative to DATA_DIR written by this job (persisted backend field). */
  files?: string[];
}

export interface FileInfo {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: number;
}

export interface DeepConfig {
  url: string;
  extract_structured?: boolean;
  nlp_enabled?: boolean;
  custom_selectors?: string[];
}

export interface DeepResult {
  id: string;
  job_id: string;
  url: string;
  structured_data: unknown;
  nlp_data: unknown;
  extracted: {
    emails: string[];
    phones: string[];
  };
  created_at: string;
}

export interface DeepBatchQuery {
  urls: string[];
  extract_structured?: boolean;
  nlp_enabled?: boolean;
  custom_selectors?: string[];
}

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

export interface Schedule {
  id: string;
  url: string;
  interval_min: number;
  config?: unknown;
  enabled: boolean;
  last_run: string | null;
  next_run: string;
  created_at: string;
}

export interface DiskStats {
  jobs: number;
  active_scrapes: number;
  files: number;
  disk_size: number;
  disk_size_human: string;
}

export interface HealthResponse {
  status: string;
  service?: string;
  version: string;
  database: string;
}

/** Result of probing the Python extractor through the backend proxy. */
export type ExtractorStatus = 'online' | 'offline' | 'unknown';

export type XwaEventType =
  | 'analysis_started'
  | 'analysis_progress'
  | 'item_found'
  | 'log'
  | 'analysis_completed'
  | 'analysis_error';

/** xwa-sdk `Event` envelope as sent over SSE (`data:` JSON). */
export interface XwaEvent {
  seq: number;
  type: XwaEventType | string;
  tool: string;
  analysis_id: string;
  ts: string;
  payload: Record<string, unknown> | null;
}

/** Body returned by `POST /api/jobs/{id}/export`. */
export interface JobExport {
  job?: Partial<JobInfo>;
  analysis?: unknown;
  files?: string[];
  emails?: string[];
  phones?: string[];
  exported_at?: string;
}

export interface PythonCrawlPage {
  url: string;
  emails?: string[];
  phones?: string[];
  structured?: unknown;
  nlp?: unknown;
  metadata?: unknown;
}

export interface PythonCrawlResult {
  pages?: number;
  files?: number;
  zip_path?: string;
  domain?: string;
  results?: PythonCrawlPage[];
}

export interface PythonCrawlStatus {
  status: string;
  pages?: number;
  files?: number;
  progress_pct?: number;
  log?: string[];
}
