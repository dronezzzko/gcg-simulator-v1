"""Every game record replays from its seed and action list to the same final state."""

from __future__ import annotations

import copy
from itertools import permutations

import pytest

from gcg_sim.deck import Deck
from gcg_sim.runner import (
    BenchmarkConfig,
    FactoryRef,
    GameRecord,
    replay,
    run_benchmark,
    state_digest,
)
from gcg_sim.runner.replayfile import ReplayError, check_replay, replay_document


def assert_replays(config: BenchmarkConfig, games: tuple[GameRecord, ...]) -> None:
    for g in games:
        st = replay(g, config)
        assert state_digest(st) == g.final_state_sha256
        assert st.turn == g.turns
        winner = st.winner
        assert winner is not None
        assert g.winner == ("draw" if winner < 0 else "dut" if winner == g.dut_seat else "bench")
        assert st.end_reason is not None
        assert st.end_reason.value == g.end_reason


def test_many_games_replay_to_the_recorded_final_state(
    federation: Deck, tekkadan: Deck, random_factory: FactoryRef
) -> None:
    config = BenchmarkConfig(tekkadan, federation, matches=40, seed=2024)
    run = run_benchmark(config, agent_factory=random_factory)
    assert len(run.games) >= 80
    assert_replays(config, run.games)


@pytest.mark.slow
def test_every_example_pairing_replays(
    federation: Deck, zeon: Deck, tekkadan: Deck, random_factory: FactoryRef
) -> None:
    for i, (a, b) in enumerate(permutations((federation, zeon, tekkadan), 2)):
        config = BenchmarkConfig(a, b, matches=60, seed=i, fmt="bo3")
        run = run_benchmark(config, agent_factory=random_factory)
        assert_replays(config, run.games)


def test_record_json_round_trip(config: BenchmarkConfig, random_factory: FactoryRef) -> None:
    run = run_benchmark(config, agent_factory=random_factory)
    for g in run.games:
        assert GameRecord.from_json(g.to_json()) == g


def test_replay_document_verifies_and_detects_tampering(
    config: BenchmarkConfig, random_factory: FactoryRef
) -> None:
    run = run_benchmark(config, agent_factory=random_factory)
    record = run.games[0]
    doc = replay_document(record, config, "win")
    check = check_replay(doc)
    assert check.ok
    assert check.mismatches == ()
    assert check.final_state_sha256 == record.final_state_sha256
    assert check.log[-1].endswith(f"({record.end_reason})")
    assert len(check.log) == len(record.actions) + 1

    truncated = copy.deepcopy(doc)
    truncated["actions"] = truncated["actions"][:-1]
    bad = check_replay(truncated)
    assert not bad.ok
    assert any(m.startswith("winner_seat: expected") for m in bad.mismatches)

    wrong_expectation = copy.deepcopy(doc)
    wrong_expectation["expected"]["turns"] += 1
    assert check_replay(wrong_expectation).mismatches == (
        f"turns: expected {record.turns + 1}, replay gave {record.turns}",
    )


@pytest.mark.parametrize(
    ("mutate", "message"),
    [
        (lambda d: d.update(format="other"), "not a gcg-sim-replay v1 document"),
        (lambda d: d.pop("seed"), "malformed replay document: KeyError"),
        (lambda d: d["actions"].insert(1, ["attack", 999, 999, -1]), "action 1 attack(999,999)"),
        (lambda d: d["actions"].append(["pass", -1, -1, -1]), "more actions than the game had"),
        (lambda d: d["actions"].append(["no-such-kind", 0, 0, 0]), "malformed replay document"),
    ],
)
def test_broken_replay_documents_raise_replay_error(
    config: BenchmarkConfig, random_factory: FactoryRef, mutate: object, message: str
) -> None:
    run = run_benchmark(config, agent_factory=random_factory)
    doc = replay_document(run.games[0], config, "loss")
    mutate(doc)  # type: ignore[operator]
    with pytest.raises(ReplayError, match=message.replace("(", r"\(").replace(")", r"\)")):
        check_replay(doc)
