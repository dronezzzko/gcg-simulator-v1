"""Derived characteristics and DSL expression evaluation.

``derived(st)`` computes, for the current state version, every card's effective AP/HP,
keywords, traits, rule modifications, cost modifiers, link status, and active abilities by
applying constant effects (rule 10-1-5) and lasting effects in a small fixpoint. It is cached
on the state until the next mutation.
"""

from __future__ import annotations

from collections.abc import Callable, Iterable
from typing import Any

from gcg_sim.cards.model import CardDef, CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import AbilityEntry, Registry, get_registry
from gcg_sim.engine.state import CardInstance, GameState
from gcg_sim.engine.types import NO_ARG, PLAYER_TARGET, Zone


def reg() -> Registry:
    return get_registry()


def cdef(st: GameState, uid: int) -> CardDef:
    return reg().db.by_id(st.cards[uid].def_id)


class Ctx:
    """Evaluation context for DSL expressions."""

    __slots__ = ("card_uid", "controller", "event", "host", "vars")

    def __init__(
        self,
        controller: int,
        host: int = NO_ARG,
        card_uid: int = NO_ARG,
        vars: dict[str, tuple[int, ...]] | None = None,
        event: tuple[tuple[str, int], ...] = (),
    ) -> None:
        self.controller = controller
        self.host = host
        self.card_uid = card_uid if card_uid != NO_ARG else host
        self.vars = vars if vars is not None else {}
        self.event = event

    def ev(self, key: str, default: int = NO_ARG) -> int:
        for k, v in self.event:
            if k == key:
                return v
        return default


class RuleInst:
    __slots__ = ("aux", "controller", "key", "rule", "source")

    def __init__(
        self, rule: d.RuleMod, controller: int, source: int, key: tuple[int, ...], aux: int = NO_ARG
    ) -> None:
        self.rule = rule
        self.controller = controller
        self.source = source
        self.key = key
        self.aux = aux


class Derived:
    __slots__ = (
        "abilities",
        "ap",
        "by_event",
        "cost_mod",
        "hp",
        "kw",
        "level_mod",
        "linked",
        "rules",
        "traits",
    )

    def __init__(self) -> None:
        self.ap: dict[int, int] = {}
        self.hp: dict[int, int] = {}
        self.kw: dict[int, dict[d.Kw, int]] = {}
        self.traits: dict[int, tuple[str, ...]] = {}
        self.rules: dict[int, list[RuleInst]] = {}
        self.cost_mod: dict[int, int] = {}
        self.level_mod: dict[int, int] = {}
        self.linked: set[int] = set()
        self.abilities: dict[int, list[AbilityEntry]] = {}
        self.by_event: dict[d.Ev, list[tuple[int, AbilityEntry]]] | None = None

    def same_values(self, other: Derived) -> bool:
        return (
            self.ap == other.ap
            and self.hp == other.hp
            and self.kw == other.kw
            and self.traits == other.traits
            and self.cost_mod == other.cost_mod
            and self.level_mod == other.level_mod
            and {k: [r.rule for r in v] for k, v in self.rules.items()}
            == {k: [r.rule for r in v] for k, v in other.rules.items()}
            and {k: [a.aid for a in v] for k, v in self.abilities.items()}
            == {k: [a.aid for a in v] for k, v in other.abilities.items()}
        )


# ---------------------------------------------------------------------------------------------
# custom hooks registered by binding modules

CustomFilterFn = Callable[[GameState, "Derived", Ctx, int, dict[str, object]], bool]
CustomValueFn = Callable[[GameState, "Derived", Ctx, dict[str, object]], int]
CustomCondFn = Callable[[GameState, "Derived", Ctx, dict[str, object]], bool]

CUSTOM_FILTERS: dict[str, CustomFilterFn] = {}
CUSTOM_VALUES: dict[str, CustomValueFn] = {}
CUSTOM_CONDS: dict[str, CustomCondFn] = {}


# ---------------------------------------------------------------------------------------------
# link / pairing helpers


def link_satisfied(unit_def: CardDef, pilot_def: CardDef) -> bool:
    """Rules 3-2-6-2 and 3-2-6-4: a paired Pilot whose name contains a bracketed name
    portion, or who has a listed trait, satisfies the link condition."""
    link = unit_def.link
    if link is None:
        return False
    names = (
        pilot_def.names if pilot_def.card_type is CardType.PILOT else (pilot_def.pilot_name or "",)
    )
    names = (*names, *reg().aliases.get(pilot_def.def_id, ()))
    for part in link.name_parts:
        if any(part in n for n in names):
            return True
    return any(t in pilot_def.traits for t in link.traits)


def _gate_ok(
    st: GameState, dv: Derived, host: int, gate: d.Gate, gate_filters: tuple[d.Filter, ...]
) -> bool:
    if gate is d.Gate.NONE:
        return True
    card = st.cards[host]
    if card.zone is not Zone.BATTLE or card.pair < 0:
        return False
    if gate is d.Gate.LINKED and host not in dv.linked:
        return False
    if gate_filters:
        ctx = Ctx(card.owner, host)
        return matches(st, dv, ctx, card.pair, gate_filters)
    return True


def _ability_where(a: d.Ability) -> d.Where:
    if isinstance(a, (d.Constant, d.Triggered, d.Activated)):
        return a.where
    return d.Where.FIELD


def _ability_gate(a: d.Ability) -> tuple[d.Gate, tuple[d.Filter, ...]]:
    if isinstance(a, (d.Constant, d.Triggered, d.Activated, d.Keyword)):
        return a.gate, a.gate_filters
    if isinstance(a, d.Replacement):
        return a.gate, ()
    return d.Gate.NONE, ()


_WHERE_FOR_ZONE = {
    Zone.BATTLE: (d.Where.FIELD, d.Where.ANY),
    Zone.BASE: (d.Where.FIELD, d.Where.ANY),
    Zone.PAIRED: (d.Where.FIELD, d.Where.ANY),
    Zone.HAND: (d.Where.HAND, d.Where.ANY),
    Zone.TRASH: (d.Where.TRASH, d.Where.ANY),
}


def _host_abilities(
    st: GameState, dv: Derived, uid: int, granted: dict[int, list[int]]
) -> list[AbilityEntry]:
    card = st.cards[uid]
    R = reg()
    entry = R.cards[card.def_id]
    z = card.zone
    if z is Zone.BATTLE or z is Zone.BASE:
        base: tuple[AbilityEntry, ...] = entry.field_entries
    elif z is Zone.HAND:
        base = entry.hand_entries
    elif z is Zone.TRASH:
        base = entry.trash_entries
    elif z is Zone.PAIRED:
        base = entry.paired_entries
    else:
        return []
    out = list(base)
    gated = entry.gated
    if z is Zone.BATTLE and card.pair >= 0:
        pentry = R.cards[st.cards[card.pair].def_id]
        if pentry.unit_entries:
            out.extend(pentry.unit_entries)
            gated = gated or pentry.gated
    g = granted.get(uid)
    if g:
        out.extend(R.abilities[aid] for aid in g)
        gated = True
    if not gated:
        return out
    kept = []
    for a in out:
        gate, gf = _ability_gate(a.ability)
        if gate is not d.Gate.NONE and not _gate_ok(st, dv, uid, gate, gf):
            continue
        kept.append(a)
    return kept


# ---------------------------------------------------------------------------------------------
# derived computation


def derived(st: GameState) -> Derived:
    dv = st._derived
    if dv is None:
        dv = _compute(st)
        st._derived = dv
    return dv  # type: ignore[no-any-return]


_HOST_ZONES = (Zone.BATTLE, Zone.BASE, Zone.PAIRED, Zone.HAND, Zone.TRASH)


def _base_view(st: GameState, granted: dict[int, list[int]], prev: Derived | None) -> Derived:
    R = reg()
    db = R.db
    cards = R.cards
    dv = Derived()
    for p in (0, 1):
        for uid in st.zones[p][Zone.BATTLE]:
            c = st.cards[uid]
            u = db.by_id(c.def_id)
            ap, hp = u.ap, u.hp
            if c.pair >= 0:
                pd = db.by_id(st.cards[c.pair].def_id)
                ap += pd.ap
                hp += pd.hp
                if link_satisfied(u, pd):
                    dv.linked.add(uid)
            dv.ap[uid] = ap
            dv.hp[uid] = hp
            dv.traits[uid] = u.traits
            dv.kw[uid] = {}
        for uid in st.zones[p][Zone.BASE]:
            b = db.by_id(st.cards[uid].def_id)
            dv.ap[uid] = b.ap
            dv.hp[uid] = b.hp
            dv.traits[uid] = b.traits
            dv.kw[uid] = {}
    gate_view = prev if prev is not None else dv
    for p in (0, 1):
        zp = st.zones[p]
        for z in (Zone.BATTLE, Zone.BASE):
            for uid in zp[z]:
                abil = _host_abilities(st, gate_view, uid, granted)
                if abil:
                    dv.abilities[uid] = abil
                    kws = dv.kw[uid]
                    for a in abil:
                        ab = a.ability
                        if isinstance(ab, d.Keyword):
                            if ab.keyword in d.STACKING_KEYWORDS:
                                kws[ab.keyword] = kws.get(ab.keyword, 0) + ab.amount
                            else:
                                kws[ab.keyword] = 1
        for z, attr in (
            (Zone.PAIRED, "paired_entries"),
            (Zone.HAND, "hand_entries"),
            (Zone.TRASH, "trash_entries"),
        ):
            for uid in zp[z]:
                if not getattr(cards[st.cards[uid].def_id], attr) and uid not in granted:
                    continue
                abil = _host_abilities(st, gate_view, uid, granted)
                if abil:
                    dv.abilities[uid] = abil
    return dv


def trigger_index(st: GameState, dv: Derived) -> dict[d.Ev, list[tuple[int, AbilityEntry]]]:
    """Triggered abilities by event, in host order: active player first, then location order
    (battle, base, paired, hand, trash), then location order within each list."""
    idx = dv.by_event
    if idx is not None:
        return idx
    idx = {}
    for p in (st.active, 1 - st.active):
        zp = st.zones[p]
        for z in (Zone.BATTLE, Zone.BASE, Zone.PAIRED, Zone.HAND, Zone.TRASH):
            for uid in zp[z]:
                abil = dv.abilities.get(uid)
                if not abil:
                    continue
                for a in abil:
                    ev = a.trigger_event
                    if ev is not None:
                        idx.setdefault(ev, []).append((uid, a))
    dv.by_event = idx
    return idx


def _copy_base(base: Derived) -> Derived:
    dv = Derived()
    dv.ap = dict(base.ap)
    dv.hp = dict(base.hp)
    dv.kw = {k: dict(v) for k, v in base.kw.items()}
    dv.traits = dict(base.traits)
    dv.linked = base.linked
    dv.abilities = base.abilities
    return dv


def _apply_cont(
    st: GameState,
    dv: Derived,
    view: Derived,
    ctx: Ctx,
    eff: d.Continuous,
    targets: Iterable[int],
    rule_key: tuple[int, ...],
    source: int,
    granted_out: dict[int, list[int]],
    aux: int = NO_ARG,
) -> None:
    if isinstance(eff, d.StatMod):
        ap = value(st, view, ctx, eff.ap) if eff.ap != 0 else 0
        hp = value(st, view, ctx, eff.hp) if eff.hp != 0 else 0
        for t in targets:
            if ap < 0 and st.cards[t].owner != ctx.controller and _ap_protected(view, t):
                if t in dv.hp:
                    dv.hp[t] += hp
                continue
            if t in dv.ap:
                dv.ap[t] += ap
                dv.hp[t] += hp
    elif isinstance(eff, d.KeywordGrant):
        amt = value(st, view, ctx, eff.amount) if eff.amount != 0 else 0
        for t in targets:
            kws = dv.kw.get(t)
            if kws is None:
                continue
            if eff.keyword in d.STACKING_KEYWORDS:
                kws[eff.keyword] = kws.get(eff.keyword, 0) + amt
            else:
                kws[eff.keyword] = 1
    elif isinstance(eff, d.TraitGrant):
        for t in targets:
            if t in dv.traits:
                cur = dv.traits[t]
                dv.traits[t] = cur + tuple(x for x in eff.traits if x not in cur)
    elif isinstance(eff, d.RuleGrant):
        for t in targets:
            dv.rules.setdefault(t, []).append(
                RuleInst(eff.rule, ctx.controller, source, rule_key, aux)
            )
    elif isinstance(eff, d.CostMod):
        cm = value(st, view, ctx, eff.cost) if eff.cost != 0 else 0
        lm = value(st, view, ctx, eff.level) if eff.level != 0 else 0
        for t in targets:
            if cm:
                dv.cost_mod[t] = dv.cost_mod.get(t, 0) + cm
            if lm:
                dv.level_mod[t] = dv.level_mod.get(t, 0) + lm
    elif isinstance(eff, d.AbilityGrant):
        entry = reg().db.get(eff.card_number)
        if entry is None:
            return
        ce = reg().cards[entry.def_id]
        aids = [*ce.own, *ce.unit]
        if eff.ability_index < len(aids):
            for t in targets:
                granted_out.setdefault(t, []).append(aids[eff.ability_index])


def _compute(st: GameState) -> Derived:
    R = reg()
    granted: dict[int, list[int]] = {}
    prev: Derived | None = None
    base = _base_view(st, granted, None)
    has_constants = any(
        isinstance(a.ability, d.Constant) for abil in base.abilities.values() for a in abil
    )
    if not st.lasting and not has_constants:
        _finalize(base)
        return base
    dv = base
    for _ in range(6):
        view = prev if prev is not None else base
        new_granted: dict[int, list[int]] = {}
        cur = _copy_base(base) if not granted else _base_view(st, granted, prev)
        for host, abil in list(view.abilities.items()):
            hc = st.cards[host]
            for a in abil:
                ab = a.ability
                if not isinstance(ab, d.Constant):
                    continue
                ctx = Ctx(hc.owner, host)
                if ab.cond is not d.TRUE and not cond(st, view, ctx, ab.cond):
                    continue
                targets = resolve(st, view, ctx, ab.scope)
                for eff in ab.effects:
                    _apply_cont(st, cur, view, ctx, eff, targets, (a.aid, host), host, new_granted)
        for le in st.lasting:
            if le.player != NO_ARG:
                continue
            live = [uid for uid, seq in le.targets if st.cards[uid].zone_seq == seq]
            if not live:
                continue
            ctx = Ctx(le.controller, le.source_uid)
            _apply_cont(
                st,
                cur,
                view,
                ctx,
                R.continuous[le.effect_key],
                live,
                (-1, le.effect_key),
                le.source_uid,
                new_granted,
                le.aux,
            )
        if prev is not None and cur.same_values(prev) and new_granted == granted:
            dv = cur
            break
        prev = cur
        granted = new_granted
        dv = cur
    _finalize(dv)
    return dv


def _ap_protected(view: Derived, uid: int) -> bool:
    """ "This Unit's AP can't be reduced by enemy effects" (rule 10-1-5-6: "can't" wins)."""
    return any(r.rule.kind is d.RuleKind.AP_CANT_BE_REDUCED for r in view.rules.get(uid, ()))


def names_of(cd: CardDef) -> tuple[str, ...]:
    """All names including aliases granted by the card's own text (rule 2-2-4)."""
    extra = reg().aliases.get(cd.def_id)
    return (*cd.names, *extra) if extra else cd.names


def _finalize(dv: Derived) -> None:
    for k, v in dv.ap.items():
        if v < 0:
            dv.ap[k] = 0
    for k, v in dv.hp.items():
        if v < 0:
            dv.hp[k] = 0


# ---------------------------------------------------------------------------------------------
# characteristic queries


def ap_of(st: GameState, dv: Derived, uid: int) -> int:
    v = dv.ap.get(uid)
    if v is not None:
        return v
    return max(0, cdef(st, uid).ap)


def hp_of(st: GameState, dv: Derived, uid: int) -> int:
    v = dv.hp.get(uid)
    if v is not None:
        return v
    c = st.cards[uid]
    if c.zone is Zone.SHIELD:
        return 1
    return max(0, cdef(st, uid).hp)


def level_of(st: GameState, uid: int) -> int:
    cd = cdef(st, uid)
    return 0 if cd.is_token else cd.level


def cost_of(st: GameState, uid: int) -> int:
    cd = cdef(st, uid)
    return 0 if cd.is_token else cd.cost


def play_cost(st: GameState, dv: Derived, uid: int) -> int:
    return max(0, cost_of(st, uid) + dv.cost_mod.get(uid, 0) + player_cost_mod(st, uid))


def play_level(st: GameState, dv: Derived, uid: int) -> int:
    return max(0, level_of(st, uid) + dv.level_mod.get(uid, 0))


def player_cost_mod(st: GameState, uid: int) -> int:
    """Player-level lasting cost modifications (``ApplyPlayer`` with a :class:`CostMod`)."""
    total = 0
    if not st.lasting:
        return 0
    R = reg()
    card = st.cards[uid]
    for le in st.lasting:
        if le.player != card.owner:
            continue
        eff = R.continuous[le.effect_key]
        if not isinstance(eff, d.CostMod):
            continue
        if le.filters_key != NO_ARG:
            dv = derived(st)
            if not matches(
                st, dv, Ctx(le.controller, le.source_uid), uid, R.filter_sets[le.filters_key]
            ):
                continue
        total += value(st, derived(st), Ctx(le.controller, le.source_uid), eff.cost)
    return total


def keywords(dv: Derived, uid: int) -> dict[d.Kw, int]:
    return dv.kw.get(uid, {})


def has_kw(dv: Derived, uid: int, kw: d.Kw) -> bool:
    return kw in dv.kw.get(uid, {})


def kw_amount(dv: Derived, uid: int, kw: d.Kw) -> int:
    return dv.kw.get(uid, {}).get(kw, 0)


def traits_of(st: GameState, dv: Derived, uid: int) -> tuple[str, ...]:
    t = dv.traits.get(uid)
    return t if t is not None else cdef(st, uid).traits


def rules_of(dv: Derived, uid: int, kind: d.RuleKind) -> list[RuleInst]:
    return [r for r in dv.rules.get(uid, ()) if r.rule.kind is kind]


def is_linked(dv: Derived, uid: int) -> bool:
    return uid in dv.linked


# ---------------------------------------------------------------------------------------------
# expression evaluation


def _players(ctrl: int, side: d.Side) -> tuple[int, ...]:
    if side is d.Side.FRIENDLY:
        return (ctrl,)
    if side is d.Side.ENEMY:
        return (1 - ctrl,)
    return (ctrl, 1 - ctrl)


def player_of(st: GameState, ctx: Ctx, p: d.P) -> int:
    if p is d.P.YOU:
        return ctx.controller
    if p is d.P.OPP:
        return 1 - ctx.controller
    if p is d.P.ACTIVE:
        return st.active
    return 1 - st.active


def _loc_cards(st: GameState, dv: Derived, ctx: Ctx, player: int, sel: d.Sel) -> list[int]:
    z = st.zones[player]
    loc = sel.loc
    if loc is d.Loc.BATTLE:
        return z[Zone.BATTLE]
    if loc is d.Loc.PAIRED:
        return z[Zone.PAIRED]
    if loc is d.Loc.BASE:
        return z[Zone.BASE]
    if loc is d.Loc.SHIELDS:
        return z[Zone.SHIELD]
    if loc is d.Loc.SHIELD_AREA:
        return z[Zone.BASE] + z[Zone.SHIELD]
    if loc is d.Loc.RESOURCE_AREA:
        return z[Zone.RESOURCE_AREA]
    if loc is d.Loc.HAND:
        return z[Zone.HAND]
    if loc is d.Loc.TRASH:
        return z[Zone.TRASH]
    if loc is d.Loc.DECK:
        return z[Zone.DECK]
    if loc is d.Loc.DECK_TOP:
        n = value(st, dv, ctx, sel.top_n)
        return z[Zone.DECK][: max(0, n)]
    if loc is d.Loc.RESOURCE_DECK:
        return z[Zone.RESOURCE_DECK]
    if loc is d.Loc.REMOVAL:
        return z[Zone.REMOVAL]
    return z[Zone.BATTLE] + z[Zone.BASE]


def select(st: GameState, dv: Derived, ctx: Ctx, sel: d.Sel) -> list[int]:
    out: list[int] = []
    for p in _players(ctx.controller, sel.side):
        for uid in _loc_cards(st, dv, ctx, p, sel):
            if not sel.filters or matches(st, dv, ctx, uid, sel.filters):
                out.append(uid)
    return out


def _kind_ok(st: GameState, dv: Derived, card: CardInstance, cd: CardDef, kind: d.CardKind) -> bool:
    t = cd.card_type
    if kind is d.CardKind.UNIT:
        return t.is_unit
    if kind is d.CardKind.PILOT:
        return t is CardType.PILOT or (t is CardType.COMMAND and card.zone is Zone.PAIRED)
    if kind is d.CardKind.COMMAND:
        return t is CardType.COMMAND and card.zone is not Zone.PAIRED
    if kind is d.CardKind.BASE:
        return t.is_base
    if kind is d.CardKind.RESOURCE:
        return t.is_resource
    if kind is d.CardKind.TOKEN:
        return t.is_token
    if kind is d.CardKind.UNIT_TOKEN:
        return t is CardType.UNIT_TOKEN
    if kind is d.CardKind.EX_RESOURCE:
        return t is CardType.EX_RESOURCE
    if kind is d.CardKind.EX_BASE:
        return t is CardType.EX_BASE
    return t.is_unit and card.uid in dv.linked


def stat(st: GameState, dv: Derived, uid: int, s: d.Stat) -> int:
    if s is d.Stat.AP:
        return ap_of(st, dv, uid)
    if s is d.Stat.HP:
        return hp_of(st, dv, uid)
    if s is d.Stat.LV:
        return level_of(st, uid)
    if s is d.Stat.COST:
        return cost_of(st, uid)
    if s is d.Stat.DAMAGE:
        return st.cards[uid].damage
    return max(0, hp_of(st, dv, uid) - st.cards[uid].damage)


def _cmp(a: int, op: d.Op, b: int) -> bool:
    if op is d.Op.LE:
        return a <= b
    if op is d.Op.GE:
        return a >= b
    if op is d.Op.EQ:
        return a == b
    if op is d.Op.LT:
        return a < b
    if op is d.Op.GT:
        return a > b
    return a != b


def matches(st: GameState, dv: Derived, ctx: Ctx, uid: int, filters: tuple[d.Filter, ...]) -> bool:
    card = st.cards[uid]
    cd = reg().db.by_id(card.def_id)
    return all(_match1(st, dv, ctx, card, cd, f) for f in filters)


def _match1(
    st: GameState, dv: Derived, ctx: Ctx, card: CardInstance, cd: CardDef, f: d.Filter
) -> bool:
    uid = card.uid
    if isinstance(f, d.IsKind):
        return any(_kind_ok(st, dv, card, cd, k) for k in f.kinds)
    if isinstance(f, d.HasTrait):
        tr = dv.traits.get(uid, cd.traits) if card.zone in (Zone.BATTLE, Zone.BASE) else cd.traits
        return any(t in tr for t in f.traits)
    if isinstance(f, d.NameContains):
        return any(part in n for part in f.parts for n in names_of(cd))
    if isinstance(f, d.NameIs):
        return any(n in f.names for n in names_of(cd))
    if isinstance(f, d.HasColor):
        return cd.color is not None and not cd.is_token and cd.color.value in f.colors
    if isinstance(f, d.StatCmp):
        return _cmp(stat(st, dv, uid, f.stat), f.op, value(st, dv, ctx, f.value))
    if isinstance(f, d.IsRested):
        return card.rested == f.rested
    if isinstance(f, d.HasKeyword):
        return f.keyword in dv.kw.get(uid, {})
    if isinstance(f, d.IsLinked):
        return (uid in dv.linked) == f.linked
    if isinstance(f, d.IsPaired):
        return (card.zone is Zone.BATTLE and card.pair >= 0) == f.paired
    if isinstance(f, d.PairedWith):
        return (
            card.zone is Zone.BATTLE
            and card.pair >= 0
            and matches(st, dv, ctx, card.pair, f.filters)
        )
    if isinstance(f, d.PairedTo):
        return (
            card.zone is Zone.PAIRED
            and card.pair >= 0
            and matches(st, dv, ctx, card.pair, f.filters)
        )
    if isinstance(f, d.IsToken):
        return cd.is_token == f.token
    if isinstance(f, d.IsDamaged):
        return (card.damage > 0) == f.damaged
    if isinstance(f, d.NotRef):
        return uid not in resolve(st, dv, ctx, f.ref)
    if isinstance(f, d.IsRef):
        return uid in resolve(st, dv, ctx, f.ref)
    if isinstance(f, d.IsAttacking):
        b = st.battle
        return (b is not None and not b.ended and b.attacker == uid) == f.attacking
    if isinstance(f, d.IsBattling):
        b = st.battle
        return (b is not None and not b.ended and uid in (b.attacker, b.target)) == f.battling
    if isinstance(f, d.HasTiming):
        return f.timing in reg().cards[card.def_id].timings
    if isinstance(f, d.HasBurst):
        return reg().cards[card.def_id].has_burst == f.burst
    if isinstance(f, d.DeployedThisTurn):
        return (card.zone in (Zone.BATTLE, Zone.BASE) and card.entered_turn == st.turn) == f.value
    if isinstance(f, d.HasZone):
        return any(z in cd.zones for z in f.zones)
    if isinstance(f, d.LevelCmpRef):
        others = resolve(st, dv, ctx, f.ref)
        if not others:
            return False
        return _cmp(stat(st, dv, uid, f.stat), f.op, stat(st, dv, others[0], f.ref_stat))
    if isinstance(f, d.AnyOf):
        return any(_match1(st, dv, ctx, card, cd, g) for g in f.filters)
    if isinstance(f, d.AllOf):
        return all(_match1(st, dv, ctx, card, cd, g) for g in f.filters)
    if isinstance(f, d.Not):
        return not _match1(st, dv, ctx, card, cd, f.filter)
    fn = CUSTOM_FILTERS[f.name]
    return fn(st, dv, ctx, uid, dict(f.params))


def resolve(st: GameState, dv: Derived, ctx: Ctx, ref: d.Ref) -> tuple[int, ...]:
    if isinstance(ref, d.This):
        return (ctx.host,) if ctx.host != NO_ARG else ()
    if isinstance(ref, d.ThisCard):
        return (ctx.card_uid,) if ctx.card_uid != NO_ARG else ()
    if isinstance(ref, d.Var):
        return ctx.vars.get(ref.name, ())
    if isinstance(ref, d.EventCard):
        v = ctx.ev(ref.key)
        return (v,) if v >= 0 else ()
    if isinstance(ref, d.PairedPilotOf):
        out = []
        for u in resolve(st, dv, ctx, ref.ref):
            c = st.cards[u]
            if c.zone is Zone.BATTLE and c.pair >= 0:
                out.append(c.pair)
        return tuple(out)
    if isinstance(ref, d.PairedUnitOf):
        out = []
        for u in resolve(st, dv, ctx, ref.ref):
            c = st.cards[u]
            if c.zone is Zone.PAIRED and c.pair >= 0:
                out.append(c.pair)
        return tuple(out)
    if isinstance(ref, d.BattlingWith):
        b = st.battle
        if b is None or b.ended:
            return ()
        out = []
        for u in resolve(st, dv, ctx, ref.ref):
            if u == b.attacker and b.target >= 0:
                out.append(b.target)
            elif u == b.target:
                out.append(b.attacker)
        return tuple(out)
    if isinstance(ref, d.All):
        return tuple(select(st, dv, ctx, ref.sel))
    seen: list[int] = []
    for r in ref.refs:
        for u in resolve(st, dv, ctx, r):
            if u not in seen:
                seen.append(u)
    return tuple(seen)


def value(st: GameState, dv: Derived, ctx: Ctx, v: d.Value) -> int:
    if isinstance(v, int):
        return v
    if isinstance(v, d.Count):
        return len(select(st, dv, ctx, v.sel))
    if isinstance(v, d.StatOf):
        cards = resolve(st, dv, ctx, v.ref)
        return stat(st, dv, cards[0], v.stat) if cards else 0
    if isinstance(v, d.PlayerLevel):
        return len(st.zones[player_of(st, ctx, v.player)][Zone.RESOURCE_AREA])
    if isinstance(v, d.HandSize):
        return len(st.zones[player_of(st, ctx, v.player)][Zone.HAND])
    if isinstance(v, d.ShieldCount):
        return len(st.zones[player_of(st, ctx, v.player)][Zone.SHIELD])
    if isinstance(v, d.DeckSize):
        return len(st.zones[player_of(st, ctx, v.player)][Zone.DECK])
    if isinstance(v, d.VarSize):
        return len(ctx.vars.get(v.var, ()))
    if isinstance(v, d.EventAmount):
        return max(0, ctx.ev(v.key, 0))
    if isinstance(v, d.Sum):
        return sum(value(st, dv, ctx, t) for t in v.terms)
    if isinstance(v, d.Times):
        return value(st, dv, ctx, v.value) * v.factor
    if isinstance(v, d.MinOf):
        return min(value(st, dv, ctx, t) for t in v.terms)
    if isinstance(v, d.KwAmount):
        cards = resolve(st, dv, ctx, v.ref)
        return kw_amount(dv, cards[0], v.keyword) if cards else 0
    return CUSTOM_VALUES[v.name](st, dv, ctx, dict(v.params))


def cond(st: GameState, dv: Derived, ctx: Ctx, c: d.Cond, did: bool = True) -> bool:
    if isinstance(c, d.And):
        return all(cond(st, dv, ctx, x, did) for x in c.conds)
    if isinstance(c, d.Or):
        return any(cond(st, dv, ctx, x, did) for x in c.conds)
    if isinstance(c, d.NotC):
        return not cond(st, dv, ctx, c.cond, did)
    if isinstance(c, d.Cmp):
        return _cmp(value(st, dv, ctx, c.left), c.op, value(st, dv, ctx, c.right))
    if isinstance(c, d.Exists):
        return len(select(st, dv, ctx, c.sel)) >= c.at_least
    if isinstance(c, d.IsTurn):
        return st.active == player_of(st, ctx, c.player)
    if isinstance(c, d.RefMatches):
        cards = resolve(st, dv, ctx, c.ref)
        return bool(cards) and all(matches(st, dv, ctx, u, c.filters) for u in cards)
    if isinstance(c, d.RefEmpty):
        return not resolve(st, dv, ctx, c.ref)
    if isinstance(c, d.AttackTargetIs):
        b = st.battle
        if b is None or b.ended:
            return False
        if c.kind == "player":
            return b.target == PLAYER_TARGET or st.cards[b.target].zone is Zone.BASE
        return b.target >= 0 and st.cards[b.target].zone is Zone.BATTLE
    if isinstance(c, d.EventFrom):
        return ctx.ev("from_loc") == LOC_CODES[c.loc]
    if isinstance(c, d.EventFlag):
        return bool(ctx.ev(c.key, 0)) == c.value
    if isinstance(c, d.HappenedThisTurn):
        who = player_of(st, ctx, c.player)
        by = player_of(st, ctx, c.by) if c.by is not None else NO_ARG
        n = 0
        for h in st.history:
            if h.kind != c.kind or h.player != who:
                continue
            if by not in (NO_ARG, h.by):
                continue
            if c.filters and (h.uid < 0 or not matches(st, dv, ctx, h.uid, c.filters)):
                continue
            n += 1
        return n >= c.at_least
    if isinstance(c, d.DidLast):
        return did
    return CUSTOM_CONDS[c.name](st, dv, ctx, dict(c.params))


LOC_CODES: dict[d.Loc, int] = {loc: i for i, loc in enumerate(d.Loc)}

ZONE_TO_LOC: dict[Zone, d.Loc] = {
    Zone.BATTLE: d.Loc.BATTLE,
    Zone.PAIRED: d.Loc.PAIRED,
    Zone.BASE: d.Loc.BASE,
    Zone.SHIELD: d.Loc.SHIELDS,
    Zone.RESOURCE_AREA: d.Loc.RESOURCE_AREA,
    Zone.HAND: d.Loc.HAND,
    Zone.TRASH: d.Loc.TRASH,
    Zone.DECK: d.Loc.DECK,
    Zone.RESOURCE_DECK: d.Loc.RESOURCE_DECK,
    Zone.REMOVAL: d.Loc.REMOVAL,
}


def zone_loc_code(z: Zone) -> int:
    loc = ZONE_TO_LOC.get(z)
    return LOC_CODES[loc] if loc is not None else NO_ARG


def params_dict(params: tuple[tuple[str, object], ...]) -> dict[str, Any]:
    return dict(params)
