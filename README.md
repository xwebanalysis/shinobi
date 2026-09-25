
<h1 align="center">Shinobi</h1>

<div align="center">
<img src="https://raw.githubusercontent.com/xscriptor/xassets/main/xwa/shinobi/shinobi-colors.svg" width="150"/> 
<p><em>Silent Web Scraper — Anti-Blocking Download System</em></p>
</div>




**Language / Idioma**  
[English](#) | [Español](./docs/esp/README.md)

<p><em><a href="https://github.com/xwebanalysis/meta">XWA</a>  <strong>submodule focused</strong> on silent web scraping with anti-blocking — under active development</em></p>

<hr>

<p>
<a href="#overview">Overview</a> ·
<a href="#capabilities">Capabilities</a> ·
<a href="#anti-blocking-system">Anti-Blocking</a> ·
<a href="#quick-start">Quick Start</a> ·
<a href="#launch-script-recommended">Launch Script</a> ·
<a href="#web-version-docker-compose">Docker Compose</a> ·
<a href="#standalone-no-docker">Standalone</a> ·
<a href="#api-and-xwa-sdk-contract">API</a> ·
<a href="#environment-variables">Env Variables</a> ·
<a href="#related-documents">Docs</a>
</p>

<hr>

<h2>Overview</h2>

<p>Shinobi is a stealth web scraper with a single web interface. It downloads entire sites — HTML, CSS, JS, images, PDFs — while evading detection through multiple anti-blocking layers. It is <strong>local-first</strong>: SQLite by default, no Redis, no external services required.</p>

<table>
  <tr>
    <th>Interface</th>
    <th>Directory</th>
    <th>Language</th>
    <th>Type</th>
  </tr>
  <tr>
    <td><strong>Shinobi Web</strong></td>
    <td><code>/</code> (monorepo root)</td>
    <td>Rust (Axum 0.8) + Angular 22</td>
    <td>Web application (standalone or Docker)</td>
  </tr>
</table>

<h3>Capabilities</h3>
<ul>
  <li><strong>Recursive Crawling</strong> — BFS page discovery with configurable depth and max pages</li>
  <li><strong>Bounded Concurrency</strong> — Configurable worker batch, hard-capped at 3 simultaneous fetches</li>
  <li><strong>JavaScript Rendering</strong> — Headless Chromium engine for SPA/React/Vue/Angular sites</li>
  <li><strong>Asset Download</strong> — HTML, CSS, JS, images, PDFs, archives, media, fonts</li>
  <li><strong>File Type Filtering</strong> — Select which file extensions to download</li>
  <li><strong>Same-Domain Scoping</strong> — Stay within target domain or crawl freely</li>
  <li><strong>Real-Time Progress</strong> — SSE stream emitting xwa-sdk <code>Event</code> objects</li>
  <li><strong>Scheduler</strong> — Recurring scrapes for schedules whose <code>next_run</code> has passed</li>
  <li><strong>Downloaded File Browser</strong> — Browse and open downloaded files from the web UI</li>
  <li><strong>Persistent Jobs</strong> — SQLite (WAL) keeps jobs, deep results and schedules across restarts</li>
  <li><strong>Two Operation Modes:</strong>
    <ul>
      <li><em>Fast Test</em> — Pure Rust, zero external deps, crawl + download + anti-blocking</li>
      <li><em>Deep Research</em> — Python sidecar adds structured data extraction (JSON-LD, microdata, Open Graph, RDFa), NLP analysis (summary, entities, keywords), custom CSS selectors, and enriched email/phone extraction</li>
    </ul>
  </li>
</ul>

<h3>Anti-Blocking System</h3>
<ul>
  <li><strong>User-Agent Rotation</strong> — Real browser UAs (Chrome, Firefox, Safari, Edge, mobile)</li>
  <li><strong>Header Randomization</strong> — Accept, Accept-Language, Sec-CH-UA, Sec-Fetch-* per request</li>
  <li><strong>Request Delay + Jitter</strong> — Configurable base delay with ±20% random jitter</li>
  <li><strong>Exponential Backoff</strong> — Retry with full jitter on failures (configurable attempts, capped at 120s)</li>
  <li><strong>Rate Limit Handling</strong> — Detects HTTP 429/503, waits, retries with longer backoff</li>
  <li><strong>Proxy Rotation</strong> — HTTP/HTTPS/SOCKS5 round-robin, advances on network failure and 429/503</li>
  <li><strong>robots.txt Compliance</strong> — <code>Allow</code>, <code>Disallow</code> (longest match wins) and <code>Crawl-delay</code> respected by default</li>
</ul>

<hr>

<h2>Quick Start</h2>

<h3>Launch Script (Recommended)</h3>
<pre><code>./shinobi.sh                    # local: SQLite + backend :8060 + extractor :9090
./shinobi.sh --build-frontend   # also build the Angular UI before starting
./shinobi.sh docker             # everything via docker-compose
./shinobi.sh --fast             # legacy: Rust backend only
./shinobi.sh --deep             # legacy: Rust + Python extractor
./shinobi.sh --help             # full usage help</code></pre>

<h3>Web Version (Docker Compose)</h3>
<pre><code>docker compose up -d --build</code></pre>
<ul>
  <li>Web UI / API: <code>http://localhost:8060</code></li>
  <li>Extractor API: <code>http://localhost:9090</code> (Deep Research only)</li>
  <li>The <code>shinobi-data</code> volume persists <code>/data/shinobi.db</code> and <code>/data/downloads</code></li>
</ul>

<h3>Standalone (No Docker)</h3>
<pre><code>Fast Test mode (Rust only)
cargo run --release

Full stack (Rust + Python extractor)
./shinobi.sh</code></pre>
<p>API/UI at <code>http://localhost:8060</code>. SQLite database at <code>./shinobi.db</code> (override with <code>SHINOBI_DB_PATH</code>), downloaded files in <code>./downloads/</code>.</p>

<h3>Building the UI</h3>
<p><code>build.rs</code> never touches npm by default, so <code>cargo build</code>, <code>cargo test</code> and <code>cargo clippy</code> work offline. The frontend is built only when explicitly requested:</p>
<pre><code>SHINOBI_BUILD_FRONTEND=1 cargo build --release</code></pre>
<p>Without <code>static/</code>, the binary still starts and serves the JSON API; requests to <code>/</code> simply 404.</p>

<h3>Frontend Development (Angular 22)</h3>
<p>The UI requires <strong>Node 24</strong> (use <code>mise</code>: <code>export PATH="$HOME/.local/share/mise/installs/node/24/bin:$PATH"</code>). It is an Angular 22 standalone app with zoneless change detection, TypeScript 6, the <code>@angular/build</code> application builder and Vitest.</p>
<pre><code>cd frontend
npm ci                 # reproducible install (package-lock.json is versioned)
npm start              # dev server on http://localhost:4260 (proxy /api → :8060)
npm test               # Vitest + jsdom unit tests
npm run build          # production build → ../static/browser (served by Rust)</code></pre>
<p><code>environment.ts</code> resolves the API host at runtime
(<code>http://&lt;window.location.hostname&gt;:8060</code>) for development;
<code>environment.production.ts</code> uses same-origin relative URLs. Fonts
(Doto, Space Grotesk, Space Mono) are self-hosted in <code>public/fonts</code> and no
Google Fonts are requested at runtime. See <a href="docs/ui-architecture.md">docs/ui-architecture.md</a>
for the full architecture, SSE <code>Event</code> handling and test map.</p>

<h3>Execution Modes</h3>
<table>
  <tr><th>Mode</th><th>Command</th><th>Description</th></tr>
  <tr><td><strong>Local (default)</strong></td><td><code>./shinobi.sh</code></td><td>Backend + Python extractor with local SQLite.</td></tr>
  <tr><td><strong>Fast Test</strong></td><td><code>./shinobi.sh --fast</code></td><td>Rust backend only. No external dependencies.</td></tr>
  <tr><td><strong>Deep Research</strong></td><td><code>./shinobi.sh --deep</code></td><td>Adds Python sidecar for structured data extraction, NLP analysis and custom CSS selectors.</td></tr>
  <tr><td><strong>Docker</strong></td><td><code>./shinobi.sh docker</code></td><td>Both services via docker-compose.</td></tr>
  <tr><td><strong>Python Only</strong></td><td><code>./shinobi.sh --python-only</code></td><td>Extractor standalone for development.</td></tr>
</table>

<h3>Environment Variables</h3>
<table>
  <tr><th>Variable</th><th>Default</th><th>Description</th></tr>
  <tr><td><code>PORT</code></td><td><code>8060</code></td><td>HTTP listen port (Rust)</td></tr>
  <tr><td><code>SHINOBI_DB_PATH</code></td><td><code>shinobi.db</code></td><td>SQLite database path</td></tr>
  <tr><td><code>DATA_DIR</code></td><td><code>downloads</code></td><td>Downloaded files directory (shared with the extractor)</td></tr>
  <tr><td><code>RUST_LOG</code></td><td><code>shinobi=info,tower_http=info</code></td><td>Logging verbosity</td></tr>
  <tr><td><code>EXTRACTOR_URL</code></td><td><code>http://localhost:9090</code></td><td>Python extractor endpoint (Deep Research)</td></tr>
  <tr><td><code>XWA_CORS_ORIGINS</code></td><td>localhost/LAN regex</td><td>CORS origin regex (credentials disabled)</td></tr>
  <tr><td><code>SHINOBI_BUILD_FRONTEND</code></td><td>unset</td><td>Set to <code>1</code> to build the Angular UI during <code>cargo build</code></td></tr>
  <tr><td><code>SHINOBI_GLOBAL_RPS</code></td><td><code>5</code></td><td>Process-wide outbound request rate cap shared by all jobs (requests/second; <code>0</code> disables). Composes with the per-domain delay + jitter.</td></tr>
</table>

<hr>

<h2 id="api-and-xwa-sdk-contract">API and xwa-sdk contract</h2>

<p>All endpoints keep their legacy <code>/api/jobs/*</code> shape and gain semantic <code>/api/analyses/*</code> aliases:</p>
<ul>
  <li><code>GET /api/health</code> → <code>{"status","service":"shinobi","version","database"}</code></li>
  <li><code>GET /api/analyses</code>, <code>GET /api/analyses/{id}</code> → xwa-sdk <code>Analysis</code> envelope</li>
  <li><code>GET /api/analyses/{id}/export?format=json|csv</code> → download with <code>Content-Disposition</code></li>
  <li><code>DELETE /api/analyses/{id}</code> → removes the job and only its artifacts</li>
  <li><code>GET /api/jobs/{id}/stream</code> → SSE with xwa-sdk <code>Event</code> JSON (<code>analysis_started</code>, <code>analysis_progress</code>, <code>item_found</code>, <code>log</code>, <code>analysis_completed</code>, <code>analysis_error</code>)</li>
</ul>

<pre><code>event: analysis_progress
data: {"seq":7,"type":"analysis_progress","tool":"shinobi","analysis_id":"&lt;job id&gt;","ts":"...","payload":{"percent":40,"pages_scraped":4,"total_pages":10,"current_url":"..."}}</code></pre>

<p>The local <code>src/contracts.rs</code> mirrors <code>xwa-sdk 0.2.0</code> (<code>Event</code>, <code>Analysis</code>, <code>Finding</code>, <code>Error</code>, <code>Summary</code>) with no external dependency.</p>

<hr>

<h2>Related Documents</h2>

<table>
  <tr><th>Document</th><th>Description</th></tr>
  <tr><td><a href="docs/architecture.md">docs/architecture.md</a></td><td>Components, data flow, persistence and politeness model</td></tr>
  <tr><td><a href="docs/manual.md">docs/manual.md</a></td><td>Development and production deployment guide</td></tr>
  <tr><td><a href="docs/ui-architecture.md">docs/ui-architecture.md</a></td><td>Frontend architecture (Angular 22, SSE Event protocol, Nothing Design)</td></tr>
  <tr><td><a href="ROADMAP.md">ROADMAP.md</a></td><td>Development phases and milestones</td></tr>
  <tr><td><a href="docs/project-structure.md">docs/project-structure.md</a></td><td>Detailed codebase structure with file-by-file breakdown and API reference</td></tr>
</table>

<hr>

<div id="x" align="center">
<h2>X</h2>

<a href="https://dev.xscriptor.com">
  <img src="https://xscriptor.github.io/icons/icons/code/product-design/xsvg/verified-filled.svg" width="24" alt="X Web" />
</a>
 & 
<a href="https://github.com/xscriptor">
  <img src="https://xscriptor.github.io/icons/icons/code/product-design/xsvg/github.svg" width="24" alt="X Github Profile" />
</a>
 & 
<a href="https://www.xscriptor.com">
  <img src="https://xscriptor.github.io/icons/icons/code/product-design/xsvg/quotes.svg" width="24" alt="Xscriptor web" />
</a>

</div>
