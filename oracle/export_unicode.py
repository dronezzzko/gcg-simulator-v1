"""Export Python's Unicode behaviour for the Rust gcg-data crate (docs/dev/PLAN.md A27).

Writes two files from the running interpreter's ``unicodedata`` and ``str`` methods:

- ``crates/gcg-data/src/unicode/tables.rs``: the tables the Rust data layer needs for
  ``deck/names.py`` (NFKC, ``casefold``, ``str.split``/``strip`` whitespace), ``deck/parse.py``
  (regex ``\\d``/``\\s``, ``int()`` digit values, ``upper``/``lower``, ``splitlines``) and
  CPython ``repr`` (``str.isprintable``);
- ``crates/gcg-data/tests/vectors/unicode.json``: 10,000 strings with their NFKC, casefold,
  ``normalize_name`` and whitespace results, plus case-mapping, ``repr`` and ``splitlines``
  vectors (T-037).

Every table is read from the interpreter (no hand-written Unicode data), and the
``unicodedata.unidata_version`` is recorded in both files.

    uv run python oracle/export_unicode.py [--check]
"""

from __future__ import annotations

import argparse
import json
import sys
import unicodedata
from collections.abc import Callable, Iterable
from functools import cache
from pathlib import Path

from gcg_sim.deck.names import normalize_name, strip_invisible
from gcg_sim.rng import SplitMix64

ROOT = Path(__file__).resolve().parents[1]
TABLES = ROOT / "crates" / "gcg-data" / "src" / "unicode" / "tables.rs"
VECTORS = ROOT / "crates" / "gcg-data" / "tests" / "vectors" / "unicode.json"

MAX_CP = 0x110000
HANGUL_FIRST, HANGUL_LAST = 0xAC00, 0xD7A3
TEXT_VECTORS = 10_000
CASE_VECTORS = 3_000
REPR_VECTORS = 2_000
# Zero-width characters and quotes of deck/names.py, sigma forms, dotted/dotless i, sharp s,
# ligatures, Roman numerals, fullwidth Z, ideographic and no-break spaces, the token-text dots
# and the lenticular brackets of 【Pilot】.
SPECIAL = (
    *(0x200B, 0x200C, 0x200D, 0x2060, 0xFEFF, 0x00AD),
    *(0x2018, 0x2019, 0x201A, 0x201B, 0x2032, 0x00B4, 0x60, 0x201C, 0x201D, 0x201E, 0x2033),
    *(0x27, 0x22, 0x3A3, 0x3C3, 0x3C2, 0x130, 0x131, 0xDF, 0x1E9E, 0xFB00, 0x2160, 0x2161),
    *(0xFF3A, 0x3000, 0xA0, 0x30FB, 0xFF65, 0x3010, 0x3011),
)


def code_points() -> Iterable[int]:
    """Every scalar value (surrogates cannot occur in a Rust ``str``)."""
    return (c for c in range(MAX_CP) if not 0xD800 <= c <= 0xDFFF)


def ranges(pred: Callable[[int], bool]) -> list[tuple[int, int]]:
    out: list[tuple[int, int]] = []
    for c in code_points():
        if not pred(c):
            continue
        if out and out[-1][1] == c - 1:
            out[-1] = (out[-1][0], c)
        else:
            out.append((c, c))
    return out


def is_line_break(c: int) -> bool:
    return len(("a" + chr(c) + "b").splitlines()) == 2


def is_cased(c: int) -> bool:
    """``_PyUnicode_IsCased`` as the Final_Sigma rule of ``str.lower`` reads it
    (``handle_capital_sigma``): a capital sigma right after the character is final. For a
    case-ignorable character the probe says False, and the rule never reads its value."""
    return (chr(c) + "\u03a3").lower().endswith("\u03c2")


def is_case_ignorable(c: int) -> bool:
    """``_PyUnicode_IsCaseIgnorable``, probed through the Final_Sigma rule of ``str.lower``
    (``handle_capital_sigma``): only a case-ignorable character between a cased letter and a
    capital sigma, and between the sigma and the end, keeps the sigma final on both sides."""
    ch = chr(c)
    before = ("A" + ch + "Σ").lower().endswith("ς")
    after = ("AΣ" + ch).lower()[1] == "ς"
    return before and after


def mapping(fn: Callable[[str], str], skip_hangul: bool = False) -> list[tuple[int, str]]:
    out = []
    for c in code_points():
        if skip_hangul and HANGUL_FIRST <= c <= HANGUL_LAST:
            continue
        mapped = fn(chr(c))
        if mapped != chr(c):
            out.append((c, mapped))
    return out


def decimal_runs() -> list[tuple[int, int, int]]:
    """Runs of decimal digits (``str.isdecimal``) whose ``int()`` values are consecutive."""
    out: list[tuple[int, int, int]] = []
    for c in code_points():
        ch = chr(c)
        if not ch.isdecimal():
            continue
        value = int(ch)
        if out and out[-1][1] == c - 1 and out[-1][2] + (c - out[-1][0]) == value:
            out[-1] = (out[-1][0], c, out[-1][2])
        else:
            out.append((c, c, value))
    return out


def ccc_runs() -> list[tuple[int, int, int]]:
    out: list[tuple[int, int, int]] = []
    for c in code_points():
        k = unicodedata.combining(chr(c))
        if k == 0:
            continue
        if out and out[-1][1] == c - 1 and out[-1][2] == k:
            out[-1] = (out[-1][0], c, k)
        else:
            out.append((c, c, k))
    return out


def composition_pairs() -> list[tuple[int, int, int]]:
    """Primary composites: a canonical two-character decomposition that NFC keeps composed
    (composition exclusions, singletons and non-starter decompositions drop out)."""
    out = []
    for c in code_points():
        if HANGUL_FIRST <= c <= HANGUL_LAST:
            continue
        d = unicodedata.decomposition(chr(c))
        if not d or d.startswith("<"):
            continue
        parts = [int(x, 16) for x in d.split()]
        if len(parts) == 2 and unicodedata.normalize("NFC", chr(c)) == chr(c):
            out.append((parts[0], parts[1], c))
    out.sort()
    return out


def rust_str(s: str) -> str:
    return "".join(f"\\u{{{ord(ch):x}}}" for ch in s)


def wrap(pieces: list[str], sep: str) -> list[str]:
    """Pack ``pieces`` into indented lines of at most 99 characters (rustfmt skips the tables,
    so the generator keeps them readable)."""
    lines: list[str] = []
    row = ""
    for piece in pieces:
        if row and len(row) + len(sep) + len(piece) > 94:
            lines.append("    " + row)
            row = ""
        row = piece if not row else row + sep + piece
    if row:
        lines.append("    " + row)
    return lines


def emit_ranges(name: str, doc: str, items: list[tuple[int, int]]) -> list[str]:
    lines = [f"/// {doc}", "#[rustfmt::skip]", f"pub(crate) static {name}: &[(u32, u32)] = &["]
    lines += wrap([f"(0x{a:x}, 0x{b:x})," for a, b in items], " ")
    lines.append("];")
    return lines


def emit_triples(name: str, doc: str, ty: str, items: list[tuple[int, int, int]]) -> list[str]:
    lines = [f"/// {doc}", "#[rustfmt::skip]", f"pub(crate) static {name}: &[{ty}] = &["]
    lines += wrap([f"(0x{a:x}, 0x{b:x}, {c})," for a, b, c in items], " ")
    lines.append("];")
    return lines


def emit_mapping(name: str, doc: str, items: list[tuple[int, str]]) -> list[str]:
    """``{name}``: sorted ``(code point, start, end)`` byte ranges into ``{name}_DATA``."""
    index: list[str] = []
    data: list[str] = []
    offset = 0
    for c, mapped in items:
        n = len(mapped.encode("utf-8"))
        index.append(f"(0x{c:x}, {offset}, {offset + n}),")
        offset += n
        data.append(rust_str(mapped))
    lines = [
        f"/// {doc}",
        f"/// Sorted by code point; the value is `{name}_DATA[start..end]`.",
        "#[rustfmt::skip]",
        f"pub(crate) static {name}: &[(u32, u32, u32)] = &[",
    ]
    lines += wrap(index, " ")
    lines.append("];")
    lines.append(f"/// The mapped strings of [`{name}`], concatenated.")
    lines.append("#[rustfmt::skip]")
    lines.append(f'pub(crate) static {name}_DATA: &str = "\\')
    lines += [line + "\\" for line in wrap(data, "")]
    lines.append('";')
    return lines


def build_tables() -> str:
    version = unicodedata.unidata_version
    out = [
        "//! Unicode tables of CPython's `unicodedata` and `str` methods (docs/dev/PLAN.md A27).",
        "//!",
        "//! Generated by `oracle/export_unicode.py`; do not edit. Regenerate with",
        "//! `uv run python oracle/export_unicode.py`, check with `--check`.",
        "",
        "/// `unicodedata.unidata_version` of the generating interpreter.",
        f'pub(crate) const UNIDATA_VERSION: &str = "{version}";',
        "",
    ]
    out += emit_ranges(
        "SPACE",
        "`str.isspace` (regex `\\s`, `strip`, `split`).",
        ranges(lambda c: chr(c).isspace()),
    )
    out.append("")
    out += emit_ranges("LINE_BREAK", "`str.splitlines` separators.", ranges(is_line_break))
    out.append("")
    out += emit_ranges(
        "NOT_PRINTABLE",
        "Code points where `str.isprintable` is false (CPython `repr` escapes them).",
        ranges(lambda c: not chr(c).isprintable()),
    )
    out.append("")
    out += emit_ranges("CASED", "`_PyUnicode_IsCased` (Final_Sigma context).", ranges(is_cased))
    out.append("")
    out += emit_ranges(
        "CASE_IGNORABLE",
        "`_PyUnicode_IsCaseIgnorable` (Final_Sigma context).",
        ranges(is_case_ignorable),
    )
    out.append("")
    out += emit_triples(
        "DECIMAL",
        "`str.isdecimal` (regex `\\d`) runs `(first, last, int(first))`; values are consecutive.",
        "(u32, u32, u8)",
        decimal_runs(),
    )
    out.append("")
    out += emit_triples(
        "CCC",
        "Runs `(first, last, class)` of non-zero canonical combining classes.",
        "(u32, u32, u8)",
        ccc_runs(),
    )
    out.append("")
    out += emit_triples(
        "COMPOSE",
        "Primary composites `(first, second, composite)`, sorted; Hangul is algorithmic.",
        "(u32, u32, u32)",
        composition_pairs(),
    )
    out.append("")
    out += emit_mapping(
        "NFKD",
        "`unicodedata.normalize('NFKD', ch)` per changed character (Hangul is algorithmic).",
        mapping(lambda ch: unicodedata.normalize("NFKD", ch), skip_hangul=True),
    )
    out.append("")
    out += emit_mapping("CASEFOLD", "`str.casefold` per character.", mapping(str.casefold))
    out.append("")
    out += emit_mapping("UPPER", "`str.upper` per character.", mapping(str.upper))
    out.append("")
    out += emit_mapping(
        "LOWER",
        "`str.lower` per character (U+03A3 here is the non-final form).",
        mapping(str.lower),
    )
    return "\n".join(out) + "\n"


# --- vectors ---------------------------------------------------------------------------------


@cache
def pools() -> list[list[str]]:
    pick = [chr(c) for c in code_points()]
    space = [ch for ch in pick if ch.isspace()]
    breaks = [chr(c) for c in code_points() if is_line_break(c)]
    marks = [ch for ch in pick if unicodedata.combining(ch)]
    decomposable = [
        ch
        for ch in pick
        if unicodedata.normalize("NFKD", ch) != ch and not HANGUL_FIRST <= ord(ch) <= HANGUL_LAST
    ]
    folding = [ch for ch in pick if ch.casefold() != ch.lower() or ch.casefold() != ch]
    digits = [ch for ch in pick if ch.isdecimal()]
    jamo = [
        chr(c) for c in (*range(0x1100, 0x1113), *range(0x1161, 0x1176), *range(0x11A7, 0x11C3))
    ]
    hangul = [chr(c) for c in range(HANGUL_FIRST, HANGUL_LAST + 1, 97)]
    special = [chr(c) for c in SPECIAL]
    ascii_chars = [chr(c) for c in range(0x20, 0x7F)]
    others = [chr(c) for c in (0x4E00, 0x4E8C, 0x1F680, 0x10400, 0x10428, 0xE000, 0x378, 0xE0001)]
    return [
        ascii_chars,
        space,
        breaks,
        marks,
        decomposable,
        folding,
        digits,
        jamo,
        hangul,
        special,
        others,
    ]


def random_string(rng: SplitMix64, groups: list[list[str]], max_len: int) -> str:
    n = 1 + rng.randrange(max_len)
    out = []
    for _ in range(n):
        group = groups[rng.randrange(len(groups))]
        out.append(group[rng.randrange(len(group))])
    return "".join(out)


def chunked(chars: list[str], size: int) -> list[str]:
    return ["".join(chars[i : i + size]) for i in range(0, len(chars), size)]


def text_inputs() -> list[str]:
    groups = pools()
    pairs = composition_pairs()
    # Every composition pair in decomposed form, every decomposable character, every folding
    # character: the tables are exercised entry by entry, then random mixes fill the rest.
    inputs = [chr(a) + chr(b) for a, b, _ in pairs]
    inputs += chunked(groups[4], 4)
    inputs += chunked(groups[5], 4)
    rng = SplitMix64(20261101)
    while len(inputs) < TEXT_VECTORS:
        inputs.append(random_string(rng, groups, 12))
    return inputs[:TEXT_VECTORS]


def case_inputs() -> list[str]:
    groups = pools()
    letters = [
        chr(c) for c in code_points() if chr(c).upper() != chr(c) or chr(c).lower() != chr(c)
    ]
    inputs = chunked(letters, 3)
    sigma = [
        "Σ",
        "AΣ",
        "AΣB",
        "A\N{COMBINING GREEK YPOGEGRAMMENI}Σ",
        "Σ\N{COMBINING GREEK YPOGEGRAMMENI}",
        "A.Σ.",
        "ΣΣ",
    ]
    inputs += sigma
    rng = SplitMix64(42)
    case_groups = [groups[0], groups[3], groups[5], groups[9], letters]
    while len(inputs) < CASE_VECTORS:
        inputs.append(random_string(rng, case_groups, 8))
    return inputs[:CASE_VECTORS]


def repr_inputs() -> list[str]:
    groups = pools()
    unprintable = [chr(c) for c in code_points() if not chr(c).isprintable()]
    sample = unprintable[:: max(1, len(unprintable) // 600)]
    inputs = chunked(sample, 2)
    inputs += ["it's", 'a"b', "a'b\"c", "\\", "\t\n\r", "\x00\x1f\x7f", "Zaku Ⅱ", ""]
    rng = SplitMix64(7)
    repr_groups = [groups[0], groups[1], groups[9], groups[10], sample, list("'\"\\")]
    while len(inputs) < REPR_VECTORS:
        inputs.append(random_string(rng, repr_groups, 10))
    return inputs[:REPR_VECTORS]


def build_vectors() -> dict[str, object]:
    text = [
        [
            s,
            unicodedata.normalize("NFKC", s),
            s.casefold(),
            normalize_name(s),
            strip_invisible(s),
            s.split(),
            s.strip(),
            s.splitlines(),
        ]
        for s in text_inputs()
    ]
    case = [[s, s.upper(), s.lower()] for s in case_inputs()]
    reprs = [[s, repr(s)] for s in repr_inputs()]
    return {
        "unidata_version": unicodedata.unidata_version,
        "text_fields": [
            "s",
            "nfkc",
            "casefold",
            "normalize_name",
            "strip_invisible",
            "split",
            "strip",
            "splitlines",
        ],
        "text": text,
        "case": case,
        "repr": reprs,
    }


def vectors_text(data: dict[str, object]) -> str:
    """One vector per line, so a stale file shows a readable diff."""
    lines = ["{"]
    items = list(data.items())
    for i, (key, value) in enumerate(items):
        sep = "," if i < len(items) - 1 else ""
        if isinstance(value, list) and value and isinstance(value[0], list):
            lines.append(f"{json.dumps(key)}: [")
            rows = [json.dumps(row, ensure_ascii=False) for row in value]
            lines += [r + ("," if j < len(rows) - 1 else "") for j, r in enumerate(rows)]
            lines.append("]" + sep)
        else:
            lines.append(f"{json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}{sep}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if a committed file is stale")
    args = ap.parse_args()
    outputs = {TABLES: build_tables(), VECTORS: vectors_text(build_vectors())}
    stale = [
        p for p, text in outputs.items() if not p.exists() or p.read_text(encoding="utf-8") != text
    ]
    if args.check:
        for p in stale:
            print(f"{p.relative_to(ROOT)} is stale; run without --check", file=sys.stderr)
        if not stale:
            print(f"Unicode tables and vectors are current (unidata {unicodedata.unidata_version})")
        return 1 if stale else 0
    for p, text in outputs.items():
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")
        print(f"wrote {p.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
