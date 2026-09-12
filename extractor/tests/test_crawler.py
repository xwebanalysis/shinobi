"""Crawler helpers tests: ZIP creation and httrack argument building."""

import zipfile
from pathlib import Path

from extractors.crawler import CrawlJob


def make_job(output_dir: Path, same_domain: bool = True) -> CrawlJob:
    return CrawlJob(
        job_id="test01",
        url="https://example.com",
        output_dir=str(output_dir),
        depth=2,
        max_pages=10,
        same_domain=same_domain,
    )


def test_create_zip_creates_output_dir_and_single_zip(tmp_path):
    mirror = tmp_path / "mirror" / "example.com"
    mirror.mkdir(parents=True)
    (mirror / "index.html").write_text("<html>home</html>", encoding="utf-8")
    (mirror / "style.css").write_text("body{}", encoding="utf-8")

    output_dir = tmp_path / "nested" / "out"  # does not exist yet
    job = make_job(output_dir)
    job._create_zip(mirror)

    zip_path = Path(job.zip_path)
    assert zip_path.exists()
    assert zip_path.parent == output_dir
    with zipfile.ZipFile(zip_path) as zf:
        names = set(zf.namelist())
    assert any(name.endswith("index.html") for name in names)
    assert any(name.endswith("style.css") for name in names)
    assert list(output_dir.glob("*.zip")) == [zip_path]


def test_create_zip_excludes_archives_and_logs(tmp_path):
    mirror = tmp_path / "mirror" / "example.com"
    mirror.mkdir(parents=True)
    (mirror / "page.html").write_text("ok", encoding="utf-8")
    (mirror / "old.zip").write_text("zip", encoding="utf-8")
    (mirror / "debug.log").write_text("log", encoding="utf-8")

    job = make_job(tmp_path / "out")
    job._create_zip(mirror)
    with zipfile.ZipFile(job.zip_path) as zf:
        names = zf.namelist()
    assert not any(name.endswith(".zip") for name in names)
    assert not any(name.endswith(".log") for name in names)


def test_build_args_includes_depth_and_same_domain(tmp_path):
    job = make_job(tmp_path / "out", same_domain=True)
    job.httrack_path = "httrack"
    args = job._build_args(tmp_path / "mirror")
    assert args[0] == "httrack"
    assert "-r2" in args
    assert "-d" in args

    job.same_domain = False
    assert "-d" not in job._build_args(tmp_path / "mirror")


def test_cancel_without_process_is_safe(tmp_path):
    job = make_job(tmp_path / "out")
    job.cancel()  # no process: must not raise
    assert job.status == "queued"
