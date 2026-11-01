"""Export canonical-state fixtures for the gcg-engine layer-1 tests (docs/port/PLAN.md §4.7-§4.8).

Plays seeded games with uniformly random legal actions (the fuzz decks, which jointly cover
the card pool, and the example decks), keeps every state that is the first to show a
serialisation feature (a decision kind, a frame-int key, a once-key form, a lasting filter, a
history kind, ...), and writes it with Python's own `canonical_state` and digests to
crates/gcg-engine/tests/fixtures/states.ndjson.

The first line is a table of the registry lookups that the canonical form needs (program,
continuous and filter-set content ids, frame-variable names, prompt strings and the prompt
instruction of every waiting frame). The Rust tests use it in place of the IR registry, so
the round trip is checked before gcg-effects lands. Each later line is a snapshot record in
the shape of PLAN §8.2 (`state`, `continuous`, `filter_sets`, `rules_checked`) plus
`digest` (`state_digest`) and `dump_sha256` (the SHA-256 of `json.dumps(canonical)` with
Python's default separators, which pins the key order).

    uv run python port/oracle/export_state_fixtures.py [--check]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections.abc import Iterator
from pathlib import Path
from typing import Any

from gcg_sim.deck.parse import load_deck, to_decklist
from gcg_sim.effects import dsl as d
from gcg_sim.effects import program as pr
from gcg_sim.effects.registry import Registry, get_registry
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import Battle, GameState, Lasting, TriggerInst
from gcg_sim.engine.types import EndReason
from gcg_sim.rng import SplitMix64, derive_seed
from gcg_sim.runner.digest import canonical_state, content_id, state_digest
from gcg_sim.tools.fuzz import deck_for

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "gcg-engine" / "tests" / "fixtures" / "states.ndjson"
DECKS = ROOT / "examples" / "decks"
MASTER = 20261101
FUZZ_GAMES = 1500
EXAMPLE_SEEDS = 6
ONCE_TAGS = (-7, -8)


def value_form(c: object) -> dict[str, Any] | None:
    """The IR node form of a value-shaped continuous effect (PLAN §3.3, §5.6), else None."""
    if isinstance(c, d.StatMod) and type(c.ap) is int and type(c.hp) is int:
        return {"_": "StatMod", "ap": c.ap, "hp": c.hp}
    if isinstance(c, d.KeywordGrant) and type(c.amount) is int:
        return {"_": "KeywordGrant", "keyword": c.keyword.value, "amount": c.amount}
    if isinstance(c, d.AbilityGrant):
        return {
            "_": "AbilityGrant",
            "card_number": c.card_number,
            "ability_index": c.ability_index,
        }
    return None


def once_kind(key: tuple[int, ...]) -> str:
    if len(key) >= 3 and key[0] in ONCE_TAGS:
        return "lasting-rule" if key[1] == -1 else "constant-rule"
    if key and key[0] == -9:
        return "rest-substitute"
    return f"len{len(key)}" + ("-tag" if key and key[0] < 0 else "")


def int_key_kind(k: str) -> str:
    for prefix in ("__loop", "__rest_as"):
        if k.startswith(prefix):
            return prefix
    return k


def waiting_instr(reg: Registry, st: GameState) -> tuple[int, int, object] | None:
    waiting = [f for f in st.frames if f.waiting]
    if not waiting:
        return None
    f = waiting[-1]
    return f.program_id, f.pc, reg.programs[f.program_id].instrs[f.pc]


def features(reg: Registry, st: GameState) -> set[str]:
    out: set[str] = {f"step:{st.step.name}", f"phase:{st.phase.name}"}
    if st.pending is None:
        out.add("pending:none")
    else:
        out.add(f"pending:{st.pending.kind.value}")
        if st.pending.context:
            out.add(f"pending:context{len(st.pending.context)}")
        if not st.pending.prompt.isascii():
            out.add("pending:non-ascii-prompt")
        w = waiting_instr(reg, st)
        if w is not None:
            out.add(f"prompt:{type(w[2]).__name__}")
    for f in st.frames:
        out.add(f"frame:{f.kind}")
        if f.event:
            out.add("frame:event")
        if f.vars:
            out.add(f"frame:vars{min(len(f.vars), 3)}")
        out.update(f"frame:int:{int_key_kind(k)}" for k in f.ints)
        if f.buf:
            out.add("frame:buf")
        if f.once_key:
            out.add("frame:once-key")
        for name, value in (("did", not f.did), ("answer", f.answer), ("acted", f.acted)):
            if value:
                out.add(f"frame:{name}")
    if len(st.frames) >= 2:
        out.add(f"frames:{min(len(st.frames), 3)}")
    for t in st.pending_triggers:
        out.add("trigger:pending")
        if t.burst:
            out.add("trigger:burst")
        if t.once_key:
            out.add("trigger:once-key")
        if t.ability_key[2]:
            out.add("trigger:unit-text")
        if len(t.event) >= 5:
            out.add("trigger:long-event")
    if st.batches:
        out.add(f"batches:{min(len(st.batches), 3)}")
        if any(not b for b in st.batches):
            out.add("batches:empty")
        if any(len(b) >= 2 for b in st.batches):
            out.add("batches:multi")
    for name, entries in (("lasting", st.lasting), ("delayed", st.delayed)):
        for le in entries:
            eff = reg.continuous[le.effect_key]
            form = "value" if value_form(eff) else "node"
            out.add(f"{name}:{type(eff).__name__}:{form}")
            out.add(f"{name}:{le.duration}")
            if le.filters_key >= 0:
                out.add(f"{name}:filters")
            if le.player >= 0:
                out.add(f"{name}:player")
            if le.aux >= 0:
                out.add(f"{name}:aux")
            if le.uses:
                out.add(f"{name}:uses")
            if len(le.targets) != 1:
                out.add(f"{name}:targets{min(len(le.targets), 2)}")
    out.update(f"once:{once_kind(k)}" for k in st.once_used)
    if len(st.once_used) >= 2:
        out.add("once:several")
    if st.effect_hits:
        out.add(f"effect-hits:{min(len(st.effect_hits), 2)}")
    for b in st.battles:
        out.add("battle")
        for name, value in (
            ("blocked", b.blocked),
            ("damage-only", b.damage_only),
            ("no-attack-triggers", not b.attack_triggers),
            ("ended", b.ended),
            ("player-target", b.target == -2),
        ):
            if value:
                out.add(f"battle:{name}")
    if st.winner is not None:
        out.add(f"winner:{st.winner}")
    if st.end_reason is not None:
        out.add(f"end:{st.end_reason.value}")
    out.update(f"redraw:{p}" for p in (0, 1) if st.redraws[p])
    out.update(f"history:{h.kind}" for h in st.history)
    for c in st.cards:
        if c.uid >= 120:
            out.add("card:token-or-late")
        if c.rested:
            out.add(f"card:rested:{c.zone.name}")
        if c.damage:
            out.add("card:damage")
        if c.pair >= 0:
            out.add(f"card:pair:{c.zone.name}")
        out.add(f"card:known{c.known}")
    for p in (0, 1):
        out.update(f"zone:{z}" for z, lst in enumerate(st.zones[p]) if lst)
    if st.priority:
        out.add("priority:1")
    if st.passes:
        out.add("passes")
    return out


def example_decks() -> list[tuple[str, DeckList]]:
    return [(p.stem, to_decklist(load_deck(p))) for p in sorted(DECKS.glob("*.txt"))]


def games() -> Iterator[tuple[str, tuple[DeckList, DeckList], int, SplitMix64]]:
    for i in range(FUZZ_GAMES):
        decks = (deck_for(2 * i, MASTER), deck_for(2 * i + 1, MASTER))
        seed = derive_seed(MASTER, "fixture-game", i)
        yield f"fuzz/g{i:03d}", decks, seed, SplitMix64(derive_seed(MASTER, "fixture-agent", i))
    named = example_decks()
    for a, (na, da) in enumerate(named):
        for b, (nb, db) in enumerate(named):
            if a == b:
                continue
            for s in range(EXAMPLE_SEEDS):
                seed = derive_seed(MASTER, "fixture-example", na, nb, s)
                rng = SplitMix64(derive_seed(MASTER, "fixture-example-agent", na, nb, s))
                yield f"example/{na}-vs-{nb}/s{s}", (da, db), seed, rng


class Tables:
    """The registry lookups of every selected snapshot, collected from the Python objects."""

    def __init__(self, reg: Registry) -> None:
        self.reg = reg
        self.programs: set[str] = set()
        self.continuous: set[str] = set()
        self.filter_sets: set[str] = set()
        self.var_names: set[str] = set()
        self.strings: set[str] = set()
        self.prompts: set[tuple[str, int, str, str]] = set()

    def program(self, pid: int) -> None:
        self.programs.add(content_id(self.reg.programs[pid]))

    def cont(self, key: int, side: dict[str, Any]) -> None:
        eff = self.reg.continuous[key]
        cid = content_id(eff)
        form = value_form(eff)
        if form is None:
            self.continuous.add(cid)
        else:
            side[cid] = form
            if form["_"] == "AbilityGrant":
                self.strings.add(str(form["card_number"]))

    def snapshot(self, st: GameState) -> dict[str, Any]:
        side: dict[str, Any] = {}
        for f in st.frames:
            self.program(f.program_id)
            self.var_names.update(f.vars)
            self.var_names.update(k for k in f.ints if k.startswith("__loop"))
        for t in (*st.pending_triggers, *(t for b in st.batches for t in b)):
            self.program(t.program_id)
        for le in (*st.lasting, *st.delayed):
            self.cont(le.effect_key, side)
            if le.filters_key >= 0:
                self.filter_sets.add(content_id(self.reg.filter_sets[le.filters_key]))
        for k in st.once_used:
            if len(k) >= 3 and k[0] in ONCE_TAGS and k[1] == -1:
                self.cont(k[2], side)
        w = waiting_instr(self.reg, st)
        if w is not None:
            pid, pc, ins = w
            cid = content_id(self.reg.programs[pid])
            if isinstance(ins, pr.AskMay):
                if ins.prompt:
                    self.strings.add(ins.prompt)
                    self.prompts.add((cid, pc, "ask", ins.prompt))
                else:
                    self.prompts.add((cid, pc, "you_may", ""))
            elif isinstance(ins, d.Choose):
                self.var_names.add(ins.var)
                self.prompts.add((cid, pc, "choose", ins.var))
        return dict(sorted(side.items()))

    def record(self) -> dict[str, Any]:
        return {
            "t": "tables",
            "v": 1,
            "def_count": len(self.reg.db),
            "programs": sorted(self.programs),
            "continuous": sorted(self.continuous),
            "filter_sets": sorted(self.filter_sets),
            "var_names": sorted(self.var_names),
            "strings": sorted(self.strings),
            "prompts": [list(p) for p in sorted(self.prompts)],
        }


def sha256(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def snapshot_record(
    reg: Registry, tables: Tables, st: GameState, sid: str, feats: list[str]
) -> dict[str, Any]:
    canonical = canonical_state(st, reg)
    return {
        "t": "snapshot",
        "id": sid,
        "state": canonical,
        "continuous": tables.snapshot(st),
        "filter_sets": {},
        "rules_checked": st._rules_checked,
        "digest": state_digest(st, reg),
        "dump_sha256": sha256(json.dumps(canonical)),
        "features": feats,
    }


RARE_SHAPES = [
    "battle:damage-only",
    "battle:ended",
    "battle:no-attack-triggers",
    "delayed:your_next_turn",
    "effect-hits:2",
    "end:turn_limit",
    "frame:int:__loop",
    "frame:int:__rest_as",
    "frame:int:__rest_i",
    "lasting:filters",
    "lasting:uses",
    "lasting:while_on_field",
    "once:lasting-rule-value",
    "once:rest-substitute",
    "once:sort-by-text",
    "trigger:burst",
    "trigger:optional",
    "winner:-1",
]


def rare_shapes(reg: Registry, base: GameState) -> GameState:
    """`base` with the serialisation shapes that random play reaches too rarely: whatever the
    engine would do with it, `canonical_state` is a pure function of it, so Python stays the
    oracle for every byte."""
    st = base.clone()
    f = st.frames[0]
    f.ints.update({"__loop3": 2, "__loop12": -1, "__rest_i": 1, "__rest_as10": 7, "__rest_as2": 8})
    f.vars["__loop3"] = (4, 5)
    f.vars["__todeck"] = ()
    mod = reg.cont_key(d.StatMod(ap=-3, hp=2))
    grant = reg.cont_key(d.AbilityGrant("GD01-001", 1))
    filters = reg.filters_key((d.IsDamaged(), d.HasBurst()))
    st.lasting.append(Lasting(mod, 1, 4, ((4, 1), (2, 3)), "while_on_field", 3, -1, 1, 2, filters))
    st.delayed.append(Lasting(grant, 0, 6, (), "your_next_turn", 2, 1, serial=-1))
    st.battles.append(
        Battle(7, 1, 9, 2, 1, 40, damage_only=True, attack_triggers=False, ended=True)
    )
    st.effect_hits = {5: 9, 2: 7}
    st.once_used |= {(-9, 3, 4), (10, 2, 1), (2, 1, 1), (-8, -1, mod, 3, 4, 1), (-7, 0, 5, 4, 1)}
    t = TriggerInst(0, 1, 5, 1, 5, (12, 34, 1), (("subject", 5), ("group", 2)), True, (34, 5, 1))
    t.optional = True
    st.pending_triggers.append(t)
    st.winner = -1
    st.end_reason = EndReason.TURN_LIMIT
    return st


def build() -> str:
    reg = get_registry()
    tables = Tables(reg)
    covered: set[str] = set()
    snapshots: list[dict[str, Any]] = []
    base: GameState | None = None
    for game_id, decks, seed, rng in games():
        st = new_game(decks, seed)
        k = 0
        while True:
            new = features(reg, st) - covered
            if new:
                covered |= new
                snapshots.append(
                    snapshot_record(reg, tables, st, f"{game_id}/d{k:04d}", sorted(new))
                )
                if base is None and st.frames and st.pending is not None:
                    base = st.clone()
            if st.winner is not None or st.pending is None:
                break
            opts = st.pending.options
            apply(st, opts[rng.randrange(len(opts))])
            k += 1
    if base is None:
        raise SystemExit("no decision state with a frame was reached")
    st = rare_shapes(reg, base)
    snapshots.append(snapshot_record(reg, tables, st, "synthetic/rare-shapes", RARE_SHAPES))
    cids = [content_id(p) for p in reg.programs]
    if len(set(cids)) != len(cids):
        raise SystemExit("program content ids are not unique (PLAN §4.8)")
    lines = [tables.record(), *snapshots]
    return "".join(json.dumps(r, ensure_ascii=True, separators=(",", ":")) + "\n" for r in lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Export canonical-state fixtures for gcg-engine.")
    ap.add_argument("--check", action="store_true", help="fail if the committed file is stale")
    args = ap.parse_args(argv)
    text = build()
    if args.check:
        if not OUT.exists() or OUT.read_text() != text:
            print(f"{OUT.relative_to(ROOT)} is stale: run {Path(__file__).name}", file=sys.stderr)
            return 1
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)}: {text.count(chr(10)) - 1} snapshots, {len(text)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
