import { TestBed } from '@angular/core/testing';
import { Subject, of } from 'rxjs';

import { ApiService } from '../../core/api.service';
import { ExportService } from '../../core/export.service';
import { JobInfo } from '../../core/models';
import { ExportActionsComponent } from './export-actions';

const job = {
  id: 'job-12345678',
  url: 'https://example.com',
  status: 'completed',
  created_at: '2026-09-12T10:00:00Z',
  pages_scraped: 10,
  files_downloaded: 4,
  total_pages: 10,
  current_url: null,
  errors: [],
  emails: [],
  phones: [],
} as unknown as JobInfo;

describe('ExportActionsComponent', () => {
  const apiStub = {
    exportJob: vi.fn(() => of({ job: { id: job.id } })),
    exportAnalysisUrl: (id: string, format: string) =>
      `http://localhost:8060/api/analyses/${id}/export?format=${format}`,
    jobZipUrl: (id: string) => `http://localhost:8060/api/jobs/${id}/download`,
  };
  const exporterStub = {
    downloadJson: vi.fn(),
  };

  beforeEach(async () => {
    apiStub.exportJob.mockReset();
    apiStub.exportJob.mockReturnValue(of({ job: { id: job.id } }));
    exporterStub.downloadJson.mockClear();

    await TestBed.configureTestingModule({
      imports: [ExportActionsComponent],
      providers: [
        { provide: ApiService, useValue: apiStub },
        { provide: ExportService, useValue: exporterStub },
      ],
    }).compileComponents();
  });

  it('should render the client button and the server links', () => {
    const fixture = TestBed.createComponent(ExportActionsComponent);
    fixture.componentInstance.job = job;
    fixture.detectChanges();

    const element = fixture.nativeElement as HTMLElement;
    expect(element.querySelector('button.export-btn')?.textContent?.trim()).toBe('JSON');

    const links = Array.from(element.querySelectorAll('a.export-btn')).map((link) => ({
      text: link.textContent?.trim(),
      href: link.getAttribute('href'),
    }));
    expect(links).toEqual([
      {
        text: 'CSV',
        href: 'http://localhost:8060/api/analyses/job-12345678/export?format=csv',
      },
      {
        text: 'ZIP',
        href: 'http://localhost:8060/api/jobs/job-12345678/download',
      },
    ]);
  });

  it('should fetch and download the client-side JSON export', () => {
    const fixture = TestBed.createComponent(ExportActionsComponent);
    fixture.componentInstance.job = job;
    fixture.detectChanges();

    const button = fixture.nativeElement.querySelector('button.export-btn') as HTMLButtonElement;
    button.click();

    expect(apiStub.exportJob).toHaveBeenCalledWith('job-12345678');
    expect(exporterStub.downloadJson).toHaveBeenCalledWith(
      { job: { id: 'job-12345678' } },
      'shinobi-export-job-1234.json',
    );
  });

  it('should expose the server URLs', () => {
    const fixture = TestBed.createComponent(ExportActionsComponent);
    fixture.componentInstance.job = job;
    expect(fixture.componentInstance.serverCsvUrl()).toContain(
      '/api/analyses/job-12345678/export?format=csv',
    );
    expect(fixture.componentInstance.zipUrl()).toContain('/api/jobs/job-12345678/download');
  });

  it('should toggle the busy state while the JSON export is pending', () => {
    const pending = new Subject<{ job: { id: string } }>();
    apiStub.exportJob.mockReturnValue(pending.asObservable());

    const fixture = TestBed.createComponent(ExportActionsComponent);
    fixture.componentInstance.job = job;
    fixture.detectChanges();

    const button = fixture.nativeElement.querySelector('button.export-btn') as HTMLButtonElement;
    button.click();
    fixture.detectChanges();

    expect(button.disabled).toBe(true);
    expect(button.textContent?.trim()).toBe('JSON...');

    pending.next({ job: { id: job.id } });
    pending.complete();
    fixture.detectChanges();

    expect(button.disabled).toBe(false);
    expect(button.textContent?.trim()).toBe('JSON');
  });
});
