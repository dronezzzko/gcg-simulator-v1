# gcg-sim — Gundam Card Game deck benchmark simulator

`gcg-sim` measures how a Gundam Card Game (English) deck performs against a benchmark deck. It
plays many matches between two search-based AI players under a deterministic implementation of
the Comprehensive Rules (Ver. 1.10.0, 2026-10-16). It then writes reproducible matchup reports
with honest error bars, which you can use to tune the deck.

The tool is a Rust workspace (`crates/`) with two binaries, `gcg-sim` and `gcg-eval`, plus the
development tool `gcg-diff`. Its
rules engine, card behaviour and reference search are verified trace by trace against a
reference oracle kept in the repository (`src/gcg_sim`), which also generates the effect IR
that the binaries embed.

- Every card in the packaged card data is implemented (1290/1290 card numbers), and every
  non-vanilla card has a behaviour test.
- Every 1v1 rule is covered by a tagged test or has a written N/A reason
  ([`docs/RULES_TRACEABILITY.md`](docs/RULES_TRACEABILITY.md)).
- Everything runs offline from data embedded in the binaries; nothing else is needed at run
  time.

## Build and run on the Apple M2 Pro

Requires a stable Rust toolchain (rustc 1.99 or later, via [rustup](https://rustup.rs)). The
crates are fetched once; every later build runs offline:

<!-- doc-check: network fetches the locked crates from crates.io -->
```bash
cargo fetch --locked
```

```bash
cargo build --release --locked --offline -p gcg-cli -p gcg-eval
target/release/gcg-sim --help
```

This builds `target/release/gcg-sim` (the benchmark tool) and `target/release/gcg-eval` (the
evaluation harness). Build settings:

- **Local builds** tune for the machine that builds them: `.cargo/config.toml` sets
  `-C target-cpu=native` (on the M2 Pro, `native` is `apple-m2`). The release profile
  (`Cargo.toml`) uses `opt-level = 3`, fat LTO, one codegen unit and `panic = "abort"`, and
  mimalloc is the allocator.
- **Distributable binaries**: `scripts/build_dist.sh` builds `gcg-sim` and `gcg-eval` with
  the `dist` profile and the `apple-m2` baseline (`GCG_TARGET_CPU` picks another) into
  `target/dist/`, next to `LICENSE-DATA` and `NOTICE.txt`.
  `scripts/verify_binary_offline.sh` runs `gcg-sim` outside the repository with networking
  denied and no Python on the `PATH`:

  <!-- doc-check: skip a second full LTO build under the dist profile; scripts/verify_all_rust.sh runs both commands -->
  ```bash
  scripts/build_dist.sh
  scripts/verify_binary_offline.sh target/dist/gcg-sim
  ```

- **Profile-guided builds** (optional): `scripts/build_pgo.sh` builds instrumented binaries,
  trains them on fixed seeds (never the comparison workload's seed 1), and writes
  `target/pgo/use/release/gcg-sim` and `gcg-eval`. It needs `rustup component add llvm-tools`.
  PGO changes code layout and inlining only: outputs are byte-identical to a plain release
  build.
  `GCG_TARGET_CPU=apple-m2` overrides `native`.

`-C target-cpu` cannot change results: Rust never fuses `a * b + c` into an FMA without
`mul_add`, which `clippy.toml` bans. On the M2 Pro, `generic`, `apple-m2` and `native` ran
within 0.4% of each other in instructions with identical outputs (`docs/dev/BENCHMARKS.md`,
Phase 5 log).

## Apple M6

No M6 was available, so nothing in this repository was measured on one. What is in place:

- **Build.** This toolchain (rustc 1.99.1) knows `apple-m4` and `apple-m5` but no M6
  `target-cpu` name. Build with the default `native` (LLVM detects the host's features), or
  with `RUSTFLAGS="-C target-cpu=apple-m5"` for a fixed target. Check what the toolchain knows
  with `rustc --print target-cpus | grep apple` and `rustc -C target-cpu=native --print cfg |
  grep target_feature`. No code path needs an optional CPU feature.
- **Detection.** At run time `gcg_core::sys::topology` reads `hw.nperflevels`, every
  `hw.perflevelN.{logicalcpu,physicalcpu,name}`, `hw.cachelinesize` and the
  `hw.optional.arm.*` listing through `/usr/sbin/sysctl`. `gcg-eval env` prints the tiers, the
  CPU features detected at run time and the features the binary was compiled for; `timing.json`
  records the tiers. Unit tests parse a listing captured on the M2 Pro and a synthetic
  three-tier listing in the M6 layout (2 super + 4 performance + 6 efficiency).
- **`--workers auto`** uses every tier except the slowest: the 6 performance cores of the M2
  Pro, and the 2 super + 4 performance cores of the M6 layout, at most one worker per match. A
  match is the unit of work and runs sequentially, so a match that starts late on an efficiency
  core (about 2.1 times slower per game on the M2 Pro) finishes after the others. On the M2 Pro,
  6 workers beat 10 on every timed workload (`docs/dev/BENCHMARKS.md`, Part 1 and Part 2). The
  worker count never changes an output.

## Usage

Check that a deck is legal:

```bash
target/release/gcg-sim validate examples/decks/red-green-zeon.txt
```

Benchmark a deck under test against a benchmark deck:

```bash
target/release/gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt \
    --matches 2 --seed 1 --workers auto --ai-preset fast --out /tmp/gcg-sim-rust-demo
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--matches N` | required | Number of matches. Use hundreds for decisions. |
| `--seed S` | 0 | Master seed; the same seed gives byte-identical `results.json` whatever `--workers` is. |
| `--workers N\|auto` | 1 | Worker threads. `auto` uses one per performance-core CPU (every tier but the efficiency cores), at most one per match. |
| `--format bo3\|bo1` | `bo3` | Best-of-three (official BO3 match rules, no sideboarding, no clock) or single games. |
| `--ai-preset standard\|fast` | `standard` | AI strength (below and [`docs/AI.md`](docs/AI.md)). Any other value is a usage error. |
| `--out DIR` | `gcg-sim-out` | Report directory. |
| `--decision-log` | off | Record the AI's per-decision search statistics. |
| `--replays K` | 3 | Representative wins and losses to save. |

The two presets:

- `standard`: uses the full search so that each player plays at the level of the strongest
  top-tier competitive player; for refining and polishing the final deck.
- `fast`: trades search depth for throughput; for rapidly screening deck variants,
  sub-variants and hypothetical builds.

Both run the strengthened search. Their budgets start from the reference presets `strong`
(`standard`) and `standard` (`fast`) and were then tuned; [`docs/AI.md`](docs/AI.md) has the
table of budgets and mechanisms. The reference preset `fast` (`gcg-eval reference-benchmark
--reference-preset fast`) is a different, smaller budget.

Other commands:

```bash
target/release/gcg-sim data status
target/release/gcg-sim replay "$(ls /tmp/gcg-sim-rust-demo/replays/*.json | head -n 1)" --log
```

`data status` prints the packaged data, rules, B&R versions, implementation coverage and
conflict counts (`--json` for JSON). `replay` re-simulates a saved game from its seeds and
verifies its outcome and final state (`--log` prints the game). Errors print a clear message and
exit non-zero: 1 for an invalid deck, data or replay, 2 for a usage error, 130 for an
interrupt.

## Two-stage workflow

1. **Screen** many variants (sub-variants, card swaps, hypothetical builds) with
   `--ai-preset fast`, the same benchmark deck and the same seed schedule, and discard the
   clearly worse ones.
2. **Polish** the survivors and the final deck with `--ai-preset standard` (the default) on
   different seeds from the screen.

Never compare or pool a `fast` rate with a `standard` rate: they are different measurements.
On an idle M2 Pro, 100 BO3 matches of red-green-zeon against blue-white-federation (seed 1)
took 82.1 s with `fast` at 6 workers, and 20 BO3 matches took 148.6 s with `standard` at 6
workers (hyperfine; method, tables and the optimisation log in
[`docs/dev/BENCHMARKS.md`](docs/dev/BENCHMARKS.md), Part 2). The
[`gcg-benchmark`](.claude/skills/gcg-benchmark/SKILL.md) Claude Code skill walks through this
workflow with adequate sample sizes.

## Deck files

```text
# comment lines start with "#"
4 GD01-008 Guntank          <- <count> <card_number> [name]
2 GD01-001_p1               <- alternate-art product ids normalize to the card number
10 R-001                    <- RESOURCE cards form the 10-card resource deck
```

Validation checks main and resource deck sizes, the 4-copy limit, one or two colors, card
types, the banned/restricted list with banned pairs, unknown ids, and names. Every violation is
reported with a specific message. Full format: [`docs/DECK_FORMAT.md`](docs/DECK_FORMAT.md).
Three legal example decks are in [`examples/decks/`](examples/decks/), and the five evaluation
decks of the strength panel are in [`examples/decks/eval/`](examples/decks/eval/).

## Reports

A benchmark writes these files to `--out`:

| File | Content |
| --- | --- |
| `summary.md` | Match and game win rates with 95% Wilson intervals, on-play/on-draw and game 1/2/3 splits, game length and end reasons, per-card statistics, benchmark cards most associated with losses, tuning hypotheses with their sample sizes, versions, seeds, AI settings (with a search line: preset, mapping, budget, mechanisms, weights) and the conflict resolutions that affect either deck. |
| `results.json` | The same data, validated against a versioned JSON Schema before it is written. |
| `games.ndjson` | One record per game. |
| `replays/*.json` | Representative games, replayable with `gcg-sim replay`. |
| `timing.json` | Wall-clock information and the CPU tiers, kept separate so the other files stay reproducible. |

`gcg-sim` writes results schema **1.2.0** (`crates/gcg-runner/schema/results.v1_2.schema.json`):
schema 1.1.0 plus `ai.search` (algorithm `gcg-search`, the preset, its mapping to a
reference preset, the iteration budget, the auxiliary apply cap, the enabled mechanisms and the
weights) and `ai.effort` (per seat: games, decisions, searches, iterations, engine applies by
purpose, worlds, capped rollouts, and per-game distributions). `gcg-eval reference-benchmark`
writes schema **1.1.0** (`src/gcg_sim/reports/schema/results.v1.schema.json`).
Replay files are the same in both, so `gcg-sim replay` verifies replays from either binary.
Read [`docs/REPORTS.md`](docs/REPORTS.md) for the fields and for how to interpret the
numbers.

## The evaluation harness `gcg-eval`

`gcg-eval` is the second binary. It is the only route to the reference search: the frozen
information-set MCTS with the exact reference budgets and parameters, which `gcg-sim
--ai-preset` never reaches.

```bash
target/release/gcg-eval reference-benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt \
    --matches 2 --seed 1 --format bo1 --reference-preset standard --workers auto --out /tmp/gcg-eval-reference-demo
target/release/gcg-eval head-to-head search:fast ref:standard --decks examples --games 12 --seed 1 --workers auto
```

- `reference-benchmark`: the benchmark pipeline with the reference search
  (`--reference-preset fast|standard|strong`), writing results schema 1.1.0.
- `head-to-head`: two agents (`random`, `greedy`, `ref:<preset>`, `ref:baseline-<preset>`,
  `search:<preset>`, `search:FILE.json`, with `:field=value` overrides) in mirrored blocks of four
  BO1 games on deck pairs, with Wilson intervals per pair and pooled; `--role` checks the seed
  against its role's range and records it in the seed ledger.
- `head-to-head-replay`: re-plays games of a `head-to-head --out` report (`--index I` or
  `--losses`), checks each against its row and writes it as a `gcg-sim replay` file.
- Evaluation and tuning tools: `compare`, `preset-benchmark`, `ablation`, `budget-curve`,
  `fidelity`, `regret`, `tune`, `tune-leaf`, `calibrate`, `compare-runs`, `decks`, `gates`,
  `evidence`, `transcripts`, `weakness` and `env`.
- Opt-in user tools, kept out of `gcg-sim` so its command line stays fixed:
  `benchmark` (`gcg-sim benchmark` plus `--analysis`, `--swiss-draws`, `--compress`, `--resume`,
  `--replays-extremes`, `--diagnostics`, `--opponent-list`; without them it writes the same
  bytes as `gcg-sim`), `replay-game` (re-simulate one game of a `games.ndjson`), `replay-check`
  (replay with a data, rules and code version check), `validate` (dated and regional B&R lists,
  `--no-banlist`, the 7-day new-card rule, `--scope`) and `data-status` (`--sources`,
  `--freshness`, `--templates`).

`gcg-eval --help` and `gcg-eval <command> --help` describe every option. How the presets'
strength is measured with it is in [`docs/AI.md`](docs/AI.md).

## How correctness is verified

The engine replays a differential corpus of traces recorded from the reference oracle (game,
search, fuzz, coverage, view, world and per-test traces of the oracle's rule and card tests)
and stops at the first divergence in state digest, legal options, RNG, error or agent
decision: tier 1 (committed) passes 3804/3804 traces and tier 2 (regenerated) 4154/4154. End
to end, `gcg-eval reference-benchmark` matched the oracle's benchmark byte for byte (after
masking three implementation fields) on 108 configurations (3 seeds x 3 example pairings x
BO1/BO3 x the reference presets `fast`, `standard`, `strong` x decision log off/on, 10
matches) at 1, 4 and 10 workers: 324/324 identical, plus the 100-match headline configuration
3/3. The matrix binaries were built before the optimisations of the Phase 5 log
(`docs/dev/BENCHMARKS.md`); the headline run was rebuilt afterwards (e14c839), and the corpus
passes after every later commit. Byte equivalence is defined on the golden platform (this M2
Pro, macOS 27.2, Python 3.15.1, rustc 1.99.1), because the reference search takes `exp`,
`log`, `pow` and `erfc` from the C library. Method, results and the seven known differences
(none changes a compared byte): [`docs/dev/EQUIVALENCE.md`](docs/dev/EQUIVALENCE.md).

## Measured strength

Final evaluation 2 of the shipped presets (`docs/dev/eval/final-2/README.md`): eval-role
seeds never used for tuning, BO1 games in mirrored blocks with play and draw balanced, the 28
pairs of an 8-deck panel at 400 games each (13 pairs held out from all tuning), and the AI
test decks (the `tests` deck set of `gcg-eval head-to-head --decks`).

| Comparison | Result (Wilson 95%) | Bar |
| --- | --- | --- |
| `standard` vs reference `strong` | 6408/11200 = 0.572 [0.563, 0.581]; held-out pairs 0.562 [0.548, 0.575] | lower bound at least 0.55: met (held-out pairs alone: 0.548) |
| `fast` vs reference `standard` | 6254/11200 = 0.558 [0.549, 0.568] | point estimate at least 0.5, no pair below: met |
| `standard` vs random | 400/400 on the test decks; 11186/11200 on the panel, every loss saved as a verified replay | met |
| `standard` vs greedy | panel 8774/11200 = 0.783 [0.776, 0.791]; test decks 304/400 = 0.760 [0.716, 0.799] | lower bound above 0.764: met on the panel, not on the test decks |


The strength measurements of both presets against the reference search, random and greedy,
with the evaluation protocol and the gates, are in [`docs/AI.md`](docs/AI.md).

## The reference oracle

The Python package in `src/gcg_sim` is the reference oracle: the differential corpus is
recorded from it, and its effect toolchain generates the effect IR that the binaries embed
(`oracle/export_ir.py`). Python is needed only for development and data refreshes:
building and running the binaries needs none, although they embed data files from
`src/gcg_sim/data/`. It needs [uv](https://docs.astral.sh/uv/) and Python 3.15 or later (uv
installs it if needed):

```bash
uv sync --locked
uv run python oracle/export_ir.py --check
```

The oracle's architecture is in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Development

The Rust gate runs on a clean clone of the committed HEAD: fmt, clippy, every test tier, the
differential corpus, the negative controls, fuzzing with invariants, the 1/4/10-worker identity
runs, schema validation and the offline binary check (`--full` adds the oracle's checks and
regenerations). `scripts/ci_local.sh` runs the steps of the GitHub Actions workflow
(`.github/workflows/rust.yml`) locally. The oracle's own gate is `scripts/verify_all.sh`.

<!-- doc-check: skip the full gates clone the repository and run every test tier, the corpus and the fuzz runs; they are run on their own, not from the docs -->
```bash
scripts/verify_all_rust.sh
scripts/ci_local.sh
scripts/verify_all.sh
```

The day-to-day commands, the oracle regeneration steps and the rules of the codebase are in
[`CLAUDE.md`](CLAUDE.md). Design: [`docs/dev/ARCHITECTURE.md`](docs/dev/ARCHITECTURE.md) and
[`docs/dev/PLAN.md`](docs/dev/PLAN.md); known weaknesses and their status:
[`docs/dev/WEAKNESSES.md`](docs/dev/WEAKNESSES.md).

Where sources disagree or card text is ambiguous, the decision is recorded:
[`docs/CONFLICTS.md`](docs/CONFLICTS.md) lists every conflict and its resolution (reviewable in
`src/gcg_sim/data/overrides.json`), and [`docs/ASSUMPTIONS.md`](docs/ASSUMPTIONS.md) lists the
engine's interpretation decisions.

## Sources and versions

| Source | Version |
| --- | --- |
| Comprehensive Rules | Ver. 1.10.0, updated 2026-10-16 |
| Card data | [gcg-api](https://github.com/yzRobo/gcg-api) data files at commit `78a64df`, dataset `31-ebd08a4b`, built 2026-11-02 |
| Banned & restricted list | effective 2026-09-25 |
| BO3 match rules, deck construction, floor rules | gundam-gcg.com/en pages, last checked 2026-11-06 |

Every source's URL, version, retrieval time and SHA-256 is locked in `data/SOURCES.lock.json` and
summarized in [`docs/SOURCES.md`](docs/SOURCES.md). Cached source files are never edited. To
update them, use the [`gcg-refresh-data`](.claude/skills/gcg-refresh-data/SKILL.md) skill: it
diffs a new snapshot, flags new or changed cards, regenerates the effect IR and the oracle
artefacts, and runs the full verification gate.

## Licences and notices

- Contains data from gcg-api (https://gcgapi.com), made available under the Open Database
  License (ODbL) v1.0; see `src/gcg_sim/data/gcgapi/LICENSE-DATA`. The packaged card database,
  including the resolutions in `src/gcg_sim/data/overrides.json`, is a derivative database
  offered under the ODbL v1.0.
- Gundam Card Game card names, card text and trademarks belong to BANDAI and SOTSU·SUNRISE.
  This is an unofficial fan project. It is not affiliated with or endorsed by Bandai. It
  contains no card images.

## Limitations

- **1v1 only.** Multiplayer (rules section 12) is out of scope; its rules are marked N/A.
- **No sideboarding or clock** in BO3, as the task specifies. Games that reach the engine's
  safety cap (200 turns or 20,000 decisions) are reported as `turn_limit` draws.
- **The AI is strong but not perfect.** Decks that need long, precise lines may be underrated.
  Check replays before trusting a surprising result. The open AI items are listed in
  [`docs/dev/WEAKNESSES.md`](docs/dev/WEAKNESSES.md); measured strength is only what
  [`docs/AI.md`](docs/AI.md) reports.
- **Card behaviour follows the packaged data and the resolutions in `docs/CONFLICTS.md`.** Where
  no official ruling exists, the most defensible reading was chosen and recorded there.
- **Two official deck-building exceptions are not applied**: unmodified ST02/ST05 starter decks.
  The validator cannot tell that a deck is an unmodified starter deck, so it reports them as
  violations and says so.
- **Reproducibility is per platform.** The engine is integer-only, but the searches'
  floating-point math uses the platform's C math library, so on another platform their values
  can differ in the last bit. Every report but `timing.json` is byte-identical across runs and
  worker counts on one platform; decisions are not claimed to match across platforms
  (`docs/dev/EQUIVALENCE.md`, W-137).
- **New card sets need the oracle's toolchain.** The binaries embed an effect IR exported from
  the Python compiler and bindings; a data refresh regenerates it (`/gcg-refresh-data`).
- **Nothing was measured on an Apple M6.** The M6 support is the build settings and run-time
  detection above.
