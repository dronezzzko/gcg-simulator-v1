"""Engine-level custom hooks used by compiled templates (registered on import)."""

from __future__ import annotations

from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import custom_cond, custom_filter, custom_step
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Zone


@custom_step("return_looked_bottom")
def return_looked_bottom(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """"Return the remaining cards randomly to the bottom of your deck.\""""
    looked = [u for u in f.vars.get("looked", ()) if st.cards[u].zone is Zone.DECK]
    if not looked:
        return False
    owner = st.cards[looked[0]].owner
    deck = st.zones[owner][Zone.DECK]
    for u in looked:
        deck.remove(u)
    st.rng.shuffle(looked)
    deck.extend(looked)
    st.touch()
    return True


@custom_step("deploy_ex_base")
def deploy_ex_base(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """Deploy an EX Base token (rule 5-17-3-1); base-section excess applies (rule 11-5)."""
    from gcg_sim.engine.interp import trash_excess

    p = f.controller
    for b in list(st.zones[p][Zone.BASE]):
        trash_excess(st, b)
    uid = core.new_card(st, V.reg().db.ex_base.def_id, p, Zone.BASE)
    core.record(st, "deployed", p, p, uid)
    core.emit(st, d.Ev.DEPLOYED, uid, player=p, by=p)
    return True


@custom_filter("is_attack_target")
def is_attack_target(st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]) -> bool:
    b = st.battle
    return b is not None and not b.ended and b.target == uid


@custom_cond("attack_target_damaged")
def attack_target_damaged(st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]) -> bool:
    b = st.battle
    return b is not None and not b.ended and b.target >= 0 and st.cards[b.target].damage > 0


__all__ = ["core"]
