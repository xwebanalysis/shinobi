"""Structured extraction tests using local HTML only (no network)."""

from extractors.structured import (
    extract_custom,
    extract_headings,
    extract_images,
    extract_links,
    extract_metadata,
    extract_structured,
    extract_tables,
)

HTML = """
<!DOCTYPE html>
<html>
<head>
  <title>Example — Home</title>
  <meta name="description" content="A description">
  <meta name="keywords" content="alpha, beta, gamma">
  <meta property="og:title" content="OG title">
  <link rel="canonical" href="https://example.com/">
  <script type="application/ld+json">
    {"@context": "https://schema.org", "@type": "WebSite", "name": "Example"}
  </script>
</head>
<body>
  <h1>Welcome</h1>
  <h2>Section</h2>
  <a href="/about">About us</a>
  <a href="https://external.example.org/page">External</a>
  <table><tr><th>Name</th><th>Age</th></tr><tr><td>Ada</td><td>36</td></tr></table>
  <img src="/logo.png" alt="Logo" width="100" height="50">
</body>
</html>
"""

URL = "https://example.com/"


def test_extract_structured_finds_json_ld_and_opengraph():
    data = extract_structured(HTML, URL)
    assert "json-ld" in data
    assert data["json-ld"]["@type"] == "WebSite"
    assert "opengraph" in data


def test_extract_metadata_and_headings():
    meta = extract_metadata(HTML, URL)
    assert meta["title"] == "Example — Home"
    assert meta["description"] == "A description"
    assert meta["keywords"] == ["alpha", "beta", "gamma"]
    assert meta["canonical"] == "https://example.com/"

    headings = extract_headings(HTML)
    assert headings["h1"] == ["Welcome"]
    assert headings["h2"] == ["Section"]


def test_extract_links_classifies_internal_and_external():
    links = extract_links(HTML, URL)
    assert links["internal_count"] == 1
    assert links["external_count"] == 1
    assert links["internal"][0]["href"] == "https://example.com/about"
    assert links["external"][0]["href"] == "https://external.example.org/page"


def test_extract_tables_and_images():
    tables = extract_tables(HTML)
    assert tables[0]["headers"] == ["Name", "Age"]
    assert ["Ada", "36"] in tables[0]["rows"]

    images = extract_images(HTML, URL)
    assert images[0]["src"] == "https://example.com/logo.png"
    assert images[0]["alt"] == "Logo"


def test_extract_custom_selectors():
    result = extract_custom(HTML, ["h1", "a"])
    assert result["h1"] == ["Welcome"]
    assert "About us" in result["a"]


def test_extract_structured_tolerates_broken_html():
    data = extract_structured("<html><body><h1>only heading", URL)
    assert isinstance(data, dict)
