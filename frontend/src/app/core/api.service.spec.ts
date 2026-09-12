import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';

import { ApiService, requestErrorMessage } from './api.service';
import { ScrapeConfig } from './models';

const BASE = 'http://localhost:8060';

describe('ApiService', () => {
  let service: ApiService;
  let httpMock: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting()],
    });
    service = TestBed.inject(ApiService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
  });

  it('should GET the shinobi health endpoint', () => {
    service.health().subscribe((health) => {
      expect(health.service).toBe('shinobi');
      expect(health.database).toBe('ok');
    });

    const request = httpMock.expectOne(`${BASE}/api/health`);
    expect(request.request.method).toBe('GET');
    request.flush({ status: 'ok', service: 'shinobi', version: '0.2.0', database: 'ok' });
  });

  it('should POST the sanitised scrape config', () => {
    const config: ScrapeConfig = {
      url: 'https://example.com',
      file_types: 'html, css , js',
      auth_username: 'user',
      auth_password: 'secret',
      auth_mode: '',
    };

    service.startScrape(config).subscribe();

    const request = httpMock.expectOne(`${BASE}/api/scrape`);
    expect(request.request.method).toBe('POST');
    const body = request.request.body as Record<string, unknown>;
    expect(body['file_types']).toEqual(['html', 'css', 'js']);
    expect(body['auth_username']).toBeUndefined();
    expect(body['auth_password']).toBeUndefined();
    expect(body['auth_mode']).toBeUndefined();
    request.flush({ id: 'job-1', url: 'https://example.com', status: 'queued' });
  });

  it('should keep auth fields when a mode is selected', () => {
    service
      .startScrape({ url: 'https://example.com', auth_username: 'user', auth_password: 'pw', auth_mode: 'basic' })
      .subscribe();

    const request = httpMock.expectOne(`${BASE}/api/scrape`);
    const body = request.request.body as Record<string, unknown>;
    expect(body['auth_username']).toBe('user');
    expect(body['auth_password']).toBe('pw');
    request.flush({ id: 'job-1', url: 'https://example.com', status: 'queued' });
  });

  it('should list jobs with offset and limit', () => {
    service.listJobs(25, 25).subscribe((page) => expect(page.total).toBe(0));

    const request = httpMock.expectOne(`${BASE}/api/jobs?offset=25&limit=25`);
    expect(request.request.method).toBe('GET');
    request.flush({ items: [], total: 0, offset: 25, limit: 25 });
  });

  it('should build SSE, export and ZIP URLs', () => {
    expect(service.jobStreamUrl('job-1')).toBe(`${BASE}/api/jobs/job-1/stream`);
    expect(service.exportAnalysisUrl('job-1', 'csv')).toBe(
      `${BASE}/api/analyses/job-1/export?format=csv`,
    );
    expect(service.exportAnalysisUrl('job-1')).toContain('format=json');
    expect(service.jobZipUrl('job-1')).toBe(`${BASE}/api/jobs/job-1/download`);
    expect(service.fileUrl('example.com/a b.html')).toBe(
      `${BASE}/api/files/example.com/a%20b.html`,
    );
  });

  it('should DELETE an analysis through the semantic alias', () => {
    service.deleteJob('job-1').subscribe();

    const request = httpMock.expectOne(`${BASE}/api/analyses/job-1`);
    expect(request.request.method).toBe('DELETE');
    request.flush({ status: 'deleted' });
  });

  it('should create and delete schedules', () => {
    service.createSchedule('https://example.com', 30).subscribe();
    const create = httpMock.expectOne(`${BASE}/api/schedules`);
    expect(create.request.method).toBe('POST');
    expect(create.request.body).toEqual({ url: 'https://example.com', interval_min: 30 });
    create.flush({ id: 's1' });

    service.deleteSchedule('s1').subscribe();
    const remove = httpMock.expectOne(`${BASE}/api/schedules/s1`);
    expect(remove.request.method).toBe('DELETE');
    remove.flush({ status: 'deleted' });
  });

  it('should extract the deep-results CSV as a blob', () => {
    service.exportDeepCsv().subscribe((blob) => expect(blob).toBeInstanceOf(Blob));
    const request = httpMock.expectOne(`${BASE}/api/deep/results.csv`);
    expect(request.request.responseType).toBe('blob');
    request.flush(new Blob(['id,url'], { type: 'text/csv' }));
  });

  it('should classify the extractor status without throwing', () => {
    let online = '';
    service.extractorStatus().subscribe((status) => (online = status));
    httpMock.expectOne(`${BASE}/api/python/docs`).flush('<html></html>');
    expect(online).toBe('online');

    let offline = '';
    service.extractorStatus().subscribe((status) => (offline = status));
    httpMock
      .expectOne(`${BASE}/api/python/docs`)
      .flush('bad gateway', { status: 502, statusText: 'Bad Gateway' });
    expect(offline).toBe('offline');

    let unknown = '';
    service.extractorStatus().subscribe((status) => (unknown = status));
    httpMock
      .expectOne(`${BASE}/api/python/docs`)
      .flush('missing', { status: 404, statusText: 'Not Found' });
    expect(unknown).toBe('unknown');
  });

  it('should describe API errors for inline status messages', () => {
    let message = '';
    service.health().subscribe({
      error: (error: unknown) => (message = requestErrorMessage(error)),
    });
    httpMock
      .expectOne(`${BASE}/api/health`)
      .flush({ error: { message: 'database offline' } }, { status: 500, statusText: 'Error' });
    expect(message).toBe('database offline');
  });
});
