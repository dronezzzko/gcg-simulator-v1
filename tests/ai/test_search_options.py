"""Search settings that the presets leave off: more iterations for close effect-target calls and
observation-keyed tree nodes. Each one changes the search only where it says it does, keeps
decisions inside the information set, and (for the tree keys) fixes the split it targets.
"""

from __future__ import annotations

import dataclasses

import pytest
from tests.ai.test_information_set import (
    AGENT_SEEDS,
    GAME_SEEDS,
    PERMUTATION_SEEDS,
    STATES_PER_GAME,
    midgame_states,
)
from tests.ai.test_puzzles_targets import PUZZLES

from gcg_sim.ai import make_agent, mcts
from gcg_sim.ai.actions import Key, canonical
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.config import SearchConfig
from gcg_sim.engine.observe import permute_hidden
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone
from gcg_sim.rng import SplitMix64
from gcg_sim.testkit import Scenario, attack, play

BASELINE = BASELINE_PRESETS["standard"]
EXTENDED = dataclasses.replace(BASELINE, target_extension=3.0)
OBSERVED = dataclasses.replace(BASELINE, observation_keys=True)
ALL_ON = dataclasses.replace(BASELINE, target_extension=3.0, observation_keys=True)
SEARCHER, OPPONENT = 0, 1


def _search(st: GameState, config: SearchConfig, seed: int) -> mcts.SearchResult:
    assert st.pending is not None
    return mcts.search(st, st.pending.player, config, BASELINE_WEIGHTS, SplitMix64(seed))


def _target_positions() -> list[GameState]:
    return [build()[0] for build, _, _ in PUZZLES.values()]


def kill_the_damaged_unit() -> GameState:
    """Zeon Remnant Forces deals 1 damage to an enemy Unit: it destroys the rested Demi Trainer
    (1/1) and only scratches the Gundam (3/4). Both decks are filler, so every world agrees and
    the search tells the targets apart within the baseline budget."""
    sc = Scenario()
    sc.resources(SEARCHER, 2)
    card = sc.add(SEARCHER, "GD01-115", Zone.HAND)
    sc.add(OPPONENT, "GD01-013")
    sc.add(OPPONENT, "ST01-008", rested=True)
    sc.shields(SEARCHER, *["GD01-060"] * 5)
    sc.shields(OPPONENT, *["GD01-060"] * 5)
    st = sc.start()
    play(st, card)
    return st


# ---------------------------------------------------------------------------------------------
# target extension


def test_extension_leaves_other_decisions_unchanged() -> None:
    checked = 0
    for index, st in enumerate(midgame_states()[:8]):
        assert st.pending is not None
        if mcts.is_target_decision(st.pending):
            continue
        seed = AGENT_SEEDS[index % len(AGENT_SEEDS)]
        assert _search(st, EXTENDED, seed) == _search(st, BASELINE, seed)
        checked += 1
    assert checked > 0


def test_extension_only_adds_iterations_after_the_baseline_budget() -> None:
    """The extra worlds and rollout seeds are drawn after the baseline's, so a search that
    is not a close call at the end of the budget is the baseline search, bit for bit."""
    extended = unchanged = 0
    for st in [*_target_positions(), kill_the_damaged_unit()]:
        assert st.pending is not None and mcts.is_target_decision(st.pending)
        for seed in range(1, 9):
            base, ext = _search(st, BASELINE, seed), _search(st, EXTENDED, seed)
            budget = BASELINE.budget(len(base.alternatives))
            if ext.iterations == base.iterations:
                assert ext == base
                unchanged += 1
            else:
                assert base.iterations < ext.iterations <= 3 * budget
                extended += 1
    assert extended > 0 and unchanged > 0


# ---------------------------------------------------------------------------------------------
# observation keys


def opponent_may_hold_a_trick() -> GameState:
    """The searcher attacks with three Units into an empty board. The defender has three
    active resources and four hidden cards from a deck that is a quarter Unforeseen Incident
    (【Main】/【Action】AP-3, Lv.3, cost 1): in some determinizations it gets an action-step
    decision in each battle, in the others the engine passes for it."""
    sc = Scenario()
    sc.add(SEARCHER, "GD01-013")  # Gundam 3/4
    sc.add(SEARCHER, "ST01-005")  # GM 2/2
    sc.add(SEARCHER, "ST01-005")
    sc.resources(OPPONENT, 3)
    pool = ["ST01-014", "ST01-005", "GD01-008", "GD01-013"]
    for i in range(4):
        sc.add(OPPONENT, pool[(i + 1) % len(pool)], Zone.HAND, known=False)
    sc.deck(OPPONENT, *[pool[i % len(pool)] for i in range(24)])
    sc.shields(SEARCHER, *["GD01-060"] * 5)
    sc.shields(OPPONENT, *["GD01-060"] * 5)
    return sc.start()


def _tree(st: GameState, config: SearchConfig, seed: int, mp: pytest.MonkeyPatch) -> mcts._Node:
    roots: list[mcts._Node] = []
    result = mcts._result

    def keep_root(
        root: mcts._Node,
        keys: list[mcts.ChildKey],
        moves: list[tuple[Key, Action]],
        iterations: int,
    ) -> mcts.SearchResult:
        roots.append(root)
        return result(root, keys, moves, iterations)

    mp.setattr(mcts, "_result", keep_root)
    _search(st, config, seed)
    mp.undo()
    return roots[0]


def _nodes(root: mcts._Node) -> list[mcts._Node]:
    out, stack = [], [root]
    while stack:
        node = stack.pop()
        out.append(node)
        stack.extend(ch.node for ch in node.children.values() if ch.node is not None)
    return out


def _is_opponent_pass(key: mcts.ChildKey, child: mcts._Child) -> bool:
    return child.mover == OPPONENT and key[2][0] == ActionKind.PASS.value


def _pass_splits(root: mcts._Node) -> int:
    """Searcher moves held both by a node and by the node below an opponent's action-step
    pass from it: the same searcher decision, reached with and without the opponent having
    had an option to act."""
    splits = 0
    for node in _nodes(root):
        own = {key[1:] for key, ch in node.children.items() if ch.mover == SEARCHER}
        for key, ch in node.children.items():
            if _is_opponent_pass(key, ch) and ch.node is not None:
                below = {k[1:] for k, c in ch.node.children.items() if c.mover == SEARCHER}
                splits += len(own & below)
    return splits


@pytest.mark.parametrize("seed", [1, 2])
def test_observation_keys_reach_searcher_nodes_the_same_way_after_an_opponent_pass(
    seed: int, monkeypatch: pytest.MonkeyPatch
) -> None:
    st = opponent_may_hold_a_trick()
    assert _pass_splits(_tree(st, BASELINE, seed, monkeypatch)) > 0
    tree = _tree(st, OBSERVED, seed, monkeypatch)
    assert _pass_splits(tree) == 0
    passes = [
        ch
        for node in _nodes(tree)
        for key, ch in node.children.items()
        if _is_opponent_pass(key, ch)
    ]
    assert any(ch.visits > 1 for ch in passes)
    assert all(ch.node is None for ch in passes)


def one_play_in_two_decision_kinds() -> tuple[GameState, GameState]:
    """The searcher can play Unforeseen Incident (【Main】/【Action】) in its main phase, and
    again in the action step of an attack on the player."""
    sc = Scenario()
    sc.resources(SEARCHER, 3)
    gm = sc.add(SEARCHER, "ST01-005")
    sc.add(SEARCHER, "ST01-014", Zone.HAND)
    sc.add(OPPONENT, "GD01-060", rested=True)
    sc.shields(SEARCHER, *["GD01-060"] * 5)
    sc.shields(OPPONENT, *["GD01-060"] * 5)
    main = sc.start()
    battle = main.clone()
    attack(battle, gm)
    return main, battle


def _command_children(config: SearchConfig) -> int:
    node = mcts._Node()
    for st in one_play_in_two_decision_kinds():
        assert st.pending is not None and st.pending.player == SEARCHER
        mcts._add_children(node, st, st.pending, canonical(st, st.pending), config)
    return sum(1 for key in node.children if key[2][0] == ActionKind.PLAY_COMMAND.value)


def test_observation_keys_keep_main_and_action_step_plays_apart() -> None:
    """A node can meet a main-phase decision and an action-step decision of one player (in
    different worlds, and after an opponent's pass the walk stays in its node): the same card
    play gets a child per decision kind instead of one shared statistic."""
    main, battle = one_play_in_two_decision_kinds()
    assert main.pending is not None and main.pending.kind is DecisionKind.MAIN
    assert battle.pending is not None and battle.pending.kind is DecisionKind.ACTION_STEP
    assert _command_children(BASELINE) == 1
    assert _command_children(OBSERVED) == 2


# ---------------------------------------------------------------------------------------------
# information set


def _decision(config: SearchConfig, seed: int, st: GameState) -> tuple[object, object]:
    agent = make_agent("mcts", seed=seed, log=True, config=config, weights=BASELINE_WEIGHTS)
    assert st.pending is not None
    action = agent.choose(st, st.pending.player)
    return action, agent.decision_log()[-1]["alternatives"]


def _invariance_states() -> list[GameState]:
    return [*midgame_states(), *_target_positions(), opponent_may_hold_a_trick()]


@pytest.mark.parametrize("index", range(len(GAME_SEEDS) * STATES_PER_GAME + len(PUZZLES) + 1))
def test_settings_keep_decisions_inside_the_information_set(index: int) -> None:
    st = _invariance_states()[index]
    assert st.pending is not None
    p = st.pending.player
    seed = AGENT_SEEDS[index % len(AGENT_SEEDS)]
    expected = _decision(ALL_ON, seed, st)
    for s in PERMUTATION_SEEDS:
        assert _decision(ALL_ON, seed, permute_hidden(st, p, s)) == expected
