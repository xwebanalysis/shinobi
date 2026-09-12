import { ChangeDetectorRef, Component, Input, inject } from '@angular/core';

import { ApiService } from '../../core/api.service';
import { ExportService } from '../../core/export.service';
import { JobInfo } from '../../core/models';

/**
 * Per-job export actions: client-side JSON (POST /api/jobs/{id}/export),
 * server-side CSV (GET /api/analyses/{id}/export?format=csv) and the ZIP
 * bundle with every downloaded file.
 */
@Component({
  selector: 'app-export-actions',
  standalone: true,
  template: `
    <div class="export-actions">
      <button
        type="button"
        class="export-btn"
        (click)="downloadJson()"
        [disabled]="busy"
        title="Client-side JSON export"
      >
        {{ busy ? 'JSON...' : 'JSON' }}
      </button>
      <a
        class="export-btn"
        [href]="serverCsvUrl()"
        title="Server-side CSV export"
        rel="noopener"
      >
        CSV
      </a>
      <a
        class="export-btn"
        [href]="zipUrl()"
        title="Download every file in a ZIP"
        rel="noopener"
      >
        ZIP
      </a>
    </div>
  `,
  styles: [
    `
      .export-actions {
        display: inline-flex;
        align-items: center;
        gap: var(--space-sm);
        flex-wrap: wrap;
      }

      .export-btn {
        display: inline-flex;
        align-items: center;
        border: 1px solid var(--border-visible);
        background-color: transparent;
        color: var(--text-secondary);
        padding: var(--space-sm) var(--space-md);
        font-family: var(--font-data);
        font-size: var(--label);
        letter-spacing: 0.08em;
        text-transform: uppercase;
        min-height: 36px;
        cursor: pointer;
        text-decoration: none;

        &:hover:not(:disabled) {
          color: var(--gold);
          border-color: var(--gold);
          opacity: 1;
        }

        &:disabled {
          opacity: 0.4;
          cursor: not-allowed;
        }
      }
    `,
  ],
})
export class ExportActionsComponent {
  @Input({ required: true }) job!: JobInfo;

  private readonly api = inject(ApiService);
  private readonly exporter = inject(ExportService);
  private readonly cdr = inject(ChangeDetectorRef);

  protected busy = false;

  downloadJson(): void {
    if (this.busy) {
      return;
    }
    this.busy = true;
    this.api.exportJob(this.job.id).subscribe({
      next: (data) => {
        this.exporter.downloadJson(data, `shinobi-export-${this.job.id.slice(0, 8)}.json`);
        this.busy = false;
        // Zoneless: `busy` is a plain property mutated in an async callback, so
        // the view must be explicitly marked for check.
        this.cdr.markForCheck();
      },
      error: () => {
        this.busy = false;
        this.cdr.markForCheck();
      },
    });
  }

  serverCsvUrl(): string {
    return this.api.exportAnalysisUrl(this.job.id, 'csv');
  }

  zipUrl(): string {
    return this.api.jobZipUrl(this.job.id);
  }
}
