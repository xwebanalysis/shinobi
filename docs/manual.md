<h1>Shinobi Application Manual</h1>

<p>This document covers local development, production configuration, the xwa-sdk contract and the anti-blocking architecture. See <a href="architecture.md">architecture.md</a> for the component/data-flow view.</p>

<hr>

<h2>1. Local Development</h2>

<h3>1.1 Prerequisites</h3>
<ul>
    <li>Rust 1.88+ (the project is tested with 1.98) with <code>cargo</code></li>
    <li>Python 3.13 and <code>uv</code> for the Deep Research extractor (optional)</li>
    <li>Docker Engine + Docker Compose (optional, for containerised runs)</li>
    <li><code>httrack</code> only if you use the extractor's <code>/crawl</code> endpoint</li>
</ul>

<h3>1.2 Standalone (No Docker)</h3>
<pre><code>cargo run --release
# Listens on http://localhost:8060
# SQLite: ./shinobi.db   Data: ./downloads/</code></pre>
<p>Hot-reload with <code>cargo-watch</code>:</p>
<pre><code>cargo install cargo-watch
cargo watch -x run</code></pre>

<h3>1.3 Launch Script</h3>
<pre><code>./shinobi.sh                    # backend :8060 + extractor :9090 (SQLite local)
./shinobi.sh --build-frontend   # also build the Angular UI
./shinobi.sh docker             # docker compose up --build
./shinobi.sh --fast             # backend only
./shinobi.sh --python-only      # extractor only</code></pre>
<p>The script creates <code>extractor/.venv</code> with <code>uv --python 3.13</code> when missing, installs the pinned requirements and waits for <code>/api/health</code> before printing the URLs.</p>

<h3>1.4 Frontend Build (opt-in)</h3>
<p><code>build.rs</code> does <strong>not</strong> run npm automatically. Builds, tests and clippy are offline and deterministic. To produce the Angular bundle in <code>static/</code>:</p>
<pre><code>SHINOBI_BUILD_FRONTEND=1 cargo build --release</code></pre>
<p>Or manually:</p>
<pre><code>cd frontend
npm install --legacy-peer-deps
npm run build   # output: ../static/browser</code></pre>
<p>Without <code>static/</code> the server still serves the full JSON/SSE API; only the browser UI at <code>/</code> returns 404.</p>

<h3>1.5 Verifying the Setup</h3>
<pre><code>curl -s localhost:8060/api/health
# {"status":"ok","service":"shinobi","version":"0.1.0","database":"ok"}</code></pre>
<p>Open <code>http://localhost:8060</code>, enter a target URL and click <strong>Start Scrape</strong>. Progress arrives as xwa-sdk <code>Event</code> objects over SSE.</p>

<hr>

<h2>2. Production Configuration</h2>

<h3>2.1 Backend (Rust)</h3>
<ul>
    <li>Build with <code>cargo build --release</code> (LTO is configured in <code>Cargo.toml</code>).</li>
    <li><code>SHINOBI_DB_PATH=/var/lib/shinobi/shinobi.db</code> for a persistent database (WAL enabled automatically).</li>
    <li><code>DATA_DIR=/var/lib/shinobi/downloads</code> for scraped files.</li>
    <li><code>PORT=8060</code> by default; run behind a reverse proxy for TLS.</li>
    <li>CORS defaults to a localhost/RFC1918 regex with credentials disabled; override with <code>XWA_CORS_ORIGINS</code> (regex).</li>
</ul>

<h3>2.2 Docker Production Build</h3>
<p>The <code>Dockerfile</code> uses three stages:</p>
<ul>
    <li><strong>ui:</strong> builds the Angular bundle with Node 24 (<code>npm ci</code> + <code>npm run build</code>).</li>
    <li><strong>builder:</strong> compiles the Rust binary with a dependency cache layer (the versioned <code>Cargo.lock</code> is copied in).</li>
    <li><strong>runtime:</strong> Debian slim + Chromium, runs as non-root <code>shinobi</code>, copies the binary and the built <code>static/</code>.</li>
</ul>
<pre><code>docker build -t shinobi:latest .
docker run -d -p 8060:8060 \
  -e SHINOBI_DB_PATH=/data/shinobi.db -e DATA_DIR=/data/downloads \
  -v shinobi-data:/data shinobi:latest</code></pre>
<p>With <code>docker compose up -d --build</code> both shinobi and the extractor share the <code>shinobi-data</code> volume.</p>

<h3>2.3 Security Considerations</h3>
<ul>
    <li><code>storage/manager.rs</code> validates every path against the base data directory (directory traversal protection).</li>
    <li>Job deletion only removes files recorded for that job, never another job's domain directory.</li>
    <li>The API has no authentication by design (local/internal tool). Use a reverse proxy with auth for external exposure.</li>
    <li>Scraping is polite by default: robots.txt, jittered delay, retries with backoff, hard concurrency cap and a page limit.</li>
</ul>

<hr>

<h2>3. xwa-sdk contract</h2>

<p>The local module <code>src/contracts.rs</code> replicates <code>xwa-sdk 0.2.0</code> types without adding a dependency: <code>Event</code>, <code>Analysis</code>, <code>Finding</code>, <code>Error</code>, <code>Summary</code> and the <code>Tool</code>/<code>Severity</code>/<code>AnalysisStatus</code>/<code>EventType</code> enums.</p>

<h3>3.1 REST aliases</h3>
<table>
  <tr><th>Method</th><th>Path</th><th>Response</th></tr>
  <tr><td>GET</td><td><code>/api/analyses</code></td><td><code>{items: Analysis[], total, offset, limit}</code></td></tr>
  <tr><td>GET</td><td><code>/api/analyses/{id}</code></td><td><code>Analysis</code></td></tr>
  <tr><td>GET</td><td><code>/api/analyses/{id}/export?format=json|csv</code></td><td>Attachment with <code>Content-Disposition</code></td></tr>
  <tr><td>DELETE</td><td><code>/api/analyses/{id}</code></td><td><code>{"status":"deleted","removed_files":N}</code></td></tr>
</table>
<p>Status mapping: <code>queued→PENDING</code>, <code>running/scraping/deep→RUNNING</code>, <code>completed→COMPLETED</code>, <code>failed→ERROR</code>, <code>cancelled→CANCELLED</code>.</p>

<h3>3.2 SSE stream</h3>
<pre><code>event: analysis_started
data: {"seq":1,"type":"analysis_started","tool":"shinobi","analysis_id":"&lt;job&gt;","ts":"...","payload":{"url":"...","total_pages":10}}

event: analysis_progress
data: {"seq":2,...,"payload":{"percent":30,"pages_scraped":3,"total_pages":10,"current_url":"..."}}

event: item_found
data: {"seq":3,...,"payload":{"kind":"email","value":"a@b.c","url":"..."}}

event: analysis_completed
data: {"seq":9,...,"payload":{"status":"COMPLETED","pages_scraped":10,"files_downloaded":42}}</code></pre>
<p>Errors during the scrape are emitted as <code>log</code> events with <code>level:"error"</code>; a terminal failure sends <code>analysis_error</code> with an xwa-sdk <code>Error</code> object.</p>

<hr>

<h2>4. Anti-Blocking Architecture</h2>

<p>The anti-blocking system (<code>src/scraper/anti_block.rs</code> + <code>client.rs</code>) applies these techniques:</p>

<h3>4.1 User-Agent and headers</h3>
<p>A pool of real browser user-agent strings is rotated per request when <code>user_agent_rotation</code> is enabled. Each request randomises <code>Accept</code>, <code>Accept-Language</code>, <code>Sec-CH-UA</code> (Chrome version) and <code>Sec-Fetch-*</code>.</p>

<h3>4.2 Timing</h3>
<ul>
    <li><strong>Base delay:</strong> configurable (default 1000 ms) between requests to the same host.</li>
    <li><strong>Jitter:</strong> ±20% per request (<code>jitter_ms</code>).</li>
    <li><strong>Crawl-delay:</strong> the effective delay is the maximum of the configured delay/rate limit and the robots.txt value.</li>
    <li><strong>Backoff:</strong> on failure, <code>base * 2^attempt + jitter</code>, capped at 120 s.</li>
</ul>

<h3>4.3 Concurrency</h3>
<p>The BFS front is consumed in batches of at most <code>concurrency</code> workers, hard-capped at <code>MAX_CONCURRENCY = 3</code>. A global semaphore (<code>AppState.scrape_semaphore</code>) also caps simultaneous scrape jobs.</p>

<h3>4.4 Rate limits and proxies</h3>
<p>HTTP 429/503 trigger a longer backoff and proxy rotation. When proxies are configured, requests use round-robin clients (one per proxy); network failures, 429 and 503 advance the cursor so the next attempt uses a different exit IP.</p>

<h3>4.5 robots.txt and sitemaps</h3>
<p>robots.txt is fetched before crawling. The parser supports <code>Allow</code>, <code>Disallow</code> and <code>Crawl-delay</code>; the longest matching path wins and <code>Allow</code> breaks ties. <code>/sitemap.xml</code> is parsed (including CDATA and XML entities) to seed the queue.</p>

<hr>

<h2>5. Database and operations</h2>

<h3>5.1 Export / import / clear</h3>
<pre><code>curl -s localhost:8060/api/database/export > backup.json
curl -s -X POST localhost:8060/api/database/import -H 'content-type: application/json' -d @backup.json
curl -s -X POST localhost:8060/api/database/clear</code></pre>
<p>The export (marker <code>SHINOBI_DB_V1</code>) contains <code>jobs</code>, <code>deep_results</code> and <code>schedules</code>. Import counts each table and old exports (jobs only) remain valid.</p>

<h3>5.2 Scheduler</h3>
<p>A background task runs every 30 s, loads schedules and launches a real scrape for every entry with <code>enabled=1</code> and <code>next_run &lt;= now</code>. It reuses <code>launch_scrape()</code> (the same code path as <code>POST /api/scrape</code>) and then updates <code>last_run</code> and <code>next_run = now + interval_min</code>.</p>

<h3>5.3 WARC export</h3>
<p>When <code>export_warc</code> is enabled the crawl writes <code>{domain}/site.warc</code> with a file-level <code>warcinfo</code> record plus one response record per page, each with valid <code>WARC-Record-ID</code>, <code>WARC-Warcinfo-ID</code>, <code>WARC-Payload-Digest</code> and <code>Content-Length</code>.</p>

<h3>5.4 JavaScript rendering</h3>
<p>With <code>javascript_rendering</code> enabled, HTML pages load in headless Chromium via chromiumoxide (<code>--no-sandbox</code>, <code>--disable-gpu</code>, <code>--disable-dev-shm-usage</code>), wait 3 s for execution and return the rendered DOM plus an optional screenshot. Chromium must be installed locally; the Docker image includes it. If launch fails, Shinobi logs a warning and falls back to the HTTP client.</p>

<hr>

<h2>6. Testing</h2>
<pre><code>cargo fmt --check
cargo clippy -- -D warnings
cargo test                      # unit + integration tests, no network

cd extractor
.venv/bin/pytest -q             # 22 tests, no network</code></pre>

<p><i>End of Manual.</i></p>
