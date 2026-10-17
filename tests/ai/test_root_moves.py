"""Root-move corrections behind search settings that are off in the baseline.

Each behaviour test also runs the baseline configuration (``BASELINE_PRESETS["standard"]``)
and shows it failing the same check; the guards show what the settings must keep.
"""

from __future__ import annotations

import dataclasses
from collections.abc import Callable, Iterator
from contextlib import contextmanager

import pytest
from tests.ai.test_information_set import (
    AGENT_SEEDS,
    GAME_SEEDS,
    PERMUTATION_SEEDS,
    STATES_PER_GAME,
    _hidden_ids,
    midgame_states,
)

from gcg_sim.ai import make_agent, noop
from gcg_sim.ai.actions import Key, canonical, prune_dominated
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.config import SearchConfig
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry, override_script, restore_entry
from gcg_sim.engine import core
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import information_set_key, permute_hidden
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, Zone
from gcg_sim.testkit import Scenario

A = ActionKind
FILLER = "GD01-060"
ZEON = ("ST03-008", "GD01-059", "GD01-035", "GD01-030", "GD01-031", "ST03-013", "GD01-115")
BASE = BASELINE_PRESETS["standard"]
SEEDS = tuple(range(31001, 31009))
NOOP_SEED = 31999


def config(**flags: bool) -> SearchConfig:
    return dataclasses.replace(BASE, **flags)


def flagged(st: GameState, **flags: bool) -> list[Action]:
    """Spending moves the no-effect check drops."""
    dec = st.pending
    assert dec is not None
    moves = prune_dominated(st, dec, canonical(st, dec))
    idle: set[Key] = noop.without_effect(st, dec.player, dec, moves, NOOP_SEED, **flags)
    return [a for key, a in moves if key in idle]


def play_turn(st: GameState, cfg: SearchConfig, seed: int) -> None:
    agents = tuple(
        make_agent("mcts", seed=seed + 100 * p, config=cfg, weights=BASELINE_WEIGHTS)
        for p in (0, 1)
    )
    stop = st.turn + 1
    while st.pending is not None and st.turn < stop:
        p = st.pending.player
        apply(st, agents[p].choose(st, p))


def hide_enemy_cards(sc: Scenario, hand: int) -> None:
    """Give player 1 ``hand`` cards hidden from player 0 and a varied deck, so that
    determinizations (and ``permute_hidden``) differ."""
    for i in range(hand):
        sc.add(1, ZEON[(2 * i) % len(ZEON)], Zone.HAND, known=False)
    sc.deck(1, *[ZEON[i % len(ZEON)] for i in range(28)])


# ---------------------------------------------------------------------------------------------
# noop_branch_choices: the no-effect check tries every option of the move's first choice


def rests_the_blocker(*, rested_decoy: bool, hidden: int = 0) -> GameState:
    """The lethal puzzle "rest the Blocker with Siege Ploy, then attack", optionally with a
    rested enemy Strike Dagger: the policy aims Siege Ploy at the more valuable Dagger, where
    resting does nothing."""
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, "ST01-005")  # GM 2/2
    sc.hand(0, "ST02-014")  # Siege Ploy: rest 1 enemy Unit with 5 or less HP
    sc.add(1, "ST01-008")  # Demi Trainer <Blocker>
    if rested_decoy:
        sc.add(1, "ST04-005", rested=True)  # Strike Dagger 3/2
    hide_enemy_cards(sc, hidden)
    sc.shields(0, FILLER, FILLER, FILLER)
    return sc.start()


def holds_removal() -> GameState:
    """Resting the Blocker now is wasted (it stands up before the Gundam can attack)."""
    sc = Scenario()
    sc.resources(0, 4, rested=3)
    sc.add(0, "GD01-013", deployed_this_turn=True)
    sc.add(0, "ST02-014", Zone.HAND)
    sc.add(1, "ST01-008")
    sc.resources(1, 2)
    sc.shields(0, FILLER, FILLER, FILLER, FILLER)
    return sc.start()


def test_branching_keeps_a_command_whose_policy_target_is_already_rested() -> None:
    st = rests_the_blocker(rested_decoy=True)
    assert [a.kind for a in flagged(st)] == [A.PLAY_COMMAND]
    assert flagged(st, branch_choices=True) == []


def test_lethal_rests_the_blocker_while_a_bigger_unit_is_rested() -> None:
    def wins(cfg: SearchConfig) -> int:
        won = 0
        for seed in SEEDS:
            st = rests_the_blocker(rested_decoy=True)
            play_turn(st, cfg, seed)
            won += st.winner == 0
        return won

    assert wins(config(noop_branch_choices=True)) >= 7
    assert wins(BASE) <= 1


def test_branching_still_prunes_removal_that_should_wait() -> None:
    st = holds_removal()
    assert [a.kind for a in flagged(st, branch_choices=True)] == [A.PLAY_COMMAND]


def test_branching_changes_nothing_when_no_option_is_better() -> None:
    st = rests_the_blocker(rested_decoy=False)
    assert flagged(st) == flagged(st, branch_choices=True) == []


# ---------------------------------------------------------------------------------------------
# noop_resource_aware: the no-effect check sees resource-area changes other than payment


def age_device(*, hidden: int = 0) -> GameState:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD02-029")  # Gundam AGE-1 Normal (AGE System)
    sc.hand(0, "GD02-103")  # AGE Device: place 1 EX Resource
    hide_enemy_cards(sc, hidden)
    sc.shields(0, FILLER, FILLER, FILLER)
    sc.shields(1, FILLER, FILLER, FILLER)
    return sc.start()


def test_resource_aware_check_keeps_placing_an_ex_resource() -> None:
    st = age_device()
    assert [a.kind for a in flagged(st)] == [A.PLAY_COMMAND]
    assert flagged(st, resource_aware=True) == []


@contextmanager
def command_script(number: str, *steps: d.Step) -> Iterator[None]:
    reg = get_registry()
    script = d.CardScript(number, abilities=(d.Command(d.Timing.MAIN, steps),))
    previous = override_script(reg, reg.db[number], script)
    core.clear_listen_cache()
    try:
        yield
    finally:
        restore_entry(reg, previous)
        core.clear_listen_cache()


def test_resource_aware_check_keeps_removing_an_enemy_ex_resource() -> None:
    """A removed EX Resource ceases to exist, so the baseline signature cannot see it."""
    enemy_ex = d.Sel(d.Side.ENEMY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),))
    with command_script("ST01-014", d.Choose("t1", enemy_ex), d.Exile(d.Var("t1"))):
        sc = Scenario()
        sc.resources(0, 3)
        sc.hand(0, "ST01-014")
        sc.resources(1, 2, ex=1)
        sc.shields(0, FILLER, FILLER, FILLER)
        sc.shields(1, FILLER, FILLER, FILLER)
        st = sc.start()
        assert [a.kind for a in flagged(st)] == [A.PLAY_COMMAND]
        assert flagged(st, resource_aware=True) == []


def test_resource_aware_check_still_prunes_removal_that_should_wait() -> None:
    st = holds_removal()
    assert [a.kind for a in flagged(st, resource_aware=True)] == [A.PLAY_COMMAND]


# ---------------------------------------------------------------------------------------------
# information-set honesty


ROOT_SETTINGS = config(
    noop_branch_choices=True,
    noop_resource_aware=True,
)


def decision(st: GameState, cfg: SearchConfig, seed: int) -> tuple[object, object]:
    assert st.pending is not None
    agent = make_agent("mcts", seed=seed, config=cfg, weights=BASELINE_WEIGHTS, log=True)
    action = agent.choose(st, st.pending.player)
    return action, agent.decision_log()[-1]["alternatives"]


SETTING_SCENARIOS: dict[str, Callable[[], GameState]] = {
    "noop_branch_choices": lambda: rests_the_blocker(rested_decoy=True, hidden=4),
    "noop_resource_aware": lambda: age_device(hidden=4),
}


@pytest.mark.parametrize("setting", list(SETTING_SCENARIOS))
def test_each_setting_is_invariant_under_hidden_permutation_where_it_applies(
    setting: str,
) -> None:
    st = SETTING_SCENARIOS[setting]()
    assert st.pending is not None
    player = st.pending.player
    cfg = config(**{setting: True})
    expected = decision(st, cfg, SEEDS[0])
    assert expected != decision(st, BASE, SEEDS[0])
    variants = [permute_hidden(st, player, s) for s in PERMUTATION_SEEDS]
    assert any(_hidden_ids(v, player) != _hidden_ids(st, player) for v in variants)
    for variant in variants:
        assert information_set_key(variant, player) == information_set_key(st, player)
        assert decision(variant, cfg, SEEDS[0]) == expected


@pytest.mark.parametrize("index", range(len(GAME_SEEDS) * STATES_PER_GAME))
def test_root_settings_keep_decisions_invariant_under_hidden_permutation(index: int) -> None:
    st = midgame_states()[index]
    assert st.pending is not None
    player = st.pending.player
    seed = AGENT_SEEDS[index % len(AGENT_SEEDS)]
    expected = decision(st, ROOT_SETTINGS, seed)
    for s in PERMUTATION_SEEDS:
        assert decision(permute_hidden(st, player, s), ROOT_SETTINGS, seed) == expected
