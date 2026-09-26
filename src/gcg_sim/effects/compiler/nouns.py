"""Selector noun-phrase grammar: ``SEL := determiner? modifier* head post_filter*``."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler.abilities import CompileError

TRAITS_RE = r"(?:\([^()]+\)(?:\s*/\s*\([^()]+\))*)"
COLORS = ("blue", "green", "red", "white", "purple")
KW_RE = r"<(Repair|Breach|Support|Blocker|First Strike|High-Maneuver|Suppression)(?:\s*\d+)?>"


def traits_of(s: str) -> tuple[str, ...]:
    return tuple(t.strip() for t in re.findall(r"\(([^()]+)\)", s))


def cmp_op(word: str) -> d.Op:
    w = word.strip().lower()
    if w in ("or less", "or lower", "or fewer"):
        return d.Op.LE
    if w in ("or more", "or higher", "or greater"):
        return d.Op.GE
    raise CompileError(f"unknown comparison {word!r}")


@dataclass
class SelSpec:
    sel: d.Sel
    count: d.Value = 1
    min_count: d.Value | None = None
    all: bool = False
    plural: bool = False
    extra: dict[str, object] = field(default_factory=dict)


_NUM_WORDS = {"a": 1, "an": 1, "one": 1, "another": 1, "two": 2, "three": 3}

_HEADS: list[tuple[str, tuple[d.CardKind, ...], d.Loc | None, tuple[d.Filter, ...]]] = [
    (r"Unit tokens?", (d.CardKind.UNIT_TOKEN,), d.Loc.BATTLE, ()),
    (r"Link Units?|Linked Units", (d.CardKind.UNIT,), d.Loc.BATTLE, (d.IsLinked(),)),
    (
        r"Units?/Bases?|Unit/Base",
        (d.CardKind.UNIT, d.CardKind.BASE),
        d.Loc.FIELD_UNITS_AND_BASES,
        (),
    ),
    (r"Unit cards?/Base cards?|Unit/Base cards?", (d.CardKind.UNIT, d.CardKind.BASE), None, ()),
    (r"Unit/Pilot cards?|Unit cards?/Pilot cards?", (d.CardKind.UNIT, d.CardKind.PILOT), None, ()),
    (
        r"Pilot/Command cards?|Pilot cards?/Command cards?",
        (d.CardKind.PILOT, d.CardKind.COMMAND),
        None,
        (),
    ),
    (r"Unit cards?", (d.CardKind.UNIT,), None, ()),
    (r"Pilot cards?", (d.CardKind.PILOT,), None, ()),
    (r"Command cards?", (d.CardKind.COMMAND,), None, ()),
    (r"Base cards?", (d.CardKind.BASE,), None, ()),
    (r"Resource cards?", (d.CardKind.RESOURCE,), None, ()),
    (r"cards?", (), None, ()),
    (r"Units?", (d.CardKind.UNIT,), d.Loc.BATTLE, ()),
    (r"Bases?", (d.CardKind.BASE,), d.Loc.BASE, ()),
    (r"Pilots?", (d.CardKind.PILOT,), d.Loc.PAIRED, ()),
    (r"EX Resources?", (d.CardKind.EX_RESOURCE,), d.Loc.RESOURCE_AREA, ()),
    (r"Resources?", (d.CardKind.RESOURCE,), d.Loc.RESOURCE_AREA, ()),
    (r"Shields?", (), d.Loc.SHIELDS, ()),
    (r"shield area cards?", (), d.Loc.SHIELD_AREA, ()),
]


def _stat_word(w: str) -> d.Stat:
    return {
        "AP": d.Stat.AP,
        "HP": d.Stat.HP,
        "Lv.": d.Stat.LV,
        "Lv": d.Stat.LV,
        "cost": d.Stat.COST,
    }[w]


_POST: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r"^that (?:is|are) Lv\.(\d+) (or lower|or higher)"), "lv_cmp"),
    (re.compile(r"^that (?:is|are) Lv\.(\d+)"), "lv_eq"),
    (re.compile(r"^(?:with|that has|that have) (\d+) (or less|or more) (AP|HP)"), "stat_cmp"),
    (re.compile(r"^(?:with|that has) (\d+) (AP|HP)"), "stat_eq"),
    (re.compile(r"^with a Lv\. of (\d+) (or lower|or higher)"), "lv_cmp"),
    (re.compile(r"^with (?:a )?cost of (\d+) (or less|or more)"), "cost_cmp"),
    (re.compile(r"^with (\d+) (or less|or more) cost"), "cost_cmp"),
    (
        re.compile(
            r"^whose Lv\. is (equal to or lower|equal to or higher|lower|higher) than (this Unit|that Unit|it)(?:'s Lv\.)?"
        ),
        "lv_rel",
    ),
    (re.compile(r"^with (?:AP|HP) equal to or less than (this Unit|it)"), "ap_rel"),
    (re.compile(r"^with " + KW_RE + r"(?:\s*/\s*" + KW_RE + r")*"), "kw"),
    (re.compile(r"^without " + KW_RE), "no_kw"),
    (
        re.compile(
            r"^with (?:no paired Pilot|no Pilot paired with it)|^that has no Pilot paired with it|^that is not paired with a Pilot"
        ),
        "unpaired",
    ),
    (re.compile(r"^paired with (?:a|an) Pilot"), "paired"),
    (re.compile(r"^paired with (?:a|an) (" + TRAITS_RE + r") Pilot"), "paired_trait"),
    (re.compile(r"^paired with (?:a|an) (blue|green|red|white|purple) Pilot"), "paired_color"),
    (re.compile(r"^paired with a Pilot that is Lv\.(\d+) (or lower|or higher)"), "paired_lv"),
    (
        re.compile(r'^with "([^"]+)"(?:\s*(?:/|or)\s*"([^"]+)")* in (?:its|their) card names?'),
        "name",
    ),
    (re.compile(r'^without "([^"]+)" in (?:its|their) card names?'), "no_name"),
    (re.compile(r"^other than (?:this Unit|this card|this Base)"), "other"),
    (re.compile(r"^other than Link Units"), "not_link"),
    (re.compile(r"^other than Unit tokens"), "not_token"),
    (re.compile(r"^with a keyword effect"), "any_kw"),
    (re.compile(r"^with (?:a|an) 【([^】]+)】 effect"), "timing"),
    (re.compile(r"^with 【Burst】"), "burst"),
    (re.compile(r"^that is being attacked"), "attacked"),
    (re.compile(r"^that is battling"), "battling"),
    (re.compile(r"^battling this Unit"), "battling_this"),
    (re.compile(r"^in play"), "in_play"),
    (re.compile(r"^(?:from|in) your trash"), "your_trash"),
    (re.compile(r"^(?:from|in) (?:any player's|either player's|a player's) trash"), "any_trash"),
    (re.compile(r"^(?:from|in) (?:your opponent's|an enemy's|the enemy's) trash"), "enemy_trash"),
    (re.compile(r"^(?:from|in) your hand"), "your_hand"),
    (re.compile(r"^(?:from|in) your resource area"), "your_resources"),
    (re.compile(r"^(?:from|in) your deck"), "your_deck"),
    (re.compile(r"^among them"), "among_them"),
    (re.compile(r"^in your shield area"), "your_shield_area"),
    (
        re.compile(r"^in (?:your opponent's|the enemy's|an enemy's) shield area"),
        "enemy_shield_area",
    ),
    (re.compile(r"^(?:in|on) the field"), "in_play"),
]


def parse_selector(phrase: str, *, default_side: d.Side | None = None) -> SelSpec:
    """Parse a noun phrase like ``1 enemy Unit with 3 or less HP`` or ``all your (Zeon) Units``."""
    s = phrase.strip().rstrip(".").strip()
    count: d.Value = 1
    min_count: d.Value | None = None
    is_all = False
    side: d.Side | None = default_side
    filters: list[d.Filter] = []
    plural = False
    m = re.match(r"^(\d+) to (\d+) ", s)
    if m:
        min_count, count = int(m.group(1)), int(m.group(2))
        s = s[m.end() :]
        plural = True
    else:
        m = re.match(r"^up to (\d+) ", s)
        if m:
            min_count, count = 0, int(m.group(1))
            s = s[m.end() :]
            plural = True
        else:
            m = re.match(
                r"^(\d+|a|an|one|another|two|three|all|each|any number of|the|every)\b ?", s, re.I
            )
            if m:
                w = m.group(1).lower()
                if w in ("all", "each", "every"):
                    is_all = True
                    plural = True
                elif w == "any number of":
                    min_count, count = 0, 99
                    plural = True
                elif w == "the":
                    pass
                elif w.isdigit():
                    count = int(w)
                    plural = count > 1
                else:
                    count = _NUM_WORDS[w]
                if w == "another":
                    filters.append(d.NotRef(d.This()))
                s = s[m.end() :]
    m = re.match(r"^of (?:your|their)(?: own)? ", s)
    if m:
        side = d.Side.FRIENDLY
        s = s[m.end() :]
    m = re.match(r"^of the ", s)
    if m:
        s = s[m.end() :]
    # modifiers
    while True:
        m = re.match(
            r"^(other|another|active|rested|damaged|undamaged|friendly|enemy|your|non-battling|non-(?:blue|green|red|white|purple)|blue|green|red|white|purple|"
            + TRAITS_RE
            + r")\s+",
            s,
            re.I,
        )
        if not m:
            break
        w = m.group(1)
        lw = w.lower()
        if lw in ("other", "another"):
            filters.append(d.NotRef(d.This()))
        elif lw == "active":
            filters.append(d.IsRested(False))
        elif lw == "rested":
            filters.append(d.IsRested(True))
        elif lw == "damaged":
            filters.append(d.IsDamaged(True))
        elif lw == "undamaged":
            filters.append(d.IsDamaged(False))
        elif lw in ("friendly", "your"):
            side = d.Side.FRIENDLY
        elif lw == "enemy":
            side = d.Side.ENEMY
        elif lw == "non-battling":
            filters.append(d.IsBattling(False))
        elif lw.startswith("non-"):
            filters.append(d.Not(d.HasColor((lw[4:].capitalize(),))))
        elif lw in COLORS:
            filters.append(d.HasColor((lw.capitalize(),)))
        else:
            filters.append(d.HasTrait(traits_of(w)))
        s = s[m.end() :]
    kinds: tuple[d.CardKind, ...] = ()
    loc: d.Loc | None = None
    for pat, ks, lc, fs in _HEADS:
        m = re.match(r"^(?:" + pat + r")(?=$|[\s,.])", s)
        if m:
            kinds = ks
            loc = lc
            filters.extend(fs)
            if m.group(0).endswith("s") and not m.group(0).endswith("ss"):
                plural = True
            s = s[m.end() :].strip()
            break
    else:
        raise CompileError(f"no selector head in {phrase!r}")
    # post filters
    while s:
        s = s.lstrip(", ").strip()
        if s.startswith("and ") or not s:
            break
        for rx, kind in _POST:
            m = rx.match(s)
            if not m:
                continue
            loc, side = _apply_post(kind, m, filters, loc, side)
            s = s[m.end() :].strip()
            break
        else:
            raise CompileError(f"unparsed selector suffix {s!r} in {phrase!r}")
    if kinds:
        filters.insert(0, d.IsKind(kinds))
    if loc is None:
        loc = d.Loc.TRASH if kinds else d.Loc.BATTLE
    if side is None:
        side = d.Side.ANY
    sel = d.Sel(side, loc, tuple(filters))
    return SelSpec(sel=sel, count=count, min_count=min_count, all=is_all, plural=plural)


def _apply_post(
    kind: str, m: re.Match[str], filters: list[d.Filter], loc: d.Loc | None, side: d.Side | None
) -> tuple[d.Loc | None, d.Side | None]:
    if kind == "lv_cmp":
        filters.append(d.StatCmp(d.Stat.LV, cmp_op(m.group(2)), int(m.group(1))))
    elif kind == "lv_eq":
        filters.append(d.StatCmp(d.Stat.LV, d.Op.EQ, int(m.group(1))))
    elif kind == "stat_cmp":
        filters.append(d.StatCmp(_stat_word(m.group(3)), cmp_op(m.group(2)), int(m.group(1))))
    elif kind == "stat_eq":
        filters.append(d.StatCmp(_stat_word(m.group(2)), d.Op.EQ, int(m.group(1))))
    elif kind == "cost_cmp":
        filters.append(d.StatCmp(d.Stat.COST, cmp_op(m.group(2)), int(m.group(1))))
    elif kind == "lv_rel":
        rel = m.group(1)
        op = {
            "equal to or lower": d.Op.LE,
            "equal to or higher": d.Op.GE,
            "lower": d.Op.LT,
            "higher": d.Op.GT,
        }[rel]
        ref: d.Ref = d.This() if m.group(2) == "this Unit" else d.Var("__that")
        filters.append(d.LevelCmpRef(d.Stat.LV, op, ref, d.Stat.LV))
    elif kind == "ap_rel":
        filters.append(d.LevelCmpRef(d.Stat.AP, d.Op.LE, d.This(), d.Stat.AP))
    elif kind == "kw":
        kws = re.findall(KW_RE, m.group(0))
        if len(kws) == 1:
            filters.append(d.HasKeyword(d.Kw(kws[0])))
        else:
            filters.append(d.AnyOf(tuple(d.HasKeyword(d.Kw(k)) for k in kws)))
    elif kind == "no_kw":
        filters.append(d.Not(d.HasKeyword(d.Kw(m.group(1)))))
    elif kind == "unpaired":
        filters.append(d.IsPaired(False))
    elif kind == "paired":
        filters.append(d.IsPaired(True))
    elif kind == "paired_trait":
        filters.append(d.PairedWith((d.HasTrait(traits_of(m.group(1))),)))
    elif kind == "paired_color":
        filters.append(d.PairedWith((d.HasColor((m.group(1).capitalize(),)),)))
    elif kind == "paired_lv":
        filters.append(d.PairedWith((d.StatCmp(d.Stat.LV, cmp_op(m.group(2)), int(m.group(1))),)))
    elif kind == "name":
        names = tuple(re.findall(r'"([^"]+)"', m.group(0)))
        filters.append(d.NameContains(names))
    elif kind == "no_name":
        filters.append(d.Not(d.NameContains((m.group(1),))))
    elif kind == "other":
        filters.append(d.NotRef(d.This()))
    elif kind == "not_link":
        filters.append(d.IsLinked(False))
    elif kind == "not_token":
        filters.append(d.IsToken(False))
    elif kind == "any_kw":
        filters.append(d.AnyOf(tuple(d.HasKeyword(k) for k in d.Kw)))
    elif kind == "timing":
        timing = {
            "Destroyed": "destroyed",
            "Deploy": "deployed",
            "Attack": "attacks",
            "When Paired": "paired",
            "When Linked": "linked",
            "Burst": "burst",
        }.get(m.group(1).strip())
        if timing is None:
            raise CompileError(f"unknown timing filter {m.group(1)}")
        filters.append(d.HasTiming(timing))
    elif kind == "burst":
        filters.append(d.HasBurst(True))
    elif kind == "attacked":
        filters.append(d.CustomFilter("is_attack_target"))
    elif kind == "battling":
        filters.append(d.IsBattling(True))
    elif kind == "battling_this":
        filters.append(d.IsRef(d.BattlingWith(d.This())))
    elif kind == "in_play":
        pass
    elif kind == "your_trash":
        loc, side = d.Loc.TRASH, d.Side.FRIENDLY
    elif kind == "any_trash":
        loc, side = d.Loc.TRASH, d.Side.ANY
    elif kind == "enemy_trash":
        loc, side = d.Loc.TRASH, d.Side.ENEMY
    elif kind == "your_hand":
        loc, side = d.Loc.HAND, d.Side.FRIENDLY
    elif kind == "your_resources":
        loc, side = d.Loc.RESOURCE_AREA, d.Side.FRIENDLY
    elif kind == "your_deck":
        loc, side = d.Loc.DECK, d.Side.FRIENDLY
    elif kind == "among_them":
        filters.append(d.IsRef(d.Var("__them")))
        loc = d.Loc.DECK
    elif kind == "your_shield_area":
        loc, side = d.Loc.SHIELD_AREA, d.Side.FRIENDLY
    elif kind == "enemy_shield_area":
        loc, side = d.Loc.SHIELD_AREA, d.Side.ENEMY
    else:
        raise CompileError(f"unhandled post filter {kind}")
    return loc, side
