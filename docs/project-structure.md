<h1 align="center">Project Structure</h1>

<p>Detailed breakdown of the Shinobi codebase (backend hardened, 2026-09).</p>

<hr>

<h2>Root Directory</h2>

<table>
  <tr><th>File</th><th>Purpose</th></tr>
  <tr>
    <td><code>Cargo.toml</code></td>
    <td>Rust package manifest. axum 0.8, tokio 1.53, tower-http 0.7, reqwest 0.13, scraper 0.27, rusqlite 0.40 (bundled), chromiumoxide 0.9 (<code>default-features = false</code>, no async-std), zip 8, rand 0.10, sha2 0.11.</td>
  </tr>
  <tr>
    <td><code>Cargo.lock</code></td>
    <td><strong>Versioned</strong> (removed from <code>.gitignore</code>) so binaries build reproducibly.</td>
  </tr>
  <tr>
    <td><code>build.rs</code></td>
    <td>Builds the Angular UI <strong>only</strong> when <code>SHINOBI_BUILD_FRONTEND=1</code>. Default builds/tests/clippy never touch npm or the network.</td>
  </tr>
  <tr>
    <td><code>src/lib.rs</code></td>
    <td>Library crate exposing <code>api</code>, <code>config</code>, <code>contracts</code>, <code>scraper</code>, <code>storage</code> for integration tests.</td>
  </tr>
  <tr>
    <td><code>src/main.rs</code></td>
    <td>Binary entry point: tracing, env vars (<code>PORT=8060</code>, <code>DATA_DIR</code>, <code>SHINOBI_DB_PATH</code>), DB load, CORS regex, router, scheduler spawn, Axum server.</td>
  </tr>
  <tr>
    <td><code>Dockerfile</code></td>
    <td>Three stages: Node 24 UI build → Rust build (with versioned lockfile cache) → Debian slim + Chromium runtime as non-root <code>shinobi</code>. Copies <code>static/</code> from the UI stage.</td>
  </tr>
  <tr>
    <td><code>docker-compose.yml</code></td>
    <td><code>shinobi</code> (:8060) + <code>extractor</code> (:9090) sharing the <code>shinobi-data</code> volume (<code>/data/shinobi.db</code>, <code>/data/downloads</code>).</td>
  </tr>
  <tr>
    <td><code>.dockerignore</code></td>
    <td>Excludes target, node_modules, venvs, DBs and downloads from the build context.</td>
  </tr>
  <tr>
    <td><code>shinobi.sh</code></td>
    <td>Launcher: <code>local</code> (default), <code>docker</code>, legacy <code>--fast</code>/<code>--deep</code>/<code>--python-only</code>/<code>--build-frontend</code>/<code>-D</code>.</td>
  </tr>
  <tr>
    <td><code>clean.sh</code></td>
    <td>Removes build artifacts, static bundle, DB, downloads and Python venv/caches (<code>--all</code> also removes <code>frontend/node_modules</code>).</td>
  </tr>
  <tr>
    <td><code>tests/</code></td>
    <td>Integration tests (<code>db_tests.rs</code>, <code>api_tests.rs</code>, <code>contracts_fixture.rs</code>) and minimal xwa-sdk fixtures.</td>
  </tr>
</table>

<hr>

<h2><code>src/</code> — Rust Backend</h2>

<h3><code>contracts.rs</code></h3>
<p>Local replica of the <code>xwa-sdk 0.2.0</code> data model (no external dependency): <code>Tool</code>, <code>AnalysisStatus</code>, <code>EventType</code>, <code>Severity</code>, <code>Confidence</code>, <code>Error</code>, <code>Summary</code>, <code>Analysis</code>, <code>Finding</code>, <code>Event</code>. Optionals are skipped when <code>None</code>; enum spellings match the schemas (<code>PENDING</code>, <code>analysis_progress</code>, <code>critical</code>…).</p>

<h3><code>config.rs</code></h3>
<p><code>ScrapeConfig</code>: URL, depth, concurrency, delay, max pages, robots, assets, UA rotation, proxies, retries, JS rendering, screenshots, email extraction, webhooks, dedup, rewriting, index generation, WARC, Basic auth, deep mode (structured/NLP/selectors), export format and extractor endpoint.</p>

<h3><code>api/routes.rs</code></h3>
<p>REST + SSE + scheduler in one module. Highlights:</p>
<ul>
  <li><code>launch_scrape()</code> — shared job creation used by <code>POST /api/scrape</code> and the scheduler.</li>
  <li><code>spawn_scrape()</code> — semaphore (max 3), <code>Downloader</code>, progress loop with <strong>throttled</strong> SQLite writes (state transitions or every 2 s).</li>
  <li><code>event_stream()</code> — diff-based xwa-sdk <code>Event</code> SSE (<code>analysis_started</code>, <code>analysis_progress</code>, <code>item_found</code>, <code>log</code>, <code>analysis_completed</code>/<code>analysis_error</code>).</li>
  <li><code>job_to_analysis()</code> — `JobInfo` → xwa-sdk <code>Analysis</code> mapping.</li>
  <li><code>schedule_is_due()</code>/<code>next_run_at()</code> — pure scheduler decision (unit-tested with an injected clock).</li>
  <li><code>scheduler_worker()</code> — every 30 s launches due schedules and advances <code>last_run</code>/<code>next_run</code>.</li>
  <li><code>delete_job_impl()</code> — deletes the job, its deep results and only the files listed in <code>job.files</code>.</li>
</ul>

<p>Complete endpoint reference:</p>
<table>
  <tr><th>Method</th><th>Path</th><th>Description</th></tr>
  <tr><td>POST</td><td><code>/api/scrape</code></td><td>Start a scrape</td></tr>
  <tr><td>GET</td><td><code>/api/jobs</code> · <code>/api/jobs/{id}</code></td><td>Legacy job listing/detail</td></tr>
  <tr><td>GET</td><td><code>/api/jobs/{id}/stream</code></td><td>SSE with xwa-sdk <code>Event</code> JSON</td></tr>
  <tr><td>POST</td><td><code>/api/jobs/{id}/cancel</code></td><td>Cancel a running job</td></tr>
  <tr><td>DELETE</td><td><code>/api/jobs/{id}</code></td><td>Delete job + its artifacts</td></tr>
  <tr><td>POST</td><td><code>/api/jobs/{id}/export</code></td><td>JSON export of job + analysis + files</td></tr>
  <tr><td>GET</td><td><code>/api/jobs/{id}/download</code></td><td>ZIP of the job's own files</td></tr>
  <tr><td>GET</td><td><code>/api/analyses</code> · <code>/{id}</code></td><td>xwa-sdk <code>Analysis</code> listing/detail</td></tr>
  <tr><td>GET</td><td><code>/api/analyses/{id}/export?format=json|csv</code></td><td>Attachment export</td></tr>
  <tr><td>DELETE</td><td><code>/api/analyses/{id}</code></td><td>Semantic alias of job delete</td></tr>
  <tr><td>GET</td><td><code>/api/health</code></td><td><code>{status,service,version,database}</code></td></tr>
  <tr><td>GET</td><td><code>/api/files</code> · <code>/api/files/{*path}</code></td><td>File listing/read (traversal-safe)</td></tr>
  <tr><td>GET</td><td><code>/api/search</code> · <code>/api/stats</code></td><td>File search and aggregate stats</td></tr>
  <tr><td>GET/POST</td><td><code>/api/database/export|import|clear</code></td><td>Full DB backup/restore/reset</td></tr>
  <tr><td>POST</td><td><code>/api/deep/scrape|batch|crawl</code></td><td>Deep Research via extractor</td></tr>
  <tr><td>GET/POST</td><td><code>/api/deep/crawl/{id}/status|results|cancel</code></td><td>Extractor crawl lifecycle</td></tr>
  <tr><td>GET/DELETE</td><td><code>/api/deep/results</code> · <code>/{id}</code> · <code>.csv</code></td><td>Deep result storage/export</td></tr>
  <tr><td>GET/POST</td><td><code>/api/schedules</code> · <code>DELETE /{id}</code></td><td>Recurring scrape definitions</td></tr>
  <tr><td>GET</td><td><code>/api/python/docs</code></td><td>Proxy to the extractor's Swagger UI</td></tr>
</table>

<h3><code>scraper/</code> — Crawling Engine</h3>

<h4><code>anti_block.rs</code></h4>
<ul>
  <li><code>MAX_CONCURRENCY = 3</code> + <code>clamp_concurrency()</code></li>
  <li><code>jitter_ms()</code> — ±20%</li>
  <li><code>backoff_ms()</code> — exponential + full jitter, capped at 120 s</li>
  <li><code>ProxyRotator</code> — round-robin cursor with failure-driven advance</li>
  <li>Real browser UAs + header randomisation</li>
</ul>

<h4><code>client.rs</code></h4>
<p>reqwest wrapper: timeouts (30 s total / 10 s connect), gzip+brotli, cookie store, Basic auth, per-domain delay with jitter, robots <code>Crawl-delay</code> integration, one pre-built client per proxy, retry with backoff and 429/503 handling.</p>

<h4><code>downloader.rs</code></h4>
<p>BFS front processed in concurrent batches (bounded by <code>clamp_concurrency</code>): robots check, optional JS rendering, SHA-256 dedup, link extraction, URL rewriting, asset filtering, screenshots, email/phone extraction, deep-mode calls, WARC recording and progress via <code>ScrapeProgress</code> (includes <code>saved_files</code> for per-job deletion).</p>

<h4><code>renderer.rs</code></h4>
<p>Headless Chromium via chromiumoxide 0.9 (new headless mode), 3 s render wait, HTML + optional screenshot.</p>

<h4><code>extractor.rs</code></h4>
<p>Regex email/phone extraction; phone regex is non-capturing and validates 7–15 digits so it returns the <strong>complete</strong> number.</p>

<h4><code>rewriter.rs</code></h4>
<p>Rewrites same-domain absolute/protocol-relative URLs to relative paths and generates the offline index.</p>

<h4><code>robots.rs</code></h4>
<p>RFC-9309-style parser: <code>Allow</code>/<code>Disallow</code> with longest-match precedence and <code>Crawl-delay</code> for <code>User-agent: *</code>.</p>

<h4><code>sitemap.rs</code></h4>
<p><code>&lt;loc&gt;</code> extraction with CDATA and XML entity handling.</p>

<h4><code>warc.rs</code></h4>
<p>WARC/1.0 writer with a <code>warcinfo</code> record and per-response <code>WARC-Record-ID</code>, <code>WARC-Warcinfo-ID</code>, <code>WARC-Payload-Digest</code> and <code>Content-Length</code>.</p>

<h3><code>storage/</code> — Persistence Layer</h3>

<h4><code>manager.rs</code></h4>
<p>File tree under <code>DATA_DIR</code>: <code>save_file</code>, traversal-safe <code>read_file</code>, recursive <code>list_files</code>, and <code>remove_file</code> that prunes empty parent directories.</p>

<h4><code>db.rs</code></h4>
<p>SQLite via rusqlite (bundled): PRAGMAs (<code>journal_mode=WAL</code>, <code>foreign_keys=ON</code>, <code>busy_timeout=5000</code>), <code>schema_meta(version)</code> migrations, all queries on <code>spawn_blocking</code>, CRUD for jobs/deep results/schedules and full <code>export_all</code>/<code>import_all</code>/<code>clear_all</code>.</p>

<hr>

<h2><code>frontend/</code> — Angular 22 SPA</h2>

<p>The UI is Angular 22 (standalone components, signals, zoneless change detection, <code>@angular/build:application</code>, output <code>../static</code>, Vitest tests, self-hosted Nothing Design fonts). Layout: <code>core/</code> (api, SSE live, events, theme, export), <code>shared/</code> (terminal, metric-card, status-badge, progress, export-actions) and <code>features/</code> (dashboard, history, schedules, files). See <a href="ui-architecture.md">ui-architecture.md</a> for the full architecture and SSE <code>Event</code> handling. The backend serves the built bundle from <code>static/browser/</code>.</p>

<hr>

<h2><code>extractor/</code> — Python Sidecar</h2>

<h3><code>main.py</code> — FastAPI Server (:9090)</h3>
<ul>
  <li><code>GET /health</code></li>
  <li><code>POST /extract</code> — structured, NLP, metadata, headings, links, tables, images, selectors, emails/phones</li>
  <li><code>POST /crawl</code> + <code>GET/POST /crawl/{id}/…</code> — httrack crawl lifecycle</li>
</ul>
<p><code>DATA_DIR</code> is shared with the backend (env override; default <code>&lt;repo&gt;/downloads</code>). Phone regex returns complete numbers and filters 7–15 digits.</p>

<h3><code>extractors/structured.py</code></h3>
<p>extruct (JSON-LD, microdata, Open Graph, RDFa) with a manual OG fallback, plus metadata, headings, internal/external link classification (netloc comparison), tables, images and custom CSS selectors.</p>

<h3><code>extractors/nlp.py</code></h3>
<p>Rule-based summarisation, entities, keywords/bigrams, sentiment, Flesch readability and text stats. If spaCy + <code>en_core_web_sm</code> are available it enriches entities (<code>method: "spacy"</code>); otherwise it degrades cleanly to <code>method: "rule-based"</code>.</p>

<h3><code>extractors/crawler.py</code></h3>
<p><code>CrawlJob</code> manages the httrack subprocess, parses progress, extracts results and creates a single ZIP in the output directory (created on demand). <code>CrawlManager</code> serialises crawl jobs.</p>

<h3><code>requirements.txt</code> / <code>requirements-dev.txt</code></h3>
<p>Pinned versions for Python 3.13 (fastapi, uvicorn, httpx, extruct, beautifulsoup4, lxml, cssselect, spacy). The spaCy model is optional and documented; pytest is a dev-only pin.</p>

<h3><code>tests/</code></h3>
<p>pytest suite (regex, structured, NLP, crawler ZIP) with no network access; run with <code>extractor/.venv/bin/pytest -q</code>.</p>

<hr>

<h2>Architecture Overview</h2>

<pre><code>                          ┌─────────────────────────────┐
                          │      Browser (Angular 22)    │
                          │   localhost:8060             │
                          └──────────┬──────────────────┘
                                     │ HTTP / SSE (xwa-sdk Event)
                          ┌──────────▼──────────────────┐
                          │     Rust Backend (Axum 0.8)  │
                          │     localhost:8060           │
                          │  api/routes.rs + scheduler   │
                          │  scraper/ (anti_block,       │
                          │    client, downloader,       │
                          │    renderer, extractor,      │
                          │    rewriter, robots,         │
                          │    sitemap, warc)            │
                          │  storage/ (manager, db)      │
                          └──────┬───────────────┬───────┘
                                 │ HTTP (JSON)   │ SQLite WAL
                          ┌──────▼────────┐ ┌────▼──────────────┐
                          │ Python :9090  │ │ shinobi.db        │
                          │ extractor     │ │ /data/shinobi.db  │
                          └───────────────┘ └───────────────────┘</code></pre>
