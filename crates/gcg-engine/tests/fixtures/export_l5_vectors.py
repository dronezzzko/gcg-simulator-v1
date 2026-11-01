"""Export the reference vectors of the gcg-engine layer-5 tests (docs/port/PLAN.md §10.4 C).

Writes crates/gcg-engine/tests/fixtures/l5_vectors.ndjson, one record per line, keyed by "t":

- ``new_game``: decks (card numbers as written), seed, options and the state after
  ``new_game`` (digest, RNG state, version, chooser, pending decision, cards), then the setup
  decisions applied one by one with the state after each;
- ``new_game_error``: a ``new_game`` call that raises, with the exception class and text;
- ``problems``: ``deck_construction_problems`` of a crafted deck;
- ``game``: a game played by a fixed policy (option 0, or seeded random options), with every
  decision (turn, step, kind, player, option count, chosen action, digest before it) and the
  final state;
- ``observe``: a snapshot (the side tables of ``port/oracle/export_state_fixtures.py``) with
  ``unknown_instances``, ``unknown_pools``, ``determinize`` and ``permute_hidden`` results and
  information-set-key equalities per (observer, seed);
- ``invariants``: a snapshot (possibly broken on purpose) with the result of
  ``invariants.check`` against the initial multiset of its game;
- ``scenario``: ``testkit.Scenario`` operations, the digest before ``Scenario.start``'s
  ``advance`` (``Scenario::prepare_start``, open item O14) and after it.

    uv run python crates/gcg-engine/tests/fixtures/export_l5_vectors.py [--check]
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "port" / "oracle"))

from export_state_fixtures import snapshot_record  # noqa: E402

import gcg_sim.testkit as tk  # noqa: E402
from gcg_sim.deck.parse import load_deck, to_decklist  # noqa: E402
from gcg_sim.effects.registry import Registry, get_registry  # noqa: E402
from gcg_sim.engine import invariants, observe  # noqa: E402
from gcg_sim.engine.game import (  # noqa: E402
    DeckList,
    apply,
    concede,
    deck_construction_problems,
    new_game,
)
from gcg_sim.engine.state import Action, GameState  # noqa: E402
from gcg_sim.engine.types import ActionKind, Step, Zone  # noqa: E402
from gcg_sim.rng import SplitMix64, derive_seed  # noqa: E402
from gcg_sim.runner.digest import state_digest  # noqa: E402

OUT = Path(__file__).resolve().parent / "l5_vectors.ndjson"
DECKS = ROOT / "examples" / "decks"
MASTER = 20261102

Json = Any


def deck(name: str) -> DeckList:
    return to_decklist(load_deck(DECKS / f"{name}.txt"))


def deck_json(d: DeckList) -> Json:
    return {"main": list(d.main), "res": list(d.resources)}


def state_facts(reg: Registry, st: GameState) -> dict[str, Json]:
    return {
        "digest": state_digest(st, reg),
        "rng": str(st.rng.state),
        "version": st.version,
        "setup_chooser": st.setup_chooser,
        "first_player": st.first_player,
        "step": int(st.step),
        "action_count": st.action_count,
        "pending": st.pending.to_json() if st.pending else None,
    }


def new_game_records(reg: Registry) -> Iterator[dict[str, Json]]:
    rgz, bwf, pwt = (
        deck("red-green-zeon"),
        deck("blue-white-federation"),
        deck("purple-white-tekkadan"),
    )
    cases: list[tuple[str, DeckList, DeckList, int, int | None, list[str]]] = [
        ("inv06b-18", rgz, bwf, 12345, None, ["go_first:1", "redraw", "keep"]),
        ("chooser0", bwf, pwt, 7, 0, ["go_first:1", "keep", "redraw"]),
        ("chooser1", pwt, rgz, 2**64 - 5, 1, ["go_first:0", "redraw", "redraw"]),
        ("die-roll", rgz, rgz, 99, None, ["go_first:1", "keep", "keep"]),
    ]
    for cid, d0, d1, seed, chooser, plan in cases:
        st = new_game((d0, d1), seed, chooser=chooser)
        after = state_facts(reg, st)
        after["cards"] = [[c.def_id, c.owner, int(c.zone), c.known] for c in st.cards]
        after["decklists"] = [list(st.decklists[0]), list(st.decklists[1])]
        after["resource_decklists"] = [
            list(st.resource_decklists[0]),
            list(st.resource_decklists[1]),
        ]
        steps = []
        for word in plan:
            name, _, arg = word.partition(":")
            a = Action(ActionKind(name), int(arg)) if arg else Action(ActionKind(name))
            apply(st, a)
            facts = state_facts(reg, st)
            facts["apply"] = a.to_json()
            facts["hands"] = [list(st.zones[p][Zone.HAND]) for p in (0, 1)]
            facts["decks"] = [list(st.zones[p][Zone.DECK]) for p in (0, 1)]
            facts["redraws"] = list(st.redraws)
            steps.append(facts)
        yield {
            "t": "new_game",
            "id": cid,
            "decks": [deck_json(d0), deck_json(d1)],
            "seed": str(seed),
            "chooser": chooser,
            "after": after,
            "steps": steps,
        }


def error_records() -> Iterator[dict[str, Json]]:
    rgz, bwf = deck("red-green-zeon"), deck("blue-white-federation")
    short = DeckList(rgz.main[:49], rgz.resources)
    unknown = DeckList((*rgz.main[:49], "XX99-999"), rgz.resources)
    cases: list[tuple[str, tuple[DeckList, DeckList], bool]] = [
        ("p1-short", (bwf, short), True),
        ("p0-unknown", (unknown, bwf), True),
        ("unknown-without-validate", (unknown, bwf), False),
    ]
    for cid, decks, validate in cases:
        try:
            new_game(decks, 1, validate=validate)
        except Exception as exc:  # the class and text are the vector
            yield {
                "t": "new_game_error",
                "id": cid,
                "decks": [deck_json(d) for d in decks],
                "seed": "1",
                "validate": validate,
                "class": type(exc).__name__,
                "message": str(exc),
            }
        else:
            raise SystemExit(f"new_game case {cid} did not raise")


def problem_records() -> Iterator[dict[str, Json]]:
    reg = get_registry()
    db = reg.db
    rgz, bwf = deck("red-green-zeon"), deck("blue-white-federation")
    pid = next(c.canonical_product_id for c in db.real_cards() if c.canonical_product_id)
    ex_res = db.ex_resource.card_number
    command = next(c.card_number for c in db.real_cards() if c.card_type.value == "COMMAND")
    colours = sorted(
        {
            c.card_number
            for c in db.real_cards()
            if c.color is not None and c.card_type.value == "UNIT"
        }
    )
    by_colour: dict[str, str] = {}
    for n in colours:
        cd = db[n]
        assert cd.color is not None
        by_colour.setdefault(cd.color.value, n)
    three = [by_colour[k] for k in sorted(by_colour)[:3]]
    cases: list[tuple[str, tuple[str, ...], tuple[str, ...]]] = [
        ("legal", rgz.main, rgz.resources),
        ("sizes", rgz.main[:3], rgz.resources[:2]),
        (
            "copies-and-types",
            (*rgz.main[:44], *[rgz.main[0]] * 5, ex_res),
            (*rgz.resources[:9], command),
        ),
        ("colours", (*bwf.main[:47], *three), (*bwf.resources[:9], ex_res)),
        (
            "product-ids-and-unknown",
            (*rgz.main[:48], pid, " XX01-001 "),
            (*rgz.resources[:9], "nope"),
        ),
    ]
    for cid, main, res in cases:
        yield {
            "t": "problems",
            "id": cid,
            "main": list(main),
            "res": list(res),
            "problems": deck_construction_problems(DeckList(tuple(main), tuple(res))),
        }


Policy = Callable[[GameState], Action]


def first_option(st: GameState) -> Action:
    assert st.pending is not None
    return st.pending.options[0]


def random_policy(seed: int) -> Policy:
    rng = SplitMix64(seed)

    def pick(st: GameState) -> Action:
        assert st.pending is not None
        opts = st.pending.options
        return opts[rng.randrange(len(opts))]

    return pick


def play(
    reg: Registry, decks: tuple[DeckList, DeckList], seed: int, policy: Policy, keep: set[int]
) -> tuple[list[dict[str, Json]], dict[str, Json], dict[int, GameState]]:
    st = new_game(decks, seed)
    decisions: list[dict[str, Json]] = []
    kept: dict[int, GameState] = {}
    i = 0
    while st.winner is None and st.pending is not None:
        if i in keep:
            kept[i] = st.clone()
        dec = st.pending
        a = policy(st)
        decisions.append(
            {
                "turn": st.turn,
                "step": int(st.step),
                "kind": dec.kind.value,
                "player": dec.player,
                "n": len(dec.options),
                "chosen": a.to_json(),
                "digest16": state_digest(st, reg)[:16],
            }
        )
        apply(st, a)
        i += 1
    kept[-1] = st.clone()
    final = state_facts(reg, st)
    final.update(
        winner=st.winner,
        end_reason=st.end_reason.value if st.end_reason else None,
        turn=st.turn,
        active=st.active,
        event_group=st.event_group,
        next_battle_id=st.next_battle_id,
        decisions=len(decisions),
    )
    return decisions, final, kept


GAMES: list[tuple[str, str, str, int, str]] = [
    ("first-option/rgz-bwf/12345", "red-green-zeon", "blue-white-federation", 12345, "first"),
    ("random/bwf-pwt/3", "blue-white-federation", "purple-white-tekkadan", 3, "random"),
    ("random/pwt-rgz/4", "purple-white-tekkadan", "red-green-zeon", 4, "random"),
]
KEEP = {0, 2, 3, 9, 25, 47, 68, 150, 300}


def observe_cases(st: GameState, sid: str, reg: Registry) -> dict[str, Json]:
    cases = []
    for observer in (0, 1):
        for seed in (7, derive_seed(MASTER, "observe", sid, observer)):
            try:
                det = observe.determinize(st, observer, seed)
                det_facts: Json = {
                    "digest": state_digest(det, reg),
                    "rng": str(det.rng.state),
                    "version": det.version,
                    "key_eq": observe.information_set_key(det, observer)
                    == observe.information_set_key(st, observer),
                }
                det_error = None
            except ValueError as exc:
                det_facts = None
                det_error = str(exc)
            perm = observe.permute_hidden(st, observer, seed)
            cases.append(
                {
                    "observer": observer,
                    "seed": str(seed),
                    "unknown": [observe.unknown_instances(st, observer, o) for o in (0, 1)],
                    "pools": [list(observe.unknown_pools(st, observer, o)) for o in (0, 1)],
                    "hidden": [
                        u for u in range(len(st.cards)) if observe.is_hidden_from(st, u, observer)
                    ],
                    "determinize": det_facts,
                    "determinize_error": det_error,
                    "permute": {
                        "digest": state_digest(perm, reg),
                        "rng": str(perm.rng.state),
                        "key_eq": observe.information_set_key(perm, observer)
                        == observe.information_set_key(st, observer),
                    },
                }
            )
    return {
        "keys_equal_across_observers": observe.information_set_key(st, 0)
        == observe.information_set_key(st, 1),
        "cases": cases,
    }


def check_result(st: GameState, init: Any) -> str | None:
    try:
        invariants.check(st, init)
    except invariants.InvariantViolation as exc:
        return str(exc)
    return None


def multiset_json(init: Any) -> Json:
    return sorted([[owner, def_id, n] for (owner, def_id), n in init.items()])


def state_record(
    reg: Registry, sid: str, st: GameState, init: Any, with_observe: bool = True
) -> dict[str, Json]:
    """A snapshot with what ``observe`` and ``invariants.check`` say about it."""
    rec = snapshot_record(reg, st, sid, [])
    conceded = []
    for p in (0, 1):
        c = st.clone()
        concede(c, p)
        conceded.append(state_digest(c, reg))
    trial = None
    units = [u for p in (0, 1) for u in st.zones[p][Zone.BATTLE]]
    if units:
        r = st.clone()
        r.cards[units[0]].rested = not r.cards[units[0]].rested
        r.touch()
        trial = {
            "uid": units[0],
            "rested": r.cards[units[0]].rested,
            "digest": state_digest(r, reg),
        }
    return {
        "t": "state",
        "id": sid,
        "snapshot": {k: rec[k] for k in ("state", "continuous", "filter_sets", "rules_checked")},
        "digest": rec["digest"],
        "frames": len(st.frames),
        "concede": conceded,
        "rested_trial": trial,
        "observe": observe_cases(st, sid, reg) if with_observe else None,
        "invariants": {
            "initial": multiset_json(init),
            "error": check_result(st, init),
            "needs_view": st.winner is None and st.phase.value != 0 and not st.frames,
        },
    }


def broken_states(base: GameState) -> Iterator[tuple[str, GameState]]:
    """States that break one invariant each, from a SETUP-phase state (no view needed)."""
    hand0 = base.zones[0][Zone.HAND]
    deck1 = base.zones[1][Zone.DECK]

    s = base.clone()
    s.cards[hand0[0]].damage = 2
    s.cards[deck1[3]].damage = -1
    yield "damage", s

    s = base.clone()
    u = deck1[0]
    s.zones[1][Zone.DECK].remove(u)
    s.zones[1][Zone.TRASH].append(u)
    s.zones[0][Zone.HAND].append(deck1[1])
    yield "zones", s

    s = base.clone()
    unit, pilot = hand0[0], hand0[1]
    s.cards[unit].pair = pilot
    s.cards[pilot].pair = deck1[2]
    s.cards[deck1[5]].pair = 3
    yield "pairing", s

    s = base.clone()
    s.cards[deck1[4]].def_id = s.cards[deck1[4]].def_id + 1
    s.cards[hand0[2]].def_id = s.cards[deck1[6]].def_id
    yield "multiset", s

    s = base.clone()
    s.pending = None
    yield "no-pending", s


def scenario_records(reg: Registry) -> Iterator[dict[str, Json]]:
    db = reg.db
    vanilla = sorted(
        c.card_number for c in db.real_cards() if c.card_type.value == "UNIT" and c.is_vanilla
    )
    pilot = next(
        c.card_number
        for c in db.real_cards()
        if c.card_type.value == "PILOT" and reg.card(c.def_id).script is not None
    )
    base_card = next(
        c.card_number
        for c in db.real_cards()
        if c.card_type.value == "BASE" and reg.card(c.def_id).script is not None
    )
    token = next(c.card_number for c in db if c.is_token and c.card_type.value == "UNIT TOKEN")
    specs: list[tuple[str, dict[str, Json], list[list[Json]]]] = [
        ("default-main", {}, [["prepare_start", int(Step.MAIN)]]),
        (
            "rich",
            {"active": 1, "turn": 6, "seed": 77, "deck_size": 12},
            [
                [
                    "add",
                    1,
                    vanilla[0],
                    int(Zone.BATTLE),
                    {"rested": True, "damage": 1, "pilot": pilot},
                ],
                ["add", 0, vanilla[1], int(Zone.BATTLE), {"deployed_this_turn": True}],
                ["add", 0, vanilla[2], int(Zone.HAND), {"known": False}],
                ["add", 1, vanilla[3], int(Zone.TRASH), {"known": False}],
                ["hand", 1, [vanilla[4], vanilla[5]]],
                ["trash", 0, [vanilla[6]]],
                ["shields", 0, [vanilla[7], vanilla[8], vanilla[9]]],
                ["deck", 0, [vanilla[10], vanilla[11]]],
                ["resources", 1, 4, 2, 1],
                ["base", 0, None, 2],
                ["base", 1, base_card, 0],
                ["resource_deck", 1, 3],
                ["add_raw", 0, token, int(Zone.BATTLE)],
                ["prepare_start", int(Step.MAIN)],
                ["add", 0, vanilla[12], int(Zone.BATTLE), {}],
            ],
        ),
        (
            "end-action",
            {"active": 0, "turn": 4},
            [["resources", 0, 2, 0, 0], ["deck", 1, []], ["prepare_start", int(Step.END_ACTION)]],
        ),
        (
            "start-step",
            {"turn": 1, "deck_size": 3},
            [["hand", 0, [vanilla[0]]], ["prepare_start", int(Step.START_STEP)]],
        ),
    ]
    for sid, init, ops in specs:
        sc = tk.Scenario(**init)
        results: list[Json] = []
        before: str | None = None
        rules_checked = 0
        for op in ops:
            name, args = op[0], op[1:]
            if name == "prepare_start":
                # Scenario.start without its advance (testkit.py:181): Scenario::prepare_start.
                namespace = vars(tk)
                original = namespace["advance"]
                namespace["advance"] = lambda st: None
                try:
                    sc.start(Step(args[0]))
                finally:
                    namespace["advance"] = original
                before = state_digest(sc.st, reg)
                rules_checked = sc.st._rules_checked
                results.append(None)
            elif name == "add":
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
            else:
                raise SystemExit(f"unknown scenario op {name}")
        assert before is not None
        st = sc.st
        rec = snapshot_record(reg, st, sid, [])
        yield {
            "t": "scenario",
            "id": sid,
            "init": init,
            "ops": ops,
            "results": results,
            "before": before,
            "rules_checked": rules_checked,
            "final": rec["digest"],
            "final_snapshot": {
                k: rec[k] for k in ("state", "continuous", "filter_sets", "rules_checked")
            },
        }
    sc = tk.Scenario(active=0)
    gundam = sc.add(0, "GD01-001", Zone.HAND)
    sc.resources(0, 5)
    st = sc.start()
    yield {
        "t": "scenario_start",
        "id": "readme-example",
        "hand": gundam,
        "after": state_digest(st, reg),
        "options": [o.to_json() for o in (st.pending.options if st.pending else ())],
    }


def assertion_text(f: Callable[[], object]) -> str:
    try:
        f()
    except AssertionError as exc:
        return str(exc)
    raise SystemExit("the testkit call did not fail")


def testkit_records(reg: Registry) -> Iterator[dict[str, Json]]:
    """Helper texts and queries that need no rules beyond setup."""
    st = new_game((deck("red-green-zeon"), deck("blue-white-federation")), 12345)
    before = state_digest(st, reg)
    fresh = tk.Scenario().st
    probe = st.clone()
    accepted = assertion_text(lambda: tk.expect_illegal(probe, Action(ActionKind.GO_FIRST, 1)))
    rejected = st.clone()
    tk.expect_illegal(rejected, Action(ActionKind.KEEP))
    conceded = st.clone()
    concede(conceded, 0)
    over = conceded.clone()
    concede(over, 1)
    red = tk.vanilla_deck("Red")
    capped = new_game((deck("red-green-zeon"), deck("blue-white-federation")), 12345, max_actions=0)
    capped_before = state_digest(capped, reg)
    apply(capped, Action(ActionKind.GO_FIRST, 1))
    yield {
        "t": "testkit",
        "id": "setup",
        "decks": [deck_json(deck("red-green-zeon")), deck_json(deck("blue-white-federation"))],
        "seed": "12345",
        "fail_no_decision": assertion_text(lambda: tk.act(fresh, ActionKind.END_MAIN)),
        "fail_keep": assertion_text(lambda: tk.act(st.clone(), ActionKind.KEEP, 3)),
        "fail_go_first": assertion_text(
            lambda: tk.act(st.clone(), ActionKind.GO_FIRST, 5, None, 0)
        ),
        "has_go_first_1": tk.has_action(st, ActionKind.GO_FIRST, 1),
        "has_go_first_2": tk.has_action(st, ActionKind.GO_FIRST, 2),
        "legal_kinds": sorted(k.value for k in tk.legal_kinds(st)),
        "accepted": accepted,
        "after_accepted": state_digest(probe, reg),
        "rejected_unchanged": state_digest(rejected, reg) == before,
        "conceded": state_facts(reg, conceded) | {"winner": conceded.winner},
        "conceded_twice": state_digest(over, reg),
        "vanilla_red": [list(red.main), list(red.resources)],
        "vanilla_error": str(_value_error(lambda: tk.vanilla_deck("Nope"))),
        "capped_before": capped_before,
        "capped": state_facts(reg, capped)
        | {
            "winner": capped.winner,
            "end_reason": capped.end_reason.value if capped.end_reason else None,
        },
    }


def _value_error(f: Callable[[], object]) -> ValueError:
    try:
        f()
    except ValueError as exc:
        return exc
    raise SystemExit("no ValueError")


def build() -> str:
    reg = get_registry()
    records: list[dict[str, Json]] = []
    records += new_game_records(reg)
    records += error_records()
    records += problem_records()
    for gid, a, b, seed, kind in GAMES:
        policy = (
            first_option if kind == "first" else random_policy(derive_seed(MASTER, "policy", gid))
        )
        decisions, final, kept = play(reg, (deck(a), deck(b)), seed, policy, KEEP)
        records.append(
            {
                "t": "game",
                "id": gid,
                "decks": [deck_json(deck(a)), deck_json(deck(b))],
                "seed": str(seed),
                "policy": kind,
                "policy_seed": None if kind == "first" else str(derive_seed(MASTER, "policy", gid)),
                "decisions": decisions,
                "final": final,
            }
        )
        init = invariants.initial_multiset(kept[0])
        for k, st in sorted(kept.items()):
            sid = f"{gid}/d{k}" if k >= 0 else f"{gid}/final"
            records.append(state_record(reg, sid, st, init))
    base = new_game((deck("red-green-zeon"), deck("blue-white-federation")), 12345)
    apply(base, Action(ActionKind.GO_FIRST, 1))
    init = invariants.initial_multiset(base)
    records.append(state_record(reg, "broken/base", base, init, with_observe=False))
    for name, st in broken_states(base):
        records.append(state_record(reg, f"broken/{name}", st, init, with_observe=False))
    trimmed = base.clone()
    trimmed.decklists = (trimmed.decklists[0][:20], trimmed.decklists[1][5:])
    records.append(state_record(reg, "pools/short", trimmed, init))
    empty = base.clone()
    empty.decklists = ((), empty.decklists[1])
    empty.resource_decklists = ((), ())
    records.append(state_record(reg, "pools/empty", empty, init))
    records += scenario_records(reg)
    records += testkit_records(reg)
    return "".join(json.dumps(r, ensure_ascii=True, separators=(",", ":")) + "\n" for r in records)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Export the gcg-engine layer-5 test vectors.")
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
