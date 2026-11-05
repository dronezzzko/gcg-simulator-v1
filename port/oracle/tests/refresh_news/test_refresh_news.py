"""W-153: news discovery of the refresh skill against recorded responses (T-155).

``fetch_official.py`` pages through the US and Asia all-news listings until an entry older than
the cutoff appears, fails loudly when the page cap runs out first, fetches every uncached entry
since the cutoff, and flags an entry whose text names a card number or says errata. The US
listing's page 1 is the recorded ``data/official_raw/en_news_index.html`` (read only); pages 2-4,
the Asia pages and the entry pages are trimmed fixtures in ``fixtures/`` with the same markup.
No test touches the network.

    uv run pytest port/oracle/tests/refresh_news -q
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path
from types import ModuleType
from typing import Any

import pytest

REPO = Path(__file__).resolve().parents[4]
SCRIPTS = REPO / ".claude" / "skills" / "gcg-refresh-data" / "scripts"
FIXTURES = Path(__file__).resolve().parent / "fixtures"
PAGE_1 = REPO / "data" / "official_raw" / "en_news_index.html"
CUTOFF = "2026-08-20"
EN = "https://www.gundam-gcg.com/en/"
ASIA = "https://www.gundam-gcg.com/asia-en/"


def _load() -> ModuleType:
    sys.path.insert(0, str(SCRIPTS))
    spec = importlib.util.spec_from_file_location("fetch_official", SCRIPTS / "fetch_official.py")
    assert spec is not None
    assert spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


fo = _load()


def _listing_fixture(url: str) -> Path | None:
    for region, prefix in (("en", "en"), ("asia-en", "asia")):
        template: str = fo.NEWS_LISTINGS[region]
        for n in range(1, 10):
            if url == template.format(n=n):
                return FIXTURES / f"{prefix}_p{n}.html"
    return None


def _page(url: str) -> bytes | None:
    """The recorded response for ``url``, or ``None`` for a URL nothing recorded."""
    if url == EN + "news/":
        return PAGE_1.read_bytes()
    listing = _listing_fixture(url)
    if listing is not None:
        return listing.read_bytes() if listing.is_file() else None
    name = url.rsplit("/", 1)[-1].removesuffix(".html")
    entry = FIXTURES / f"entry_{name}.html"
    if entry.is_file():
        return entry.read_bytes()
    if url.startswith(("https://www.gundam-gcg.com/en/", "https://www.gundam-gcg.com/asia-en/")):
        return (FIXTURES / "entry_plain.html").read_bytes()
    return None


class Recorder:
    """A getter for ``cmd_fetch`` that serves recorded responses and logs the URLs asked for."""

    def __init__(self) -> None:
        self.urls: list[str] = []

    def __call__(self, url: str, _context: object) -> dict[str, Any]:
        self.urls.append(url)
        body = _page(url)
        if body is None:
            return {"error": f"no recorded response for {url}"}
        return {
            "status": 200,
            "final_url": url,
            "last_modified": None,
            "content_type": "text/html",
            "body": body,
        }


def _get_page(n: int, url: str) -> str:
    if n == 1 and url == fo.NEWS_LISTINGS["en"].format(n=1):
        return PAGE_1.read_text("utf-8")
    body = _page(url)
    if body is None:
        raise fo.PagingError(f"{url}: no recorded response")
    return body.decode("utf-8")


def _root(tmp_path: Path) -> Path:
    """A checkout holding only the news listing sources and one cached announcement."""
    root = tmp_path / "root"
    (root / "data" / "sources.d").mkdir(parents=True)
    sources = [
        {
            "id": "official-en-news-index",
            "url": EN + "news/",
            "kind": "official-html",
            "retrieved_at": f"{CUTOFF}T09:00:00Z",
        },
        {
            "id": "official-en-news-rules",
            "url": EN + "news/?subcategory=all&tag=RULES&page=1",
            "kind": "official-html",
            "retrieved_at": "2026-08-19T09:00:00Z",
        },
        {
            "id": "official-asia-news-rules",
            "url": ASIA + "news/?subcategory=all&tag=RULES&page=1",
            "kind": "official-html",
            "retrieved_at": "2026-08-19T09:00:00Z",
        },
        {
            "id": "official-en-bnr",
            "url": EN + "news/01_279.html",
            "kind": "official-html",
            "retrieved_at": "2026-10-15T20:00:00Z",
        },
        {
            "id": "official-asia-bnr-0710",
            "url": ASIA + "news/01_277.html",
            "kind": "official-html",
            "retrieved_at": "2026-10-15T20:00:00Z",
        },
    ]
    (root / "data" / "sources.d" / "official.json").write_text(json.dumps(sources), "utf-8")
    return root


def _fetch_args(root: Path, out: Path, cap: int, getter: Recorder) -> argparse.Namespace:
    return argparse.Namespace(
        root=root,
        out=out,
        only=None,
        url=None,
        delay=0.0,
        since=None,
        news_max_pages=cap,
        getter=getter,
    )


def test_the_recorded_listing_page_parses_with_its_pager_and_pinned_entries() -> None:
    html = PAGE_1.read_text("utf-8")
    items = fo.parse_news(html, EN + "news/")
    assert len(items) == 10
    assert fo.parse_pager(html) == (1, 14)
    assert [i["pickup"] for i in items[:3]] == [True, True, False]
    assert items[0]["date"] == "2026-10-02"
    assert items[8] == {
        "date": "2026-09-25",
        "category": "RULES",
        "tags": "RULES",
        "title": "Banned/Restricted Cards",
        "url": EN + "news/01_279.html",
        "pickup": False,
    }


def test_the_cutoff_is_the_newest_news_listing_retrieval(tmp_path: Path) -> None:
    root = _root(tmp_path)
    entries = json.loads((root / "data" / "sources.d" / "official.json").read_text("utf-8"))
    # The B&R announcement pages were retrieved later, but they are not news listings.
    assert fo.news_cutoff(entries) == CUTOFF


def test_paging_reads_page_4_when_page_3_still_ends_after_the_cutoff() -> None:
    res = fo.page_listing(_get_page, fo.NEWS_LISTINGS["en"], CUTOFF, 30)
    assert [u.rsplit("=", 1)[-1] for u in res["pages"]] == ["1", "2", "3", "4"]
    assert res["stop"] == "cutoff"
    assert res["oldest"] == "2026-08-14"
    page3 = fo.parse_news(_get_page(3, fo.NEWS_LISTINGS["en"].format(n=3)), "")
    assert min(i["date"] for i in page3) > CUTOFF


def test_an_exhausted_page_cap_is_an_error() -> None:
    with pytest.raises(fo.PagingError, match=r"3 pages read .* not older than the cutoff"):
        fo.page_listing(_get_page, fo.NEWS_LISTINGS["en"], CUTOFF, 3)


def test_a_pinned_old_entry_does_not_stop_the_paging_and_the_last_page_does() -> None:
    res = fo.page_listing(_get_page, fo.NEWS_LISTINGS["asia-en"], CUTOFF, 30)
    # Page 1 pins a July entry, older than the cutoff; paging goes on to the last page (2/2).
    assert len(res["pages"]) == 2
    assert res["stop"] == "last page"
    assert res["oldest"] == "2026-08-25"


def test_entries_are_flagged_by_their_text_not_only_by_their_title() -> None:
    item = {"category": "NEWS", "title": "Card text update notice"}
    assert fo.flag_reasons(item, None) == []
    text = (FIXTURES / "entry_02_198.html").read_bytes()
    assert fo.flag_reasons(item, fo.html_to_text(text)) == ["names GD03-015"]
    errata = fo.html_to_text((FIXTURES / "entry_03_049.html").read_bytes())
    assert fo.flag_reasons({"category": "NEWS", "title": "Notice (Asia)"}, errata) == [
        "names ST01-005",
        "says errata",
    ]
    rules = {"category": "RULES", "title": "Errata and revision in card description"}
    assert fo.flag_reasons(rules, "") == ["RULES tag", "title"]


def test_fetch_reads_every_listing_to_the_cutoff_and_every_uncached_entry(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    root = _root(tmp_path)
    out = tmp_path / "fetched"
    rec = Recorder()
    assert fo.cmd_fetch(_fetch_args(root, out, 30, rec)) == 0
    manifest = json.loads((out / "fetch_manifest.json").read_text("utf-8"))
    scan = manifest["news_scan"]
    assert scan["cutoff"] == CUTOFF
    assert scan["listings"]["en"]["stop"] == "cutoff"
    assert len(scan["listings"]["en"]["pages"]) == 4
    assert scan["listings"]["asia-en"]["stop"] == "last page"
    # US page 1 is the index source, fetched once; pages 2-4 and the Asia pages follow.
    assert rec.urls.count(EN + "news/") == 1
    assert fo.NEWS_LISTINGS["en"].format(n=1) not in rec.urls
    assert fo.NEWS_LISTINGS["en"].format(n=5) not in rec.urls
    entries = manifest["news_entries"]
    # Every uncached entry dated on or after the cutoff, flagged or not; never the cached
    # 01_279 / 01_277 announcements or the 2026-08-14 entry before the cutoff.
    assert EN + "news/02_194.html" in entries  # dated on the cutoff
    assert EN + "news/02_190.html" not in entries
    assert EN + "news/01_279.html" not in entries
    assert ASIA + "news/01_277.html" not in entries
    assert len(entries) == 9 + 4 + 3 + 1 + 2 + 1
    assert all("error" not in e and e["text_file"].startswith("news/") for e in entries.values())
    capsys.readouterr()
    assert fo.cmd_news(argparse.Namespace(root=root, fetched=out, since=None)) == 0
    lines = capsys.readouterr().out.splitlines()
    relevant = [ln for ln in lines if ln.startswith("RELEVANT")]
    assert len(relevant) == 3, relevant
    assert any("02_199.html  [title; names GD02-053; says errata]" in ln for ln in relevant)
    assert any("02_198.html  [names GD03-015]" in ln for ln in relevant)
    assert any("03_049.html  [names ST01-005; says errata]" in ln for ln in relevant)
    assert any(ln.startswith("news en: 4 page(s), stopped at the cutoff") for ln in lines)


def test_an_exhausted_cap_fails_fetch_and_news(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    root = _root(tmp_path)
    out = tmp_path / "fetched"
    assert fo.cmd_fetch(_fetch_args(root, out, 3, Recorder())) == 1
    manifest = json.loads((out / "fetch_manifest.json").read_text("utf-8"))
    assert "error" in manifest["news_scan"]["listings"]["en"]
    assert manifest["news_scan"]["listings"]["asia-en"]["stop"] == "last page"
    capsys.readouterr()
    assert fo.cmd_news(argparse.Namespace(root=root, fetched=out, since=None)) == 1
    captured = capsys.readouterr()
    assert "WARNING: news en not read to the cutoff" in captured.out
    assert "INCOMPLETE" in captured.err


def test_a_fetch_without_a_news_scan_is_reported_as_incomplete(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    root = _root(tmp_path)
    out = tmp_path / "fetched"
    out.mkdir()
    (out / "fetch_manifest.json").write_text(json.dumps({"sources": {}}), "utf-8")
    assert fo.cmd_news(argparse.Namespace(root=root, fetched=out, since=None)) == 1
    assert "has no news scan" in capsys.readouterr().err
