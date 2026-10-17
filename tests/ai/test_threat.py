"""Threat-aware rollouts: the lethal extension, the face-first push and the Blocker guard.

Every setting defaults to the baseline behaviour; each test shows the baseline failing the
check that the setting passes.
"""

from __future__ import annotations

import dataclasses
import itertools
from typing import Any

import pytest

from gcg_sim.ai import mcts, policy
from gcg_sim.ai.config import LEAF_FLOOR, PRESETS, WIN_DECAY, SearchConfig
from gcg_sim.ai.evaluation import (
    DEFAULT_WEIGHTS,
    can_kill_to_move,
    threat_to_move,
    threatened,
)
from gcg_sim.ai.policy import BASELINE_KNOBS, GUARD, LETHAL, PolicyKnobs, knobs_for
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind
from gcg_sim.rng import SplitMix64
from gcg_sim.testkit import Scenario, attack

A = ActionKind
FILLER = "GD01-060"
STANDARD = PRESETS["standard"]
PUSH = PolicyKnobs(push=True)
GUARDED = PolicyKnobs(guard=True)


def scores(st: GameState, knobs: PolicyKnobs) -> dict[Action, float]:
    dec = st.pending
    assert dec is not None
    return dict(zip(dec.options, policy.option_scores(st, dec, knobs), strict=True))


def best(st: GameState, knobs: PolicyKnobs) -> Action:
    s = scores(st, knobs)
    return max(s, key=s.__getitem__)


# ---------------------------------------------------------------------------------------------
# settings


def test_baseline_values_reproduce_the_baseline_policy() -> None:
    assert knobs_for(STANDARD) == BASELINE_KNOBS
    on = dataclasses.replace(STANDARD, rollout_push="all", rollout_guard=True)
    assert knobs_for(on) == PolicyKnobs(push=True, guard=True)
    assert knobs_for(dataclasses.replace(STANDARD, rollout_push="extension")) == BASELINE_KNOBS


@pytest.mark.parametrize(
    "change",
    [
        {"extension_trigger": "sometimes"},
        {"rollout_push": "always"},
        {"lethal_extension_turns": 3},
        {"horizon_turns": 5},
    ],
)
def test_invalid_settings_are_rejected(change: dict[str, Any]) -> None:
    with pytest.raises(ValueError):
        dataclasses.replace(STANDARD, **change)


def test_a_win_at_the_end_of_the_longest_valid_extension_outranks_every_estimate() -> None:
    cfg = dataclasses.replace(STANDARD, lethal_extension_turns=2)
    latest_win = 1.0 - WIN_DECAY * (cfg.horizon_turns + cfg.lethal_extension_turns)
    assert latest_win > 1.0 - LEAF_FLOOR


# ---------------------------------------------------------------------------------------------
# threat tests


def _shielded_kill(active: int) -> GameState:
    """``active`` has three ready GMs; the enemy has one Shield, no Blocker that is ready, and
    a rested 1/1 that a GM destroys without dying."""
    sc = Scenario(active=active)
    for _ in range(3):
        sc.add(active, "ST01-005")  # GM 2/2
    sc.add(1 - active, "ST01-008", rested=True)  # Demi Trainer 1/1 <Blocker>
    sc.shields(1 - active, FILLER)
    sc.shields(active, FILLER, FILLER, FILLER)
    return sc.start()


def test_threat_helpers() -> None:
    st = _shielded_kill(0)
    assert can_kill_to_move(st)
    assert threat_to_move(st)
    assert threatened(st, 1)
    assert not threatened(st, 0)


def test_threatened_counts_the_other_blockers_only() -> None:
    sc = Scenario()
    first = sc.add(0, "ST01-008")  # Demi Trainer <Blocker>
    second = sc.add(0, "ST01-008")
    sc.add(1, "ST01-005")
    st = sc.start()
    assert not threatened(st, 0)
    assert not threatened(st, 0, spare=first)
    st.cards[second].rested = True
    st.touch()
    assert not threatened(st, 0)
    assert not threatened(st, 0, spare=second)
    assert threatened(st, 0, spare=first)


# ---------------------------------------------------------------------------------------------
# push


@pytest.mark.parametrize("seat", [0, 1])
def test_push_goes_face_when_the_attacking_side_can_kill(seat: int) -> None:
    st = _shielded_kill(seat)
    assert best(st, BASELINE_KNOBS).b != PLAYER_TARGET
    s = scores(st, PUSH)
    pushed = best(st, PUSH)
    assert pushed.kind is A.ATTACK
    assert pushed.b == PLAYER_TARGET
    assert s[pushed] >= LETHAL / 2


@pytest.mark.parametrize("seat", [0, 1])
def test_push_changes_nothing_without_lethal(seat: int) -> None:
    sc = Scenario(active=seat)
    sc.add(seat, "ST01-005")
    sc.add(seat, "ST01-005")
    sc.add(1 - seat, "ST01-008", rested=True)
    sc.shields(1 - seat, FILLER, FILLER)
    st = sc.start()
    assert not can_kill_to_move(st)
    assert scores(st, PUSH) == scores(st, BASELINE_KNOBS)


# ---------------------------------------------------------------------------------------------
# guard


def _exposed_with_blocker(*, shields: int = 0, enemy_rested: bool = False) -> GameState:
    """Player 0 has a <Blocker> and a GM; the enemy has one 1/1 Unit."""
    sc = Scenario()
    sc.add(0, "ST01-008")  # Demi Trainer 1/1 <Blocker>
    sc.add(0, "ST01-005")  # GM 2/2
    sc.add(1, "ST01-008", rested=enemy_rested)
    sc.shields(0, *[FILLER] * shields)
    sc.shields(1, *[FILLER] * 4)
    return sc.start()


def _attack_with(attacker: int, target: int = PLAYER_TARGET) -> Action:
    return Action(A.ATTACK, attacker, target)


def test_guard_keeps_the_last_blocker_home_while_threatened() -> None:
    st = _exposed_with_blocker()
    blocker_face = _attack_with(0)
    gm_face = _attack_with(1)
    base, guarded = scores(st, BASELINE_KNOBS), scores(st, GUARDED)
    assert base[blocker_face] > base[Action(A.END_MAIN)]
    assert guarded[blocker_face] == pytest.approx(base[blocker_face] - GUARD)
    assert guarded[blocker_face] < guarded[Action(A.END_MAIN)]
    assert guarded[gm_face] == base[gm_face]


def test_guard_leaves_attacks_alone_when_not_threatened() -> None:
    st = _exposed_with_blocker(shields=1)
    assert not threatened(st, 0, spare=0)
    assert scores(st, GUARDED) == scores(st, BASELINE_KNOBS)


def test_guard_allows_a_blocker_attack_that_destroys_an_attacker() -> None:
    st = _exposed_with_blocker(enemy_rested=True)
    kill = _attack_with(0, 2)
    face = _attack_with(0)
    base, guarded = scores(st, BASELINE_KNOBS), scores(st, GUARDED)
    assert guarded[kill] == base[kill]
    assert guarded[face] < base[face]


def test_guard_never_holds_back_a_lethal_attempt() -> None:
    """Both players are exposed; the enemy's ready <Blocker> can stop one of three attackers,
    so attacking with the 1/1 Blocker is part of the lethal even though it may die."""
    sc = Scenario()
    sc.add(0, "ST01-008")  # Demi Trainer 1/1 <Blocker>
    sc.add(0, "ST01-005")
    sc.add(0, "ST01-005")
    sc.add(1, "ST01-008")
    st = sc.start()
    assert threatened(st, 0, spare=0)
    face = _attack_with(0)
    base, guarded = scores(st, BASELINE_KNOBS), scores(st, GUARDED)
    assert base[face] < LETHAL / 2
    assert guarded[face] == base[face]


def _block(*, attacker: str, target_unit: bool, extra_enemy: bool) -> GameState:
    """Player 1 attacks exposed player 0, whose only <Blocker> is Shiden Custom 4/3."""
    sc = Scenario(active=1)
    sc.add(0, "GD05-058")  # Shiden Custom 4/3 <Blocker>
    target = sc.add(0, "ST01-005", rested=True)
    unit = sc.add(1, attacker)
    if extra_enemy:
        sc.add(1, "GD01-004", deployed_this_turn=True)  # Guncannon 2/3
    sc.shields(1, *[FILLER] * 4)
    st = sc.start()
    attack(st, unit, target if target_unit else PLAYER_TARGET)
    return st


def test_guard_declines_a_block_that_spends_the_last_blocker() -> None:
    st = _block(attacker="ST01-002", target_unit=True, extra_enemy=True)  # MA Form 4/3
    assert best(st, BASELINE_KNOBS).kind is A.BLOCK
    assert best(st, GUARDED).kind is A.NO_BLOCK


def test_guard_keeps_a_block_the_blocker_survives_with_no_other_attacker() -> None:
    st = _block(attacker="ST01-005", target_unit=True, extra_enemy=False)  # GM 2/2
    assert scores(st, GUARDED) == scores(st, BASELINE_KNOBS)
    assert best(st, GUARDED).kind is A.BLOCK


def test_guard_never_declines_the_block_that_prevents_the_loss() -> None:
    st = _block(attacker="ST01-002", target_unit=False, extra_enemy=True)
    assert scores(st, GUARDED) == scores(st, BASELINE_KNOBS)
    assert best(st, GUARDED).kind is A.BLOCK


def test_guard_counts_the_attacker_a_trade_destroys() -> None:
    """Shiden Custom and MA Form destroy each other; the one enemy Unit left cannot break the
    last Shield and hit Player 0, so the trade is not guarded."""
    sc = Scenario(active=1)
    shiden = sc.add(0, "GD05-058")
    target = sc.add(0, "ST01-005", rested=True)
    ma_form = sc.add(1, "ST01-002")
    sc.add(1, "GD01-004", rested=True)
    sc.shields(0, FILLER)
    sc.shields(1, *[FILLER] * 4)
    st = sc.start()
    attack(st, ma_form, target)
    assert threatened(st, 0, spare=shiden)
    assert not threatened(st, 0, spare=shiden, removed=ma_form)
    assert scores(st, GUARDED) == scores(st, BASELINE_KNOBS)
    assert best(st, GUARDED).kind is A.BLOCK


def _block_survived(*, shields: int) -> tuple[GameState, int]:
    """Player 1's GM attacks a rested Unit of Player 0, whose only <Blocker> (Shiden Custom 4/3)
    destroys it and survives; Player 1 also has an active Guncannon and a rested GM."""
    sc = Scenario(active=1)
    shiden = sc.add(0, "GD05-058")
    target = sc.add(0, "ST01-005", rested=True)
    gm = sc.add(1, "ST01-005")
    sc.add(1, "GD01-004")
    sc.add(1, "ST01-005", rested=True)
    sc.shields(0, *[FILLER] * shields)
    sc.shields(1, *[FILLER] * 4)
    st = sc.start()
    attack(st, gm, target)
    return st, shiden


def test_guard_counts_only_this_turns_attackers_for_a_blocker_that_survives() -> None:
    """The Blocker is active again on Player 1's next turn; this turn only the Guncannon can
    still attack, which the last Shield absorbs."""
    st, shiden = _block_survived(shields=1)
    assert threatened(st, 0, spare=shiden)
    assert not threatened(st, 0, spare=shiden, now=True)
    assert scores(st, GUARDED) == scores(st, BASELINE_KNOBS)
    assert best(st, GUARDED).kind is A.BLOCK


def test_guard_declines_a_block_that_lets_another_attacker_through_this_turn() -> None:
    st, shiden = _block_survived(shields=0)
    assert threatened(st, 0, spare=shiden, now=True)
    assert best(st, BASELINE_KNOBS).kind is A.BLOCK
    assert best(st, GUARDED).kind is A.NO_BLOCK


# ---------------------------------------------------------------------------------------------
# lethal extension


def _two_turn_lethal() -> GameState:
    """Player 0's GMs were deployed this turn: the lethal on the one-Shield opponent comes on
    turn root+2, the first turn after the rollout horizon."""
    sc = Scenario()
    sc.add(0, "ST01-005", deployed_this_turn=True)
    sc.add(0, "ST01-005", deployed_this_turn=True)
    sc.resources(0, 2)
    sc.hand(0, "ST04-005")  # Strike Dagger 3/2
    sc.shields(0, FILLER, FILLER, FILLER)
    sc.shields(1, FILLER)
    return sc.start()


def _leaves(monkeypatch: pytest.MonkeyPatch, cfg: SearchConfig) -> list[tuple[int | None, int]]:
    """(winner, turns after the root) of every leaf a search from ``_two_turn_lethal`` values."""
    seen: list[tuple[int | None, int]] = []
    leaf_value = mcts._leaf_value

    def spy(s: GameState, ctx: Any) -> float:
        seen.append((s.winner, s.turn - ctx.root_turn))
        return leaf_value(s, ctx)

    monkeypatch.setattr(mcts, "_leaf_value", spy)
    mcts.search(_two_turn_lethal(), 0, cfg, DEFAULT_WEIGHTS, SplitMix64(33020))
    return seen


@pytest.mark.parametrize(
    ("change", "extends"),
    [
        ({}, False),
        ({"lethal_extension_turns": 1}, True),
        ({"lethal_extension_turns": 1, "extension_trigger": "threat"}, True),
        ({"lethal_extension_turns": 1, "extension_trigger": "opponent"}, False),
    ],
)
def test_extension_plays_the_lethal_turn_after_the_horizon(
    monkeypatch: pytest.MonkeyPatch, change: dict[str, Any], extends: bool
) -> None:
    seen = _leaves(monkeypatch, dataclasses.replace(STANDARD, **change))
    wins_after_horizon = sum(1 for winner, turns in seen if winner == 0 and turns == 2)
    assert seen
    if extends:
        assert wins_after_horizon >= len(seen) // 2
    else:
        assert wins_after_horizon == 0


def test_extension_uses_the_rollout_budget_left_and_pushes_only_inside_it(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls: list[tuple[int, int, int, PolicyKnobs]] = []
    play = mcts.play_until_turn

    def spy(s: GameState, turn: int, rng: Any, temp: float, cap: int, knobs: PolicyKnobs) -> int:
        n = play(s, turn, rng, temp, cap, knobs)
        calls.append((turn, cap, n, knobs))
        return n

    monkeypatch.setattr(mcts, "play_until_turn", spy)
    st = _two_turn_lethal()
    cfg = dataclasses.replace(
        STANDARD, lethal_extension_turns=1, rollout_push="extension", max_rollout_decisions=40
    )
    mcts.search(st, 0, cfg, DEFAULT_WEIGHTS, SplitMix64(33021))
    horizon = st.turn + cfg.horizon_turns
    extended = [
        (first, second)
        for first, second in itertools.pairwise(calls)
        if first[0] == horizon and second[0] == horizon + 1
    ]
    assert extended
    for first, second in extended:
        assert second[1] == cfg.max_rollout_decisions - first[2]
        assert not first[3].push
        assert second[3].push
