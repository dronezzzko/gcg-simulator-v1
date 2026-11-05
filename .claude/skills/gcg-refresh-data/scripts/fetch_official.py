#!/usr/bin/env python3
"""Fetch the official English Gundam Card Game pages and compare them with the cached copies.

The page list is every entry of ``data/sources.d/official.json`` (US and Asia rules pages, B&R
announcements, BO3/language/deck-building pages, errata notices, rules PDFs, news listings) plus
the pages of the US and Asia all-news listings back to the news cutoff. Requests go one at a
time with a pause and a descriptive User-Agent; nothing in the repository is modified.

    uv run --no-project --with pypdf --with certifi python fetch_official.py fetch --out DIR \
        [--since YYYY-MM-DD] [--news-max-pages N]
    uv run --no-project python fetch_official.py compare --fetched DIR [--json OUT]
    uv run --no-project python fetch_official.py news --fetched DIR [--since YYYY-MM-DD]

``fetch`` writes ``<slug>.html|pdf``, a ``<slug>.txt`` extraction (PDF text needs pypdf, hence
``--with pypdf``) and ``fetch_manifest.json``. TLS is verified against certifi's CA bundle
(``--with certifi``) when it is installed, otherwise against the system default. ``compare``
reports, per page, whether the bytes or at least the extracted text are unchanged, shows text
diffs, and checks the Comprehensive Rules version and the banned/restricted links. ``news`` lists
news entries that are not cached yet (possible new B&R lists, errata or rule updates).

News discovery (W-153). The cutoff is ``--since``, by default the newest ``retrieved_at`` of the
news listing sources in ``official.json`` (the URLs whose path ends in ``/news/``). ``fetch``
pages through each all-news listing (``NEWS_LISTINGS``) until a page holds an entry dated
before the cutoff, or the listing's last page; pinned ("pickup") entries are kept but do not
stop the paging, because they are out of date order. A listing still newer than the cutoff
after ``--news-max-pages`` pages is an error (exit 1): older entries would go unread. Every
uncached entry dated on or after the cutoff is then fetched into ``news/``, not only the
flagged ones, and ``news`` flags an entry as RELEVANT when it is RULES-tagged, its title matches
``RELEVANT``, or its text names a card number or says errata.
"""

from __future__ import annotations

import argparse
import datetime as dt
import difflib
import hashlib
import json
import re
import ssl
import sys
import time
import urllib.error
import urllib.request
from collections.abc import Callable, Iterable
from pathlib import Path
from typing import Any
from urllib.parse import urljoin, urlparse

from html_to_text import html_to_text

REPO = Path(__file__).resolve().parents[4]
USER_AGENT = "gcg-sim data refresh (Claude Code skill gcg-refresh-data; one request at a time)"
NEWS_LISTINGS = {
    "en": "https://www.gundam-gcg.com/en/news/?subcategory=all&tag=all&page={n}",
    "asia-en": "https://www.gundam-gcg.com/asia-en/news/?subcategory=all&tag=all&page={n}",
}
NEWS_MAX_PAGES = 30
NEWS_DIR = "news"
RELEVANT = re.compile(
    r"errata|banned|restricted|rule|correction|revision|faq|comprehensive|regulation", re.I
)
CARD_NUMBER = re.compile(r"\b(?:(?:GD|ST|EB)\d{2}|EXBP|EXRP|EXR|EXB|RP|R|T)-\d{3}\b")
ERRATA = re.compile(r"\berrat(?:a|um)\b", re.I)
_NEWS_ITEM = re.compile(
    r'<div class="newsDetail([^"]*)" data-tags="([^"]*)">(.*?)cardCategory">([^<]*)<', re.S
)
_PAGER = re.compile(r'pageBtnMin">\s*(\d+)\s*/\s*(\d+)\s*<')
_ISO_DATE = re.compile(r"\d{4}-\d{2}-\d{2}")


class PagingError(RuntimeError):
    """A news listing is still newer than the cutoff after the page cap."""


def _utc_now() -> str:
    return dt.datetime.now(dt.UTC).strftime("%Y-%m-%dT%H:%M:%SZ")


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _url_slug(url: str) -> str:
    u = urlparse(url)
    path = u.path.strip("/")
    path = re.sub(r"\.(html|php|pdf)$", "", path) if path else "index"
    if u.path.endswith("/"):
        path += "_index"
    slug = path.replace("/", "_")
    if u.query:
        slug += "_" + re.sub(r"[^A-Za-z0-9]+", "-", u.query).strip("-")
    host = u.netloc.removeprefix("www.")
    if host != "gundam-gcg.com":
        slug = host.split(".")[0] + "_" + slug
    return slug


def load_sources(root: Path) -> list[dict[str, Any]]:
    """One row per distinct URL of the official fragment, with a file slug."""
    entries = json.loads((root / "data" / "sources.d" / "official.json").read_text("utf-8"))
    rows: list[dict[str, Any]] = []
    used: set[str] = set()
    seen_urls: set[str] = set()
    for e in sorted(entries, key=lambda e: str(e["id"])):
        url = str(e["url"])
        if url in seen_urls:
            continue
        seen_urls.add(url)
        kind = "pdf" if str(e.get("kind")).endswith("pdf") else "html"
        local = e.get("local_path") or ""
        region = urlparse(url).path.strip("/").split("/")[0]
        slug = Path(local).stem if local.startswith("data/official_raw/") else ""
        if (
            not slug.startswith(region + "_")
            or slug in used
            or Path(local).suffix not in (".html", ".pdf")
        ):
            slug = _url_slug(url)
        used.add(slug)
        rows.append({"id": e["id"], "url": url, "kind": kind, "slug": slug, "entry": e})
    return rows


def _tls_context() -> ssl.SSLContext:
    # macOS's /etc/ssl/cert.pem, which uv-managed Pythons read, lacks newer roots such as the
    # GlobalSign Root R46 that gundam-gcg.com chains to; certifi's bundle has them.
    try:
        import certifi
    except ImportError:
        return ssl.create_default_context()
    return ssl.create_default_context(cafile=certifi.where())


def _get(url: str, context: ssl.SSLContext) -> dict[str, Any]:
    req = urllib.request.Request(
        url, headers={"User-Agent": USER_AGENT, "Accept-Language": "en-US,en;q=0.8"}
    )
    try:
        with urllib.request.urlopen(req, timeout=60, context=context) as resp:
            body = resp.read()
            return {
                "status": resp.status,
                "final_url": resp.geturl(),
                "last_modified": resp.headers.get("Last-Modified"),
                "content_type": resp.headers.get("Content-Type"),
                "body": body,
            }
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        return {"error": str(exc)}


def _extract_pdf(path: Path, name: str) -> str | None:
    try:
        from pdf_to_text import pdf_to_text

        return pdf_to_text(path, name)
    except ImportError:
        return None


Getter = Callable[[str], dict[str, Any]]


def _save(out: Path, row: dict[str, Any], res: dict[str, Any], prefix: str = "") -> dict[str, Any]:
    """The manifest record of one fetched page; writes the page and its text into ``out``."""
    rec: dict[str, Any] = {"id": row["id"], "url": row["url"], "kind": row["kind"]}
    rec["retrieved_at"] = _utc_now()
    if "error" in res:
        rec["error"] = res["error"]
        print(f"FAILED {row['slug']}: {res['error']}", file=sys.stderr)
        return rec
    res = dict(res)
    body: bytes = res.pop("body")
    path = out / f"{row['slug']}.{row['kind']}"
    path.write_bytes(body)
    rec.update(res)
    rec.update({"file": prefix + path.name, "sha256": _sha256(body), "bytes": len(body)})
    text = html_to_text(body) if row["kind"] == "html" else _extract_pdf(path, path.name)
    if text is None:
        rec["text_file"] = None
        rec["note"] = "PDF text not extracted: run this with `uv run --no-project --with pypdf`"
    else:
        txt = out / f"{row['slug']}.txt"
        with txt.open("w", encoding="utf-8", newline="\n") as fh:
            fh.write(text)
        rec["text_file"] = prefix + txt.name
    print(f"{rec['status']} {row['slug']} {len(body):,} bytes")
    return rec


def _is_news_listing(url: str) -> bool:
    return urlparse(url).path.endswith("/news/")


def news_cutoff(entries: Iterable[dict[str, Any]]) -> str:
    """The newest ``retrieved_at`` date of the news listing sources (``YYYY-MM-DD``)."""
    dates = [
        str(e.get("retrieved_at") or "")[:10] for e in entries if _is_news_listing(str(e["url"]))
    ]
    if not any(dates):
        raise SystemExit("official.json has no news listing source with a retrieved_at date")
    return max(dates)


def parse_pager(html: str) -> tuple[int, int] | None:
    """``(page, pages)`` from a listing's ``pageBtnMin`` pager, or ``None`` without one."""
    m = _PAGER.search(html)
    return (int(m.group(1)), int(m.group(2))) if m else None


def page_listing(
    get_page: Callable[[int, str], str], template: str, cutoff: str, cap: int
) -> dict[str, Any]:
    """Reads listing pages ``1, 2, ...`` of ``template`` until a page holds an entry dated
    before ``cutoff`` (pinned entries excepted) or the listing's last page.

    ``get_page(n, url)`` returns the page's HTML. The result holds the entries in page order,
    the pages read, the stop reason and the oldest dated entry. Raises ``PagingError`` when
    ``cap`` pages were read and none reached the cutoff or the last page.
    """
    items: list[dict[str, Any]] = []
    pages: list[str] = []
    oldest: str | None = None
    for n in range(1, cap + 1):
        url = template.format(n=n)
        html = get_page(n, url)
        pages.append(url)
        found = parse_news(html, url)
        items.extend(found)
        dated = [i["date"] for i in found if not i["pickup"] and _ISO_DATE.fullmatch(i["date"])]
        if dated:
            oldest = min([*dated, oldest] if oldest else dated)
        if any(d < cutoff for d in dated):
            return {"items": items, "pages": pages, "stop": "cutoff", "oldest": oldest}
        pager = parse_pager(html)
        if not found or (pager is not None and pager[0] >= pager[1]):
            return {"items": items, "pages": pages, "stop": "last page", "oldest": oldest}
    raise PagingError(
        f"{template.format(n='N')}: {cap} pages read and the oldest entry ({oldest}) is not "
        f"older than the cutoff {cutoff}; older news would go unread. Raise --news-max-pages."
    )


def uncached_since(
    items: Iterable[dict[str, Any]], cached: set[str], cutoff: str
) -> list[dict[str, Any]]:
    """The entries dated on or after ``cutoff`` whose URL is not cached, first seen first."""
    out: dict[str, dict[str, Any]] = {}
    for item in items:
        url = item["url"].split("?")[0]
        if url and url not in cached and item["date"] >= cutoff:
            out.setdefault(url, item)
    return list(out.values())


def flag_reasons(item: dict[str, Any], text: str | None) -> list[str]:
    """Why an uncached news entry may matter for the data: empty when nothing flags it."""
    reasons = []
    if item["category"] == "RULES":
        reasons.append("RULES tag")
    if RELEVANT.search(item["title"]):
        reasons.append("title")
    if text is not None:
        numbers = sorted(set(CARD_NUMBER.findall(text)))
        if numbers:
            shown = ", ".join(numbers[:5]) + (", ..." if len(numbers) > 5 else "")
            reasons.append(f"names {shown}")
        if ERRATA.search(text):
            reasons.append("says errata")
    return reasons


def scan_news(
    get: Getter, out: Path, cutoff: str, cap: int, cached: set[str], first_pages: dict[str, str]
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], bool]:
    """Pages every all-news listing back to ``cutoff`` and fetches its uncached entries.

    ``first_pages`` maps a region to the HTML of its page 1 when that page was already fetched
    as a source. Returns the listing-page records, the scan summary per region, the entry
    records by URL, and whether everything was read.
    """
    pages: dict[str, Any] = {}
    scan: dict[str, Any] = {"cutoff": cutoff, "max_pages": cap, "listings": {}}
    ok = True
    items: list[dict[str, Any]] = []
    for region, template in NEWS_LISTINGS.items():

        def get_page(n: int, url: str, region: str = region) -> str:
            if n == 1 and region in first_pages:
                return first_pages[region]
            row = {"id": f"{region}-news-page-{n}", "url": url, "kind": "html"}
            row["slug"] = f"{region}_news_index_p{n}"
            rec = _save(out, row, get(url))
            pages[row["slug"]] = rec
            if "error" in rec:
                raise PagingError(f"{url}: {rec['error']}")
            return (out / str(rec["file"])).read_text("utf-8")

        try:
            res = page_listing(get_page, template, cutoff, cap)
        except PagingError as exc:
            print(f"NEWS PAGING FAILED ({region}): {exc}", file=sys.stderr)
            scan["listings"][region] = {"error": str(exc)}
            ok = False
            continue
        items += res["items"]
        scan["listings"][region] = {k: res[k] for k in ("pages", "stop", "oldest")}
        print(
            f"news {region}: {len(res['pages'])} listing page(s), stopped at the "
            f"{res['stop']} (oldest entry {res['oldest']}, cutoff {cutoff})"
        )
    entries: dict[str, Any] = {}
    (out / NEWS_DIR).mkdir(exist_ok=True)
    for item in uncached_since(items, cached, cutoff):
        url = item["url"].split("?")[0]
        kind = "pdf" if urlparse(url).path.endswith(".pdf") else "html"
        row = {"id": url, "url": url, "kind": kind, "slug": _url_slug(url)}
        rec = _save(out / NEWS_DIR, row, get(url), prefix=f"{NEWS_DIR}/")
        rec.update({k: item[k] for k in ("date", "category", "tags", "title")})
        entries[url] = rec
        ok = ok and "error" not in rec
    return pages, scan, entries, ok


def cmd_fetch(args: argparse.Namespace) -> int:
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    entries = json.loads((args.root / "data" / "sources.d" / "official.json").read_text("utf-8"))
    rows = load_sources(args.root)
    targeted = bool(args.only or args.url)
    if targeted:
        rows = [r for r in rows if r["slug"] in (args.only or ()) or r["id"] in (args.only or ())]
    for url in args.url or ():
        kind = "pdf" if urlparse(url).path.endswith(".pdf") else "html"
        rows.append({"id": url, "url": url, "kind": kind, "slug": _url_slug(url)})
    manifest_path = out / "fetch_manifest.json"
    manifest: dict[str, Any] = (
        json.loads(manifest_path.read_text("utf-8"))
        if manifest_path.is_file()
        else {"started_at": _utc_now(), "user_agent": USER_AGENT, "sources": {}}
    )
    context = _tls_context()
    calls = 0

    def get(url: str) -> dict[str, Any]:
        nonlocal calls
        if calls:
            time.sleep(args.delay)
        calls += 1
        getter: Callable[[str, ssl.SSLContext], dict[str, Any]] = args.getter
        return getter(url, context)

    for row in rows:
        manifest["sources"][row["slug"]] = _save(out, row, get(row["url"]))
    failed = sum(1 for row in rows if "error" in manifest["sources"][row["slug"]])
    news_ok = True
    if not targeted:
        first = {}
        index = manifest["sources"].get("en_news_index", {})
        if index.get("file"):
            first["en"] = (out / index["file"]).read_text("utf-8")
        cached = {str(e["url"]).split("?")[0] for e in entries}
        cutoff = args.since or news_cutoff(entries)
        pages, scan, news, news_ok = scan_news(get, out, cutoff, args.news_max_pages, cached, first)
        manifest["sources"].update(pages)
        manifest["news_scan"] = scan
        manifest["news_entries"] = news
        print(f"news: {len(news)} uncached entries since {cutoff} fetched into {out / NEWS_DIR}")
    manifest["finished_at"] = _utc_now()
    (out / "fetch_manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=False) + "\n", "utf-8"
    )
    print(f"fetched {len(rows) - failed}/{len(rows)} sources into {out}")
    return 1 if failed or not news_ok else 0


def _read_manifest(fetched: Path) -> dict[str, Any]:
    path = fetched / "fetch_manifest.json"
    if not path.is_file():
        raise SystemExit(f"{path} not found; run `fetch --out {fetched}` first")
    data: dict[str, Any] = json.loads(path.read_text("utf-8"))
    return data


def _parse_long_date(text: str) -> str | None:
    for fmt in ("%B %d, %Y", "%b %d, %Y", "%B %d,%Y"):
        try:
            return dt.datetime.strptime(text.strip(), fmt).date().isoformat()
        except ValueError:
            continue
    return None


def rules_version_check(root: Path, fetched: Path, manifest: dict[str, Any]) -> dict[str, Any]:
    cached = json.loads(
        (root / "src" / "gcg_sim" / "data" / "official" / "rules_version.json").read_text("utf-8")
    )
    md = root / "src" / "gcg_sim" / "data" / "rules" / "gundam-card-game-comprehensive-rules.md"
    head = md.read_text("utf-8")[:600]
    md_version = re.search(r"Ver\.\s*([0-9][0-9.]*)", head)
    md_updated = re.search(r"Updated\s+([A-Z][a-z]+\.? \d{1,2}, \d{4})", head)
    out: dict[str, Any] = {
        "cached_latest_version": cached.get("latest_version"),
        "cached_latest_date": cached.get("latest_date"),
        "markdown_version": md_version.group(1) if md_version else None,
        "markdown_updated": _parse_long_date(md_updated.group(1)) if md_updated else None,
    }
    page = fetched / "en_rules_index.txt"
    if page.is_file():
        m = re.search(
            r"COMPREHENSIVE RULES.*?Updated ([A-Z][a-z]+ \d{1,2}, ?\d{4})",
            page.read_text("utf-8"),
            re.S,
        )
        out["page_updated"] = _parse_long_date(m.group(1)) if m else None
    html = fetched / "en_rules_index.html"
    if html.is_file():
        link = re.search(r'href="([^"]*comprehensiverules_en\.pdf[^"]*)"', html.read_text("utf-8"))
        out["pdf_link"] = (
            urljoin("https://www.gundam-gcg.com/en/rules/", link.group(1)) if link else None
        )
    pdf_txt = fetched / "en_pdf_comprehensiverules_en.txt"
    if pdf_txt.is_file():
        first = pdf_txt.read_text("utf-8")[:2000]
        v = re.search(r"Ver\.\s*([0-9][0-9.]*)", first)
        u = re.search(r"Updated\s+([A-Z][a-z]+\.? \d{1,2}, \d{4})", first)
        out["pdf_version"] = v.group(1) if v else None
        out["pdf_updated"] = _parse_long_date(u.group(1).replace(".", "")) if u else None
    pdf = manifest["sources"].get("en_pdf_comprehensiverules_en", {})
    out["pdf_sha256"] = pdf.get("sha256")
    out["pdf_sha256_cached"] = cached.get("pdf_sha256")
    newer = [
        out.get("page_updated") not in (None, cached.get("latest_date")),
        out.get("pdf_version") not in (None, cached.get("latest_version")),
        out.get("pdf_updated") not in (None, cached.get("latest_date")),
    ]
    out["new_version"] = any(newer)
    out["markdown_matches_cache"] = out["markdown_version"] == cached.get("latest_version") and out[
        "markdown_updated"
    ] == cached.get("latest_date")
    return out


def banlist_links_check(root: Path, fetched: Path) -> dict[str, Any]:
    cached_urls = {
        str(e["url"]).split("?")[0]
        for e in json.loads((root / "data" / "sources.d" / "official.json").read_text("utf-8"))
    }
    out: dict[str, Any] = {}
    for slug, base in (
        ("en_rules_index", "https://www.gundam-gcg.com/en/rules/"),
        ("asia-en_rules_index", "https://www.gundam-gcg.com/asia-en/rules/"),
    ):
        html = fetched / f"{slug}.html"
        if not html.is_file():
            continue
        raw = html.read_text("utf-8")
        links = sorted(
            {
                urljoin(base, h).split("?")[0]
                for h in re.findall(r'href="([^"]*news/\d+_\d+\.html)', raw)
            }
        )
        text = (
            (fetched / f"{slug}.txt").read_text("utf-8")
            if (fetched / f"{slug}.txt").is_file()
            else ""
        )
        out[slug] = {
            "announcement_links": links,
            "uncached_links": [u for u in links if u not in cached_urls],
            "effective_dates": re.findall(r"Effective ([A-Z][a-z]+ \d{1,2}, \d{4})", text),
        }
    return out


def cmd_compare(args: argparse.Namespace) -> int:
    manifest = _read_manifest(args.fetched)
    rows = {r["slug"]: r for r in load_sources(args.root)}
    pages: list[dict[str, Any]] = []
    for slug, rec in sorted(manifest["sources"].items()):
        row = rows.get(slug)
        entry = row["entry"] if row else {}
        item: dict[str, Any] = {"slug": slug, "url": rec["url"], "id": rec["id"]}
        if "error" in rec:
            item["status"] = "fetch failed"
            item["detail"] = rec["error"]
            pages.append(item)
            continue
        local = entry.get("local_path") or ""
        cached_txt = (args.root / local).with_suffix(".txt") if local else None
        if not row:
            item["status"] = "not cached (new page)"
        elif rec["sha256"] == entry.get("sha256"):
            item["status"] = "unchanged (bytes)"
        elif cached_txt and cached_txt.is_file() and rec.get("text_file"):
            old = cached_txt.read_text("utf-8").splitlines()
            new = (args.fetched / rec["text_file"]).read_text("utf-8").splitlines()
            listing = "news_index" in slug or slug.endswith("_top")
            if old == new:
                item["status"] = "unchanged (text)"
            elif listing and sorted(old) == sorted(new):
                item["status"] = "unchanged (listing reordered)"
            else:
                item["status"] = "CHANGED"
                item["diff"] = list(
                    difflib.unified_diff(old, new, "cached", "fetched", n=1, lineterm="")
                )
        else:
            item["status"] = "CHANGED (bytes; no cached text to compare)"
        pages.append(item)
    report = {
        "pages": pages,
        "rules_version": rules_version_check(args.root, args.fetched, manifest),
        "banlist_links": banlist_links_check(args.root, args.fetched),
    }
    for p in pages:
        print(f"{p['status']:<44} {p['slug']}")
        for line in p.get("diff", [])[: args.max_diff_lines]:
            print(f"    {line}")
    rv = report["rules_version"]
    print(
        f"rules: page says {rv.get('page_updated')}, PDF Ver. {rv.get('pdf_version')} "
        f"({rv.get('pdf_updated')}); cached {rv['cached_latest_version']} @ {rv['cached_latest_date']}; "
        f"packaged markdown Ver. {rv['markdown_version']} ({rv['markdown_updated']})"
    )
    print(
        "rules: NEW VERSION - follow step 3 of SKILL.md"
        if rv["new_version"]
        else "rules: up to date"
    )
    for slug, info in report["banlist_links"].items():
        print(
            f"{slug}: B&R links {len(info['announcement_links'])}, effective {info['effective_dates']}"
        )
        for u in info["uncached_links"]:
            print(f"    NEW announcement link not in the cache: {u}")
    if args.json:
        args.json.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", "utf-8")
    return 0


def parse_news(html: str, base: str) -> list[dict[str, Any]]:
    """The entries of one news listing page in page order; ``pickup`` marks pinned entries."""
    items: list[dict[str, Any]] = []
    for cls, tags, body, category in _NEWS_ITEM.findall(html):
        href = re.search(r'href="([^"]+)"', body)
        date = re.search(r'cardDate">([^<]*)<', body)
        lead = re.search(r'cardLead">([^<]*)<', body)
        raw_date = date.group(1).strip() if date else ""
        items.append(
            {
                "date": _parse_long_date(raw_date) or raw_date,
                "category": category.strip(),
                "tags": tags,
                "title": lead.group(1).strip() if lead else "",
                "url": urljoin(base, href.group(1)) if href else "",
                "pickup": "is-pickup" in cls.split(),
            }
        )
    return items


def cmd_news(args: argparse.Namespace) -> int:
    manifest = _read_manifest(args.fetched)
    entries = json.loads((args.root / "data" / "sources.d" / "official.json").read_text("utf-8"))
    cached = {str(e["url"]).split("?")[0] for e in entries}
    scan = manifest.get("news_scan")
    since = args.since or (scan or {}).get("cutoff") or news_cutoff(entries)
    complete = True
    if scan is None:
        complete = False
        print(
            "WARNING: the fetch has no news scan (an old or targeted fetch); run `fetch` again",
            file=sys.stderr,
        )
    else:
        for region, info in sorted(scan["listings"].items()):
            if "error" in info:
                complete = False
                print(f"WARNING: news {region} not read to the cutoff: {info['error']}")
            else:
                print(
                    f"news {region}: {len(info['pages'])} page(s), stopped at the {info['stop']} "
                    f"(oldest entry {info['oldest']}, cutoff {scan['cutoff']})"
                )
        if since < scan["cutoff"]:
            complete = False
            print(f"WARNING: --since {since} is before the fetch's cutoff {scan['cutoff']}")
    seen: dict[str, dict[str, Any]] = {}
    for slug, rec in sorted(manifest["sources"].items()):
        if "news_index" not in slug or not rec.get("file"):
            continue
        html = (args.fetched / rec["file"]).read_text("utf-8")
        for item in parse_news(html, rec["url"]):
            seen.setdefault(item["url"], item)
    fetched: dict[str, Any] = manifest.get("news_entries", {})
    rows = sorted(seen.values(), key=lambda i: (i["date"], i["url"]), reverse=True)
    print(f"news entries on the fetched listing pages: {len(rows)}; showing uncached since {since}")
    older_relevant = 0
    for item in rows:
        url = item["url"].split("?")[0]
        if url in cached:
            continue
        rec = fetched.get(url)
        text = None
        if rec and rec.get("text_file"):
            text = (args.fetched / rec["text_file"]).read_text("utf-8")
        reasons = flag_reasons(item, text)
        if item["date"] < since:
            older_relevant += bool(reasons)
            continue
        if rec is None or "error" in rec:
            complete = False
            reasons.append("entry not fetched")
        flag = "RELEVANT" if reasons else "        "
        why = f"  [{'; '.join(reasons)}]" if reasons else ""
        print(f"{flag} {item['date']} {item['category']:<8} {item['title']}  {url}{why}")
    if older_relevant:
        print(
            f"({older_relevant} older rules-related entries are not cached on purpose, e.g. team, "
            "sealed and battle-royale formats; list them with --since 2000-01-01)"
        )
    if not complete:
        print("news: INCOMPLETE - some news since the cutoff was not read", file=sys.stderr)
    return 0 if complete else 1


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--root", type=Path, default=REPO, help="gcg-sim checkout")
    sub = ap.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("--out", type=Path, required=True)
    f.add_argument("--only", nargs="*", help="slugs or source ids to fetch")
    f.add_argument("--url", action="append", help="also fetch this page (new notice); repeatable")
    f.add_argument("--delay", type=float, default=1.5, help="seconds between requests")
    f.add_argument(
        "--since", help="news cutoff YYYY-MM-DD (default: newest news-listing retrieval date)"
    )
    f.add_argument(
        "--news-max-pages",
        type=int,
        default=NEWS_MAX_PAGES,
        help="listing pages per region to read at most; reaching it before the cutoff fails",
    )
    f.set_defaults(func=cmd_fetch, getter=_get)
    c = sub.add_parser("compare")
    c.add_argument("--fetched", type=Path, required=True)
    c.add_argument("--json", type=Path)
    c.add_argument("--max-diff-lines", type=int, default=60)
    c.set_defaults(func=cmd_compare)
    n = sub.add_parser("news")
    n.add_argument("--fetched", type=Path, required=True)
    n.add_argument("--since", help="YYYY-MM-DD (default: the cutoff the fetch paged back to)")
    n.set_defaults(func=cmd_news)
    args = ap.parse_args(argv)
    code: int = args.func(args)
    return code


if __name__ == "__main__":
    raise SystemExit(main())
