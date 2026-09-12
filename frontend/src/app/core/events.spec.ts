import {
  applyJobEvent,
  isTerminalStatus,
  logLineFor,
  normalizeJobStatus,
  parseXwaEvent,
  progressPercent,
} from './events';
import { JobInfo } from './models';

function envelope(type: string, payload: unknown, seq = 1): string {
  return JSON.stringify({
    seq,
    type,
    tool: 'shinobi',
    analysis_id: 'job-1',
    ts: '2026-09-12T10:00:00Z',
    payload,
  });
}

function baseJob(overrides: Partial<JobInfo> = {}): JobInfo {
  return {
    id: 'job-1',
    url: 'https://example.com',
    status: 'running',
    created_at: '2026-09-12T10:00:00Z',
    pages_scraped: 0,
    files_downloaded: 0,
    total_pages: 10,
    current_url: null,
    errors: [],
    emails: [],
    phones: [],
    ...overrides,
  };
}

describe('parseXwaEvent', () => {
  it('parses the analysis_started envelope', () => {
    const event = parseXwaEvent(
      envelope('analysis_started', { url: 'https://example.com', total_pages: 10, status: 'running' }),
    );
    expect(event?.type).toBe('analysis_started');
    expect(event?.tool).toBe('shinobi');
    expect(event?.analysis_id).toBe('job-1');
    expect(event?.payload?.['total_pages']).toBe(10);
  });

  it('parses the analysis_progress envelope', () => {
    const event = parseXwaEvent(
      envelope('analysis_progress', {
        percent: 40,
        pages_scraped: 4,
        files_downloaded: 7,
        total_pages: 10,
        current_url: 'https://example.com/a',
      }),
    );
    expect(event?.payload?.['percent']).toBe(40);
    expect(event?.payload?.['pages_scraped']).toBe(4);
  });

  it('parses the item_found envelope', () => {
    const event = parseXwaEvent(
      envelope('item_found', { kind: 'email', value: 'a@b.c', url: 'https://example.com' }),
    );
    expect(event?.payload?.['kind']).toBe('email');
    expect(event?.payload?.['value']).toBe('a@b.c');
  });

  it('parses the log envelope', () => {
    const event = parseXwaEvent(envelope('log', { level: 'info', message: '[PAGE] 1/10' }));
    expect(event?.payload?.['message']).toBe('[PAGE] 1/10');
  });

  it('parses the analysis_completed envelope', () => {
    const event = parseXwaEvent(
      envelope('analysis_completed', {
        status: 'COMPLETED',
        pages_scraped: 10,
        files_downloaded: 42,
        emails: ['a@b.c'],
        phones: ['+34 600'],
      }),
    );
    expect(event?.payload?.['status']).toBe('COMPLETED');
    expect(event?.payload?.['files_downloaded']).toBe(42);
  });

  it('parses the analysis_error envelope', () => {
    const event = parseXwaEvent(
      envelope('analysis_error', {
        code: 'SCRAPE_FAILED',
        message: 'boom',
        detail: { url: 'https://example.com' },
        retryable: false,
      }),
    );
    expect(event?.payload?.['code']).toBe('SCRAPE_FAILED');
    expect(event?.payload?.['message']).toBe('boom');
  });

  it('rejects invalid or non-envelope frames', () => {
    expect(parseXwaEvent('not-json')).toBeNull();
    expect(parseXwaEvent('{"seq":1}')).toBeNull();
    expect(parseXwaEvent('[1,2,3]')).toBeNull();
    expect(parseXwaEvent('42')).toBeNull();
  });
});

describe('applyJobEvent', () => {
  it('applies analysis_started fields', () => {
    const event = parseXwaEvent(envelope('analysis_started', { total_pages: 20, status: 'running' }))!;
    const job = applyJobEvent(baseJob({ total_pages: 0 }), event);
    expect(job.status).toBe('running');
    expect(job.total_pages).toBe(20);
  });

  it('applies analysis_progress counters and current URL', () => {
    const event = parseXwaEvent(
      envelope('analysis_progress', {
        percent: 33,
        pages_scraped: 3,
        files_downloaded: 9,
        total_pages: 9,
        current_url: 'https://example.com/x',
      }),
    )!;
    const job = applyJobEvent(baseJob(), event);
    expect(job.pages_scraped).toBe(3);
    expect(job.files_downloaded).toBe(9);
    expect(job.current_url).toBe('https://example.com/x');
    expect(progressPercent(job)).toBe(33);
  });

  it('collects unique item_found emails and phones', () => {
    const email = parseXwaEvent(envelope('item_found', { kind: 'email', value: 'a@b.c' }))!;
    const phone = parseXwaEvent(envelope('item_found', { kind: 'phone', value: '+34 600' }))!;
    let job = applyJobEvent(baseJob(), email);
    job = applyJobEvent(job, email);
    job = applyJobEvent(job, phone);
    expect(job.emails).toEqual(['a@b.c']);
    expect(job.phones).toEqual(['+34 600']);
  });

  it('records log errors but not info lines', () => {
    const info = parseXwaEvent(envelope('log', { level: 'info', message: '[PAGE] 1/10' }))!;
    const error = parseXwaEvent(envelope('log', { level: 'error', message: 'timeout' }))!;
    let job = applyJobEvent(baseJob(), info);
    expect(job.errors).toEqual([]);
    job = applyJobEvent(job, error);
    expect(job.errors).toEqual(['timeout']);
  });

  it('normalises the xwa-sdk completed status and totals', () => {
    const event = parseXwaEvent(
      envelope('analysis_completed', {
        status: 'COMPLETED',
        pages_scraped: 5,
        files_downloaded: 8,
        emails: ['x@y.z'],
        phones: [],
      }),
    )!;
    const job = applyJobEvent(baseJob(), event);
    expect(job.status).toBe('completed');
    expect(job.pages_scraped).toBe(5);
    expect(job.files_downloaded).toBe(8);
    expect(job.emails).toEqual(['x@y.z']);
    expect(isTerminalStatus(job.status)).toBe(true);
  });

  it('maps CANCELLED completions to cancelled', () => {
    const event = parseXwaEvent(envelope('analysis_completed', { status: 'CANCELLED' }))!;
    const job = applyJobEvent(baseJob(), event);
    expect(job.status).toBe('cancelled');
  });

  it('turns analysis_error into a failed job with the error message', () => {
    const event = parseXwaEvent(
      envelope('analysis_error', { code: 'SCRAPE_FAILED', message: 'boom' }),
    )!;
    const job = applyJobEvent(baseJob(), event);
    expect(job.status).toBe('failed');
    expect(job.errors).toEqual(['boom']);
  });
});

describe('progressPercent', () => {
  it('handles zero totals and caps at 100', () => {
    expect(progressPercent(baseJob({ total_pages: 0 }))).toBe(0);
    expect(progressPercent(baseJob({ pages_scraped: 4, total_pages: 10 }))).toBe(40);
    expect(progressPercent(baseJob({ pages_scraped: 40, total_pages: 10 }))).toBe(100);
  });
});

describe('normalizeJobStatus', () => {
  it('maps xwa-sdk states onto job statuses', () => {
    expect(normalizeJobStatus('PENDING')).toBe('queued');
    expect(normalizeJobStatus('RUNNING')).toBe('running');
    expect(normalizeJobStatus('COMPLETED')).toBe('completed');
    expect(normalizeJobStatus('ERROR')).toBe('failed');
    expect(normalizeJobStatus('CANCELLED')).toBe('cancelled');
    expect(normalizeJobStatus(undefined, 'queued')).toBe('queued');
  });
});

describe('logLineFor', () => {
  it('renders one line per event type', () => {
    expect(
      logLineFor(parseXwaEvent(envelope('analysis_started', { url: 'https://a.b' }))!),
    ).toContain('STARTED #job-1 https://a.b');
    expect(
      logLineFor(
        parseXwaEvent(
          envelope('analysis_progress', { pages_scraped: 1, total_pages: 4, percent: 25, files_downloaded: 2 }),
        )!,
      ),
    ).toBe('[PROGRESS] 1/4 25% 2 FILES');
    expect(
      logLineFor(parseXwaEvent(envelope('item_found', { kind: 'email', value: 'a@b.c' }))!),
    ).toBe('+ EMAIL a@b.c');
    expect(
      logLineFor(parseXwaEvent(envelope('log', { level: 'error', message: 'boom' }))!),
    ).toBe('[ERROR] boom');
    expect(
      logLineFor(
        parseXwaEvent(envelope('analysis_completed', { pages_scraped: 9, files_downloaded: 3 }))!,
      ),
    ).toBe('COMPLETED #job-1 PAGES=9 FILES=3');
    expect(
      logLineFor(parseXwaEvent(envelope('analysis_error', { message: 'boom' }))!),
    ).toBe('[ERROR] boom');
  });
});
