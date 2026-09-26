#!/usr/bin/env python3
"""Compare a fetched gcg-api clone with the packaged snapshot (thin wrapper).

    uv run python .claude/skills/gcg-refresh-data/scripts/diff_snapshot.py --new-data DIR \\
        [--old-data src/gcg_sim/data/gcgapi] [--json OUT]

Same as ``uv run python -m gcg_sim.tools.refresh diff``; run it through ``uv run`` so the
project package is importable.
"""

from __future__ import annotations

import sys

from gcg_sim.tools.refresh import main

if __name__ == "__main__":
    raise SystemExit(main(["diff", *sys.argv[1:]]))
