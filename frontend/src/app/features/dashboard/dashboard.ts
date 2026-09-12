import { Component, HostListener, OnDestroy, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { Subscription } from 'rxjs';

import { ApiService, requestErrorMessage } from '../../core/api.service';
import { applyJobEvent, isTerminalStatus, logLineFor, progressPercent } from '../../core/events';
import { ExportService } from '../../core/export.service';
import { LiveService } from '../../core/live.service';
import {
  DeepConfig,
  DiskStats,
  JobInfo,
  PythonCrawlPage,
  PythonCrawlResult,
  ScrapeConfig,
  XwaEvent,
} from '../../core/models';
import { MetricCardComponent } from '../../shared/metric-card/metric-card';
import { ProgressComponent } from '../../shared/progress/progress';
import { StatusBadgeComponent } from '../../shared/status-badge/status-badge';
import { TerminalComponent } from '../../shared/terminal/terminal';

interface DashboardStats {
  jobs: number;
  active: number;
  pages: number;
  files: number;
  deep: number;
}

@Component({
  selector: 'app-dashboard',
  standalone: true,
  imports: [
    FormsModule,
    MetricCardComponent,
    ProgressComponent,
    StatusBadgeComponent,
    TerminalComponent,
  ],
  templateUrl: './dashboard.html',
  styleUrl: './dashboard.scss',
})
export class DashboardComponent implements OnInit, OnDestroy {
  private readonly api = inject(ApiService);
  private readonly live = inject(LiveService);
  private readonly router = inject(Router);
  private readonly exporter = inject(ExportService);

  protected mode: 'fast' | 'deep' = 'fast';
  protected deepSubMode: 'single' | 'crawl' | 'batch' | 'pycrawl' = 'single';

  protected config: ScrapeConfig = {
    url: '',
    depth: 2,
    max_pages: 100,
    delay_ms: 1000,
    concurrency: 3,
    same_domain_only: true,
    download_assets: true,
    user_agent_rotation: true,
    javascript_rendering: false,
    respect_robots_txt: true,
    deduplicate: true,
    take_screenshots: false,
    extract_emails: false,
    rewrite_urls: true,
    generate_index: false,
    export_warc: false,
    rate_limit: 0,
  };

  protected deepConfig: DeepConfig = {
    url: '',
    extract_structured: true,
    nlp_enabled: false,
    custom_selectors: [],
  };
  protected deepCrawlConfig = {
    url: '',
    depth: 2,
    max_pages: 50,
    extract_structured: true,
    nlp_enabled: false,
  };
  protected pyCrawlConfig = {
    url: '',
    depth: 3,
    max_pages: 100,
    extract_structured: true,
    nlp_enabled: true,
  };
  protected batchUrls = '';
  protected customSelectorInput = '';

  protected readonly stats = signal<DashboardStats>({
    jobs: 0,
    active: 0,
    pages: 0,
    files: 0,
    deep: 0,
  });
  protected readonly disk = signal<DiskStats | null>(null);

  protected readonly statusMsg = signal('');
  protected readonly statusType = signal<'ok' | 'error' | 'warn' | ''>('');
  protected readonly showKeys = signal(false);

  protected readonly starting = signal(false);
  protected readonly deepStarting = signal(false);
  protected readonly deleting = signal(false);
  protected readonly activeJob = signal<JobInfo | null>(null);
  protected readonly progress = signal(0);
  protected readonly jobLog = signal<string[]>([]);

  protected readonly crawlJobId = signal('');
  protected readonly crawlStatus = signal('');
  protected readonly crawlPages = signal(0);
  protected readonly crawlFiles = signal(0);
  protected readonly crawlProgress = signal(0);
  protected readonly crawlLog = signal<string[]>([]);
  protected readonly crawlResult = signal<PythonCrawlResult | null>(null);
  protected readonly crawlSelected = signal<PythonCrawlPage | null>(null);

  protected readonly crawlPagesList = computed(() => this.crawlResult()?.results ?? []);

  private stream: Subscription | null = null;
  private streamClosed = true;
  private statsTimer: ReturnType<typeof setInterval> | null = null;
  private crawlTimer: ReturnType<typeof setInterval> | null = null;
  private statusTimeout: ReturnType<typeof setTimeout> | null = null;

  @HostListener('document:keydown', ['$event'])
  handleKeydown(event: KeyboardEvent): void {
    if (event.key === '?' && !event.ctrlKey && !event.metaKey) {
      this.showKeys.update((value) => !value);
      return;
    }
    if (event.key === 'Escape') {
      this.showKeys.set(false);
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      if (this.mode === 'fast') {
        this.startScrape();
      } else if (this.deepSubMode === 'single') {
        this.startDeepSingle();
      } else if (this.deepSubMode === 'crawl') {
        this.startDeepCrawl();
      } else if (this.deepSubMode === 'batch') {
        this.startDeepBatch();
      } else if (this.deepSubMode === 'pycrawl') {
        this.startPyCrawl();
      }
    }
  }

  ngOnInit(): void {
    this.loadStats();
    this.statsTimer = setInterval(() => this.loadStats(), 3000);
  }

  ngOnDestroy(): void {
    if (this.statsTimer) {
      clearInterval(this.statsTimer);
    }
    if (this.crawlTimer) {
      clearInterval(this.crawlTimer);
    }
    if (this.statusTimeout) {
      clearTimeout(this.statusTimeout);
    }
    this.stream?.unsubscribe();
    this.stream = null;
  }

  protected setStatus(message: string, type: 'ok' | 'error' | 'warn' = 'ok'): void {
    this.statusMsg.set(message);
    this.statusType.set(type);
    if (this.statusTimeout) {
      clearTimeout(this.statusTimeout);
    }
    this.statusTimeout = setTimeout(() => {
      this.statusMsg.set('');
      this.statusType.set('');
    }, 5000);
  }

  // ── stats ─────────────────────────────────────────────────────────────

  protected loadStats(): void {
    this.api.listJobs(0, 200).subscribe({
      next: (page) => {
        this.stats.update((current) => ({
          ...current,
          jobs: page.total,
          active: page.items.filter((job) => job.status === 'running').length,
          pages: page.items.reduce((sum, job) => sum + job.pages_scraped, 0),
          files: page.items.reduce((sum, job) => sum + job.files_downloaded, 0),
        }));
      },
    });

    this.api.listDeepResults(0, 1).subscribe({
      next: (page) => this.stats.update((current) => ({ ...current, deep: page.total })),
    });

    this.api.getStats().subscribe({
      next: (disk) => this.disk.set(disk),
    });
  }

  // ── fast mode ─────────────────────────────────────────────────────────

  protected startScrape(): void {
    if (!this.config.url.trim()) {
      return;
    }
    this.starting.set(true);
    this.config.deep_mode = false;
    this.api.startScrape(this.config).subscribe({
      next: (job) => {
        this.starting.set(false);
        this.setStatus('JOB STARTED');
        this.attachJob(job);
        this.loadStats();
      },
      error: (error: unknown) => {
        this.starting.set(false);
        this.setStatus(`SCRAPE FAILED: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  // ── deep mode ─────────────────────────────────────────────────────────

  protected startDeepSingle(): void {
    if (!this.deepConfig.url.trim()) {
      return;
    }
    this.deepStarting.set(true);
    const body: DeepConfig = { ...this.deepConfig };
    if (this.customSelectorInput.trim()) {
      body.custom_selectors = this.customSelectorInput
        .split(',')
        .map((selector) => selector.trim())
        .filter(Boolean);
    } else {
      delete body.custom_selectors;
    }
    this.api.startDeepScrape(body).subscribe({
      next: () => {
        this.deepStarting.set(false);
        this.setStatus('EXTRACTION COMPLETE');
        this.openDeepResults();
      },
      error: (error: unknown) => {
        this.deepStarting.set(false);
        this.setStatus(`DEEP RESEARCH: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  protected startDeepCrawl(): void {
    if (!this.deepCrawlConfig.url.trim()) {
      return;
    }
    this.starting.set(true);
    const body: ScrapeConfig = {
      url: this.deepCrawlConfig.url,
      depth: this.deepCrawlConfig.depth,
      max_pages: this.deepCrawlConfig.max_pages,
      deep_mode: true,
      extract_structured: this.deepCrawlConfig.extract_structured,
      nlp_enabled: this.deepCrawlConfig.nlp_enabled,
      download_assets: false,
      same_domain_only: true,
      respect_robots_txt: true,
      delay_ms: 1000,
      concurrency: 3,
      user_agent_rotation: true,
    };
    this.api.startScrape(body).subscribe({
      next: (job) => {
        this.starting.set(false);
        this.setStatus('DEEP CRAWL STARTED');
        this.attachJob(job);
        this.loadStats();
      },
      error: (error: unknown) => {
        this.starting.set(false);
        this.setStatus(`DEEP CRAWL: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  protected startDeepBatch(): void {
    const urls = this.batchUrls
      .split('\n')
      .map((url) => url.trim())
      .filter(Boolean);
    if (!urls.length) {
      this.setStatus('ENTER AT LEAST ONE URL', 'warn');
      return;
    }
    this.deepStarting.set(true);
    this.api.startDeepBatch({ urls, extract_structured: true, nlp_enabled: false }).subscribe({
      next: (results) => {
        this.deepStarting.set(false);
        this.setStatus(`EXTRACTED ${results.length} URLS`);
        this.openDeepResults();
      },
      error: (error: unknown) => {
        this.deepStarting.set(false);
        this.setStatus(`BATCH: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  private openDeepResults(): void {
    void this.router.navigate(['/history'], { queryParams: { tab: 'deep' } });
  }

  // ── python crawl ──────────────────────────────────────────────────────

  protected startPyCrawl(): void {
    if (!this.pyCrawlConfig.url.trim()) {
      return;
    }
    this.deepStarting.set(true);
    this.resetCrawl();

    this.api
      .startPythonCrawl({
        url: this.pyCrawlConfig.url,
        depth: this.pyCrawlConfig.depth,
        max_pages: this.pyCrawlConfig.max_pages,
        extract_structured: this.pyCrawlConfig.extract_structured,
        nlp_enabled: this.pyCrawlConfig.nlp_enabled,
      })
      .subscribe({
        next: (response) => {
          const jobId = typeof response['job_id'] === 'string' ? response['job_id'] : '';
          this.crawlJobId.set(jobId);
          this.deepStarting.set(false);
          if (!jobId) {
            this.setStatus('CRAWL START FAILED: NO JOB ID', 'error');
            return;
          }
          this.crawlStatus.set('running');
          this.setStatus('CRAWL STARTED');
          this.pollCrawl();
        },
        error: (error: unknown) => {
          this.deepStarting.set(false);
          this.setStatus(`PYTHON CRAWL: ${requestErrorMessage(error)}`, 'error');
        },
      });
  }

  protected cancelPyCrawl(): void {
    const jobId = this.crawlJobId();
    if (!jobId) {
      return;
    }
    this.api.cancelCrawl(jobId).subscribe({
      next: () => {
        this.stopCrawlPolling();
        this.crawlStatus.set('cancelled');
        this.setStatus('CRAWL CANCELLED');
      },
      error: () => this.setStatus('CANCEL FAILED', 'error'),
    });
  }

  private pollCrawl(): void {
    const jobId = this.crawlJobId();
    if (!jobId) {
      return;
    }
    this.stopCrawlPolling();
    this.crawlTimer = setInterval(() => {
      this.api.crawlStatus(jobId).subscribe({
        next: (data) => {
          this.crawlStatus.set(data.status);
          this.crawlPages.set(data.pages ?? 0);
          this.crawlFiles.set(data.files ?? 0);
          this.crawlProgress.set(data.progress_pct ?? 0);
          if (data.log) {
            this.crawlLog.set(data.log);
          }
          if (['completed', 'failed', 'cancelled'].includes(data.status)) {
            this.stopCrawlPolling();
            if (data.status === 'completed') {
              this.fetchCrawlResults();
            } else {
              this.setStatus(`CRAWL ${data.status.toUpperCase()}`, 'error');
            }
          }
        },
        error: () => this.stopCrawlPolling(),
      });
    }, 1000);
  }

  private fetchCrawlResults(): void {
    this.api.crawlResults(this.crawlJobId()).subscribe({
      next: (data) => {
        this.crawlResult.set(data);
        const pages = data.results ?? [];
        this.crawlSelected.set(pages[0] ?? null);
        this.setStatus(`CRAWLED ${data.pages ?? 0} PAGES, ${data.files ?? 0} FILES`);
      },
      error: () => this.setStatus('FAILED TO FETCH RESULTS', 'error'),
    });
  }

  private stopCrawlPolling(): void {
    if (this.crawlTimer) {
      clearInterval(this.crawlTimer);
      this.crawlTimer = null;
    }
  }

  private resetCrawl(): void {
    this.stopCrawlPolling();
    this.crawlJobId.set('');
    this.crawlStatus.set('starting');
    this.crawlPages.set(0);
    this.crawlFiles.set(0);
    this.crawlProgress.set(0);
    this.crawlLog.set([]);
    this.crawlResult.set(null);
    this.crawlSelected.set(null);
  }

  // ── job SSE stream ────────────────────────────────────────────────────

  private attachJob(job: JobInfo): void {
    this.stream?.unsubscribe();
    this.streamClosed = false;
    this.jobLog.set([]);
    this.activeJob.set(job);
    this.progress.set(progressPercent(job));
    this.stream = this.live.connect(this.api.jobStreamUrl(job.id)).subscribe({
      next: (event) => this.handleJobEvent(job.id, event),
      error: () => {
        if (!this.streamClosed) {
          this.appendLog('[ERROR] STREAM CLOSED');
          this.streamClosed = true;
        }
      },
    });
  }

  private handleJobEvent(id: string, event: XwaEvent): void {
    this.activeJob.update((job) => (job ? applyJobEvent(job, event) : job));

    const line = logLineFor(event);
    if (line) {
      this.appendLog(line);
    }

    const job = this.activeJob();
    if (job) {
      this.progress.set(progressPercent(job));
      if (isTerminalStatus(job.status)) {
        this.closeStream(id);
      }
    }
  }

  private closeStream(id: string): void {
    if (this.streamClosed) {
      return;
    }
    this.streamClosed = true;
    this.stream?.unsubscribe();
    this.stream = null;

    this.api.getJob(id).subscribe({
      next: (job) => {
        this.activeJob.set(job);
        this.progress.set(progressPercent(job));
      },
    });
    this.loadStats();
  }

  private appendLog(line: string): void {
    const stamp = new Date().toTimeString().slice(0, 8);
    this.jobLog.update((lines) => [...lines, `${stamp}  ${line}`].slice(-200));
  }

  protected cancelJob(id: string): void {
    this.api.cancelJob(id).subscribe({
      next: () => {
        this.closeStream(id);
        this.setStatus('JOB CANCELLED');
        this.loadStats();
      },
    });
  }

  // ── database export / import / clear ──────────────────────────────────

  protected exportDatabase(): void {
    this.api.exportDatabase().subscribe({
      next: (data) => {
        this.exporter.downloadJson(
          data,
          `shinobi-db-export-${new Date().toISOString().slice(0, 10)}.json`,
        );
        this.setStatus('DB EXPORTED');
      },
      error: (error: unknown) => this.setStatus(`EXPORT FAILED: ${requestErrorMessage(error)}`, 'error'),
    });
  }

  protected importDatabase(): void {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json';
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) {
        return;
      }
      file
        .text()
        .then((text) => {
          let parsed: Record<string, unknown>;
          try {
            parsed = JSON.parse(text) as Record<string, unknown>;
          } catch {
            this.setStatus('INVALID JSON FILE', 'error');
            return;
          }
          this.api.importDatabase({ jobs: parsed['jobs'] ?? [] }).subscribe({
            next: () => {
              this.setStatus('DATABASE IMPORTED');
              this.loadStats();
            },
            error: (error: unknown) =>
              this.setStatus(`IMPORT FAILED: ${requestErrorMessage(error)}`, 'error'),
          });
        })
        .catch(() => this.setStatus('COULD NOT READ FILE', 'error'));
    };
    input.click();
  }

  protected clearDatabase(): void {
    if (!confirm('Delete ALL jobs, deep results, and files?')) {
      return;
    }
    this.deleting.set(true);
    this.api.clearDatabase().subscribe({
      next: () => {
        this.deleting.set(false);
        this.activeJob.set(null);
        this.jobLog.set([]);
        this.progress.set(0);
        this.loadStats();
        this.setStatus('DATABASE CLEARED');
      },
      error: (error: unknown) => {
        this.deleting.set(false);
        this.setStatus(`CLEAR FAILED: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  // ── view helpers ──────────────────────────────────────────────────────

  protected windowOpen(url: string): void {
    window.open(url, '_blank', 'noopener');
  }

  protected formatJson(value: unknown): string {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  protected structuredTypes(value: unknown): string[] {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      return [];
    }
    return Object.keys(value as Record<string, unknown>);
  }
}
