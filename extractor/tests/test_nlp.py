"""NLP tests exercising the rule-based path (spaCy model optional)."""

from extractors.nlp import (
    analyze,
    extract_entities,
    extract_keywords,
    readability,
    sentiment_analysis,
    summarize,
)

HTML = """
<html><body>
<h1>Great product</h1>
<p>The product is excellent and the support team is helpful. It works fast and is easy to use.
However the documentation is confusing and sometimes slow.</p>
<p>Contact ada.lovelace@example.com for more information.</p>
</body></html>
"""


def test_analyze_returns_expected_sections():
    result = analyze(HTML, "https://example.com")
    assert "summary" in result
    assert "keywords" in result
    assert "sentiment" in result
    assert "readability" in result
    assert result["text_stats"]["word_count"] > 0
    assert result["method"] in {"rule-based", "spacy"}


def test_summarize_returns_sentences():
    text = "First sentence about the topic. Second sentence with more details. Third one here."
    summary = summarize(text, 2)
    assert 1 <= len(summary) <= 2
    assert all(len(s) > 15 for s in summary)


def test_keywords_and_bigrams_shape():
    result = extract_keywords("rust rust rust web web crawler crawler crawler data", 5)
    assert result["total_words"] > 0
    assert result["keywords"][0]["word"] == "rust"
    assert isinstance(result["bigrams"], list)


def test_sentiment_classifies_positive_and_negative():
    assert sentiment_analysis("excellent amazing wonderful great")["label"] == "positive"
    assert sentiment_analysis("terrible awful horrible failure")["label"] == "negative"


def test_readability_has_bounded_score():
    result = readability("This is a simple sentence. It has a few words only.")
    assert 0 <= result["flesch_score"] <= 100
    assert result["words"] > 0
    assert result["sentences"] >= 1


def test_entities_extracts_email_names():
    entities = extract_entities("Write to ada.lovelace@example.com today")
    assert "Ada Lovelace" in entities["people"]
