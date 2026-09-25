#!/usr/bin/env python3
"""Fetch the official English Gundam Card Game pages and compare them with the cached copies.

The page list is every entry of ``data/sources.d/official.json`` (US and Asia rules pages, B&R
announcements, BO3/language/deck-building pages, errata notices, rules PDFs, news listings) plus
extra pages of the US news listing. Requests go one at a time with a pause and a descriptive
User-Agent; nothing in the repository is modified.

    uv run --no-project --with pypdf python fetch_official.py fetch --out DIR
    uv run --no-project python fetch_official.py compare --fetched DIR [--json OUT]
    uv run --no-project python fetch_official.py news --fetched DIR [--since YYYY-MM-DD]

``fetch`` writes ``<slug>.html|pdf``, a ``<slug>.txt`` extraction (PDF text needs pypdf, hence
``--with pypdf``) and ``fetch_manifest.json``. ``compare`` reports, per page, whether the bytes
or at least the extracted text are unchanged, shows text diffs, and checks the Comprehensive
Rules version and the banned/restricted links. ``news`` lists news entries that are not cached
yet (possible new B&R lists, errata or rule updates).
"""

from __future__ import annotations

import argparse
import datetime as dt
import difflib
import hashlib
import json
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any
from urllib.parse import urljoin, urlparse

from html_to_text import html_to_text

REPO = Path(__file__).resolve().parents[4]
USER_AGENT = "gcg-sim data refresh (Claude Code skill gcg-refresh-data; one request at a time)"
NEWS_PAGE = "https://www.gundam-gcg.com/en/news/?subcategory=all&tag=all&page={n}"
RELEVANT = re.compile(
    r"errata|banned|restricted|rule|correction|revision|faq|comprehensive|regulation", re.I
)
_NEWS_ITEM = re.compile(
    r'<div class="newsDetail[^"]*" data-tags="([^"]*)">(.*?)cardCategory">([^<]*)<', re.S
)


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


def _get(url: str) -> dict[str, Any]:
    req = urllib.request.Request(
        url, headers={"User-Agent": USER_AGENT, "Accept-Language": "en-US,en;q=0.8"}
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
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


def cmd_fetch(args: argparse.Namespace) -> int:
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    rows = load_sources(args.root)
    rows += [
        {
            "id": f"news-page-{n}",
            "url": NEWS_PAGE.format(n=n),
            "kind": "html",
            "slug": f"en_news_index_p{n}",
        }
        for n in range(2, args.news_pages + 1)
    ]
    if args.only or args.url:
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
    for i, row in enumerate(rows):
        if i:
            time.sleep(args.delay)
        res = _get(row["url"])
        rec: dict[str, Any] = {"id": row["id"], "url": row["url"], "kind": row["kind"]}
        rec["retrieved_at"] = _utc_now()
        if "error" in res:
            rec["error"] = res["error"]
            print(f"FAILED {row['slug']}: {res['error']}", file=sys.stderr)
        else:
            body: bytes = res.pop("body")
            path = out / f"{row['slug']}.{row['kind']}"
            path.write_bytes(body)
            rec.update(res)
            rec.update({"file": path.name, "sha256": _sha256(body), "bytes": len(body)})
            text = (
                html_to_text(body)
                if row["kind"] == "html"
                else _extract_pdf(path, f"{row['slug']}.pdf")
            )
            if text is None:
                rec["text_file"] = None
                rec["note"] = (
                    "PDF text not extracted: run this with `uv run --no-project --with pypdf`"
                )
            else:
                txt = out / f"{row['slug']}.txt"
                with txt.open("w", encoding="utf-8", newline="\n") as fh:
                    fh.write(text)
                rec["text_file"] = txt.name
            print(f"{rec['status']} {row['slug']} {len(body):,} bytes")
        manifest["sources"][row["slug"]] = rec
    manifest["finished_at"] = _utc_now()
    (out / "fetch_manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=False) + "\n", "utf-8"
    )
    failed = sum(1 for row in rows if "error" in manifest["sources"][row["slug"]])
    print(f"fetched {len(rows) - failed}/{len(rows)} into {out}")
    return 1 if failed else 0


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


def parse_news(html: str, base: str) -> list[dict[str, str]]:
    items = []
    for tags, body, category in _NEWS_ITEM.findall(html):
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
            }
        )
    return items


def cmd_news(args: argparse.Namespace) -> int:
    manifest = _read_manifest(args.fetched)
    entries = json.loads((args.root / "data" / "sources.d" / "official.json").read_text("utf-8"))
    cached = {str(e["url"]).split("?")[0] for e in entries}
    since = args.since or max(str(e.get("retrieved_at") or "")[:10] for e in entries)
    seen: dict[str, dict[str, str]] = {}
    for slug, rec in sorted(manifest["sources"].items()):
        if "news_index" not in slug or not rec.get("file"):
            continue
        html = (args.fetched / rec["file"]).read_text("utf-8")
        for item in parse_news(html, rec["url"]):
            seen.setdefault(item["url"], item)
    rows = sorted(seen.values(), key=lambda i: (i["date"], i["url"]), reverse=True)
    print(f"news entries on the fetched listing pages: {len(rows)}; showing uncached since {since}")
    older_relevant = 0
    for item in rows:
        if item["url"].split("?")[0] in cached:
            continue
        relevant = item["category"] == "RULES" or bool(RELEVANT.search(item["title"]))
        if item["date"] < since:
            older_relevant += relevant
            continue
        flag = "RELEVANT" if relevant else "        "
        print(f"{flag} {item['date']} {item['category']:<8} {item['title']}  {item['url']}")
    if older_relevant:
        print(
            f"({older_relevant} older rules-related entries are not cached on purpose, e.g. team, "
            "sealed and battle-royale formats; list them with --since 2000-01-01)"
        )
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--root", type=Path, default=REPO, help="gcg-sim checkout")
    sub = ap.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("--out", type=Path, required=True)
    f.add_argument("--only", nargs="*", help="slugs or source ids to fetch")
    f.add_argument("--url", action="append", help="also fetch this page (new notice); repeatable")
    f.add_argument("--delay", type=float, default=1.5, help="seconds between requests")
    f.add_argument("--news-pages", type=int, default=3, help="US news listing pages to read")
    f.set_defaults(func=cmd_fetch)
    c = sub.add_parser("compare")
    c.add_argument("--fetched", type=Path, required=True)
    c.add_argument("--json", type=Path)
    c.add_argument("--max-diff-lines", type=int, default=60)
    c.set_defaults(func=cmd_compare)
    n = sub.add_parser("news")
    n.add_argument("--fetched", type=Path, required=True)
    n.add_argument("--since", help="YYYY-MM-DD (default: latest cached retrieval date)")
    n.set_defaults(func=cmd_news)
    args = ap.parse_args(argv)
    code: int = args.func(args)
    return code


if __name__ == "__main__":
    raise SystemExit(main())
