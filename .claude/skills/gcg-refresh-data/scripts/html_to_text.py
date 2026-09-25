#!/usr/bin/env python3
"""Plain-text extraction of an official gundam-gcg.com HTML page (stdlib only).

Adopted from the official-sources research generator. The ``.txt`` files next to the raw pages
in ``data/official_raw/`` were produced by this algorithm, so re-extracting a cached page
reproduces its ``.txt`` byte for byte and a text diff between a cached and a freshly fetched page
shows only real content changes.

    python html_to_text.py PAGE.html PAGE.txt
"""

from __future__ import annotations

import argparse
import re
from html.parser import HTMLParser
from pathlib import Path

BLOCK = frozenset(
    {
        "p",
        "div",
        "br",
        "li",
        "tr",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "section",
        "article",
        "table",
        "ul",
        "ol",
        "dt",
        "dd",
        "dl",
        "header",
        "footer",
        "nav",
        "hr",
        "th",
        "td",
        "main",
        "figure",
        "figcaption",
    }
)
SKIP = frozenset({"script", "style", "noscript", "svg", "head"})
_SPACES = re.compile(r"[ \t　\xa0]+")


class _Extract(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.out: list[str] = []
        self.skip = 0

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in SKIP:
            self.skip += 1
        elif tag in BLOCK:
            self.out.append("\n")
        if tag == "li":
            self.out.append("- ")
        if tag in {"td", "th"}:
            self.out.append(" | ")
        if tag == "img":
            alt = dict(attrs).get("alt")
            if alt:
                self.out.append(f"[img:{alt}]")

    def handle_endtag(self, tag: str) -> None:
        if tag in SKIP:
            self.skip = max(0, self.skip - 1)
        elif tag in BLOCK:
            self.out.append("\n")

    def handle_data(self, data: str) -> None:
        if not self.skip:
            self.out.append(data)


def html_to_text(raw: bytes) -> str:
    parser = _Extract()
    parser.feed(raw.decode("utf-8", errors="replace"))
    parser.close()
    lines = [_SPACES.sub(" ", ln).strip() for ln in "".join(parser.out).splitlines()]
    cleaned: list[str] = []
    for ln in lines:
        if ln == "" and cleaned and cleaned[-1] == "":
            continue
        cleaned.append(ln)
    return "\n".join(cleaned).strip() + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("src", type=Path)
    ap.add_argument("dst", type=Path)
    args = ap.parse_args(argv)
    with args.dst.open("w", encoding="utf-8", newline="\n") as fh:
        fh.write(html_to_text(args.src.read_bytes()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
