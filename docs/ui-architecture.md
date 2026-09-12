<h1>Shinobi: UI Architecture &amp; Structure</h1>

<p>The frontend is an <strong>Angular 22 standalone SPA</strong> served by the Rust
backend from <code>static/browser/</code>. It talks to the REST API and consumes the
SSE stream, which carries <strong>xwa-sdk <code>Event</code> envelopes</strong>. No Google
Fonts, no <code>zone.js</code>, no runtime dependencies beyond Angular + RxJS.</p>

<hr>

<h2>1. Stack</h2>

<table>
  <tr><th>Layer</th><th>Choice</th></tr>
  <tr><td>Framework</td><td>Angular 22.1 (standalone components, signals, zoneless change detection)</td></tr>
  <tr><td>Language</td><td>TypeScript 6.0 (strict mode, strict templates)</td></tr>
  <tr><td>Builder</td><td><code>@angular/build:application</code> (esbuild/Vite)</td></tr>
  <tr><td>HTTP</td><td><code>@angular/common/http</code> (<code>HttpClient</code>)</td></tr>
  <tr><td>Live updates</td><td>Native <code>EventSource</code> wrapped by <code>LiveService</code></td></tr>
  <tr><td>State</td><td><code>signal()</code> / <code>computed()</code>; plain objects for form models</td></tr>
  <tr><td>Tests</td><td>Vitest 4 + jsdom via <code>@angular/build:unit-test</code></td></tr>
  <tr><td>Styling</td><td>SCSS + Nothing Design tokens; self-hosted Doto / Space Grotesk / Space Mono woff2</td></tr>
</table>

<hr>

<h2>2. File Layout</h2>

<pre><code>frontend/
├── angular.json                   # builder application, output ../static (browser/), hashing none
├── proxy.conf.json                # dev-server /api → http://localhost:8060
├── scripts/test.sh                # npm test wrapper (ng test --watch=false)
├── public/fonts/*.woff2           # Doto, Space Grotesk, Space Mono (self-hosted)
└── src/
    ├── _fonts.scss                # @font-face declarations (compiled into styles.css)
    ├── styles.scss                # Nothing tokens + shared primitives
    ├── index.html                 # SPA shell (no external font/CDN requests)
    ├── main.ts                    # bootstrapApplication
    ├── environments/
    │   ├── environment.ts         # dev: http://<hostname>:8060
    │   └── environment.production.ts  # relative URLs (served same-origin by Rust)
    └── app/
        ├── app.ts|html|scss       # shell: nav, health dots, theme toggle, footer
        ├── app.config.ts          # router + HttpClient providers (zoneless default)
        ├── app.routes.ts          # "", history, schedules, files
        ├── core/
        │   ├── models.ts          # domain + xwa-sdk Event types
        │   ├── api.service.ts     # REST client + URL builders + error helper
        │   ├── live.service.ts    # EventSource → Observable&lt;XwaEvent&gt;
        │   ├── events.ts          # pure Event parsing/reducer/log/progress helpers
        │   ├── theme.service.ts   # dark/light, localStorage, body classes
        │   └── export.service.ts  # client-side JSON/CSV/text downloads
        ├── shared/
        │   ├── terminal/          # auto-scrolling log panel
        │   ├── metric-card/       # Doto hero metric
        │   ├── status-badge/      # [ STATUS ] label with value-level color
        │   ├── progress/          # aria-enabled progress bar
        │   └── export-actions/    # per-job JSON / CSV / ZIP actions
        └── features/
            ├── dashboard/         # modes, forms, SSE progress, Python crawl, DB panel
            ├── history/           # jobs + deep results, search, pagination, exports
            ├── schedules/         # list / create / delete recurring scrapes
            └── files/             # downloaded file browser + preview modal
</code></pre>

<p>Building the UI is opt-in for Cargo: <code>SHINOBI_BUILD_FRONTEND=1 cargo build --release</code>
or <code>./shinobi.sh --build-frontend</code>. Without a build the API still works;
<code>/</code> returns 404.</p>

<hr>

<h2>3. Routes</h2>

<table>
  <tr><th>Route</th><th>URL</th><th>Feature</th><th>Preserved functionality</th></tr>
  <tr><td>Dashboard</td><td><code>/#/</code></td><td>Dashboard</td><td>stats, fast/deep modes, single/crawl/batch/pycrawl submodes, job progress + terminal, Python crawl results + ZIP, database export/import/clear, keyboard shortcuts</td></tr>
  <tr><td>History</td><td><code>/#/history</code></td><td>History</td><td>job list with pagination/search, per-job JSON/CSV/ZIP exports, cancel/delete, deep results with JSON/CSV export, delete/clear-all, detail view</td></tr>
  <tr><td>Schedules</td><td><code>/#/schedules</code></td><td>Schedules</td><td>list, create (URL + interval ≥ 5 min), delete; the backend scheduler launches them</td></tr>
  <tr><td>Files</td><td><code>/#/files</code></td><td>Files</td><td>downloaded file browser, filter, pagination, image/text preview modal (Esc closes)</td></tr>
</table>

<p><strong>Hash location strategy.</strong> The Rust backend serves the bundle with
<code>tower_http::ServeDir</code> and no SPA rewrite, and backend changes are out of
scope for this phase, so hash URLs keep every route reachable on a hard refresh
(<code>GET /</code> always serves <code>index.html</code>). The router still uses real
route definitions with <code>&lt;router-outlet&gt;</code>; only the URL form changes.</p>

<p>Deep extractions started from the dashboard navigate to <code>/#/history?tab=deep</code>
so results stay reachable. The database tools stay on the dashboard, as before.</p>

<hr>

<h2>4. SSE: xwa-sdk <code>Event</code> protocol</h2>

<p><code>GET /api/jobs/{id}/stream</code> emits <strong>named</strong> SSE events whose
<code>data:</code> line is a JSON envelope:</p>

<pre><code>event: analysis_progress
data: {"seq":7,"type":"analysis_progress","tool":"shinobi",
       "analysis_id":"&lt;job id&gt;","ts":"...","payload":{"percent":40,
       "pages_scraped":4,"files_downloaded":9,"total_pages":10,
       "current_url":"https://example.com/a"}}</code></pre>

<table>
  <tr><th>Type</th><th>Payload</th><th>UI effect</th></tr>
  <tr><td><code>analysis_started</code></td><td><code>url, total_pages, status</code></td><td>status/total pages updated; log line</td></tr>
  <tr><td><code>analysis_progress</code></td><td><code>percent, pages_scraped, files_downloaded, total_pages, current_url</code></td><td>progress bar + counters; log line</td></tr>
  <tr><td><code>item_found</code></td><td><code>kind (email|phone), value, url</code></td><td>appends to job emails/phones (deduplicated); log line</td></tr>
  <tr><td><code>log</code></td><td><code>level, message</code></td><td>terminal line; <code>level=error</code> appends to job errors</td></tr>
  <tr><td><code>analysis_completed</code></td><td><code>status, pages_scraped, files_downloaded, emails, phones, files</code></td><td>terminal state (COMPLETED/CANCELLED normalised); stream closes; stats refreshed</td></tr>
  <tr><td><code>analysis_error</code></td><td><code>code, message, detail, retryable</code></td><td>status → <code>failed</code>, message into errors; stream closes</td></tr>
</table>

<ul>
  <li><strong>Named events:</strong> <code>LiveService</code> registers
  <code>addEventListener(type, …)</code> for all six types plus <code>onmessage</code>;
  a bare <code>EventSource.onmessage</code> would never fire.</li>
  <li><strong>Reconstruction:</strong> <code>core/events.ts</code> exposes pure helpers
  (<code>parseXwaEvent</code>, <code>applyJobEvent</code>, <code>logLineFor</code>,
  <code>progressPercent</code>, <code>normalizeJobStatus</code>) that rebuild the legacy
  <code>JobInfo</code> snapshot (counters, current URL, emails/phones, errors), so the
  dashboard shows the same lines and data the old protocol produced.</li>
  <li><strong>Reconnect:</strong> transient EventSource errors
  (<code>readyState CONNECTING</code>) are ignored; only <code>CLOSED</code> is surfaced.
  On terminal events the component reconciles with <code>GET /api/jobs/{id}</code>.</li>
  <li><strong>Fallback polling:</strong> the history route refreshes <code>GET /api/jobs</code>
  every 3 seconds, as before.</li>
</ul>

<hr>

<h2>5. Health indicators</h2>

<ul>
  <li><strong>RUST</strong> — <code>GET /api/health</code>. Green when reachable, red otherwise,
  grey while probing.</li>
  <li><strong>PYTHON</strong> — the extractor health endpoint is <em>not</em> proxied by the
  backend. <code>ApiService.extractorStatus()</code> probes the <code>/api/python/docs</code>
  proxy: 200 → online, 502/503/504 → offline, anything else (missing endpoint, network
  failure) → <strong>unknown</strong>. The dot is grey for unknown and a tooltip explains the
  probe, so the UI never breaks when the sidecar is absent.</li>
</ul>

<hr>

<h2>6. Exports</h2>

<table>
  <tr><th>Export</th><th>Mechanism</th></tr>
  <tr><td>Job JSON</td><td><code>POST /api/jobs/{id}/export</code> → client-side download</td></tr>
  <tr><td>Analysis JSON/CSV</td><td><code>GET /api/analyses/{id}/export?format=json|csv</code> (server anchor, Content-Disposition)</td></tr>
  <tr><td>Job ZIP</td><td><code>GET /api/jobs/{id}/download</code> (server anchor)</td></tr>
  <tr><td>Deep JSON</td><td>client-side blob from the stored <code>DeepResult</code></td></tr>
  <tr><td>Deep CSV</td><td><code>GET /api/deep/results.csv</code> → blob download</td></tr>
  <tr><td>Database JSON</td><td><code>GET /api/database/export</code> → client-side download</td></tr>
  <tr><td>Database import</td><td><code>POST /api/database/import</code> with <code>{jobs:[...]}</code></td></tr>
</table>

<hr>

<h2>7. State management</h2>

<p>No state library. Signals hold every value that changes asynchronously
(lists, totals, jobs, progress, health, theme) so zoneless change detection can
schedule updates without <code>zone.js</code>; <code>computed()</code> derives filtered
lists and the dashboard aggregates. Form models remain plain properties because
user events already trigger change detection. Theme preference persists in
<code>localStorage</code> under <code>shinobi-theme</code>.</p>

<p><strong>Zoneless rule (Angular 22, no <code>zone.js</code>).</strong> Writing to a
signal (<code>set</code>/<code>update</code>) always schedules change detection. Mutating a
<strong>plain component property</strong> inside an asynchronous callback (SSE,
<code>HttpClient</code>, <code>setTimeout</code>, promise) does <strong>not</strong>: if the
template reads it, inject <code>ChangeDetectorRef</code> and call
<code>this.cdr.markForCheck()</code> after the mutation, or move the state into a
signal. Current cases: <code>ExportActionsComponent</code> (<code>busy</code> while the
client-side JSON export resolves) and the schedules form model reset after
<code>POST /api/schedules</code>. Mounted async callbacks (<code>setInterval</code> polls,
SSE reducers) mutate signals, so they schedule updates on their own.</p>

<hr>

<h2>8. Nothing Design</h2>

<ul>
  <li>Canonical tokens from kabuki/tengu (dark + light first-class), including the
  tokenised <code>--gold</code>; no hardcoded <code>#FFD700</code> anywhere else.</li>
  <li>Self-hosted woff2 (<code>public/fonts</code>, <code>@font-face</code> in
  <code>_fonts.scss</code>); <code>index.html</code> has no Google Fonts links.</li>
  <li>Three-layer hierarchy: Doto display metrics, Space Grotesk headings/body,
  Space Mono ALL-CAPS labels; status colors apply to values, never row backgrounds.</li>
  <li>No shadows, gradients (except the allowed dot-grid motif), skeletons or emojis;
  loading states use <code>[ LOADING... ]</code> text and errors use inline status text.</li>
</ul>

<hr>

<h2>9. Build, dev and tests</h2>

<pre><code># reproducible install (Node 24)
npm ci

# dev server on :4260 (proxy /api → :8060; environment points at http://&lt;host&gt;:8060)
npm start

# unit tests (Vitest + jsdom through the Angular unit-test builder)
npm test

# production build → ../static/browser (outputHashing: none for the Rust server)
npm run build</code></pre>

<p>The production configuration replaces <code>environment.ts</code> with
<code>environment.production.ts</code> (relative URLs), so the same bundle works behind
Docker, a LAN IP or a reverse proxy. The dev configuration resolves
<code>window.location.hostname</code> at runtime, which also works over the LAN.</p>

<p>Test suites:</p>
<ul>
  <li><code>core/api.service.spec.ts</code> — health, scrape payload sanitisation,
  pagination, URL builders, schedules, blob export, extractor status classification, error messages.</li>
  <li><code>core/events.spec.ts</code> — JSON fixtures for every Event type, the
  <code>applyJobEvent</code> reducer, terminal statuses, progress math and log lines.</li>
  <li><code>core/theme.service.spec.ts</code> — default/restored theme, toggle and persistence.</li>
  <li><code>shared/export-actions/export-actions.spec.ts</code> — rendered controls,
  client JSON delegation and server CSV/ZIP URLs.</li>
</ul>

<p><code>build.rs</code> is unchanged: the Angular build only runs with
<code>SHINOBI_BUILD_FRONTEND=1</code>, so <code>cargo build/test/clippy</code> stay offline.
The Dockerfile builds the UI in a <code>node:24-slim</code> stage and copies
<code>/static</code> into the runtime image.</p>

<p>Browser validation lives in <code>e2e/</code>: <code>e2e/browser_smoke.py</code>
(Playwright + headless Chromium) serves the local fixture, verifies the health
dots/stats without clicks, launches a fast scrape from the UI and watches the
progress bar, counters and terminal advance to <code>completed</code> on their own,
then covers History, file preview, schedules and the JSON/CSV exports, failing
on any console/page error. See <code>e2e/README.md</code>.</p>

<hr>

<p><i>Base architecture designed for the Shinobi web scraping tool within the XWA suite.</i></p>
