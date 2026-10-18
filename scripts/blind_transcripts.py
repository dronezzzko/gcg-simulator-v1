"""Blinded transcripts of saved games for a replay audit of the AI (docs/AI.md, "Strength").

Each replay is re-played from its seed and actions; every decision with more than one option is
printed with the full board (both hands; the non-decider's is marked hidden from the decider),
all legal options and the move played. Search statistics, agent names and versions are left out,
and files are named by a hash of their content, so auditors cannot tell which AI played. The key
mapping ids to source files is written separately.

The 2026-10 audit used five BO1 games per example pairing for each AI version, from a checkout
of each version, with ``--seed 90031``, ``90032``, ``90033`` for the three pairings in the order
red-green-zeon/blue-white-federation, blue-white-federation/purple-white-tekkadan,
purple-white-tekkadan/red-green-zeon:

    uv run gcg-sim benchmark A.txt B.txt --format bo1 --matches 5 --seed S --workers 2 \
        --decision-log --replays 5 --out OUT/A-vs-B

    uv run python scripts/blind_transcripts.py OUT_DIR KEY_FILE RUN_DIR [RUN_DIR ...]
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from gcg_sim.engine import view as V
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import PLAYER_TARGET, Zone
from gcg_sim.runner.replayfile import _decklist, describe_action


def label(st: GameState, uid: int) -> str:
    if uid == PLAYER_TARGET:
        return "player"
    if not 0 <= uid < len(st.cards):
        return f"#{uid}"
    cd = V.cdef(st, uid)
    return f"{cd.card_number} {cd.name}[{uid}]"


def unit(st: GameState, dv: V.Derived, u: int) -> str:
    c = st.cards[u]
    cd = V.cdef(st, u)
    kws = ",".join(sorted(k.value for k in dv.kw.get(u, ())))
    pilot = ""
    if c.pair >= 0:
        pilot = f" +{V.cdef(st, c.pair).name}{'(linked)' if u in dv.linked else ''}"
    hp = V.hp_of(st, dv, u)
    return (
        f"[{u}] {cd.card_number} {cd.name}{pilot} Lv{cd.level} AP{V.ap_of(st, dv, u)} "
        f"HP{hp - c.damage}/{hp} {'rested' if c.rested else 'active'}"
        f"{' (deployed this turn)' if c.entered_turn == st.turn else ''}"
        f"{' {' + kws + '}' if kws else ''}"
    )


def board(st: GameState, decider: int) -> list[str]:
    dv = V.derived(st)
    out = []
    for p in (decider, 1 - decider):
        z = st.zones[p]
        res = z[Zone.RESOURCE_AREA]
        active = sum(1 for u in res if not st.cards[u].rested)
        ex = sum(1 for u in res if V.cdef(st, u).card_type.value == "ex_resource")
        out.append(
            f"  P{p} ({'decider' if p == decider else 'opponent'}): shields {len(z[Zone.SHIELD])}, "
            f"resources {active}/{len(res)} active, EX {ex}, deck {len(z[Zone.DECK])}, "
            f"trash {len(z[Zone.TRASH])}"
        )
        out += [f"    base: {unit(st, dv, u)}" for u in z[Zone.BASE]]
        out += [f"    unit: {unit(st, dv, u)}" for u in z[Zone.BATTLE]]
        hidden = " (hidden from the decider)" if p != decider else ""
        out.append(f"    hand{hidden}: {', '.join(label(st, u) for u in z[Zone.HAND]) or '-'}")
    return out


def transcript(doc: dict[str, Any]) -> list[str]:
    seats = sorted(doc["seats"], key=lambda s: int(s["seat"]))
    decks = (_decklist(seats[0]), _decklist(seats[1]))
    st = new_game(
        decks,
        int(doc["seed"]),
        chooser=doc["chooser"],
        turn_limit=doc["turn_limit"],
        max_actions=doc["max_actions"],
    )
    lines: list[str] = []
    for i, raw in enumerate(doc["actions"]):
        dec = st.pending
        assert dec is not None
        action = Action.from_json(raw)
        head = f"#{i} turn {st.turn} P{dec.player} {dec.kind.value}"
        if len(dec.options) > 1:
            lines.append(f"\n{head}" + (f" prompt: {dec.prompt}" if dec.prompt else ""))
            lines += board(st, dec.player)
            options = sorted({describe_action(st, dec, o) for o in dec.options})
            lines.append("  options: " + " | ".join(options))
            lines.append(f"  PLAYED: {describe_action(st, dec, action)}")
        else:
            lines.append(f"{head}: {describe_action(st, dec, action)} (only option)")
        apply(st, action)
    lines.append(f"\nGAME OVER: winner P{st.winner} on turn {st.turn} ({st.end_reason})")
    return lines


def main(argv: list[str]) -> None:
    out, key_file, *runs = argv
    Path(out).mkdir(parents=True, exist_ok=True)
    key: dict[str, str] = {}
    for run in runs:
        for path in sorted(Path(run).rglob("replays/*.json")):
            doc = json.loads(path.read_text(encoding="utf-8"))
            digest = hashlib.sha256(json.dumps(doc["actions"]).encode() + str(path).encode())
            gid = digest.hexdigest()[:8]
            text = "\n".join(transcript(doc)) + "\n"
            Path(out, f"game-{gid}.txt").write_text(text, encoding="utf-8")
            key[gid] = str(path)
    Path(key_file).write_text(json.dumps(key, indent=1) + "\n", encoding="utf-8")
    print(f"{len(key)} transcripts in {out}; key in {key_file}")


if __name__ == "__main__":
    main(sys.argv[1:])
