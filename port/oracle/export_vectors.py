"""Export reference vectors from Python for the Rust gcg-core property tests.

Writes crates/gcg-core/tests/vectors.json: SplitMix64 streams, randrange, random, shuffle,
derive_seed, float repr / fixed formatting / round, sum() of floats and json.dumps samples.
Floats are carried as IEEE-754 bit patterns (16 hex digits) so nothing is lost in transit.

    uv run python port/oracle/export_vectors.py [--check]
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from pathlib import Path

from gcg_sim.rng import SplitMix64, derive_seed

OUT = Path(__file__).resolve().parents[2] / "crates" / "gcg-core" / "tests" / "vectors.json"


def bits(x: float) -> str:
    return struct.pack(">d", x).hex()


def from_bits(rng: SplitMix64) -> float:
    while True:
        (x,) = struct.unpack(">d", rng.next_u64().to_bytes(8, "big"))
        if math.isfinite(x):
            return x


def build() -> dict[str, object]:
    gen = SplitMix64(20261101)
    streams = []
    for seed in (0, 1, 42, (1 << 64) - 1, 0x9E3779B97F4A7C15, *(gen.next_u64() for _ in range(5))):
        r = SplitMix64(seed)
        streams.append({"seed": str(seed), "next_u64": [str(r.next_u64()) for _ in range(16)]})

    randrange = []
    for i, n in enumerate((1, 2, 3, 7, 10, 52, 1000, (1 << 63) + 1, (1 << 64) - 1, 6)):
        r = SplitMix64(1000 + i)
        randrange.append(
            {"seed": str(1000 + i), "n": str(n), "out": [str(r.randrange(n)) for _ in range(12)]}
        )

    randoms = []
    for seed in (3, 99, 123456789):
        r = SplitMix64(seed)
        randoms.append({"seed": str(seed), "out": [bits(r.random()) for _ in range(16)]})

    shuffles = []
    for seed in (0, 5, 77, 2026):
        r = SplitMix64(seed)
        items = list(range(52))
        r.shuffle(items)
        shuffles.append({"seed": str(seed), "n": 52, "out": items})

    paths: list[tuple[int, tuple[object, ...]]] = [
        (0, ()),
        (1, ("match", 0, "game", 1)),
        (1, ("match", 3, "game", 2, "agent", 1)),
        ((1 << 64) + 5, ("hidden-rng",)),
        (-1, ("permuted-rng",)),
        (7, ("mcts", "decision", 12)),
        (2026, ("pairing", -3)),
        (5, ("it's",)),
        (5, ("a'b\"c",)),
        (5, ("tab\there", "back\\slash")),
        (123456789012345, ("greedy", "decision", 0)),
    ]
    derive = [
        {
            "master": str(m),
            "path": [p if isinstance(p, str) else {"int": p} for p in path],
            "out": str(derive_seed(m, *path)),
        }
        for m, path in paths
    ]

    special = [
        0.0,
        -0.0,
        1.0,
        0.1,
        1e-5,
        1e-4,
        1e16,
        1e15,
        1.5e16,
        0.5774,
        2 / 3,
        1e100,
        5e-324,
        1.7976931348623157e308,
        123456.789,
        0.125,
        0.375,
        2.5,
        1e22,
        1e23,
        9007199254740993.0,
    ]
    floats = special + [from_bits(gen) for _ in range(1500)]
    floats += [gen.random() * 10 ** (i % 12 - 4) for i in range(1500)]
    # Exact ties at 16-17 significant digits, where shortest-digit generators must round half
    # to even (e.g. 785081882490962.25 -> 785081882490962.2).
    for i in range(3000):
        k = (1 << 49) + gen.randrange(1 << 52)
        floats.append(k + (0.25, 0.75, 0.5)[i % 3])
        floats.append(float(k * 2 ** (i % 30 + 1)) + (i % 7))
    reprs = [{"x": bits(x), "repr": repr(x)} for x in floats]

    fixed = []
    tie_cases = [0.125, 0.375, 2.5, 0.5, 1.5, 0.045, 1.005, 2.675, 0.0005]
    for x in tie_cases + [gen.random() * 100 - 50 for _ in range(600)]:
        for prec in range(7):
            fixed.append(
                {
                    "x": bits(x),
                    "prec": prec,
                    "fmt": format(x, f".{prec}f"),
                    "round": bits(round(x, prec)),
                }
            )

    sums = []
    for i in range(300):
        n = i % 40
        vals = [(gen.random() - 0.5) * 10 ** (gen.randrange(12) - 6) for _ in range(n)]
        sums.append({"values": [bits(v) for v in vals], "sum": bits(sum(vals))})
    sums.append(
        {"values": [bits(v) for v in (1e100, 1.0, -1e100)], "sum": bits(sum([1e100, 1.0, -1e100]))}
    )

    libm_cases = []
    for i in range(4000):
        x = (gen.random() - 0.5) * (1.0, 8.0, 60.0, 1e3)[i % 4]
        libm_cases.append(
            {
                "x": bits(x),
                "erfc": bits(math.erfc(x)),
                "square": bits(x**2),
                "exp": bits(math.exp(x)) if x < 700 else None,
                "log": bits(math.log(abs(x))) if x != 0 else None,
            }
        )

    sample = {
        "b": [1, 2.5, -0.0, 1e-7, None, True, False],
        "a": 'Zeon · “quoted” \\ "x" \n\t\u0001 \U0001f680',
        "nested": {"z": {}, "y": [], "x": [[1], {"k": 3}]},
        "seed": (1 << 64) - 1,
    }
    dumps = [
        {
            "opts": "pretty_sorted_utf8",
            "out": json.dumps(sample, indent=2, sort_keys=True, ensure_ascii=False),
        },
        {
            "opts": "compact_sorted_ascii",
            "out": json.dumps(sample, sort_keys=True, separators=(",", ":")),
        },
        {
            "opts": "indent1_sorted_utf8",
            "out": json.dumps(sample, indent=1, sort_keys=True, ensure_ascii=False),
        },
        {"opts": "default", "out": json.dumps(sample)},
    ]
    return {
        "python": sys.version.split()[0],
        "streams": streams,
        "randrange": randrange,
        "random": randoms,
        "shuffle": shuffles,
        "derive_seed": derive,
        "repr": reprs,
        "fixed": fixed,
        "sum": sums,
        "libm": libm_cases,
        "json_sample_note": "the Rust test rebuilds `sample` by hand",
        "dumps": dumps,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if the committed file is stale")
    args = ap.parse_args()
    data = build()
    data.pop("python")
    text = json.dumps(data, indent=1, sort_keys=True) + "\n"
    if args.check:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != text:
            print(f"{OUT} is stale; run without --check", file=sys.stderr)
            return 1
        print(f"{OUT} is current")
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text, encoding="utf-8")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
