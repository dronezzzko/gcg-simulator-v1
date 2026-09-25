"""Global, deterministic registry of compiled card scripts, abilities, and programs.

Built once per process from the card database. Ability ids and program ids depend only on
the data snapshot and code, so they are identical in every worker process.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from functools import cache

from gcg_sim.cards.db import CardDB, get_card_db
from gcg_sim.cards.model import CardDef, CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.program import Program, flatten
from gcg_sim.engine.types import Duration


class UnimplementedCardError(Exception):
    """Raised when a game needs a card whose effect has no compiled or bound implementation."""


@dataclass(frozen=True, slots=True)
class AbilityEntry:
    aid: int
    card_number: str
    def_id: int
    index: int
    unit_text: bool
    ability: d.Ability
    program_id: int  # -1 when the ability has no steps (keywords, constants)
    cost_program_id: int = -1


@dataclass(slots=True)
class CardEntry:
    def_id: int
    card_number: str
    script: d.CardScript | None
    error: str | None
    own: tuple[int, ...] = ()
    unit: tuple[int, ...] = ()
    command_aid: int = -1
    burst_aid: int = -1
    has_burst: bool = False
    timings: frozenset[str] = frozenset()


# Engine-provided keyword programs (rule 13-1).
REPAIR_STEPS: tuple[d.Step, ...] = (d.Recover(d.This(), d.KwAmount(d.Kw.REPAIR)),)
BREACH_STEPS: tuple[d.Step, ...] = (d.DamageShieldArea(d.P.OPP, d.KwAmount(d.Kw.BREACH)),)
SUPPORT_STEPS: tuple[d.Step, ...] = (
    d.Rest(d.This()),
    d.Choose("t", d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (d.NotRef(d.This()),))),
    d.Apply(d.Var("t"), d.StatMod(ap=d.KwAmount(d.Kw.SUPPORT)), Duration.THIS_TURN),
    d.CustomStep("support_used"),
)


@dataclass
class Registry:
    db: CardDB
    programs: list[Program] = field(default_factory=list)
    abilities: list[AbilityEntry] = field(default_factory=list)
    cards: list[CardEntry] = field(default_factory=list)
    continuous: list[d.Continuous] = field(default_factory=list)
    filter_sets: list[tuple[d.Filter, ...]] = field(default_factory=list)
    _cont_index: dict[d.Continuous, int] = field(default_factory=dict)
    _filter_index: dict[tuple[d.Filter, ...], int] = field(default_factory=dict)
    _program_index: dict[tuple[d.Step, ...], int] = field(default_factory=dict)
    aliases: dict[int, tuple[str, ...]] = field(default_factory=dict)
    repair_program: int = -1
    breach_program: int = -1
    support_program: int = -1
    support_cost_program: int = -1

    def program(self, steps: tuple[d.Step, ...], label: str) -> int:
        pid = self._program_index.get(steps)
        if pid is not None:
            return pid
        self.programs.append(flatten(steps, label))
        pid = len(self.programs) - 1
        self._program_index[steps] = pid
        return pid

    def cont_key(self, c: d.Continuous) -> int:
        k = self._cont_index.get(c)
        if k is None:
            self.continuous.append(c)
            k = len(self.continuous) - 1
            self._cont_index[c] = k
        return k

    def filters_key(self, f: tuple[d.Filter, ...]) -> int:
        k = self._filter_index.get(f)
        if k is None:
            self.filter_sets.append(f)
            k = len(self.filter_sets) - 1
            self._filter_index[f] = k
        return k

    def card(self, def_id: int) -> CardEntry:
        return self.cards[def_id]

    def require(self, def_id: int) -> CardEntry:
        e = self.cards[def_id]
        if e.script is None:
            raise UnimplementedCardError(f"{e.card_number}: {e.error}")
        return e


BURST_PROMPT = "activate Burst?"


def cost_steps(costs: tuple[d.Cost, ...]) -> tuple[d.Step, ...]:
    """Activation costs (rule 10-1-7-2) as steps executed before the effect. ① payments are
    made by the engine when the ability is activated."""
    out: list[d.Step] = []
    for i, c in enumerate(costs):
        var = f"__cost{i}"
        if isinstance(c, d.RestSelf):
            out.append(d.Rest(d.This()))
        elif isinstance(c, d.RestCards):
            sel = d.Sel(c.sel.side, c.sel.loc, (*c.sel.filters, d.IsRested(False)), c.sel.top_n)
            out += [d.Choose(var, sel, c.count, targeting=False), d.Rest(d.Var(var))]
        elif isinstance(c, d.DestroyCards):
            out += [d.Choose(var, c.sel, c.count, targeting=False), d.Destroy(d.Var(var))]
        elif isinstance(c, d.ExileCards):
            out += [d.Choose(var, c.sel, c.count, targeting=False), d.Exile(d.Var(var))]
        elif isinstance(c, d.DiscardCards):
            out.append(d.Discard(count=c.count, filters=c.sel.filters, var=var))
        elif isinstance(c, d.ReturnSelf):
            out.append(d.ReturnToHand(d.ThisCard()))
        elif isinstance(c, d.DestroySelf):
            out.append(d.Destroy(d.This()))
        elif isinstance(c, d.TrashSelf):
            out.append(d.ToTrash(d.ThisCard()))
    return tuple(out)


def _ability_steps(a: d.Ability) -> tuple[d.Step, ...] | None:
    if isinstance(a, d.Activated):
        return cost_steps(a.costs) + a.steps
    if isinstance(a, d.Burst):
        return (d.May(a.steps, prompt=BURST_PROMPT),)
    if isinstance(a, (d.Triggered, d.Command, d.Replacement)):
        return a.steps
    return None


def _timings(script: d.CardScript) -> frozenset[str]:
    out: set[str] = set()
    for a in (*script.abilities, *script.unit_abilities):
        if isinstance(a, d.Triggered):
            out.add(a.trigger.event.value)
        elif isinstance(a, d.Activated):
            out.add("activate_" + a.timing.value)
        elif isinstance(a, d.Command):
            out.add(a.timing.value)
        elif isinstance(a, d.Burst):
            out.add("burst")
    return frozenset(out)


def _register_card(
    reg: Registry, cdef: CardDef, script: d.CardScript | None, error: str | None
) -> None:
    entry = CardEntry(def_id=cdef.def_id, card_number=cdef.card_number, script=script, error=error)
    reg.cards.append(entry)
    if script is None:
        return
    own: list[int] = []
    unit: list[int] = []
    for unit_text, abilities in ((False, script.abilities), (True, script.unit_abilities)):
        for idx, a in enumerate(abilities):
            steps = _ability_steps(a)
            pid = -1
            if steps is not None:
                pid = reg.program(steps, f"{cdef.card_number}#{'u' if unit_text else 'a'}{idx}")
            cost_pid = -1
            if isinstance(a, d.PlayModifier) and a.costs:
                cost_pid = reg.program(cost_steps(a.costs), f"{cdef.card_number}#alt{idx}")
            if isinstance(a, d.NameAlias):
                reg.aliases[cdef.def_id] = reg.aliases.get(cdef.def_id, ()) + a.names
            aid = len(reg.abilities)
            reg.abilities.append(
                AbilityEntry(
                    aid=aid,
                    card_number=cdef.card_number,
                    def_id=cdef.def_id,
                    index=idx,
                    unit_text=unit_text,
                    ability=a,
                    program_id=pid,
                    cost_program_id=cost_pid,
                )
            )
            (unit if unit_text else own).append(aid)
            if isinstance(a, d.Command) and not unit_text:
                entry.command_aid = aid
            if isinstance(a, d.Burst) and not unit_text:
                entry.burst_aid = aid
                entry.has_burst = True
            for dt in _delayed_steps(a):
                reg.program(dt, f"{cdef.card_number}#delayed")
    entry.own = tuple(own)
    entry.unit = tuple(unit)
    entry.timings = _timings(script)


def _delayed_steps(a: d.Ability) -> list[tuple[d.Step, ...]]:
    out: list[tuple[d.Step, ...]] = []
    steps = _ability_steps(a)
    if steps is None:
        return out

    def walk(ss: tuple[d.Step, ...]) -> None:
        for s in ss:
            if isinstance(s, d.DelayedTrigger):
                out.append(s.steps)
                walk(s.steps)
            elif isinstance(s, d.If):
                walk(s.then)
                walk(s.otherwise)
            elif isinstance(s, (d.May, d.IfYouDo, d.ForEach, d.Repeat)):
                walk(s.steps)
            elif isinstance(s, d.ChooseMode):
                for _, body in s.options:
                    walk(body)

    walk(steps)
    return out


def build_registry(db: CardDB) -> Registry:
    from gcg_sim.effects.scripts import script_for

    reg = Registry(db=db)
    reg.repair_program = reg.program(REPAIR_STEPS, "keyword:Repair")
    reg.breach_program = reg.program(BREACH_STEPS, "keyword:Breach")
    reg.support_program = reg.program(SUPPORT_STEPS, "keyword:Support")
    for cdef in db:
        try:
            script = script_for(cdef)
            error = None
        except UnimplementedCardError as exc:
            script, error = None, str(exc)
        _register_card(reg, cdef, script, error)
    return reg


@cache
def get_registry() -> Registry:
    return build_registry(get_card_db())


def is_main_deck_type(ctype: CardType) -> bool:
    return ctype in (CardType.UNIT, CardType.PILOT, CardType.COMMAND, CardType.BASE)
