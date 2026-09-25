"""Agents: legal choices for every decision kind, determinism, and the decision-log format."""

from __future__ import annotations

import json
from collections.abc import Callable

import pytest
from hypothesis import given, settings
from hypothesis import strategies as hs
from tests.ai.decks import DECKS
from tests.ai.situations import EXTRA_SELECTS, SITUATIONS

from gcg_sim.ai import AGENT_KINDS, PRESETS, SearchConfig, make_agent
from gcg_sim.ai.actions import Key, canonical, ex_used, prune_dominated
from gcg_sim.ai.agents import MctsAgent
from gcg_sim.ai.noop import without_effect
from gcg_sim.ai.selfplay import run_game
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone
from gcg_sim.testkit import Scenario, attack, pass_all

ALL_SITUATIONS: dict[str, Callable[[], GameState]] = {
    **{k.value: f for k, f in SITUATIONS.items()},
    **EXTRA_SELECTS,
}


def test_situations_cover_every_decision_kind() -> None:
    covered = {f().pending.kind for f in SITUATIONS.values()}  # type: ignore[union-attr]
    assert covered == set(DecisionKind) - {DecisionKind.PAYMENT}
    for kind, build in SITUATIONS.items():
        st = build()
        assert st.pending is not None and st.pending.kind is kind


@pytest.mark.parametrize("kind", AGENT_KINDS)
@pytest.mark.parametrize("situation", sorted(ALL_SITUATIONS))
def test_agent_returns_a_legal_action(kind: str, situation: str) -> None:
    st = ALL_SITUATIONS[situation]()
    dec = st.pending
    assert dec is not None
    before = json.dumps(st.to_json(), sort_keys=True)
    action = make_agent(kind, seed=3).choose(st, dec.player)
    assert action in dec.options
    assert json.dumps(st.to_json(), sort_keys=True) == before, "choose() must not mutate st"
    apply(st, action)


def test_choose_rejects_wrong_player() -> None:
    st = SITUATIONS[DecisionKind.MAIN]()
    assert st.pending is not None
    with pytest.raises(ValueError, match="no decision pending"):
        make_agent("random").choose(st, 1 - st.pending.player)


def test_make_agent_validates_arguments() -> None:
    assert set(PRESETS) == {"standard", "strong"}
    assert isinstance(PRESETS["standard"], SearchConfig)
    assert PRESETS["strong"].max_iterations > PRESETS["standard"].max_iterations
    with pytest.raises(ValueError, match="unknown agent kind"):
        make_agent("minimax")
    with pytest.raises(ValueError, match="unknown AI preset"):
        make_agent("mcts", preset="weak")
    assert make_agent("mcts", preset="strong").name == "mcts-strong"


def _choices(kind: str, seed: int, decisions: int) -> list[Action]:
    agents = (make_agent(kind, seed=seed), make_agent(kind, seed=seed + 1))
    st = new_game(DECKS, 42)
    out = []
    while st.pending is not None and len(out) < decisions:
        p = st.pending.player
        a = agents[p].choose(st, p)
        out.append(a)
        apply(st, a)
    return out


@pytest.mark.parametrize(("kind", "decisions"), [("random", 200), ("greedy", 60), ("mcts", 14)])
def test_same_seed_same_choices(kind: str, decisions: int) -> None:
    assert _choices(kind, 11, decisions) == _choices(kind, 11, decisions)


def test_random_agent_depends_on_seed() -> None:
    assert _choices("random", 1, 60) != _choices("random", 2, 60)


@pytest.mark.parametrize("kind", AGENT_KINDS)
def test_decision_log_format(kind: str) -> None:
    agent = make_agent(kind, seed=5, log=True)
    silent = make_agent(kind, seed=5)
    st = SITUATIONS[DecisionKind.MAIN]()
    assert st.pending is not None
    chosen = agent.choose(st, st.pending.player)
    assert silent.choose(st, st.pending.player) == chosen
    assert silent.decision_log() == []
    log = agent.decision_log()
    assert len(log) == 1
    entry = log[0]
    assert set(entry) >= {"turn", "player", "decision", "chosen", "alternatives"}
    assert entry["turn"] == st.turn and entry["player"] == st.pending.player
    assert entry["decision"] == "main"
    assert Action.from_json(entry["chosen"]) == chosen
    assert entry["alternatives"], "alternatives are listed"
    for alt in entry["alternatives"]:
        assert set(alt) >= {"action", "value", "visits"}
        assert Action.from_json(alt["action"]) in st.pending.options
        assert isinstance(alt["value"], float) and isinstance(alt["visits"], int)
    json.dumps(log)


def test_mcts_log_ranks_by_visits_and_chooses_most_visited() -> None:
    agent = make_agent("mcts", seed=9, log=True)
    st = SITUATIONS[DecisionKind.MAIN]()
    assert st.pending is not None
    chosen = agent.choose(st, st.pending.player)
    alts = agent.decision_log()[0]["alternatives"]
    visits = [a["visits"] for a in alts]
    assert visits == sorted(visits, reverse=True)
    assert Action.from_json(alts[0]["action"]) == chosen
    assert sum(visits) > 0


def test_log_accumulates_over_a_game() -> None:
    agents = (make_agent("greedy", seed=1, log=True), make_agent("random", seed=2, log=True))
    st, decisions = run_game(agents, DECKS, 8)
    assert st.winner is not None
    logged = len(agents[0].decision_log()) + len(agents[1].decision_log())
    assert logged == decisions


def test_canonical_moves_merge_duplicates_and_prefer_fewer_ex() -> None:
    from gcg_sim.testkit import Scenario

    sc = Scenario()
    sc.resources(0, 1, ex=1)
    sc.hand(0, "ST01-005", "ST01-005")
    st = sc.start()
    assert st.pending is not None
    plays = [o for o in st.pending.options if o.kind.value == "play_unit"]
    assert len(plays) == 4  # two copies x (pay with EX or not)
    moves = [a for _, a in canonical(st, st.pending) if a.kind.value == "play_unit"]
    assert len(moves) == 1 and ex_used(moves[0]) == 0


@settings(max_examples=8, deadline=None)
@given(seed=hs.integers(min_value=0, max_value=2**32))
def test_random_and_greedy_finish_games_legally(seed: int) -> None:
    agents = (make_agent("greedy", seed=seed), make_agent("random", seed=seed + 1))
    st, _ = run_game(agents, DECKS, seed)
    assert st.winner in (-1, 0, 1)


def test_mcts_iteration_budget_is_deterministic_and_bounded() -> None:
    cfg = SearchConfig("tiny", max_iterations=12, min_iterations=4, iterations_per_option=2)
    st = SITUATIONS[DecisionKind.MAIN]()
    assert st.pending is not None
    agent = MctsAgent(7, config=cfg, log=True)
    agent.choose(st, st.pending.player)
    visits = sum(a["visits"] for a in agent.decision_log()[0]["alternatives"])
    n = len(canonical(st, st.pending))
    assert visits <= cfg.budget(n) <= cfg.max_iterations


@pytest.mark.parametrize("kind", ["greedy", "mcts"])
@pytest.mark.parametrize("decision", [DecisionKind.REDRAW, DecisionKind.CHOOSE_FIRST])
def test_pre_game_choices_log_their_options(kind: str, decision: DecisionKind) -> None:
    st = SITUATIONS[decision]()
    assert st.pending is not None
    agent = make_agent(kind, seed=4, log=True)
    agent.choose(st, st.pending.player)
    logged = {tuple(a["action"]) for a in agent.decision_log()[0]["alternatives"]}
    if kind == "greedy" and decision is DecisionKind.CHOOSE_FIRST:
        assert logged == set()  # greedy always plays first without simulating
    else:
        assert logged == {tuple(o.to_json()) for o in st.pending.options}


def _zero_ap_attacker(*, trick: bool) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 3)
    cgs = sc.add(0, "ST05-003")  # CGS Mobile Worker, AP 0, no attack trigger
    sc.add(1, "GD01-060")
    sc.shields(1, "GD01-060", "GD01-060")
    if trick:
        sc.add(0, "ST01-014", Zone.HAND)  # 【Main】/【Action】 Unforeseen Incident
    st = sc.start()
    return st, cgs


def test_zero_ap_attack_without_any_follow_up_is_pruned() -> None:
    st, cgs = _zero_ap_attacker(trick=False)
    dec = st.pending
    assert dec is not None and any(o.kind is ActionKind.ATTACK and o.a == cgs for o in dec.options)
    moves = prune_dominated(st, dec, canonical(st, dec))
    assert not any(a.kind is ActionKind.ATTACK for _, a in moves)
    assert moves


def test_zero_ap_attack_is_kept_when_an_action_step_trick_could_matter() -> None:
    st, cgs = _zero_ap_attacker(trick=True)
    dec = st.pending
    assert dec is not None
    moves = prune_dominated(st, dec, canonical(st, dec))
    assert any(a.kind is ActionKind.ATTACK and a.a == cgs for _, a in moves)


def test_a_free_add_to_hand_burst_is_always_accepted() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, "GD01-060")
    sc.shields(0, "ST05-010")  # 【Burst】Add this card to your hand.
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.BURST
    moves = prune_dominated(st, dec, canonical(st, dec))
    assert [a.kind for _, a in moves] == [ActionKind.YES]
    for seed in range(3):
        assert make_agent("mcts", seed=seed).choose(st, 0) == Action(ActionKind.YES)


def _after_attacks_with(command: str) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD01-060", rested=True)  # already attacked
    sc.add(1, "GD01-060")  # active enemy Unit without <Blocker>: nothing can battle it now
    sc.shields(1, "GD01-060", "GD01-060")
    card = sc.add(0, command, Zone.HAND)
    st = sc.start()
    return st, card


def test_a_command_that_changes_nothing_this_turn_is_recognised() -> None:
    st, card = _after_attacks_with("ST01-014")  # 【Main】/【Action】 AP-3 during this turn
    dec = st.pending
    assert dec is not None
    moves = canonical(st, dec)
    idle = without_effect(st, 0, dec, moves, seed=3)
    assert [a for key, a in moves if key in idle] == [
        a for _, a in moves if a.kind is ActionKind.PLAY_COMMAND and a.a == card
    ]
    assert make_agent("mcts", seed=1).choose(st, 0).kind is ActionKind.END_MAIN


def test_a_command_with_a_lasting_result_is_not_pruned() -> None:
    st, _ = _after_attacks_with("GD01-100")  # draws cards
    dec = st.pending
    assert dec is not None
    assert without_effect(st, 0, dec, canonical(st, dec), seed=3) == set()


def _jaburo(
    enemy_rested: bool, attacker: bool = False
) -> tuple[GameState, list[tuple[Key, Action]]]:
    sc = Scenario()
    sc.base(0, "GD04-122")  # Jaburo: Rest 1 of your (EF) Units: rest an enemy Lv.3 or lower
    sc.add(0, "GD01-008")  # Guntank (Earth Federation), the only Unit that can pay the cost
    if attacker:
        sc.add(0, "ST14-008")  # 6/4, not Earth Federation: can attack the rested enemy
    sc.add(1, "GD01-060", rested=enemy_rested)  # Lv.2
    sc.shields(1, "GD01-060", "GD01-060")
    st = sc.start()
    dec = st.pending
    assert dec is not None
    return st, canonical(st, dec)


def test_an_activation_whose_only_result_is_its_cost_is_recognised() -> None:
    st, moves = _jaburo(enemy_rested=True)
    assert st.pending is not None
    activations = {key for key, a in moves if a.kind is ActionKind.ACTIVATE}
    assert activations
    assert activations <= without_effect(st, 0, st.pending, moves, seed=5)


def test_an_activation_that_enables_an_attack_is_kept() -> None:
    st, moves = _jaburo(enemy_rested=False, attacker=True)
    assert st.pending is not None
    activations = {key for key, a in moves if a.kind is ActionKind.ACTIVATE}
    assert activations
    assert not activations & without_effect(st, 0, st.pending, moves, seed=5)
