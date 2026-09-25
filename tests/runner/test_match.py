"""Best-of-three sequencing (BO3 Match Rules, TRM 4.6/5.2) and single-game matches."""

from __future__ import annotations

import dataclasses

import pytest

from gcg_sim.engine.game import new_game
from gcg_sim.engine.types import DecisionKind, EndReason, Zone
from gcg_sim.runner import BenchmarkConfig, FactoryRef, GameRecord, game_seed, play_match
from gcg_sim.runner import match as match_module
from gcg_sim.runner.config import seat_decks
from gcg_sim.runner.game import match_loser_seat, match_point
from gcg_sim.runner.records import DutCardEvents

EMPTY_EVENTS = DutCardEvents((), False, (), (), (), (), ())


def fake_record(
    m: int, g: int, chooser: int | None, winner: str, point: str | None, reason: str
) -> GameRecord:
    return GameRecord(
        match_index=m,
        game_index=g,
        seed=0,
        agent_seeds=(0, 0),
        agents=("a", "b"),
        dut_seat=m % 2,
        chooser=chooser,
        chooser_seat=0 if chooser is None else chooser,
        first_player=0,
        dut_on_play=m % 2 == 0,
        winner=winner,
        end_reason=reason,
        match_point=point,
        turns=10,
        active_at_end=0,
        actions=(),
        redraws=(False, False),
        dut_cards=EMPTY_EVENTS,
        bench_seen=(),
        final_state_sha256="",
        turn_limit=200,
        max_actions=20000,
    )


def scripted(
    monkeypatch: pytest.MonkeyPatch, outcomes: list[tuple[str, str | None, str]]
) -> list[int | None]:
    """Make play_match see ``outcomes`` (winner, match point, end reason); returns choosers."""
    choosers: list[int | None] = []

    def fake_play_game(
        config: BenchmarkConfig, m: int, g: int, chooser: int | None, **_: object
    ) -> GameRecord:
        choosers.append(chooser)
        winner, point, reason = outcomes[g]
        return fake_record(m, g, chooser, winner, point, reason)

    monkeypatch.setattr(match_module, "play_game", fake_play_game)
    return choosers


def test_match_ends_when_a_player_has_two_wins(
    config: BenchmarkConfig, monkeypatch: pytest.MonkeyPatch
) -> None:
    choosers = scripted(monkeypatch, [("dut", "dut", "battle_damage")] * 3)
    record = play_match(config, 0)
    assert len(record.games) == 2
    assert (record.winner, record.dut_points, record.bench_points) == ("dut", 2, 0)
    assert choosers == [None, 1]  # DUT is seat 0 in match 0; the bench seat lost game 1


def test_simultaneous_defeat_scores_for_the_non_turn_player_in_bo3(
    config: BenchmarkConfig, monkeypatch: pytest.MonkeyPatch
) -> None:
    both = EndReason.BOTH_DEFEATED.value
    choosers = scripted(
        monkeypatch,
        [("draw", "bench", both), ("dut", "dut", "battle_damage"), ("dut", "dut", "deck_out")],
    )
    record = play_match(config, 1)
    assert [g.winner for g in record.games] == ["draw", "dut", "dut"]
    assert (record.winner, record.dut_points, record.bench_points) == ("dut", 2, 1)
    assert choosers == [None, 1, 0]  # match 1: DUT in seat 1 lost the drawn game on points


def test_turn_limit_draw_scores_nothing_and_next_chooser_is_random(
    config: BenchmarkConfig, monkeypatch: pytest.MonkeyPatch
) -> None:
    limit = EndReason.TURN_LIMIT.value
    choosers = scripted(
        monkeypatch,
        [("draw", None, limit), ("dut", "dut", "battle_damage"), ("bench", "bench", "deck_out")],
    )
    record = play_match(config, 0)
    assert len(record.games) == 3
    assert (record.winner, record.dut_points, record.bench_points) == ("draw", 1, 1)
    assert choosers == [None, None, 1]


def test_bo1_plays_a_single_game(config: BenchmarkConfig, monkeypatch: pytest.MonkeyPatch) -> None:
    both = EndReason.BOTH_DEFEATED.value
    scripted(monkeypatch, [("draw", None, both)])
    record = play_match(dataclasses.replace(config, fmt="bo1"), 0)
    assert len(record.games) == 1
    assert record.winner == "draw"


def test_match_point_rules(config: BenchmarkConfig) -> None:
    st = new_game(seat_decks(config, 0), 1)
    st.winner, st.end_reason = 1, EndReason.BATTLE_DAMAGE
    assert match_point(st, 0, "bo3") == "bench"
    st.winner, st.end_reason, st.active = -1, EndReason.BOTH_DEFEATED, 1
    assert match_point(st, 0, "bo3") == "dut"
    assert match_point(st, 1, "bo3") == "bench"
    assert match_point(st, 0, "bo1") is None
    st.end_reason = EndReason.TURN_LIMIT
    assert match_point(st, 0, "bo3") is None


def test_real_bo3_matches_follow_the_loser_chooses_rule(
    config: BenchmarkConfig, random_factory: FactoryRef
) -> None:
    for m in range(8):
        record = play_match(config, m, agent_factory=random_factory)
        assert 2 <= len(record.games) <= 3
        assert all(g.dut_seat == m % 2 for g in record.games)
        assert record.games[0].chooser is None
        for prev, game in zip(record.games, record.games[1:], strict=False):
            assert game.chooser == match_loser_seat(prev)
            assert game.chooser_seat == game.chooser
        points = [g.match_point for g in record.games]
        assert record.dut_points == points.count("dut")
        assert record.bench_points == points.count("bench")
        assert max(record.dut_points, record.bench_points) == 2
        assert record.winner == ("dut" if record.dut_points == 2 else "bench")


@pytest.mark.faq("Q9")
def test_game_one_player_one_is_chosen_by_a_seeded_random_winner_before_hands(
    config: BenchmarkConfig,
) -> None:
    choosers = set()
    for m in range(10):
        st = new_game(seat_decks(config, m), game_seed(config.seed, m, 0), chooser=None)
        assert st.pending is not None
        assert st.pending.kind is DecisionKind.CHOOSE_FIRST
        assert st.pending.player == st.setup_chooser
        assert not st.zones[0][Zone.HAND]
        assert not st.zones[1][Zone.HAND]
        choosers.add(st.setup_chooser)
    assert choosers == {0, 1}


def test_real_turn_limit_games_are_draws_without_match_points(
    config: BenchmarkConfig, random_factory: FactoryRef
) -> None:
    capped = dataclasses.replace(config, turn_limit=2)
    record = play_match(capped, 0, agent_factory=random_factory)
    assert [g.end_reason for g in record.games] == [EndReason.TURN_LIMIT.value] * 3
    assert [g.winner for g in record.games] == ["draw"] * 3
    assert [g.chooser for g in record.games] == [None, None, None]
    assert (record.winner, record.dut_points, record.bench_points) == ("draw", 0, 0)
