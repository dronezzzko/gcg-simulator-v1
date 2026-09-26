"""Per-card instrumentation observes the real game, consistently with the action stream."""

from __future__ import annotations

import dataclasses
from collections import Counter
from typing import Any

import pytest

from gcg_sim.deck import Deck
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind
from gcg_sim.rng import SplitMix64
from gcg_sim.runner import (
    AgentSpec,
    BenchmarkConfig,
    FactoryRef,
    GameFailure,
    GameRecord,
    game_seed,
    play_game,
)
from gcg_sim.runner.config import own_turn, seat_decks
from gcg_sim.runner.instrument import PLAY_KINDS, card_number


def contained(small: Counter[str], big: Counter[str]) -> bool:
    return all(big[k] >= v for k, v in small.items())


def played_by_actions(
    record: GameRecord, config: BenchmarkConfig
) -> tuple[Counter[str], Counter[str]]:
    """Cards played from hand by (DUT, benchmark), recomputed from the action stream alone."""
    st = new_game(seat_decks(config, record.match_index), record.seed, chooser=record.chooser)
    dut, bench = Counter[str](), Counter[str]()
    for raw in record.actions:
        action = Action.from_json(list(raw))
        assert st.pending is not None
        if action.kind in PLAY_KINDS:
            side = dut if st.pending.player == record.dut_seat else bench
            side[card_number(st, action.a)] += 1
        apply(st, action)
    return dut, bench


@pytest.fixture(scope="module")
def games(
    federation: Deck, zeon: Deck, random_factory: FactoryRef
) -> list[tuple[BenchmarkConfig, GameRecord]]:
    config = BenchmarkConfig(federation, zeon, matches=1, seed=99)
    return [
        (config, play_game(config, m, 0, None, agent_factory=random_factory)) for m in range(24)
    ]


def test_hands_and_redraws(games: list[tuple[BenchmarkConfig, GameRecord]]) -> None:
    for _, g in games:
        c = g.dut_cards
        assert len(c.initial_hand) == 5
        assert len(c.opening_hand) == 5
        assert c.redrew == g.redraws[g.dut_seat]
        if not c.redrew:
            assert c.initial_hand == c.opening_hand
    assert any(g.dut_cards.redrew for _, g in games)
    assert not all(g.dut_cards.redrew for _, g in games)


def test_drawn_cards_are_consistent(games: list[tuple[BenchmarkConfig, GameRecord]]) -> None:
    for config, g in games:
        c = g.dut_cards
        deck = Counter(config.deck_under_test.main)
        drawn = Counter(c.drawn)
        assert contained(Counter(c.opening_hand), drawn)
        assert contained(drawn, deck)
        assert contained(Counter(c.never_played), drawn)
        assert contained(Counter(c.in_hand_at_end), drawn)
        turns_started = own_turn(g.turns, g.dut_seat, g.first_player)
        assert len(c.drawn) >= 5 + turns_started - 1


def test_played_cards_match_the_action_stream(
    games: list[tuple[BenchmarkConfig, GameRecord]],
) -> None:
    for config, g in games:
        dut_played, bench_played = played_by_actions(g, config)
        assert Counter(p.card_number for p in g.dut_cards.played) == dut_played
        assert set(bench_played) <= set(g.bench_seen)
        assert set(g.bench_seen) <= set(config.benchmark_deck.main)
        for p in g.dut_cards.played:
            assert 1 <= p.turn <= g.turns
            assert 0 <= p.own_turn <= own_turn(g.turns, g.dut_seat, g.first_player)
        never = Counter(g.dut_cards.never_played)
        drawn = Counter(g.dut_cards.drawn)
        assert sum(never.values()) <= sum(drawn.values())


class SearchingRandomAgent:
    """Chooses exactly like the test random agent, after searching determinized copies."""

    def __init__(self, seed: int) -> None:
        self.name = "searching-random"
        self._rng = SplitMix64(seed)
        self._search = SplitMix64(seed ^ 0xABCDEF)

    def choose(self, st: GameState, player: int) -> Action:
        assert st.pending is not None
        copy = determinize(st, player, self._search.next_u64())
        for _ in range(40):
            if copy.pending is None:
                break
            opts = copy.pending.options
            apply(copy, opts[self._search.randrange(len(opts))])
        options = st.pending.options
        return options[self._rng.randrange(len(options))]

    def decision_log(self) -> list[dict[str, Any]]:
        return []


def test_search_copies_do_not_leak_into_the_records(
    federation: Deck, zeon: Deck, random_factory: FactoryRef
) -> None:
    config = BenchmarkConfig(federation, zeon, matches=1, seed=5)
    for m in range(4):
        plain = play_game(config, m, 0, None, agent_factory=random_factory)
        searched = play_game(
            config, m, 0, None, agent_factory=lambda spec: SearchingRandomAgent(spec.seed)
        )
        assert dataclasses.replace(searched, agents=plain.agents) == plain


class MutatingAgent:
    name = "mutating"

    def choose(self, st: GameState, player: int) -> Action:
        assert st.pending is not None
        apply(st, st.pending.options[0])
        assert st.pending is not None
        return st.pending.options[0]

    def decision_log(self) -> list[dict[str, Any]]:
        return []


class IllegalAgent:
    name = "illegal"

    def choose(self, st: GameState, player: int) -> Action:
        return Action(ActionKind.ATTACK, 9999, 9999)

    def decision_log(self) -> list[dict[str, Any]]:
        return []


def test_an_agent_that_mutates_the_real_game_is_rejected(config: BenchmarkConfig) -> None:
    with pytest.raises(GameFailure, match="agent 'mutating' mutated the game state"):
        play_game(config, 0, 0, None, agent_factory=lambda spec: MutatingAgent())


def test_illegal_agent_actions_fail_with_game_context(config: BenchmarkConfig) -> None:
    seed = game_seed(config.seed, 2, 1)
    with pytest.raises(GameFailure, match=rf"^match 2 game 2 \(seed {seed}\) failed: Illegal"):
        play_game(config, 2, 1, None, agent_factory=lambda spec: IllegalAgent())


def test_decision_log_is_collected_in_decision_order(
    config: BenchmarkConfig, random_factory: FactoryRef
) -> None:
    logged = dataclasses.replace(config, decision_log=True)
    record = play_game(logged, 0, 0, None, agent_factory=random_factory)
    assert record.decision_log is not None
    assert len(record.decision_log) == len(record.actions)
    assert [e["chosen"] for e in record.decision_log] == [list(a) for a in record.actions]
    assert play_game(config, 0, 0, None, agent_factory=random_factory).decision_log is None


def test_agents_receive_their_role_seat_and_seed(config: BenchmarkConfig) -> None:
    specs: list[AgentSpec] = []

    def factory(spec: AgentSpec) -> IllegalAgent:
        specs.append(spec)
        return IllegalAgent()

    with pytest.raises(GameFailure):
        play_game(config, 1, 0, None, agent_factory=factory)
    assert [(s.role, s.seat) for s in specs] == [("bench", 0), ("dut", 1)]
    assert specs[0].seed != specs[1].seed
    assert {s.preset for s in specs} == {"standard"}
