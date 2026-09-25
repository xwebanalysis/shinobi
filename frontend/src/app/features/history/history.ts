import { Component, OnDestroy, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute } from '@angular/router';

import { ApiService, requestErrorMessage } from '../../core/api.service';
import { ExportService } from '../../core/export.service';
import { DeepResult, JobInfo } from '../../core/models';
import {
  XwaChartColorKey,
  XwaChartComponent,
  XwaChartDatum,
} from '../../shared/charts/xwa-chart.component';
import { ExportActionsComponent } from '../../shared/export-actions/export-actions';
import { StatusBadgeComponent } from '../../shared/status-badge/status-badge';

/** Maps a job status (uppercased) to the chart severity color key. */
function jobStatusColor(status: string): XwaChartColorKey {
  switch (status) {
    case 'COMPLETED':
      return 'success';
    case 'RUNNING':
    case 'QUEUED':
      return 'warning';
    case 'FAILED':
    case 'CANCELLED':
      return 'critical';
    default:
      return 'neutral-strong';
  }
}

@Component({
  selector: 'app-history',
  standalone: true,
  imports: [ExportActionsComponent, FormsModule, StatusBadgeComponent, XwaChartComponent],
  templateUrl: './history.html',
  styleUrl: './history.scss',
})
export class HistoryComponent implements OnInit, OnDestroy {
  private readonly api = inject(ApiService);
  private readonly exporter = inject(ExportService);
  private readonly route = inject(ActivatedRoute);

  protected activeTab: 'jobs' | 'deep' = 'jobs';

  protected readonly searchQuery = signal('');
  protected readonly jobs = signal<JobInfo[]>([]);
  protected readonly totalJobs = signal(0);
  protected readonly pageJobs = signal(0);
  protected readonly loadingJobs = signal(false);

  protected readonly deepResults = signal<DeepResult[]>([]);
  protected readonly totalDeep = signal(0);
  protected readonly pageDeep = signal(0);
  protected readonly loadingDeep = signal(false);
  protected readonly selectedDeep = signal<DeepResult | null>(null);

  protected readonly deleting = signal(false);
  protected readonly statusMsg = signal('');
  protected readonly statusType = signal<'ok' | 'error' | 'warn' | ''>('');

  protected readonly pageSize = 25;

  protected readonly filteredJobs = computed(() => {
    const query = this.searchQuery().toLowerCase();
    if (!query) {
      return this.jobs();
    }
    return this.jobs().filter(
      (job) =>
        job.url.toLowerCase().includes(query) ||
        job.id.includes(query) ||
        job.emails.some((email) => email.toLowerCase().includes(query)),
    );
  });

  // ── charts ────────────────────────────────────────────────────────────

  /** All jobs (up to 1000) for the overview charts; the list stays paginated. */
  protected readonly chartJobs = signal<JobInfo[]>([]);

  protected readonly statusDonutData = computed<XwaChartDatum[]>(() => {
    const counts = new Map<string, number>();
    for (const job of this.chartJobs()) {
      const key = job.status.toUpperCase();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return [...counts.entries()].map(([label, value]) => ({
      label,
      value,
      color: jobStatusColor(label),
    }));
  });

  protected readonly jobsPerDayData = computed<XwaChartDatum[]>(() => {
    const byDay = new Map<string, number>();
    for (const job of this.chartJobs()) {
      const day = job.created_at.slice(0, 10) || 'UNKNOWN';
      byDay.set(day, (byDay.get(day) ?? 0) + 1);
    }
    return [...byDay.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([label, value]) => ({ label, value }));
  });

  protected readonly filteredDeep = computed(() => {
    const query = this.searchQuery().toLowerCase();
    if (!query) {
      return this.deepResults();
    }
    return this.deepResults().filter(
      (result) =>
        result.url.toLowerCase().includes(query) ||
        (result.extracted?.emails ?? []).some((email) => email.toLowerCase().includes(query)),
    );
  });

  private jobsTimer: ReturnType<typeof setInterval> | null = null;
  private statusTimeout: ReturnType<typeof setTimeout> | null = null;

  ngOnInit(): void {
    if (this.route.snapshot.queryParamMap.get('tab') === 'deep') {
      this.activeTab = 'deep';
    }
    this.loadJobs();
    this.loadChartJobs();
    this.loadDeepResults();
    this.jobsTimer = setInterval(() => {
      this.loadJobs();
      this.loadChartJobs();
    }, 3000);
  }

  ngOnDestroy(): void {
    if (this.jobsTimer) {
      clearInterval(this.jobsTimer);
    }
    if (this.statusTimeout) {
      clearTimeout(this.statusTimeout);
    }
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

  // ── jobs ──────────────────────────────────────────────────────────────

  protected loadJobs(): void {
    this.loadingJobs.set(true);
    this.api.listJobs(this.pageJobs(), this.pageSize).subscribe({
      next: (page) => {
        this.jobs.set(page.items);
        this.totalJobs.set(page.total);
        this.loadingJobs.set(false);
      },
      error: (error: unknown) => {
        this.loadingJobs.set(false);
        this.setStatus(`FAILED TO LOAD JOBS: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  /** Fetches the full job list (paged, up to 1000) for the overview charts. */
  protected loadChartJobs(): void {
    this.api.listJobs(0, 1000).subscribe({
      next: (page) => this.chartJobs.set(page.items),
    });
  }

  protected cancelJob(id: string): void {
    this.api.cancelJob(id).subscribe({
      next: () => {
        this.setStatus('JOB CANCELLED');
        this.loadJobs();
      },
      error: (error: unknown) => this.setStatus(`CANCEL FAILED: ${requestErrorMessage(error)}`, 'error'),
    });
  }

  protected deleteJob(job: JobInfo, event: Event): void {
    event.stopPropagation();
    if (!confirm(`Delete job ${job.id.slice(0, 8)}?`)) {
      return;
    }
    this.api.deleteJob(job.id).subscribe({
      next: () => {
        this.jobs.update((items) => items.filter((item) => item.id !== job.id));
        this.chartJobs.update((items) => items.filter((item) => item.id !== job.id));
        this.totalJobs.update((total) => Math.max(0, total - 1));
        this.setStatus('JOB DELETED');
      },
      error: (error: unknown) => this.setStatus(`DELETE FAILED: ${requestErrorMessage(error)}`, 'error'),
    });
  }

  // ── deep results ──────────────────────────────────────────────────────

  protected loadDeepResults(): void {
    this.loadingDeep.set(true);
    this.api.listDeepResults(this.pageDeep(), this.pageSize).subscribe({
      next: (page) => {
        this.deepResults.set(page.items);
        this.totalDeep.set(page.total);
        this.loadingDeep.set(false);
      },
      error: () => this.loadingDeep.set(false),
    });
  }

  protected selectDeep(result: DeepResult): void {
    this.selectedDeep.set(result);
  }

  protected exportDeepResult(result: DeepResult, event: Event): void {
    event.stopPropagation();
    this.exporter.downloadJson(result, `deep-${result.id.slice(0, 8)}.json`);
  }

  protected exportDeepCsv(): void {
    this.api.exportDeepCsv().subscribe({
      next: (blob) => this.exporter.downloadBlob(blob, 'deep-results.csv'),
      error: () => this.setStatus('CSV EXPORT FAILED', 'error'),
    });
  }

  protected deleteDeepResult(result: DeepResult, event: Event): void {
    event.stopPropagation();
    if (!confirm('Delete this result?')) {
      return;
    }
    this.api.deleteDeepResult(result.id).subscribe({
      next: () => {
        this.deepResults.update((items) => items.filter((item) => item.id !== result.id));
        this.totalDeep.update((total) => Math.max(0, total - 1));
        if (this.selectedDeep()?.id === result.id) {
          this.selectedDeep.set(null);
        }
        this.setStatus('RESULT DELETED');
      },
      error: (error: unknown) => this.setStatus(`DELETE FAILED: ${requestErrorMessage(error)}`, 'error'),
    });
  }

  protected clearDeepResults(): void {
    if (!confirm('Delete all deep research results?')) {
      return;
    }
    this.deleting.set(true);
    this.api.clearDeepResults().subscribe({
      next: () => {
        this.deepResults.set([]);
        this.totalDeep.set(0);
        this.selectedDeep.set(null);
        this.deleting.set(false);
        this.setStatus('DEEP RESULTS CLEARED');
      },
      error: (error: unknown) => {
        this.deleting.set(false);
        this.setStatus(`CLEAR FAILED: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  // ── pagination ────────────────────────────────────────────────────────

  protected prevPage(tab: 'jobs' | 'deep'): void {
    if (tab === 'jobs') {
      this.pageJobs.update((page) => Math.max(0, page - this.pageSize));
      this.loadJobs();
    } else {
      this.pageDeep.update((page) => Math.max(0, page - this.pageSize));
      this.loadDeepResults();
    }
  }

  protected nextPage(tab: 'jobs' | 'deep'): void {
    if (tab === 'jobs') {
      if (this.pageJobs() + this.pageSize < this.totalJobs()) {
        this.pageJobs.update((page) => page + this.pageSize);
        this.loadJobs();
      }
    } else if (this.pageDeep() + this.pageSize < this.totalDeep()) {
      this.pageDeep.update((page) => page + this.pageSize);
      this.loadDeepResults();
    }
  }

  protected pageInfo(tab: 'jobs' | 'deep'): string {
    const total = tab === 'jobs' ? this.totalJobs() : this.totalDeep();
    const offset = tab === 'jobs' ? this.pageJobs() : this.pageDeep();
    if (!total) {
      return '';
    }
    return `${offset + 1}-${Math.min(offset + this.pageSize, total)} / ${total}`;
  }

  // ── view helpers ──────────────────────────────────────────────────────

  protected structuredTypes(result: DeepResult): string[] {
    const data = result.structured_data;
    if (!data || typeof data !== 'object' || Array.isArray(data)) {
      return [];
    }
    return Object.keys(data as Record<string, unknown>);
  }

  protected formatJson(value: unknown): string {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }
}
