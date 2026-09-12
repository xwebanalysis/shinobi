# Shinobi Architecture

This document describes the current backend components, data flow and the
politeness model. It is the companion to `docs/manual.md` (operations) and
`docs/project-structure.md` (file-by-file map).

## 1. Components

```
                    ┌────────────────────────────────────────────┐
                    │             Angular UI (static/)           │
                    │        EventSource + fetch() JSON          │
                    └───────────────┬────────────────────────────┘
                                    │ HTTP / SSE
                    ┌───────────────▼────────────────────────────┐
                    │            Rust backend (Axum 0.8)         │
                    │   api::routes  (REST + SSE + scheduler)    │
                    │   storage::db  (rusqlite, spawn_blocking)  │
                    │   storage::manager (file tree, traversal)  │
                    │   scraper::*   (crawl, politeness, WARC)   │
                    └───────┬───────────────────────┬────────────┘
                            │ JSON (/extract,/crawl)│ SQLite (WAL)
                    ┌───────▼───────────┐   ┌───────▼────────────┐
                    │ Python extractor  │   │ shinobi.db         │
                    │ FastAPI :9090     │   │ jobs/deep/schedules│
                    │ extruct + spacy   │   └────────────────────┘
                    └───────────────────┘
```

| Layer | Technology | Notes |
|---|---|---|
| HTTP/SSE | Axum 0.8 + Tokio 1.53 | Conexiones asíncronas; SSE con envelope `Event` de xwa-sdk |
| Persistence | SQLite vía rusqlite 0.40 (`bundled`) | WAL, `foreign_keys=ON`, `busy_timeout=5000` |
| Crawling | reqwest 0.13 + scraper 0.27 | BFS por lotes con `buffer_unordered`, tope duro de 3 |
| JS rendering | chromiumoxide 0.9 (`default-features = false`) | Sin `async-std` en el árbol de dependencias |
| Deep research | FastAPI + extruct + BeautifulSoup + spaCy opcional | `DATA_DIR` compartido con el backend |
| Contracts | `src/contracts.rs` | Réplica local de `xwa-sdk 0.2.0`, sin dependencia externa |

## 2. Request / scrape lifecycle

1. `POST /api/scrape` valida la URL y convierte el body (`ScrapeQuery`) en un
   `ScrapeConfig` saneado (concurrencia acotada, límites de profundidad y
   páginas).
2. `launch_scrape` crea el `JobInfo` (estado `queued`), lo inserta en el
   `DashMap` y lanza la tarea compartida. El scheduler usa **la misma función**
   para los schedules vencidos.
3. `spawn_scrape` adquiere un permiso del semáforo global
   (`MAX_CONCURRENCY = 3`), construye el `Downloader` y arranca el crawl.
4. El `Downloader` procesa el frontier BFS en lotes de hasta `concurrency`
   workers (nunca más de 3), emitiendo `ScrapeProgress` por un canal mpsc.
5. El handler de progreso actualiza el `JobInfo` y persiste en SQLite **solo en
   transiciones de estado o cada 2 s** (throttle para no escribir en cada
   evento SSE).
6. SSE re-lee el job cada segundo y emite eventos xwa-sdk: `analysis_started`,
   `analysis_progress` + `log [PAGE]`, `item_found` para emails/teléfonos,
   `log` de errores y `analysis_completed`/`analysis_error` al terminar.

## 3. Persistence and migrations

* `schema_meta(version)` guarda la versión de esquema; `migrate()` es
  idempotente y añade columnas nuevas (`jobs.files`) si faltan.
* `export_all()` serializa `jobs`, `deep_results` y `schedules` bajo el
  marcador `SHINOBI_DB_V1`; `import_all()` acepta ese JSON y devuelve conteos
  por tabla.
* `DELETE /api/analyses/{id}` elimina el job, sus deep results y **solo los
  ficheros listados en `job.files`**, nunca el directorio completo del dominio.
* Docker monta un volumen compartido `/data`:
  `SHINOBI_DB_PATH=/data/shinobi.db` y `DATA_DIR=/data/downloads`.

## 4. Politeness / anti-blocking model

| Control | Implementation |
|---|---|
| Concurrency | `anti_block::MAX_CONCURRENCY = 3`; `clamp_concurrency()` |
| Delay | base configurable, jitter ±20% (`jitter_ms`) |
| Backoff | `base * 2^attempt + jitter`, cap 120 s |
| Rate limit | 429/503 => backoff largo + rotación de proxy |
| Proxies | `ProxyRotator` round-robin; clientes pre-construidos por proxy |
| robots.txt | `Allow`/`Disallow` por prefijo más largo + `Crawl-delay` |
| Page limit | efectivo en el batch BFS (`max_pages`) y en la cola (`2 * max_pages`) |
| Timeouts | 30 s total, 10 s de conexión, keepalive 30 s |
| WARC | `WARC-Record-ID`, `WARC-Warcinfo-ID` y `WARC-Payload-Digest` válidos |

## 5. Failure modes

* Sin base de datos: la API funciona en memoria (`database: "disabled"`).
* Sin UI compilada (`static/`): la API responde; `/` devuelve 404.
* Sin extractor: los endpoints deep devuelven 502 y el crawl sigue en modo fast.
* Sin modelo spaCy: `extractors/nlp.py` degrada al analizador rule-based.
* Sin `httrack`: `/crawl` del extractor falla con un error explícito.
