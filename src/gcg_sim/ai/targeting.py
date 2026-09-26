"""Whether a pending card choice helps or hurts the chosen cards.

The resolving effect's program is public card text, so the policy reads the instructions
that follow the waiting ``Choose`` and classifies what happens to the chosen variable.
"""

from __future__ import annotations

from gcg_sim.effects import dsl as d
from gcg_sim.effects import program as pr
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState

HARMS = -1
UNKNOWN = 0
HELPS = 1

_HARMFUL_STEPS = (d.Damage, d.Destroy, d.Rest, d.ReturnToHand, d.ToDeck, d.Exile, d.ToTrash)
_HELPFUL_STEPS = (d.Recover, d.SetActive, d.AddToHand, d.DeployCard, d.PlayCard)
_cache: dict[tuple[int, int], int] = {}


def choice_polarity(st: GameState) -> int:
    """HELPS / HARMS / UNKNOWN for the cards chosen at the waiting instruction."""
    if not st.frames or not st.frames[-1].waiting:
        return UNKNOWN
    f = st.frames[-1]
    key = (f.program_id, f.pc)
    cached = _cache.get(key)
    if cached is None:
        instrs = V.reg().programs[f.program_id].instrs
        cached = _polarity_at(instrs, f.pc)
        _cache[key] = cached
    return cached


_harm_cache: dict[int, bool] = {}


def command_harms_enemy_units(def_id: int) -> bool:
    """Whether the card's Command effect chooses enemy Units and harms them (removal)."""
    cached = _harm_cache.get(def_id)
    if cached is None:
        R = V.reg()
        aid = R.cards[def_id].command_aid
        cached = False
        if aid >= 0:
            instrs = R.programs[R.abilities[aid].program_id].instrs
            cached = any(
                isinstance(ins, d.Choose)
                and ins.sel.side is d.Side.ENEMY
                and _polarity_at(instrs, pc) == HARMS
                for pc, ins in enumerate(instrs)
            )
        _harm_cache[def_id] = cached
    return cached


def _polarity_at(instrs: tuple[pr.Instr, ...], pc: int) -> int:
    ins = instrs[pc]
    if not isinstance(ins, d.Choose):
        return UNKNOWN
    for later in instrs[pc + 1 :]:
        ref = getattr(later, "ref", None)
        if not isinstance(ref, (d.Var, d.Union)) or not _uses(ref, ins.var):
            continue
        if isinstance(later, _HARMFUL_STEPS):
            return HARMS
        if isinstance(later, _HELPFUL_STEPS):
            return HELPS
        if isinstance(later, d.Apply):
            return _effect_polarity(later.effect)
    return UNKNOWN


def _uses(ref: d.Ref, var: str) -> bool:
    if isinstance(ref, d.Var):
        return ref.name == var
    if isinstance(ref, d.Union):
        return any(_uses(r, var) for r in ref.refs)
    return False


def _effect_polarity(eff: d.Continuous) -> int:
    if isinstance(eff, d.StatMod):
        total = sum(v for v in (eff.ap, eff.hp) if isinstance(v, int))
        if total == 0:
            return UNKNOWN
        return HELPS if total > 0 else HARMS
    if isinstance(eff, (d.KeywordGrant, d.TraitGrant, d.AbilityGrant)):
        return HELPS
    if isinstance(eff, d.RuleGrant):
        return HARMS if eff.rule.kind.value.startswith("cant_") else HELPS
    return UNKNOWN
