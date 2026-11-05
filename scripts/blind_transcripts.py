"""Blinded transcripts of saved games for a replay audit of the AI (docs/python/AI.md, "Strength").

Each replay is first checked with ``gcg_sim.runner.replayfile.check_replay`` (it must reproduce
its recorded outcome), then re-played from its seed and actions; every decision with more than
one option is printed with the full board (both hands; the non-decider's is marked hidden from
the decider), all legal options and the move played. Search statistics, agent names, agent
seeds, decision logs and versions are left out, and files are named by a hash of the game
alone (seed, chooser, caps, both decklists and the actions; docs/dev/WEAKNESSES.md W-119), so
auditors cannot tell which AI played and the same game gets the same id wherever its replay
file lives. The key mapping ids to source files is written separately; a game saved twice
lists both files.

The 2026-10 audit used five BO1 games per example pairing for each AI version, from a checkout
of each version, with ``--seed 90031``, ``90032``, ``90033`` for the three pairings in the order
red-green-zeon/blue-white-federation, blue-white-federation/purple-white-tekkadan,
purple-white-tekkadan/red-green-zeon:

    uv run gcg-sim benchmark A.txt B.txt --format bo1 --matches 5 --seed S --workers 2 \
        --decision-log --replays 5 --out OUT/A-vs-B

    uv run python scripts/blind_transcripts.py OUT_DIR KEY_FILE RUN_DIR [RUN_DIR ...]
    uv run python scripts/blind_transcripts.py --selftest
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
import tempfile
from collections.abc import Mapping
from pathlib import Path
from typing import Any

from gcg_sim.cli import main as gcg_sim_main
from gcg_sim.engine import view as V
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import PLAYER_TARGET, Zone
from gcg_sim.runner.replayfile import check_replay, describe_action

ROOT = Path(__file__).resolve().parents[1]
# The replay fields that define the game; agents, seeds of agents, logs and labels stay out.
GAME_FIELDS = ("seed", "chooser", "turn_limit", "max_actions", "actions")


def decklist(seat: Mapping[str, Any]) -> DeckList:
    """A replay seat's ``main``/``resources`` counts as a sorted ``DeckList``."""

    def expand(counts: Mapping[str, int]) -> tuple[str, ...]:
        return tuple(sorted(n for n, c in counts.items() for _ in range(int(c))))

    return DeckList(main=expand(seat["main"]), resources=expand(seat["resources"]))


def game_id(doc: Mapping[str, Any]) -> str:
    """A hash of the game alone: the same game has the same id in any directory."""
    seats = sorted(doc["seats"], key=lambda s: int(s["seat"]))
    content = {k: doc[k] for k in GAME_FIELDS}
    content["decks"] = [{"main": s["main"], "resources": s["resources"]} for s in seats]
    text = json.dumps(content, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
    return hashlib.sha256(text.encode()).hexdigest()[:12]


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


def transcript(doc: Mapping[str, Any]) -> list[str]:
    seats = sorted(doc["seats"], key=lambda s: int(s["seat"]))
    decks = (decklist(seats[0]), decklist(seats[1]))
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


def write_transcripts(out: Path, key_file: Path, runs: list[Path]) -> dict[str, list[str]]:
    out.mkdir(parents=True, exist_ok=True)
    key: dict[str, list[str]] = {}
    for run in runs:
        for path in sorted(run.rglob("replays/*.json")):
            doc = json.loads(path.read_text(encoding="utf-8"))
            check = check_replay(doc)
            if not check.ok:
                raise SystemExit(f"{path}: the replay does not reproduce: {check.mismatches}")
            gid = game_id(doc)
            if gid not in key:
                text = "\n".join(transcript(doc)) + "\n"
                Path(out, f"game-{gid}.txt").write_text(text, encoding="utf-8")
            key.setdefault(gid, []).append(str(path))
    key_file.write_text(json.dumps(key, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    return key


# Words of the decision log and the agent descriptions that a blinded transcript never holds.
LEAKS = ("visits", "iterations", "mean", "alternatives", "mcts", "search", "greedy", "agent")


def selftest() -> int:
    """W-119: the same replay in two directories gets one id, and transcripts hold no search
    statistics or agent names."""
    problems = []
    with tempfile.TemporaryDirectory(prefix="blind-transcripts-") as tmp:
        base = Path(tmp)
        run = base / "run"
        code = gcg_sim_main(
            [
                "benchmark",
                str(ROOT / "examples/decks/red-green-zeon.txt"),
                str(ROOT / "examples/decks/blue-white-federation.txt"),
                "--format",
                "bo1",
                "--matches",
                "1",
                "--seed",
                "5",
                "--ai-preset",
                "fast",
                "--decision-log",
                "--replays",
                "1",
                "--out",
                str(run),
            ]
        )
        replays = sorted(run.rglob("replays/*.json"))
        if code != 0 or not replays:
            problems.append(f"the benchmark gave exit {code} and {len(replays)} replays")
        else:
            doc = json.loads(replays[0].read_text(encoding="utf-8"))
            if not doc.get("decision_log"):
                problems.append("the fixture replay has no decision log to leak")
            for d in ("a/deep/dir", "b"):
                dst = base / "runs" / d / "replays"
                dst.mkdir(parents=True)
                shutil.copy(replays[0], dst / "renamed.json")
            key = write_transcripts(base / "out", base / "key.json", [base / "runs"])
            if len(key) != 1 or len(next(iter(key.values()))) != 2:
                problems.append(f"one game in two directories gave the key {key}")
            texts = sorted((base / "out").glob("game-*.txt"))
            body = texts[0].read_text(encoding="utf-8").lower() if texts else ""
            if "game over" not in body or "played:" not in body:
                problems.append("the transcript has no moves")
            problems += [f"the transcript names {w!r}" for w in LEAKS if w in body]
            moved = dict(doc, seats=[dict(s, agent="other", agent_seed=0) for s in doc["seats"]])
            if game_id(moved) != game_id(doc):
                problems.append("the id depends on the agents")
            if game_id(dict(doc, actions=doc["actions"][:-1])) == game_id(doc):
                problems.append("the id ignores the actions")
    for p in problems:
        print(f"selftest: {p}", file=sys.stderr)
    print(f"blind_transcripts selftest {'failed' if problems else 'passed'}")
    return 1 if problems else 0


def main(argv: list[str]) -> int:
    if argv == ["--selftest"]:
        return selftest()
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    out, key_file, *runs = argv
    key = write_transcripts(Path(out), Path(key_file), [Path(r) for r in runs])
    print(f"{len(key)} transcripts in {out}; key in {key_file}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
