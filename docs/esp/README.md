
<h1 align="center">Shinobi</h1>

<div align="center">
<p><em>Web scraper silencioso — Sistema de descarga con anti-bloqueo</em></p>
</div>

> **Idioma:** Esta es la versión en español de la documentación. Para la versión en inglés, consulta [README.md](../../README.md).

<p><em><a href="https://github.com/xwebanalysis/meta">XWA</a>  <strong>submódulo enfocado</strong> en scraping web sigiloso con anti-bloqueo — en desarrollo activo</em></p>

<hr>

<p>
<a href="#resumen">Resumen</a> ·
<a href="#capacidades">Capacidades</a> ·
<a href="#sistema-anti-bloqueo">Anti-Bloqueo</a> ·
<a href="#inicio-rapido">Inicio Rápido</a> ·
<a href="#con-docker">Docker</a> ·
<a href="#sin-docker-independiente">Standalone</a> ·
<a href="#variables-de-entorno">Variables</a> ·
<a href="#documentos-relacionados">Docs</a>
</p>

<hr>

<h2>Resumen</h2>

<p>Shinobi es un web scraper sigiloso con una interfaz web única. Descarga sitios completos — HTML, CSS, JS, imágenes, PDFs — mientras evade la detección mediante múltiples capas de anti-bloqueo.</p>

<table>
  <tr>
    <th>Interfaz</th>
    <th>Directorio</th>
    <th>Lenguaje</th>
    <th>Tipo</th>
  </tr>
  <tr>
    <td><strong>Shinobi Web</strong></td>
    <td><code>/</code> (raíz del monorepo)</td>
    <td>Rust (Axum 0.8) + Angular 22</td>
    <td>Aplicación web (independiente o Docker)</td>
  </tr>
</table>

<h3>Capacidades</h3>
<ul>
  <li><strong>Crawling Recursivo</strong> — Descubrimiento BFS con profundidad y límite de páginas configurables</li>
  <li><strong>Descarga de Activos</strong> — HTML, CSS, JS, imágenes, PDFs, archivos, multimedia, fuentes</li>
  <li><strong>Filtro por Tipo de Archivo</strong> — Selecciona qué extensiones descargar</li>
  <li><strong>Restricción de Dominio</strong> — Limita el crawling al dominio objetivo o explora libremente</li>
  <li><strong>Progreso en Tiempo Real</strong> — Stream SSE con eventos <code>Event</code> de xwa-sdk</li>
  <li><strong>Scheduler</strong> — Scrapes recurrentes para schedules vencidos</li>
  <li><strong>Persistencia SQLite</strong> — WAL, migraciones con <code>schema_meta</code> y export/import completo</li>
  <li><strong>Explorador de Archivos</strong> — Navega y abre archivos descargados desde la interfaz web</li>
</ul>
<p><small>Frontend migrado a Angular 22 + TypeScript 6 + <code>@angular/build</code> + Vitest con tokens Nothing Design y fuentes self-hosted (Fase 5).</small></p>

<h3>Sistema Anti-Bloqueo</h3>
<ul>
  <li><strong>Rotación de User-Agent</strong> — 15 UAs reales (Chrome, Firefox, Safari, Edge, móvil)</li>
  <li><strong>Aleatorización de Cabeceras</strong> — Accept, Accept-Language, Sec-CH-UA, Sec-Fetch-* por petición</li>
  <li><strong>Delay + Jitter</strong> — Retardo base configurable con jitter aleatorio</li>
  <li><strong>Backoff Exponencial</strong> — Reintento con jitter en fallos (intentos configurables)</li>
  <li><strong>Manejo de Rate Limit</strong> — Detecta HTTP 429/503, espera y reintenta con backoff más largo</li>
  <li><strong>Soporte de Proxies</strong> — Rotación round-robin real (avanza en fallo o 429/503)</li>
  <li><strong>robots.txt</strong> — <code>Allow</code>/<code>Disallow</code> (coincidencia más larga) y <code>Crawl-delay</code></li>
  <li><strong>Concurrencia acotada</strong> — Máximo duro de 3 peticiones simultáneas</li>
</ul>

<hr>

<h2>Inicio Rápido</h2>

<h3>Con Docker</h3>
<pre><code>./shinobi.sh docker</code></pre>
<ul>
  <li>Interfaz web / API: <code>http://localhost:8060</code></li>
  <li>El volumen <code>shinobi-data</code> persiste la BD y las descargas</li>
</ul>

<h3>Sin Docker (Independiente)</h3>
<pre><code>./shinobi.sh            # backend :8060 + extractor :9090
cargo run --release     # solo backend</code></pre>
<p>Escucha en <code>http://localhost:8060</code>. BD en <code>./shinobi.db</code> y descargas en <code>./downloads/</code>.</p>

<h3>Variables de Entorno</h3>
<table>
  <tr><th>Variable</th><th>Por Defecto</th><th>Descripción</th></tr>
  <tr><td><code>PORT</code></td><td><code>8060</code></td><td>Puerto HTTP</td></tr>
  <tr><td><code>SHINOBI_DB_PATH</code></td><td><code>shinobi.db</code></td><td>Ruta de la BD SQLite</td></tr>
  <tr><td><code>DATA_DIR</code></td><td><code>downloads</code></td><td>Directorio de descargas (compartido con el extractor)</td></tr>
  <tr><td><code>RUST_LOG</code></td><td><code>shinobi=info,tower_http=info</code></td><td>Verbosidad de logs</td></tr>
</table>

<hr>

<h2>Documentos Relacionados</h2>

<table>
  <tr><th>Documento</th><th>Descripción</th></tr>
  <tr><td><a href="../manual.md">manual.md</a></td><td>Guía de despliegue de desarrollo y producción</td></tr>
  <tr><td><a href="../ui-architecture.md">ui-architecture.md</a></td><td>Especificación de arquitectura frontend</td></tr>
  <tr><td><a href="../../ROADMAP.md">ROADMAP.md</a></td><td>Fases de desarrollo e hitos</td></tr>
  <tr><td><a href="project-structure.md">project-structure.md</a></td><td>Estructura detallada del código con referencia de API</td></tr>
</table>

<hr>

<div id="x" align="center">
<h2>X</h2>

<a href="https://dev.xscriptor.com">
  <img src="https://xscriptor.github.io/icons/icons/code/product-design/xsvg/verified-filled.svg" width="24" alt="X Web" />
</a>
 & 
<a href="https://github.com/xscriptor">
  <img src="https://xscriptor.github.io/icons/icons/code/product-design/xsvg/github.svg" width="24" alt="Perfil de X en Github" />
</a>
 & 
<a href="https://www.xscriptor.com">
  <img src="https://xscriptor.github.io/icons/icons/code/product-design/xsvg/quotes.svg" width="24" alt="Sitio web de Xscriptor" />
</a>

</div>
