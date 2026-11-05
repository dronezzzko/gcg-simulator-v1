"""W-17 step (3): the blinded audit packets of the shipped-preset sample (docs/port/WEAKNESSES.md W-17).

The sample is 20 BO1 games per example pairing for each AI version (`runs/search-standard`, the
shipped strengthened `standard` of M15b, and the reference `standard` of
`docs/port/eval/GAP-A1/audit/runs/mcts-standard`, both seats the same version). Its blinded
transcripts come from `gcg-eval transcripts` (W-119): no agent names, no search statistics and
ids derived from the game alone.

The auditors get two kinds of material, neither naming a version:

- full transcripts of a stratified sub-sample: per version and pairing, the lowest-numbered
  game the deck under test won and the lowest-numbered game it lost (12 games);
- for every other game that the rule-based audit (`audit-report.json`, `gcg-eval weakness
  audit`) flagged, an excerpt with every decision of the flagged turn, naming the player whose
  decisions to check but not the flag.

Packets are shuffled by SHA-256 of their file names and dealt to auditors in order. The private
assignment (packet to game id) is written next to the packets; the version of a game id is only
in the transcripts' key file, which the auditors never see.

    uv run python docs/port/eval/final-2/audit/prepare_audit.py TRANSCRIPTS_DIR KEY_FILE OUT_DIR
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPORT = HERE / "audit-report.json"
FULL_PER_AUDITOR = 4
EXCERPTS_PER_AUDITOR = 13
BLOCK = re.compile(r"^#\d+ turn (\d+) P(\d)")


def version_of(path: str) -> str:
    if "/search-standard/" in path:
        return "search-standard"
    if "/mcts-standard/" in path:
        return "mcts-standard"
    raise ValueError(f"unknown version in {path}")


def parse_replay_path(path: str) -> tuple[str, str, str, int]:
    parts = Path(path).parts
    pairing = parts[-3]
    m = re.fullmatch(r"(win|loss|draw)-m(\d+)-g1\.json", parts[-1])
    if m is None:
        raise ValueError(f"unexpected replay name {path}")
    return version_of(path), pairing, m.group(1), int(m.group(2))


def blocks(text: str) -> list[tuple[int, int, str]]:
    """The transcript's decision blocks as (turn, player, text)."""
    out: list[tuple[int, int, str]] = []
    current: list[str] = []
    head: tuple[int, int] | None = None
    for line in text.splitlines():
        m = BLOCK.match(line)
        if m:
            if head is not None:
                out.append((head[0], head[1], "\n".join(current).rstrip()))
            head = (int(m.group(1)), int(m.group(2)))
            current = [line]
        elif head is not None:
            if line.startswith("GAME OVER"):
                break
            current.append(line)
    if head is not None:
        out.append((head[0], head[1], "\n".join(current).rstrip()))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("transcripts", type=Path)
    ap.add_argument("key", type=Path)
    ap.add_argument("out", type=Path)
    args = ap.parse_args()

    key: dict[str, list[str]] = json.loads(args.key.read_text())
    by_path = {paths[0]: gid for gid, paths in key.items()}
    meta = {gid: parse_replay_path(paths[0]) for gid, paths in key.items()}

    full: list[str] = []
    for version in ("search-standard", "mcts-standard"):
        for pairing in sorted({m[1] for m in meta.values()}):
            for result in ("win", "loss"):
                cands = sorted(
                    (m[3], gid)
                    for gid, m in meta.items()
                    if m[0] == version and m[1] == pairing and m[2] == result
                )
                if cands:
                    full.append(cands[0][1])

    report = json.loads(REPORT.read_text())
    excerpts: list[tuple[str, int, int]] = []
    for run in report["runs"]:
        for g in run["flagged_games"]:
            path = f"{run['run']}/replays/{g['file']}"
            gid = by_path[path]
            if gid in full:
                continue
            flags = {(f[0], f[1]) for kind in ("missed_lethal", "wasted_effect") for f in g[kind]}
            flags |= {(f[0], f[1]) for f in g["self_harm_target"]}
            excerpts.extend((gid, p, t) for p, t in sorted(flags))

    packets: list[tuple[str, str, dict[str, object]]] = []
    for gid in full:
        text = (args.transcripts / f"game-{gid}.txt").read_text()
        packets.append((f"game-{gid}.txt", text, {"kind": "full", "game": gid}))
    for gid, player, turn in excerpts:
        text = (args.transcripts / f"game-{gid}.txt").read_text()
        body = [b for t, _, b in blocks(text) if t == turn]
        if not body:
            raise SystemExit(f"no decision of turn {turn} in game {gid}")
        name = f"excerpt-{gid}-t{turn}-p{player}.txt"
        header = (
            f"Excerpt of game {gid}: every decision of turn {turn}. "
            f"Check the decisions of P{player}.\n\n"
        )
        packets.append(
            (
                name,
                header + "\n\n".join(body) + "\n",
                {"kind": "excerpt", "game": gid, "turn": turn, "player": player},
            )
        )

    def order(p: tuple[str, str, dict[str, object]]) -> str:
        return hashlib.sha256(p[0].encode()).hexdigest()

    fulls = sorted((p for p in packets if p[2]["kind"] == "full"), key=order)
    parts = sorted((p for p in packets if p[2]["kind"] == "excerpt"), key=order)
    auditors: list[list[tuple[str, str, dict[str, object]]]] = []
    for i in range(0, len(fulls), FULL_PER_AUDITOR):
        auditors.append(fulls[i : i + FULL_PER_AUDITOR])
    for i in range(0, len(parts), EXCERPTS_PER_AUDITOR):
        auditors.append(parts[i : i + EXCERPTS_PER_AUDITOR])

    args.out.mkdir(parents=True, exist_ok=False)
    assignment: dict[str, list[dict[str, object]]] = {}
    for n, items in enumerate(auditors, 1):
        d = args.out / f"auditor-{n}"
        d.mkdir()
        assignment[d.name] = []
        for name, text, info in items:
            (d / name).write_text(text)
            assignment[d.name].append({"file": name, **info})
    (args.out / "assignment.json").write_text(
        json.dumps(assignment, indent=1, sort_keys=True) + "\n"
    )
    print(
        f"{len(fulls)} full transcripts and {len(parts)} excerpts for {len(auditors)} auditors in {args.out}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
