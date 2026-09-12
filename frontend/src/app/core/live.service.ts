import { Injectable } from '@angular/core';
import { Observable } from 'rxjs';

import { XWA_EVENT_TYPES, parseXwaEvent } from './events';
import { XwaEvent } from './models';

/**
 * Server-Sent Events wrapper exposing the xwa-sdk `Event` envelopes as an
 * Observable.
 *
 * The backend emits **named** events (`event: analysis_progress`), so a plain
 * `onmessage` handler is not enough: every known type is registered via
 * `addEventListener`, plus `onmessage` for unnamed frames. EventSource
 * auto-reconnects on transient errors; only a fully CLOSED socket is reported
 * as an error so the UI does not flap while the browser retries.
 */
@Injectable({ providedIn: 'root' })
export class LiveService {
  connect(url: string): Observable<XwaEvent> {
    return new Observable<XwaEvent>((subscriber) => {
      const source = new EventSource(url);

      const handler = (message: MessageEvent): void => {
        const event = parseXwaEvent(String(message.data ?? ''));
        if (event) {
          subscriber.next(event);
        }
      };

      source.onmessage = handler;
      for (const type of XWA_EVENT_TYPES) {
        source.addEventListener(type, handler);
      }

      source.onerror = () => {
        // EventSource reconnects on transient failures (readyState CONNECTING);
        // only report when the browser has given up (CLOSED).
        if (source.readyState === EventSource.CLOSED) {
          subscriber.error(new Error('SSE connection closed'));
        }
      };

      return () => {
        source.onmessage = null;
        for (const type of XWA_EVENT_TYPES) {
          source.removeEventListener(type, handler);
        }
        source.close();
      };
    });
  }
}
