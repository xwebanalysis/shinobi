import { ChangeDetectorRef, Component, OnInit, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { ApiService, requestErrorMessage } from '../../core/api.service';
import { Schedule } from '../../core/models';

@Component({
  selector: 'app-schedules',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './schedules.html',
  styleUrl: './schedules.scss',
})
export class SchedulesComponent implements OnInit {
  private readonly api = inject(ApiService);
  private readonly cdr = inject(ChangeDetectorRef);

  protected readonly schedules = signal<Schedule[]>([]);
  protected readonly loading = signal(false);
  protected readonly deleting = signal(false);
  protected readonly statusMsg = signal('');
  protected readonly statusType = signal<'ok' | 'error' | 'warn' | ''>('');

  protected newScheduleUrl = '';
  protected newScheduleInterval = 60;

  private statusTimeout: ReturnType<typeof setTimeout> | null = null;

  ngOnInit(): void {
    this.load();
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

  protected load(): void {
    this.loading.set(true);
    this.api.listSchedules().subscribe({
      next: (schedules) => {
        this.schedules.set(schedules);
        this.loading.set(false);
      },
      error: (error: unknown) => {
        this.loading.set(false);
        this.setStatus(`FAILED TO LOAD SCHEDULES: ${requestErrorMessage(error)}`, 'error');
      },
    });
  }

  protected addSchedule(): void {
    const url = this.newScheduleUrl.trim();
    if (!url || this.newScheduleInterval < 5) {
      this.setStatus('URL AND INTERVAL >= 5 MIN REQUIRED', 'warn');
      return;
    }
    this.deleting.set(true);
    this.api.createSchedule(url, this.newScheduleInterval).subscribe({
      next: () => {
        this.newScheduleUrl = '';
        this.newScheduleInterval = 60;
        this.deleting.set(false);
        this.setStatus('SCHEDULE CREATED');
        this.load();
        // Zoneless: the form model is plain state; mark the view for check.
        this.cdr.markForCheck();
      },
      error: (error: unknown) => {
        this.deleting.set(false);
        this.setStatus(`FAILED: ${requestErrorMessage(error)}`, 'error');
        this.cdr.markForCheck();
      },
    });
  }

  protected deleteSchedule(schedule: Schedule): void {
    if (!confirm(`Delete schedule for ${schedule.url}?`)) {
      return;
    }
    this.api.deleteSchedule(schedule.id).subscribe({
      next: () => {
        this.schedules.update((items) => items.filter((item) => item.id !== schedule.id));
        this.setStatus('SCHEDULE DELETED');
      },
      error: (error: unknown) => this.setStatus(`DELETE FAILED: ${requestErrorMessage(error)}`, 'error'),
    });
  }
}
