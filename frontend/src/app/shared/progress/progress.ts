import { Component, Input } from '@angular/core';

@Component({
  selector: 'app-progress',
  standalone: true,
  template: `
    <div class="progress-block">
      @if (label) {
        <div class="progress-head">
          <span class="t-label">{{ label }}</span>
          <span class="t-label">{{ value }}%</span>
        </div>
      }
      <div
        class="progress-track"
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        [attr.aria-valuenow]="value"
        [attr.aria-label]="label || 'progress'"
      >
        <div class="progress-fill" [style.width.%]="value"></div>
      </div>
    </div>
  `,
  styles: [
    `
      .progress-block {
        width: 100%;
      }

      .progress-head {
        display: flex;
        justify-content: space-between;
        margin-bottom: var(--space-xs);
      }

      .progress-track {
        height: 3px;
        background-color: var(--surface-raised);
        overflow: hidden;
      }

      .progress-fill {
        height: 100%;
        background-color: var(--interactive);
        transition: width 0.3s cubic-bezier(0.25, 0.1, 0.25, 1);
      }
    `,
  ],
})
export class ProgressComponent {
  @Input() value = 0;
  @Input() label = '';
}
