"""Policy settings carried by a search configuration (``SearchConfig.policy_*``)."""

from __future__ import annotations

import dataclasses

import pytest
from tests.ai.situations import burst

from gcg_sim.ai import make_agent, policy, targeting
from gcg_sim.ai.baseline import BASELINE_PRESETS
from gcg_sim.ai.config import PRESETS
from gcg_sim.ai.policy import BASELINE_KNOBS, PolicyKnobs, knobs_for
from gcg_sim.effects import dsl as d
from gcg_sim.effects import program as pr
from gcg_sim.engine import view as V
from gcg_sim.engine.game import apply
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.rng import SplitMix64
from gcg_sim.testkit import Scenario, attack, card_text, play

A = ActionKind
SAMPLES = 1000
FILLER = "GD01-060"
RULE_TABLE = PolicyKnobs(rule_polarity_table=True)
ZERO_AP_FACE = PolicyKnobs(zero_ap_face=True)


def _replace(**changes: object) -> PolicyKnobs:
    return knobs_for(dataclasses.replace(BASELINE_PRESETS["standard"], **changes))


def test_baseline_configurations_use_the_baseline_policy() -> None:
    assert all(knobs_for(c) == BASELINE_KNOBS for c in BASELINE_PRESETS.values())


def test_search_settings_map_to_policy_knobs() -> None:
    assert _replace(policy_rule_polarity_table=True) == RULE_TABLE
    assert _replace(policy_zero_ap_face=True) == ZERO_AP_FACE


def test_burst_score_sets_how_often_rollouts_accept_a_burst() -> None:
    st = burst()
    dec = st.pending
    assert dec is not None

    def declines(knobs: PolicyKnobs) -> int:
        rng = SplitMix64(7)
        scores = policy.option_scores(st, dec, knobs)
        picks = (policy.pick(dec.options, scores, rng, 0.5) for _ in range(SAMPLES))
        return sum(1 for a in picks if a.kind is ActionKind.NO)

    sure = _replace(policy_burst_yes_score=4.0)
    assert sure == PolicyKnobs(burst_yes_score=4.0)
    assert declines(BASELINE_KNOBS) > 80
    assert declines(sure) <= 2


# ---------------------------------------------------------------------------------------------
# rule polarity


def _pending_select(st: GameState) -> None:
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT


def _rule_sites(kind: d.RuleKind, side: d.Side) -> list[tuple[tuple[pr.Instr, ...], int]]:
    """Every Choose in the card pool whose chosen cards on ``side`` first get a ``kind`` rule."""
    R = V.reg()
    for cd in R.db:
        R.require(cd.def_id)
    sites = []
    for prog in R.programs:
        for pc, ins in enumerate(prog.instrs):
            if not isinstance(ins, d.Choose) or ins.sel.side is not side:
                continue
            step = targeting._first_step(prog.instrs, pc)
            effect = step.effect if isinstance(step, d.Apply) else None
            if isinstance(effect, d.RuleGrant) and effect.rule.kind is kind:
                sites.append((prog.instrs, pc))
    return sites


@pytest.mark.parametrize(
    ("kind", "side", "table_says", "prefix_says"),
    [
        (d.RuleKind.CANT_RECEIVE_DAMAGE, d.Side.FRIENDLY, targeting.HELPS, targeting.HARMS),
        (d.RuleKind.CANT_BE_ATTACKED, d.Side.FRIENDLY, targeting.HELPS, targeting.HARMS),
        (d.RuleKind.STAYS_RESTED_IN_START_PHASE, d.Side.ENEMY, targeting.HARMS, targeting.HELPS),
        (d.RuleKind.CANT_BLOCK, d.Side.ENEMY, targeting.HARMS, targeting.HARMS),
        (d.RuleKind.MAY_ATTACK_ACTIVE, d.Side.FRIENDLY, targeting.HELPS, targeting.HELPS),
    ],
)
def test_rule_polarity_table_signs_granted_rules(
    kind: d.RuleKind, side: d.Side, table_says: int, prefix_says: int
) -> None:
    """Every pool site of the kind and side. The first three rows are the sites the prefix rule
    signs wrongly; the can't-block and may-attack-active rows are controls on which both rules
    agree, so the baseline passes them too."""
    sites = _rule_sites(kind, side)
    assert sites
    for instrs, pc in sites:
        assert targeting._polarity_at(instrs, pc, rule_table=True) == table_says
        assert targeting._polarity_at(instrs, pc) == prefix_says


def test_rule_polarity_table_covers_every_rule_kind() -> None:
    assert set(targeting.RULE_POLARITY) == set(d.RuleKind)
    cheaper = d.RuleMod(d.RuleKind.PLAY_COST_DELTA, amount=-1)
    dearer = d.RuleMod(d.RuleKind.PLAY_COST_DELTA, amount=1)
    assert targeting._rule_polarity(cheaper) == targeting.HELPS
    assert targeting._rule_polarity(dearer) == targeting.HARMS


def test_rule_polarity_table_locks_down_the_most_valuable_enemy() -> None:
    """Moment of Rest keeps a rested enemy Unit rested through its next start phase: the
    table makes that harmful, so the policy aims it at the more valuable Unit."""
    sc = Scenario()
    sc.resources(0, 4)
    moment = sc.add(0, "GD04-102", Zone.HAND)
    aile = sc.add(1, "ST04-001", rested=True)  # 4/4 <Blocker>
    gm = sc.add(1, "ST01-005", rested=True)  # 2/2
    st = sc.start()
    play(st, moment)
    _pending_select(st)
    assert policy.choose(st, None, 0.0) == Action(A.SELECT, gm)
    assert policy.choose(st, None, 0.0, RULE_TABLE) == Action(A.SELECT, aile)


def _protection(attacked_is_big: bool) -> tuple[GameState, int]:
    """The opponent's GM (2 AP) attacks one of your two rested Units, which it would destroy;
    The Blue Giant in hand protects a friendly Unit from enemy Units with 2 or less AP during
    this battle."""
    sc = Scenario(active=1)
    sc.resources(0, 4)
    sc.hand(0, "ST03-014")
    gm = sc.add(0, "ST01-005", rested=True)  # GM 2/2
    gundam = sc.add(0, "GD01-013", rested=True, damage=2 if attacked_is_big else 0)  # Gundam 3/4
    attacker = sc.add(1, "ST01-005")
    sc.shields(0, FILLER, FILLER)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    target = gundam if attacked_is_big else gm
    attack(st, attacker, target)
    return st, target


def test_rule_polarity_table_protects_the_unit_in_the_battle() -> None:
    """A rule granted for this battle helps only a Unit in the battle: the table aims the
    protection at the attacked Unit whether it is the more or the less valuable one; the prefix
    rule protects the least valuable Unit, which is right only when that one is attacked."""
    for attacked_is_big in (True, False):
        st, target = _protection(attacked_is_big)
        play(st, st.zones[0][Zone.HAND][0])
        _pending_select(st)
        assert policy.choose(st, None, 0.0, RULE_TABLE) == Action(A.SELECT, target)
        prefix_protects_it = policy.choose(st, None, 0.0) == Action(A.SELECT, target)
        assert prefix_protects_it is not attacked_is_big


def test_rule_polarity_table_aims_a_battle_lock_at_the_best_enemy() -> None:
    """Control, which the prefix rule passes too: a lock on an enemy Unit for this battle
    matters for the Units outside the battle, so only rules for your own Units are aimed at the
    battle. An 【Attack】 trigger stops one enemy Unit from blocking: it goes to the more valuable
    enemy, not to the cheaper one."""
    with card_text("ST04-005", "【Attack】Choose 1 enemy Unit. It can't block during this battle."):
        sc = Scenario()
        dagger = sc.add(0, "ST04-005")  # Strike Dagger 3/2
        aile = sc.add(1, "ST04-001")  # Aile Strike Gundam 4/4 <Blocker>
        sc.add(1, "ST01-005")  # GM 2/2
        sc.shields(1, FILLER)
        st = sc.start()
        attack(st, dagger)
        _pending_select(st)
        assert policy.choose(st, None, 0.0, RULE_TABLE) == Action(A.SELECT, aile)
        assert policy.choose(st, None, 0.0) == Action(A.SELECT, aile)


TABLE_SEARCH = dataclasses.replace(PRESETS["standard"], policy_rule_polarity_table=True)


@pytest.mark.parametrize("seed", [1, 2])
def test_rule_polarity_table_search_saves_the_attacked_unit(seed: int) -> None:
    """The search keeps the protection play only if the policy's target makes it do something
    (the no-effect check plays the policy's choices), so the target choice decides the battle."""
    for attacked_is_big in (True, False):
        st, target = _protection(attacked_is_big)
        agents = (
            make_agent("mcts", seed=seed, config=TABLE_SEARCH),
            make_agent("mcts", seed=seed + 100),
        )
        while st.pending is not None and st.battle is not None:
            p = st.pending.player
            apply(st, agents[p].choose(st, p))
        assert st.cards[target].zone is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# attacks that cannot deal damage


def _face_minus_end(st: GameState, attacker: int, knobs: PolicyKnobs) -> float:
    """Score of attacking the player with ``attacker`` minus the score of ending the main phase."""
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.MAIN
    scores = policy.option_scores(st, dec, knobs)
    face = scores[dec.options.index(Action(A.ATTACK, attacker, PLAYER_TARGET))]
    return face - scores[dec.options.index(Action(A.END_MAIN))]


@pytest.mark.parametrize("exposed", [True, False])
def test_zero_ap_face_attack_scores_below_ending_the_main_phase(exposed: bool) -> None:
    sc = Scenario()
    worker = sc.add(0, "ST05-003")  # CGS Mobile Worker 0/2, no attack trigger
    if not exposed:
        sc.shields(1, FILLER)
    st = sc.start()
    assert _face_minus_end(st, worker, BASELINE_KNOBS) > 2.0
    assert _face_minus_end(st, worker, ZERO_AP_FACE) < 0.0


def test_zero_ap_face_attack_keeps_its_score_when_a_trick_could_make_it_matter() -> None:
    """Control, which the baseline passes too: an Action Command in hand could make the 0-AP
    attack matter in the battle's action step, so the setting leaves its score alone."""
    sc = Scenario()
    sc.resources(0, 3)
    sc.hand(0, "ST01-014")  # Unforeseen Incident, playable in the battle's action step
    worker = sc.add(0, "ST05-003")
    sc.add(1, "ST01-005", rested=True)  # a target for the trick
    st = sc.start()
    assert _face_minus_end(st, worker, ZERO_AP_FACE) == _face_minus_end(st, worker, BASELINE_KNOBS)


def test_zero_ap_attacker_does_not_count_against_the_blockers() -> None:
    """The enemy is exposed with one Blocker. A 0-AP Unit that the setting keeps from attacking
    is not a second attacker, so the real attacker is blocked from lethal and removal for the
    Blocker becomes the priority (the baseline counts the Mobile Worker and goes face)."""
    sc = Scenario()
    sc.add(0, "ST05-003")  # CGS Mobile Worker 0/2, no attack trigger
    gm = sc.add(0, "ST01-005")  # GM 2/2
    sc.add(1, "ST01-008")  # Demi Trainer <Blocker>
    st = sc.start()
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.MAIN
    dv = V.derived(st)
    assert not policy._board(st, dv, dec, BASELINE_KNOBS).blocked_from_lethal
    assert policy._board(st, dv, dec, ZERO_AP_FACE).blocked_from_lethal
    face = Action(A.ATTACK, gm, PLAYER_TARGET)
    scores = policy.option_scores(st, dec, ZERO_AP_FACE)
    assert scores[dec.options.index(face)] == pytest.approx(2.0)
