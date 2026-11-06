"""The evaluation deck panel and its pre-registered held-out split (docs/dev/PLAN.md §12.5, X16,
A14; WEAKNESSES W-20).

The panel is the three example decks (``examples/decks/*.txt``) followed by the five evaluation
decks (``examples/decks/eval/*.txt``), each set sorted by file name, as gcg-eval's ``--decks
panel`` loads them (``crates/gcg-eval/src/decks.rs`` ``load_decks``). The script

- checks every evaluation deck: it loads, is legal under ``validate_deck`` with the B&R list and
  the implemented-card check (the checks of ``gcg-sim validate``), has a ``# Archetype: <kind>.
  Plan: ...`` header after its title line, and names an archetype no other evaluation deck uses;
- computes main-deck copy overlaps, the size of the multiset intersection of two main decks'
  card numbers (W-20), between the panel decks, the tuning decks (``ai/tuning.py:89-93``
  ``TUNING_DECKS``) and the AI test decks (``tests/ai/decks.py`` ``DECKS``), and asserts that it
  reproduces the overlaps W-20 recomputed on the example decks;
- picks the two held-out decks (PLAN §12.5): among the evaluation decks whose overlap with every
  tuning deck and every AI test deck is at most 10 of 50, the two that sort first by (maximum
  copy overlap, total copy overlap, maximum name overlap, total name overlap, file name), each
  taken over the other twelve decks (the other seven panel decks, the three tuning decks and the
  two AI test decks). The name overlap counts copies whose card names match, so it also sees other
  printings of the same Unit or Pilot;
- writes ``docs/dev/eval/panel.json`` (``held_out`` is what gcg-eval reads, ``Panel::load``) and
  ``examples/decks/eval/PAIRS.md``: the eight decks with their paths, archetypes and digests, all
  28 pairs in ``itertools.combinations`` order (``ai/compare.py:131``) flagged ``heldout`` (13
  pairs, those with a held-out deck) or ``tuning`` (15), the selection table, the overlap matrix
  and the seed-role ranges of PLAN A14 (checked against ``crates/gcg-eval/src/contracts.rs``).

The split is pre-registered: both files are committed before any tuning run, and a later change
to a panel deck changes its digest, so ``--check`` fails until the split is regenerated.

    uv run python oracle/eval_decks.py [--check]
"""

from __future__ import annotations

import argparse
import itertools
import json
import re
import sys
from collections import Counter
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from export_decks import test_decks

from gcg_sim.ai.tuning import TUNING_DECKS
from gcg_sim.cards.db import get_card_db
from gcg_sim.deck import Deck, deck_digest, load_deck, to_decklist, validate_deck

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES_DIR = ROOT / "examples" / "decks"
EVAL_DIR = EXAMPLES_DIR / "eval"
PANEL_JSON = ROOT / "docs" / "dev" / "eval" / "panel.json"
PAIRS_MD = EVAL_DIR / "PAIRS.md"
CONTRACTS_RS = ROOT / "crates" / "gcg-eval" / "src" / "contracts.rs"
SCRIPT = "oracle/eval_decks.py"

MAIN_SIZE = 50
EXAMPLE_DECKS = 3
EVAL_DECKS = 5
HELD_OUT_DECKS = 2
OVERLAP_BOUND = 10
TUNING_NAMES = ("tuning-zeon", "tuning-wing", "tuning-tekkadan")
TEST_NAMES = ("tests-federation", "tests-seed")
TUNING_SOURCE = "src/gcg_sim/ai/tuning.py:89-93 TUNING_DECKS"
TEST_SOURCE = "tests/ai/decks.py DECKS"
# PLAN A14: (role, first master, last master, may play held-out pairs).
SEED_ROLES: tuple[tuple[str, int, int, bool], ...] = (
    ("tuning", 10_000_000, 19_999_999, False),
    ("dev", 20_000_000, 29_999_999, False),
    ("confirm", 30_000_000, 30_999_999, False),
    ("eval", 90_000_000, 90_999_999, True),
    ("review", 91_000_000, 91_999_999, True),
)
# W-20's recomputation of main-deck copy overlaps on the example decks (WEAKNESSES.md W-20).
W20_OVERLAPS: tuple[tuple[str, str, int], ...] = (
    ("tuning-zeon", "red-green-zeon", 27),
    ("tuning-tekkadan", "purple-white-tekkadan", 29),
    ("tuning-wing", "blue-white-federation", 2),
    ("tests-federation", "blue-white-federation", 27),
    ("tests-federation", "purple-white-tekkadan", 3),
    ("tests-seed", "blue-white-federation", 3),
    ("tests-seed", "red-green-zeon", 2),
)
ARCHETYPE = re.compile(r"Archetype: (?P<kind>[^.]+)\. Plan: (?P<plan>.+)")
ROLE_BOUNDS = re.compile(r"Self::(?P<role>[A-Za-z]+) => \((?P<first>[0-9_]+), (?P<last>[0-9_]+)\)")


class PanelError(Exception):
    """A panel deck or the split breaks a rule of PLAN §12.5."""


@dataclass(frozen=True, slots=True)
class PanelDeck:
    name: str
    path: str
    source: str
    title: str
    archetype: str | None
    plan: str
    colors: tuple[str, ...]
    main: tuple[str, ...]
    digest: str


@dataclass(frozen=True, slots=True)
class Candidate:
    name: str
    max_bound_overlap: int
    max_overlap: int
    total_overlap: int
    max_name_overlap: int
    total_name_overlap: int
    eligible: bool

    def key(self) -> tuple[int, int, int, int, str]:
        return (
            self.max_overlap,
            self.total_overlap,
            self.max_name_overlap,
            self.total_name_overlap,
            self.name,
        )


def copy_overlap(a: Sequence[str], b: Sequence[str]) -> int:
    """Main-deck copies two decks share: the size of the multiset intersection (W-20)."""
    return sum((Counter(a) & Counter(b)).values())


def header_lines(path: Path) -> list[str]:
    """The leading comment block of a deck file, without ``#`` and without the format line."""
    lines: list[str] = []
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        text = raw.strip()
        if not text.startswith("#"):
            break
        body = text[1:].strip()
        if not body.startswith("Format:"):
            lines.append(body)
    return lines


def panel_deck(path: Path, source: str) -> PanelDeck:
    rel = path.relative_to(ROOT).as_posix()
    deck: Deck = load_deck(path)
    problems = validate_deck(deck)
    if problems:
        detail = "; ".join(f"{v.code}: {v.message}" for v in problems)
        raise PanelError(f"{rel} is not legal: {detail}")
    decklist = to_decklist(deck)
    if len(decklist.main) != MAIN_SIZE:
        raise PanelError(f"{rel}: {len(decklist.main)} main-deck cards, expected {MAIN_SIZE}")
    lines = header_lines(path)
    if len(lines) < 2:
        raise PanelError(f"{rel}: the '#' header needs a title line and a plan")
    title, body = lines[0], " ".join(lines[1:])
    archetype: str | None = None
    match = ARCHETYPE.fullmatch(body)
    if match is not None:
        archetype, plan = match["kind"].strip(), match["plan"].strip()
    elif body.startswith("Plan: "):
        plan = body.removeprefix("Plan: ").strip()
    else:
        raise PanelError(
            f"{rel}: the header after the title must start with 'Archetype:' or 'Plan:'"
        )
    if source == "eval" and archetype is None:
        raise PanelError(f"{rel}: an evaluation deck needs a '# Archetype: <kind>. Plan: ...' line")
    db = get_card_db()
    colors = tuple(sorted({c.value for n in decklist.main if (c := db[n].color) is not None}))
    return PanelDeck(
        name=path.stem,
        path=rel,
        source=source,
        title=title,
        archetype=archetype,
        plan=plan,
        colors=colors,
        main=decklist.main,
        digest=deck_digest(deck),
    )


def load_panel() -> list[PanelDeck]:
    examples = [panel_deck(p, "example") for p in sorted(EXAMPLES_DIR.glob("*.txt"))]
    evals = [panel_deck(p, "eval") for p in sorted(EVAL_DIR.glob("*.txt"))]
    if len(examples) != EXAMPLE_DECKS or len(evals) != EVAL_DECKS:
        raise PanelError(
            f"the panel needs {EXAMPLE_DECKS} example and {EVAL_DECKS} evaluation decks, "
            f"found {len(examples)} and {len(evals)}"
        )
    kinds = [d.archetype for d in evals]
    if len(set(kinds)) != len(kinds):
        raise PanelError(f"the evaluation decks' archetypes are not distinct: {kinds}")
    return examples + evals


def comparison_decks(panel: Sequence[PanelDeck]) -> dict[str, tuple[str, ...]]:
    """Every deck an overlap is computed against, by name: panel, tuning, AI test decks."""
    tests = test_decks()
    if len(TUNING_DECKS) != len(TUNING_NAMES) or len(tests) != len(TEST_NAMES):
        raise PanelError("the tuning or AI test deck sets changed size")
    decks = {d.name: d.main for d in panel}
    decks.update((n, d.main) for n, d in zip(TUNING_NAMES, TUNING_DECKS, strict=True))
    decks.update((n, d.main) for n, d in zip(TEST_NAMES, tests, strict=True))
    return decks


def check_w20(decks: dict[str, tuple[str, ...]]) -> None:
    for a, b, expected in W20_OVERLAPS:
        got = copy_overlap(decks[a], decks[b])
        if got != expected:
            raise PanelError(f"overlap {a} / {b} is {got}, W-20 recomputed {expected}")


def check_seed_roles() -> None:
    """PLAN A14's ranges equal gcg-eval's ``SeedRole::bounds``."""
    found = {
        m["role"].lower(): (int(m["first"].replace("_", "")), int(m["last"].replace("_", "")))
        for m in ROLE_BOUNDS.finditer(CONTRACTS_RS.read_text(encoding="utf-8"))
    }
    expected = {role: (first, last) for role, first, last, _ in SEED_ROLES}
    if found != expected:
        raise PanelError(f"seed-role ranges differ from {CONTRACTS_RS.name}: {found}")


def candidates(panel: Sequence[PanelDeck], decks: dict[str, tuple[str, ...]]) -> list[Candidate]:
    db = get_card_db()
    names = {k: tuple(db[n].name for n in v) for k, v in decks.items()}
    bound = (*TUNING_NAMES, *TEST_NAMES)
    out: list[Candidate] = []
    for deck in panel:
        if deck.source != "eval":
            continue
        others = [k for k in decks if k != deck.name]
        copies = [copy_overlap(deck.main, decks[k]) for k in others]
        by_name = [copy_overlap(names[deck.name], names[k]) for k in others]
        max_bound = max(copy_overlap(deck.main, decks[k]) for k in bound)
        out.append(
            Candidate(
                name=deck.name,
                max_bound_overlap=max_bound,
                max_overlap=max(copies),
                total_overlap=sum(copies),
                max_name_overlap=max(by_name),
                total_name_overlap=sum(by_name),
                eligible=max_bound <= OVERLAP_BOUND,
            )
        )
    return out


def held_out(cands: Sequence[Candidate]) -> tuple[list[str], list[Candidate]]:
    ranked = sorted(cands, key=lambda c: (not c.eligible, c.key()))
    chosen = [c for c in ranked if c.eligible][:HELD_OUT_DECKS]
    if len(chosen) < HELD_OUT_DECKS:
        raise PanelError(
            f"only {len(chosen)} evaluation decks have overlap <= {OVERLAP_BOUND}/{MAIN_SIZE} with "
            "every tuning and AI test deck; revise the decks (PLAN §12.5)"
        )
    return sorted(c.name for c in chosen), ranked


def pairs(panel: Sequence[PanelDeck], held: Sequence[str]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for position, (a, b) in enumerate(itertools.combinations(panel, 2)):
        is_held = a.name in held or b.name in held
        out.append(
            {
                "position": position,
                "decks": [a.name, b.name],
                "split": "heldout" if is_held else "tuning",
            }
        )
    return out


def build() -> tuple[dict[str, Any], list[PanelDeck], list[Candidate]]:
    panel = load_panel()
    decks = comparison_decks(panel)
    check_w20(decks)
    check_seed_roles()
    held, ranked = held_out(candidates(panel, decks))
    pair_list = pairs(panel, held)
    n_held = sum(p["split"] == "heldout" for p in pair_list)
    names = list(decks)
    data: dict[str, Any] = {
        "format": "gcg-eval-panel",
        "version": 1,
        "generated_by": SCRIPT,
        "plan": "docs/dev/PLAN.md §12.5, X16, A14; WEAKNESSES W-20",
        "pre_registered": (
            "committed before any tuning run; held-out pairs are never used in tuning, screening "
            "or budget curves, and gcg-eval refuses them under --role tuning|dev|confirm"
        ),
        "held_out": held,
        "counts": {
            "decks": len(panel),
            "pairs": len(pair_list),
            "heldout_pairs": n_held,
            "tuning_pairs": len(pair_list) - n_held,
        },
        "decks": [
            {
                "index": i,
                "name": d.name,
                "path": d.path,
                "source": d.source,
                "archetype": d.title,
                "kind": d.archetype,
                "plan": d.plan,
                "colors": list(d.colors),
                "digest": d.digest,
                "held_out": d.name in held,
            }
            for i, d in enumerate(panel)
        ],
        "pairs": pair_list,
        "pair_master": 'derive_seed(master, "pairing", position) (ai/compare.py:136-140)',
        "seed_roles": [
            {"role": role, "first": first, "last": last, "heldout_pairs": allowed}
            for role, first, last, allowed in SEED_ROLES
        ],
        "selection": {
            "rule": (
                f"evaluation decks with copy overlap <= {OVERLAP_BOUND}/{MAIN_SIZE} with every "
                "tuning deck and every AI test deck, sorted by (maximum copy overlap, total copy "
                "overlap, maximum name overlap, total name overlap, name) over the other twelve "
                f"decks; the first {HELD_OUT_DECKS} are held out"
            ),
            "bound": OVERLAP_BOUND,
            "candidates": [
                {
                    "rank": rank,
                    "name": c.name,
                    "eligible": c.eligible,
                    "max_overlap_tuning_and_tests": c.max_bound_overlap,
                    "max_overlap": c.max_overlap,
                    "total_overlap": c.total_overlap,
                    "max_name_overlap": c.max_name_overlap,
                    "total_name_overlap": c.total_name_overlap,
                    "held_out": c.name in held,
                }
                for rank, c in enumerate(ranked, 1)
            ],
        },
        "overlap": {
            "measure": "main-deck copies shared (multiset intersection of card numbers, W-20)",
            "sources": {"tuning": TUNING_SOURCE, "tests": TEST_SOURCE},
            "decks": names,
            "copies": [[copy_overlap(decks[a], decks[b]) for b in names] for a in names],
        },
    }
    return data, panel, ranked


def render_md(data: dict[str, Any]) -> str:
    held = data["held_out"]
    lines = [
        "# Evaluation panel and the held-out split",
        "",
        f"Generated by `{SCRIPT}` from the deck files; do not edit. The same split is in",
        "`docs/dev/eval/panel.json`, which `gcg-eval` reads (`--decks panel --pairs "
        "tuning|held-out`).",
        "",
        "**Pre-registration.** This split is committed before any tuning run (PLAN §12.5, X16).",
        "Every pair that contains a held-out deck is a held-out pair: it is never used in tuning,",
        "screening or budget curves, and `gcg-eval` refuses it under `--role tuning|dev|confirm`.",
        "The other pairs are tuning pairs. The digest of each deck pins its contents, so editing a",
        f"panel deck makes `uv run python {SCRIPT} --check` fail.",
        "",
        "## Decks",
        "",
        "The panel is the example decks, then the evaluation decks, each sorted by file name (the",
        "order of `gcg-eval --decks panel`).",
        "",
        "| # | Deck | Archetype | Colours | Held out |",
        "| --- | --- | --- | --- | --- |",
    ]
    for d in data["decks"]:
        file_name = Path(d["path"]).name
        link = file_name if d["source"] == "eval" else f"../{file_name}"
        lines.append(
            f"| {d['index']} | [`{d['name']}`]({link}) | {d['archetype']} "
            f"| {'/'.join(d['colors'])} | {'yes' if d['held_out'] else 'no'} |"
        )
    lines += ["", "Game plans:", ""]
    lines += [f"- `{d['name']}`: {d['plan']}" for d in data["decks"]]
    sel = data["selection"]
    lines += [
        "",
        "## Held-out decks",
        "",
        f"Held out: {', '.join(f'`{n}`' for n in held)}.",
        "",
        f"Rule (PLAN §12.5): {sel['rule']}.",
        "",
        "The twelve decks are the other seven panel decks, the tuning decks",
        f"(`{TUNING_SOURCE}`) and the AI test decks (`{TEST_SOURCE}`). Copy overlap counts",
        "main-deck copies of the same card number (W-20); name overlap counts copies with the same",
        "card name, which also covers other printings of the same Unit or Pilot.",
        "",
        "| Rank | Deck | Max overlap with tuning and test decks | Max overlap | Total overlap "
        "| Max name overlap | Total name overlap | Held out |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for c in sel["candidates"]:
        lines.append(
            f"| {c['rank']} | `{c['name']}` | {c['max_overlap_tuning_and_tests']}/{MAIN_SIZE} "
            f"| {c['max_overlap']} | {c['total_overlap']} | {c['max_name_overlap']} "
            f"| {c['total_name_overlap']} | {'yes' if c['held_out'] else 'no'} |"
        )
    counts = data["counts"]
    lines += [
        "",
        "## Pairs",
        "",
        f"All {counts['pairs']} pairs in `itertools.combinations` order of the deck list "
        f"(`ai/compare.py:131`): {counts['heldout_pairs']} held-out pairs and "
        f"{counts['tuning_pairs']} tuning pairs. Pair `p` plays on the master "
        '`derive_seed(master, "pairing", p)` (`ai/compare.py:136-140`).',
        "",
        "| Position | Deck A | Deck B | Split |",
        "| --- | --- | --- | --- |",
    ]
    lines += [
        f"| {p['position']} | `{p['decks'][0]}` | `{p['decks'][1]}` | {p['split']} |"
        for p in data["pairs"]
    ]
    lines += [
        "",
        "## Seed roles",
        "",
        "Master seeds come from disjoint ranges per role (PLAN A14); `gcg-eval --role` rejects a",
        "master outside its role's range and records every master in",
        "`docs/dev/eval/seed-ledger.json`.",
        "",
        "| Role | First master | Last master | Held-out pairs |",
        "| --- | --- | --- | --- |",
    ]
    lines += [
        f"| {r['role']} | {r['first']:_} | {r['last']:_} | {'allowed' if r['heldout_pairs'] else 'refused'} |"
        for r in data["seed_roles"]
    ]
    ov = data["overlap"]
    short = list(ov["decks"])
    lines += [
        "",
        "## Main-deck copy overlaps",
        "",
        "Copies of the same card number shared by two main decks (of 50).",
        "",
        "| | " + " | ".join(str(i) for i in range(len(short))) + " |",
        "| --- | " + " | ".join("---" for _ in short) + " |",
    ]
    for i, (name, row) in enumerate(zip(short, ov["copies"], strict=True)):
        cells = ["-" if i == j else str(v) for j, v in enumerate(row)]
        lines.append(f"| {i} `{name}` | " + " | ".join(cells) + " |")
    lines += [
        "",
        "## Commands",
        "",
        "```bash",
        f"uv run python {SCRIPT}           # regenerate panel.json and this file",
        f"uv run python {SCRIPT} --check   # fail if either is stale or a rule is broken",
        "target/release/gcg-eval head-to-head A B --decks panel --pairs tuning --role dev --seed S",
        "```",
        "",
    ]
    return "\n".join(lines)


def outputs() -> dict[Path, bytes]:
    data, _, _ = build()
    return {
        PANEL_JSON: (json.dumps(data, indent=2, ensure_ascii=False) + "\n").encode("utf-8"),
        PAIRS_MD: render_md(data).encode("utf-8"),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0] if __doc__ else None)
    ap.add_argument("--check", action="store_true", help="fail if the committed files are stale")
    args = ap.parse_args()
    try:
        files = outputs()
    except PanelError as e:
        print(f"eval_decks: {e}", file=sys.stderr)
        return 1
    if args.check:
        stale = [p for p, data in files.items() if not p.is_file() or p.read_bytes() != data]
        if stale:
            shown = ", ".join(p.relative_to(ROOT).as_posix() for p in stale)
            print(f"stale: {shown}; run uv run python {SCRIPT}", file=sys.stderr)
            return 1
        print(f"{', '.join(p.relative_to(ROOT).as_posix() for p in files)} are current")
        return 0
    for path, data in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        print(f"wrote {path.relative_to(ROOT).as_posix()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
