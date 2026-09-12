# Shinobi Development Roadmap

This document tracks the strategic steps required to evolve Shinobi into a full-scale silent web scraping platform.
This file is formatted to be synced automatically with GitHub Issues using the `xgh` roadmap standard.

> Legend: `[x]` shipped and verified in-tree · `[ ]` pending. Only real,
> verifiable work is marked as done.

## Core Engine <!-- phase:core -->

- [x] BFS recursive crawler with configurable depth and max pages
- [x] Bounded concurrent batch processing (config `concurrency`, hard cap 3)
- [x] HTML link extraction (a[href], link[href], img[src], script[src], etc.)
- [x] Asset download (CSS, JS, images, PDFs, archives, media, fonts)
- [x] Same-domain scoping
- [x] File type filtering
- [x] URL canonicalization (strip fragments, normalize trailing slashes)
- [x] Content deduplication by SHA-256 hash
- [x] Sitemap.xml parsing for URL discovery (CDATA/entities)
- [x] robots.txt parsing with Allow/Disallow (longest match) + Crawl-delay
- [x] Per-job artifact tracking: delete/export/zip only touch that job's files
- [x] WARC/1.0 output with valid Record-ID/Warcinfo-ID/Payload-Digest

## Persistence & Scheduling <!-- phase:persistence -->

- [x] SQLite via rusqlite with WAL, foreign_keys and busy_timeout
- [x] Schema versioning (`schema_meta`) with idempotent migrations
- [x] All DB work off the async runtime (`tokio::task::spawn_blocking`)
- [x] Throttled progress persistence (state transitions / every 2 s)
- [x] Full-database export/import/clear (jobs + deep results + schedules)
- [x] Real scheduler: launches due schedules via the same path as `POST /api/scrape`
- [x] Injected-clock unit tests for the scheduler decision

## Anti-Blocking System <!-- phase:anti-blocking -->

- [x] User-Agent rotation (real browser UAs)
- [x] Header randomisation (Accept, Accept-Language, Sec-CH-UA, Sec-Fetch-*)
- [x] Request delay with ±20% jitter
- [x] Exponential backoff with jitter (cap 120 s)
- [x] HTTP 429/503 rate-limit detection and longer backoff
- [x] HTTP/HTTPS/SOCKS5 proxy support
- [x] Real round-robin proxy rotation (advances on network failure/429/503)
- [x] Explicit request timeouts
- [ ] Request fingerprint randomisation (TLS client hello)
- [ ] Global cross-job throttle beyond the per-domain delay

## Web Interface <!-- phase:web-ui -->

- [x] Scrape configuration form
- [x] Real-time SSE progress streaming (xwa-sdk `Event` envelope)
- [x] Jobs list with status badges
- [x] Downloaded files browser
- [x] Cancel running job
- [x] Dark instrument-panel UI theme
- [x] Angular 22 SPA (standalone components, signals, zoneless)
- [x] Opt-in frontend build (`SHINOBI_BUILD_FRONTEND=1`); `cargo test` stays offline
- [x] Two-mode tabs: Fast Test / Deep Research
- [x] ZIP download for completed jobs (per-job contents)
- [x] Export JSON/CSV via `/api/analyses/{id}/export`
- [x] Scheduler UI panel (backend scheduler is functional)
- [x] Angular 22 + TypeScript 6 + `@angular/build` + Vitest (Fase 5)
- [x] Nothing design tokens + self-hosted fonts (Fase 5)

## Deep Research (Python Sidecar) <!-- phase:deep -->

### Extraction
- [x] Structured data extraction (JSON-LD, microdata, Open Graph, RDFa) via extruct
- [x] Metadata extraction (title, description, keywords, canonical)
- [x] Headings extraction (document outline)
- [x] Link analysis (internal vs external, anchor text)
- [x] HTML table extraction to structured data
- [x] Image extraction (src, alt, dimensions)
- [x] Email and phone extraction (complete phone numbers, 7–15 digit filter)
- [x] Custom CSS selector extraction
- [x] pytest suite for regex/structured/NLP/crawler (no network)
- [x] Pinned Python 3.13 requirements; optional spaCy model with rule-based fallback
- [ ] Content extraction (Readability / Mozilla Readability)
- [ ] Article extraction (news article, blog post body)
- [ ] Form field detection and analysis
- [ ] Schema.org validation and normalization
- [ ] Price / product data extraction
- [ ] Review / rating extraction
- [ ] API endpoint discovery from web pages

### NLP & Analysis
- [x] Text extraction from HTML (strip markup)
- [x] Extractive summarization (TF + position scoring)
- [x] Named entity recognition (pattern-based)
- [x] Keyword extraction with TF, bigrams, density
- [x] Sentiment analysis (dictionary-based)
- [x] Readability scoring (Flesch Reading Ease)
- [x] Text statistics (word count, sentence count)
- [x] spaCy integration (optional model, degrades to rule-based)
- [ ] Language detection (lingua / langdetect)
- [ ] Translation (NLLB / deep-translator)
- [ ] Topic modeling (LDA / BERTopic)
- [ ] Text classification (zero-shot / LLM)
- [ ] Keyphrase extraction (RAKE / TextRank)
- [ ] Named entity linking (Wikipedia / Wikidata)
- [ ] Relation extraction between entities
- [ ] Content similarity / clustering across pages
- [ ] Change detection (diff between scrapes)

### Infrastructure
- [x] FastAPI server with /extract endpoint
- [x] Health check endpoint
- [x] Docker support with docker-compose (shared `/data` volume)
- [x] Auto venv creation and pip install (uv, Python 3.13)
- [x] Shared `DATA_DIR` default (`<repo>/downloads`, Docker `/data/downloads`)
- [ ] Async endpoint for batch processing
- [ ] WebSocket for real-time extraction progress
- [ ] Extraction pipeline configuration (YAML)
- [ ] Plugin system for custom extractors
- [ ] Redis queue for job distribution (local-first: not a priority)
- [ ] Result caching with TTL
- [ ] Rate limiting for external APIs

## Advanced Features <!-- phase:advanced -->

- [x] JavaScript rendering via headless Chromium (chromiumoxide 0.9)
- [x] Screenshot capture of scraped pages
- [x] Email and phone number extraction from scraped content
- [x] Webhook notification on scrape completion
- [x] JSON export of job results
- [x] Batch URL extraction
- [x] Crawl + Extract mode (combine BFS with Python sidecar)
- [x] CSV export of deep research results
- [x] WARC/ARC archive format output
- [ ] Readability/article extraction
- [ ] Screenshot gallery in the UI
- [ ] Full-text search across scraped content
- [ ] Diff view between scrape versions
- [ ] Visual comparison of scraped pages

## Production Hardening <!-- phase:production -->

- [ ] Authentication middleware for API endpoints
- [x] CORS origin restrictability (`XWA_CORS_ORIGINS`, credentials disabled)
- [x] Path traversal protection on file reads
- [x] Per-job artifact isolation (no whole-domain deletes)
- [ ] Output size limits and disk usage monitoring
- [ ] Structured logging with span-based request tracing
- [ ] Pause/resume jobs
- [ ] Job queue with priority levels
- [ ] Email/Slack/Discord notifications
- [ ] Prometheus metrics endpoint
- [ ] Rate limiting across all jobs (global throttle)

## XWA Integration <!-- phase:xwa -->

- [x] Local `src/contracts.rs` replicating xwa-sdk 0.2.0 (Event/Analysis/Finding/Error/Summary)
- [x] `GET /api/analyses` + `/{id}` + `/{id}/export` + `DELETE /{id}` aliases
- [x] SSE emits xwa-sdk events with `analysis_id` = persisted job id
- [x] `/api/health` standard JSON (`status`,`service`,`version`,`database`)
- [ ] Angular shared component library with Samurai (Fase 5)
- [ ] Cross-compatible database schema with Samurai
- [ ] Unified XWA docker-compose orchestration
- [ ] XWA API gateway integration
