"""Regex and DATA_DIR tests (no network)."""

from pathlib import Path

from main import extract_emails, extract_phones, get_data_dir


def test_phone_regex_captures_complete_number():
    phones = extract_phones("Call (555) 123-4567 or +34 612 345 678 now")
    assert "(555) 123-4567" in phones
    assert "+34 612 345 678" in phones
    # The old buggy implementation returned only the prefix group.
    assert all(not p.isdigit() or len(p) >= 7 for p in phones)


def test_phone_regex_handles_plain_and_dashed_numbers():
    phones = extract_phones("Tel: 612345678 / 555-123-4567")
    assert "612345678" in phones
    assert "555-123-4567" in phones


def test_phone_regex_rejects_short_numbers():
    assert extract_phones("Ref 12-34 and total 123456") == []


def test_email_regex_lowercases_and_deduplicates():
    emails = extract_emails("A@Example.COM a@example.com B+C@sub.example.org")
    assert emails == ["a@example.com", "b+c@sub.example.org"]


def test_data_dir_defaults_to_repo_downloads(monkeypatch):
    monkeypatch.delenv("DATA_DIR", raising=False)
    assert get_data_dir() == Path(__file__).resolve().parent.parent.parent / "downloads"


def test_data_dir_env_override(monkeypatch, tmp_path):
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    assert get_data_dir() == tmp_path
