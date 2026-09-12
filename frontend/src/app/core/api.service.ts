import { HttpClient, HttpErrorResponse } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable, catchError, map, of } from 'rxjs';

import { environment } from '../../environments/environment';
import {
  DeepBatchQuery,
  DeepConfig,
  DeepResult,
  DiskStats,
  ExtractorStatus,
  FileInfo,
  HealthResponse,
  JobExport,
  JobInfo,
  PaginatedResponse,
  PythonCrawlResult,
  PythonCrawlStatus,
  ScrapeConfig,
  Schedule,
} from './models';

/** Extracts a human message from an HttpErrorResponse or any thrown value. */
export function requestErrorMessage(error: unknown): string {
  if (error instanceof HttpErrorResponse) {
    const body = error.error;
    if (typeof body === 'string' && body.trim()) {
      return body.trim().slice(0, 300);
    }
    if (body && typeof body === 'object') {
      const candidate = (body as { error?: unknown }).error;
      if (typeof candidate === 'string') {
        return candidate;
      }
      if (candidate && typeof candidate === 'object') {
        const message = (candidate as { message?: unknown }).message;
        if (typeof message === 'string') {
          return message;
        }
      }
    }
    return error.message || `HTTP ${error.status}`;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error ?? 'unknown error');
}

@Injectable({ providedIn: 'root' })
export class ApiService {
  private readonly http = inject(HttpClient);
  private readonly base = environment.apiBaseUrl;

  /** Absolute URL for an API path (empty base means same-origin). */
  url(path: string): string {
    return `${this.base}${path}`;
  }

  // ── health ────────────────────────────────────────────────────────────

  health(): Observable<HealthResponse> {
    return this.http.get<HealthResponse>(this.url('/api/health'));
  }

  /**
   * The extractor health endpoint is not proxied by the backend. Probing the
   * `/api/python/docs` proxy tells us whether the sidecar is reachable: 200 →
   * online, 502/503/504 → offline, anything else (missing endpoint, network) →
   * unknown. It never throws, so the UI cannot break on it.
   */
  extractorStatus(): Observable<ExtractorStatus> {
    return this.http.get(this.url('/api/python/docs'), { responseType: 'text' }).pipe(
      map((): ExtractorStatus => 'online'),
      catchError((error: unknown) => {
        if (error instanceof HttpErrorResponse) {
          if (error.status === 502 || error.status === 503 || error.status === 504) {
            return of<ExtractorStatus>('offline');
          }
        }
        return of<ExtractorStatus>('unknown');
      }),
    );
  }

  // ── scrape lifecycle ──────────────────────────────────────────────────

  startScrape(config: ScrapeConfig): Observable<JobInfo> {
    return this.http.post<JobInfo>(this.url('/api/scrape'), this.scrapeBody(config));
  }

  listJobs(offset = 0, limit = 50): Observable<PaginatedResponse<JobInfo>> {
    return this.http.get<PaginatedResponse<JobInfo>>(
      this.url(`/api/jobs?offset=${offset}&limit=${limit}`),
    );
  }

  getJob(id: string): Observable<JobInfo> {
    return this.http.get<JobInfo>(this.url(`/api/jobs/${id}`));
  }

  cancelJob(id: string): Observable<unknown> {
    return this.http.post(this.url(`/api/jobs/${id}/cancel`), {});
  }

  deleteJob(id: string): Observable<unknown> {
    return this.http.delete(this.url(`/api/analyses/${id}`));
  }

  /** SSE endpoint consumed through `LiveService` (EventSource requires a URL). */
  jobStreamUrl(id: string): string {
    return this.url(`/api/jobs/${id}/stream`);
  }

  /** Legacy export used by the client-side JSON download. */
  exportJob(id: string): Observable<JobExport> {
    return this.http.post<JobExport>(this.url(`/api/jobs/${id}/export`), {});
  }

  /** Server-side export (Content-Disposition attachment), `format=json|csv`. */
  exportAnalysisUrl(id: string, format: 'json' | 'csv' = 'json'): string {
    return this.url(`/api/analyses/${id}/export?format=${format}`);
  }

  /** ZIP bundle with every file downloaded by the job. */
  jobZipUrl(id: string): string {
    return this.url(`/api/jobs/${id}/download`);
  }

  // ── files / stats / search ────────────────────────────────────────────

  listFiles(prefix = '', offset = 0, limit = 50): Observable<PaginatedResponse<FileInfo>> {
    return this.http.get<PaginatedResponse<FileInfo>>(
      this.url(`/api/files?prefix=${encodeURIComponent(prefix)}&offset=${offset}&limit=${limit}`),
    );
  }

  fileUrl(path: string): string {
    const encoded = path
      .split('/')
      .map((segment) => encodeURIComponent(segment))
      .join('/');
    return this.url(`/api/files/${encoded}`);
  }

  getFileText(path: string): Observable<string> {
    return this.http.get(this.fileUrl(path), { responseType: 'text' });
  }

  searchFiles(q: string, offset = 0, limit = 50): Observable<PaginatedResponse<FileInfo>> {
    return this.http.get<PaginatedResponse<FileInfo>>(
      this.url(`/api/search?q=${encodeURIComponent(q)}&offset=${offset}&limit=${limit}`),
    );
  }

  getStats(): Observable<DiskStats> {
    return this.http.get<DiskStats>(this.url('/api/stats'));
  }

  // ── deep research (Python extractor) ──────────────────────────────────

  startDeepScrape(config: DeepConfig): Observable<DeepResult> {
    return this.http.post<DeepResult>(this.url('/api/deep/scrape'), config);
  }

  startDeepBatch(query: DeepBatchQuery): Observable<DeepResult[]> {
    return this.http.post<DeepResult[]>(this.url('/api/deep/batch'), query);
  }

  listDeepResults(offset = 0, limit = 50): Observable<PaginatedResponse<DeepResult>> {
    return this.http.get<PaginatedResponse<DeepResult>>(
      this.url(`/api/deep/results?offset=${offset}&limit=${limit}`),
    );
  }

  getDeepResult(id: string): Observable<DeepResult> {
    return this.http.get<DeepResult>(this.url(`/api/deep/results/${id}`));
  }

  deleteDeepResult(id: string): Observable<unknown> {
    return this.http.delete(this.url(`/api/deep/results/${id}`));
  }

  clearDeepResults(): Observable<unknown> {
    return this.http.delete(this.url('/api/deep/results'));
  }

  exportDeepCsv(): Observable<Blob> {
    return this.http.get(this.url('/api/deep/results.csv'), { responseType: 'blob' });
  }

  startPythonCrawl(config: DeepConfig & { depth?: number; max_pages?: number }): Observable<{
    job_id?: string;
    [key: string]: unknown;
  }> {
    return this.http.post<{ job_id?: string; [key: string]: unknown }>(
      this.url('/api/deep/crawl'),
      config,
    );
  }

  crawlStatus(jobId: string): Observable<PythonCrawlStatus> {
    return this.http.get<PythonCrawlStatus>(this.url(`/api/deep/crawl/${jobId}/status`));
  }

  crawlResults(jobId: string): Observable<PythonCrawlResult> {
    return this.http.get<PythonCrawlResult>(this.url(`/api/deep/crawl/${jobId}/results`));
  }

  cancelCrawl(jobId: string): Observable<unknown> {
    return this.http.post(this.url(`/api/deep/crawl/${jobId}/cancel`), {});
  }

  // ── schedules ─────────────────────────────────────────────────────────

  listSchedules(): Observable<Schedule[]> {
    return this.http.get<Schedule[]>(this.url('/api/schedules'));
  }

  createSchedule(url: string, intervalMin: number): Observable<Schedule> {
    return this.http.post<Schedule>(this.url('/api/schedules'), {
      url,
      interval_min: intervalMin,
    });
  }

  deleteSchedule(id: string): Observable<unknown> {
    return this.http.delete(this.url(`/api/schedules/${id}`));
  }

  // ── database ──────────────────────────────────────────────────────────

  exportDatabase(): Observable<Record<string, unknown>> {
    return this.http.get<Record<string, unknown>>(this.url('/api/database/export'));
  }

  importDatabase(payload: Record<string, unknown>): Observable<Record<string, unknown>> {
    return this.http.post<Record<string, unknown>>(this.url('/api/database/import'), payload);
  }

  clearDatabase(): Observable<unknown> {
    return this.http.post(this.url('/api/database/clear'), {});
  }

  // ── helpers ───────────────────────────────────────────────────────────

  /** Mirrors the legacy payload sanitisation (file types + auth fields). */
  private scrapeBody(config: ScrapeConfig): Record<string, unknown> {
    const body: Record<string, unknown> = { ...config };
    const fileTypes = body['file_types'];
    if (typeof fileTypes === 'string' && fileTypes.trim()) {
      body['file_types'] = fileTypes
        .split(',')
        .map((value) => value.trim())
        .filter(Boolean);
    } else {
      delete body['file_types'];
    }

    if (!body['auth_mode']) {
      delete body['auth_username'];
      delete body['auth_password'];
      delete body['auth_mode'];
    }
    return body;
  }
}
