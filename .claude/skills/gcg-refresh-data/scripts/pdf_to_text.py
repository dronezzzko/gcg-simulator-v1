#!/usr/bin/env python3
"""Plain-text extraction of an official PDF (Comprehensive Rules, TRM, floor rules).

pypdf is not a project dependency; run this in a throwaway environment:

    uv run --no-project --with pypdf python pdf_to_text.py IN.pdf OUT.txt [--name NAME]

Adopted from the official-sources research generator: the header lines (source name, page
count, PDF metadata) and the ``===== page N =====`` separators match the extractions stored in
``data/official_raw/``. ``--name`` sets the file name shown in the header (default: IN's name);
use the cached slug, e.g. ``en_pdf_comprehensiverules_en.pdf``.
"""

from __future__ import annotations

import argparse
from pathlib import Path


def pdf_to_text(path: Path, name: str | None = None) -> str:
    from pypdf import PdfReader

    reader = PdfReader(str(path))
    meta = reader.metadata or {}
    parts = [f"# Source PDF: {name or path.name} ({len(reader.pages)} pages)"]
    for key in ("/Title", "/CreationDate", "/ModDate"):
        if key in meta:
            parts.append(f"# {key[1:]}: {meta[key]}")
    for number, page in enumerate(reader.pages, 1):
        parts.append(f"\n===== page {number} =====\n")
        parts.append((page.extract_text() or "").strip())
    return "\n".join(parts) + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("src", type=Path)
    ap.add_argument("dst", type=Path)
    ap.add_argument("--name", help="file name for the header line")
    args = ap.parse_args(argv)
    with args.dst.open("w", encoding="utf-8", newline="\n") as fh:
        fh.write(pdf_to_text(args.src, args.name))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
