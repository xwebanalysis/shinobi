# Shinobi — E2E browser smoke test

`browser_smoke.py` drives the Angular UI served by the Rust backend with
Playwright (headless Chromium). It exists to catch the **zoneless rendering
bugs** that unit tests cannot: the script never clicks while it watches the
scrape advance, so any stuck `[ LOADING... ]`, frozen progress bar or late list
makes the run fail.

## What it validates

1. **Dashboard without clicks** — `RUST` and `PYTHON` health dots turn green and
   the stat/disk cards render on their own.
2. **Fast scrape from the UI** (`POST /api/scrape` + SSE) against the local
   fixture (`http://127.0.0.1:8106/`): progress bar, page counter and terminal
   log advance **without any user interaction** and the job reaches
   `[ completed ]`.
3. **SSE** — `GET /api/jobs/{id}/stream` is consumed with
   `Content-Type: text/event-stream`.
4. **History** — the job appears without a page reload; the client-side JSON
   export downloads and the button returns to `JSON` on its own (regression
   test for the flat-property `busy` mutation); the server-side CSV export
   downloads.
5. **Files** — a downloaded `index.html` opens in the preview modal and
   `Escape` closes it.
6. **Schedules** — a schedule is created, listed without reload and deleted.
7. **Console hygiene** — the run fails on any browser console `error` or
   `pageerror`.

The fixture lives in `e2e/fixture/` (linked pages, CSS/JS/PNG assets, robots.txt
and sitemap) and is served automatically on port 8106 by the script.

## Requirements

- Node 24 (mise) to rebuild the UI.
- Python with Playwright + Chromium. The XWA venv already has it:
  `/home/x/Documents/xwebanalysis/samurai/backend/.venv/bin/python`.
- The backend and the Python extractor running locally.

## Run

```bash
# 1. Build the UI that the Rust server serves (static/browser)
export PATH="$HOME/.local/share/mise/installs/node/24/bin:$PATH"
cd frontend && npm ci && npm run build

# 2. Start backend (:8060) + extractor (:9090)
cd .. && ./shinobi.sh local

# 3. In another terminal: run the smoke test
/home/x/Documents/xwebanalysis/samurai/backend/.venv/bin/python \
    e2e/browser_smoke.py
```

Useful flags:

```bash
e2e/browser_smoke.py --headed          # visible Chromium
e2e/browser_smoke.py --no-fixture      # reuse an existing fixture server
e2e/browser_smoke.py --backend http://127.0.0.1:8060 --fixture-port 8106
```

The script exits `0` on success and `1` on the first failed expectation,
printing a textual `[PASS]/[FAIL]` report. It terminates the fixture server it
started; stop the backend with `Ctrl+C` when done.

## Notes

- The scraper first probes `robots.txt` on the origin without the port (backend
  behaviour, out of frontend scope), so a crawl may spend ~10 s before the
  first page. The script's default timeout is 120 s.
- If the extractor is not running the `PYTHON` dot is `unknown` and step 1
  fails: always use `./shinobi.sh local` (not `-f`).
