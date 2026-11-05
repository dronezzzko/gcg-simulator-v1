"""Export the playout-policy and evaluation vectors for the Rust tests (workstream D1).

docs/dev/PLAN.md §8.1 (`components`), §10.4 D1.
Writes crates/gcg-ai/tests/vectors/policy/:

- ``pure.ndjson``: ``pick`` (seeded Gumbel-max and argmax, with the generator state after,
  including a draw of exactly ``0.0``) and ``logistic``.
- ``registry.ndjson``: per ``(program, pc)`` of every ``Choose`` the polarity with and without
  the rule table, the battle-only rule flag and the class of the first step; per definition
  ``card_worth`` and ``command_harms_enemy_units``.
- ``states-NNN.ndjson``: decision states sampled from the tier-1 game traces (``full``,
  ``baselines``, ``coverage``, ``fuzz``, ``search``), replayed here from their headers and
  recorded choices. Each record holds the state's canonical snapshot with its side tables and
  ``state_digest``, then ``option_scores`` for the baseline and the adopted knobs, ``choose``
  with ``rng=None`` and seeded, ``burst_density`` for both observers, ``features``,
  ``role_features``, ``score`` and ``evaluate`` for both players, the choice polarity, a
  ``RandomAgent`` decision and, for every fourth record, a ``GreedyAgent`` decision.
  ``terminal`` records hold finished games (``evaluate`` is exact there).
- ``opening.ndjson``: ``opening_score`` of sampled hands.
- ``games.ndjson``: the game-level vectors (recorded items 3-7) over the AI test decks.

Every state is computed on a clone whose cached view is cleared, so each value is the one a
freshly loaded state gives (``canon::from_canonical`` starts with an empty cache, PLAN §4.8).
Floats are ``float.hex()`` strings, u64 values of 2**53 and above decimal strings, actions
``[kind, a, b, c]``; lines are compact JSON with sorted keys (``traces.dumps_line``).

    uv run python oracle/export_policy_vectors.py [--check]
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import sys
from collections import Counter
from collections.abc import Iterator, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest
import traces as T

import gcg_sim.testkit as TK
from gcg_sim.ai import evaluation as E
from gcg_sim.ai import policy as P
from gcg_sim.ai import targeting
from gcg_sim.ai.agents import GreedyAgent, RandomAgent
from gcg_sim.ai.base import Alternative
from gcg_sim.ai.config import PRESETS
from gcg_sim.ai.playout import play_to_end, play_until_turn
from gcg_sim.ai.selfplay import run_game
from gcg_sim.ai.setup import opening_score
from gcg_sim.ai.values import card_worth, exposed
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import game as G
from gcg_sim.engine import view as V
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Duration, Zone
from gcg_sim.rng import SplitMix64, derive_seed
from gcg_sim.runner.digest import content_id, state_digest
from gcg_sim.testkit import Scenario

Json = Any

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "gcg-ai" / "tests" / "vectors" / "policy"
MASTER = 20261101
SOURCES = ("full", "baselines", "coverage", "fuzz", "search")
STATES_PER_FILE = 100
SIGNATURE_REPEATS = 2
STRIDE = 23
COMMON_KINDS = ("main", "select")
KIND_CAPS = {
    "main": 110,
    "select": 70,
    "action_step": 30,
    "block": 30,
    "burst": 24,
    "yes_no": 18,
    "arrange": 16,
    "excess": 14,
    "discard": 16,
    "order_trigger": 12,
    "choose_first": 8,
    "redraw": 12,
}
GREEDY_EVERY = 4
EXTRA_CAP = 40
# A cheap removal Command (`command_harms_enemy_units`): "Choose 1 to 2 enemy Units with 2 or
# less HP. Rest them."
REMOVAL = "EB01-075"
# Reference tests whose states reach scorer branches the game traces never do: the rule-table
# and 0-AP knobs (MAIN, SELECT), "next card (top first)" and "top or bottom" arrangements, and
# the "rest instead" substitute prompt (`ai/policy.py:334-358`).
TEST_NODES = (
    "tests/ai/test_policy_knobs.py::test_rule_polarity_table_locks_down_the_most_valuable_enemy",
    "tests/ai/test_policy_knobs.py::test_rule_polarity_table_protects_the_unit_in_the_battle",
    "tests/ai/test_policy_knobs.py::test_zero_ap_face_attack_scores_below_ending_the_main_phase",
    "tests/ai/test_policy_knobs.py"
    "::test_zero_ap_face_attack_keeps_its_score_when_a_trick_could_make_it_matter",
    "tests/ai/test_policy_knobs.py::test_zero_ap_attacker_does_not_count_against_the_blockers",
    "tests/cards/test_wp_gd02_b.py"
    "::test_gd02_104_keeps_one_on_top_bottoms_rest_then_draws_with_newtype_pilot",
    "tests/cards/test_wp_gd03_a.py::test_gd03_079_rest_instead_of_the_base",
    "tests/cards/test_wp_gd05_b.py::test_gd05_124_rest_this_base_instead_of_a_unit",
    "tests/cards/test_wp_st09_11.py"
    "::test_st09_001_returns_itself_and_deploys_impulse_without_paying_its_cost",
)
IR = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
TERMINAL_EVERY = 8
BASE_TEMPERATURE = 0.5
ADOPTED_TEMPERATURE = 0.3
ADOPTED = P.knobs_for(PRESETS["standard"])
GOLDEN_GAMMA = 0x9E3779B97F4A7C15
MASK = (1 << 64) - 1
# The recorded game digests, reproduced here before anything is written.
RANDOM_GAME_DIGEST = "ed729d1efd8bd75a513fb2a89eba83ced433d8d4c2ca86fa5342f22999aa45f3"
COVER_GAMES_DIGEST = "18b42cb9d7917ddcbe4af568dc842d779bbc67ad396f29a146c6793df95e856e"
GREEDY_GAME_DIGEST = "9241c6142b26faadd6d1b782c7f4aed57f259331a9090fb35090971eb2d67b62"


def fx(x: float) -> str:
    return float.hex(x)


def fxs(xs: Sequence[float]) -> list[str]:
    return [float.hex(x) for x in xs]


def aj(a: Action) -> list[Json]:
    return a.to_json()


def fresh(st: GameState) -> GameState:
    """A clone with no cached view (the state ``canon::from_canonical`` rebuilds)."""
    s = st.clone()
    s._derived = None
    return s


def snapshot(st: GameState) -> dict[str, Json]:
    rec = T.snapshot_record(st, 0)
    return {k: rec[k] for k in ("state", "continuous", "filter_sets", "rules_checked")}


# ---------------------------------------------------------------------------------------------
# pure vectors


def _unmix(z: int) -> int:
    """The SplitMix64 state whose output is ``z`` (the finaliser is a bijection)."""

    def unxorshift(x: int, s: int) -> int:
        y = x
        for _ in range(64 // s + 1):
            y = x ^ (y >> s)
        return y & MASK

    z = unxorshift(z, 31)
    z = (z * pow(0x94D049BB133111EB, -1, 1 << 64)) & MASK
    z = unxorshift(z, 27)
    z = (z * pow(0xBF58476D1CE4E5B9, -1, 1 << 64)) & MASK
    return unxorshift(z, 30)


def pick_case(scores: list[float], seed: int | None, temperature: float) -> dict[str, Json]:
    opts = [Action(ActionKind.PASS, i) for i in range(len(scores))]
    rng = None if seed is None else SplitMix64(seed)
    chosen = P.pick(opts, scores, rng, temperature)
    return {
        "t": "pick",
        "scores": fxs(scores),
        "seed": None if seed is None else T.u64(seed),
        "temperature": fx(temperature),
        "index": chosen.a,
        "rng_after": None if rng is None else T.u64(rng.state),
    }


def pure_records() -> list[dict[str, Json]]:
    out = [
        pick_case([1.0, 2.0, 0.5, 2.0], 42, 0.5),
        pick_case([1.0, 2.0, 0.5, 2.0], None, 0.5),
    ]
    assert out[0]["index"] == 3
    assert out[0]["rng_after"] == T.u64(8709371129873690750)
    assert out[1]["index"] == 1
    # A first draw of exactly 0.0 (``rng.random() or 1e-300``): next_u64() == 0.
    zero_seed = (_unmix(0) - GOLDEN_GAMMA) & MASK
    assert SplitMix64(zero_seed).random() == 0.0
    out.append(pick_case([0.0, -3.0, 2.0], zero_seed, 0.5))
    out.append(pick_case([5.0, 0.0], zero_seed, 1.0))
    gen = SplitMix64(derive_seed(MASTER, "pick"))
    pool = (0.0, -0.0, 1.0, -1.0, 2.5, 60.0, -0.25, 0.1, 4.0, 1.8000000000000003, 100.0)
    temperatures = (0.5, 0.3, 1.0, 1.5, 0.05, 0.0, -1.0)
    for _ in range(400):
        n = 1 + gen.randrange(12)
        scores = [
            pool[gen.randrange(len(pool))] if gen.randrange(2) else gen.random() * 20.0 - 10.0
            for _ in range(n)
        ]
        temperature = temperatures[gen.randrange(len(temperatures))]
        seed = None if gen.randrange(6) == 0 else gen.next_u64()
        out.append(pick_case(scores, seed, temperature))
    zs = [0.0, -0.0, 2.0, -2.0, 0.3, -37.5, 800.0, -800.0, 1e-300, -1e-300, 709.0, -709.0]
    zs += [745.5, -745.5, float("inf"), float("-inf"), 36.0, -36.0, 1e-17, -1e-17]
    zs += [gen.random() * 80.0 - 40.0 for _ in range(200)]
    zs += [(gen.random() * 2.0 - 1.0) * 1e-5 for _ in range(50)]
    for z in zs:
        out.append({"t": "logistic", "z": fx(z), "value": fx(E.logistic(z))})
    return out


# ---------------------------------------------------------------------------------------------
# registry vectors


def registry_records() -> list[dict[str, Json]]:
    reg = get_registry()
    out: list[dict[str, Json]] = []
    chooses = 0
    for prog in reg.programs:
        cid = content_id(prog)
        for pc, ins in enumerate(prog.instrs):
            if not isinstance(ins, d.Choose):
                continue
            chooses += 1
            step = targeting._first_step(prog.instrs, pc)
            out.append(
                {
                    "t": "choose",
                    "program": cid,
                    "pc": pc,
                    "polarity": targeting._polarity_at(prog.instrs, pc, False),
                    "polarity_table": targeting._polarity_at(prog.instrs, pc, True),
                    "battle": _lasts_this_battle(step),
                    "first_step": None if step is None else type(step).__name__,
                }
            )
    harms = 0
    for def_id in range(len(reg.cards)):
        cd = reg.db.by_id(def_id)
        h = targeting.command_harms_enemy_units(def_id)
        harms += h
        out.append(
            {
                "t": "card",
                "def_id": def_id,
                "card_number": cd.card_number,
                "worth": fx(card_worth(cd)),
                "harms": h,
            }
        )
    out.append(
        {
            "t": "counts",
            "programs": len(reg.programs),
            "chooses": chooses,
            "cards": len(reg.cards),
            "harms": harms,
        }
    )
    return out


def _lasts_this_battle(step: object) -> bool:
    """``choice_rule_lasts_this_battle``'s cached value for a first step
    (``ai/targeting.py:84-88``)."""
    return (
        isinstance(step, d.Apply)
        and isinstance(step.effect, d.RuleGrant)
        and step.duration is Duration.THIS_BATTLE
    )


# ---------------------------------------------------------------------------------------------
# sampled states


class TracedRandom(RandomAgent):
    """``RandomAgent`` that keeps its per-decision generator (an observer only)."""

    rng: SplitMix64 | None = None

    def _begin(self, st: GameState, player: int) -> tuple[Decision, SplitMix64]:
        dec, rng = super()._begin(st, player)
        self.rng = rng
        return dec, rng


class TracedGreedy(GreedyAgent):
    """``GreedyAgent`` that keeps its per-decision generator and the list it logs."""

    rng: SplitMix64 | None = None
    alternatives: list[Alternative] | None = None

    def _begin(self, st: GameState, player: int) -> tuple[Decision, SplitMix64]:
        dec, rng = super()._begin(st, player)
        self.rng = rng
        return dec, rng

    def _finish(
        self, st: GameState, player: int, chosen: Action, alternatives: list[Alternative]
    ) -> Action:
        self.alternatives = list(alternatives)
        return super()._finish(st, player, chosen, alternatives)


@dataclass
class Sample:
    id: str
    state: GameState  # a fresh clone, never applied to
    terminal: bool


def trace_files() -> list[Path]:
    files: list[Path] = []
    for name in SOURCES:
        found = sorted((T.TIER_DIRS[1] / name).glob("*.ndjson"))
        files += found
    return files


def signature(st: GameState, dec: Decision) -> tuple[Json, ...]:
    """What makes a decision state a new case for the scorers (the sampler keeps the first
    ``SIGNATURE_REPEATS`` states of each)."""
    kind = dec.kind
    if kind is DecisionKind.SELECT:
        return (
            kind.value,
            dec.ctx("mode") == 1,
            dec.ctx("rest_sub") >= 0,
            targeting.choice_polarity(st, False),
            targeting.choice_polarity(st, True),
            targeting.choice_rule_lasts_this_battle(st),
        )
    if kind is DecisionKind.ARRANGE:
        return (kind.value, dec.ctx("order") == 1, dec.ctx("card") < 0)
    if kind is DecisionKind.BLOCK:
        b = st.battle
        at_player = b is not None and (b.target < 0 or st.cards[b.target].zone is Zone.BASE)
        return (kind.value, at_player, exposed(st, dec.player), len(dec.options))
    if kind is DecisionKind.MAIN:
        s = fresh(st)
        differs = P.option_scores(s, dec) != P.option_scores(s, dec, ADOPTED)
        board = P._board(s, V.derived(s), dec)
        removal = any(
            o.kind is ActionKind.PLAY_COMMAND
            and targeting.command_harms_enemy_units(s.cards[o.a].def_id)
            for o in dec.options
        )
        return (
            kind.value,
            tuple(sorted({o.kind.value for o in dec.options})),
            differs,
            board.enemy_exposed,
            board.blocked_from_lethal,
            bool(board.enemy_blockers),
            removal,
        )
    return (kind.value, len(dec.options) > 1)


def sample_states() -> list[Sample]:
    """The sampled decision states and terminal states, in a fixed order."""
    seen: Counter[tuple[Json, ...]] = Counter()
    taken: Counter[str] = Counter()
    out: list[Sample] = []
    trace_no = 0
    for path in trace_files():
        for trace in T.read_traces(path):
            head = trace[0]
            if head["seed"] is None:
                continue
            trace_no += 1
            st = T.start_state(trace)
            multi = 0
            for rec in trace:
                if rec["t"] != "decision":
                    continue
                dec = st.pending
                assert dec is not None
                assert dec.kind.value == rec["kind"]
                kind = dec.kind.value
                several = len(dec.options) > 1
                if several:
                    multi += 1
                cap = KIND_CAPS.get(kind, 0)
                if taken[kind] < cap:
                    sig = signature(st, dec)
                    strided = several and (multi + trace_no) % STRIDE == 0
                    rare = several and kind not in COMMON_KINDS
                    if seen[sig] < SIGNATURE_REPEATS or strided or rare:
                        seen[sig] += 1
                        s = fresh(st)
                        snap = snapshot(s)
                        if not snap["filter_sets"]:
                            assert state_digest(s)[:16] == rec["digest16"], head["id"]
                            taken[kind] += 1
                            out.append(Sample(f"{head['id']}@{rec['i']}", s, False))
                apply(st, Action.from_json(rec["chosen"]))
            if trace_no % TERMINAL_EVERY == 0 and st.pending is None:
                s = fresh(st)
                if not snapshot(s)["filter_sets"]:
                    out.append(Sample(f"{head['id']}@end", s, True))
    extra = [*scenario_states(), *test_states()]
    kept = 0
    for name, s in extra:
        dec = s.pending
        assert dec is not None
        sig = signature(s, dec)
        snap = snapshot(s)
        if seen[sig] >= SIGNATURE_REPEATS or kept >= EXTRA_CAP or snap["filter_sets"]:
            continue
        if not programs_in_ir(snap["state"]):
            continue
        seen[sig] += 1
        kept += 1
        out.append(Sample(name, s, False))
    return out


def programs_in_ir(canon: dict[str, Json]) -> bool:
    """Whether every program the canonical state names (frames, then position 0 of each
    pending and batched trigger) is in the effect IR: a test may register a program of its
    own (``effects/registry.py:115-122``), which ``canon::from_canonical`` cannot resolve."""
    known = {p["content_id"] for p in json.loads(IR.read_text())["programs"]}
    names = [f["program_id"] for f in canon["frames"]]
    names += [t[0] for t in canon["pending_triggers"]]
    names += [t[0] for batch in canon["batches"] for t in batch]
    return all(n in known for n in names)


def scenario_states() -> list[tuple[str, GameState]]:
    """Main-phase boards the traces never reach (``ai/policy.py:179-180``, ``:236-238``): the
    enemy is exposed with one ready Blocker, and a removal Command is in hand."""
    out: list[tuple[str, GameState]] = []
    for attackers in (1, 2):
        sc = Scenario()
        sc.resources(0, 4)
        sc.hand(0, REMOVAL)
        for _ in range(attackers):
            sc.add(0, "ST01-005")  # GM 2/2
        sc.add(1, "ST01-008")  # Demi Trainer <Blocker>
        out.append((f"scenario/exposed-blocker/{attackers}-attackers", fresh(sc.start())))
    return out


def test_states() -> list[tuple[str, GameState]]:
    """Decision states of a few reference tests (``TEST_NODES``), captured transparently: the
    pending decision before every ``apply`` and at every ``policy.option_scores`` call on a
    state's own pending decision. The tests run in this process, unchanged."""
    captured: list[tuple[str, GameState]] = []
    current = ["?"]
    orig_apply = G.apply
    orig_scores = P.option_scores

    def capture(st: GameState) -> None:
        if st.pending is not None:
            captured.append((f"{current[0]}#{len(captured)}", fresh(st)))

    def traced_apply(st: GameState, action: Action, *, check: bool = True) -> None:
        capture(st)
        orig_apply(st, action, check=check)

    def traced_scores(
        st: GameState, dec: Decision, knobs: P.PolicyKnobs = P.BASELINE_KNOBS
    ) -> list[float]:
        if dec is st.pending:
            capture(st)
        return orig_scores(st, dec, knobs)

    class Names:
        @staticmethod
        def pytest_runtest_setup(item: pytest.Item) -> None:
            current[0] = item.nodeid

    patched: list[tuple[object, str, object]] = [
        (G, "apply", traced_apply),
        (TK, "apply", traced_apply),
        (P, "option_scores", traced_scores),
    ]
    saved = [(m, n, getattr(m, n)) for m, n, _ in patched]
    for m, n, v in patched:
        setattr(m, n, v)
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            code = pytest.main([*TEST_NODES, "-q", "-p", "no:cacheprovider"], plugins=[Names()])
    finally:
        for m, n, v in saved:
            setattr(m, n, v)
    assert code == 0, f"the captured tests failed ({code})"
    return captured


def evaluation_block(s: GameState) -> dict[str, Json]:
    dens = [E.burst_density(s, 0), E.burst_density(s, 1)]
    return {
        "density": [fxs(x) for x in dens],
        "features": [fxs(E.features(s, p, dens[p])) for p in (0, 1)],
        "roles": [fxs(E.role_features(s, dens[p])) for p in (0, 1)],
        "score": {
            name: [fx(E.score(s, p, w, dens[p])) for p in (0, 1)]
            for name, w in (
                ("roles", E.ROLE_WEIGHTS),
                ("tuned", E.TUNED_WEIGHTS),
                ("hand", E.HAND_WEIGHTS),
            )
        },
        "evaluate": {
            name: [fx(E.evaluate(s, p, w, dens[p])) for p in (0, 1)]
            for name, w in (("roles", E.ROLE_WEIGHTS), ("tuned", E.TUNED_WEIGHTS))
        },
    }


def knobs_json(k: P.PolicyKnobs) -> dict[str, Json]:
    return {
        "burst_yes_score": fx(k.burst_yes_score),
        "rule_polarity_table": k.rule_polarity_table,
        "zero_ap_face": k.zero_ap_face,
    }


def states_header() -> dict[str, Json]:
    """The settings every record of a states file was computed with."""
    return {
        "t": "header",
        "baseline": knobs_json(P.BASELINE_KNOBS),
        "adopted": knobs_json(ADOPTED),
        "base_temperature": fx(BASE_TEMPERATURE),
        "adopted_temperature": fx(ADOPTED_TEMPERATURE),
        "greedy_weights": E.DEFAULT_WEIGHTS.feature_set,
    }


def sampled_choice(s: GameState, seed: int, temperature: float, knobs: P.PolicyKnobs) -> Json:
    rng = SplitMix64(seed)
    chosen = P.choose(s, rng, temperature, knobs)
    return {"chosen": aj(chosen), "rng_after": T.u64(rng.state)}


def state_record(n: int, sample: Sample) -> dict[str, Json]:
    s = sample.state
    rec: dict[str, Json] = {
        "t": "terminal" if sample.terminal else "state",
        "id": sample.id,
        "snapshot": snapshot(s),
        "digest": state_digest(s),
        "winner": s.winner,
    }
    if not sample.terminal:
        dec = s.pending
        assert dec is not None
        p = dec.player
        seed = derive_seed(MASTER, "sample", n)
        rec.update(
            player=p,
            kind=dec.kind.value,
            options=[aj(o) for o in dec.options],
            scores_base=fxs(P.option_scores(s, dec)),
            scores_adopted=fxs(P.option_scores(s, dec, ADOPTED)),
            choose_base=aj(P.choose(s, None, 0.0)),
            choose_adopted=aj(P.choose(s, None, 0.0, ADOPTED)),
            sample={
                "seed": T.u64(seed),
                "base": sampled_choice(s, seed, BASE_TEMPERATURE, P.BASELINE_KNOBS),
                "adopted": sampled_choice(s, seed, ADOPTED_TEMPERATURE, ADOPTED),
            },
            polarity=[
                targeting.choice_polarity(s, False),
                targeting.choice_polarity(s, True),
            ],
            battle=targeting.choice_rule_lasts_this_battle(s),
        )
        r_seed = derive_seed(MASTER, "random", n)
        ra = TracedRandom(r_seed)
        chosen = ra.choose(s, p)
        assert ra.rng is not None
        rec["random"] = {
            "seed": T.u64(r_seed),
            "chosen": aj(chosen),
            "decision_seed": T.u64(derive_seed(r_seed, "random", "decision", 1)),
            "rng_after": T.u64(ra.rng.state),
        }
        if n % GREEDY_EVERY == 0:
            g_seed = derive_seed(MASTER, "greedy", n)
            ga = TracedGreedy(g_seed)
            chosen = ga.choose(s, p)
            assert ga.rng is not None
            assert ga.alternatives is not None
            rec["greedy"] = {
                "seed": T.u64(g_seed),
                "chosen": aj(chosen),
                "decision_seed": T.u64(derive_seed(g_seed, "greedy", "decision", 1)),
                "rng_after": T.u64(ga.rng.state),
                "alternatives": [[aj(a.action), fx(a.value), a.visits] for a in ga.alternatives],
            }
    rec.update(evaluation_block(s))
    return rec


# ---------------------------------------------------------------------------------------------
# opening scores


def opening_records() -> list[dict[str, Json]]:
    reg = get_registry()
    decks: list[DeckList] = [*T.test_decks(), *(T.example(n) for n in T.EXAMPLE_DECKS)]
    pools = [sorted(reg.db[n].def_id for n in deck.main) for deck in decks]
    gen = SplitMix64(derive_seed(MASTER, "opening"))
    out: list[dict[str, Json]] = []
    for _ in range(300):
        pool = list(pools[gen.randrange(len(pools))])
        gen.shuffle(pool)
        size = 5 if gen.randrange(10) < 7 else gen.randrange(10)
        hand = pool[:size]
        first = gen.randrange(2) == 0
        score = opening_score([reg.db.by_id(i) for i in hand], first)
        out.append({"t": "opening", "hand": hand, "first": first, "score": fx(score)})
    return out


# ---------------------------------------------------------------------------------------------
# game vectors (recorded items 3-7)


def deck_json(deck: DeckList) -> dict[str, Json]:
    return {"main": list(deck.main), "res": list(deck.resources)}


def game_records() -> list[dict[str, Json]]:
    decks = T.test_decks()
    out: list[dict[str, Json]] = [{"t": "decks", "decks": [deck_json(x) for x in decks]}]

    # item 3: RandomAgent(7) vs RandomAgent(8) from new_game(DECKS, 12345)
    agents = (RandomAgent(7), RandomAgent(8))
    st = new_game(decks, 12345)
    h = hashlib.sha256()
    records: list[dict[str, Json]] = []
    n = 0
    while st.pending is not None and n < 400:
        dec = st.pending
        dens = E.burst_density(st, dec.player)
        rec = {
            "n": n,
            "kind": dec.kind.value,
            "base": fxs(P.option_scores(st, dec)),
            "adopted": fxs(P.option_scores(st, dec, ADOPTED)),
            "density": fxs(dens),
            "features": fxs(E.features(st, dec.player, dens)),
            "roles": fxs(E.role_features(st, dens)),
            "score_roles": fx(E.score(st, dec.player, E.ROLE_WEIGHTS, dens)),
            "score_tuned": fx(E.score(st, dec.player, E.TUNED_WEIGHTS, dens)),
            "eval_roles": fx(E.evaluate(st, dec.player, E.ROLE_WEIGHTS, dens)),
        }
        h.update(json.dumps(rec, sort_keys=True).encode())
        p = dec.player
        chosen = agents[p].choose(st, p)
        records.append({**rec, "digest16": state_digest(st)[:16], "chosen": aj(chosen)})
        apply(st, chosen)
        n += 1
    assert h.hexdigest() == RANDOM_GAME_DIGEST
    out.append(
        {
            "t": "random_game",
            "seed": 12345,
            "agent_seeds": [7, 8],
            "decisions": n,
            "winner": st.winner,
            "turn": st.turn,
            "digest": h.hexdigest(),
            "records": records,
        }
    )

    # item 4: 20 random games, every decision's scores
    kinds: Counter[str] = Counter()
    h = hashlib.sha256()
    for g in range(20):
        agents = (RandomAgent(100 + g), RandomAgent(200 + g))
        st = new_game(decks, 5000 + g)
        while st.pending is not None:
            dec = st.pending
            dens = E.burst_density(st, dec.player)
            kinds[dec.kind.value] += 1
            row = [
                dec.kind.value,
                fxs(P.option_scores(st, dec)),
                fxs(P.option_scores(st, dec, ADOPTED)),
                fx(E.score(st, dec.player, E.ROLE_WEIGHTS, dens)),
                fx(E.score(st, dec.player, E.TUNED_WEIGHTS, dens)),
            ]
            h.update(json.dumps(row).encode())
            apply(st, agents[dec.player].choose(st, dec.player))
    assert h.hexdigest() == COVER_GAMES_DIGEST
    out.append(
        {
            "t": "cover_games",
            "games": 20,
            "seed_base": 5000,
            "agent_bases": [100, 200],
            "kinds": dict(sorted(kinds.items())),
            "digest": h.hexdigest(),
        }
    )

    # item 5: policy playouts from new_game(DECKS, 99)
    for label, rng_seed, temperature, knobs in (
        ("adopted", 5, 0.5, ADOPTED),
        ("baseline", 5, 0.5, P.BASELINE_KNOBS),
        ("argmax", None, 0.0, P.BASELINE_KNOBS),
    ):
        st = new_game(decks, 99)
        rng = None if rng_seed is None else SplitMix64(rng_seed)
        moves = play_to_end(st, rng, temperature, 3000, knobs)
        out.append(
            {
                "t": "playout",
                "label": label,
                "game_seed": 99,
                "rng_seed": rng_seed,
                "temperature": fx(temperature),
                "adopted": knobs == ADOPTED,
                "max_decisions": 3000,
                "until_turn": None,
                "moves": moves,
                "winner": st.winner,
                "turn": st.turn,
                "rng_after": None if rng is None else T.u64(rng.state),
                "final_digest": state_digest(st),
            }
        )
    st = new_game(decks, 99)
    moves = play_until_turn(st, 3, None, 0.0, 200)
    assert st.pending is not None
    out.append(
        {
            "t": "playout",
            "label": "until_turn_3",
            "game_seed": 99,
            "rng_seed": None,
            "temperature": fx(0.0),
            "adopted": False,
            "max_decisions": 200,
            "until_turn": 3,
            "moves": moves,
            "winner": st.winner,
            "turn": st.turn,
            "rng_after": None,
            "final_digest": state_digest(st),
            "pending": st.pending.kind.value,
        }
    )

    # item 6: greedy vs greedy with decision logs
    g0, g1 = GreedyAgent(1, log=True), GreedyAgent(2, log=True)
    st, decisions = run_game((g0, g1), decks, 777)
    logs = [g0.decision_log(), g1.decision_log()]
    digest = hashlib.sha256(json.dumps(logs).encode()).hexdigest()
    assert digest == GREEDY_GAME_DIGEST
    out.append(
        {
            "t": "greedy_game",
            "seed": 777,
            "agent_seeds": [1, 2],
            "decisions": decisions,
            "winner": st.winner,
            "turn": st.turn,
            "final_digest": state_digest(st),
            "logs_digest": digest,
            "logs": logs,
        }
    )

    # item 7: random vs random
    r0, r1 = RandomAgent(1, log=True), RandomAgent(2, log=True)
    st, decisions = run_game((r0, r1), decks, 777)
    logs = [r0.decision_log(), r1.decision_log()]
    out.append(
        {
            "t": "random_vs_random",
            "seed": 777,
            "agent_seeds": [1, 2],
            "decisions": decisions,
            "winner": st.winner,
            "turn": st.turn,
            "final_digest": state_digest(st),
            "logs_digest": hashlib.sha256(json.dumps(logs).encode()).hexdigest(),
        }
    )
    return out


# ---------------------------------------------------------------------------------------------
# driver


def lines(records: Sequence[dict[str, Json]]) -> bytes:
    return "".join(T.dumps_line(r) + "\n" for r in records).encode()


def chunks(records: list[dict[str, Json]], size: int) -> Iterator[list[dict[str, Json]]]:
    for i in range(0, len(records), size):
        yield records[i : i + size]


def build() -> dict[str, bytes]:
    files = {
        "pure.ndjson": lines(pure_records()),
        "registry.ndjson": lines(registry_records()),
        "opening.ndjson": lines(opening_records()),
    }
    states = [state_record(n, s) for n, s in enumerate(sample_states())]
    for i, part in enumerate(chunks(states, STATES_PER_FILE)):
        files[f"states-{i:03}.ndjson"] = lines([states_header(), *part])
    files["games.ndjson"] = lines(game_records())
    return files


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true", help="compare with the committed files")
    args = ap.parse_args(argv)
    files = build()
    if args.check:
        current = {p.name: p.read_bytes() for p in sorted(OUT.glob("*.ndjson"))}
        if current != files:
            stale = sorted(set(current) ^ set(files)) + sorted(
                n for n in set(current) & set(files) if current[n] != files[n]
            )
            print(f"stale policy vectors: {', '.join(stale)}", file=sys.stderr)
            return 1
        print(f"policy vectors current ({len(files)} files)")
        return 0
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.ndjson"):
        if old.name not in files:
            old.unlink()
    for name, data in files.items():
        (OUT / name).write_bytes(data)
    total = sum(len(v) for v in files.values())
    print(f"wrote {len(files)} files, {total} bytes, to {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
