"""Cluster Gundam Card Game effect texts into templates and plan per-set work packages.

Reads the pinned gcg-api snapshot (``src/gcg_sim/data/gcgapi/cards.ndjson``) and writes
``docs/research/effect_templates.json``, ``docs/research/work_packages.json`` and the
generated blocks of ``docs/research/EFFECT_TEMPLATES.md``. Output is byte-identical for a
given snapshot: no timestamps, no randomness, every collection is sorted.

Usage: ``uv run python scripts/cluster_effects.py [--check] [--no-markdown]``
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter, defaultdict
from collections.abc import Callable, Iterable, Iterator
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DATA_DIR = REPO / "src" / "gcg_sim" / "data" / "gcgapi"
OUT_DIR = REPO / "docs" / "research"
TEMPLATES_JSON = OUT_DIR / "effect_templates.json"
PACKAGES_JSON = OUT_DIR / "work_packages.json"
MARKDOWN = OUT_DIR / "EFFECT_TEMPLATES.md"
GCG_API_COMMIT = "f57b7c0b0ebc4c13d359649de19750c793ecbefc"

REGULAR_MIN_CARDS = 2
COVERAGE_KS = (1, 2, 3, 4, 5, 6, 8, 10, 15, 20, 30, 50)
TOP_TEMPLATES_IN_MD = 80
MAX_EXAMPLES = 6

KEYWORD_NAMES = (
    "Repair",
    "Breach",
    "Support",
    "Blocker",
    "First Strike",
    "High-Maneuver",
    "Suppression",
)
MIDDOT = "･"
FULL_COLON = "："

# --------------------------------------------------------------------------------------
# Normalization
# --------------------------------------------------------------------------------------

NORMALIZATION_RULES: list[tuple[str, str]] = [
    (
        "N01",
        "Convert CRLF to LF and delete zero-width characters (U+200B, U+200C, U+200D, U+FEFF).",
    ),
    (
        "N02",
        "Unify middle dots: U+30FB '・' becomes U+FF65 '･' (the majority form); spaces around '･' are removed.",
    ),
    (
        "N03",
        "Unify colons: ASCII ':' becomes U+FF1A '：' (the majority form); spaces around '：' are removed.",
    ),
    (
        "N04",
        "Unify apostrophes: '’'/'‘' become \"'\"; OCR-split contractions such as \"owner' s\", \"won' t\" are rejoined.",
    ),
    (
        "N05",
        "Unify double quotes: '“'/'”' become '\"'; spaces just inside quoted names are trimmed.",
    ),
    (
        "N06",
        "Keyword written with square brackets ('[Suppression]') becomes angle brackets ('<Suppression>').",
    ),
    (
        "N07",
        "Strip explanatory parentheticals (rule 2-11-4): a balanced '(...)' group is removed when it is not "
        "immediately preceded by ']' (token stat block) and its content ends with '.' or has 5+ words. "
        "Traits '(Zeon)', token definitions '[Zaku Ⅱ]((Zeon)･AP1･HP1)', names '[Zeong (Head)]' and stat "
        "blocks '(AP3･HP3)' are kept.",
    ),
    (
        "N08",
        "Whitespace: runs of spaces/tabs collapse to one space, lines are trimmed, empty lines are dropped, "
        "spaces after a leading 【...】 marker chain and before '.'/',' are removed.",
    ),
    (
        "N09",
        "A capitalised verb after 'Then, ' or 'If you do, ' is lower-cased ('Then, Choose' -> 'Then, choose').",
    ),
]

ZERO_WIDTH_RE = re.compile("[\u200b\u200c\u200d\ufeff]")
PAREN_RE = re.compile(r"\((?:[^()]|\([^()]*\))*\)")
SPLIT_CONTRACTION_RE = re.compile(r"(\w)' ([st])\b")
QUOTED_RE = re.compile(r'"\s*([^"\n]*?)\s*"')
BRACKET_KEYWORD_RE = re.compile(
    r"\[(" + "|".join(re.escape(k) for k in KEYWORD_NAMES) + r")( \d+)?\]"
)
LEADING_CHAIN_SPACE_RE = re.compile(r"^((?:【[^】]+】/?)+)\s+", re.M)
SPACE_BEFORE_PUNCT_RE = re.compile(r" +([.,])")
LOWER_AFTER_CONNECTOR_RE = re.compile(
    r"\b(Then|If you do), (Choose|Deal|Draw|Rest|Return|Deploy|Place|Add|Set|Discard|Destroy|Look|Exile)\b"
)


def is_reminder(text: str, match: re.Match[str]) -> bool:
    if match.start() > 0 and text[match.start() - 1] == "]":
        return False
    inner = match.group(0)[1:-1].strip()
    return inner.endswith(".") or len(inner.split()) >= 5


@dataclass
class NormalizationLog:
    rule_hits: Counter[str] = field(default_factory=Counter)
    stripped: Counter[str] = field(default_factory=Counter)
    stripped_cards: dict[str, set[str]] = field(default_factory=lambda: defaultdict(set))


def normalize_text(raw: str, card: str, log: NormalizationLog | None = None) -> str:
    hits: set[str] = set()

    def sub(rule: str, pattern: re.Pattern[str] | str, repl: str, text: str) -> str:
        new = re.sub(pattern, repl, text)
        if new != text:
            hits.add(rule)
        return new

    text = raw.replace("\r\n", "\n")
    text = sub("N01", ZERO_WIDTH_RE, "", text)
    text = sub("N02", r"[ \t]*[・･][ \t]*", MIDDOT, text)
    text = sub("N03", r"[ \t]*[:：][ \t]*", FULL_COLON, text)
    text = sub("N04", "[’‘]", "'", text)
    text = sub("N04", SPLIT_CONTRACTION_RE, r"\1'\2", text)
    text = sub("N05", "[“”]", '"', text)
    text = sub("N05", QUOTED_RE, r'"\1"', text)
    text = sub("N06", BRACKET_KEYWORD_RE, r"<\1\2>", text)

    pieces: list[str] = []
    last = 0
    for match in PAREN_RE.finditer(text):
        if is_reminder(text, match):
            pieces.append(text[last : match.start()])
            last = match.end()
            hits.add("N07")
            if log is not None:
                inner = match.group(0)
                log.stripped[inner] += 1
                log.stripped_cards[inner].add(card)
    pieces.append(text[last:])
    text = "".join(pieces)

    lines = [re.sub(r"[ \t]+", " ", line).strip() for line in text.split("\n")]
    joined = "\n".join(line for line in lines if line)
    joined = LEADING_CHAIN_SPACE_RE.sub(r"\1", joined)
    joined = SPACE_BEFORE_PUNCT_RE.sub(r"\1", joined)
    if joined != text.strip():
        hits.add("N08")
    lowered = LOWER_AFTER_CONNECTOR_RE.sub(lambda m: f"{m.group(1)}, {m.group(2).lower()}", joined)
    if lowered != joined:
        hits.add("N09")
    if log is not None:
        for rule in hits:
            log.rule_hits[rule] += 1
    return lowered


# --------------------------------------------------------------------------------------
# Slot templating
# --------------------------------------------------------------------------------------

TYPE_WORD = r"(?:Unit|Pilot|Command|Base)"
TYPE_CARD = rf"(?:{TYPE_WORD} )?cards?"
TYPE_RE = rf"{TYPE_CARD}(?:/(?:1 )?{TYPE_CARD})*|{TYPE_WORD}s?/(?:enemy )?{TYPE_WORD}s?"
COLOR_RE = r"(?i:\b(?:non-)?(?:blue|green|red|white|purple)\b)"
CMP_RE = r"(?i:equal to or lower than|equal to or less than|or less|or more|or higher|or lower)"
TRAIT_RE = r"\([A-Za-z0-9][A-Za-z0-9 .'&\-]*\)(?:/\([A-Za-z0-9][A-Za-z0-9 .'&\-]*\))*"

SLOT_TYPES: dict[str, tuple[str, str]] = {
    "TOKEN": (
        r"\[[^\[\]]+\]\((?:[^()]|\([^()]*\))*\)",
        "Unit token definition '[Name]((Trait)･APa･HPb[･extra])'",
    ),
    "NAME": (
        r"\[[^\[\]]+\]|\"[^\"]+\"(?:(?:/| or )\"[^\"]+\")*",
        "Card name: '[Pilot Name]' or quoted name fragment '\"Gundam Lfrith\"' (with '/'/' or ' alternatives)",
    ),
    "KW": (
        r"<[^<>]+>(?:/<[^<>]+>)*",
        "Keyword effect '<Blocker>', '<Breach 3>' (with '/' alternatives)",
    ),
    "TIMING": (
        r"【[^】]+】(?:/【[^】]+】)?",
        "Timing keyword referenced inside text: '【Main】', '【Main】/【Action】', '【Destroyed】'",
    ),
    "TRAIT": (TRAIT_RE, "Parenthesised trait '(Zeon)' with '/' alternatives '(Zeon)/(Neo Zeon)'"),
    "COLOR": (COLOR_RE, "Card colour: blue/green/red/white/purple, optionally 'non-'"),
    "CMP": (
        CMP_RE,
        "Comparator: 'or less', 'or more', 'or higher', 'or lower', 'equal to or lower/less than'",
    ),
    "RES": (r"[①-⑨]", "Circled resource cost ①..⑨"),
    "TYPE": (
        TYPE_RE,
        "Card-type phrase: 'Unit card', 'Command cards', 'cards' (counted), 'Unit card/Pilot card', 'Unit/Base'",
    ),
    "ZONE": (
        r"hand|trash",
        "Source zone after 'from your'/'in your'/'in their'/\"any player's\": hand or trash",
    ),
    "N": (r"\d+", "Integer"),
}

SLOT_ORDER = ("TOKEN", "NAME", "KW", "TIMING", "TRAIT", "COLOR", "CMP", "RES", "TYPE", "ZONE", "N")
ZONE_CONTEXT = r"(?:(?<=from your )|(?<=in your )|(?<=in their )|(?<=any player's ))"
TOKENIZER_RE = re.compile(
    "|".join(
        f"(?P<{name}>{ZONE_CONTEXT}(?:{SLOT_TYPES[name][0]}))"
        if name == "ZONE"
        else f"(?P<{name}>{SLOT_TYPES[name][0]})"
        for name in SLOT_ORDER
    )
)
COUNTED_CONTEXT_RE = re.compile(
    r"(?:\d|\)|more|less|higher|lower|blue|red|green|white|purple|each|all|same number of) $"
)
BARE_CARD_RE = re.compile(r"cards?")
SLOT_TOKEN_RE = re.compile(r"\{(" + "|".join(SLOT_ORDER) + r")\}")


def templatize(text: str) -> tuple[str, list[tuple[str, str]]]:
    slots: list[tuple[str, str]] = []
    out: list[str] = []
    last = 0
    for match in TOKENIZER_RE.finditer(text):
        name = match.lastgroup
        assert name is not None
        value = match.group(0)
        if (
            name == "TYPE"
            and BARE_CARD_RE.fullmatch(value)
            and not COUNTED_CONTEXT_RE.search(text[: match.start()])
        ):
            continue
        out.append(text[last : match.start()])
        out.append("{" + name + "}")
        slots.append((name, value))
        last = match.end()
    out.append(text[last:])
    return "".join(out), slots


def template_regex(pattern: str) -> str:
    parts: list[str] = []
    counters: Counter[str] = Counter()
    last = 0
    for match in SLOT_TOKEN_RE.finditer(pattern):
        parts.append(re.escape(pattern[last : match.start()]))
        name = match.group(1)
        counters[name] += 1
        parts.append(f"(?P<{name}{counters[name]}>{SLOT_TYPES[name][0]})")
        last = match.end()
    parts.append(re.escape(pattern[last:]))
    return "^" + "".join(parts) + "$"


# --------------------------------------------------------------------------------------
# Ability / sentence / segment parsing
# --------------------------------------------------------------------------------------

MARKER_CHAIN_RE = re.compile(r"^(?:【[^】]+】/?)+")
MARKER_RE = re.compile(r"【([^】]+)】(/?)")
KEYWORD_LINE_RE = re.compile(r"^(?:<[^<>]+>\s*)+$")
PILOT_NAME_RE = re.compile(r"^\[[^\]]+\]$")
SENTENCE_SPLIT_RE = re.compile(r"(?<=\.)\s+(?=[A-Z\"【<\[(■])")
CONNECTOR_RE = re.compile(r"^(Then|If you do), ")
INSTEAD_RE = re.compile(r" instead\.$")
PREFIX_RULES: list[tuple[str, re.Pattern[str]]] = [
    (
        "scope",
        re.compile(
            r"^(during (?:your turn|your opponent's turn|this turn|this battle|a turn where [^,]+)), ",
            re.I,
        ),
    ),
    ("scope", re.compile(r"^(on the turn this Unit is deployed), ", re.I)),
    ("trigger", re.compile(r"^((?:when|whenever) [^,]+), ", re.I)),
    ("trigger", re.compile(r"^(at the (?:start|end) of [^,]+), ", re.I)),
    ("trigger", re.compile(r"^(after activating [^,]+), ", re.I)),
    ("condition", re.compile(r"^(if [^,]+), ", re.I)),
    ("condition", re.compile(r"^(while [^,]+), ", re.I)),
]

MARKER_KINDS = {
    "Burst": "burst",
    "Deploy": "deploy",
    "Attack": "attack",
    "Destroyed": "destroyed",
    "When Paired": "when_paired",
    "When Linked": "when_linked",
    "During Pair": "during_pair",
    "During Link": "during_link",
    "Once per Turn": "once_per_turn",
    "Main": "main",
    "Action": "action",
    "Pilot": "pilot",
    "Activate": "activate",
}
TRIGGER_MARKERS = {"burst", "deploy", "attack", "destroyed", "when_paired", "when_linked"}


def capitalize_first(text: str) -> str:
    return text[:1].upper() + text[1:] if text else text


@dataclass
class Segment:
    role: str
    text: str
    pattern: str = ""
    slots: list[tuple[str, str]] = field(default_factory=list)
    sel_pattern: str = ""
    selectors: list[str] = field(default_factory=list)


@dataclass
class Sentence:
    text: str
    connector: str | None
    instead: bool
    segments: list[Segment]


@dataclass
class Ability:
    chain: str
    markers: list[str]
    timings: list[str]
    qualifiers: dict[str, str | bool]
    kind: str
    body: str
    cost: list[Segment]
    sentences: list[Sentence]
    options: list[Ability]
    structural: Segment | None = None

    def all_sentences(self) -> Iterator[Sentence]:
        yield from self.sentences
        for option in self.options:
            yield from option.all_sentences()

    def all_segments(self) -> Iterator[Segment]:
        yield from self.cost
        for sentence in self.sentences:
            yield from sentence.segments
        for option in self.options:
            yield from option.all_segments()

    def walk(self) -> Iterator[Ability]:
        yield self
        for option in self.options:
            yield from option.walk()


def split_sentences(body: str) -> list[str]:
    return [s.strip() for s in SENTENCE_SPLIT_RE.split(body) if s.strip()]


def segment_sentence(text: str) -> Sentence:
    rest = text
    connector = None
    match = CONNECTOR_RE.match(rest)
    if match:
        connector = "then" if match.group(1) == "Then" else "if_you_do"
        rest = rest[match.end() :]
    instead = bool(INSTEAD_RE.search(rest))
    if instead:
        rest = INSTEAD_RE.sub(".", rest)
    segments: list[Segment] = []
    for _ in range(6):
        for role, prefix_re in PREFIX_RULES:
            prefix = prefix_re.match(rest)
            if prefix:
                segments.append(Segment(role, capitalize_first(prefix.group(1))))
                rest = rest[prefix.end() :]
                break
        else:
            break
    segments.append(Segment("core", capitalize_first(rest)))
    return Sentence(text, connector, instead, segments)


def parse_marker(marker: str) -> tuple[str, str | None]:
    base, _, qualifier = marker.partition(MIDDOT)
    kind = MARKER_KINDS.get(base.strip(), "unknown")
    if kind == "activate":
        kind = "activate_" + qualifier.strip().lower()
        return kind, None
    return kind, qualifier.strip() or None


def classify_untagged(sentences: list[Sentence], body: str) -> str:
    if re.search(r"name is also treated as", body):
        return "alias"
    if re.search(r"^When playing this card", body):
        return "play_modifier"
    if re.search(r"in your (?:hand|trash) gets|Reduce the cost of this card", body):
        return "cost_modifier"
    first = sentences[0].segments if sentences else []
    if any(seg.role == "trigger" for seg in first):
        return "triggered_text"
    return "constant"


def parse_line(line: str) -> Ability:
    chain_match = MARKER_CHAIN_RE.match(line)
    chain = chain_match.group(0) if chain_match else ""
    body = line[len(chain) :].strip()
    markers: list[str] = []
    timings: list[str] = []
    qualifiers: dict[str, str | bool] = {}
    for marker_match in MARKER_RE.finditer(chain):
        marker = marker_match.group(1)
        markers.append(marker)
        kind, qualifier = parse_marker(marker)
        if kind in {"during_pair", "during_link", "once_per_turn"}:
            qualifiers[kind] = qualifier or True
        elif kind == "pilot":
            qualifiers["pilot"] = True
        else:
            timings.append(kind)
            if qualifier:
                qualifiers[f"{kind}_qualifier"] = qualifier

    if qualifiers.get("pilot") and PILOT_NAME_RE.match(body):
        seg = Segment("pilot_name", line)
        return Ability(chain, markers, timings, qualifiers, "pilot_name", body, [], [], [], seg)
    if KEYWORD_LINE_RE.match(body):
        kind = "activated_keyword" if any(t.startswith("activate_") for t in timings) else "keyword"
        seg = Segment("keyword", body)
        return Ability(chain, markers, timings, qualifiers, kind, body, [], [], [], seg)

    cost: list[Segment] = []
    if any(t.startswith("activate_") for t in timings) and FULL_COLON in body:
        cost_text, _, body = body.partition(FULL_COLON)
        cost = [Segment("cost", capitalize_first(part.strip())) for part in cost_text.split(", ")]
    sentences = [segment_sentence(s) for s in split_sentences(body)]
    if any(t.startswith("activate_") for t in timings):
        kind = "activated"
    elif any(t in {"main", "action"} for t in timings):
        kind = "command"
    elif any(t in TRIGGER_MARKERS for t in timings):
        kind = "triggered"
    else:
        kind = classify_untagged(sentences, body)
    return Ability(chain, markers, timings, qualifiers, kind, body, cost, sentences, [])


def parse_abilities(text: str) -> list[Ability]:
    abilities: list[Ability] = []
    for line in text.split("\n"):
        if line.startswith("■"):
            option = parse_line(line[1:].strip())
            if not option.chain and option.kind == "constant":
                option.kind = "option"
            if abilities:
                abilities[-1].options.append(option)
            else:
                abilities.append(option)
            continue
        abilities.append(parse_line(line))
    return abilities


# --------------------------------------------------------------------------------------
# Card loading
# --------------------------------------------------------------------------------------

FAMILY_ORDER = {
    "GD": 0,
    "ST": 1,
    "EB": 2,
    "SC": 3,
    "T": 4,
    "EXB": 5,
    "EXBP": 6,
    "EXR": 7,
    "EXRP": 8,
    "R": 9,
    "RP": 10,
}


def card_prefix(number: str) -> str:
    return number.split("-", 1)[0]


def card_sort_key(number: str) -> tuple[int, str, int, str]:
    prefix, _, rest = number.partition("-")
    family = re.match(r"[A-Z]+", prefix)
    fam = family.group(0) if family else prefix
    digits = re.sub(r"\D", "", rest)
    return (FAMILY_ORDER.get(fam, 50), prefix, int(digits) if digits else 0, number)


def product_sort_key(product_id: str) -> tuple[int, str]:
    suffix = re.search(r"_p(\d+)$", product_id)
    return (int(suffix.group(1)) if suffix else -1, product_id)


@dataclass
class Card:
    number: str
    product_id: str
    name: str
    card_type: str
    raw_effect: str
    text: str
    abilities: list[Ability]

    @property
    def vanilla(self) -> bool:
        return not self.abilities


def load_records(path: Path) -> list[dict[str, object]]:
    with path.open(encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]


def effect_of(record: dict[str, object]) -> str:
    effect = record.get("effect")
    text = effect if isinstance(effect, str) else ""
    return "" if text.strip() in {"", "-"} else text


def select_canonical(
    records: list[dict[str, object]],
) -> tuple[dict[str, dict[str, object]], list[dict[str, str]]]:
    by_number: dict[str, list[dict[str, object]]] = defaultdict(list)
    for record in records:
        by_number[str(record["card_number"])].append(record)
    canonical: dict[str, dict[str, object]] = {}
    fallbacks: list[dict[str, str]] = []
    for number in sorted(by_number, key=card_sort_key):
        printings = sorted(by_number[number], key=lambda r: product_sort_key(str(r["product_id"])))
        exact = [r for r in printings if r["product_id"] == number]
        if exact:
            canonical[number] = exact[0]
        else:
            canonical[number] = printings[0]
            fallbacks.append({"card_number": number, "product_id": str(printings[0]["product_id"])})
    return canonical, fallbacks


def build_card(record: dict[str, object], log: NormalizationLog | None) -> Card:
    number = str(record["card_number"])
    raw = effect_of(record)
    text = normalize_text(raw, number, log)
    abilities = parse_abilities(text) if text else []
    return Card(
        number,
        str(record["product_id"]),
        str(record["name"]),
        str(record["card_type"]),
        raw,
        text,
        abilities,
    )


def divergent_printings(
    records: list[dict[str, object]], canonical: dict[str, dict[str, object]]
) -> list[dict[str, object]]:
    by_number: dict[str, list[dict[str, object]]] = defaultdict(list)
    for record in records:
        by_number[str(record["card_number"])].append(record)
    result: list[dict[str, object]] = []
    for number in sorted(by_number, key=card_sort_key):
        printings = sorted(by_number[number], key=lambda r: product_sort_key(str(r["product_id"])))
        raw_texts = {effect_of(r) for r in printings}
        if len(raw_texts) <= 1:
            continue
        base = normalize_text(effect_of(canonical[number]), number)
        variants = []
        for record in printings:
            norm = normalize_text(effect_of(record), number)
            if norm != base:
                variants.append(
                    {
                        "product_id": record["product_id"],
                        "set_code": record["set_code"],
                        "normalized": norm,
                    }
                )
        result.append(
            {
                "card_number": number,
                "canonical_product_id": canonical[number]["product_id"],
                "n_printings": len(printings),
                "n_raw_texts": len(raw_texts),
                "differs_after_normalization": bool(variants),
                "canonical_normalized": base,
                "variants": variants,
            }
        )
    return result


# --------------------------------------------------------------------------------------
# Clustering
# --------------------------------------------------------------------------------------


@dataclass
class TemplateStat:
    role: str
    pattern: str
    cards: set[str] = field(default_factory=set)
    occurrences: int = 0
    examples: dict[str, str] = field(default_factory=dict)
    slot_values: dict[str, Counter[str]] = field(default_factory=lambda: defaultdict(Counter))


def iter_card_segments(card: Card) -> Iterator[Segment]:
    for ability in card.abilities:
        yield from ability.all_segments()


def cluster(cards: list[Card]) -> dict[tuple[str, str], TemplateStat]:
    stats: dict[tuple[str, str], TemplateStat] = {}
    for card in cards:
        for seg in iter_card_segments(card):
            seg.pattern, seg.slots = templatize(seg.text)
            seg.sel_pattern, seg.selectors = abstract_selectors(seg.pattern)
            key = (seg.role, seg.pattern)
            stat = stats.setdefault(key, TemplateStat(seg.role, seg.pattern))
            stat.cards.add(card.number)
            stat.occurrences += 1
            stat.examples.setdefault(card.number, seg.text)
            counters: Counter[str] = Counter()
            for name, value in seg.slots:
                counters[name] += 1
                stat.slot_values[f"{name}{counters[name]}"][value] += 1
    return stats


def rank_templates(stats: dict[tuple[str, str], TemplateStat]) -> list[TemplateStat]:
    return sorted(stats.values(), key=lambda s: (-len(s.cards), -s.occurrences, s.role, s.pattern))


def template_key(stat: TemplateStat) -> str:
    return hashlib.sha1(f"{stat.role}\x00{stat.pattern}".encode()).hexdigest()[:10]


def verify_templates(ranked: list[TemplateStat]) -> None:
    for stat in ranked:
        regex = re.compile(template_regex(stat.pattern))
        for number, text in stat.examples.items():
            if not regex.fullmatch(text):
                msg = f"template regex round-trip failed for {number}: {text!r} vs {stat.pattern!r}"
                raise AssertionError(msg)


# --------------------------------------------------------------------------------------
# Compositional layer: selector noun phrases abstracted to {SEL}
# --------------------------------------------------------------------------------------


def slot_literals(regex: str) -> str:
    return re.sub(r"\{([A-Z]+)\}", lambda m: re.escape("{" + m.group(1) + "}"), regex)


SEL_DET = r"(?:(?:{N} to {N}|{N}|a|an|another|all|All|no|each|only {N}|one) (?:of (?:your|their)(?: own)? )?)"
SEL_MOD = r"(?:(?:other|another|active|rested|damaged|undamaged|friendly|Friendly|enemy|Enemy|your|non-battling|{COLOR}|{TRAIT}) )"
SEL_HEAD = r"(?:Unit tokens?|Link Units?|Linked Units|Units?/Bases?|Unit/Base|Units?|Bases?|Pilots?|Resources?|Shields?|shield area cards?|{TYPE})"
SEL_POST = (
    r"(?: that (?:is|are) Lv\.{N}(?: {CMP})?| with {N} {CMP} (?:HP|AP)| with {N} HP| with {KW}| without {KW}"
    r"| with no paired Pilot| that has no Pilot paired with it| paired with an? (?:{COLOR} |{TRAIT} )?Pilot(?: that is Lv\.{N} {CMP})?"
    r"| with the (?:lowest|highest) (?:Lv\.|HP|AP)| whose Lv\. is {CMP} (?:this|that) Unit| with(?:out)? {NAME} in (?:its|their) card names?"
    r"| other than (?:Link Units|Unit tokens)| belonging to (?:each enemy player|another player|an enemy player with the most Units)"
    r"| battling (?:this Unit|a friendly Unit with {KW}|an enemy Unit)| that is battling one of your Units that is Lv\.{N} {CMP}"
    r"| that is being attacked| with a keyword effect| with AP {CMP} this Unit)*"
)
SEL_NOT_AFTER = r"(?<![A-Za-z])(?<!top )(?<!first )(?<!Place )(?<!place )(?<!lace {N} )"
SEL_RE = re.compile(
    slot_literals(rf"{SEL_NOT_AFTER}{SEL_DET}?{SEL_MOD}*{SEL_HEAD}{SEL_POST}(?![A-Za-z])")
)
SEL_HEAD_RE = re.compile(slot_literals(SEL_HEAD))
SELECTOR_GRAMMAR = {
    "determiner": SEL_DET,
    "modifier": SEL_MOD,
    "head": SEL_HEAD,
    "post_filters": SEL_POST,
    "rule": "SEL := determiner? modifier* head post_filter*; a bare head ('Unit' in 'this Unit') is not abstracted",
}


def abstract_selectors(pattern: str) -> tuple[str, list[str]]:
    found: list[str] = []

    def replace(match: re.Match[str]) -> str:
        phrase = match.group(0)
        if SEL_HEAD_RE.fullmatch(phrase):
            return phrase
        found.append(phrase)
        return "{SEL}"

    return SEL_RE.sub(replace, pattern), found


def compositional_stats(
    cards: list[Card],
) -> tuple[dict[tuple[str, str], set[str]], dict[str, set[str]]]:
    by_template: dict[tuple[str, str], set[str]] = defaultdict(set)
    by_phrase: dict[str, set[str]] = defaultdict(set)
    for card in cards:
        for seg in iter_card_segments(card):
            by_template[(seg.role, seg.sel_pattern)].add(card.number)
            for phrase in seg.selectors:
                by_phrase[phrase].add(card.number)
    return by_template, by_phrase


# --------------------------------------------------------------------------------------
# Primitive inventory
# --------------------------------------------------------------------------------------

Detector = tuple[str, str, str, str, str]  # (category, id, description, scope, regex)

DETECTORS: list[Detector] = [
    # --- timing keywords (marker chains) ---
    ("trigger", "trigger.burst", "【Burst】 shield trigger", "marker", r"^Burst$"),
    (
        "trigger",
        "trigger.deploy",
        "【Deploy】 (incl. 【Deploy･Development n】)",
        "marker",
        r"^Deploy(?:･|$)",
    ),
    ("trigger", "trigger.attack", "【Attack】 on attack declaration", "marker", r"^Attack$"),
    (
        "trigger",
        "trigger.destroyed",
        "【Destroyed】 (resolves from trash, rule 13-2-8-2)",
        "marker",
        r"^Destroyed$",
    ),
    (
        "trigger",
        "trigger.when_paired",
        "【When Paired】 (all variants incl. qualified and Development)",
        "marker",
        r"^When Paired",
    ),
    (
        "trigger",
        "trigger.when_paired_qualified",
        "【When Paired･(qualifier)】 pilot trait/colour/Lv qualifier (rule 13-2-9-2)",
        "marker",
        r"^When Paired･(?!Development)",
    ),
    ("trigger", "trigger.when_linked", "【When Linked】", "marker", r"^When Linked"),
    (
        "trigger",
        "trigger.development",
        "･Development n (optional exile n (G Generation) cards, then ■ effect)",
        "marker",
        r"･Development \d",
    ),
    (
        "trigger",
        "trigger.deploy_or_attack",
        "【Deploy】/【Attack】 shared body",
        "chain",
        r"【Deploy】/【Attack】",
    ),
    (
        "timing",
        "timing.activate_main",
        "【Activate･Main】 activated ability",
        "marker",
        r"^Activate･Main$",
    ),
    (
        "timing",
        "timing.activate_action",
        "【Activate･Action】 activated ability",
        "marker",
        r"^Activate･Action$",
    ),
    ("timing", "timing.command_main", "Command 【Main】 only", "chain", r"^【Main】$"),
    ("timing", "timing.command_action", "Command 【Action】 only", "chain", r"^【Action】$"),
    (
        "timing",
        "timing.command_main_or_action",
        "Command 【Main】/【Action】",
        "chain",
        r"【Main】/【Action】",
    ),
    (
        "timing",
        "timing.pilot_name",
        "Command 【Pilot】[Name] pilot mode (rule 3-4-6)",
        "marker",
        r"^Pilot$",
    ),
    (
        "qualifier",
        "qualifier.during_pair",
        "【During Pair】 (any pilot)",
        "marker",
        r"^During Pair$",
    ),
    (
        "qualifier",
        "qualifier.during_pair_qualified",
        "【During Pair･(qualifier)】",
        "marker",
        r"^During Pair･",
    ),
    ("qualifier", "qualifier.during_link", "【During Link】", "marker", r"^During Link$"),
    (
        "qualifier",
        "qualifier.once_per_turn",
        "【Once per Turn】 (rule 13-2-13)",
        "marker",
        r"^Once per Turn$",
    ),
    # --- untagged / text triggers ---
    (
        "trigger",
        "trigger.self_destroys_enemy_unit",
        "when this Unit destroys an enemy Unit with (battle) damage",
        "trigger",
        r"^When this Unit destroys an enemy Unit with (?:battle )?damage",
    ),
    (
        "trigger",
        "trigger.friendly_destroys_enemy_unit",
        "when one of your/a friendly/chosen Unit destroys an enemy Unit with battle damage",
        "trigger",
        r"^When (?:one of your|a friendly|your|it|they)\b.*destroys? an enemy Unit with battle damage",
    ),
    (
        "trigger",
        "trigger.destroys_enemy_shield_card",
        "when (this/one of your) Unit destroys an enemy shield area card",
        "trigger",
        r"destroys an enemy shield area card",
    ),
    (
        "trigger",
        "trigger.destroys_enemy_card",
        "when this Unit/they destroy an enemy card with battle damage",
        "trigger",
        r"destroys? an enemy card with battle damage",
    ),
    (
        "trigger",
        "trigger.deals_battle_damage",
        "when this/your Unit deals battle damage to an enemy Unit",
        "trigger",
        r"deals battle damage to an enemy Unit",
    ),
    (
        "trigger",
        "trigger.unit_deployed",
        "when this Unit / one of your (X) Units / another friendly Unit is deployed",
        "trigger",
        r"is deployed$",
    ),
    (
        "trigger",
        "trigger.self_receives_damage",
        "when this Unit/Base receives (enemy/effect/battle) damage",
        "trigger",
        r"^When this (?:Unit|Base) (?:would )?receives? ",
    ),
    (
        "trigger",
        "trigger.friendly_receives_damage",
        "when one of your / a friendly Unit (token, shield area card) receives damage",
        "trigger",
        r"^When (?:one of your|a friendly|it)\b.*receives? .*damage",
    ),
    ("trigger", "trigger.recovers_hp", "when this Unit recovers HP", "trigger", r"recovers HP$"),
    (
        "trigger",
        "trigger.rested_by_effect",
        "when this/one of your Units is rested by an (opponent's) effect",
        "trigger",
        r"is rested by",
    ),
    (
        "trigger",
        "trigger.set_active_by_effect",
        "when this rested Unit is set as active by an effect",
        "trigger",
        r"is set as active by an effect",
    ),
    (
        "trigger",
        "trigger.pilot_paired",
        "when you pair a (qualified) Pilot with this/one of your Units",
        "trigger",
        r"^When you pair a",
    ),
    ("trigger", "trigger.unit_links", "when a friendly (X) Unit links", "trigger", r"Unit links$"),
    (
        "trigger",
        "trigger.place_ex_resource",
        "when you place an EX Resource",
        "trigger",
        r"^When you place an EX Resource",
    ),
    (
        "trigger",
        "trigger.ex_resource_exiled",
        "when one of your EX Resources is exiled",
        "trigger",
        r"EX Resources is exiled",
    ),
    (
        "trigger",
        "trigger.command_played_with_ex",
        "when you play and activate a (X) Command card using an EX Resource",
        "trigger",
        r"play and activate .*Command card using an EX Resource",
    ),
    (
        "trigger",
        "trigger.command_activated",
        "when you activate a (X) Command's 【Main】/【Action】",
        "trigger",
        r"^When you activate a .*Command's",
    ),
    (
        "trigger",
        "trigger.cost_paid_for_effect",
        "when you pay ① or more for a Unit's effect",
        "trigger",
        r"^When you pay ",
    ),
    (
        "trigger",
        "trigger.support_used",
        "when you use this Unit's <Support> to increase a (X) Unit's AP",
        "trigger",
        r"use this Unit's <Support>",
    ),
    (
        "trigger",
        "trigger.unit_attacks",
        "when another Unit / a Unit with <Repair> attacks",
        "trigger",
        r"attacks(?: an enemy Unit)?$",
    ),
    (
        "trigger",
        "trigger.enemy_destroyed_while_attacking",
        "when an enemy (Link) Unit is destroyed with (effect) damage while this Unit is attacking",
        "trigger",
        r"is destroyed with (?:effect )?damage while this Unit is attacking",
    ),
    (
        "trigger",
        "trigger.friendly_destroyed_by_effect",
        "when one of your Units is destroyed by an effect",
        "trigger",
        r"is destroyed by an effect",
    ),
    (
        "trigger",
        "trigger.ap_reduced_by_enemy",
        "when this Unit's AP is reduced by an enemy effect",
        "trigger",
        r"AP is reduced by an enemy effect",
    ),
    (
        "trigger",
        "trigger.blocked",
        "when this Unit is blocked by an enemy Unit",
        "trigger",
        r"is blocked by",
    ),
    (
        "trigger",
        "trigger.draw_by_effect",
        "when you draw with an effect",
        "trigger",
        r"draw with an effect",
    ),
    (
        "trigger",
        "trigger.end_of_turn",
        "at the end of your turn / a turn",
        "trigger",
        r"^At the end of",
    ),
    (
        "trigger",
        "trigger.start_of_opponent_turn",
        "at the start of your opponent's turn (delayed check)",
        "sentence",
        r"at the start of your opponent's turn",
    ),
    (
        "trigger",
        "trigger.play_this_card",
        "when playing this card (from hand): alternative/modified play cost or modal choice",
        "trigger",
        r"^When playing this card",
    ),
    (
        "trigger",
        "trigger.after_command_main",
        "after activating this card's 【Main】 (Command pairs itself from trash)",
        "trigger",
        r"^After activating this card's",
    ),
    (
        "trigger",
        "trigger.this_effect_destroys",
        "when this effect destroys an enemy Unit (reflexive)",
        "trigger",
        r"^When this effect destroys",
    ),
    (
        "trigger",
        "trigger.rest_substitution",
        "when you (would) rest a Base/Unit with an effect -> rest this instead",
        "trigger",
        r"^When you (?:would )?rest",
    ),
    # --- conditions ---
    (
        "condition",
        "condition.units_in_play_count",
        "N or more / 1 to N (filtered) Units in play (either side)",
        "condition",
        r"\d+ (?:or more |or less |to \d+ )?.*(?:Units?|tokens?|Pilots?|Bases?) (?:is |are )?in play|\d+ or more are in play",
    ),
    (
        "condition",
        "condition.unit_exists",
        "a/an/another (filtered) Unit/Pilot/Base is in play (you have ...)",
        "condition",
        r"\b(?:an?|another)\b(?!.*\bno\b).*\bin play",
    ),
    (
        "condition",
        "condition.none_in_play",
        "no (filtered) Units/tokens/Base in play",
        "condition",
        r"\bno\b.*\bin play",
    ),
    (
        "condition",
        "condition.trash_count",
        "N or more (filtered) cards in your/an enemy's trash",
        "condition",
        r"cards? in (?:your|their) trash|cards with .* in your trash",
    ),
    (
        "condition",
        "condition.player_level",
        "you are Lv.N or higher (player level = resources)",
        "condition",
        r"you are Lv\.\d+",
    ),
    (
        "condition",
        "condition.turn_owner",
        "it is your / your opponent's turn",
        "condition",
        r"it is (?:your|your opponent's) turn",
    ),
    (
        "condition",
        "condition.hand_size",
        "a player has N or more/less cards in hand",
        "sentence",
        r"\d+ or (?:more|less) cards in (?:their|your) hand",
    ),
    (
        "condition",
        "condition.shield_count",
        "N or less (enemy) Shields / a player with N or less Shields",
        "condition",
        r"Shields",
    ),
    (
        "condition",
        "condition.self_damaged",
        "this Unit is damaged",
        "condition",
        r"this(?: Unit)? is damaged",
    ),
    (
        "condition",
        "condition.self_rested",
        "this Unit is rested",
        "condition",
        r"this Unit is rested",
    ),
    (
        "condition",
        "condition.self_stat",
        "this Unit has N (or more) AP / N HP",
        "condition",
        r"this Unit has \d+ (?:or more )?(?:AP|HP)",
    ),
    (
        "condition",
        "condition.self_has_keyword",
        "this Unit has <Keyword>",
        "condition",
        r"this Unit has <",
    ),
    (
        "condition",
        "condition.self_color_or_trait",
        "this (Unit) is (a) colour / (Trait) Unit (Pilot text checks its paired Unit)",
        "condition",
        r"this(?: Unit)? is (?:an? )?(?:\(|blue|red|white|green|purple)|[Tt]his is an? ",
    ),
    (
        "condition",
        "condition.self_is_link",
        "this / it is a Link Unit",
        "sentence",
        r"is a Link Unit",
    ),
    (
        "condition",
        "condition.self_unpaired",
        "a Pilot is not paired with this Unit",
        "condition",
        r"Pilot is not paired",
    ),
    (
        "condition",
        "condition.attack_target_kind",
        "you are attacking the enemy player / an enemy Unit / a damaged enemy Unit",
        "condition",
        r"attacking (?:the enemy player|an enemy Unit|a damaged enemy Unit)",
    ),
    (
        "condition",
        "condition.battling",
        "while this Unit is battling an enemy Unit with a property",
        "condition",
        r"battling",
    ),
    (
        "condition",
        "condition.ex_resource",
        "EX Resource state: none left / opponent has one / used to play this card",
        "condition",
        r"EX Resource",
    ),
    (
        "condition",
        "condition.deployed_from_trash",
        "if you deploy this Unit from your trash",
        "condition",
        r"deploy this Unit from your trash",
    ),
    (
        "condition",
        "condition.destroyed_how",
        "if this Unit is destroyed with battle damage / by your (X) card's effect",
        "condition",
        r"is destroyed (?:with battle damage|by)",
    ),
    (
        "condition",
        "condition.turn_history",
        "event already happened this turn (activated, destroyed, discarded, set active, paid)",
        "sentence",
        r"have activated|has destroyed|has been destroyed|have not set|has discarded|have paid",
    ),
    (
        "condition",
        "condition.effect_result",
        "result of the preceding step (placed/drew/discarded/rested with this effect, revealed card is ...)",
        "condition",
        r"with this effect|this effect rested|^If it is an? (?:card|Unit card|\()",
    ),
    (
        "condition",
        "condition.rested_units_count",
        "N or more (other) rested Units in play (either side)",
        "condition",
        r"rested (?:enemy |friendly )?Units? (?:is |are )?in play",
    ),
    (
        "condition",
        "condition.name_in_play",
        'you have a Unit with "X" in its card name in play',
        "condition",
        r"with \".*\" in its card name.*in play",
    ),
    (
        "condition",
        "condition.target_property",
        "if it (the chosen card) has/is ...",
        "condition",
        r"^If (?:it|they) (?:has|is|are)\b(?! (?:your|a card|a Unit card|an? \())",
    ),
    (
        "condition",
        "condition.multiplayer",
        "multiplayer wording (2+ enemy players, each enemy player, most Units); 1v1 has exactly one enemy player",
        "sentence",
        r"2 or more enemy players|enemy players each|each enemy player|belonging to another player|with the most Units|number of enemy players|All enemy players",
    ),
    # --- costs ---
    ("cost", "cost.resource", "① / ② / ④ pay N resources", "cost", r"^[①-⑨]$"),
    (
        "cost",
        "cost.rest_self",
        "Rest this Unit / Rest this Base",
        "cost",
        r"^Rest this (?:Unit|Base)$",
    ),
    ("cost", "cost.destroy_self", "Destroy this Unit", "cost", r"^Destroy this Unit$"),
    (
        "cost",
        "cost.rest_other_units",
        "Rest N of your (other) (X) Units / 1 friendly Unit",
        "cost",
        r"^Rest \d+ ",
    ),
    (
        "cost",
        "cost.exile_from_trash",
        "Exile N (filtered) cards from/in your trash",
        "cost",
        r"^Exile .*trash",
    ),
    ("cost", "cost.discard", "Discard 1 (filtered) card", "cost", r"^Discard "),
    (
        "cost",
        "cost.destroy_token",
        "Destroy 1 friendly Unit token",
        "cost",
        r"^Destroy \d+ friendly Unit token",
    ),
    (
        "cost",
        "cost.return_self_to_deck",
        "Return this Unit to the bottom of its owner's deck",
        "cost",
        r"[Rr]eturn this Unit to the bottom",
    ),
    (
        "cost",
        "cost.support_keyword",
        "【Activate･Main】<Support n> (rest this Unit: another Unit gets AP+n)",
        "keyword_activated",
        r"<Support",
    ),
    (
        "optional_cost",
        "optional_cost.discard",
        '"(You may) discard N. If you do, ..."',
        "if_you_do_pre",
        r"\b[Dd]iscard \d",
    ),
    (
        "optional_cost",
        "optional_cost.exile_from_trash",
        '"(You may) choose N (X) cards from your trash. Exile them... If you do" (incl. Development)',
        "if_you_do_pre",
        r"[Ee]xile .*from the game",
    ),
    (
        "optional_cost",
        "optional_cost.rest_own",
        '"Choose 1 active friendly X. Rest it. If you do" / "You may rest this Unit"',
        "if_you_do_pre",
        r"^(?:You may )?[Rr]est (?:it|them|this Unit|this Base)\.$",
    ),
    (
        "optional_cost",
        "optional_cost.destroy_own",
        '"(You may) choose 1 of your Units. Destroy it. If you do"',
        "if_you_do_pre",
        r"^Destroy it\.$|destroy this Unit",
    ),
    (
        "optional_cost",
        "optional_cost.damage_own",
        '"Deal 1 damage to this Unit/it. If you do"',
        "if_you_do_pre",
        r"[Dd]eal \d+ damage to (?:this Unit|it)\.$",
    ),
    (
        "optional_cost",
        "optional_cost.pay_resource",
        '"You may pay ①. If you do"',
        "if_you_do_pre",
        r"pay [①-⑨]",
    ),
    (
        "optional_cost",
        "optional_cost.return_revealed",
        '"reveal 1 X from your hand. Return it to the bottom of your deck. If you do"',
        "if_you_do_pre",
        r"Return it to the bottom of your deck",
    ),
    (
        "optional_cost",
        "optional_cost.other_precondition",
        'any other step that gates an "If you do" (draw, add from trash, set active, mill...)',
        "if_you_do_pre",
        r"^(?!.*(?:[Dd]iscard \d|[Ee]xile .*from the game|^(?:You may )?[Rr]est (?:it|them|this Unit|this Base)\.$|^Destroy it\.$|destroy this Unit|[Dd]eal \d+ damage to (?:this Unit|it)\.$|pay [①-⑨]|Return it to the bottom of your deck)).+",
    ),
    # --- selectors ---
    (
        "selector",
        "selector.choose_exact",
        "Choose N (exactly N if possible)",
        "sentence",
        r"\b[Cc]hoose \d+ (?!to \d)",
    ),
    ("selector", "selector.choose_range", "Choose N to M", "sentence", r"\b[Cc]hoose \d+ to \d+"),
    (
        "selector",
        "selector.all_matching",
        "all (your/friendly/enemy/other) Units/Bases matching a filter",
        "sentence",
        r"\b[Aa]ll (?:your |friendly |enemy |other |Units|Bases)",
    ),
    (
        "selector",
        "selector.side_enemy",
        "enemy side filter",
        "sentence",
        r"\benemy (?:Units?|Base|Pilot|Shield|shield area card|card)|belonging to (?:each enemy|another) player",
    ),
    (
        "selector",
        "selector.side_friendly",
        "friendly side filter (friendly / of your / your)",
        "sentence",
        r"\bfriendly\b|\b(?:of )?your (?:other |active |rested |damaged |[a-z]+ )*(?:\(|Units?|Bases?|Resources?|Link|Shields|shield)",
    ),
    (
        "selector",
        "selector.side_any",
        'either side ("Choose 1 Unit", "all Units")',
        "sentence",
        r"\b[Cc]hoose \d+(?: to \d+)? (?:other |rested |active |damaged )*(?:\([^)]*\) )?(?:Units?|Pilot)\b|\bto all (?:Units|Bases)\b|\b[Aa]ll Units\b",
    ),
    ("selector", "selector.trait", "trait filter (X) with '/' alternatives", "sentence", TRAIT_RE),
    ("selector", "selector.color", "colour filter", "sentence", COLOR_RE),
    (
        "selector",
        "selector.level_cmp",
        "Lv.N or lower/higher",
        "sentence",
        r"Lv\.\d+ or (?:lower|higher)",
    ),
    (
        "selector",
        "selector.level_exact",
        "that is Lv.N (exact)",
        "sentence",
        r"that (?:is|are) Lv\.\d+(?! or)",
    ),
    (
        "selector",
        "selector.level_relative",
        "whose Lv. is equal to or lower than this/that Unit/number",
        "sentence",
        r"Lv\. is equal to or lower than",
    ),
    (
        "selector",
        "selector.ap_cmp",
        "with N or less/more AP",
        "sentence",
        r"\d+ or (?:less|more) AP",
    ),
    (
        "selector",
        "selector.ap_relative",
        "AP equal to or less than this Unit",
        "sentence",
        r"AP equal to or less than",
    ),
    (
        "selector",
        "selector.hp_cmp",
        "with N or less HP / with 1 HP",
        "sentence",
        r"\d+ or (?:less|more) HP|with 1 HP",
    ),
    (
        "selector",
        "selector.lowest_highest",
        "with the lowest/highest Lv/HP",
        "sentence",
        r"with the (?:lowest|highest)",
    ),
    (
        "selector",
        "selector.rested",
        "rested filter",
        "sentence",
        r"(?<!lace 1 )\brested (?:enemy |friendly |Units?|Resource|\(|[a-z]+ )",
    ),
    (
        "selector",
        "selector.active",
        "active filter",
        "sentence",
        r"(?<!as )(?<!place 1 )(?<!places 1 )\bactive (?:enemy |friendly |Units?|Base|\(|[a-z]+ Unit)",
    ),
    (
        "selector",
        "selector.damaged",
        "damaged / undamaged filter",
        "sentence",
        r"\b(?:un)?damaged (?:enemy|friendly|active|Unit|\()|friendly damaged|your damaged",
    ),
    (
        "selector",
        "selector.battling",
        "battling / being attacked / non-battling",
        "sentence",
        r"battling|being attacked",
    ),
    (
        "selector",
        "selector.keyword",
        "with/without <Keyword> / with a keyword effect / with a 【Destroyed】 effect",
        "sentence",
        r"with <|without <|with a keyword effect|with a 【",
    ),
    (
        "selector",
        "selector.name",
        'with/without "X" in its card name',
        "sentence",
        r"with(?:out)? \"[^\"]+\"(?:(?:/| or )\"[^\"]+\")* in (?:its|their) card names?",
    ),
    (
        "selector",
        "selector.paired_state",
        "paired with a (X/Lv) Pilot / with no paired Pilot",
        "sentence",
        r"paired with an? |no paired Pilot|no Pilot paired|Units paired with",
    ),
    (
        "selector",
        "selector.link_unit",
        "Link Unit filter (incl. 'other than Link Units')",
        "sentence",
        r"Link(?:ed)? Units?",
    ),
    (
        "selector",
        "selector.token",
        "Unit token filter (incl. 'other than Unit tokens')",
        "sentence",
        r"Unit tokens?",
    ),
    (
        "selector",
        "selector.other",
        "other / another (exclude self)",
        "sentence",
        r"\b(?:other|another)\b",
    ),
    ("selector", "selector.base", "Base as target/filter", "sentence", r"\bBases?\b(?! card)"),
    (
        "selector",
        "selector.pilot_on_field",
        "a Pilot on the field (paired with an enemy Unit / enemy Pilot)",
        "sentence",
        r"Pilot paired with an enemy Unit|enemy Pilot\b",
    ),
    (
        "selector",
        "selector.shield_card",
        "first (N) card(s) in a shield area",
        "sentence",
        r"first (?:\d+ )?cards? in (?:that player's|your opponent's)? ?shield area",
    ),
    (
        "selector",
        "selector.resource",
        "your Resources (not EX)",
        "sentence",
        r"(?<!EX )Resources?\. |of (?:your|their) Resources",
    ),
    (
        "selector",
        "selector.trash_card",
        "card in trash (your / any player's)",
        "sentence",
        r"from (?:your|any player's|their) trash|in your trash from the game",
    ),
    (
        "selector",
        "selector.player",
        "player selector (enemy player / all players / you and X)",
        "sentence",
        r"[Cc]hoose 1 enemy player|^All players|you and the player|All enemy players|[Yy]ou and that player",
    ),
    (
        "selector",
        "selector.deck_top",
        "top N cards of your deck (look/mill)",
        "sentence",
        r"top (?:\d+ cards|card) of your deck",
    ),
    # --- actions ---
    ("action", "action.draw", "draw N", "core", r"\b[Dd]raw (?:\d+|a number)"),
    (
        "action",
        "action.discard",
        "discard N (you / enemy player)",
        "core",
        r"\b[Dd]iscard (?:\d+|the same)|\b[Tt]hey discard|may discard",
    ),
    (
        "action",
        "action.damage_fixed",
        "deal N damage to chosen/all/this",
        "core",
        r"[Dd]eal \d+ damage",
    ),
    (
        "action",
        "action.damage_variable",
        "damage equal to a count / per 4 AP",
        "core",
        r"damage (?:to it )?equal to|for each \d+ AP|[Dd]eal an amount of damage|[Dd]eal damage equal",
    ),
    ("action", "action.damage_all", "deal N damage to all matching", "core", r"damage to all"),
    (
        "action",
        "action.destroy",
        "destroy chosen/all/that/first N shields",
        "core",
        r"\b[Dd]estroy (?:it|them|that enemy Unit|all|the first)\b",
    ),
    (
        "action",
        "action.rest",
        "rest chosen / rest all Units",
        "core",
        r"\b[Rr]est (?:it|them|the enemy Unit|all Units)\b",
    ),
    (
        "action",
        "action.set_active",
        "set chosen/this Unit/Resource as active",
        "core",
        r"[Ss]et (?:it|them|this Unit) as active",
    ),
    (
        "action",
        "action.return_to_hand",
        "return chosen card / paired Pilot to its owner's (or your) hand (bounce)",
        "core",
        r"[Rr]eturn .*to (?:its|their|your) (?:owner's |owners' )?hands?",
    ),
    (
        "action",
        "action.return_to_deck",
        "return to top/bottom of owner's deck (incl. shuffle)",
        "core",
        r"to the (?:top|bottom) of (?:its|their) owner's deck|owner's deck and shuffle",
    ),
    (
        "action",
        "action.deploy_token",
        "deploy N (rested) Unit token(s) '[Name]((Trait)･AP･HP)'",
        "core",
        r"[Dd]eploy .*Unit tokens?",
    ),
    (
        "action",
        "action.add_shield_to_hand",
        "Add 1 of your Shields to your hand (Base 【Deploy】)",
        "core",
        r"Add 1 of your Shields to your hand",
    ),
    (
        "action",
        "action.burst_deploy_self",
        "【Burst】Deploy this card.",
        "core",
        r"^Deploy this card\.$",
    ),
    (
        "action",
        "action.burst_add_self",
        "【Burst】Add this card to your hand.",
        "core",
        r"^Add this card to your hand\.$",
    ),
    (
        "action",
        "action.activate_own_command",
        "Activate this card's 【Main】/【Action】",
        "core",
        r"^Activate this card's 【",
    ),
    (
        "action",
        "action.look_top",
        "look at the top N cards of your deck",
        "core",
        r"[Ll]ook at the top (?:\d+ cards|card) of your deck",
    ),
    (
        "action",
        "action.reveal_add_to_hand",
        "reveal matching card(s) among them/it and add to hand",
        "core",
        r"reveal .*add (?:it|them) to (?:your|their) hand",
    ),
    (
        "action",
        "action.deploy_among_looked",
        "you may deploy 1 matching Unit card among them",
        "core",
        r"You may deploy \d+ .* among them",
    ),
    (
        "action",
        "action.bottom_rest_random",
        "return the remaining cards randomly to the bottom of your deck",
        "core",
        r"remaining cards randomly to the bottom",
    ),
    (
        "action",
        "action.scry",
        "return it to the top or bottom / return 1 to the top",
        "core",
        r"top or bottom|return 1 to the top|top of your deck or place it into your trash",
    ),
    (
        "action",
        "action.mill",
        "place the top N cards of your deck into your trash",
        "core",
        r"[Pp]lace the top (?:\d+ cards|card) of your deck into your trash",
    ),
    (
        "action",
        "action.add_from_trash",
        "choose card from trash, add it to hand",
        "ability",
        r"from (?:your|any player's) trash\. Add it to your hand|from your trash and add it to your hand|add it from your trash to your hand|Add \d+ .*you placed from your deck with this effect to your hand",
    ),
    (
        "action",
        "action.deploy_from_trash",
        "choose Unit/Base card from trash: deploy it (pay cost / free / rested)",
        "ability",
        r"from your trash\. (?:Pay its cost to deploy it|Deploy it)",
    ),
    (
        "action",
        "action.deploy_from_hand",
        "deploy 1 (filtered) Unit/Base card from your hand (free)",
        "core",
        r"[Dd]eploy \d+ .* from your hand",
    ),
    (
        "action",
        "action.pair_pilot",
        "pair a Pilot/this Command from hand or trash with a Unit",
        "core",
        r"\bpair (?:1 |this card|that card|it)|Pair it with this Unit",
    ),
    (
        "action",
        "action.exile",
        "exile (remove from game, rule 5-12) chosen cards / this card",
        "core",
        r"[Ee]xile .*from the game",
    ),
    (
        "action",
        "action.shuffle_into_deck",
        "return cards from trash to deck and shuffle",
        "core",
        r"deck and shuffle",
    ),
    ("action", "action.ap_plus", "AP+N (temporary or static)", "core", r"AP\+\d"),
    ("action", "action.ap_minus", "AP-N / reduce its AP", "core", r"AP-\d|[Rr]educe its AP"),
    ("action", "action.hp_plus", "HP+N", "core", r"HP\+\d"),
    (
        "action",
        "action.stat_scaling",
        "stat/cost change scaled by a count ('for each', 'equal to the number of')",
        "sentence",
        r"for each|by an amount equal to|Increase this Unit's AP|same number of <",
    ),
    (
        "action",
        "action.gain_keyword",
        "gain/get <Keyword> (temporary or static)",
        "core",
        r"(?:gains?|gets) (?:AP\+\d+ and )?<",
    ),
    ("action", "action.gain_trait", "gain (Trait)", "core", r"gain \("),
    (
        "action",
        "action.cant_attack",
        "can't attack / can only attack when ...",
        "sentence",
        r"can't attack|can't be paired with a Pilot or attack|can only attack|This Unit can't attack",
    ),
    (
        "action",
        "action.cant_attack_player",
        "can't choose the enemy player as its attack target",
        "core",
        r"can't choose the (?:enemy player|same enemy player)",
    ),
    (
        "action",
        "action.cant_block",
        "Units can't activate <Blocker>",
        "core",
        r"can't activate <Blocker>",
    ),
    (
        "action",
        "action.attack_active_unit",
        "may choose an active enemy Unit (filtered) as its attack target",
        "core",
        r"may choose an? (?:damaged )?active enemy Unit",
    ),
    (
        "action",
        "action.attack_on_deploy_turn",
        "may attack on the turn deployed / may attack a rested Unit on the deploy turn",
        "sentence",
        r"may attack on the turn it is deployed|it may choose a rested enemy Unit as its attack target and attack it",
    ),
    (
        "action",
        "action.forced_attack_target",
        "enemy Units must choose this/that rested Unit as attack target if possible",
        "core",
        r"as their attack target if possible|must choose that Unit as their attack target",
    ),
    (
        "action",
        "action.untargetable_by_attack",
        "enemy Units can't choose this/it as their attack target",
        "core",
        r"can't choose (?:this Unit|it) as their attack target",
    ),
    (
        "action",
        "action.change_attack_target",
        "change a battling enemy Unit's attack target to chosen Unit",
        "core",
        r"[Cc]hange (?:a battling enemy Unit's attack target|the attack target)",
    ),
    (
        "action",
        "action.damage_immunity",
        "can't receive (battle/effect) damage (from filtered sources)",
        "core",
        r"can't receive",
    ),
    (
        "action",
        "action.damage_reduction",
        "reduce (the next / battle) damage by N",
        "core",
        r"[Rr]educe (?:it\b|the next damage|battle damage)|[Rr]educe by \d",
    ),
    (
        "action",
        "action.damage_redirect_or_negate",
        "damage dealt to another Unit instead / doesn't receive that damage",
        "core",
        r"dealt to (?:that Unit|this Unit) instead|doesn't receive that damage",
    ),
    (
        "action",
        "action.indestructible_by_effects",
        "can't be destroyed by enemy effects",
        "core",
        r"can't be destroyed",
    ),
    (
        "action",
        "action.ap_unreducible",
        "AP can't be reduced by enemy effects",
        "core",
        r"AP can't be reduced",
    ),
    (
        "action",
        "action.freeze",
        "won't be set as active during the (next) start phase",
        "core",
        r"won't be set as active",
    ),
    (
        "action",
        "action.recover_hp",
        "recover N HP",
        "core",
        r"recovers? (?:\d+ )?HP|recover \d+ HP",
    ),
    (
        "action",
        "action.place_ex_resource",
        "place N (rested) EX Resource(s)",
        "core",
        r"[Pp]laces? \d+ (?:rested )?EX Resources?",
    ),
    (
        "action",
        "action.place_resource",
        "place 1 rested Resource (ramp from resource deck)",
        "core",
        r"[Pp]lace \d+ rested Resource",
    ),
    ("action", "action.deploy_ex_base", "deploy 1 EX Base", "core", r"Deploy 1 EX Base"),
    (
        "action",
        "action.set_resource_active",
        "choose N of your Resources, set them as active",
        "ability",
        r"Resources?\. Set (?:it|them) as active",
    ),
    (
        "action",
        "action.cost_modifier",
        "this card in hand/trash gets cost -N (and Lv. -N)",
        "core",
        r"gets (?:Lv\. -\d+ and )?cost -\d|[Rr]educe the cost of this card",
    ),
    (
        "action",
        "action.play_as_if",
        "play this card as if it has N Lv. and cost (alternative play)",
        "core",
        r"as if it has",
    ),
    (
        "action",
        "action.begin_battle",
        "begin a battle between two Units and only perform the damage step (rule 5-22-3/4)",
        "core",
        r"[Bb]egin a battle",
    ),
    (
        "action",
        "action.modal_choice",
        "choose 1 of the following effects (■ options)",
        "core",
        r"[Cc]hoose 1 of the following effects",
    ),
    (
        "action",
        "action.grant_ability",
        "chosen Unit gains the following effect (■ granted ability)",
        "core",
        r"gains the following effect",
    ),
    (
        "action",
        "action.name_alias",
        "this card's name is also treated as [X]",
        "core",
        r"name is also treated as",
    ),
    (
        "action",
        "action.deploy_as_unit",
        "deploy a Pilot as an (APn･HPn) Unit instead",
        "sentence",
        r"deploy it as an",
    ),
    (
        "action",
        "action.activate_other_main",
        "activate 【Main】 of the paired / discarded Command card",
        "core",
        r"[Aa]ctivate 【Main】 on the card paired|activate its 【Main】",
    ),
    (
        "action",
        "action.copy_keywords",
        "gets all keywords of a chosen trash card",
        "core",
        r"all <Repair>/<Breach>",
    ),
    (
        "action",
        "action.deployed_rested_static",
        "matching Units are deployed rested (static)",
        "core",
        r"are deployed rested",
    ),
    (
        "action",
        "action.opponent_exiles_trash",
        "enemy player chooses N Unit cards from their trash and exiles them",
        "sentence",
        r"That player chooses \d+ Unit cards from their trash",
    ),
    (
        "action",
        "action.symmetric_effect",
        "effect applied to all players / you and another player",
        "core",
        r"^All players|[Yy]ou and the player who destroyed|[Tt]hey may draw|^You and that player",
    ),
    (
        "action",
        "action.return_paired_pilot",
        "return the card/Pilot paired with this Unit",
        "core",
        r"paired Pilot to|card paired with this Unit to|Pilot paired with this Unit to",
    ),
    (
        "action",
        "action.pilot_removal",
        "destroy / return an enemy Pilot",
        "sentence",
        r"Pilot paired with an enemy Unit|enemy Pilot\b",
    ),
    (
        "action",
        "action.self_destroy",
        "destroy this Unit (as effect)",
        "core",
        r"[Dd]estroy this(?: Unit)?\b",
    ),
    (
        "action",
        "action.reveal_from_hand",
        "reveal a card from your hand",
        "core",
        r"[Rr]eveal \d+ .* from your hand",
    ),
    (
        "action",
        "action.optional",
        "optional step: you may / they may",
        "sentence",
        r"\b[Yy]ou may\b(?! choose (?:an?|the) (?:active|rested|damaged))|\b[Tt]hey may\b|\bmay recover\b|player may discard",
    ),
    (
        "action",
        "action.substitution",
        "substitution wording: '... instead' / 'instead of' (rule 10-1-9)",
        "sentence",
        r"\binstead\b",
    ),
    # --- durations ---
    (
        "duration",
        "duration.this_turn",
        "during this turn",
        "sentence",
        r"during this turn|this turn\b",
    ),
    ("duration", "duration.this_battle", "during this battle", "sentence", r"during this battle"),
    (
        "duration",
        "duration.until_next_start_phase",
        "won't be set active during the start phase of the opponent's next turn",
        "sentence",
        r"start phase of (?:your opponent's next turn|their turn)",
    ),
    (
        "duration",
        "duration.next_damage",
        "the next damage it receives (one-shot shield)",
        "sentence",
        r"the next damage",
    ),
    (
        "duration",
        "duration.your_turn",
        "during your turn (scope of static/trigger)",
        "sentence",
        r"[Dd]uring your turn",
    ),
    (
        "duration",
        "duration.opponent_turn",
        "during your opponent's turn (scope of static/trigger)",
        "sentence",
        r"[Dd]uring your opponent's turn",
    ),
    (
        "duration",
        "duration.while",
        "while <condition> (continuous)",
        "sentence",
        r"^While |, while |\bwhile (?:you|this|there|a|an|no)\b",
    ),
    (
        "duration",
        "duration.on_deploy_turn",
        "on the turn this Unit is deployed",
        "sentence",
        r"[Oo]n the turn (?:this Unit|it) is deployed",
    ),
]

DETECTORS += [
    (
        "keyword",
        f"keyword.{name.lower().replace(' ', '_')}.printed",
        f"printed <{name}> keyword line",
        "keyword_line",
        rf"<{name}(?: \d+)?>",
    )
    for name in KEYWORD_NAMES
]
DETECTORS += [
    (
        "keyword",
        f"keyword.{name.lower().replace(' ', '_')}.granted",
        f"<{name}> granted by an effect (gains/gets, static or temporary)",
        "core",
        rf"(?:gains?|gets)\b[^.]*<{name}",
    )
    for name in KEYWORD_NAMES
]
DETECTORS += [
    (
        "keyword",
        f"keyword.{name.lower().replace(' ', '_')}.referenced",
        f"<{name}> referenced as a filter or condition (with/without/has/activate)",
        "sentence",
        rf"(?:with|without|has|activate|use this Unit's) <{name}",
    )
    for name in KEYWORD_NAMES
]
DETECTORS += [
    (
        "reference",
        "reference.it_them",
        "back-reference to the previously chosen card(s): it / them / its / they",
        "core",
        r"\b(?:[Ii]t|[Tt]hem|[Ii]ts|[Tt]hey)\b",
    ),
    (
        "reference",
        "reference.this_unit",
        "self-reference: this Unit / this Base / this card / this",
        "sentence",
        r"\b[Tt]his (?:Unit|Base|card)\b|\b[Tt]his (?:is|gets|gains)\b",
    ),
    (
        "reference",
        "reference.that_unit",
        "reference to a Unit named earlier in the sentence (that Unit / the enemy Unit / the attacking Unit)",
        "sentence",
        r"\bthat (?:enemy |friendly )?Unit\b|\bthe (?:enemy|attacking) Unit\b|That friendly Unit",
    ),
    (
        "reference",
        "reference.paired_card",
        "the card / Pilot paired with this Unit",
        "sentence",
        r"paired with this Unit|this Unit's paired Pilot",
    ),
]

CONNECTOR_DETECTORS = (
    ("connector", "connector.then", "'Then, ...' (rule 5-20-2)"),
    ("connector", "connector.if_you_do", "'If you do, ...' (rule 5-20-1)"),
)


def scope_texts(card: Card) -> dict[str, list[str]]:
    texts: dict[str, list[str]] = defaultdict(list)
    for top in card.abilities:
        for ability in top.walk():
            texts["marker"].extend(ability.markers)
            if ability.chain:
                texts["chain"].append(ability.chain)
            if ability.kind == "activated_keyword":
                texts["keyword_activated"].append(ability.body)
            if ability.structural is not None and ability.kind in {"keyword", "activated_keyword"}:
                texts["keyword_line"].append(ability.body)
            texts["ability"].append(ability.body)
            texts["cost"].extend(seg.text for seg in ability.cost)
            previous: Sentence | None = None
            for sentence in ability.sentences:
                texts["sentence"].append(sentence.text)
                for seg in sentence.segments:
                    key = "condition" if seg.role in {"condition", "scope"} else seg.role
                    texts[key].append(seg.text)
                if sentence.connector:
                    texts[f"connector.{sentence.connector}"].append(sentence.text)
                if sentence.connector == "if_you_do" and previous is not None:
                    texts["if_you_do_pre"].append(previous.segments[-1].text)
                previous = sentence
    return texts


def primitive_inventory(cards: list[Card]) -> dict[str, list[dict[str, object]]]:
    compiled = [(cat, pid, desc, scope, re.compile(rx)) for cat, pid, desc, scope, rx in DETECTORS]
    hits: dict[str, set[str]] = defaultdict(set)
    occurrences: Counter[str] = Counter()
    per_card_scopes = {card.number: scope_texts(card) for card in cards}
    for card in cards:
        texts = per_card_scopes[card.number]
        for _cat, pid, _desc, scope, regex in compiled:
            n = sum(len(regex.findall(t)) for t in texts.get(scope, []))
            if n:
                hits[pid].add(card.number)
                occurrences[pid] += n
        for _cat, pid, _desc in CONNECTOR_DETECTORS:
            n = len(texts.get(pid, []))
            if n:
                hits[pid].add(card.number)
                occurrences[pid] += n
    inventory: dict[str, list[dict[str, object]]] = defaultdict(list)
    rows: list[tuple[str, str, str, str]] = [(c, p, d, s) for c, p, d, s, _ in DETECTORS]
    rows += [(c, p, d, "sentence") for c, p, d in CONNECTOR_DETECTORS]
    for cat, pid, desc, scope in rows:
        numbers = sorted(hits.get(pid, set()), key=card_sort_key)
        inventory[cat].append(
            {
                "id": pid,
                "description": desc,
                "scope": scope,
                "n_cards": len(numbers),
                "occurrences": occurrences.get(pid, 0),
                "examples": numbers[:MAX_EXAMPLES],
                "card_numbers": numbers,
            }
        )
    return dict(inventory)


def ability_kind_counts(cards: list[Card]) -> dict[str, dict[str, object]]:
    kinds: dict[str, set[str]] = defaultdict(set)
    counts: Counter[str] = Counter()
    for card in cards:
        for top in card.abilities:
            for ability in top.walk():
                kinds[ability.kind].add(card.number)
                counts[ability.kind] += 1
    return {
        kind: {
            "n_cards": len(kinds[kind]),
            "occurrences": counts[kind],
            "examples": sorted(kinds[kind], key=card_sort_key)[:MAX_EXAMPLES],
        }
        for kind in sorted(kinds, key=lambda k: (-counts[k], k))
    }


# --------------------------------------------------------------------------------------
# Coverage and irregulars
# --------------------------------------------------------------------------------------


SegKey = Callable[[Segment], tuple[str, str]]
Freq = dict[tuple[str, str], int]


def flat_key(seg: Segment) -> tuple[str, str]:
    return (seg.role, seg.pattern)


def sel_key(seg: Segment) -> tuple[str, str]:
    return (seg.role, seg.sel_pattern)


def card_units(card: Card) -> Iterator[tuple[int, str, list[Segment]]]:
    """Yield (ability index, text, segments) for every sentence and cost clause of a card."""
    for index, top in enumerate(card.abilities):
        for ability in top.walk():
            for seg in ability.cost:
                yield index, f"[cost] {seg.text}", [seg]
            for sentence in ability.sentences:
                yield index, sentence.text, sentence.segments


def unit_regular(segs: list[Segment], freq: Freq, key: SegKey, k: int = REGULAR_MIN_CARDS) -> bool:
    return all(freq[key(s)] >= k for s in segs)


def card_ok(card: Card, freq: Freq, key: SegKey, k: int = REGULAR_MIN_CARDS) -> bool:
    return all(unit_regular(segs, freq, key, k) for _, _, segs in card_units(card))


@dataclass
class LevelSummary:
    counts: dict[str, int]
    irregular: list[dict[str, object]]
    irregular_cards: list[str]
    coverage_curve: list[dict[str, int]]


def level_summary(
    nonvanilla: list[Card], freq: Freq, key: SegKey, ids: dict[tuple[str, str], str]
) -> LevelSummary:
    total = 0
    templated = 0
    irregular: list[dict[str, object]] = []
    irregular_cards: set[str] = set()
    for card in nonvanilla:
        for index, text, segs in card_units(card):
            total += 1
            if unit_regular(segs, freq, key):
                templated += 1
                continue
            irregular_cards.add(card.number)
            irregular.append(
                {
                    "card_number": card.number,
                    "ability_index": index,
                    "text": text,
                    "irregular_segments": [
                        {"role": s.role, "text": s.text, "template": ids[key(s)]}
                        for s in segs
                        if freq[key(s)] < REGULAR_MIN_CARDS
                    ],
                }
            )
    all_segs = [s for c in nonvanilla for _, _, segs in card_units(c) for s in segs]
    counts = {
        "n_templates": len(freq),
        "n_templates_regular": sum(1 for n in freq.values() if n >= REGULAR_MIN_CARDS),
        "sentences_total": total,
        "sentences_templated": templated,
        "segments_total": len(all_segs),
        "segments_templated": sum(1 for s in all_segs if freq[key(s)] >= REGULAR_MIN_CARDS),
        "cards_fully_templated": sum(1 for c in nonvanilla if card_ok(c, freq, key)),
        "n_irregular_cards": len(irregular_cards),
        "n_irregular_sentences": len(irregular),
    }
    curve = [
        {
            "k": k,
            "templates_with_freq_ge_k": sum(1 for n in freq.values() if n >= k),
            "sentences_covered": sum(
                1
                for c in nonvanilla
                for _, _, segs in card_units(c)
                if unit_regular(segs, freq, key, k)
            ),
            "nonvanilla_cards_fully_covered": sum(
                1 for c in nonvanilla if card_ok(c, freq, key, k)
            ),
        }
        for k in COVERAGE_KS
    ]
    return LevelSummary(counts, irregular, sorted(irregular_cards, key=card_sort_key), curve)


# --------------------------------------------------------------------------------------
# Work packages
# --------------------------------------------------------------------------------------

SPLIT_SETS = ("GD01", "GD02", "GD03", "GD04", "GD05", "EB01")
GROUPED_SETS: list[tuple[str, tuple[str, ...]]] = [
    ("WP-ST01-04", ("ST01", "ST02", "ST03", "ST04")),
    ("WP-ST05-08", ("ST05", "ST06", "ST07", "ST08")),
    ("WP-ST09-11", ("ST09", "ST10", "ST11")),
    ("WP-ST12-14", ("ST12", "ST13", "ST14")),
    ("WP-TOKENS", ("T",)),
    ("WP-EX", ("EXB", "EXBP", "EXR", "EXRP")),
    ("WP-RESOURCES", ("R", "RP")),
]
EFFORT_WEIGHTS = {
    "per_nonvanilla_card": 0.5,
    "per_templated_sentence": 0.5,
    "per_selector_grammar_sentence": 1.5,
    "per_irregular_sentence": 3.0,
}
EFFORT_SIZES = ((15.0, "XS"), (60.0, "S"), (110.0, "M"), (160.0, "L"))


def effort_size(points: float) -> str:
    for limit, size in EFFORT_SIZES:
        if points < limit:
            return size
    return "XL"


def card_effort(card: Card, freq: Freq, freq_sel: Freq) -> float:
    if card.vanilla:
        return 0.0
    points = EFFORT_WEIGHTS["per_nonvanilla_card"]
    for _, _, segs in card_units(card):
        if unit_regular(segs, freq, flat_key):
            points += EFFORT_WEIGHTS["per_templated_sentence"]
        elif unit_regular(segs, freq_sel, sel_key):
            points += EFFORT_WEIGHTS["per_selector_grammar_sentence"]
        else:
            points += EFFORT_WEIGHTS["per_irregular_sentence"]
    return points


def balanced_split(members: list[Card], freq: Freq, freq_sel: Freq) -> int:
    """Index splitting a set into two contiguous card-number ranges with the closest effort."""
    efforts = [card_effort(c, freq, freq_sel) for c in members]
    total = sum(efforts)
    lo, hi = len(members) // 4, (3 * len(members)) // 4
    best_index, best_gap = lo, float("inf")
    running = sum(efforts[:lo])
    for index in range(lo, hi + 1):
        gap = abs(total - 2 * running)
        if gap < best_gap:
            best_index, best_gap = index, gap
        if index < len(efforts):
            running += efforts[index]
    return best_index


def build_packages(cards: list[Card], freq: Freq, freq_sel: Freq) -> list[dict[str, object]]:
    by_prefix: dict[str, list[Card]] = defaultdict(list)
    for card in cards:
        by_prefix[card_prefix(card.number)].append(card)
    plan: list[tuple[str, list[str], list[Card]]] = []
    used: set[str] = set()
    for prefix in SPLIT_SETS:
        members = sorted(by_prefix.get(prefix, []), key=lambda c: card_sort_key(c.number))
        if not members:
            continue
        half = balanced_split(members, freq, freq_sel)
        plan.append((f"WP-{prefix}-A", [prefix], members[:half]))
        plan.append((f"WP-{prefix}-B", [prefix], members[half:]))
        used.add(prefix)
    for package_id, prefixes in GROUPED_SETS:
        members = [c for p in prefixes for c in by_prefix.get(p, [])]
        if members:
            plan.append(
                (
                    package_id,
                    [p for p in prefixes if p in by_prefix],
                    sorted(members, key=lambda c: card_sort_key(c.number)),
                )
            )
        used.update(prefixes)
    for prefix in sorted(set(by_prefix) - used, key=lambda p: card_sort_key(p + "-000")):
        plan.append(
            (
                f"WP-{prefix}",
                [prefix],
                sorted(by_prefix[prefix], key=lambda c: card_sort_key(c.number)),
            )
        )

    packages: list[dict[str, object]] = []
    for package_id, prefixes, members in plan:
        nonvanilla = [c for c in members if not c.vanilla]
        irregular_sentences = 0
        templated_sentences = 0
        binding_sentences = 0
        irregular_cards: list[str] = []
        binding_cards: list[str] = []
        template_ids: set[tuple[str, str]] = set()
        for card in nonvanilla:
            bad = 0
            needs_binding = 0
            for _, _, segs in card_units(card):
                template_ids.update(flat_key(s) for s in segs)
                if unit_regular(segs, freq, flat_key):
                    templated_sentences += 1
                else:
                    bad += 1
                if not unit_regular(segs, freq_sel, sel_key):
                    needs_binding += 1
            irregular_sentences += bad
            binding_sentences += needs_binding
            if bad:
                irregular_cards.append(card.number)
            if needs_binding:
                binding_cards.append(card.number)
        points = sum(card_effort(c, freq, freq_sel) for c in members)
        packages.append(
            {
                "id": package_id,
                "sets": prefixes,
                "range": f"{members[0].number}..{members[-1].number}",
                "card_numbers": [c.number for c in members],
                "n_cards": len(members),
                "n_nonvanilla": len(nonvanilla),
                "n_sentences": templated_sentences + irregular_sentences,
                "n_templated_sentences": templated_sentences,
                "n_irregular_sentences": irregular_sentences,
                "n_irregular_sentences_after_selector_grammar": binding_sentences,
                "n_distinct_templates": len(template_ids),
                "irregular_card_numbers": irregular_cards,
                "per_card_binding_card_numbers": binding_cards,
                "effort_points": round(points, 1),
                "est_effort": effort_size(points),
            }
        )
    return packages


def verify_partition(packages: list[dict[str, object]], cards: list[Card]) -> None:
    seen: Counter[str] = Counter()
    for package in packages:
        numbers = package["card_numbers"]
        assert isinstance(numbers, list)
        seen.update(str(n) for n in numbers)
    expected = {c.number for c in cards}
    duplicates = sorted(n for n, c in seen.items() if c > 1)
    missing = sorted(expected - set(seen))
    extra = sorted(set(seen) - expected)
    if duplicates or missing or extra:
        msg = f"package partition broken: duplicates={duplicates} missing={missing} extra={extra}"
        raise AssertionError(msg)


# --------------------------------------------------------------------------------------
# Output assembly
# --------------------------------------------------------------------------------------


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def serialize_ability(
    ability: Ability, ids: dict[tuple[str, str], str], sel_ids: dict[tuple[str, str], str]
) -> dict[str, object]:
    def seg_json(seg: Segment) -> dict[str, object]:
        return {
            "role": seg.role,
            "text": seg.text,
            "template": ids[(seg.role, seg.pattern)],
            "compositional": sel_ids[sel_key(seg)],
        }

    out: dict[str, object] = {
        "kind": ability.kind,
        "chain": ability.chain,
        "timings": ability.timings,
    }
    if ability.qualifiers:
        out["qualifiers"] = ability.qualifiers
    if ability.structural is not None:
        out["text"] = ability.body
        return out
    if ability.cost:
        out["cost"] = [seg_json(s) for s in ability.cost]
    out["sentences"] = [
        {
            "text": s.text,
            **({"connector": s.connector} if s.connector else {}),
            **({"instead": True} if s.instead else {}),
            "segments": [seg_json(seg) for seg in s.segments],
        }
        for s in ability.sentences
    ]
    if ability.options:
        out["options"] = [serialize_ability(o, ids, sel_ids) for o in ability.options]
    return out


def template_chain(chain: str) -> str:
    return MARKER_RE.sub(lambda m: "【" + templatize(m.group(1))[0] + "】" + m.group(2), chain)


def marker_templates(cards: list[Card]) -> list[dict[str, object]]:
    stats: dict[str, set[str]] = defaultdict(set)
    for card in cards:
        for top in card.abilities:
            for ability in top.walk():
                if ability.chain:
                    stats[template_chain(ability.chain)].add(card.number)
    return [
        {
            "pattern": p,
            "n_cards": len(stats[p]),
            "examples": sorted(stats[p], key=card_sort_key)[:MAX_EXAMPLES],
        }
        for p in sorted(stats, key=lambda p: (-len(stats[p]), p))
    ]


def build_outputs(data_dir: Path) -> tuple[dict[str, object], dict[str, object]]:
    cards_path = data_dir / "cards.ndjson"
    manifest = json.loads((data_dir / "manifest.json").read_text(encoding="utf-8"))
    records = load_records(cards_path)
    canonical, fallbacks = select_canonical(records)
    log = NormalizationLog()
    cards = [build_card(canonical[n], log) for n in sorted(canonical, key=card_sort_key)]
    for card in cards:
        for top in card.abilities:
            for ability in top.walk():
                if ability.structural is not None:
                    ability.structural.pattern, ability.structural.slots = templatize(
                        ability.structural.text
                    )

    stats = cluster(cards)
    ranked = rank_templates(stats)
    verify_templates(ranked)
    ids = {(s.role, s.pattern): f"T{i + 1:03d}" for i, s in enumerate(ranked)}
    freq = {(s.role, s.pattern): len(s.cards) for s in ranked}

    sel_cards, phrase_cards = compositional_stats(cards)
    sel_ranked = sorted(sel_cards, key=lambda k: (-len(sel_cards[k]), k[0], k[1]))
    sel_ids = {key: f"C{i + 1:03d}" for i, key in enumerate(sel_ranked)}
    freq_sel = {key: len(sel_cards[key]) for key in sel_ranked}
    flat_by_sel: dict[tuple[str, str], list[str]] = defaultdict(list)
    for s in ranked:
        flat_by_sel[(s.role, abstract_selectors(s.pattern)[0])].append(ids[(s.role, s.pattern)])

    nonvanilla = [c for c in cards if not c.vanilla]
    flat = level_summary(nonvanilla, freq, flat_key, ids)
    comp = level_summary(nonvanilla, freq_sel, sel_key, sel_ids)
    role_counts = Counter(s.role for s in ranked)
    role_regular = Counter(s.role for s in ranked if len(s.cards) >= REGULAR_MIN_CARDS)
    structural = Counter(
        ability.kind
        for c in cards
        for top in c.abilities
        for ability in top.walk()
        if ability.structural is not None
    )
    stripped = [
        {
            "text": t,
            "occurrences": n,
            "n_cards": len(log.stripped_cards[t]),
            "examples": sorted(log.stripped_cards[t], key=card_sort_key)[:4],
        }
        for t, n in sorted(log.stripped.items(), key=lambda kv: (-kv[1], kv[0]))
    ]
    figures: dict[str, object] = {
        "n_card_numbers": len(cards),
        "n_printings": len(records),
        "n_vanilla": len(cards) - len(nonvanilla),
        "n_nonvanilla": len(nonvanilla),
        **flat.counts,
        "structural_lines": dict(sorted(structural.items())),
        "templates_by_role": {
            r: {"total": role_counts[r], "regular": role_regular[r]} for r in sorted(role_counts)
        },
        "compositional": {**comp.counts, "n_selector_phrases": len(phrase_cards)},
    }
    templates_json = [
        {
            "id": ids[(s.role, s.pattern)],
            "key": template_key(s),
            "role": s.role,
            "pattern": s.pattern,
            "regex": template_regex(s.pattern),
            "slots": [m.group(1) for m in SLOT_TOKEN_RE.finditer(s.pattern)],
            "n_cards": len(s.cards),
            "occurrences": s.occurrences,
            "regular": len(s.cards) >= REGULAR_MIN_CARDS,
            "compositional_template": sel_ids[(s.role, abstract_selectors(s.pattern)[0])],
            "card_numbers": sorted(s.cards, key=card_sort_key),
            "examples": [
                {"card_number": n, "text": s.examples[n]}
                for n in sorted(s.examples, key=card_sort_key)[:2]
            ],
            "slot_values": {
                slot: dict(sorted(values.items(), key=lambda kv: (-kv[1], kv[0])))
                for slot, values in sorted(s.slot_values.items())
            },
        }
        for s in ranked
    ]
    compositional_json = [
        {
            "id": sel_ids[key],
            "role": key[0],
            "pattern": key[1],
            "n_cards": freq_sel[key],
            "regular": freq_sel[key] >= REGULAR_MIN_CARDS,
            "flat_templates": flat_by_sel[key],
            "examples": sorted(sel_cards[key], key=card_sort_key)[:MAX_EXAMPLES],
        }
        for key in sel_ranked
    ]
    selector_phrases = [
        {
            "phrase": p,
            "n_cards": len(phrase_cards[p]),
            "examples": sorted(phrase_cards[p], key=card_sort_key)[:MAX_EXAMPLES],
        }
        for p in sorted(phrase_cards, key=lambda p: (-len(phrase_cards[p]), p))
    ]
    comp_irregular_keys = {(i["card_number"], i["text"]) for i in comp.irregular}
    for item in flat.irregular:
        item["needs_per_card_binding"] = (item["card_number"], item["text"]) in comp_irregular_keys
    packages = build_packages(cards, freq, freq_sel)
    verify_partition(packages, cards)

    source = {
        "cards_ndjson": "src/gcg_sim/data/gcgapi/cards.ndjson",
        "sha256": sha256_of(cards_path),
        "gcg_api_commit": GCG_API_COMMIT,
        "dataset_version": manifest.get("dataset_version"),
        "built_at": manifest.get("built_at"),
        "rules": "gundam-card-game-comprehensive-rules.md (Ver. 1.9.0)",
    }
    templates_doc: dict[str, object] = {
        "schema_version": 1,
        "generated_by": "scripts/cluster_effects.py",
        "source": source,
        "method": {
            "canonical_printing": "product_id == card_number; otherwise the lowest _pN printing (see canonical_fallbacks)",
            "units": "Each ability line is split into sentences (period + space + capital); each sentence is split into "
            "a connector ('Then,'/'If you do,'), leading prefix segments (scope 'During ...,', trigger 'When .../At the ...', "
            "condition 'If .../While ...') and a core; a trailing ' instead.' is recorded as a flag. Activated-ability costs "
            "(text before '：') are split on ', ' into cost segments. Keyword-only lines and 【Pilot】[Name] lines are structural "
            "and not counted as sentences.",
            "regular_threshold": f"a template is regular when it is used by >= {REGULAR_MIN_CARDS} distinct card numbers",
            "slot_order": list(SLOT_ORDER),
        },
        "normalization": [
            {"id": rid, "rule": desc, "texts_changed": log.rule_hits.get(rid, 0)}
            for rid, desc in NORMALIZATION_RULES
        ],
        "stripped_parentheticals": stripped,
        "canonical_fallbacks": fallbacks,
        "divergent_printings": divergent_printings(records, canonical),
        "figures": figures,
        "coverage_curve": flat.coverage_curve,
        "coverage_curve_compositional": comp.coverage_curve,
        "slot_types": {
            name: {"regex": rx, "description": desc} for name, (rx, desc) in SLOT_TYPES.items()
        },
        "selector_grammar": SELECTOR_GRAMMAR,
        "ability_kinds": ability_kind_counts(cards),
        "marker_templates": marker_templates(cards),
        "templates": templates_json,
        "compositional_templates": compositional_json,
        "selector_phrases": selector_phrases,
        "irregular": flat.irregular,
        "irregular_cards": flat.irregular_cards,
        "irregular_compositional": comp.irregular,
        "irregular_compositional_cards": comp.irregular_cards,
        "primitives": primitive_inventory(cards),
        "cards": {
            c.number: {
                "name": c.name,
                "card_type": c.card_type,
                "product_id": c.product_id,
                "vanilla": c.vanilla,
                "normalized_text": c.text,
                "abilities": [serialize_ability(a, ids, sel_ids) for a in c.abilities],
            }
            for c in cards
        },
    }
    packages_doc: dict[str, object] = {
        "schema_version": 1,
        "generated_by": "scripts/cluster_effects.py",
        "source": source,
        "effort_model": {
            "weights": EFFORT_WEIGHTS,
            "sizes": [{"below_points": limit, "size": size} for limit, size in EFFORT_SIZES]
            + [{"below_points": None, "size": "XL"}],
            "note": "points = 0.5 * non-vanilla cards + 0.5 * templated sentences + 1.5 * sentences that need the selector "
            "grammar + 3 * sentences that stay irregular after it (per-card bindings)",
        },
        "n_packages": len(packages),
        "n_cards_total": sum(int(str(p["n_cards"])) for p in packages),
        "packages": packages,
    }
    return templates_doc, packages_doc


# --------------------------------------------------------------------------------------
# Markdown generated blocks
# --------------------------------------------------------------------------------------


def md_code(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def md_escape(text: str) -> str:
    return md_code(text).replace("<", "\\<")


def md_block_stats(doc: dict[str, object]) -> str:
    fig = doc["figures"]
    assert isinstance(fig, dict)
    rows: list[str] = []
    for key, value in fig.items():
        if key == "compositional" and isinstance(value, dict):
            rows += [f"| compositional.{k} | {v} |" for k, v in value.items()]
        else:
            rows.append(f"| {key} | {md_escape(json.dumps(value, ensure_ascii=False))} |")
    return "| figure | value |\n| --- | --- |\n" + "\n".join(rows)


def md_block_normalization(doc: dict[str, object]) -> str:
    rules = doc["normalization"]
    stripped = doc["stripped_parentheticals"]
    assert isinstance(rules, list)
    assert isinstance(stripped, list)
    lines = ["| id | rule | texts changed |", "| --- | --- | --- |"]
    lines += [f"| {r['id']} | {md_escape(r['rule'])} | {r['texts_changed']} |" for r in rules]
    lines += [
        "",
        "Stripped explanatory parentheticals (all distinct texts):",
        "",
        "| occurrences | cards | text | examples |",
        "| --- | --- | --- | --- |",
    ]
    lines += [
        f"| {s['occurrences']} | {s['n_cards']} | {md_escape(s['text'])} | {', '.join(s['examples'])} |"
        for s in stripped
    ]
    return "\n".join(lines)


def md_block_coverage(doc: dict[str, object]) -> str:
    flat = doc["coverage_curve"]
    comp = doc["coverage_curve_compositional"]
    assert isinstance(flat, list)
    assert isinstance(comp, list)
    lines = [
        "| k (min cards per template) | flat templates >= k | flat sentences covered | flat cards fully covered "
        "| {SEL} templates >= k | {SEL} sentences covered | {SEL} cards fully covered |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for f, c in zip(flat, comp, strict=True):
        lines.append(
            f"| {f['k']} | {f['templates_with_freq_ge_k']} | {f['sentences_covered']} | {f['nonvanilla_cards_fully_covered']} "
            f"| {c['templates_with_freq_ge_k']} | {c['sentences_covered']} | {c['nonvanilla_cards_fully_covered']} |"
        )
    return "\n".join(lines)


def md_block_templates(doc: dict[str, object]) -> str:
    templates = doc["templates"]
    assert isinstance(templates, list)
    lines = [
        "| id | role | cards | pattern | {SEL} template | examples |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for t in templates[:TOP_TEMPLATES_IN_MD]:
        ex = ", ".join(e["card_number"] for e in t["examples"])
        lines.append(
            f"| {t['id']} | {t['role']} | {t['n_cards']} | `{md_code(t['pattern'])}` | {t['compositional_template']} | {ex} |"
        )
    return "\n".join(lines)


def md_block_compositional(doc: dict[str, object]) -> str:
    templates = doc["compositional_templates"]
    phrases = doc["selector_phrases"]
    assert isinstance(templates, list)
    assert isinstance(phrases, list)
    lines = [
        "| id | role | cards | flat templates merged | pattern | examples |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for t in templates[: TOP_TEMPLATES_IN_MD // 2]:
        lines.append(
            f"| {t['id']} | {t['role']} | {t['n_cards']} | {len(t['flat_templates'])} | `{md_code(t['pattern'])}` | {', '.join(t['examples'][:3])} |"
        )
    lines += [
        "",
        f"Selector phrases found by the grammar ({len(phrases)} distinct; top {TOP_TEMPLATES_IN_MD // 2}):",
        "",
    ]
    lines += ["| cards | selector phrase (flat pattern) | examples |", "| --- | --- | --- |"]
    lines += [
        f"| {p['n_cards']} | `{md_code(p['phrase'])}` | {', '.join(p['examples'][:3])} |"
        for p in phrases[: TOP_TEMPLATES_IN_MD // 2]
    ]
    return "\n".join(lines)


def md_block_kinds(doc: dict[str, object]) -> str:
    kinds = doc["ability_kinds"]
    markers = doc["marker_templates"]
    assert isinstance(kinds, dict)
    assert isinstance(markers, list)
    lines = ["| ability kind | cards | occurrences | examples |", "| --- | --- | --- | --- |"]
    lines += [
        f"| {k} | {v['n_cards']} | {v['occurrences']} | {', '.join(v['examples'])} |"
        for k, v in kinds.items()
    ]
    lines += ["", "| marker chain template | cards | examples |", "| --- | --- | --- |"]
    lines += [
        f"| `{md_code(m['pattern'])}` | {m['n_cards']} | {', '.join(m['examples'])} |"
        for m in markers
    ]
    return "\n".join(lines)


def md_block_primitives(doc: dict[str, object]) -> str:
    prims = doc["primitives"]
    assert isinstance(prims, dict)
    lines: list[str] = []
    for category, rows in prims.items():
        lines += [
            f"#### {category}",
            "",
            "| id | description | cards | occ. | examples |",
            "| --- | --- | --- | --- | --- |",
        ]
        for r in sorted(rows, key=lambda r: (-r["n_cards"], r["id"])):
            lines.append(
                f"| `{r['id']}` | {md_escape(r['description'])} | {r['n_cards']} | {r['occurrences']} | {', '.join(r['examples'])} |"
            )
        lines.append("")
    return "\n".join(lines).rstrip()


def md_block_irregular(doc: dict[str, object]) -> str:
    irregular = doc["irregular"]
    assert isinstance(irregular, list)
    lines = [
        "| card | sentence (normalized) | irregular segment(s) | per-card binding |",
        "| --- | --- | --- | --- |",
    ]
    for item in irregular:
        segs = "; ".join(f"{s['role']}: {s['text']}" for s in item["irregular_segments"])
        binding = "yes" if item["needs_per_card_binding"] else "no ({SEL})"
        lines.append(
            f"| {item['card_number']} | {md_escape(item['text'])} | {md_escape(segs)} | {binding} |"
        )
    return "\n".join(lines)


def md_block_divergent(doc: dict[str, object]) -> str:
    div = doc["divergent_printings"]
    fallbacks = doc["canonical_fallbacks"]
    assert isinstance(div, list)
    assert isinstance(fallbacks, list)
    lines = [
        "Canonical fallbacks (no printing with product_id == card_number): "
        + ", ".join(f"{f['card_number']} -> {f['product_id']}" for f in fallbacks),
        "",
        "| card | printings | raw texts | differs after normalization | differing printings |",
        "| --- | --- | --- | --- | --- |",
    ]
    for d in div:
        variants = "; ".join(
            f"{v['product_id']} ({v['set_code']}): {v['normalized']}" for v in d["variants"]
        )
        lines.append(
            f"| {d['card_number']} | {d['n_printings']} | {d['n_raw_texts']} | {'yes' if d['differs_after_normalization'] else 'no'} | {md_escape(variants)} |"
        )
    return "\n".join(lines)


def md_block_packages(doc: dict[str, object]) -> str:
    packages = doc["packages"]
    assert isinstance(packages, list)
    lines = [
        "| package | sets | range | cards | non-vanilla | sentences | irregular | effort |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for p in packages:
        lines.append(
            f"| {p['id']} | {', '.join(p['sets'])} | {p['range']} | {p['n_cards']} | {p['n_nonvanilla']} | "
            f"{p['n_sentences']} | {p['n_irregular_sentences']} | {p['est_effort']} ({p['effort_points']}) |"
        )
    return "\n".join(lines)


MD_BLOCKS = {
    "stats": lambda t, p: md_block_stats(t),
    "normalization": lambda t, p: md_block_normalization(t),
    "coverage": lambda t, p: md_block_coverage(t),
    "kinds": lambda t, p: md_block_kinds(t),
    "templates": lambda t, p: md_block_templates(t),
    "compositional": lambda t, p: md_block_compositional(t),
    "primitives": lambda t, p: md_block_primitives(t),
    "irregular": lambda t, p: md_block_irregular(t),
    "divergent": lambda t, p: md_block_divergent(t),
    "packages": lambda t, p: md_block_packages(p),
}


def render_markdown(
    existing: str, templates_doc: dict[str, object], packages_doc: dict[str, object]
) -> str:
    text = existing
    for name, render in MD_BLOCKS.items():
        start = f"<!-- GENERATED:{name}:start -->"
        end = f"<!-- GENERATED:{name}:end -->"
        body = render(templates_doc, packages_doc)
        block = f"{start}\n{body}\n{end}"
        pattern = re.compile(re.escape(start) + r".*?" + re.escape(end), re.S)
        if pattern.search(text):
            text = pattern.sub(lambda _m, b=block: b, text)
        else:
            text = text.rstrip("\n") + f"\n\n## Generated: {name}\n\n{block}\n"
    return text


def dump_json(doc: dict[str, object]) -> str:
    return json.dumps(doc, ensure_ascii=False, indent=1) + "\n"


def write_or_check(path: Path, content: str, check: bool) -> bool:
    current = path.read_text(encoding="utf-8") if path.exists() else None
    if check:
        return current == content
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return True


def summary_lines(
    templates_doc: dict[str, object], packages_doc: dict[str, object]
) -> Iterable[str]:
    fig = templates_doc["figures"]
    assert isinstance(fig, dict)
    for key in (
        "n_templates",
        "n_templates_regular",
        "sentences_total",
        "sentences_templated",
        "cards_fully_templated",
        "n_irregular_cards",
        "n_irregular_sentences",
    ):
        yield f"{key}: {fig[key]}"
    yield f"packages: {packages_doc['n_packages']} covering {packages_doc['n_cards_total']} card numbers"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    parser.add_argument("--data-dir", type=Path, default=DATA_DIR)
    parser.add_argument("--check", action="store_true", help="exit 1 if outputs are not up to date")
    parser.add_argument(
        "--no-markdown", action="store_true", help="do not touch EFFECT_TEMPLATES.md"
    )
    args = parser.parse_args(argv)

    templates_doc, packages_doc = build_outputs(args.data_dir)
    outputs = [(TEMPLATES_JSON, dump_json(templates_doc)), (PACKAGES_JSON, dump_json(packages_doc))]
    if not args.no_markdown:
        existing = (
            MARKDOWN.read_text(encoding="utf-8") if MARKDOWN.exists() else "# Effect templates\n"
        )
        outputs.append((MARKDOWN, render_markdown(existing, templates_doc, packages_doc)))
    ok = True
    for path, content in outputs:
        if not write_or_check(path, content, args.check):
            print(f"out of date: {path.relative_to(REPO)}", file=sys.stderr)
            ok = False
    for line in summary_lines(templates_doc, packages_doc):
        print(line)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
