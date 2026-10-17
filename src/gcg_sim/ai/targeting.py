"""Whether a pending card choice helps or hurts the chosen cards.

The resolving effect's program is public card text, so the policy reads the instructions
that follow the waiting ``Choose`` and classifies what happens to the chosen variable.
"""

from __future__ import annotations

from gcg_sim.effects import dsl as d
from gcg_sim.effects import program as pr
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Duration

HARMS = -1
UNKNOWN = 0
HELPS = 1

_HARMFUL_STEPS = (d.Damage, d.Destroy, d.Rest, d.ReturnToHand, d.ToDeck, d.Exile, d.ToTrash)
_HELPFUL_STEPS = (d.Recover, d.SetActive, d.AddToHand, d.DeployCard, d.PlayCard)
RK = d.RuleKind
RULE_POLARITY: dict[d.RuleKind, int] = {
    RK.CANT_ATTACK: HARMS,
    RK.CANT_ATTACK_PLAYER: HARMS,
    RK.CANT_ATTACK_UNITS: HARMS,
    RK.CANT_BLOCK: HARMS,
    RK.CANT_BE_BLOCKED: HELPS,
    RK.MAY_ATTACK_ACTIVE: HELPS,
    RK.ATTACK_ON_DEPLOY_TURN: HELPS,
    RK.CANT_BE_SET_ACTIVE: HARMS,
    RK.STAYS_RESTED_IN_START_PHASE: HARMS,
    RK.DEPLOYED_RESTED: HARMS,
    RK.CANT_BE_PAIRED: HARMS,
    RK.CANT_RECEIVE_DAMAGE: HELPS,
    RK.REDUCE_DAMAGE: HELPS,
    RK.CANT_BE_DESTROYED: HELPS,
    RK.CANT_BE_CHOSEN: HELPS,
    RK.CANT_BE_RETURNED: HELPS,
    RK.NO_BLOCKER_VS: HELPS,
    RK.MUST_ATTACK: HARMS,
    RK.PLAY_COST_DELTA: UNKNOWN,
    RK.PLAY_LEVEL_DELTA: UNKNOWN,
    RK.ATTACK_TARGET_FIXED: HARMS,
    RK.CANT_BE_ATTACKED: HELPS,
    RK.FORCE_ATTACK_TARGET: HELPS,
    RK.SHIELD_AREA_PROTECTION: HELPS,
    RK.SHIELD_AREA_REDUCTION: HELPS,
    RK.AP_CANT_BE_REDUCED: HELPS,
    RK.REDIRECT_BATTLE_DAMAGE: HELPS,
    RK.REST_SUBSTITUTE: HELPS,
    RK.CUSTOM: UNKNOWN,
}
_SIGNED_RULES = frozenset({RK.PLAY_COST_DELTA, RK.PLAY_LEVEL_DELTA})
_cache: dict[tuple[int, int, bool], int] = {}
_battle_cache: dict[tuple[int, int], bool] = {}


def choice_polarity(st: GameState, rule_table: bool = False) -> int:
    """HELPS / HARMS / UNKNOWN for the cards chosen at the waiting instruction. With
    ``rule_table`` a granted rule is judged by ``RULE_POLARITY``; otherwise every "can't" rule
    counts as harmful and every other rule as helpful."""
    if not st.frames or not st.frames[-1].waiting:
        return UNKNOWN
    f = st.frames[-1]
    key = (f.program_id, f.pc, rule_table)
    cached = _cache.get(key)
    if cached is None:
        instrs = V.reg().programs[f.program_id].instrs
        cached = _polarity_at(instrs, f.pc, rule_table)
        _cache[key] = cached
    return cached


def choice_rule_lasts_this_battle(st: GameState) -> bool:
    """Whether the waiting ``Choose`` grants the chosen cards a rule only for the current battle
    (one that helps your own Unit then matters only for a Unit in that battle)."""
    if not st.frames or not st.frames[-1].waiting:
        return False
    f = st.frames[-1]
    key = (f.program_id, f.pc)
    cached = _battle_cache.get(key)
    if cached is None:
        step = _first_step(V.reg().programs[f.program_id].instrs, f.pc)
        cached = (
            isinstance(step, d.Apply)
            and isinstance(step.effect, d.RuleGrant)
            and step.duration is Duration.THIS_BATTLE
        )
        _battle_cache[key] = cached
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


def _polarity_at(instrs: tuple[pr.Instr, ...], pc: int, rule_table: bool = False) -> int:
    step = _first_step(instrs, pc)
    if isinstance(step, _HARMFUL_STEPS):
        return HARMS
    if isinstance(step, _HELPFUL_STEPS):
        return HELPS
    if isinstance(step, d.Apply):
        return _effect_polarity(step.effect, rule_table)
    return UNKNOWN


def _first_step(instrs: tuple[pr.Instr, ...], pc: int) -> pr.Instr | None:
    """The first harmful, helpful or ``Apply`` step done to the cards chosen at ``pc``."""
    ins = instrs[pc]
    if not isinstance(ins, d.Choose):
        return None
    for later in instrs[pc + 1 :]:
        ref = getattr(later, "ref", None)
        if not isinstance(ref, (d.Var, d.Union)) or not _uses(ref, ins.var):
            continue
        if isinstance(later, (*_HARMFUL_STEPS, *_HELPFUL_STEPS, d.Apply)):
            return later
    return None


def _uses(ref: d.Ref, var: str) -> bool:
    if isinstance(ref, d.Var):
        return ref.name == var
    if isinstance(ref, d.Union):
        return any(_uses(r, var) for r in ref.refs)
    return False


def _effect_polarity(eff: d.Continuous, rule_table: bool = False) -> int:
    if isinstance(eff, d.StatMod):
        total = sum(v for v in (eff.ap, eff.hp) if isinstance(v, int))
        if total == 0:
            return UNKNOWN
        return HELPS if total > 0 else HARMS
    if isinstance(eff, (d.KeywordGrant, d.TraitGrant, d.AbilityGrant)):
        return HELPS
    if isinstance(eff, d.RuleGrant):
        if rule_table:
            return _rule_polarity(eff.rule)
        return HARMS if eff.rule.kind.value.startswith("cant_") else HELPS
    return UNKNOWN


def _rule_polarity(rule: d.RuleMod) -> int:
    if rule.kind in _SIGNED_RULES and isinstance(rule.amount, int) and rule.amount:
        return HELPS if rule.amount < 0 else HARMS
    return RULE_POLARITY[rule.kind]
