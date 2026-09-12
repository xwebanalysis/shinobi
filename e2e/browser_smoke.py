#!/usr/bin/env python3
"""Shinobi browser smoke test (Playwright, headless Chromium).

Validates the *actual* zoneless rendering behaviour of the Angular UI served by
the Rust backend, from a real browser:

  1. Health dots + dashboard stats render with no user interaction.
  2. A "fast single" scrape against a local fixture (port 8106) is launched
     from the UI and progress / terminal / counters advance *without any click*
     until the job reaches `completed`.
  3. The job shows up in History without a page reload.
  4. A downloaded file can be previewed from Files.
  5. A schedule can be created and appears in the list.
  6. Per-job JSON (client-side) and CSV (server-side) exports download.
  7. `/api/jobs/{id}/stream` (SSE) is consumed correctly and the browser console
     stays free of errors/page errors.

Usage (backends and extractor already running, see e2e/README.md):

    /home/x/Documents/xwebanalysis/samurai/backend/.venv/bin/python \
        e2e/browser_smoke.py

Options:
    --backend URL        UI/API origin (default http://127.0.0.1:8060)
    --fixture-port N     Fixture HTTP port (default 8106)
    --no-fixture         Do not start the fixture server (reuse an existing one)
    --headed             Run Chromium with a visible window
    --timeout SECONDS    Global scrape timeout (default 120)
"""

from __future__ import annotations

import argparse
import contextlib
import socket
import subprocess
import sys
import time
from pathlib import Path

try:
    from playwright.sync_api import Page, sync_playwright
except ImportError:  # pragma: no cover - operator feedback
    print("Playwright is missing. Run with the XWA venv python:")
    print("  /home/x/Documents/xwebanalysis/samurai/backend/.venv/bin/python e2e/browser_smoke.py")
    sys.exit(2)

FIXTURE_DIR = Path(__file__).resolve().parent / "fixture"
DEFAULT_BACKEND = "http://127.0.0.1:8060"
DEFAULT_FIXTURE_PORT = 8106


def port_open(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.3)
        return sock.connect_ex(("127.0.0.1", port)) == 0


@contextlib.contextmanager
def fixture_server(port: int, enabled: bool):
    """Serve e2e/fixture on 127.0.0.1:<port> for the duration of the test."""
    process: subprocess.Popen | None = None
    if not enabled:
        yield
        return
    if port_open(port):
        print(f"[fixture] port {port} already in use, reusing existing server")
        yield
        return
    process = subprocess.Popen(
        [sys.executable, "-m", "http.server", str(port), "--bind", "127.0.0.1"],
        cwd=str(FIXTURE_DIR),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(50):
            if port_open(port):
                break
            time.sleep(0.1)
        if not port_open(port):
            raise RuntimeError(f"fixture server did not start on port {port}")
        print(f"[fixture] serving {FIXTURE_DIR} on http://127.0.0.1:{port}")
        yield
    finally:
        if process is not None:
            process.terminate()
            with contextlib.suppress(Exception):
                process.wait(timeout=5)


class Harness:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.passes = 0
        self._section = ""

    def section(self, name: str) -> None:
        self._section = name
        print(f"\n=== {name} ===")

    def check(self, condition: bool, label: str, detail: str = "") -> bool:
        if condition:
            self.passes += 1
            print(f"  [PASS] {label}")
        else:
            self.failures.append(f"{self._section}: {label} {detail}".strip())
            print(f"  [FAIL] {label} {detail}".rstrip())
        return bool(condition)

    def summary(self) -> int:
        print("\n------------------------------------------------------------")
        if self.failures:
            print(f"RESULT: FAIL ({len(self.failures)} failure(s), {self.passes} pass(es))")
            for failure in self.failures:
                print(f"  - {failure}")
            return 1
        print(f"RESULT: PASS ({self.passes} check(s))")
        return 0


def wait_for_health_dots(page: Page, timeout_s: float = 20.0) -> list[str]:
    page.wait_for_function(
        "() => document.querySelectorAll('.topbar .health .dot').length === 2",
        timeout=int(timeout_s * 1000),
    )
    deadline = time.time() + timeout_s
    classes: list[str] = []
    while time.time() < deadline:
        classes = page.eval_on_selector_all(
            ".topbar .health .dot", "els => els.map(e => e.className)"
        )
        if all("online" in value for value in classes):
            return classes
        time.sleep(0.25)
    return classes


def job_snapshot(page: Page) -> dict | None:
    """Reads the PROGRESS panel without dispatching any DOM event."""
    return page.evaluate(
        """() => {
            const panel = [...document.querySelectorAll('section.panel')]
                .find(s => s.querySelector('h2')?.textContent.trim() === 'PROGRESS');
            if (!panel) return null;
            const bar = panel.querySelector('[role=progressbar]');
            const rows = [...panel.querySelectorAll('.detail-row')];
            const pages = rows.find(r => r.querySelector('.t-label')?.textContent.trim() === 'PAGES');
            return {
                progress: bar ? Number(bar.getAttribute('aria-valuenow')) : null,
                status: (panel.querySelector('app-status-badge .badge')?.textContent || '').trim(),
                lines: panel.querySelectorAll('.terminal-line').length,
                pages: (pages?.querySelector('.t-data')?.textContent || '').trim(),
                current: (panel.querySelector('.current-url')?.textContent || '').trim(),
            };
        }"""
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", default=DEFAULT_BACKEND)
    parser.add_argument("--fixture-port", type=int, default=DEFAULT_FIXTURE_PORT)
    parser.add_argument("--no-fixture", action="store_true")
    parser.add_argument("--headed", action="store_true")
    parser.add_argument("--timeout", type=float, default=120.0)
    args = parser.parse_args()

    backend = args.backend.rstrip("/")
    fixture_url = f"http://127.0.0.1:{args.fixture_port}/"
    harness = Harness()

    with fixture_server(args.fixture_port, not args.no_fixture):
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(headless=not args.headed)
            context = browser.new_context(accept_downloads=True)
            page = context.new_page()

            console_errors: list[str] = []
            page_errors: list[str] = []
            stream_responses: list[tuple[str, int, str]] = []
            page.on(
                "console",
                lambda message: console_errors.append(f"{message.type}: {message.text}")
                if message.type == "error"
                else None,
            )
            page.on("pageerror", lambda error: page_errors.append(str(error)))
            page.on(
                "response",
                lambda response: stream_responses.append(
                    (
                        response.url,
                        response.status,
                        response.headers.get("content-type", ""),
                    )
                )
                if "/stream" in response.url
                else None,
            )
            page.on("dialog", lambda dialog: dialog.accept())

            try:
                # ── 1. dashboard, health + stats with no interaction ────────
                harness.section("dashboard initial render (no clicks)")
                page.goto(f"{backend}/#/", wait_until="networkidle")
                dots = wait_for_health_dots(page)
                harness.check(
                    len(dots) == 2 and all("online" in cls for cls in dots),
                    "RUST and PYTHON health dots turn online without interaction",
                    f"(dots={dots})",
                )
                page.wait_for_function(
                    "() => document.querySelectorAll('app-metric-card .metric-value').length >= 6",
                    timeout=15000,
                )
                metrics = page.eval_on_selector_all(
                    "app-metric-card .metric-value", "els => els.map(e => e.textContent.trim())"
                )
                harness.check(
                    len(metrics) >= 6,
                    "stats and disk metrics render without interaction",
                    f"(values={metrics})",
                )

                # ── 2. fast single scrape launched from the UI ───────────────
                harness.section("fast single scrape (autonomous progress)")
                page.fill("#fast-url", fixture_url)
                page.fill("#fast-pages", "10")
                page.fill("#fast-delay", "200")
                page.click("button.btn-primary:has-text('START SCAN')")
                page.wait_for_selector("section.panel:has(h2:text-is('PROGRESS'))", timeout=20000)

                samples: list[dict] = []
                terminal_advanced = False
                progress_advanced = False
                reached_completed = False
                deadline = time.time() + args.timeout
                previous_lines = 0
                previous_progress = 0
                while time.time() < deadline:
                    snapshot = job_snapshot(page)
                    if snapshot:
                        samples.append(snapshot)
                        terminal_advanced = terminal_advanced or snapshot["lines"] > previous_lines
                        progress_advanced = progress_advanced or (
                            snapshot["progress"] is not None
                            and snapshot["progress"] > previous_progress
                        )
                        previous_lines = max(previous_lines, snapshot["lines"])
                        previous_progress = snapshot["progress"] or 0
                        if "completed" in snapshot["status"].lower():
                            reached_completed = True
                            break
                    time.sleep(0.25)

                harness.check(reached_completed, "job reaches [ completed ] on its own")
                harness.check(
                    terminal_advanced,
                    "terminal log grows without any click",
                    f"(final_lines={previous_lines})",
                )
                harness.check(
                    progress_advanced and previous_progress > 0,
                    "progress bar advances without any click",
                    f"(final_progress={previous_progress})",
                )
                last_pages = page.eval_on_selector(
                    "section.panel:has(h2:text-is('PROGRESS')) .detail-row:nth-child(2) .t-data",
                    "el => el.textContent.trim()",
                )
                harness.check(
                    last_pages and not last_pages.startswith("0 /"),
                    "page counter advanced",
                    f"(pages={last_pages})",
                )
                harness.check(
                    len(samples) >= 3,
                    "multiple independent samples observed",
                    f"(samples={len(samples)})",
                )
                step = max(1, len(samples) // 6)
                trace = [(s["status"], s["lines"], s["progress"]) for s in samples[::step]]
                if samples:
                    trace.append((samples[-1]["status"], samples[-1]["lines"], samples[-1]["progress"]))
                print(f"  [trace] (status, terminal_lines, progress%)={trace}")
                stream_ok = any(
                    status == 200 and "text/event-stream" in content_type
                    for _, status, content_type in stream_responses
                )
                harness.check(
                    stream_ok,
                    "GET /api/jobs/{id}/stream consumed as text/event-stream",
                    f"(responses={stream_responses[-2:]})",
                )

                # ── 3. History shows the job without a reload ────────────────
                harness.section("history list + exports")
                page.click("a[href='#/history']")
                page.wait_for_selector(".entry", timeout=15000)
                page.wait_for_function(
                    "(url) => [...document.querySelectorAll('.entry-url')].some(e => e.textContent.includes(url))",
                    arg=fixture_url,
                    timeout=15000,
                )
                entries = page.locator(".entry").count()
                harness.check(entries >= 1, "job appears in History with no page reload")
                first_entry = page.locator(".entry").first
                export_actions = first_entry.locator("app-export-actions")

                with page.expect_download(timeout=20000) as json_download_info:
                    export_actions.locator("button.export-btn", has_text="JSON").click()
                json_download = json_download_info.value
                json_body = ""
                with contextlib.suppress(Exception):
                    json_body = Path(json_download.path()).read_text(encoding="utf-8")
                harness.check(
                    json_download.suggested_filename.endswith(".json") and json_body.strip().startswith("{"),
                    "client-side JSON export downloads",
                    f"({json_download.suggested_filename})",
                )

                # The busy button must flip back without any interaction (fix
                # for the zoneless flat-property mutation bug).
                json_button = export_actions.locator("button.export-btn", has_text="JSON").first
                deadline = time.time() + 4
                restored = False
                while time.time() < deadline:
                    label = (json_button.text_content() or "").strip()
                    if label == "JSON" and not json_button.is_disabled():
                        restored = True
                        break
                    time.sleep(0.1)
                harness.check(restored, "JSON button resets to [ JSON ] after the async export")

                with page.expect_download(timeout=20000) as csv_download_info:
                    export_actions.locator("a.export-btn", has_text="CSV").click()
                csv_download = csv_download_info.value
                csv_body = ""
                with contextlib.suppress(Exception):
                    csv_body = Path(csv_download.path()).read_text(encoding="utf-8")
                harness.check(
                    csv_download.suggested_filename.endswith(".csv") and len(csv_body) > 0,
                    "server-side CSV export downloads",
                    f"({csv_download.suggested_filename})",
                )

                # ── 4. files browser + preview ───────────────────────────────
                harness.section("files preview")
                page.click("a[href='#/files']")
                page.wait_for_selector(".file-item", timeout=15000)
                page.locator(".file-item", has_text="index.html").first.click()
                page.wait_for_selector(".preview-modal", timeout=10000)
                preview = page.locator(".preview-text")
                preview.wait_for(timeout=10000)
                harness.check(
                    "Fixture Home" in (preview.text_content() or ""),
                    "downloaded index.html renders in the preview modal",
                )
                page.keyboard.press("Escape")
                page.wait_for_selector(".preview-modal", state="detached", timeout=5000)
                harness.check(True, "Escape closes the preview modal")

                # ── 5. schedules create/list ─────────────────────────────────
                harness.section("schedules")
                page.click("a[href='#/schedules']")
                page.wait_for_selector("#schedule-url", timeout=10000)
                page.fill("#schedule-url", fixture_url)
                page.fill("#schedule-interval", "60")
                page.click("button.btn-primary:has-text('ADD')")
                schedule_entry = page.locator(".entry", has_text=fixture_url).first
                schedule_entry.wait_for(timeout=15000)
                harness.check(True, "created schedule appears in the list without reload")
                page.locator(".entry", has_text=fixture_url).first.locator(
                    "button:has-text('DELETE')"
                ).click()
                page.wait_for_function(
                    "(url) => ![...document.querySelectorAll('.entry-url')].some(e => e.textContent.includes(url))",
                    arg=fixture_url,
                    timeout=10000,
                )
                harness.check(True, "schedule deleted again after the check")

                # ── 6. console cleanliness ───────────────────────────────────
                harness.section("console / page errors")
                harness.check(not console_errors, "no console errors", f"({console_errors})")
                harness.check(not page_errors, "no page errors", f"({page_errors})")
            finally:
                context.close()
                browser.close()

    return harness.summary()


if __name__ == "__main__":
    sys.exit(main())
