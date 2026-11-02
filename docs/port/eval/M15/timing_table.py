"""M15: the cost table of docs/port/eval/M15/README.md from timing/timing.tsv.

Each line of the TSV is one `/usr/bin/time -l` run of the Part 2 workload (`benchmark
examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt --format bo3
--seed 1`) under the load of a shared machine: `gcg-eval preset-benchmark --search-config` for
a candidate, `gcg-eval reference-benchmark` for the reference. Wall times are upper bounds of an
idle machine's; instructions retired do not depend on the load.

Usage: uv run python docs/port/eval/M15/timing_table.py [TSV]
"""

from __future__ import annotations

import csv
import sys
from pathlib import Path


def main() -> None:
    tsv = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else Path(__file__).with_name("timing") / "timing.tsv"
    )
    rows = list(csv.DictReader(tsv.open(), delimiter="\t"))
    base = {}
    for r in rows:
        if r["preset"].startswith("ref-") and (r["preset"], r["mode"]) not in base:
            base[(r["preset"], r["mode"])] = r
    print(
        "| run | scale | matches / workers | games | wall s (load) | user s | user s per game | "
        "instructions per game (G) | instructions vs reference |"
    )
    print("| --- | ---: | --- | ---: | --- | ---: | ---: | ---: | ---: |")
    for r in rows:
        games = int(r["games"])
        instr = int(r["instructions"])
        ref_name = "ref-strong" if r["preset"] in ("standard", "ref-strong") else "ref-standard"
        ref = base.get((ref_name, r["mode"]))
        ratio = "—"
        if ref is not None and not r["preset"].startswith("ref-"):
            ratio = f"{(instr / games) / (int(ref['instructions']) / int(ref['games'])):.2f}x"
        print(
            f"| {r['preset']} | {r['scale']} | {r['matches']} / {r['workers']} | {games} | "
            f"{float(r['wall_s']):.1f} ({float(r['load1_before']):.0f}) | {float(r['user_s']):.0f} | "
            f"{float(r['user_s']) / games:.2f} | {instr / games / 1e9:.1f} | {ratio} |"
        )


if __name__ == "__main__":
    main()
