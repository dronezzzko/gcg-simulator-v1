"""Sentence grammar: marker chains, connectors, prefixes, conditions, triggers, costs,
constant effects, and core step templates.

Rules referenced: 5-20 ("If you do"/"Then"), 10-1-5 (constant), 10-1-6 (triggered),
10-1-7 (activated), 10-1-8 (command), 13-1 (keywords incl. 13-1-8 Development),
13-2 (timing keywords).

1v1 interpretation of multiplayer wording (rule 12 is out of scope): "each enemy player",
"an enemy player", "another player" and "that player" all mean the opponent; "all players"
means both players; "2 or more enemy players" is never true.
"""

from __future__ import annotations

import re
from collections.abc import Callable
from dataclasses import dataclass

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler.abilities import CompileError, parse_keyword_line
from gcg_sim.effects.compiler.nouns import KW_RE, TRAITS_RE, cmp_op, parse_selector, traits_of
from gcg_sim.engine.types import Duration

RES_SYMBOLS = {"①": 1, "②": 2, "③": 3, "④": 4, "⑤": 5, "⑥": 6}
FALSE: d.Cond = d.Or(())


@dataclass
class G:
    """Per-ability compile context (pronoun state, variable counter)."""

    cdef: CardDef
    n: int = 0
    it: d.Ref | None = None
    after_then: bool = False
    event: d.Ev | None = None
    burst: bool = False
    development: int = 0

    def var(self) -> str:
        self.n += 1
        return f"t{self.n}"


# ---------------------------------------------------------------------------------------------
# text helpers


def split_sentences(body: str) -> list[str]:
    protected = body.replace("Lv.", "Lv§")
    parts = re.split(r"(?<=[.!:])\s+(?=[A-Z\"【<\[(■])|\n", protected)
    return [p.replace("Lv§", "Lv.").strip() for p in parts if p.strip()]


def _strip_period(s: str) -> str:
    return s.strip().rstrip(".").strip()


def _cap(s: str) -> str:
    return s[0].upper() + s[1:] if s else s


def _multiplayer(s: str) -> str:
    """Rewrite 1v1-equivalent multiplayer wording."""
    s = re.sub(r"\s+belonging to (?:each enemy player|another player|an enemy player)", "", s)
    s = re.sub(r"\ban enemy player\b", "your opponent", s)
    s = re.sub(r"\beach enemy player\b", "your opponent", s)
    s = re.sub(r"\ball enemy players each\b", "your opponent", s, flags=re.I)
    s = re.sub(r"\bthat (?:enemy )?player\b", "your opponent", s, flags=re.I)
    return s


# ---------------------------------------------------------------------------------------------
# references


_PRONOUN = re.compile(
    r"^(?:it|them|they|those Units|those cards|that card|these cards|those)$", re.I
)


def ref_phrase(phrase: str, g: G, *, allow_choose: bool = True) -> tuple[list[d.Step], d.Ref]:
    """Resolve a target phrase to (preceding steps, reference)."""
    p = _strip_period(phrase)
    pl = p.lower()
    if _PRONOUN.match(p):
        if g.it is None:
            raise CompileError(f"pronoun {p!r} without antecedent")
        return [], g.it
    if pl in ("this unit", "this base", "this pilot", "this", "this link unit", "this rested unit"):
        return [], d.This()
    if pl in ("this card",):
        return [], d.ThisCard()
    if pl in (
        "that unit",
        "that enemy unit",
        "the enemy unit",
        "the attacking unit",
        "that link unit",
        "that card",
        "the destroyed unit",
    ):
        return [], _that_ref(g)
    if pl in ("the attacking enemy unit",):
        return [], d.EventCard("subject")
    if pl in (
        "its paired pilot",
        "the pilot paired with this unit",
        "the card paired with this unit",
        "this unit's paired pilot",
        "the pilot paired with this unit",
    ):
        return [], d.PairedPilotOf(d.This())
    if pl in ("the pilot paired with it", "its pilot", "the card paired with it"):
        if g.it is None:
            raise CompileError("pronoun without antecedent")
        return [], d.PairedPilotOf(g.it)
    if pl in (
        "the enemy unit battling this unit",
        "the enemy unit this unit is battling",
        "the unit battling this unit",
        "the battling enemy unit",
        "a battling enemy unit",
    ):
        return [], d.BattlingWith(d.This())
    m = re.match(r"^(this Unit|it) and (.+)$", p, re.I)
    if m:
        s1, r1 = ref_phrase(m.group(1), g, allow_choose=allow_choose)
        s2, r2 = ref_phrase(m.group(2), g, allow_choose=allow_choose)
        return [*s1, *s2], d.Union((r1, r2))
    spec = parse_selector(p)
    if spec.all:
        return [], d.All(spec.sel)
    if not allow_choose:
        raise CompileError(f"selector not allowed here: {p!r}")
    v = g.var()
    step = d.Choose(v, spec.sel, spec.count, spec.min_count, after_then=g.after_then)
    g.it = d.Var(v)
    return [step], d.Var(v)


def _that_ref(g: G) -> d.Ref:
    if g.event in (d.Ev.DESTROYS_BY_BATTLE, d.Ev.DESTROYS_SHIELD_CARD, d.Ev.DEALS_DAMAGE):
        return d.EventCard("target")
    if g.event is d.Ev.BLOCKS:
        return d.EventCard("attacker")
    return d.EventCard("subject")


# ---------------------------------------------------------------------------------------------
# conditions


def _p(word: str) -> d.P:
    return d.P.YOU if word.lower() in ("you", "your") else d.P.OPP


def parse_count_cmp(n: str, op: str | None) -> tuple[d.Op, int]:
    if op is None or not op.strip():
        return d.Op.GE, int(n)
    return cmp_op(op), int(n)


def _friendly(sel: d.Sel) -> d.Sel:
    return (
        sel
        if sel.side is not d.Side.ANY
        else d.Sel(d.Side.FRIENDLY, sel.loc, sel.filters, sel.top_n)
    )


def parse_condition(text: str, g: G) -> d.Cond:
    c = _multiplayer(_strip_period(text))
    for rx, fn in _CONDS:
        m = rx.match(c)
        if m:
            try:
                return fn(m, g)
            except CompileError:
                continue
    parts = re.split(r",? and (?=(?:you|this|it|there|your|if|a|an|the|another)\b)", c)
    if len(parts) > 1:
        return d.And(tuple(parse_condition(p, g) for p in parts))
    parts = re.split(r",? or (?=(?:you|this|it|there|your|if)\b)", c)
    if len(parts) > 1:
        return d.Or(tuple(parse_condition(p, g) for p in parts))
    raise CompileError(f"unknown condition {c!r}")


def _cond_have_in_play(m: re.Match[str], g: G) -> d.Cond:
    spec = parse_selector(m.group(1), default_side=d.Side.FRIENDLY)
    return d.Exists(_friendly(spec.sel))


def _cond_count_in_play(m: re.Match[str], g: G) -> d.Cond:
    op, n = parse_count_cmp(m.group(1), m.group(2))
    spec = parse_selector("all " + m.group(3))
    return d.Cmp(d.Count(spec.sel), op, n)


def _cond_count_you(m: re.Match[str], g: G) -> d.Cond:
    op, n = parse_count_cmp(m.group(1), m.group(2))
    spec = parse_selector("all " + m.group(3), default_side=d.Side.FRIENDLY)
    return d.Cmp(d.Count(_friendly(spec.sel)), op, n)


def _cond_only_n(m: re.Match[str], g: G) -> d.Cond:
    spec = parse_selector("all " + m.group(2), default_side=d.Side.FRIENDLY)
    return d.Cmp(d.Count(_friendly(spec.sel)), d.Op.EQ, int(m.group(1)))


def _cond_is_in_play(m: re.Match[str], g: G) -> d.Cond:
    spec = parse_selector(m.group(1))
    return d.Exists(spec.sel)


def _zone_side(zone: str) -> tuple[d.Side, d.Loc]:
    z = zone.lower()
    side = d.Side.FRIENDLY if z.startswith("your ") and "opponent" not in z else d.Side.ENEMY
    if "any player" in z or "either player" in z:
        side = d.Side.ANY
    loc = d.Loc.TRASH if "trash" in z else (d.Loc.HAND if "hand" in z else d.Loc.REMOVAL)
    return side, loc


def _cond_zone_count(m: re.Match[str], g: G) -> d.Cond:
    op, n = parse_count_cmp(m.group(1), m.group(2))
    side, loc = _zone_side(m.group(4))
    spec = parse_selector("all " + m.group(3))
    return d.Cmp(d.Count(d.Sel(side, loc, spec.sel.filters)), op, n)


def _cond_level(m: re.Match[str], g: G) -> d.Cond:
    return d.Cmp(d.PlayerLevel(_p(m.group(1))), cmp_op(m.group(3)), int(m.group(2)))


def _cond_hand(m: re.Match[str], g: G) -> d.Cond:
    return d.Cmp(d.HandSize(_p(m.group(1))), cmp_op(m.group(3)), int(m.group(2)))


def _cond_shields(m: re.Match[str], g: G) -> d.Cond:
    return d.Cmp(d.ShieldCount(_p(m.group(1))), cmp_op(m.group(3)), int(m.group(2)))


def _cond_enemy_shields(m: re.Match[str], g: G) -> d.Cond:
    return d.Cmp(d.ShieldCount(d.P.OPP), cmp_op(m.group(2)), int(m.group(1)))


def _cond_turn(m: re.Match[str], g: G) -> d.Cond:
    return d.IsTurn(d.P.YOU if m.group(1).lower() == "your" else d.P.OPP)


def _self_filters(desc: str) -> tuple[d.Filter, ...]:
    s = desc.strip()
    sl = s.lower()
    simple: dict[str, tuple[d.Filter, ...]] = {
        "a link unit": (d.IsLinked(),),
        "linked": (d.IsLinked(),),
        "rested": (d.IsRested(True),),
        "active": (d.IsRested(False),),
        "damaged": (d.IsDamaged(True),),
        "undamaged": (d.IsDamaged(False),),
        "paired": (d.IsPaired(True),),
        "attacking": (d.IsAttacking(True),),
        "battling": (d.IsBattling(True),),
    }
    if sl in simple:
        return simple[sl]
    if sl in ("blue", "green", "red", "white", "purple"):
        return (d.HasColor((sl.capitalize(),)),)
    m = re.match(r"^paired with (?:a|an) (.+?) Pilot$", s)
    if m:
        return (d.PairedWith(_pilot_filters(m.group(1))),)
    m = re.match(r"^paired with a Pilot that is Lv\.(\d+) (or lower|or higher)$", s)
    if m:
        return (d.PairedWith((d.StatCmp(d.Stat.LV, cmp_op(m.group(2)), int(m.group(1))),)),)
    m = re.match(r"^an? (.+)$", s)
    if m:
        spec = parse_selector("1 " + m.group(1))
        return spec.sel.filters
    m = re.match(r"^(" + TRAITS_RE + r")$", s)
    if m:
        return (d.HasTrait(traits_of(m.group(1))),)
    raise CompileError(f"unknown self description {desc!r}")


def _pilot_filters(q: str) -> tuple[d.Filter, ...]:
    q = q.strip()
    m = re.match(r"^(" + TRAITS_RE + r")$", q)
    if m:
        return (d.HasTrait(traits_of(m.group(1))),)
    if q.lower() in ("blue", "green", "red", "white", "purple"):
        return (d.HasColor((q.capitalize(),)),)
    raise CompileError(f"unknown pilot qualifier {q!r}")


def _subject_ref(word: str, g: G) -> d.Ref:
    w = word.lower()
    if w in ("this unit", "this", "this base", "this card"):
        return d.This()
    if w in ("it", "they"):
        return g.it or d.This()
    if w in ("that unit", "the enemy unit", "that enemy unit"):
        return _that_ref(g)
    raise CompileError(f"unknown subject {word!r}")


def _cond_self_is(m: re.Match[str], g: G) -> d.Cond:
    ref = _subject_ref(m.group(1), g)
    desc = m.group(2)
    neg = desc.startswith("not ")
    c: d.Cond = d.RefMatches(ref, _self_filters(desc[4:] if neg else desc))
    return d.NotC(c) if neg else c


def _cond_self_has_kw(m: re.Match[str], g: G) -> d.Cond:
    ref = _subject_ref(m.group(1), g)
    return d.RefMatches(ref, (d.HasKeyword(d.Kw(m.group(2))),))


def _cond_self_stat(m: re.Match[str], g: G) -> d.Cond:
    ref = _subject_ref(m.group(1), g)
    stat = d.Stat.AP if m.group(4) == "AP" else d.Stat.HP
    op = cmp_op(m.group(3)) if m.group(3) else d.Op.EQ
    return d.RefMatches(ref, (d.StatCmp(stat, op, int(m.group(2))),))


def _cond_attacking(m: re.Match[str], g: G) -> d.Cond:
    what = m.group(1).lower()
    if "player" in what:
        return d.AttackTargetIs("player")
    if "damaged" in what:
        return d.And((d.AttackTargetIs("unit"), d.CustomCond("attack_target_damaged")))
    return d.AttackTargetIs("unit")


def _cond_deployed_from_trash(m: re.Match[str], g: G) -> d.Cond:
    return d.EventFrom(d.Loc.TRASH)


def _cond_unpaired(m: re.Match[str], g: G) -> d.Cond:
    return d.RefMatches(d.This(), (d.IsPaired(False),))


def _cond_none_in_play(m: re.Match[str], g: G) -> d.Cond:
    spec = parse_selector("all " + m.group(1), default_side=d.Side.FRIENDLY)
    return d.NotC(d.Exists(_friendly(spec.sel)))


def _cond_none_enemy(m: re.Match[str], g: G) -> d.Cond:
    spec = parse_selector("all " + m.group(1))
    return d.NotC(d.Exists(spec.sel))


def _cond_it_is(m: re.Match[str], g: G) -> d.Cond:
    ref = g.it or d.This()
    return d.RefMatches(ref, _self_filters(m.group(1)))


def _cond_false(m: re.Match[str], g: G) -> d.Cond:
    return FALSE


def _cond_ex_used(m: re.Match[str], g: G) -> d.Cond:
    return d.Cmp(d.EventAmount("ex_used"), d.Op.GE, 1)


def _cond_opp_has_ex(m: re.Match[str], g: G) -> d.Cond:
    return d.Exists(
        d.Sel(d.Side.ENEMY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),))
    )


def _cond_you_have_ex(m: re.Match[str], g: G) -> d.Cond:
    c: d.Cond = d.Exists(
        d.Sel(d.Side.FRIENDLY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),))
    )
    return d.NotC(c) if m.group(1) else c


def _cond_deck(m: re.Match[str], g: G) -> d.Cond:
    return d.Cmp(d.DeckSize(_p(m.group(1))), cmp_op(m.group(3)), int(m.group(2)))


_CONDS: list[tuple[re.Pattern[str], Callable[[re.Match[str], G], d.Cond]]] = [
    (re.compile(r"^(?:it is|it's|during) (your|your opponent's) turn$", re.I), _cond_turn),
    (re.compile(r"^there are (?:2|two) or more enemy players$", re.I), _cond_false),
    (
        re.compile(r"^you (?:have )?use[d]? an EX Resource to play this (?:card|Unit)$", re.I),
        _cond_ex_used,
    ),
    (re.compile(r"^your opponent has an EX Resource(?: in play)?$", re.I), _cond_opp_has_ex),
    (
        re.compile(r"^you (don't |do not )?have an EX Resource(?: in play)?$", re.I),
        _cond_you_have_ex,
    ),
    (re.compile(r"^you have no (.+?) in play$", re.I), _cond_none_in_play),
    (re.compile(r"^there (?:are|is) no (enemy .+?|.+?) in play$", re.I), _cond_none_enemy),
    (re.compile(r"^(?:you have )?only (\d+) (.+?) in play$", re.I), _cond_only_n),
    (
        re.compile(r"^you have (\d+) (or more|or less|or fewer)? ?(.+?) in play$", re.I),
        _cond_count_you,
    ),
    (re.compile(r"^you have (.+?) in play$", re.I), _cond_have_in_play),
    (
        re.compile(r"^(\d+) (or more|or less|or fewer)? ?(.+?) (?:are|is) in play$", re.I),
        _cond_count_in_play,
    ),
    (
        re.compile(r"^there (?:is|are) (\d+) (or more|or less|or fewer)? ?(.+?) in play$", re.I),
        _cond_count_in_play,
    ),
    (
        re.compile(
            r"^there (?:is|are) (\d+) (or more|or less|or fewer)? ?(.+?) in (your trash|your opponent's trash|your hand|any player's trash|either player's trash|your removal area)$",
            re.I,
        ),
        _cond_zone_count,
    ),
    (
        re.compile(
            r"^you have (\d+) (or more|or less|or fewer)? ?(.+?) in (your trash|your hand)$", re.I
        ),
        _cond_zone_count,
    ),
    (
        re.compile(
            r"^your opponent has (\d+) (or more|or less|or fewer)? ?(.+?) in (their trash)$", re.I
        ),
        _cond_zone_count,
    ),
    (
        re.compile(r"^(you|your opponent) (?:are|is) Lv\.(\d+) (or higher|or lower)$", re.I),
        _cond_level,
    ),
    (
        re.compile(
            r"^(you|your opponent) (?:have|has) (\d+) (or more|or less|or fewer) cards in (?:your|their) hand$",
            re.I,
        ),
        _cond_hand,
    ),
    (
        re.compile(
            r"^(you|your opponent) (?:have|has) (\d+) (or more|or less|or fewer) Shields$", re.I
        ),
        _cond_shields,
    ),
    (
        re.compile(r"^there are (\d+) (or more|or less|or fewer) enemy Shields$", re.I),
        _cond_enemy_shields,
    ),
    (
        re.compile(
            r"^(you|your opponent) (?:have|has) (\d+) (or more|or less|or fewer) cards in (?:your|their) deck$",
            re.I,
        ),
        _cond_deck,
    ),
    (
        re.compile(
            r"^(?:you are|this Unit is|it is) attacking (the enemy player|an enemy Unit|a damaged enemy Unit|an enemy Base)$",
            re.I,
        ),
        _cond_attacking,
    ),
    (
        re.compile(r"^you deploy(?:ed)? this (?:Unit|card) from your trash$", re.I),
        _cond_deployed_from_trash,
    ),
    (re.compile(r"^this Unit was deployed from your trash$", re.I), _cond_deployed_from_trash),
    (re.compile(r"^a Pilot is not paired with this Unit$", re.I), _cond_unpaired),
    (
        re.compile(r"^(this Unit|this|it|this Base|they) (?:has|have) " + KW_RE + r"$", re.I),
        _cond_self_has_kw,
    ),
    (
        re.compile(r"^(this Unit|it|this) has (\d+) ?(or more|or less)? (AP|HP)$", re.I),
        _cond_self_stat,
    ),
    (
        re.compile(r"^(this Unit|this|it|this Base|that Unit|the enemy Unit) is (.+)$", re.I),
        _cond_self_is,
    ),
    (re.compile(r"^it is an? (.+)$", re.I), _cond_it_is),
    (re.compile(r"^there is (an? .+?) in play$", re.I), _cond_is_in_play),
    (re.compile(r"^(.+?) (?:is|are) in play$", re.I), _cond_is_in_play),
    (re.compile(r"^(an? .+?) in play$", re.I), _cond_is_in_play),
]


# ---------------------------------------------------------------------------------------------
# continuous predicates


def split_duration(s: str) -> tuple[str, Duration | None]:
    s2 = _strip_period(s)
    for suffix, dur in (
        (" during this turn", Duration.THIS_TURN),
        (" this turn", Duration.THIS_TURN),
        (" during this battle", Duration.THIS_BATTLE),
        (" during your opponent's next turn", Duration.OPPONENT_NEXT_TURN),
        (" until the end of your opponent's next turn", Duration.OPPONENT_NEXT_TURN),
    ):
        if s2.endswith(suffix):
            return s2[: -len(suffix)], dur
    return s2, None


_STAT_MOD = re.compile(r"^gets? (AP|HP)([+-])(\d+)(?:,? and (AP|HP)([+-])(\d+))?$")
_KWS = re.compile(
    r"<(Repair|Breach|Support|Blocker|First Strike|High-Maneuver|Suppression)(?:\s*(\d+))?>"
)


def continuous_from_predicate(pred: str, g: G) -> list[d.Continuous]:
    """Predicates like ``gets AP+2``, ``gains <Blocker>``, ``can't attack``."""
    p = pred.strip()
    m = re.match(r"^(gets? [^,]+?)(?:,)? and (gains? .+|<.+>)$", p)
    if m and not _STAT_MOD.match(p):
        second = m.group(2)
        if second.startswith("<"):
            second = "gains " + second
        return [*continuous_from_predicate(m.group(1), g), *continuous_from_predicate(second, g)]
    m = _STAT_MOD.match(p)
    if m:
        vals = {"AP": 0, "HP": 0}
        vals[m.group(1)] = int(m.group(3)) * (1 if m.group(2) == "+" else -1)
        if m.group(4):
            vals[m.group(4)] = int(m.group(6)) * (1 if m.group(5) == "+" else -1)
        return [d.StatMod(ap=vals["AP"], hp=vals["HP"])]
    m = re.match(r"^gets? (AP|HP)([+-])(\d+) for (?:each|every) (.+?)(?: in play)?$", p)
    if m:
        v = _count_value(m.group(4), g)
        amt: d.Value = d.Times(v, int(m.group(3)) * (1 if m.group(2) == "+" else -1))
        return [d.StatMod(ap=amt) if m.group(1) == "AP" else d.StatMod(hp=amt)]
    m = re.match(r"^gains? (.+)$", p)
    if (
        m
        and _KWS.search(m.group(1))
        and not _KWS.sub("", m.group(1))
        .replace("and", "")
        .replace(",", "")
        .replace("/", "")
        .strip()
    ):
        return [d.KeywordGrant(d.Kw(k), int(n or 0)) for k, n in _KWS.findall(m.group(1))]
    m = re.match(r"^gains? (" + TRAITS_RE + r")$", p)
    if m:
        return [d.TraitGrant(traits_of(m.group(1)))]
    m = re.match(r"^gets cost ([+-]\d+)$", p)
    if m:
        return [d.CostMod(cost=int(m.group(1)))]
    rule = _rule_predicate(p, g)
    if rule is not None:
        return [d.RuleGrant(rule)]
    raise CompileError(f"unknown predicate {pred!r}")


def _count_value(phrase: str, g: G) -> d.Value:
    ph = phrase.strip()
    m = re.match(r"^(.+?) in (your trash|your opponent's trash|your hand)$", ph)
    if m:
        side, loc = _zone_side(m.group(2))
        spec = parse_selector("all " + m.group(1))
        return d.Count(d.Sel(side, loc, spec.sel.filters))
    spec = parse_selector("all " + ph, default_side=d.Side.FRIENDLY)
    return d.Count(spec.sel)


def _source_filters(desc: str) -> tuple[d.Side, tuple[d.Filter, ...]]:
    s = desc.strip()
    if not s:
        return d.Side.ANY, ()
    if s.lower() in (
        "an enemy",
        "enemies",
        "the enemy",
        "your opponent",
        "enemy effects",
        "an enemy effect",
    ):
        return d.Side.ENEMY, ()
    spec = parse_selector("1 " + re.sub(r"^(?:an?|any) ", "", s))
    return spec.sel.side, spec.sel.filters


def _rule_predicate(p: str, g: G) -> d.RuleMod | None:
    pl = p.lower()
    simple = {
        "can't attack": d.RuleKind.CANT_ATTACK,
        "can't choose the enemy player as its attack target": d.RuleKind.CANT_ATTACK_PLAYER,
        "can't choose the enemy player as their attack target": d.RuleKind.CANT_ATTACK_PLAYER,
        "can't attack the enemy player": d.RuleKind.CANT_ATTACK_PLAYER,
        "can't be blocked": d.RuleKind.CANT_BE_BLOCKED,
        "can't be set as active": d.RuleKind.CANT_BE_SET_ACTIVE,
        "can't be paired with a pilot": d.RuleKind.CANT_BE_PAIRED,
        "can't activate <blocker>": d.RuleKind.CANT_BLOCK,
        "can't block": d.RuleKind.CANT_BLOCK,
        "can attack on the turn it is deployed": d.RuleKind.ATTACK_ON_DEPLOY_TURN,
        "may attack on the turn it is deployed": d.RuleKind.ATTACK_ON_DEPLOY_TURN,
        "can attack during the turn it is deployed": d.RuleKind.ATTACK_ON_DEPLOY_TURN,
        "can't be chosen by enemy effects": d.RuleKind.CANT_BE_CHOSEN,
        "can't be returned to its owner's hand by enemy effects": d.RuleKind.CANT_BE_RETURNED,
    }
    if pl in simple:
        kind = simple[pl]
        side = d.Side.ENEMY if "enemy effects" in pl else d.Side.ANY
        return d.RuleMod(kind, source_side=side)
    if pl in ("can't be set as active or paired with a pilot",):
        return None
    m = re.match(
        r"^can't receive (enemy )?(battle |effect )?damage(?: from (.+?))?(?: that (?:is|are) Lv\.(\d+) (or lower|or higher))?$",
        p,
    )
    if m:
        dkind = {"battle ": d.DamageKind.BATTLE, "effect ": d.DamageKind.EFFECT}.get(
            m.group(2) or "", d.DamageKind.ANY
        )
        src = m.group(3) or ""
        side, filters = _source_filters(src) if src else (d.Side.ANY, ())
        if m.group(1):
            side = d.Side.ENEMY
        if m.group(4):
            filters = (*filters, d.StatCmp(d.Stat.LV, cmp_op(m.group(5)), int(m.group(4))))
        return d.RuleMod(
            d.RuleKind.CANT_RECEIVE_DAMAGE,
            damage_kind=dkind,
            source_filters=filters,
            source_side=side,
        )
    m = re.match(r"^(?:may|can) choose (.+?) as (?:its|their) attack target$", p)
    if m:
        spec = parse_selector(m.group(1))
        return d.RuleMod(d.RuleKind.MAY_ATTACK_ACTIVE, source_filters=spec.sel.filters)
    m = re.match(r"^can't be destroyed by (enemy |your opponent's )?effects$", p)
    if m:
        return d.RuleMod(
            d.RuleKind.CANT_BE_DESTROYED,
            damage_kind=d.DamageKind.EFFECT,
            source_side=d.Side.ENEMY if m.group(1) else d.Side.ANY,
        )
    if pl in (
        "can't have its ap reduced by enemy effects",
        "'s ap can't be reduced by enemy effects",
    ):
        return d.RuleMod(d.RuleKind.AP_CANT_BE_REDUCED, source_side=d.Side.ENEMY)
    return None


# ---------------------------------------------------------------------------------------------
# triggers from text


def parse_trigger(text: str, g: G) -> d.Trigger:
    t = _multiplayer(_strip_period(text))
    for rx, fn in _TRIGGERS:
        m = rx.match(t)
        if m:
            trig = fn(m, g)
            g.event = trig.event
            return trig
    raise CompileError(f"unknown trigger {t!r}")


def _subject_sel(phrase: str, default_side: d.Side = d.Side.FRIENDLY) -> tuple[d.Sel, bool]:
    ph = re.sub(r"^one of your ", "1 friendly ", phrase, flags=re.I)
    ph = re.sub(r"^another ", "1 other ", ph, flags=re.I)
    ph = re.sub(r"^(?:an?) ", "1 ", ph, flags=re.I)
    spec = parse_selector(ph, default_side=default_side)
    include_self = not any(isinstance(f, d.NotRef) for f in spec.sel.filters)
    filters = tuple(f for f in spec.sel.filters if not isinstance(f, d.NotRef))
    side = spec.sel.side if spec.sel.side is not d.Side.ANY else default_side
    return d.Sel(side, spec.sel.loc, filters), include_self


def _with_self(subj: str) -> tuple[bool, str]:
    m = re.match(r"^this Unit or (.+)$", subj, re.I)
    if m:
        return True, m.group(1)
    return False, subj


def _tr_destroys_unit(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    tf: tuple[d.Filter, ...] = (d.IsKind((d.CardKind.UNIT,)),)
    extra = (m.group(2) or "").strip()
    if extra:
        spec = parse_selector("1 enemy " + extra + " Unit")
        tf = spec.sel.filters
    if subj.lower() == "this unit":
        return d.Trigger(d.Ev.DESTROYS_BY_BATTLE, target_filters=tf)
    sel, inc = _subject_sel(subj)
    return d.Trigger(
        d.Ev.DESTROYS_BY_BATTLE, self_only=False, subject=sel, target_filters=tf, include_self=inc
    )


def _tr_destroys_shield(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    battle = True if m.group(2) and "battle" in m.group(2) else None
    if subj.lower() == "this unit":
        return d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, battle_only=battle)
    sel, inc = _subject_sel(subj)
    return d.Trigger(
        d.Ev.DESTROYS_SHIELD_CARD,
        self_only=False,
        subject=sel,
        battle_only=battle,
        include_self=inc,
    )


def _tr_deals_damage(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    battle = True if m.group(2) else None
    spec = parse_selector("1 " + m.group(3))
    tf = spec.sel.filters
    if subj.lower() == "this unit":
        return d.Trigger(d.Ev.DEALS_DAMAGE, battle_only=battle, target_filters=tf)
    sel, inc = _subject_sel(subj)
    return d.Trigger(
        d.Ev.DEALS_DAMAGE,
        self_only=False,
        subject=sel,
        battle_only=battle,
        target_filters=tf,
        include_self=inc,
    )


def _tr_deployed(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    if subj.lower() in ("this unit", "this base", "this card"):
        return d.Trigger(d.Ev.DEPLOYED)
    with_self, phrase = _with_self(subj)
    sel, inc = _subject_sel(phrase)
    return d.Trigger(d.Ev.DEPLOYED, self_only=False, subject=sel, include_self=inc or with_self)


def _tr_receives_damage(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    kind = ((m.group(2) or "") + " " + (m.group(3) or "")).lower()
    by_enemy = True if "enemy" in kind else None
    battle = True if "battle" in kind else (False if "effect" in kind else None)
    if subj.lower() in ("this unit", "this base"):
        return d.Trigger(d.Ev.DAMAGED, by_enemy=by_enemy, battle_only=battle)
    sel, inc = _subject_sel(subj)
    return d.Trigger(
        d.Ev.DAMAGED,
        self_only=False,
        subject=sel,
        by_enemy=by_enemy,
        battle_only=battle,
        include_self=inc,
    )


def _tr_ex_placed(m: re.Match[str], g: G) -> d.Trigger:
    return d.Trigger(
        d.Ev.EX_RESOURCE_PLACED,
        self_only=False,
        subject=d.Sel(d.Side.FRIENDLY, d.Loc.RESOURCE_AREA),
    )


def _tr_turn(m: re.Match[str], g: G) -> d.Trigger:
    ev = d.Ev.TURN_START if m.group(1).lower() == "start" else d.Ev.TURN_END
    whose = m.group(2).lower()
    wt = d.P.YOU if whose == "your turn" else (d.P.OPP if "opponent" in whose else None)
    return d.Trigger(ev, self_only=False, whose_turn=wt)


def _tr_destroyed(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    tail = (m.group(2) or "").lower()
    battle = True if "battle" in tail else (False if "effect" in tail else None)
    by_enemy = True if ("enemy" in tail or "opponent" in tail) else None
    if subj.lower() in ("this unit", "this base"):
        return d.Trigger(d.Ev.DESTROYED, battle_only=battle, by_enemy=by_enemy)
    with_self, phrase = _with_self(subj)
    sel, inc = _subject_sel(phrase)
    return d.Trigger(
        d.Ev.DESTROYED,
        self_only=False,
        subject=sel,
        include_self=inc or with_self,
        battle_only=battle,
        by_enemy=by_enemy,
    )


def _tr_attacks(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    tf: tuple[d.Filter, ...] = ()
    if m.group(2):
        tf = (d.IsKind((d.CardKind.UNIT,)),)
    if subj.lower() == "this unit":
        return d.Trigger(d.Ev.ATTACKS, target_filters=tf)
    with_self, phrase = _with_self(subj)
    sel, inc = _subject_sel(phrase)
    return d.Trigger(
        d.Ev.ATTACKS, self_only=False, subject=sel, include_self=inc or with_self, target_filters=tf
    )


def _tr_rested_by(m: re.Match[str], g: G) -> d.Trigger:
    by_enemy = True if m.group(2) else None
    subj = m.group(1)
    if subj.lower() == "this unit":
        return d.Trigger(d.Ev.RESTED, by_enemy=by_enemy)
    sel, inc = _subject_sel(subj)
    return d.Trigger(d.Ev.RESTED, self_only=False, subject=sel, by_enemy=by_enemy, include_self=inc)


def _tr_set_active(m: re.Match[str], g: G) -> d.Trigger:
    return d.Trigger(d.Ev.SET_ACTIVE, by_enemy=None)


def _tr_paired_pilot(m: re.Match[str], g: G) -> d.Trigger:
    pf: tuple[d.Filter, ...] = ()
    if m.group(1):
        pf = _pilot_filters(m.group(1).strip())
    target = m.group(2)
    if target.lower() == "this unit":
        return d.Trigger(d.Ev.PAIRED, pilot_filters=pf)
    with_self, phrase = _with_self(target)
    sel, inc = _subject_sel(phrase)
    return d.Trigger(
        d.Ev.PAIRED, self_only=False, subject=sel, pilot_filters=pf, include_self=inc or with_self
    )


def _tr_links(m: re.Match[str], g: G) -> d.Trigger:
    sel, inc = _subject_sel(m.group(1))
    return d.Trigger(d.Ev.LINKED, self_only=False, subject=sel, include_self=inc)


def _tr_recovers(m: re.Match[str], g: G) -> d.Trigger:
    return d.Trigger(d.Ev.RECOVERED)


def _tr_blocked(m: re.Match[str], g: G) -> d.Trigger:
    return d.Trigger(d.Ev.BLOCKED)


def _tr_after_main(m: re.Match[str], g: G) -> d.Trigger:
    return d.Trigger(d.Ev.COMMAND_RESOLVED)


def _tr_blocks(m: re.Match[str], g: G) -> d.Trigger:
    return d.Trigger(d.Ev.BLOCKS)


def _tr_shield_destroyed(m: re.Match[str], g: G) -> d.Trigger:
    side = d.Side.FRIENDLY if m.group(1).lower() in ("one of your", "your") else d.Side.ENEMY
    return d.Trigger(d.Ev.SHIELD_DESTROYED, self_only=False, subject=d.Sel(side, d.Loc.SHIELDS))


def _tr_discard(m: re.Match[str], g: G) -> d.Trigger:
    side = d.Side.ENEMY if "opponent" in m.group(1).lower() else d.Side.FRIENDLY
    return d.Trigger(d.Ev.DISCARDED, self_only=False, subject=d.Sel(side, d.Loc.TRASH))


def _tr_returned(m: re.Match[str], g: G) -> d.Trigger:
    subj = m.group(1)
    if subj.lower() in ("this unit", "this base"):
        return d.Trigger(d.Ev.RETURNED_TO_HAND)
    sel, inc = _subject_sel(subj)
    return d.Trigger(d.Ev.RETURNED_TO_HAND, self_only=False, subject=sel, include_self=inc)


def _tr_command(m: re.Match[str], g: G) -> d.Trigger:
    spec = parse_selector("1 " + m.group(1) + " card") if m.group(1) else None
    filters = spec.sel.filters if spec else (d.IsKind((d.CardKind.COMMAND,)),)
    return d.Trigger(
        d.Ev.COMMAND_PLAYED, self_only=False, subject=d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, filters)
    )


_TRIGGERS: list[tuple[re.Pattern[str], Callable[[re.Match[str], G], d.Trigger]]] = [
    (
        re.compile(
            r"^when (this Unit|one of your .+?|a friendly .+?|another friendly .+?) destroys an enemy (.*?)Unit with (?:battle )?damage$",
            re.I,
        ),
        _tr_destroys_unit,
    ),
    (
        re.compile(
            r"^when (this Unit|one of your .+?|a friendly .+?) destroys an enemy (?:shield area card|Shield)( with (?:battle )?damage)?$",
            re.I,
        ),
        _tr_destroys_shield,
    ),
    (
        re.compile(
            r"^when (this Unit|one of your .+?) deals (battle )?damage to (an enemy .+?)$", re.I
        ),
        _tr_deals_damage,
    ),
    (re.compile(r"^when (.+?) is deployed$", re.I), _tr_deployed),
    (
        re.compile(
            r"^when (this Unit|this Base|one of your .+?|a friendly .+?) receives ((?:enemy )?(?:battle |effect )?)damage( from an enemy)?$",
            re.I,
        ),
        _tr_receives_damage,
    ),
    (re.compile(r"^when you place an EX Resource$", re.I), _tr_ex_placed),
    (
        re.compile(
            r"^at the (start|end) of (your turn|your opponent's turn|the turn|each turn|a turn|every turn)$",
            re.I,
        ),
        _tr_turn,
    ),
    (
        re.compile(
            r"^when (.+?) is destroyed( with battle damage| with damage| by an? (?:enemy )?effect| by an enemy| by your opponent's effect)?$",
            re.I,
        ),
        _tr_destroyed,
    ),
    (
        re.compile(
            r"^when (this Unit|this Unit or .+?|another .+?|one of your .+?|an? .+?) attacks( an enemy Unit)?$",
            re.I,
        ),
        _tr_attacks,
    ),
    (
        re.compile(
            r"^when (this Unit|one of your .+?) is rested by an? (enemy |opponent's )?effect$", re.I
        ),
        _tr_rested_by,
    ),
    (
        re.compile(r"^when this (?:rested )?Unit is set as active by an effect$", re.I),
        _tr_set_active,
    ),
    (
        re.compile(
            r"^when you pair an? ((?:\([^)]+\)(?:/\([^)]+\))*|blue|green|red|white|purple) )?Pilot with (this Unit|this Unit or one of your .+?|one of your .+?)$",
            re.I,
        ),
        _tr_paired_pilot,
    ),
    (re.compile(r"^when (?:a|one of your|another) (.+?) links$", re.I), _tr_links),
    (re.compile(r"^when this Unit recovers HP$", re.I), _tr_recovers),
    (re.compile(r"^when this Unit is blocked(?: by an enemy Unit)?$", re.I), _tr_blocked),
    (re.compile(r"^when this Unit blocks$", re.I), _tr_blocks),
    (re.compile(r"^after activating this card's 【(?:Main|Action)】$", re.I), _tr_after_main),
    (
        re.compile(r"^when (one of your|your|an enemy) Shields? (?:is|are) destroyed$", re.I),
        _tr_shield_destroyed,
    ),
    (re.compile(r"^when (you|your opponent) discards?$", re.I), _tr_discard),
    (
        re.compile(
            r"^when (this Unit|one of your .+?) is returned to (?:its owner's|your) hand$", re.I
        ),
        _tr_returned,
    ),
    (
        re.compile(
            r"^when you (?:play|activate) (?:an? )?(.+?)?Command card(?:'s 【Main】/【Action】)?$",
            re.I,
        ),
        _tr_command,
    ),
]


# ---------------------------------------------------------------------------------------------
# core step templates (ordered: specific before generic)

StepFn = Callable[[re.Match[str], G], list[d.Step]]
_CORE: list[tuple[re.Pattern[str], StepFn]] = []


def core(pattern: str, flags: int = re.I) -> Callable[[StepFn], StepFn]:
    rx = re.compile("^(?:" + pattern + ")$", flags)

    def deco(fn: StepFn) -> StepFn:
        _CORE.append((rx, fn))
        return fn

    return deco


def _num(s: str | None, default: int = 1) -> int:
    if s is None:
        return default
    w = s.strip().lower()
    return {"a": 1, "an": 1, "one": 1, "two": 2, "three": 3}.get(
        w, int(w) if w.isdigit() else default
    )


@core(
    r"return the remaining cards? randomly to the bottom of your deck|return any remaining cards? (?:randomly )?to the bottom of your deck|return the rest (?:randomly )?to the bottom of your deck|return the other cards? (?:randomly )?to the bottom of your deck"
)
def _rest_bottom(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.CustomStep("return_looked_bottom")]


@core(r"return (it|this card|that card) to the top or bottom of your deck")
def _top_or_bottom(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1), g)
    return [d.Arrange(ref, "top_or_bottom")]


@core(r"(?:choose )?1 (?:enemy )?player|choose (?:an|1) enemy player")
def _choose_player(m: re.Match[str], g: G) -> list[d.Step]:
    return []


@core(r"choose (.+?) and (\d+ .+|an? .+|one .+)")
def _choose_two(m: re.Match[str], g: G) -> list[d.Step]:
    a = parse_selector(m.group(1))
    b = parse_selector(m.group(2))
    va, vb = g.var(), g.var()
    steps: list[d.Step] = [
        d.Choose(va, a.sel, a.count, a.min_count, after_then=g.after_then),
        d.Choose(vb, b.sel, b.count, b.min_count, distinct_from=(va,), after_then=g.after_then),
    ]
    g.it = d.Union((d.Var(va), d.Var(vb)))
    return steps


def _choose_step(phrase: str, g: G, optional: bool, chooser: d.P = d.P.YOU) -> list[d.Step]:
    spec = parse_selector(phrase)
    v = g.var()
    targeting = spec.sel.loc not in (d.Loc.HAND, d.Loc.DECK, d.Loc.DECK_TOP) and chooser is d.P.YOU
    g.it = d.Var(v)
    if spec.all:
        return [d.BindVar(v, d.All(spec.sel))]
    return [
        d.Choose(
            v,
            spec.sel,
            spec.count,
            spec.min_count,
            optional=optional,
            targeting=targeting,
            chooser=chooser,
            after_then=g.after_then,
        )
    ]


@core(r"(you may )?choose (.+)")
def _choose(m: re.Match[str], g: G) -> list[d.Step]:
    return _choose_step(m.group(2), g, bool(m.group(1)))


@core(r"your opponent chooses (.+)|(?:all enemy players each|your opponent each) choose (.+)")
def _opp_choose(m: re.Match[str], g: G) -> list[d.Step]:
    phrase = m.group(1) or m.group(2)
    phrase = re.sub(r"^(\d+|one) of their (?:own )?", r"\1 enemy ", phrase)
    phrase = phrase.replace("from their trash", "from your opponent's trash")
    return _choose_step(phrase, g, False, chooser=d.P.OPP)


@core(r"deal (\d+) damage to (the enemy player|your opponent)")
def _deal_player(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.DamagePlayer(d.P.OPP, int(m.group(1)))]


@core(
    r"deal (\d+) damage to the first (?:(\d+) )?cards? (?:in|of) (?:your opponent's|the enemy's|the enemy|an enemy|each enemy player's|that player's|the enemy player's) shield area"
)
def _deal_shield(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.DamageShieldArea(d.P.OPP, int(m.group(1)), cards=int(m.group(2) or 1))]


@core(r"deal (\d+) damage to (.+)")
def _deal(m: re.Match[str], g: G) -> list[d.Step]:
    steps, ref = ref_phrase(m.group(2), g)
    return [*steps, d.Damage(ref, int(m.group(1)))]


@core(r"deal damage to (.+?) equal to (.+)")
def _deal_var(m: re.Match[str], g: G) -> list[d.Step]:
    steps, ref = ref_phrase(m.group(1), g)
    amount = _amount_phrase(m.group(2), g, ref)
    return [*steps, d.Damage(ref, amount)]


def _amount_phrase(text: str, g: G, ref: d.Ref | None = None) -> d.Value:
    t = _strip_period(text)
    tl = t.lower()
    if tl in ("this unit's ap", "the ap of this unit"):
        return d.StatOf(d.This(), d.Stat.AP)
    if tl in ("its ap",) and g.it is not None:
        return d.StatOf(g.it, d.Stat.AP)
    m = re.match(r"^the number of (.+)$", t)
    if m:
        return _count_value(m.group(1), g)
    raise CompileError(f"unknown amount {text!r}")


def _simple_ref_step(ctor: Callable[[d.Ref], d.Step]) -> StepFn:
    def fn(m: re.Match[str], g: G) -> list[d.Step]:
        steps, ref = ref_phrase(m.group(1), g)
        return [*steps, ctor(ref)]

    return fn


core(r"rest (.+)")(_simple_ref_step(d.Rest))
core(r"destroy (.+)")(_simple_ref_step(d.Destroy))
core(r"exile (.+?) from the game")(_simple_ref_step(d.Exile))
core(r"place (.+?) into (?:its owner's|their owners'|your) trash")(_simple_ref_step(d.ToTrash))


@core(r"set (.+?) as active")
def _set_active(m: re.Match[str], g: G) -> list[d.Step]:
    phrase = m.group(1)
    mm = re.match(r"^(\d+) of your Resources$", phrase)
    if mm:
        return [d.SetResourcesActive(int(mm.group(1)))]
    steps, ref = ref_phrase(phrase, g)
    return [*steps, d.SetActive(ref)]


@core(r"return (.+?) to (?:its owner's|their owners'|your|its owners'|their owner's|their) hands?")
def _bounce(m: re.Match[str], g: G) -> list[d.Step]:
    steps, ref = ref_phrase(m.group(1), g)
    return [*steps, d.ReturnToHand(ref)]


@core(
    r"return (.+?) to the (top|bottom) of (?:its owner's|your|their owners'|their owner's|their) decks?"
)
def _to_deck(m: re.Match[str], g: G) -> list[d.Step]:
    steps, ref = ref_phrase(m.group(1), g)
    return [*steps, d.ToDeck(ref, bottom=m.group(2).lower() == "bottom")]


@core(
    r"return (.+?) to (?:its owner's|their owners'|their owner's|your) deck and shuffle (?:it|them|that deck|your deck)"
)
def _to_deck_shuffle(m: re.Match[str], g: G) -> list[d.Step]:
    steps, ref = ref_phrase(m.group(1), g)
    return [*steps, d.ToDeck(ref, bottom=True, shuffle=True)]


@core(r"add (\d+|one) of your Shields to your hand")
def _shield_to_hand(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.ShieldToHand(_num(m.group(1)))]


@core(r"add (.+?) to (?:your|its owner's|their owners'|their) hands?")
def _add_to_hand(m: re.Match[str], g: G) -> list[d.Step]:
    steps, ref = ref_phrase(m.group(1), g)
    return [*steps, d.AddToHand(ref)]


@core(r"deploy (this card|it|them|this Unit)( rested)?")
def _deploy_ref(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1), g)
    return [d.DeployCard(ref, rested=bool(m.group(2)))]


@core(r"pay its cost to deploy it|deploy it by paying its cost")
def _pay_deploy(m: re.Match[str], g: G) -> list[d.Step]:
    if g.it is None:
        raise CompileError("no antecedent")
    return [d.PlayCard(g.it)]


_TOKDEF = r"(\[[^\[\]]+\]\(\((?:[^()]|\([^()]*\))*\))"


@core(
    r"deploy (\d+|\d+ to \d+)( rested)? "
    + _TOKDEF
    + r" Unit tokens?(?: and (\d+)( rested)? "
    + _TOKDEF
    + r" Unit tokens?)?"
)
def _deploy_token(m: re.Match[str], g: G) -> list[d.Step]:
    def one(cnt: str, rested: bool, tdef: str, var: str) -> list[d.Step]:
        specs = parse_token_specs(tdef)
        if len(specs) != 1:
            raise CompileError("token definition")
        if " to " in cnt:
            lo, hi = (int(x) for x in cnt.split(" to "))
            out: list[d.Step] = [d.DeployToken(specs[0].key, lo, rested=rested, var=var)]
            if hi > lo:
                out.append(
                    d.May((d.DeployToken(specs[0].key, hi - lo, rested=rested, var=var + "_x"),))
                )
            return out
        return [d.DeployToken(specs[0].key, int(cnt), rested=rested, var=var)]

    steps = one(m.group(1), bool(m.group(2)), m.group(3), "tokens")
    g.it = d.Var("tokens")
    if m.group(6):
        steps += one(m.group(4), bool(m.group(5)), m.group(6), "tokens2")
        g.it = d.Union((d.Var("tokens"), d.Var("tokens2")))
    return steps


@core(r"deploy 1 EX Base")
def _deploy_ex_base(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.CustomStep("deploy_ex_base")]


@core(r"(you may )?deploy (\d+|an?) (.+?) (from your hand|from your trash|among them)( rested)?")
def _deploy_from(m: re.Match[str], g: G) -> list[d.Step]:
    where = m.group(4).lower()
    phrase = f"{m.group(2)} {m.group(3)}"
    spec = parse_selector(phrase)
    if where == "among them":
        sel = d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (*spec.sel.filters, d.IsRef(d.Var("looked"))))
    elif where == "from your hand":
        sel = d.Sel(d.Side.FRIENDLY, d.Loc.HAND, spec.sel.filters)
    else:
        sel = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, spec.sel.filters)
    v = g.var()
    g.it = d.Var(v)
    return [
        d.Choose(
            v,
            sel,
            spec.count,
            0 if m.group(1) else None,
            optional=bool(m.group(1)),
            targeting=where == "from your trash",
        ),
        d.DeployCard(d.Var(v), rested=bool(m.group(5))),
    ]


@core(r"draw (\d+)")
def _draw(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.Draw(int(m.group(1)))]


@core(r"all players (?:each )?draw (\d+)|you and your opponent each draw (\d+)")
def _all_draw(m: re.Match[str], g: G) -> list[d.Step]:
    n = int(m.group(1) or m.group(2))
    return [d.Draw(n, d.P.ACTIVE), d.Draw(n, d.P.STANDBY)]


@core(r"discard (\d+)")
def _discard(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.Discard(int(m.group(1)))]


@core(r"discard (\d+) (.+)")
def _discard_filtered(m: re.Match[str], g: G) -> list[d.Step]:
    spec = parse_selector("1 " + m.group(2))
    return [d.Discard(int(m.group(1)), filters=spec.sel.filters)]


@core(r"(?:your opponent|the enemy player|they|your opponent may) discards? (\d+)")
def _opp_discard(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.Discard(int(m.group(1)), player=d.P.OPP)]


@core(r"place (\d+) (rested )?EX Resources?")
def _place_ex(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.PlaceExResource(rested=bool(m.group(2)))] * int(m.group(1))


@core(r"place (\d+) (rested |active )?Resources?")
def _place_res(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.PlaceResource(rested=(m.group(2) or "").strip() == "rested")] * int(m.group(1))


@core(r"look at the top (\d+) cards of your deck")
def _look_n(m: re.Match[str], g: G) -> list[d.Step]:
    g.it = d.Var("looked")
    return [d.LookTop(int(m.group(1)))]


@core(r"look at the top card of your deck")
def _look_1(m: re.Match[str], g: G) -> list[d.Step]:
    g.it = d.Var("looked")
    return [d.LookTop(1)]


@core(r"(you may )?reveal (.+?) among them and add (?:it|them) to your hand")
def _reveal_add(m: re.Match[str], g: G) -> list[d.Step]:
    spec = parse_selector(m.group(2))
    sel = d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (*spec.sel.filters, d.IsRef(d.Var("looked"))))
    v = g.var()
    steps: list[d.Step] = [
        d.Choose(v, sel, spec.count, spec.min_count, optional=bool(m.group(1)), targeting=False),
        d.AddToHand(d.Var(v), reveal=True),
    ]
    g.it = d.Var(v)
    return steps


@core(r"(?:if it is|if that card is) (.+?), (?:you may )?reveal it and add it to your hand")
def _reveal_if(m: re.Match[str], g: G) -> list[d.Step]:
    spec = parse_selector("1 " + re.sub(r"^(?:an?|one) ", "", m.group(1)))
    sel = d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (*spec.sel.filters, d.IsRef(d.Var("looked"))))
    v = g.var()
    g.it = d.Var(v)
    return [d.Choose(v, sel, 1, optional=True, targeting=False), d.AddToHand(d.Var(v), reveal=True)]


@core(
    r"place the top (\d+) cards of your deck into your trash|place the top card of your deck into your trash"
)
def _mill(m: re.Match[str], g: G) -> list[d.Step]:
    g.it = d.Var("milled")
    return [d.Mill(int(m.group(1) or 1))]


@core(
    r"place the top (\d+) cards? of (?:your opponent's|the enemy's|their) deck into (?:their|your opponent's) trash"
)
def _mill_opp(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.Mill(int(m.group(1)), player=d.P.OPP, var="milled_opp")]


@core(r"(.+?) recovers? (\d+) HP(?: and (gets .+))?")
def _recover(m: re.Match[str], g: G) -> list[d.Step]:
    steps, ref = ref_phrase(m.group(1), g)
    out: list[d.Step] = [*steps, d.Recover(ref, int(m.group(2)))]
    if m.group(3):
        pred, dur = split_duration(m.group(3))
        out += [
            d.Apply(ref, e, dur or Duration.WHILE_ON_FIELD)
            for e in continuous_from_predicate(pred, g)
        ]
    return out


@core(r"activate this card's 【(?:Main|Action)】")
def _activate_main(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.ActivateMain(d.ThisCard())]


@core(
    r"activate (?:the )?【Main】 (?:of|on) the (?:Command )?card paired with this Unit|activate the 【Main】 of its paired Command card"
)
def _activate_paired_main(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.ActivateMain(d.PairedPilotOf(d.This()))]


@core(r"shuffle your deck")
def _shuffle(m: re.Match[str], g: G) -> list[d.Step]:
    return [d.Shuffle()]


@core(r"pay (\S+)")
def _pay(m: re.Match[str], g: G) -> list[d.Step]:
    sym = m.group(1)
    if not all(ch in RES_SYMBOLS for ch in sym):
        raise CompileError(f"unknown payment {sym!r}")
    return [d.PayCost(sum(RES_SYMBOLS[ch] for ch in sym))]


@core(
    r"(it|they|this Unit|this Base|that Unit|them) (?:won't|will not) be set as active during the start phase of your opponent's next turn"
)
def _freeze(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1), g)
    return [
        d.Apply(
            ref, d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BE_SET_ACTIVE)), Duration.OPPONENT_NEXT_TURN
        )
    ]


@core(
    r"change (?:a|the) battling enemy Unit's attack target to (it|this Unit)|change the attack target of the battling enemy Unit to (it|this Unit)"
)
def _change_target(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1) or m.group(2), g)
    return [d.ChangeAttackTarget(ref)]


@core(
    r"begin a battle between (this Unit|them|it) and (it|them|this Unit)? ?and only perform the damage step|begin a battle between them and only perform the damage step"
)
def _begin_battle(m: re.Match[str], g: G) -> list[d.Step]:
    if m.group(1) and m.group(1).lower() == "this unit" and m.group(2):
        _, r2 = ref_phrase(m.group(2), g)
        return [d.StartBattle(d.This(), r2)]
    if g.it is None:
        raise CompileError("no antecedent for battle")
    it = g.it
    if isinstance(it, d.Union) and len(it.refs) == 2:
        return [d.StartBattle(it.refs[0], it.refs[1])]
    raise CompileError("battle participants unclear")


@core(r"(you may )?pair (\d+|an?) (.+?) from your (hand|trash) with (this Unit|it)")
def _pair_from(m: re.Match[str], g: G) -> list[d.Step]:
    spec = parse_selector(f"{m.group(2)} {m.group(3)}")
    loc = d.Loc.HAND if m.group(4) == "hand" else d.Loc.TRASH
    v = g.var()
    _, unit = ref_phrase(m.group(5), g)
    return [
        d.Choose(
            v,
            d.Sel(d.Side.FRIENDLY, loc, spec.sel.filters),
            1,
            optional=bool(m.group(1)),
            targeting=loc is d.Loc.TRASH,
        ),
        d.Pair(d.Var(v), unit),
    ]


@core(r"pair (it|this card) with (this Unit|it)")
def _pair_it(m: re.Match[str], g: G) -> list[d.Step]:
    _, pilot = ref_phrase(m.group(1), g)
    _, unit = ref_phrase(m.group(2), g) if m.group(2).lower() != "it" else ([], d.This())
    return [d.Pair(pilot, unit)]


@core(r"(?:you may )?pair this card from your trash with (.+)")
def _pair_self_from_trash(m: re.Match[str], g: G) -> list[d.Step]:
    spec = parse_selector(m.group(1))
    v = g.var()
    return [
        d.May(
            (
                d.Choose(v, spec.sel, 1, targeting=True),
                d.Pair(d.ThisCard(), d.Var(v)),
            )
        )
    ]


@core(r"(.+?) (gets? AP[+-]\d+ for (?:each|every) .+?) (?:during this turn|during this battle)")
def _apply_per(m: re.Match[str], g: G) -> list[d.Step]:
    dur = Duration.THIS_BATTLE if m.group(0).endswith("battle") else Duration.THIS_TURN
    steps, ref = ref_phrase(m.group(1), g)
    return [*steps, *(d.Apply(ref, e, dur) for e in continuous_from_predicate(m.group(2), g))]


@core(
    r"for each (.+?), (it|this Unit) gets (AP|HP)([+-])(\d+) (during this turn|during this battle)"
)
def _for_each_apply(m: re.Match[str], g: G) -> list[d.Step]:
    v = _count_value(m.group(1), g)
    _, ref = ref_phrase(m.group(2), g)
    amt = d.Times(v, int(m.group(5)) * (1 if m.group(4) == "+" else -1))
    eff = d.StatMod(ap=amt) if m.group(3) == "AP" else d.StatMod(hp=amt)
    dur = Duration.THIS_TURN if m.group(6).endswith("turn") else Duration.THIS_BATTLE
    return [d.Apply(ref, eff, dur)]


@core(
    r"reduce (?:its|it's|this Unit's) AP (?:during this (turn|battle) )?by an amount equal to (.+?)(?: during this (turn|battle))?"
)
def _reduce_ap_by(m: re.Match[str], g: G) -> list[d.Step]:
    which = m.group(1) or m.group(3) or "turn"
    dur = Duration.THIS_TURN if which == "turn" else Duration.THIS_BATTLE
    amt = _amount_phrase(m.group(2), g)
    ref = g.it or d.This()
    return [d.Apply(ref, d.StatMod(ap=d.Times(amt, -1)), dur)]


@core(
    r"(?:reduce )?the next damage (it|this Unit) receives (?:is reduced )?by (\d+) during this (turn|battle)|reduce the next damage (it|this Unit) receives by (\d+) during this (turn|battle)"
)
def _next_damage(m: re.Match[str], g: G) -> list[d.Step]:
    who = m.group(1) or m.group(4)
    amt = int(m.group(2) or m.group(5))
    which = m.group(3) or m.group(6)
    _, ref = ref_phrase(who, g)
    rule = d.RuleMod(d.RuleKind.REDUCE_DAMAGE, amount=amt, once_per_turn=True)
    return [
        d.Apply(
            ref, d.RuleGrant(rule), Duration.THIS_TURN if which == "turn" else Duration.THIS_BATTLE
        )
    ]


@core(r"reduce battle damage (it|this Unit) receives by (\d+) during this (turn|battle)")
def _reduce_battle_damage(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1), g)
    rule = d.RuleMod(
        d.RuleKind.REDUCE_DAMAGE, amount=int(m.group(2)), damage_kind=d.DamageKind.BATTLE
    )
    return [
        d.Apply(
            ref,
            d.RuleGrant(rule),
            Duration.THIS_TURN if m.group(3) == "turn" else Duration.THIS_BATTLE,
        )
    ]


@core(
    r"when (it|this Unit|they) receives? ((?:enemy )?(?:battle )?)damage(?: from an enemy Unit)?, reduce it by (\d+) during this (turn|battle)"
)
def _reduce_when(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1), g)
    kind = (
        d.DamageKind.BATTLE if "battle" in m.group(2) or "Unit" in m.group(0) else d.DamageKind.ANY
    )
    side = d.Side.ENEMY if "enemy" in m.group(0) else d.Side.ANY
    rule = d.RuleMod(
        d.RuleKind.REDUCE_DAMAGE, amount=int(m.group(3)), damage_kind=kind, source_side=side
    )
    return [
        d.Apply(
            ref,
            d.RuleGrant(rule),
            Duration.THIS_TURN if m.group(4) == "turn" else Duration.THIS_BATTLE,
        )
    ]


@core(
    r"(your shield area cards|cards in your shield area) can't receive damage from (.+?) during this (battle|turn)"
)
def _protect_shields(m: re.Match[str], g: G) -> list[d.Step]:
    side, filters = _source_filters(m.group(2))
    rule = d.RuleMod(d.RuleKind.SHIELD_AREA_PROTECTION, source_filters=filters, source_side=side)
    dur = Duration.THIS_BATTLE if m.group(3) == "battle" else Duration.THIS_TURN
    return [d.ApplyPlayer(d.P.YOU, d.RuleGrant(rule), dur)]


@core(
    r"enemy Units can't choose (it|this Unit|them) as their attack target (?:during )?this (turn|battle)"
)
def _cant_be_attacked(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1), g)
    dur = Duration.THIS_TURN if m.group(2) == "turn" else Duration.THIS_BATTLE
    return [d.Apply(ref, d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BE_ATTACKED)), dur)]


@core(
    r"all enemy Units must choose (that Unit|it|this Unit) as their attack target(?: if possible)? when attacking during this turn"
)
def _force_target(m: re.Match[str], g: G) -> list[d.Step]:
    _, ref = ref_phrase(m.group(1), g)
    return [
        d.Apply(ref, d.RuleGrant(d.RuleMod(d.RuleKind.FORCE_ATTACK_TARGET)), Duration.THIS_TURN)
    ]


@core(
    r"(?:(.+?) that are Lv\.(\d+) (or lower|or higher)|(.+?)) can't activate <Blocker> during this (battle|turn)"
)
def _no_block(m: re.Match[str], g: G) -> list[d.Step]:
    phrase = m.group(1) or m.group(4)
    spec = parse_selector(
        "all "
        + ("enemy " if not phrase.lower().startswith(("enemy", "all", "friendly")) else "")
        + phrase
    )
    sel = spec.sel
    if m.group(2):
        sel = d.Sel(
            sel.side,
            sel.loc,
            (*sel.filters, d.StatCmp(d.Stat.LV, cmp_op(m.group(3)), int(m.group(2)))),
        )
    dur = Duration.THIS_BATTLE if m.group(5) == "battle" else Duration.THIS_TURN
    return [d.Apply(d.All(sel), d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BLOCK)), dur)]


@core(
    r"when (it|they|this Unit|that Unit) (destroys? an enemy (?:.*?)Unit with battle damage|destroys? an enemy (?:card|shield area card) with battle damage|deals? battle damage to an enemy Unit.*?)(?: during this turn)?, (.+)"
)
def _delayed_when(m: re.Match[str], g: G) -> list[d.Step]:
    who = m.group(1).lower()
    subject_text = "this Unit " + re.sub(
        r"^destroy\b", "destroys", re.sub(r"^deal\b", "deals", m.group(2))
    )
    subject_text = re.sub(r" during this turn$", "", subject_text)
    trig = parse_trigger("when " + subject_text, g)
    bind: tuple[str, ...] = ()
    if who in ("it", "they", "that unit") and isinstance(g.it, d.Var):
        bind = (g.it.name,)
    inner_g = G(g.cdef, n=g.n + 10, it=None, event=trig.event)
    steps: list[d.Step] = []
    compile_sentence(m.group(3), inner_g, steps)
    if bind:
        return [d.DelayedTrigger(trig, tuple(steps), Duration.THIS_TURN, bind_vars=bind)]
    return [d.DelayedTrigger(trig, tuple(steps), Duration.THIS_TURN)]


@core(r"when this effect destroys an enemy Unit, (.+)")
def _reflexive(m: re.Match[str], g: G) -> list[d.Step]:
    if not isinstance(g.it, d.Var):
        raise CompileError("reflexive trigger without target")
    inner: list[d.Step] = []
    compile_sentence(m.group(1), g, inner)
    cond = d.HappenedThisTurn("destroyed", d.P.OPP, filters=(d.IsRef(g.it),))
    return [d.If(cond, tuple(inner))]


def _lasting(subject: str, pred: str, g: G, default: Duration) -> list[d.Step]:
    steps, ref = ref_phrase(subject, g)
    pred2, dur = split_duration(pred)
    effs = continuous_from_predicate(pred2, g)
    return [*steps, *(d.Apply(ref, e, dur or default) for e in effs)]


@core(
    r"(it|they|them|this Unit|this Base|this|that Unit|the enemy Unit|the attacking enemy Unit|all .+?|enemy Units.*?|friendly Units.*?|your Units.*?|all your .+?|(?:\d+|a) .+?) (gets? .+|gains? .+|can't .+|may choose .+|can choose .+|may attack .+|can attack .+)"
)
def _apply_pred(m: re.Match[str], g: G) -> list[d.Step]:
    subject = m.group(1)
    sl = subject.lower()
    if sl.startswith(("enemy units", "friendly units", "your units")):
        subject = "all " + subject
    return _lasting(subject, m.group(2), g, Duration.WHILE_ON_FIELD)


# ---------------------------------------------------------------------------------------------
# sentence compiler


def compile_steps(body: str, g: G) -> tuple[d.Step, ...]:
    steps: list[d.Step] = []
    sentences = split_sentences(body)
    i = 0
    while i < len(sentences):
        s = sentences[i]
        if re.search(r"choose 1 of the following effects|activate the following effect", s, re.I):
            options = []
            j = i + 1
            while j < len(sentences) and sentences[j].startswith("■"):
                options.append(sentences[j][1:].strip())
                j += 1
            opt_bodies: list[str] = []
            for o in options:
                if opt_bodies and not o:
                    continue
                opt_bodies.append(o)
            k = j
            while k < len(sentences) and not sentences[k].startswith("■") and opt_bodies:
                opt_bodies[-1] += " " + sentences[k]
                k += 1
            _compile_option_block(s, opt_bodies, g, steps)
            i = k
            continue
        compile_sentence(s, g, steps)
        i += 1
    return tuple(steps)


def _compile_option_block(lead: str, options: list[str], g: G, steps: list[d.Step]) -> None:
    if re.search(r"choose 1 of the following effects", lead, re.I):
        opts: list[tuple[str, tuple[d.Step, ...]]] = []
        for idx, o in enumerate(options):
            og = G(g.cdef, n=g.n + 20 * (idx + 1), event=g.event)
            opts.append((f"option{idx + 1}", compile_steps(o, og)))
        steps.append(d.ChooseMode(tuple(opts)))
        return
    # Development (rule 13-1-8) / "If you do, activate the following effect:"
    lead_s = _strip_period(lead.rstrip(":"))
    m = re.match(
        r"^You may exile the specified number of (.+?) in your trash from the game\. If you do, activate the following effect$",
        lead_s,
    )
    if m and g.development:
        spec = parse_selector("all " + m.group(1))
        sel = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, spec.sel.filters)
        v = g.var()
        body = compile_steps(" ".join(options), g)
        steps.append(
            d.If(
                d.Exists(sel, g.development),
                (
                    d.May(
                        (
                            d.Choose(v, sel, g.development, targeting=True),
                            d.Exile(d.Var(v)),
                            *body,
                        )
                    ),
                ),
            )
        )
        return
    raise CompileError(f"unknown option block {lead!r}")


def _try_condition(text: str, g: G) -> d.Cond | None:
    try:
        return parse_condition(text, g)
    except CompileError:
        return None


def compile_sentence(sentence: str, g: G, steps: list[d.Step]) -> None:
    s = _multiplayer(_strip_period(sentence))
    if s.startswith("■"):
        s = s[1:].strip()
    m = re.match(r"^Then, (.+)$", s, re.I)
    if m:
        g.after_then = True
        compile_sentence(_cap(m.group(1)), g, steps)
        return
    m = re.match(r"^If you do, (.+)$", s, re.I)
    if m:
        inner: list[d.Step] = []
        compile_sentence(_cap(m.group(1)), g, inner)
        steps.append(d.IfYouDo(tuple(inner)))
        return
    m = re.match(r"^If (.+?), (.+?) instead$", s, re.I)
    if m and steps:
        cond = parse_condition(m.group(1), g)
        alt: list[d.Step] = []
        compile_sentence(_cap(m.group(2)), g, alt)
        prev = steps.pop()
        steps.append(d.If(cond, tuple(alt), (prev,)))
        return
    m = re.match(r"^During (this turn|this battle), (.+)$", s, re.I)
    if m:
        body = m.group(2)
        if re.match(r"^when ", body, re.I):
            compile_sentence(_cap(body), g, steps)
            return
        if not re.search(r"(?:during )?this (turn|battle)$", body):
            body = body + " during " + m.group(1).lower()
        compile_sentence(_cap(body), g, steps)
        return
    m = re.match(r"^During your turn, (.+)$", s, re.I)
    if m:
        inner = []
        compile_sentence(_cap(m.group(1)), g, inner)
        steps.append(d.If(d.IsTurn(d.P.YOU), tuple(inner)))
        return
    m = re.match(r"^If (.+?), (.+)$", s, re.I)
    if m and not re.match(r"^if (it is|that card is) .+?, (?:you may )?reveal it", s, re.I):
        opt_cond = _try_condition(m.group(1), g)
        if opt_cond is not None:
            inner = []
            compile_sentence(_cap(m.group(2)), g, inner)
            steps.append(d.If(opt_cond, tuple(inner)))
            return
    m = re.match(r"^You may (?!choose)(.+)$", s, re.I)
    if m:
        inner = []
        compile_sentence(_cap(m.group(1)), g, inner)
        steps.append(d.May(tuple(inner)))
        return
    m = re.match(r"^(.+?) if (you have .+|there .+|it is your turn|you are Lv\..+)$", s)
    if m and not re.match(r"^(?:reveal|choose)", s, re.I):
        opt_cond = _try_condition(m.group(2), g)
        if opt_cond is not None:
            inner = []
            compile_sentence(_cap(m.group(1)), g, inner)
            steps.append(d.If(opt_cond, tuple(inner)))
            return
    for rx, fn in _CORE:
        mm = rx.match(s)
        if mm:
            steps.extend(fn(mm, g))
            return
    raise CompileError(f"no step template for {s!r}")


# ---------------------------------------------------------------------------------------------
# costs


def parse_costs(text: str, g: G) -> tuple[d.Cost, ...]:
    out: list[d.Cost] = []
    for part in re.split(r",\s*|\s*･\s*", text.strip()):
        p = part.strip()
        if not p:
            continue
        if all(ch in RES_SYMBOLS for ch in p):
            out.append(d.PayResources(sum(RES_SYMBOLS[ch] for ch in p)))
            continue
        pl = p.lower()
        if pl in ("rest this unit", "rest this base"):
            out.append(d.RestSelf())
            continue
        if pl in ("destroy this unit", "destroy this base"):
            out.append(d.DestroySelf())
            continue
        if pl in ("return this unit to your hand", "return this card to your hand"):
            out.append(d.ReturnSelf())
            continue
        m = re.match(r"^rest (\d+) (.+)$", p, re.I)
        if m:
            spec = parse_selector(m.group(1) + " " + m.group(2))
            out.append(d.RestCards(_friendly(spec.sel), int(m.group(1))))
            continue
        m = re.match(r"^destroy (\d+) (.+)$", p, re.I)
        if m:
            spec = parse_selector(m.group(1) + " " + m.group(2))
            out.append(d.DestroyCards(_friendly(spec.sel), int(m.group(1))))
            continue
        m = re.match(r"^exile (\d+) (.+?) (?:from|in) your trash(?: from the game)?$", p, re.I)
        if m:
            spec = parse_selector(m.group(1) + " " + m.group(2))
            out.append(
                d.ExileCards(d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, spec.sel.filters), int(m.group(1)))
            )
            continue
        m = re.match(r"^discard (\d+)(?: (.+))?$", p, re.I)
        if m:
            filters: tuple[d.Filter, ...] = ()
            if m.group(2):
                filters = parse_selector("1 " + m.group(2)).sel.filters
            out.append(d.DiscardCards(d.Sel(d.Side.FRIENDLY, d.Loc.HAND, filters), int(m.group(1))))
            continue
        raise CompileError(f"unknown cost {p!r}")
    return tuple(out)


# ---------------------------------------------------------------------------------------------
# constant (untagged) abilities


_CONST_SUBJECT = (
    r"This Unit|This Base|This|this Unit|this|it|It|All .+?|all .+?|"
    r"Enemy Units.*?|Friendly .+?|Your .+?|friendly .+?|your .+?|enemy Units.*?"
)


def compile_constant_sentence(
    s: str, g: G, cond: d.Cond, gate: d.Gate, gf: tuple[d.Filter, ...]
) -> list[d.Ability]:
    s = _strip_period(s)
    m = re.match(r"^This card in your (hand|trash) gets cost ([+-]\d+)$", s, re.I)
    if m:
        where = d.Where.HAND if m.group(1) == "hand" else d.Where.TRASH
        return [d.Constant((d.CostMod(cost=int(m.group(2))),), d.This(), cond, where=where)]
    m = re.match(r"^This card in your hand gets Lv\. ?([+-]\d+) and cost ([+-]\d+)$", s, re.I)
    if m:
        return [
            d.Constant(
                (d.CostMod(cost=int(m.group(2)), level=int(m.group(1))),),
                d.This(),
                cond,
                where=d.Where.HAND,
            )
        ]
    m = re.match(r"^This card's name is also treated as \[(.+)\]$", s)
    if m:
        raise CompileError("name alias requires a binding")
    m = re.match(
        r"^("
        + _CONST_SUBJECT
        + r") (gets? .+|gains? .+|can't .+|may choose .+|can choose .+|can attack .+|may attack .+)$",
        s,
    )
    if m:
        subject = m.group(1)
        pred, dur = split_duration(m.group(2))
        dcond = cond
        if pred.endswith(" during your opponent's turn"):
            pred = pred[: -len(" during your opponent's turn")]
            dcond = d.IsTurn(d.P.OPP) if cond is d.TRUE else d.And((cond, d.IsTurn(d.P.OPP)))
        elif pred.endswith(" during your turn"):
            pred = pred[: -len(" during your turn")]
            dcond = d.IsTurn(d.P.YOU) if cond is d.TRUE else d.And((cond, d.IsTurn(d.P.YOU)))
        if dur is not None:
            raise CompileError("duration inside constant")
        effs = continuous_from_predicate(pred, g)
        scope: d.Ref
        sl = subject.lower()
        if sl in ("this unit", "this base", "this", "it"):
            scope = d.This()
        else:
            phrase = subject if sl.startswith("all ") else "all " + subject
            phrase = re.sub(r"^all your other ", "all other friendly ", phrase, flags=re.I)
            spec = parse_selector(phrase)
            scope = d.All(spec.sel)
        return [d.Constant(tuple(effs), scope, dcond, gate=gate, gate_filters=gf)]
    m = re.match(
        r"^Enemy Units choose this (?:rested )?Unit as their attack target if possible(?: when attacking)?$",
        s,
    )
    if m:
        return [
            d.Constant(
                (d.RuleGrant(d.RuleMod(d.RuleKind.FORCE_ATTACK_TARGET)),),
                d.This(),
                cond,
                gate=gate,
                gate_filters=gf,
            )
        ]
    m = re.match(r"^Enemy Units can't choose this Unit as their attack target$", s)
    if m:
        return [
            d.Constant(
                (d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BE_ATTACKED)),),
                d.This(),
                cond,
                gate=gate,
                gate_filters=gf,
            )
        ]
    m = re.match(
        r"^On the turn this Unit is deployed, it may choose a rested enemy Unit as its attack target and attack it$",
        s,
    )
    if m:
        rule = d.RuleMod(
            d.RuleKind.ATTACK_ON_DEPLOY_TURN, name="units_only", source_filters=(d.IsRested(True),)
        )
        return [d.Constant((d.RuleGrant(rule),), d.This(), cond, gate=gate, gate_filters=gf)]
    raise CompileError(f"unknown constant {s!r}")


def compile_untagged(
    body: str, g: G, gate: d.Gate, gf: tuple[d.Filter, ...], once: bool
) -> list[d.Ability]:
    kws = parse_keyword_line(body)
    if kws is not None:
        return [d.Keyword(k.keyword, k.amount, gate=gate, gate_filters=gf) for k in kws]
    sentences = split_sentences(body)
    first = _multiplayer(_strip_period(sentences[0]))
    whose: d.P | None = None
    scope_cond: d.Cond = d.TRUE
    m = re.match(r"^During (your turn|your opponent's turn), (.+)$", first, re.I)
    if m:
        whose = d.P.YOU if m.group(1).lower() == "your turn" else d.P.OPP
        scope_cond = d.IsTurn(whose)
        first = m.group(2)
    m = re.match(r"^((?:When|At the (?:start|end) of|After) .+?), (.+)$", first, re.I)
    if m:
        red = _damage_reduction(
            m.group(1), m.group(2), sentences[1:], g, once, gate, gf, scope_cond
        )
        if red is not None:
            return red
        trig = parse_trigger(m.group(1), g)
        if whose is not None:
            trig = d.Trigger(
                trig.event,
                trig.self_only,
                trig.subject,
                whose,
                trig.pilot_filters,
                trig.target_filters,
                trig.battle_only,
                trig.by_enemy,
                trig.include_self,
            )
        where = d.Where.TRASH if trig.event is d.Ev.COMMAND_RESOLVED else d.Where.FIELD
        steps = compile_steps(" ".join([_cap(m.group(2)) + ".", *sentences[1:]]), g)
        return [
            d.Triggered(trig, steps, once_per_turn=once, gate=gate, gate_filters=gf, where=where)
        ]
    out: list[d.Ability] = []
    for idx, sent in enumerate([first, *sentences[1:]]):
        s = _multiplayer(_strip_period(sent))
        cond = scope_cond if idx == 0 else d.TRUE
        mm = re.match(r"^(?:While|If) (.+?), (.+)$", s)
        if mm:
            c = parse_condition(mm.group(1), g)
            cond = c if cond is d.TRUE else d.And((cond, c))
            s = _cap(mm.group(2))
            mm2 = re.match(r"^During (your turn|your opponent's turn), (.+)$", s, re.I)
            if mm2:
                t = d.IsTurn(d.P.YOU if mm2.group(1).lower() == "your turn" else d.P.OPP)
                cond = d.And((cond, t))
                s = _cap(mm2.group(2))
        mm = re.match(r"^(.+?) while (.+)$", s)
        if mm and not re.search(r"while (?:this|it) is attacking", s):
            try:
                c = parse_condition(mm.group(2), g)
                cond = c if cond is d.TRUE else d.And((cond, c))
                s = mm.group(1)
            except CompileError:
                pass
        out.extend(compile_constant_sentence(s, g, cond, gate, gf))
    return out


def _damage_reduction(
    trig_text: str,
    rest: str,
    more: list[str],
    g: G,
    once: bool,
    gate: d.Gate,
    gf: tuple[d.Filter, ...],
    cond: d.Cond,
) -> list[d.Ability] | None:
    """ "When X receives (enemy) (battle/effect) damage, reduce it by N" modifies damage before it
    is received (rule 5-21), so it compiles to a constant damage-reduction rule."""
    m = re.match(r"^reduce (?:it|that damage|the damage) by (\d+)$", _strip_period(rest), re.I)
    if not m or more:
        return None
    t = re.match(
        r"^when (this Unit|this Base|one of your .+?|a friendly .+?) receives ((?:enemy )?(?:battle |effect )?)damage( from an enemy)?$",
        _strip_period(trig_text),
        re.I,
    )
    if not t:
        return None
    kind_txt = (t.group(2) + (t.group(3) or "")).lower()
    kind = (
        d.DamageKind.BATTLE
        if "battle" in kind_txt
        else (d.DamageKind.EFFECT if "effect" in kind_txt else d.DamageKind.ANY)
    )
    side = d.Side.ENEMY if "enemy" in kind_txt else d.Side.ANY
    rule = d.RuleMod(
        d.RuleKind.REDUCE_DAMAGE,
        amount=int(m.group(1)),
        damage_kind=kind,
        source_side=side,
        once_per_turn=once,
    )
    subj = t.group(1).lower()
    scope: d.Ref = d.This()
    if subj not in ("this unit", "this base"):
        spec = parse_selector(
            "all " + re.sub(r"^(one of your|a friendly) ", "friendly ", t.group(1), flags=re.I)
        )
        scope = d.All(spec.sel)
    return [d.Constant((d.RuleGrant(rule),), scope, cond, gate=gate, gate_filters=gf)]


# ---------------------------------------------------------------------------------------------
# marker chains


def _gate_qualifier(q: str) -> tuple[d.Filter, ...]:
    q = q.strip()
    m = re.match(r"^(" + TRAITS_RE + r") Pilot$", q)
    if m:
        return (d.HasTrait(traits_of(m.group(1))),)
    m = re.match(r"^(Blue|Green|Red|White|Purple) Pilot$", q, re.I)
    if m:
        return (d.HasColor((m.group(1).capitalize(),)),)
    m = re.match(r"^Lv\.(\d+) (or higher|or lower) Pilot$", q, re.I)
    if m:
        return (d.StatCmp(d.Stat.LV, cmp_op(m.group(2)), int(m.group(1))),)
    raise CompileError(f"unknown pilot qualifier {q!r}")


def compile_marked(markers: list[str], body: str, cdef: CardDef) -> list[d.Ability]:
    g = G(cdef)
    gate = d.Gate.NONE
    gf: tuple[d.Filter, ...] = ()
    once = False
    timings: list[str] = []
    for mk in markers:
        mk = mk.replace(" ･", "･").replace("･ ", "･")
        if mk == "Once per Turn":
            once = True
        elif mk.startswith("During Pair"):
            gate = d.Gate.PAIRED
            if "･" in mk:
                gf = _gate_qualifier(mk.split("･", 1)[1])
        elif mk == "During Link":
            gate = d.Gate.LINKED
        else:
            timings.append(mk)
    if not timings:
        return compile_untagged(body, g, gate, gf, once)
    out: list[d.Ability] = []
    for t in timings:
        out.extend(_compile_timed(t, body, cdef, gate, gf, once))
    return merge_command_timings(out)


def _compile_timed(
    t: str, body: str, cdef: CardDef, gate: d.Gate, gf: tuple[d.Filter, ...], once: bool
) -> list[d.Ability]:
    g = G(cdef)
    dev = re.match(r"^(Deploy|When Paired|When Linked)･Development (\d+)$", t)
    if dev:
        g.development = int(dev.group(2))
        t = dev.group(1)
    if t == "Burst":
        g.burst = True
        return [d.Burst(compile_steps(body, g))]
    trig: d.Trigger | None = None
    if t == "Deploy":
        trig = d.Trigger(d.Ev.DEPLOYED)
    elif t == "Attack":
        trig = d.Trigger(d.Ev.ATTACKS)
    elif t == "Destroyed":
        trig = d.Trigger(d.Ev.DESTROYED)
    elif t == "When Linked":
        trig = d.Trigger(d.Ev.LINKED)
    elif t.startswith("When Paired"):
        pf: tuple[d.Filter, ...] = ()
        if "･" in t:
            pf = _gate_qualifier(t.split("･", 1)[1])
        trig = d.Trigger(d.Ev.PAIRED, pilot_filters=pf)
    if trig is not None:
        g.event = trig.event
        return [
            d.Triggered(
                trig, compile_steps(body, g), once_per_turn=once, gate=gate, gate_filters=gf
            )
        ]
    if t in ("Activate･Main", "Activate･Action"):
        timing = d.Timing.MAIN if t.endswith("Main") else d.Timing.ACTION
        kws = parse_keyword_line(body)
        if kws is not None:
            return [d.Keyword(k.keyword, k.amount, gate=gate, gate_filters=gf) for k in kws]
        costs: tuple[d.Cost, ...] = ()
        mm = re.match(r"^([^:]*?):(.+)$", body, re.S)
        if mm and not re.search(r"[.]", mm.group(1)):
            costs = parse_costs(mm.group(1), g)
            body = mm.group(2).strip()
        return [
            d.Activated(
                timing,
                costs,
                compile_steps(body, g),
                once_per_turn=once,
                gate=gate,
                gate_filters=gf,
            )
        ]
    if t in ("Main", "Action"):
        return [
            d.Command(d.Timing.MAIN if t == "Main" else d.Timing.ACTION, compile_steps(body, g))
        ]
    raise CompileError(f"unknown marker {t!r}")


def merge_command_timings(abilities: list[d.Ability]) -> list[d.Ability]:
    """【Main】/【Action】 on one line is a single command effect usable at either time (13-2-3-2)."""
    cmds = [a for a in abilities if isinstance(a, d.Command)]
    if len(cmds) == 2 and cmds[0].steps == cmds[1].steps:
        rest = [a for a in abilities if not isinstance(a, d.Command)]
        return [d.Command(d.Timing.MAIN_OR_ACTION, cmds[0].steps), *rest]
    return abilities
