"""Search-side AI vectors of the Python reference for gcg-ai, gcg-ai-reference and gcg-search.

docs/dev/PLAN.md §8.1 ``export_ai_vectors.py`` (workstream D2 part). Writes
``crates/gcg-ai/tests/vectors/search/``:

- ``presets.json``: every field of every reference configuration (``PRESETS``,
  ``BASELINE_PRESETS``, ``TUNING_CONFIG``) as printed from the live objects, floats as
  ``float.hex()``, and the budgets for 1-30 root moves;
- ``opening.json``: ``opening_score`` of seeded five-card hands drawn from the example and AI
  test decks, both seats;
- ``decisions.ndjson``: one record per sampled decision of three games between reference
  agents at the ``fast`` preset (example decks). Each record holds the canonical snapshot of
  the true state before the decision (the shape of ``traces.py`` snapshot records, so that
  ``canon::from_canonical`` rebuilds it), the per-decision seed of the agent that decides, and
  what the Python functions return on that state: ``canonical``; ``prune_dominated`` for all
  four flag sets; ``merge_equivalent`` of the canonical moves; ``without_effect`` with the
  baseline flags and knobs and with every flag on and the adopted knobs; the mulligan (96 and
  48 samples) at REDRAW, the play/draw choice (8 games, adopted knobs) at CHOOSE_FIRST, and
  ``search`` at the other decisions for ``fast`` (always), ``standard``, ``strong`` and
  ``baseline-standard`` (sampled), each with its action, every alternative (value as
  ``float.hex()``), iterations and the generator state afterwards; and the decision-log entry
  the agent writes for its choice;
- ``baseline.json``: the ``tests/ai/test_baseline.py`` game (decks in order, seeds, the
  recorded outcome and payload digest), re-played here and checked against the test's
  constants.

The games are played by a driver that makes exactly ``MctsAgent.choose``'s dispatch
(``ai/agents.py:85-98``) with each seat's per-decision generator
``SplitMix64(derive_seed(seed, "mcts-fast", "decision", k))`` (``ai/base.py:60-65``), so the
``fast`` vectors are the moves played. Every function only reads the true state.

    uv run python oracle/export_ai_vectors.py [--check]
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import sys
from pathlib import Path
from typing import Any, cast

import traces

from gcg_sim.ai import mcts, noop, setup
from gcg_sim.ai.actions import Key, canonical, merge_equivalent, prune_dominated
from gcg_sim.ai.agents import make_agent
from gcg_sim.ai.base import AgentCore, Alternative, log_entry
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.ai.evaluation import DEFAULT_WEIGHTS, Weights
from gcg_sim.ai.policy import BASELINE_KNOBS, knobs_for
from gcg_sim.ai.tuning import TUNING_CONFIG
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import DecisionKind
from gcg_sim.rng import SplitMix64, derive_seed
from gcg_sim.runner.config import agent_seed, game_seed

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "gcg-ai" / "tests" / "vectors" / "search"
VECTOR_MASTER = 7
PRESET = "fast"
GAMES = tuple((a, b) for a in traces.EXAMPLE_DECKS for b in traces.EXAMPLE_DECKS if a != b)
MAX_RECORDS_PER_GAME = 24
SAMPLE_EVERY = 4
STANDARD_EVERY = 3
STRONG_RECORDS = 3
OPENING_HANDS = 120
BUDGET_OPTIONS = (1, 2, 3, 4, 5, 7, 10, 20, 30)
SPENDING = noop.SPENDING
TOP_LEVEL = noop.TOP_LEVEL
BASELINE_GAME_SEED = 4242
BASELINE_AGENT_SEED = 17
BASELINE_DIGEST = "5cff789d2a8bb26cfef4db98b1ef1ef8f37c3bf63210114781d357bd215f3b96"
BASELINE_OUTCOME = (1, 20, 115)

Json = Any


def key_json(key: Key) -> list[Json]:
    return list(key)


def alts_json(alts: list[Alternative]) -> list[Json]:
    return [[a.action.to_json(), traces.fhex(a.value), a.visits] for a in alts]


def config_json(c: SearchConfig) -> dict[str, Json]:
    out: dict[str, Json] = {}
    for f in dataclasses.fields(c):
        v = getattr(c, f.name)
        out[f.name] = traces.fhex(v) if isinstance(v, float) else v
    out["budgets"] = [c.budget(n) for n in BUDGET_OPTIONS]
    return out


def presets_json() -> dict[str, Json]:
    out: dict[str, Json] = {"budget_options": list(BUDGET_OPTIONS)}
    out["presets"] = {name: config_json(c) for name, c in PRESETS.items()}
    out["baseline_presets"] = {name: config_json(c) for name, c in BASELINE_PRESETS.items()}
    out["tuning"] = config_json(TUNING_CONFIG)
    out["agent_names"] = {
        name: cast(AgentCore, make_agent("mcts", preset=name)).name for name in PRESETS
    }
    return out


def opening_json() -> list[Json]:
    db = get_registry().db
    decks: list[DeckList] = [traces.example(name) for name in traces.EXAMPLE_DECKS]
    decks.extend(traces.test_decks())
    rng = SplitMix64(derive_seed(VECTOR_MASTER, "opening"))
    out: list[Json] = []
    for i in range(OPENING_HANDS):
        deck = decks[i % len(decks)]
        ids = sorted(db[n].def_id for n in deck.main)
        rng.shuffle(ids)
        hand = ids[:5]
        first = i % 2 == 0
        score = setup.opening_score([db.by_id(d) for d in hand], first)
        out.append({"hand": hand, "first": first, "score": traces.fhex(score)})
    return out


@dataclasses.dataclass
class Seat:
    seed: int
    name: str
    decisions: int = 0

    def next_rng(self) -> tuple[int, int]:
        self.decisions += 1
        return self.decisions, derive_seed(self.seed, self.name, "decision", self.decisions)


def search_json(
    st: GameState, player: int, config: SearchConfig, weights: Weights, seed: int, label: str
) -> tuple[dict[str, Json], mcts.SearchResult]:
    rng = SplitMix64(seed)
    res = mcts.search(st, player, config, weights, rng)
    return (
        {
            "config": label,
            "weights": weights.feature_set,
            "action": res.action.to_json(),
            "alts": alts_json(res.alternatives),
            "iterations": res.iterations,
            "rng_after": traces.u64(rng.state),
        },
        res,
    )


class Recorder:
    def __init__(self) -> None:
        self.records: list[str] = []
        self.searches = 0
        self.strong = 0

    def components(self, st: GameState, rec: dict[str, Json], seed: int) -> None:
        dec = st.pending
        assert dec is not None
        moves = canonical(st, dec)
        rec["canonical"] = [[key_json(k), a.to_json()] for k, a in moves]
        prune: dict[str, Json] = {}
        for strict in (False, True):
            for rest in (False, True):
                name = ("t" if strict else "f") + ("t" if rest else "f")
                kept = prune_dominated(st, dec, moves, strict_attacks=strict, rest_targets=rest)
                prune[name] = [key_json(k) for k, _ in kept]
        rec["prune"] = prune
        rec["merge"] = [key_json(k) for k, _ in merge_equivalent(st, dec, moves)]
        if dec.kind in TOP_LEVEL and any(a.kind in SPENDING for _, a in moves):
            noop_seed = SplitMix64(seed).next_u64()
            adopted = knobs_for(PRESETS[PRESET])
            idle = []
            for flags, knobs in ((False, BASELINE_KNOBS), (True, adopted)):
                keys = noop.without_effect(
                    st,
                    dec.player,
                    dec,
                    moves,
                    noop_seed,
                    knobs,
                    branch_choices=flags,
                    resource_aware=flags,
                    weights=DEFAULT_WEIGHTS,
                )
                idle.append(
                    {
                        "seed": traces.u64(noop_seed),
                        "flags": flags,
                        "knobs": "adopted" if flags else "baseline",
                        "keys": sorted(key_json(k) for k in keys),
                    }
                )
            rec["idle"] = idle

    def decide(
        self, st: GameState, seat: Seat, game: int, i: int, sampled: bool
    ) -> tuple[Action, list[Alternative]]:
        dec = st.pending
        assert dec is not None
        p = dec.player
        k, seed = seat.next_rng()
        config = PRESETS[PRESET]
        if len(dec.options) == 1:
            return dec.options[0], []
        rec: dict[str, Json] = {}
        if sampled:
            rec = {
                "id": f"g{game}/d{i}",
                "player": p,
                "kind": dec.kind.value,
                "agent": seat.name,
                "agent_seed": traces.u64(seat.seed),
                "k": k,
                "seed": traces.u64(seed),
                "snapshot": {
                    key: value
                    for key, value in traces.snapshot_record(st, i).items()
                    if key not in ("t", "i")
                },
            }
            self.components(st, rec, seed)
        rng = SplitMix64(seed)
        if dec.kind is DecisionKind.REDRAW:
            action, alts = setup.mulligan_choice(st, p, config.mulligan_samples, rng)
            if sampled:
                mull = []
                for samples in (config.mulligan_samples, 48):
                    r = SplitMix64(seed)
                    a, s = setup.mulligan_choice(st, p, samples, r)
                    mull.append(
                        {
                            "samples": samples,
                            "action": a.to_json(),
                            "alts": alts_json(s),
                            "rng_after": traces.u64(r.state),
                        }
                    )
                rec["mulligan"] = mull
        elif dec.kind is DecisionKind.CHOOSE_FIRST:
            action, alts = setup.first_player_choice(
                st, p, config.first_player_games, rng, knobs_for(config)
            )
            if sampled:
                rec["first_player"] = {
                    "games": config.first_player_games,
                    "knobs": "adopted",
                    "action": action.to_json(),
                    "alts": alts_json(alts),
                    "rng_after": traces.u64(rng.state),
                }
        else:
            res = mcts.search(st, p, config, DEFAULT_WEIGHTS, rng)
            action, alts = res.action, res.alternatives
            if sampled:
                searches: list[Json] = [
                    {
                        "config": PRESET,
                        "weights": DEFAULT_WEIGHTS.feature_set,
                        "action": res.action.to_json(),
                        "alts": alts_json(res.alternatives),
                        "iterations": res.iterations,
                        "rng_after": traces.u64(rng.state),
                    }
                ]
                n = self.searches
                self.searches += 1
                if n % STANDARD_EVERY == 0:
                    searches.append(
                        search_json(st, p, PRESETS["standard"], DEFAULT_WEIGHTS, seed, "standard")[
                            0
                        ]
                    )
                if n % STANDARD_EVERY == 2:
                    searches.append(
                        search_json(
                            st,
                            p,
                            BASELINE_PRESETS["standard"],
                            BASELINE_WEIGHTS,
                            seed,
                            "baseline-standard",
                        )[0]
                    )
                if n % STANDARD_EVERY == 1 and self.strong < STRONG_RECORDS:
                    self.strong += 1
                    searches.append(
                        search_json(st, p, PRESETS["strong"], DEFAULT_WEIGHTS, seed, "strong")[0]
                    )
                rec["search"] = searches
        if sampled:
            rec["chosen"] = action.to_json()
            rec["rng_after"] = traces.u64(rng.state)
            entry = log_entry(st, p, action, alts)
            rec["log"] = entry
            rec["log_text"] = json.dumps(entry, sort_keys=True, separators=(",", ":"))
            self.records.append(traces.dumps_line(rec))
        return action, alts


def reduces(st: GameState) -> bool:
    """Pruning or merging drops a move of this decision."""
    dec = st.pending
    assert dec is not None
    moves = canonical(st, dec)
    kept = prune_dominated(st, dec, moves, strict_attacks=True, rest_targets=True)
    return len(kept) < len(moves) or len(merge_equivalent(st, dec, moves)) < len(moves)


def play_game(rec: Recorder, game: int, dut: str, bench: str) -> None:
    decks = (traces.example(dut), traces.example(bench))
    st = new_game(decks, game_seed(VECTOR_MASTER, game, 0))
    name = f"mcts-{PRESET}"
    seats = (
        Seat(agent_seed(VECTOR_MASTER, game, 0, 0), name),
        Seat(agent_seed(VECTOR_MASTER, game, 0, 1), name),
    )
    taken = 0
    multi = 0
    i = 0
    while st.pending is not None:
        dec = st.pending
        sampled = False
        if len(dec.options) > 1 and taken < MAX_RECORDS_PER_GAME:
            spends = any(a.kind in SPENDING for a in dec.options)
            if spends or dec.kind is not DecisionKind.MAIN or reduces(st):
                sampled = True
            else:
                sampled = multi % SAMPLE_EVERY == 0
                multi += 1
        if sampled:
            taken += 1
        action, _ = rec.decide(st, seats[dec.player], game, i, sampled)
        apply(st, action)
        i += 1


def baseline_json() -> dict[str, Json]:
    decks = traces.test_decks()
    agents = tuple(
        cast(
            AgentCore,
            make_agent(
                "mcts",
                seed=BASELINE_AGENT_SEED + seat,
                log=True,
                config=BASELINE_PRESETS["standard"],
                weights=BASELINE_WEIGHTS,
            ),
        )
        for seat in (0, 1)
    )
    st = new_game(decks, BASELINE_GAME_SEED)
    actions = []
    while st.pending is not None:
        p = st.pending.player
        action = cast(Any, agents[p]).choose(st, p)
        actions.append(action.to_json())
        apply(st, action)
    payload = {
        "actions": actions,
        "logs": [agent.decision_log() for agent in agents],
        "winner": st.winner,
        "turn": st.turn,
    }
    digest = hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()
    outcome = (st.winner, st.turn, len(actions))
    if digest != BASELINE_DIGEST or outcome != BASELINE_OUTCOME:
        raise AssertionError(f"baseline game drifted: {outcome} {digest}")
    return {
        "decks": [traces.deck_json(d) for d in decks],
        "game_seed": BASELINE_GAME_SEED,
        "agent_seeds": [BASELINE_AGENT_SEED, BASELINE_AGENT_SEED + 1],
        "config": "baseline-standard",
        "weights": BASELINE_WEIGHTS.feature_set,
        "winner": outcome[0],
        "turn": outcome[1],
        "actions": outcome[2],
        "digest": digest,
    }


def build() -> dict[str, str]:
    rec = Recorder()
    for game, (dut, bench) in enumerate(GAMES):
        play_game(rec, game, dut, bench)
    return {
        "presets.json": pretty(presets_json()),
        "opening.json": pretty(opening_json()),
        "decisions.ndjson": "".join(line + "\n" for line in rec.records),
        "baseline.json": pretty(baseline_json()),
    }


def pretty(obj: Json) -> str:
    return json.dumps(obj, indent=1, sort_keys=True) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if the committed files are stale")
    args = ap.parse_args()
    files = build()
    if args.check:
        stale = [
            n
            for n, text in files.items()
            if not (OUT / n).exists() or (OUT / n).read_text(encoding="utf-8") != text
        ]
        if stale:
            print(f"stale in {OUT}: {', '.join(stale)}; run without --check", file=sys.stderr)
            return 1
        print(f"{OUT} is current")
        return 0
    OUT.mkdir(parents=True, exist_ok=True)
    for name, text in files.items():
        (OUT / name).write_text(text, encoding="utf-8")
    print(f"wrote {', '.join(files)} to {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
