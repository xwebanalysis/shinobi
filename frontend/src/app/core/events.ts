/**
 * Pure helpers for the xwa-sdk `Event` SSE protocol.
 *
 * The backend emits named SSE events (`event: analysis_progress`) whose `data:`
 * line is a JSON `Event` envelope:
 *
 * ```json
 * {"seq":7,"type":"analysis_progress","tool":"shinobi",
 *  "analysis_id":"<job id>","ts":"...","payload":{"percent":40,...}}
 * ```
 *
 * `applyJobEvent` rebuilds the legacy `JobInfo` snapshot the dashboard used to
 * receive, so progress bars, counters and the terminal keep working.
 */
import { JobInfo, XwaEvent, XwaEventType } from './models';

export const XWA_EVENT_TYPES: readonly XwaEventType[] = [
  'analysis_started',
  'analysis_progress',
  'item_found',
  'log',
  'analysis_completed',
  'analysis_error',
];

const TERMINAL_STATUSES: readonly string[] = ['completed', 'failed', 'cancelled'];

export function isTerminalStatus(status: string | null | undefined): boolean {
  return typeof status === 'string' && TERMINAL_STATUSES.includes(status.toLowerCase());
}

/** Parses one SSE `data:` frame; returns null for anything that is not an Event. */
export function parseXwaEvent(data: string): XwaEvent | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return null;
  }

  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return null;
  }

  const record = parsed as Record<string, unknown>;
  const type = record['type'];
  if (typeof type !== 'string' || type.length === 0) {
    return null;
  }

  const payload = record['payload'];
  return {
    seq: typeof record['seq'] === 'number' ? record['seq'] : 0,
    type,
    tool: typeof record['tool'] === 'string' ? record['tool'] : 'shinobi',
    analysis_id: typeof record['analysis_id'] === 'string' ? record['analysis_id'] : '',
    ts: typeof record['ts'] === 'string' ? record['ts'] : '',
    payload:
      payload && typeof payload === 'object' && !Array.isArray(payload)
        ? (payload as Record<string, unknown>)
        : null,
  };
}

function asString(value: unknown, fallback: string): string {
  return typeof value === 'string' ? value : fallback;
}

function asNumber(value: unknown, fallback: number): number {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'string') {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
  }
  return fallback;
}

function asStringArray(value: unknown, fallback: string[]): string[] {
  if (!Array.isArray(value)) {
    return fallback;
  }
  return value.filter((item): item is string => typeof item === 'string');
}

/** Normalises xwa-sdk analysis states (PENDING/RUNNING/…) to job statuses. */
export function normalizeJobStatus(value: unknown, fallback = 'running'): string {
  if (typeof value !== 'string' || value.length === 0) {
    return fallback;
  }
  switch (value.toUpperCase()) {
    case 'PENDING':
      return 'queued';
    case 'RUNNING':
      return 'running';
    case 'COMPLETED':
      return 'completed';
    case 'ERROR':
      return 'failed';
    case 'CANCELLED':
      return 'cancelled';
    default:
      return value.toLowerCase();
  }
}

/** Applies one Event to a job snapshot, returning a new immutable object. */
export function applyJobEvent(job: JobInfo, event: XwaEvent): JobInfo {
  const payload = event.payload ?? {};

  switch (event.type) {
    case 'analysis_started':
      return {
        ...job,
        url: asString(payload['url'], job.url),
        status: normalizeJobStatus(payload['status'], 'running'),
        total_pages: asNumber(payload['total_pages'], job.total_pages),
      };

    case 'analysis_progress':
      return {
        ...job,
        status: 'running',
        pages_scraped: asNumber(payload['pages_scraped'], job.pages_scraped),
        files_downloaded: asNumber(payload['files_downloaded'], job.files_downloaded),
        total_pages: asNumber(payload['total_pages'], job.total_pages),
        current_url:
          payload['current_url'] === null || payload['current_url'] === undefined
            ? job.current_url
            : String(payload['current_url']),
      };

    case 'item_found': {
      const kind = asString(payload['kind'], '');
      const value = asString(payload['value'], '');
      if (!value) {
        return job;
      }
      if (kind === 'email' && !job.emails.includes(value)) {
        return { ...job, emails: [...job.emails, value] };
      }
      if (kind === 'phone' && !job.phones.includes(value)) {
        return { ...job, phones: [...job.phones, value] };
      }
      return job;
    }

    case 'log': {
      const level = asString(payload['level'], 'info');
      const message = asString(payload['message'], '');
      if (level === 'error' && message && !job.errors.includes(message)) {
        return { ...job, errors: [...job.errors, message] };
      }
      return job;
    }

    case 'analysis_completed':
      return {
        ...job,
        status: normalizeJobStatus(payload['status'], 'completed'),
        pages_scraped: asNumber(payload['pages_scraped'], job.pages_scraped),
        files_downloaded: asNumber(payload['files_downloaded'], job.files_downloaded),
        emails: asStringArray(payload['emails'], job.emails),
        phones: asStringArray(payload['phones'], job.phones),
      };

    case 'analysis_error': {
      const message = asString(payload['message'], 'analysis failed');
      return {
        ...job,
        status: 'failed',
        errors: job.errors.includes(message) ? job.errors : [...job.errors, message],
      };
    }

    default:
      return job;
  }
}

/** Percentage used by the progress bar (same math as the legacy dashboard). */
export function progressPercent(job: JobInfo): number {
  if (job.total_pages <= 0) {
    return 0;
  }
  return Math.min(100, Math.round((job.pages_scraped / job.total_pages) * 100));
}

/** Terminal/log line for one Event, or null when nothing should be printed. */
export function logLineFor(event: XwaEvent): string | null {
  const payload = event.payload ?? {};

  switch (event.type) {
    case 'analysis_started':
      return `STARTED #${event.analysis_id} ${asString(payload['url'], '')}`.trimEnd();

    case 'analysis_progress':
      return (
        `[PROGRESS] ${asNumber(payload['pages_scraped'], 0)}/${asNumber(payload['total_pages'], 0)}` +
        ` ${asNumber(payload['percent'], 0)}% ${asNumber(payload['files_downloaded'], 0)} FILES`
      );

    case 'item_found': {
      const kind = asString(payload['kind'], 'item').toUpperCase();
      return `+ ${kind} ${asString(payload['value'], '')}`.trimEnd();
    }

    case 'log':
      return `[${asString(payload['level'], 'info').toUpperCase()}] ${asString(payload['message'], '')}`.trimEnd();

    case 'analysis_completed':
      return (
        `COMPLETED #${event.analysis_id}` +
        ` PAGES=${asNumber(payload['pages_scraped'], 0)}` +
        ` FILES=${asNumber(payload['files_downloaded'], 0)}`
      );

    case 'analysis_error':
      return `[ERROR] ${asString(payload['message'], 'analysis failed')}`;

    default:
      return `${event.type} #${event.seq}`;
  }
}
