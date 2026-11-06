"""Primitive vectors for gcg-engine layer L3 (docs/dev/PLAN.md §10.4 C, gate M5.3).

Wraps the functions of ``engine/core.py`` and ``engine/battle.py`` listed in ``TARGETS`` with
pass-through recorders, plays seeded games with uniformly random legal actions (vanilla decks,
keyword-only decks, the fuzz decks and the example decks), and records the outermost call of a
wrapped function: the state before (a canonical snapshot with the side tables of
``oracle/traces.py``), the arguments, the result or the exception, and the state after (a
delta of the canonical form against the state before, ``state_digest`` and ``_rules_checked``).
A call is kept when it shows a feature no kept call of its function showed (counters that
moved, history kinds appended, triggers queued, an error, the argument shape, ...), up to a
cap per function and source kind. Two built scenarios add the W-143 redirect chain and cycle.

The second source kind is the rule and card test suites (``tests/rules``, ``tests/cards``), run
in this process with the same recorders, which reach the targeted situations random play
rarely does. A test call is kept only when the state is one the embedded effect IR can rebuild
exactly: no test override is active (``testkit.card_text``, ``override_script``, the
``force_link`` patch of ``V.link_satisfied``), every program, continuous effect and filter of
the state is an IR program or node (or a value-shaped effect), and Python's cached view, if
any, equals a fresh computation (a test that wrote the state without ``touch`` leaves a stale
view, which a rebuilt state cannot have).

A vector's ``view`` is ``base`` when every derived view the call read is the base view
(``_compute`` takes its early exit: nothing lasting and no constant ability,
``engine/view.py:409-419``) and the call evaluated no non-empty filter list, no condition but
``And(())`` and no non-literal value; otherwise ``full``. The default writes the ``base``
vectors, which layer L3 replays over the base view alone (it landed before the view of layer
L2); ``--all-views`` adds the ``full`` vectors and the W-143 scenarios to the same file, which
is the fixture once layer L2 has landed (the M5.5 step). The Rust replay is the
``core::vectors`` module of ``crates/gcg-engine/src/core.rs``, which runs every vector of the
file.

The exporter only observes: it clones the state before each recorded call and reads states
without touching them; ``--check`` compares the regenerated file with the committed one.

    uv run python oracle/export_core_vectors.py [--all-views] [--check] [--out FILE]
"""

from __future__ import annotations

import argparse
import contextlib
import enum
import inspect
import io
import json
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import pytest
from export_state_fixtures import side_tables, value_form

from gcg_sim.cards.model import CardType
from gcg_sim.deck.parse import load_deck, to_decklist
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import Registry, get_registry
from gcg_sim.engine import battle as B
from gcg_sim.engine import core
from gcg_sim.engine import interp as I
from gcg_sim.engine import view as V
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import Battle, GameState
from gcg_sim.engine.types import Duration
from gcg_sim.rng import SplitMix64, derive_seed
from gcg_sim.runner.digest import canonical_state, content_id, state_digest
from gcg_sim.testkit import FILLER_RESOURCE, Scenario, vanilla_deck
from gcg_sim.tools.fuzz import deck_for

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates" / "gcg-engine" / "tests" / "fixtures"
OUT = FIXTURES / "core_vectors.ndjson"
IR_PATH = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
TEST_PATHS = ("tests/rules", "tests/cards")
DECKS = ROOT / "examples" / "decks"
MASTER = 20261102
COLORS = ("Blue", "Green", "Purple", "Red", "White")
FUZZ_GAMES = 160
PAIR_GAMES = 3
CAP_DEFAULT = 16
CAPS = {"core.move": 20, "core.emit": 32, "core.rules_management": 16, "battle.damage_step": 24}

Json = Any

TARGETS: dict[str, tuple[Any, tuple[str, ...]]] = {
    "core": (
        core,
        (
            "move",
            "remove_from_game",
            "new_card",
            "shuffle_deck",
            "emit",
            "ability_snapshot",
            "damage_card",
            "resolve_damage",
            "apply_damage",
            "shields_receiving_damage",
            "destroy_shields",
            "damage_shield_area",
            "destroy",
            "set_winner",
            "rules_management",
            "shield_area_protected",
            "shield_area_reduction",
        ),
    ),
    "battle": (
        B,
        (
            "declare_attack",
            "battle_broken",
            "eligible_blockers",
            "block",
            "damage_step",
            "end_battle",
            "damage_only_battle",
        ),
    ),
}


# ---------------------------------------------------------------------------------------------
# JSON encoding of arguments and results


def enc(x: object) -> Json:
    """An argument or result as JSON: enums are their values, a set is sorted, an LKI dict is
    ``[[uid, seq, [aid, ...]], ...]`` in insertion order, a ``Battle`` its ``to_json``."""
    if isinstance(x, bool) or x is None:
        return x
    if isinstance(x, enum.Enum):
        return x.value
    if isinstance(x, (int, str)):
        return x
    if isinstance(x, Battle):
        return x.to_json()
    if isinstance(x, core.Snapshot):
        return [x.seq, [a.aid for a in x.abilities]]
    if isinstance(x, dict):
        return [[k, *enc(v)] if isinstance(v, core.Snapshot) else [k, enc(v)] for k, v in x.items()]
    if isinstance(x, (set, frozenset)):
        return sorted(enc(v) for v in x)
    if isinstance(x, (list, tuple)):
        return [enc(v) for v in x]
    raise TypeError(f"cannot encode {type(x).__name__}")


def encode_args(sig: inspect.Signature, args: tuple[Any, ...], kwargs: dict[str, Any]) -> Json:
    ba = sig.bind(*args, **kwargs)
    ba.apply_defaults()
    out: dict[str, Json] = {}
    for k, v in ba.arguments.items():
        if k == "st":
            continue
        if k == "extra":
            out[k] = {kk: enc(vv) for kk, vv in sorted(v.items())}
        else:
            out[k] = enc(v)
    return out


def dump(x: Json) -> str:
    return json.dumps(x, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def delta(before: dict[str, Json], after: dict[str, Json]) -> dict[str, Json]:
    """The top-level keys of the canonical form that changed: ``{"set": [[i, v], ...]}`` for a
    list of unchanged length (the changed items), else ``{"value": v}``."""
    out: dict[str, Json] = {}
    for k, a in after.items():
        b = before[k]
        if dump(a) == dump(b):
            continue
        if isinstance(a, list) and isinstance(b, list) and len(a) == len(b):
            out[k] = {
                "set": [
                    [i, v] for i, (u, v) in enumerate(zip(b, a, strict=True)) if dump(u) != dump(v)
                ]
            }
        else:
            out[k] = {"value": a}
    return out


# ---------------------------------------------------------------------------------------------
# Capture


@dataclass
class Capture:
    reg: Registry
    all_views: bool
    depth: int = 0
    full: bool = False
    source: str = ""
    kind: str = "game"  # or "test"
    seen: set[tuple[Any, ...]] = field(default_factory=set)
    counts: dict[tuple[str, str, str], int] = field(default_factory=dict)
    vectors: list[dict[str, Json]] = field(default_factory=list)
    skipped: int = 0
    rebuildable: Rebuildable | None = None

    def keep(self, name: str, view: str, feats: tuple[Any, ...]) -> bool:
        if view == "full" and not self.all_views:
            return False
        key = (self.kind, name, view, feats)
        n = self.counts.get((self.kind, name, view), 0)
        return key not in self.seen and n < CAPS.get(name, CAP_DEFAULT)

    def kept(self, name: str, view: str, feats: tuple[Any, ...]) -> None:
        self.seen.add((self.kind, name, view, feats))
        k = (self.kind, name, view)
        self.counts[k] = self.counts.get(k, 0) + 1


@dataclass(frozen=True)
class Rebuildable:
    """What a state may name for the embedded IR to rebuild it (PLAN §4.8)."""

    entries: tuple[object, ...]
    link_satisfied: object
    programs: frozenset[str]
    nodes: frozenset[str]

    @staticmethod
    def of(reg: Registry) -> Rebuildable:
        ir = json.loads(IR_PATH.read_text())
        return Rebuildable(
            tuple(reg.cards),
            V.link_satisfied,
            frozenset(p["content_id"] for p in ir["programs"]),
            frozenset(ir["node_content_ids"]),
        )

    def holds(self, reg: Registry, st: GameState) -> bool:
        if len(reg.cards) != len(self.entries) or any(
            a is not b for a, b in zip(reg.cards, self.entries, strict=True)
        ):
            return False  # an override_script / card_text is active
        if V.link_satisfied is not self.link_satisfied:
            return False  # the force_link patch
        programs = [f.program_id for f in st.frames] + [
            t.program_id for t in (*st.pending_triggers, *(t for b in st.batches for t in b))
        ]
        if any(content_id(reg.programs[pid]) not in self.programs for pid in programs):
            return False
        effects = [reg.continuous[le.effect_key] for le in (*st.lasting, *st.delayed)]
        effects += [
            reg.continuous[k[2]]
            for k in st.once_used
            if len(k) >= 3 and k[0] in (core.PREVENT_ONCE_TAG, core.REDUCE_ONCE_TAG) and k[1] == -1
        ]
        if any(value_form(e) is None and content_id(e) not in self.nodes for e in effects):
            return False
        filters = [
            f
            for le in (*st.lasting, *st.delayed)
            if le.filters_key >= 0
            for f in reg.filter_sets[le.filters_key]
        ]
        return all(content_id(f) in self.nodes for f in filters) and not stale_view(st)


def stale_view(st: GameState) -> bool:
    """Python holds a cached view that a fresh computation of the state does not give."""
    cached = st._derived
    if cached is None or cached.key != V._view_key(st):
        return False
    fresh = ORIGINAL_COMPUTE(st)
    if not (fresh.same_values(cached) and fresh.linked == cached.linked):
        return True
    if cached.by_event is None:
        return False
    idx = V.trigger_index(st.clone(), fresh)
    keyed = {ev: [(h, a.aid) for h, a in v] for ev, v in idx.items()}
    return keyed != {ev: [(h, a.aid) for h, a in v] for ev, v in cached.by_event.items()}


ORIGINAL_COMPUTE = V._compute


class Active:
    """The capture the installed wrappers record into."""

    capture: Capture | None = None


def cap() -> Capture:
    assert Active.capture is not None
    return Active.capture


def bucket(n: int, top: int) -> int:
    return max(-top, min(top, n))


def call_features(
    name: str, args: Json, before: GameState, st: GameState, result: object, err: str | None
) -> tuple[Any, ...]:
    hist = tuple(sorted({h.kind for h in st.history[len(before.history) :]}))
    common = (
        err,
        bucket(st.version - before.version, 6),
        bucket(st.event_group - before.event_group, 6),
        bucket(len(st.pending_triggers) - len(before.pending_triggers), 4),
        hist,
        st.winner is not None,
        dump(sorted(st.effect_hits.items())) != dump(sorted(before.effect_hits.items())),
        len(st.once_used) != len(before.once_used),
        st._rules_checked != before._rules_checked,
    )
    extra: tuple[Any, ...] = ()
    if name == "core.move":
        c = before.cards[args["uid"]]
        extra = (
            int(c.zone),
            args["dst"],
            args["bottom"],
            args["reveal"],
            args["rested"],
            c.pair >= 0,
            core.card_type(before, args["uid"]).is_token,
        )
    elif name == "core.emit":
        extra = (args["ev"], bool(args["lki"]), tuple(sorted(args["extra"])))
    elif name in ("core.destroy", "core.destroy_shields", "core.shields_receiving_damage"):
        extra = (min(len(args["uids"]), 3), args.get("battle"))
    elif name in ("battle.eligible_blockers", "battle.battle_broken"):
        extra = (dump(enc(result)) if not isinstance(result, list) else min(len(result), 3),)
    elif name == "battle.damage_step":
        b = before.battle
        extra = (b.target if b is not None and b.target < 0 else "unit",)
    return common + extra


def install(c: Capture) -> None:
    Active.capture = c
    for prefix, (mod, names) in TARGETS.items():
        for name in names:
            orig = getattr(mod, name)
            setattr(mod, name, recorder(f"{prefix}.{name}", orig))
    for name, check in (
        ("derived", _derived_check),
        ("matches", _matches_check),
        ("cond", _cond_check),
        ("value", _value_check),
    ):
        setattr(V, name, check(getattr(V, name)))


def recorder(name: str, orig: Callable[..., Any]) -> Callable[..., Any]:
    sig = inspect.signature(orig)

    def wrapped(*args: Any, **kwargs: Any) -> Any:
        c = cap()
        if c.depth > 0:
            c.depth += 1
            try:
                return orig(*args, **kwargs)
            finally:
                c.depth -= 1
        st: GameState = args[0]
        before = st.clone()
        enc_args = encode_args(sig, args, kwargs)
        c.depth += 1
        c.full = False
        result: object = None
        error: BaseException | None = None
        try:
            result = orig(*args, **kwargs)
        except Exception as exc:  # recorded, then re-raised unchanged
            error = exc
        finally:
            c.depth -= 1
        err = type(error).__name__ if error is not None else None
        view = "full" if c.full else "base"
        feats = call_features(name, enc_args, before, st, result, err)
        if c.keep(name, view, feats):
            if c.kind == "test" and not (c.rebuildable and c.rebuildable.holds(c.reg, before)):
                c.skipped += 1
            else:
                c.kept(name, view, feats)
                c.vectors.append(vector(c, name, view, enc_args, before, st, result, error))
        if error is not None:
            raise error
        return result

    return wrapped


def _derived_check(orig: Callable[[GameState], V.Derived]) -> Callable[[GameState], V.Derived]:
    """Every view a call reads, computed or cached (a cached view equals a fresh compute of the
    state, as the `--nocache` corpus check establishes, PLAN §8.5)."""

    def wrapped(st: GameState) -> V.Derived:
        dv = orig(st)
        if st.lasting or any(
            isinstance(a.ability, d.Constant) for abil in dv.abilities.values() for a in abil
        ):
            cap().full = True
        return dv

    return wrapped


def _matches_check(orig: Callable[..., bool]) -> Callable[..., bool]:
    def wrapped(st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, filters: Any) -> bool:
        if filters:
            cap().full = True
        return orig(st, dv, ctx, uid, filters)

    return wrapped


def _cond_check(orig: Callable[..., bool]) -> Callable[..., bool]:
    def wrapped(st: GameState, dv: V.Derived, ctx: V.Ctx, c: Any, did: bool = True) -> bool:
        if not (isinstance(c, d.And) and not c.conds):
            cap().full = True
        return orig(st, dv, ctx, c, did)

    return wrapped


def _value_check(orig: Callable[..., int]) -> Callable[..., int]:
    def wrapped(st: GameState, dv: V.Derived, ctx: V.Ctx, v: Any) -> int:
        if type(v) is not int:
            cap().full = True
        return orig(st, dv, ctx, v)

    return wrapped


def vector(
    c: Capture,
    name: str,
    view: str,
    args: Json,
    before: GameState,
    st: GameState,
    result: object,
    error: BaseException | None,
) -> dict[str, Json]:
    reg = c.reg
    b = canonical_state(before, reg)
    a = canonical_state(st, reg)
    cont, fsets = side_tables(before, reg)
    return {
        "id": f"{c.source}/{len(c.vectors):04d}",
        "fn": name,
        "view": view,
        "args": args,
        "before": {
            "state": b,
            "continuous": cont,
            "filter_sets": fsets,
            "rules_checked": before._rules_checked,
        },
        "after": {
            "delta": delta(b, a),
            "digest": state_digest(st, reg),
            "rules_checked": st._rules_checked,
        },
        "result": None if error is not None else enc(result),
        "error": None if error is None else {"type": type(error).__name__},
    }


# ---------------------------------------------------------------------------------------------
# Games and scenarios


def keyword_deck(reg: Registry, color: str) -> DeckList:
    """50 cards of one colour whose scripts hold only keyword abilities (Units and Pilots),
    four copies each, filled up with the colour's vanilla Units."""
    db = reg.db
    picks: list[str] = []
    for cdef in db.real_cards():
        if cdef.color is None or cdef.color.value != color:
            continue
        if cdef.card_type not in (CardType.UNIT, CardType.PILOT):
            continue
        e = reg.cards[cdef.def_id]
        aids = (*e.own, *e.unit)
        if e.script is None or not aids:
            continue
        if all(isinstance(reg.abilities[a].ability, d.Keyword) for a in aids):
            picks.append(cdef.card_number)
    main = [n for n in sorted(picks) for _ in range(4)][:30]
    main += list(vanilla_deck(color).main)[: 50 - len(main)]
    return DeckList(tuple(main), (FILLER_RESOURCE,) * 10)


def games(reg: Registry) -> Iterator[tuple[str, tuple[DeckList, DeckList], int, SplitMix64]]:
    def keyword(color: str) -> DeckList:
        return keyword_deck(reg, color)

    builders: tuple[tuple[str, Callable[[str], DeckList]], ...] = (
        ("vanilla", vanilla_deck),
        ("keyword", keyword),
    )
    for kind, build_deck in builders:
        for c0 in COLORS:
            for c1 in COLORS:
                if c0 == c1:
                    continue
                for g in range(PAIR_GAMES):
                    seed = derive_seed(MASTER, kind, c0, c1, g)
                    yield (
                        f"{kind}/{c0.lower()}-{c1.lower()}/g{g}",
                        (build_deck(c0), build_deck(c1)),
                        seed,
                        SplitMix64(derive_seed(seed, "agent")),
                    )
    for i in range(FUZZ_GAMES):
        seed = derive_seed(MASTER, "fuzz-game", i)
        yield (
            f"fuzz/g{i:02d}",
            (deck_for(2 * i, MASTER), deck_for(2 * i + 1, MASTER)),
            seed,
            SplitMix64(derive_seed(seed, "agent")),
        )
    named = [(p.stem, to_decklist(load_deck(p))) for p in sorted(DECKS.glob("*.txt"))]
    for na, da in named:
        for nb, db in named:
            if na == nb:
                continue
            for g in range(PAIR_GAMES):
                seed = derive_seed(MASTER, "example", na, nb, g)
                rng = SplitMix64(derive_seed(seed, "agent"))
                yield f"example/{na}-vs-{nb}/g{g}", (da, db), seed, rng


def redirect_scenarios(c: Capture) -> None:
    """W-143 (docs/dev/WEAKNESSES.md): REDIRECT_BATTLE_DAMAGE lasting effects (the GD04-095
    rule, `effects/bindings/wp_gd04_b.py:153-163`) from A to B (a chain) and, in a second
    state, from A to B and from B to A (a cycle, Python's `RecursionError`)."""
    rule = d.RuleGrant(d.RuleMod(d.RuleKind.REDIRECT_BATTLE_DAMAGE))
    for cyclic in (False, True):
        sc = Scenario(active=1)
        a = sc.add(0, "GD01-060")
        b = sc.add(0, "GD01-060")
        e = sc.add(1, "GD01-060")
        sc.start()
        st = sc.st
        seq = {u: st.cards[u].zone_seq for u in (a, b)}
        I.add_lasting(st, rule, 0, b, ((a, seq[a]),), Duration.THIS_TURN, aux=b)
        if cyclic:
            I.add_lasting(st, rule, 0, a, ((b, seq[b]),), Duration.THIS_TURN, aux=a)
        c.source = f"scenario/redirect-{'cycle' if cyclic else 'chain'}"
        before = len(c.vectors)
        with contextlib.suppress(RecursionError):
            core.resolve_damage(st, a, 3, source=e, battle=True, by=1)
        if len(c.vectors) != before + 1:
            raise SystemExit(f"{c.source}: the resolve_damage call was not recorded")


class TestSource:
    """A pytest plugin naming each test's vectors after the test."""

    def __init__(self, c: Capture) -> None:
        self.c = c
        self.failed: list[str] = []

    @pytest.hookimpl(tryfirst=True)
    def pytest_runtest_setup(self, item: pytest.Item) -> None:
        self.c.source = item.nodeid

    def pytest_runtest_logreport(self, report: pytest.TestReport) -> None:
        if report.failed:
            self.failed.append(report.nodeid)


def test_vectors(c: Capture) -> None:
    """The recorders over the rule and card suites; the suites must pass under them."""
    c.kind = "test"
    plugin = TestSource(c)
    args = [*(str(ROOT / t) for t in TEST_PATHS), "-q", "-p", "no:cacheprovider", "--no-header"]
    with contextlib.redirect_stdout(io.StringIO()):
        rc = pytest.main(args, plugins=[plugin])
    if rc != 0 or plugin.failed:
        raise SystemExit(f"the test suites failed under the recorders: {plugin.failed[:5]}")


def build(all_views: bool) -> str:
    sys.setrecursionlimit(1000)
    reg = get_registry()
    c = Capture(reg, all_views, rebuildable=Rebuildable.of(reg))
    install(c)
    for source, decks, seed, rng in games(reg):
        c.source = source
        st = new_game(decks, seed)
        while st.winner is None and st.pending is not None:
            opts = st.pending.options
            apply(st, opts[rng.randrange(len(opts))])
    if all_views:
        redirect_scenarios(c)
    test_vectors(c)
    print(f"{c.skipped} test calls not rebuildable from the IR", file=sys.stderr)
    return "".join(dump(v) + "\n" for v in c.vectors)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Export gcg-engine layer-3 primitive vectors.")
    ap.add_argument("--all-views", action="store_true", help="also write the `full` vectors")
    ap.add_argument("--check", action="store_true", help="fail if the output file is stale")
    ap.add_argument("--out", type=Path, default=OUT, help="the output file (default: the fixture)")
    args = ap.parse_args(argv)
    out: Path = args.out.resolve()
    text = build(args.all_views)
    if args.check:
        if not out.exists() or out.read_text() != text:
            print(f"{out} is stale: run {Path(__file__).name}", file=sys.stderr)
            return 1
        return 0
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text)
    print(f"wrote {out}: {text.count(chr(10))} vectors, {len(text)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
