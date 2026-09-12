<h1 align="center">Estructura del Proyecto</h1>

<p>Desglose detallado del código base de Shinobi.</p>

<div style="border:1px solid #D4A843;padding:10px 14px;margin:12px 0">
<strong>Nota:</strong> la versión inglesa (<a href="project-structure.md">docs/project-structure.md</a>)
refleja el backend endurecido (Axum 0.8, SQLite WAL, contratos xwa-sdk, scheduler real).
Esta traducción puede quedar desactualizada en detalles puntuales.
</div>

<hr>

<h2>Directorio Raíz</h2>

<table>
  <tr><th>Archivo</th><th>Propósito</th></tr>
  <tr>
    <td><code>Cargo.toml</code></td>
    <td>Manifiesto de Rust. Dependencias: <code>axum</code>, <code>reqwest</code> (con socks/cookies/gzip/brotli), <code>scraper</code>, <code>chromiumoxide</code> (Chrome headless), <code>rusqlite</code> (SQLite), <code>tokio</code>, <code>serde</code>, <code>tower-http</code>, <code>zip</code>, <code>flate2</code>, <code>sha2</code>, <code>regex</code>, <code>uuid</code>, <code>chrono</code>, <code>rand</code>, <code>base64</code>.</td>
  </tr>
  <tr>
    <td><code>build.rs</code></td>
    <td>Compila el frontend Angular <strong>solo</strong> si <code>SHINOBI_BUILD_FRONTEND=1</code>. Por defecto <code>cargo build/test/clippy</code> no tocan npm ni la red.</td>
  </tr>
  <tr>
    <td><code>Dockerfile</code></td>
    <td>Build multi-etapa: etapa 1 compila Rust + Angular, etapa 2 copia el binario + Chromium para renderizado JS. Ejecuta como usuario no-root <code>shinobi</code>.</td>
  </tr>
  <tr>
    <td><code>docker-compose.yml</code></td>
    <td>Despliegue de dos servicios: <code>shinobi</code> (Rust, puerto 8060) y <code>extractor</code> (Python, puerto 9090) con volumen compartido <code>shinobi-data</code> (<code>/data/shinobi.db</code> + <code>/data/downloads</code>).</td>
  </tr>
  <tr>
    <td><code>shinobi.sh</code></td>
    <td>Script de lanzamiento. Modos: <code>local</code> (default), <code>docker</code>, y alias legacy <code>--fast</code>, <code>--deep</code>, <code>--python-only</code>, <code>--build-frontend</code>. Crea el venv del extractor con uv y hace apagado graceful.</td>
  </tr>
  <tr>
    <td><code>shinobi.db</code></td>
    <td>Base de datos SQLite (en tiempo de ejecución). Almacena trabajos, resultados deep, y schedules.</td>
  </tr>
</table>

<hr>

<h2><code>src/</code> — Backend Rust</h2>

<h3><code>main.rs</code></h3>
<p>Punto de entrada. Inicializa tracing, lee variables de entorno (<code>PORT</code>, <code>DATA_DIR</code>, <code>SHINOBI_DB_PATH</code>), configura <code>StorageManager</code> + <code>DbStore</code>, monta el router Axum con CORS, e inicia el worker de scheduler (se ejecuta cada 60s para lanzar scrapes programados).</p>

<h3><code>config.rs</code></h3>
<p>Define <code>ScrapeConfig</code> — el modelo completo de parámetros de scraping con valores por defecto sensatos:</p>
<ul>
  <li>URL, profundidad (2), concurrencia (3), delay (1000ms), max páginas (100)</li>
  <li>Restricción de mismo dominio, respeto a robots.txt, toggles de descarga de assets</li>
  <li>Anti-bloqueo: rotación de UA, lista de proxies, reintentos (3)</li>
  <li>Renderizado JS, captura de screenshots, extracción de emails</li>
  <li>Modo deep: datos estructurados, NLP, selectores CSS personalizados</li>
  <li>Auth: usuario/contraseña Basic con modo configurable</li>
  <li>Exportación: WARC, ZIP, generación de index.html</li>
</ul>

<h3><code>api/</code> — Capa de API REST</h3>

<h4><code>routes.rs</code></h4>
<p>~980 líneas — todos los endpoints HTTP en un solo archivo. Grupos principales:</p>

<p><strong>Trabajos de Scrape</strong></p>
<ul>
  <li><code>POST /api/scrape</code> — inicia scrape, devuelve ID del trabajo, ejecuta BFS en background</li>
  <li><code>GET /api/jobs</code> — listado paginado de trabajos</li>
  <li><code>GET /api/jobs/:id</code> — detalle de un trabajo</li>
  <li><code>GET /api/jobs/:id/stream</code> — progreso SSE en tiempo real</li>
  <li><code>POST /api/jobs/:id/cancel</code> — cancelar trabajo en ejecución</li>
  <li><code>DELETE /api/jobs/:id</code> — eliminar trabajo + limpieza opcional de archivos</li>
  <li><code>POST /api/jobs/:id/export</code> — exportación JSON de metadatos</li>
  <li><code>GET /api/jobs/:id/download</code> — archivo ZIP de archivos descargados</li>
</ul>

<p><strong>Archivos</strong></p>
<ul>
  <li><code>GET /api/files</code> — listado paginado de archivos</li>
  <li><code>GET /api/files/*path</code> — servir archivo descargado</li>
  <li><code>GET /api/search</code> — búsqueda de archivos por nombre</li>
</ul>

<p><strong>Sistema</strong></p>
<ul>
  <li><code>GET /api/stats</code> — estadísticas (trabajos, scrapes activos, archivos, uso de disco)</li>
  <li><code>GET /api/health</code> — health check</li>
  <li><code>POST /api/database/export</code> / <code>import</code> / <code>clear</code> — gestión de base de datos</li>
</ul>

<p><strong>Deep Research</strong></p>
<ul>
  <li><code>POST /api/deep/scrape</code> — extracción de URL única</li>
  <li><code>POST /api/deep/batch</code> — extracción por lotes</li>
  <li><code>POST /api/deep/crawl</code> — crawl Python vía httrack</li>
  <li><code>GET /api/deep/crawl/:id/status</code> / <code>results</code> / <code>cancel</code> — ciclo de vida del crawl</li>
  <li><code>GET /api/deep/results</code> / <code>:id</code> — listar/leer resultados</li>
  <li><code>DELETE /api/deep/results</code> / <code>:id</code> — eliminar resultados</li>
  <li><code>GET /api/deep/results.csv</code> — exportación CSV</li>
</ul>

<p><strong>Schedule</strong></p>
<ul>
  <li><code>GET /api/schedules</code> — listar schedules</li>
  <li><code>POST /api/schedules</code> — crear schedule (intervalo mínimo: 5 min)</li>
  <li><code>DELETE /api/schedules/:id</code> — eliminar schedule</li>
</ul>

<p>Referencia completa de endpoints:</p>
<table>
  <tr><th>Método</th><th>Ruta</th><th>Descripción</th></tr>
  <tr><td><code>POST</code></td><td><code>/api/scrape</code></td><td>Iniciar un nuevo trabajo de scrape</td></tr>
  <tr><td><code>GET</code></td><td><code>/api/jobs</code></td><td>Listar todos los trabajos</td></tr>
  <tr><td><code>GET</code></td><td><code>/api/jobs/:id</code></td><td>Obtener estado de un trabajo</td></tr>
  <tr><td><code>GET</code></td><td><code>/api/jobs/:id/stream</code></td><td>Stream SSE de progreso en vivo</td></tr>
  <tr><td><code>POST</code></td><td><code>/api/jobs/:id/cancel</code></td><td>Cancelar un trabajo en ejecución</td></tr>
  <tr><td><code>GET</code></td><td><code>/api/files</code></td><td>Listar archivos descargados</td></tr>
  <tr><td><code>GET</code></td><td><code>/api/files/*path</code></td><td>Descargar un archivo escrapeado</td></tr>
  <tr><td><code>POST</code></td><td><code>/api/deep/scrape</code></td><td>Extracción Deep Research (requiere Python sidecar)</td></tr>
  <tr><td><code>GET</code></td><td><code>/api/deep/results</code></td><td>Listar resultados de deep research</td></tr>
  <tr><td><code>GET</code></td><td><code>/api/deep/results/:id</code></td><td>Obtener un resultado deep específico</td></tr>
</table>

<h3><code>scraper/</code> — Motor de Crawling</h3>

<h4><code>anti_block.rs</code></h4>
<p>Sistema de evasión anti-bloqueo:</p>
<ul>
  <li>15 User-Agent reales (Chrome, Firefox, Safari, Edge, Opera, Vivaldi, móvil)</li>
  <li>Aleatorización de cabeceras: variantes de Accept, Accept-Language (en/es/de/fr/pt-BR), Sec-CH-UA Chrome (120–126), Sec-CH-UA-Platform, cabeceras Sec-Fetch-*</li>
  <li><code>random_user_agent()</code> + <code>random_headers()</code> — aleatorización por petición</li>
  <li><code>backoff_ms(attempt, base_ms)</code> — backoff exponencial: <code>base × 2^attempt + random(0..1000)</code></li>
</ul>

<h4><code>client.rs</code></h4>
<p>Wrapper de cliente HTTP basado en <code>reqwest</code>:</p>
<ul>
  <li>Timeout configurable (30s), descompresión gzip + brotli, store de cookies</li>
  <li>Soporte Basic Auth (credenciales base64)</li>
  <li>Proxy HTTP/HTTPS/SOCKS5 vía <code>reqwest::Proxy</code></li>
  <li>Rate limiting por dominio con delay configurable</li>
  <li><code>get_with_retry()</code> — reintento con backoff exponencial, manejo especial de 429/503</li>
</ul>

<h4><code>downloader.rs</code></h4>
<p>Motor de crawling BFS (~392 líneas):</p>
<ul>
  <li>Parsea URL objetivo, carga <code>robots.txt</code>, opcionalmente lanza Chromium headless</li>
  <li>Bucle BFS: extrae URL → fetch (opcionalmente con JS renderer) → extrae enlaces → guarda archivos</li>
  <li>Canonicalización de URL: limpia fragmentos, normaliza slashes</li>
  <li>Deduplicación de contenido vía SHA-256</li>
  <li>Extracción de enlaces desde <code>a[href]</code>, <code>link[href]</code>, <code>img[src]</code>, <code>script[src]</code>, <code>source[src]</code>, <code>video[src]</code>, <code>audio[src]</code></li>
  <li>Parseo de Sitemap.xml para descubrimiento adicional de URLs</li>
  <li>Filtrado de descarga de assets por whitelist de extensiones</li>
  <li>Reescritura de URLs HTML para navegación offline (mismo dominio)</li>
  <li>Captura de screenshots (PNG)</li>
  <li>Extracción de emails/teléfonos del contenido</li>
  <li>Modo deep: envía HTML al extractor Python</li>
  <li>Reporte de progreso SSE vía <code>mpsc::channel</code></li>
  <li>Notificación webhook al completar</li>
  <li>Exportación WARC + generación de index.html</li>
</ul>

<h4><code>renderer.rs</code></h4>
<p>Renderizado JS headless con Chromium vía <code>chromiumoxide</code> (Chrome DevTools Protocol):</p>
<ul>
  <li>Lanza con flags: <code>--no-sandbox</code>, <code>--disable-gpu</code>, <code>--disable-dev-shm-usage</code></li>
  <li><code>fetch_page()</code> — navega a URL, espera 3s por ejecución JS, devuelve HTML renderizado + screenshot opcional</li>
</ul>

<h4><code>extractor.rs</code></h4>
<p>Extracción de emails y teléfonos basada en regex:</p>
<ul>
  <li>Patrón email: <code>[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}</code></li>
  <li>Patrón teléfono: formato internacional</li>
  <li>Devuelve listas deduplicadas y ordenadas</li>
</ul>

<h4><code>rewriter.rs</code></h4>
<p>Reescritura de URLs para navegación offline:</p>
<ul>
  <li>Reescribe atributos <code>href</code> y <code>src</code> a rutas relativas</li>
  <li>Maneja URLs absolutas y relativas al protocolo (<code>//</code>)</li>
  <li>Solo mismo dominio</li>
  <li><code>generate_index()</code> — crea index.html con tema oscuro listando archivos descargados</li>
</ul>

<h4><code>robots.rs</code></h4>
<p>Parser mínimo de robots.txt:</p>
<ul>
  <li>Parsea directivas <code>User-agent: *</code> y <code>Disallow:</code></li>
  <li><code>is_allowed(path)</code> — verifica URL contra paths prohibidos</li>
</ul>

<h4><code>sitemap.rs</code></h4>
<p>Parser de Sitemap.xml:</p>
<ul>
  <li>Extrae elementos <code>&lt;loc&gt;</code></li>
  <li>Devuelve URLs descubiertas para seed de la cola de crawling</li>
</ul>

<h4><code>warc.rs</code></h4>
<p>Soporte de formato de archivo WARC:</p>
<ul>
  <li>Struct <code>WarcRecord</code>: URI objetivo, fecha, tipo de contenido, cuerpo</li>
  <li>Genera registros en formato WARC/1.0</li>
  <li><code>create_warc_file()</code> — concatena registros en un solo archivo</li>
</ul>

<h3><code>storage/</code> — Capa de Persistencia</h3>

<h4><code>manager.rs</code></h4>
<p>Gestor de almacenamiento de archivos:</p>
<ul>
  <li><code>save_file()</code> — crea directorios padre, escribe en <code>{DATA_DIR}/{path}</code></li>
  <li><code>read_file()</code> — lee con protección contra path traversal (<code>join_safe()</code> valida que la ruta resuelta esté dentro del directorio base)</li>
  <li><code>list_files()</code> — listado recursivo de directorios devolviendo <code>FileInfo</code> (nombre, ruta, es_dir, tamaño, modificado)</li>
</ul>

<h4><code>db.rs</code></h4>
<p>Persistencia SQLite vía <code>rusqlite</code> (bundled):</p>
<ul>
  <li>Auto-crea tablas: <code>jobs</code>, <code>deep_results</code>, <code>schedules</code></li>
  <li>CRUD para trabajos, resultados deep y schedules</li>
  <li>Exportación/importación masiva de trabajos</li>
  <li>Opcional: la app continúa sin DB si falla la inicialización</li>
</ul>

<hr>

<h2><code>frontend/</code> — SPA Angular 22</h2>

<table>
  <tr><th>Archivo</th><th>Propósito</th></tr>
  <tr>
    <td><code>package.json</code></td>
    <td>Configuración NPM. Dependencias: Angular 22 core/forms/router, RxJS, tslib (sin zone.js: zoneless). DevDeps: <code>@angular/build</code> 22, Angular CLI 22, TypeScript 6, Vitest + jsdom.</td>
  </tr>
  <tr>
    <td><code>angular.json</code></td>
    <td>Configuración CLI. Salida de build: <code>../static</code> (<code>outputHashing: none</code>). Builder: <code>@angular/build:application</code>; <code>npm start</code> en :4260 con proxy a :8060.</td>
  </tr>
  <tr>
    <td><code>proxy.conf.json</code></td>
    <td>Proxy del dev-server: <code>/api</code> → <code>http://localhost:8060</code>.</td>
  </tr>
  <tr>
    <td><code>tsconfig.json</code></td>
    <td>Configuración TypeScript. Target: ES2022, strict mode, Angular strict templates; <code>tsconfig.spec.json</code> con tipos de Vitest.</td>
  </tr>
  <tr>
    <td><code>public/fonts/</code></td>
    <td>Fuentes self-hosted woff2 (Doto, Space Grotesk, Space Mono). Sin Google Fonts en runtime.</td>
  </tr>
</table>

<h3><code>src/</code></h3>

<h4><code>index.html</code></h4>
<p>Shell HTML. Sin peticiones externas (fuentes self-hosted). Título: "Shinobi — Silent Web Scraper".</p>

<h4><code>styles.scss</code> + <code>_fonts.scss</code></h4>
<p>Tokens canónicos Nothing Design (idénticos a kabuki/tengu): tipografía, escala, espaciado, color dark/light, estado y <code>--gold</code>. Tema oscuro (default) y claro (<code>.theme-light</code>) de primera clase, jerarquía de 3 capas (Doto/Space Grotesk/Space Mono), sin sombras ni gradientes (solo el motivo dot-grid) y sin emojis. <code>_fonts.scss</code> declara los <code>@font-face</code> de los woff2 de <code>public/fonts</code>.</p>

<h4><code>app/</code></h4>

<p><strong><code>app.ts|html|scss</code></strong> — Shell raíz: branding, navegación (<code>[ DASHBOARD ] [ HISTORY ] [ SCHEDULES ] [ FILES ]</code>), indicadores de salud Rust (<code>/api/health</code>) y Python (proxy <code>/api/python/docs</code>, estado <em>unknown</em> con tooltip si no se puede clasificar), toggle de tema y footer.</p>

<p><strong><code>app.config.ts</code></strong> — Router con hash location (<code>#/history</code>, <code>#/schedules</code>, <code>#/files</code>), scroll restoration y <code>HttpClient</code>; zoneless por defecto.</p>

<p><strong><code>app.routes.ts</code></strong> — Rutas reales: <code>""</code> → dashboard, <code>history</code>, <code>schedules</code>, <code>files</code>, wildcard → dashboard.</p>

<p><strong><code>core/models.ts</code></strong> — Tipos de la API Rust (<code>ScrapeConfig</code>, <code>JobInfo</code>, <code>FileInfo</code>, <code>DeepResult</code>, <code>Schedule</code>, …) y el envelope xwa-sdk <code>Event</code>.</p>

<p><strong><code>core/api.service.ts</code></strong> — Cliente <code>HttpClient</code> para todos los endpoints (jobs, analyses, deep, schedules, database, stats) más constructores de URL (SSE, export, ZIP) y <code>requestErrorMessage</code>.</p>

<p><strong><code>core/live.service.ts</code></strong> — <code>EventSource</code> → <code>Observable&lt;XwaEvent&gt;</code>; registra listeners para los eventos SSE con nombre (analysis_started/progress/item_found/log/completed/error).</p>

<p><strong><code>core/events.ts</code></strong> — Helpers puros del protocolo: <code>parseXwaEvent</code>, <code>applyJobEvent</code> (reconstruye el snapshot JobInfo), <code>logLineFor</code>, <code>progressPercent</code>, <code>normalizeJobStatus</code>.</p>

<p><strong><code>core/theme.service.ts</code></strong> — Tema dark/light con persistencia en localStorage (<code>shinobi-theme</code>). <strong><code>core/export.service.ts</code></strong> — descargas cliente JSON/CSV/texto.</p>

<h4><code>shared/</code> y <code>features/</code></h4>
<ul>
  <li><code>shared/</code>: terminal auto-scroll, metric-card (Doto), status-badge, progress con ARIA y export-actions (JSON cliente, CSV servidor, ZIP).</li>
  <li><code>features/dashboard/</code>: stats, modos fast/deep y sub-modos single/crawl/batch/pycrawl, progreso SSE + terminal, resultados de crawl Python con ZIP, panel Database (export/import/clear) y atajos de teclado.</li>
  <li><code>features/history/</code>: jobs y deep results con búsqueda, paginación, exportaciones, cancelar/borrar y detalle JSON.</li>
  <li><code>features/schedules/</code>: listar, crear y borrar schedules (el scheduler real vive en Rust).</li>
  <li><code>features/files/</code>: explorador de archivos descargados con filtro, paginación y modal de previsualización (imagen/texto, Esc cierra).</li>
</ul>

<hr>

<h2><code>extractor/</code> — Sidecar Python</h2>

<h3><code>main.py</code> — Servidor FastAPI</h3>
<p>~274 líneas, corre en puerto 9090:</p>
<ul>
  <li><code>GET /health</code> — health check</li>
  <li><code>POST /extract</code> — pipeline de extracción de URL única (structured, NLP, metadata, headings, links, tables, images, custom selectors, emails, phones)</li>
  <li><code>POST /crawl</code> — iniciar crawl basado en httrack (hilo en background)</li>
  <li><code>GET /crawl/{id}</code> — detalles del crawl</li>
  <li><code>GET /crawl/{id}/status</code> — progreso (páginas, archivos, %, URL actual, errores, log)</li>
  <li><code>GET /crawl/{id}/results</code> — datos extraídos + ruta ZIP</li>
  <li><code>POST /crawl/{id}/cancel</code> — cancelar crawl</li>
</ul>

<h3><code>extractors/</code></h3>

<h4><code>structured.py</code></h4>
<p>~166 líneas — extracción de datos estructurados:</p>
<ul>
  <li><code>extruct</code> para JSON-LD, microdata, Open Graph, RDFa</li>
  <li>Fallback manual OG desde etiquetas <code>&lt;meta&gt;</code></li>
  <li>Extracción por selectores CSS personalizados vía BeautifulSoup</li>
  <li>Metadata: title, description, keywords, canonical URL</li>
  <li>Encabezados (h1–h6), enlaces internos/externos, tablas, imágenes</li>
</ul>

<h4><code>nlp.py</code></h4>
<p>~285 líneas — procesamiento de lenguaje natural:</p>
<ul>
  <li>Extracción de texto (limpia <code>&lt;script&gt;</code>, <code>&lt;style&gt;</code>, nav, footer, header)</li>
  <li>Resumen: basado en TF con puntuación de posición, top 5 oraciones</li>
  <li>Extracción de entidades: entidades capitalizadas por patrón + emails</li>
  <li>Palabras clave: frecuencia/densidad estilo TF-IDF + extracción de bigramas</li>
  <li>Sentimiento: basado en diccionario (listas de palabras positivas/negativas), puntuación + etiqueta</li>
  <li>Legibilidad: Flesch Reading Ease</li>
  <li>Integración spaCy NER (PERSON, ORG, GPE, DATE, MONEY)</li>
</ul>

<h4><code>crawler.py</code></h4>
<p>~285 líneas — crawling Python vía httrack:</p>
<ul>
  <li>Clase <code>CrawlJob</code>: gestiona subproceso httrack en hilo de background</li>
  <li>Profundidad configurable, max páginas, mismo dominio</li>
  <li>Parseo de progreso desde stdout de httrack</li>
  <li>Extracción de resultados desde archivos HTML descargados</li>
  <li>Creación de ZIP con todos los archivos</li>
  <li>Recolección de emails/teléfonos en todas las páginas</li>
  <li>Singleton <code>CrawlManager</code>: cola + ejecución de un solo worker</li>
</ul>

<h3><code>requirements.txt</code></h3>
<ul>
  <li><code>fastapi>=0.115.0</code>, <code>uvicorn[standard]</code>, <code>httpx</code></li>
  <li><code>extruct>=0.16.0</code> (datos estructurados)</li>
  <li><code>spacy>=3.8.0</code> + <code>en_core_web_sm</code> (NLP)</li>
  <li><code>beautifulsoup4>=4.12.0</code>, <code>lxml>=5.3.0</code>, <code>cssselect>=1.2.0</code> (parseo HTML)</li>
</ul>

<hr>

<h2><code>static/</code> — Frontend Compilado</h2>

<table>
  <tr><th>Archivo</th><th>Propósito</th></tr>
  <tr><td><code>browser/index.html</code></td><td>Shell SPA Angular compilado</td></tr>
  <tr><td><code>browser/main.js</code></td><td>Bundle Angular compilado</td></tr>
  <tr><td><code>browser/polyfills.js</code></td><td>Polyfills Zone.js</td></tr>
  <tr><td><code>browser/styles.css</code></td><td>Estilos compilados de styles.scss</td></tr>
</table>

<hr>

<h2><code>downloads/</code> — Salida de Scraping</h2>

<table>
  <tr><th>Archivo</th><th>Propósito</th></tr>
  <tr><td><code>.gitkeep</code></td><td>Placeholder para mantener el directorio en git</td></tr>
  <tr><td><code>*.json</code></td><td>Exportaciones JSON de metadatos de crawl</td></tr>
  <tr><td><code>*.zip</code></td><td>Archivos ZIP de resultados de crawl</td></tr>
  <tr><td><code>{domain}/</code></td><td>Directorios de sitios clonados</td></tr>
</table>

<hr>

<h2><code>docs/</code> — Documentación</h2>

<table>
  <tr><th>Archivo</th><th>Propósito</th></tr>
  <tr><td><code>manual.md</code></td><td>Manual completo de desarrollo/producción</td></tr>
  <tr><td><code>ui-architecture.md</code></td><td>Especificación de arquitectura frontend</td></tr>
  <tr><td><code>project-structure.md</code></td><td>Este archivo — desglose detallado del código</td></tr>
  <tr><td><code>esp/README.md</code></td><td>Traducción al español del README principal</td></tr>
</table>

<hr>

<h2>Diagrama de Arquitectura</h2>

<pre><code>                          ┌─────────────────────────────┐
                          │     Navegador (Angular 22)   │
                          │   localhost:8060             │
                          └──────────┬──────────────────┘
                                     │ HTTP / SSE
                          ┌──────────▼──────────────────┐
                          │     Backend Rust (Axum)      │
                          │     localhost:8060            │
                          │                              │
                          │  api/routes.rs               │
                          │  scraper/ (anti_block,       │
                          │    client, downloader,       │
                          │    renderer, extractor,      │
                          │    rewriter, robots,         │
                          │    sitemap, warc)            │
                          │  storage/ (manager, db)      │
                          └──────────┬──────────────────┘
                                     │ HTTP (JSON)
                          ┌──────────▼──────────────────┐
                          │  Extractor Python (FastAPI)  │
                          │  localhost:9090              │
                          │                              │
                          │  extractors/structured.py    │
                          │  extractors/nlp.py           │
                          │  extractors/crawler.py       │
                          └─────────────────────────────┘</code></pre>
