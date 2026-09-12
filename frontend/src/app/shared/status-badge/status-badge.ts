import { Component, Input } from '@angular/core';

@Component({
  selector: 'app-status-badge',
  standalone: true,
  template: `<span class="t-label badge" [class]="statusClass()">[ {{ status }} ]</span>`,
  styles: [
    `
      .badge {
        letter-spacing: 0.12em;
        white-space: nowrap;
      }
    `,
  ],
})
export class StatusBadgeComponent {
  @Input({ required: true }) status = '';

  statusClass(): string {
    switch (this.status.toLowerCase()) {
      case 'completed':
      case 'complete':
        return 'text-success';
      case 'running':
      case 'pending':
      case 'queued':
        return 'text-warning';
      case 'failed':
      case 'error':
      case 'cancelled':
      case 'canceled':
        return 'text-accent';
      default:
        return 'text-muted';
    }
  }
}
