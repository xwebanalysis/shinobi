import { DOCUMENT } from '@angular/common';
import { Injectable, inject } from '@angular/core';

/**
 * Client-side file exports (JSON/CSV/text). Used by the per-job export actions
 * and the deep-research list; server-side exports stay as plain anchors.
 */
@Injectable({ providedIn: 'root' })
export class ExportService {
  private readonly document = inject(DOCUMENT);

  downloadJson(data: unknown, filename: string): void {
    this.downloadText(JSON.stringify(data, null, 2), filename, 'application/json;charset=utf-8');
  }

  downloadText(text: string, filename: string, mime = 'text/plain;charset=utf-8'): void {
    this.downloadBlob(new Blob([text], { type: mime }), filename);
  }

  downloadCsv(rows: (string | number)[][], filename: string): void {
    const csv = rows
      .map((row) => row.map((cell) => this.escapeCsv(cell)).join(','))
      .join('\n');
    this.downloadText(csv, filename, 'text/csv;charset=utf-8');
  }

  downloadBlob(blob: Blob, filename: string): void {
    const url = URL.createObjectURL(blob);
    const link = this.document.createElement('a');
    link.href = url;
    link.download = filename;
    link.rel = 'noopener';
    this.document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }

  private escapeCsv(value: string | number): string {
    const text = String(value ?? '');
    if (/[",\n]/.test(text)) {
      return `"${text.replace(/"/g, '""')}"`;
    }
    return text;
  }
}
