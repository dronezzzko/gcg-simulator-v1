"""Export the reference vectors of the gcg-engine Phase 4 tests (docs/dev/PLAN.md §4.4, §9.6;
docs/dev/tests.json T-046, T-150, T-151).

Writes crates/gcg-engine/tests/fixtures/engine_vectors.ndjson, one record per line, keyed by "t":

- ``setup_touch`` (T-046): ``new_game`` on a deck pair and seed, then the three setup
  decisions of a plan (``go_first(p)``, then ``keep`` or ``redraw`` for each player), with the
  version, digest and ``redraws`` after each;
- ``game_touch`` (T-046): a game that always takes option 0, with the version and
  action count before every decision and the final state's facts;
- ``primitive_touch`` (T-046): a ``testkit.Scenario`` given as operations, one call of an
  engine primitive, the version before and after it, the documented delta,
  and the digest after (for ``determinize``/``permute_hidden``, of the new state);
- ``deck_order`` (T-151): one deck multiset in two orders, ``new_game`` with the same seed and
  chooser on each, with each player's deck and resource deck (def ids, top first), the RNG
  state, the decklists and the digest (``engine/game.py:107-158``: the caller's order is
  shuffled, the resource deck is not);
- ``once_used`` (T-150): a Scenario, once keys added in order with ``set.add`` (duplicates
  included), the canonical ``once_used`` list (``runner/digest.py:48``), the version and the
  digest.

A Scenario operation is a list: ``["add", player, number, zone, {kwargs}]``,
``["hand"|"trash"|"shields"|"deck", player, [numbers]]``, ``["resources", player, n, rested,
ex]``, ``["base", player, number|null, damage]``, ``["resource_deck", player, n]``,
``["add_raw", player, number, zone]`` or ``["start", step]`` (``Scenario.start``, with its
``advance``); ``results`` holds what each returned.

    uv run python oracle/export_engine_vectors.py [--check]
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any

import gcg_sim.testkit as tk
from gcg_sim.deck.parse import load_deck, to_decklist
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import Registry, get_registry
from gcg_sim.engine import battle, core, observe
from gcg_sim.engine import interp as I
from gcg_sim.engine.game import DeckList, apply, concede, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, Duration, Step, Zone
from gcg_sim.rng import SplitMix64, derive_seed
from gcg_sim.runner.digest import canonical_state, state_digest

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "gcg-engine" / "tests" / "fixtures" / "engine_vectors.ndjson"
DECKS = ROOT / "examples" / "decks"
MASTER = 20261102

VANILLA = "GD01-060"  # Zaku Mariner, a vanilla Lv2 Unit (testkit.FILLER_UNIT)
PILOT = "GD01-089"  # Riddhe Marcenas, a Pilot card
GUNDAM = "GD01-001"  # Gundam, which a Pilot pairs with in the lead examples

Json = Any
Op = list[Json]


def deck(name: str) -> DeckList:
    return to_decklist(load_deck(DECKS / f"{name}.txt"))


def deck_json(dl: DeckList) -> Json:
    return {"main": list(dl.main), "res": list(dl.resources)}


def digest(reg: Registry, st: GameState) -> str:
    return state_digest(st, reg)


# ---------------------------------------------------------------------------------------------
# Scenarios


def run_ops(init: dict[str, Json], ops: list[Op]) -> tuple[GameState, list[Json]]:
    """The Scenario of ``init`` after ``ops``, and what each operation returned."""
    sc = tk.Scenario(**init)
    results: list[Json] = []
    st: GameState | None = None
    for op in ops:
        name, args = op[0], op[1:]
        if name == "add":
            p, number, zone, kw = args
            results.append(sc.add(p, number, Zone(zone), **kw))
        elif name in ("hand", "trash", "shields"):
            results.append(getattr(sc, name)(args[0], *args[1]))
        elif name == "deck":
            sc.deck(args[0], *args[1])
            results.append(None)
        elif name == "resources":
            results.append(sc.resources(args[0], args[1], rested=args[2], ex=args[3]))
        elif name == "base":
            results.append(sc.base(args[0], args[1], damage=args[2]))
        elif name == "resource_deck":
            sc.resource_deck(args[0], args[1])
            results.append(None)
        elif name == "add_raw":
            results.append(sc.add_raw(args[0], args[1], Zone(args[2])))
        elif name == "start":
            st = sc.start(Step(args[0]))
            results.append(None)
        else:
            raise SystemExit(f"unknown scenario op {name}")
    if st is None:
        raise SystemExit("a scenario must end with start")
    return st, results


# ---------------------------------------------------------------------------------------------
# T-046: setup accounting


def setup_records(reg: Registry) -> Iterator[dict[str, Json]]:
    pairs = [
        ("rgz-bwf", "red-green-zeon", "blue-white-federation", 12345),
        ("bwf-pwt", "blue-white-federation", "purple-white-tekkadan", 7),
        ("pwt-rgz", "purple-white-tekkadan", "red-green-zeon", 2**64 - 5),
    ]
    for pid, a, b, seed in pairs:
        for first in (0, 1):
            for p1 in ("keep", "redraw"):
                for p2 in ("keep", "redraw"):
                    st = new_game((deck(a), deck(b)), seed)
                    after_new = {"version": st.version, "digest": digest(reg, st)}
                    steps = []
                    for a_ in (
                        Action(ActionKind.GO_FIRST, first),
                        Action(ActionKind(p1)),
                        Action(ActionKind(p2)),
                    ):
                        apply(st, a_)
                        steps.append(
                            {
                                "apply": a_.to_json(),
                                "version": st.version,
                                "step": int(st.step),
                                "redraws": list(st.redraws),
                                "digest": digest(reg, st),
                            }
                        )
                    assert st.step is Step.MAIN
                    assert st.turn == 1
                    yield {
                        "t": "setup_touch",
                        "id": f"{pid}/{seed}/first{first}/{p1}-{p2}",
                        "decks": [deck_json(deck(a)), deck_json(deck(b))],
                        "seed": str(seed),
                        "after_new_game": after_new,
                        "steps": steps,
                    }


# ---------------------------------------------------------------------------------------------
# T-046: the "always option 0" games


def game_records(reg: Registry) -> Iterator[dict[str, Json]]:
    games = [
        ("rgz-bwf/12345", "red-green-zeon", "blue-white-federation", 12345),
        ("bwf-pwt/3", "blue-white-federation", "purple-white-tekkadan", 3),
        ("pwt-rgz/4", "purple-white-tekkadan", "red-green-zeon", 4),
    ]
    for gid, a, b, seed in games:
        st = new_game((deck(a), deck(b)), seed)
        decisions = []
        while st.pending is not None:
            choice = st.pending.options[0]
            decisions.append(
                {
                    "version": st.version,
                    "action_count": st.action_count,
                    "kind": st.pending.kind.value,
                    "chosen": choice.to_json(),
                }
            )
            apply(st, choice)
        yield {
            "t": "game_touch",
            "id": gid,
            "decks": [deck_json(deck(a)), deck_json(deck(b))],
            "seed": str(seed),
            "decisions": decisions,
            "final": {
                "version": st.version,
                "digest": digest(reg, st),
                "winner": st.winner,
                "end_reason": st.end_reason.value if st.end_reason else None,
                "turn": st.turn,
                "action_count": st.action_count,
                "event_group": st.event_group,
                "rng": str(st.rng.state),
            },
        }


# ---------------------------------------------------------------------------------------------
# T-046: per-primitive version deltas

PRIMITIVE_INIT: dict[str, Json] = {"seed": 5}
PRIMITIVE_OPS: list[Op] = [
    ["resources", 0, 3, 0, 2],  # uids 0-2 Resources, 3-4 EX Resources
    ["add", 0, VANILLA, int(Zone.BATTLE), {}],  # 5
    ["add", 0, GUNDAM, int(Zone.BATTLE), {"pilot": PILOT}],  # 6, its Pilot 7
    ["add", 0, PILOT, int(Zone.HAND), {}],  # 8
    ["add", 0, VANILLA, int(Zone.HAND), {}],  # 9
    ["add", 1, VANILLA, int(Zone.BATTLE), {}],  # 10
    ["shields", 1, [VANILLA, VANILLA]],  # 11, 12
    ["base", 1, None, 0],  # 13
    ["start", int(Step.MAIN)],
]

# (id, call, documented version delta). A call is [name, *args, {kwargs}].
PRIMITIVES: list[tuple[str, list[Json], int]] = [
    ("touch", ["touch", {}], 1),
    ("move/unit-to-trash", ["core.move", 5, int(Zone.TRASH), {}], 1),
    ("move/paired-unit-to-hand", ["core.move", 6, int(Zone.HAND), {}], 1),
    ("move/hand-to-deck-bottom", ["core.move", 9, int(Zone.DECK), {"bottom": True}], 1),
    ("move/hand-to-shield-revealed", ["core.move", 9, int(Zone.SHIELD), {"reveal": True}], 1),
    ("remove_from_game", ["core.remove_from_game", 3, {}], 1),
    ("new_card", ["core.new_card", VANILLA, 1, int(Zone.TRASH), {}], 1),
    ("new_card/rested", ["core.new_card", VANILLA, 0, int(Zone.BATTLE), {"rested": True}], 1),
    ("shuffle_deck", ["core.shuffle_deck", 0, {}], 1),
    ("record", ["core.record", "attack", 0, {"by": 0, "uid": 5}], 0),
    (
        "apply_damage/zero",
        ["core.apply_damage", 10, 0, {"source": -1, "battle": False, "by": 0}],
        0,
    ),
    (
        "apply_damage/effect",
        ["core.apply_damage", 10, 1, {"source": 5, "battle": False, "by": 0}],
        1,
    ),
    ("apply_damage/base", ["core.apply_damage", 13, 2, {"source": 5, "battle": True, "by": 0}], 1),
    ("apply_damage/hand", ["core.apply_damage", 9, 2, {"source": -1, "battle": False, "by": 0}], 0),
    # interp.pair has no testkit forwarder: the PAIR action reaches it after pay_generic (+1),
    # and pair touches twice (its move and its own touch, interp.py:1370-1374).
    ("apply/pair", ["game.apply", ["pair", 8, 5, 0], {}], 3),
    ("pay_generic/zero", ["interp.pay_generic", 0, 0, {}], 0),
    ("pay_generic/normal", ["interp.pay_generic", 0, 2, {}], 1),
    ("pay_generic/with-ex", ["interp.pay_generic", 0, 4, {"ex_used": 2}], 3),
    ("pay_generic/unpayable", ["interp.pay_generic", 0, 9, {}], 0),
    (
        "add_lasting/this-turn",
        ["interp.add_lasting", {"StatMod": [1, 0]}, 0, 5, [[5, 0]], "this_turn", {}],
        1,
    ),
    (
        "add_lasting/this-battle-outside-a-battle",
        ["interp.add_lasting", {"StatMod": [1, 0]}, 0, 5, [[5, 0]], "this_battle", {}],
        0,
    ),
    ("draw/two", ["interp.draw", 0, 2, {}], 2),
    ("draw/none", ["interp.draw", 1, 0, {}], 0),
    ("deploy_cards", ["interp.deploy_cards", [9], {"rested": False, "by": 0}], 1),
    ("declare_attack", ["battle.declare_attack", 5, PLAYER_TARGET, {}], 2),
    ("concede", ["game.concede", 1, {}], 0),
    ("trial_rest", ["trial_rest", 10, {}], 1),
    ("determinize", ["observe.determinize", 0, 7, {}], 1),
    ("permute_hidden", ["observe.permute_hidden", 1, 7, {}], 1),
]


def effect_of(spec: dict[str, list[int]]) -> d.Continuous:
    ((kind, args),) = spec.items()
    if kind != "StatMod":
        raise SystemExit(f"unknown effect {kind}")
    return d.StatMod(ap=args[0], hp=args[1])


def run_primitive(reg: Registry, st: GameState, call: list[Json]) -> tuple[GameState, Json]:
    """Runs ``call`` on ``st``: the state whose version it changes, and what it returned."""
    name, args, kw = call[0], call[1:-1], call[-1]
    if name == "touch":
        st.touch()
        return st, None
    if name == "core.move":
        core.move(st, args[0], Zone(args[1]), **kw)
        return st, None
    if name == "core.remove_from_game":
        core.remove_from_game(st, args[0])
        return st, None
    if name == "core.new_card":
        return st, core.new_card(st, reg.db[args[0]].def_id, args[1], Zone(args[2]), **kw)
    if name == "core.shuffle_deck":
        core.shuffle_deck(st, args[0])
        return st, None
    if name == "core.record":
        core.record(st, args[0], args[1], **kw)
        return st, None
    if name == "core.apply_damage":
        return st, core.apply_damage(st, args[0], args[1], **kw)
    if name == "game.apply":
        apply(st, Action.from_json(args[0]))
        return st, None
    if name == "interp.pay_generic":
        return st, I.pay_generic(st, args[0], args[1], **kw)
    if name == "interp.add_lasting":
        targets = tuple((u, s) for u, s in args[3])
        I.add_lasting(st, effect_of(args[0]), args[1], args[2], targets, Duration(args[4]))
        return st, None
    if name == "interp.draw":
        return st, I.draw(st, args[0], args[1])
    if name == "interp.deploy_cards":
        I.deploy_cards(st, list(args[0]), **kw)
        return st, None
    if name == "battle.declare_attack":
        battle.declare_attack(st, args[0], args[1])
        return st, None
    if name == "game.concede":
        concede(st, args[0])
        return st, None
    if name == "trial_rest":
        # ai/actions.py:228-229, the AI's write into a trial world.
        st.cards[args[0]].rested = True
        st.touch()
        return st, None
    if name == "observe.determinize":
        return observe.determinize(st, args[0], args[1]), None
    if name == "observe.permute_hidden":
        return observe.permute_hidden(st, args[0], args[1]), None
    raise SystemExit(f"unknown primitive {name}")


def primitive_records(reg: Registry) -> Iterator[dict[str, Json]]:
    for pid, call, expect in PRIMITIVES:
        st, results = run_ops(PRIMITIVE_INIT, PRIMITIVE_OPS)
        before = st.version
        source_digest = digest(reg, st)
        out, returned = run_primitive(reg, st, call)
        delta = out.version - before
        if delta != expect:
            raise SystemExit(
                f"{pid}: Python's version delta is {delta}, the documented delta is {expect}"
            )
        if out is not st and digest(reg, st) != source_digest:
            raise SystemExit(f"{pid}: the call changed its input state")
        yield {
            "t": "primitive_touch",
            "id": pid,
            "init": PRIMITIVE_INIT,
            "ops": PRIMITIVE_OPS,
            "results": results,
            "call": call,
            "expect": expect,
            "before": before,
            "after": out.version,
            "returned": returned,
            "digest": digest(reg, out),
        }


# ---------------------------------------------------------------------------------------------
# T-151: the caller's deck order


def zone_defs(st: GameState, p: int, zone: Zone) -> list[int]:
    return [st.cards[u].def_id for u in st.zones[p][zone]]


def game_facts(reg: Registry, st: GameState) -> dict[str, Json]:
    return {
        "digest": digest(reg, st),
        "rng": str(st.rng.state),
        "setup_chooser": st.setup_chooser,
        "deck": [zone_defs(st, p, Zone.DECK) for p in (0, 1)],
        "resource_deck": [zone_defs(st, p, Zone.RESOURCE_DECK) for p in (0, 1)],
        "decklists": [list(st.decklists[p]) for p in (0, 1)],
        "resource_decklists": [list(st.resource_decklists[p]) for p in (0, 1)],
    }


def permuted(cards: tuple[str, ...], seed: int) -> tuple[str, ...]:
    out = list(cards)
    SplitMix64(seed).shuffle(out)
    return tuple(out)


def deck_order_records(reg: Registry) -> Iterator[dict[str, Json]]:
    rgz, bwf, pwt = (
        deck("red-green-zeon"),
        deck("blue-white-federation"),
        deck("purple-white-tekkadan"),
    )
    mixed = DeckList(rgz.main, ("R-001",) * 5 + ("R-002",) * 5)
    cases: list[tuple[str, DeckList, DeckList, int, int | None]] = [
        ("rgz-bwf/12345", rgz, bwf, 12345, None),
        ("pwt-rgz/7/chooser0", pwt, rgz, 7, 0),
        ("mixed-resources/bwf/18446744073709551611", mixed, bwf, 2**64 - 5, None),
    ]
    for cid, d0, d1, seed, chooser in cases:
        other = (
            DeckList(tuple(reversed(d0.main)), tuple(reversed(d0.resources))),
            DeckList(
                permuted(d1.main, derive_seed(MASTER, "deck-order", cid)),
                permuted(d1.resources, derive_seed(MASTER, "resource-order", cid)),
            ),
        )
        orders: list[dict[str, Json]] = []
        for name, decks in (("parsed", (d0, d1)), ("permuted", other)):
            st = new_game(decks, seed, chooser=chooser)
            orders.append(
                {
                    "name": name,
                    "decks": [deck_json(x) for x in decks],
                    "facts": game_facts(reg, st),
                }
            )
        a, b = orders[0]["facts"], orders[1]["facts"]
        if a["decklists"] != b["decklists"] or a["rng"] != b["rng"]:
            raise SystemExit(f"{cid}: the two orders are not one multiset with one RNG stream")
        if a["deck"][0] == b["deck"][0] or a["deck"][1] == b["deck"][1]:
            raise SystemExit(f"{cid}: the two orders shuffled to the same deck")
        yield {
            "t": "deck_order",
            "id": cid,
            "seed": str(seed),
            "chooser": chooser,
            "orders": orders,
        }


# ---------------------------------------------------------------------------------------------
# T-150: once_used set semantics

ONCE_OPS: list[Op] = [
    ["add", 0, VANILLA, int(Zone.BATTLE), {}],
    ["add", 1, VANILLA, int(Zone.BATTLE), {}],
    ["start", int(Step.MAIN)],
]

# Keys of every int-only form: (aid, host, zone_seq), (-9, uid, turn),
# (-8|-7, aid, host, uid, zone_seq) and a binding tag; json.dumps order differs from numeric
# order for [10, ...] against [9, ...] and [-1, ...] against [-10, ...].
ONCE_KEYS: list[list[int]] = [
    [12, 0, 1],
    [3, 0, 1],
    [12, 0, 2],
    [-9, 1, 3],
    [-9, 0, 3],
    [-8, 4, 0, 1, 1],
    [-7, 4, 0, 1, 1],
    [100, 1, 1],
    [10, 1, 1],
    [9, 99, 1],
    [-1, 2, 3],
    [-10, 0, 0],
]


def once_records(reg: Registry) -> Iterator[dict[str, Json]]:
    plans = [
        ("in-order", ONCE_KEYS),
        ("reversed-with-duplicates", [*reversed(ONCE_KEYS), *ONCE_KEYS[::3], ONCE_KEYS[0]]),
        ("one-key-five-times", [ONCE_KEYS[4]] * 5),
    ]
    for oid, keys in plans:
        st, results = run_ops({}, ONCE_OPS)
        version = st.version
        for k in keys:
            st.once_used.add(tuple(k))
        if st.version != version:
            raise SystemExit(f"{oid}: once_used.add touched the state")
        yield {
            "t": "once_used",
            "id": oid,
            "init": {},
            "ops": ONCE_OPS,
            "results": results,
            "keys": keys,
            "canonical": canonical_state(st, reg)["once_used"],
            "version": st.version,
            "digest": digest(reg, st),
        }


# ---------------------------------------------------------------------------------------------


def build() -> str:
    reg = get_registry()
    makers: list[Callable[[Registry], Iterator[dict[str, Json]]]] = [
        setup_records,
        game_records,
        primitive_records,
        deck_order_records,
        once_records,
    ]
    records = [r for make in makers for r in make(reg)]
    return "".join(json.dumps(r, ensure_ascii=True, separators=(",", ":")) + "\n" for r in records)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Export the gcg-engine Phase 4 test vectors.")
    ap.add_argument("--check", action="store_true", help="fail if the committed file is stale")
    args = ap.parse_args(argv)
    text = build()
    if args.check:
        if not OUT.exists() or OUT.read_text() != text:
            print(f"{OUT.relative_to(ROOT)} is stale: run {Path(__file__).name}", file=sys.stderr)
            return 1
        return 0
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)}: {text.count(chr(10))} records, {len(text)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
