# Benchmarks: Python reference vs Rust port

Wall-clock comparison of the two implementations. This is the project's only timing
activity, and it lives outside the Rust crates (`scripts/compare_python_vs_rust.sh`). The
Rust codebase contains no benchmark suite, timing test or timing-dependent logic.

## Protocol

- **Workload.** `benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt
  --format bo3 --seed 1 --out <dir>`, with `--ai-preset`, `--matches` and `--workers` set per
  configuration.
- **Runner.** hyperfine 2.0.0, with a fresh `--out` directory per run (`--prepare 'rm -rf …'`)
  and no other load. Long configurations get 3 runs, short ones 5. Each configuration reports
  the median, min and max wall time. The machine runs at full performance; no warm-up, power
  or thermal controls are used.
- **Implementations.**
  - Python: `.venv/bin/gcg-sim` after `uv sync --locked`.
  - Rust, part 1: `target/release/gcg-eval reference-benchmark --reference-preset …`, which
    is the reference search at the reference budgets.
  - Rust, part 2: `target/release/gcg-sim benchmark --ai-preset fast|standard`, which is the
    shipped presets.
- **Pairs.**
  - Screening: Python `standard` against Rust reference `standard` (part 1) and Rust `fast`
    (part 2).
  - Full search: Python `strong` against Rust reference `strong` (part 1) and Rust `standard`
    (part 2).
- **Configurations.**
  - Headline (screening pairs): 100 matches, at each implementation's best worker count and
    at 10 workers.
  - Single-thread (screening pairs): 20 matches, 1 worker.
  - Full search: 20 matches at the best worker count.
- **Commands.**
  - `scripts/compare_python_vs_rust.sh best-workers python|rust-reference|rust-presets`
  - `scripts/compare_python_vs_rust.sh python|part1|part2 --best N`
  - `scripts/compare_python_vs_rust.sh report`
- **Output.** Raw hyperfine JSON and the report directories go to `build/comparison/`, which is
  not committed. The tables below copy them.

## Machine

| Item | Value |
| --- | --- |
| CPU | Apple M2 Pro, 10 cores: 6 performance + 4 efficiency (cache line 128 B) |
| Memory | 32 GB (34,359,738,368 bytes) |
| OS | macOS 27.2 (26B123) |
| Python | 3.15.1 (uv 0.13.6) |
| Rust | rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1 |
| hyperfine | 2.0.0 |

## Python baseline (measured 2026-11-01, commit d9d41af)

The Python best worker count was found with one run each of `--ai-preset standard --matches 20`.
The median of one run is that run's time.

| Workers | Wall time |
| ---: | ---: |
| 6 | 255.7 s |
| 8 | 239.0 s |
| **10** | **221.0 s** |

The best worker count is 10, so the "best" and "10 workers" headline configurations coincide
for Python.

| Configuration | Command flags | Runs | Median | Min | Max | Games |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Headline, screening | `--ai-preset standard --matches 100 --workers 10` | 3 | 972.0 s | 971.4 s | 981.5 s | 251 |
| Single-thread, screening | `--ai-preset standard --matches 20 --workers 1` | 3 | 1269.5 s | 1262.3 s | 1292.2 s | 50 |
| Full search | `--ai-preset strong --matches 20 --workers 10` | 3 | 356.0 s | 355.0 s | 356.1 s | 54 |

The 100-match headline played 251 games, the same count docs/AI.md reports for this
workload. The match result was 52 DUT wins out of 100, and 124 of 251 games. A read-only planning
workflow, made of LLM calls and file reads, ran during these measurements. No other local
computation ran.

## Python profile (where the time goes)

Full method, numbers and scripts: [inventory/11-python-profile.md](inventory/11-python-profile.md).
One reference `standard` BO1 game (seed 1, red-green-zeon in seat 0 against
blue-white-federation) takes 13.3 s unprofiled (hyperfine 13.336 ± 0.032 s). It runs 15,116
MCTS iterations, 140,925 `apply` calls, 8,184 determinizations and 23,326 clones.

| Share of game time (cProfile / sampler / wrappers) | |
| --- | --- |
| engine `apply`/`advance` (inclusive) | 80.7 / 80.9 / 79.8% |
| `view.derived` (1.7M calls, 18.6% cache misses) | 33.1 / 34.7 / 32.0% |
| legal-action generation (mostly main-phase options) | about 21–24% |
| playout policy | about 6% |
| `determinize` (including its clone) | 3.9–5.5% |
| `GameState.clone` (17.4 µs on a mid-game state) | 3.2–3.7% |
| evaluation | 1.3–1.7% |
| MCTS bookkeeping (exclusive) | 2.3–2.7% |

The engine dominates, so a compiled engine with a compact state (clone close to a memcpy)
and a cheaper derived view is where the speedup comes from.

## Phase 5 optimisation log (development measurements, not the comparison)

These numbers guided the Phase 5 changes. They are not the external comparison (Part 1 and
Part 2 below), which hyperfine takes on an idle machine. The machine was shared with other
work during all of them, with load averages between 7 and 35. So the decision metric is
`instructions retired`, which load does not change, with `cycles elapsed` and user CPU time
as checks. All three come from `/usr/bin/time -l` on one process. Each row ran the same three
workloads, always with `--seed 1 --workers 1` and the decks
`examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt`:

- **std**: `gcg-eval reference-benchmark … --matches 1 --format bo1 --reference-preset standard`;
- **strong**: the same with `--reference-preset strong`;
- **BO3**: `--matches 4 --format bo3 --reference-preset standard` (10 games).

Every row produced the same `results.json` (apart from `code_digest`), `games.ndjson`, replays
and `summary.md` bytes as the baseline. The std game's `final_state_sha256` is
`69c66131…7e4e`, Python's. Cycle counts are the median of the runs, or one run where only
one was made.

| Change (commit) | std instr / cycles / user | strong instr / cycles | BO3 instr / cycles | Kept |
| --- | ---: | ---: | ---: | --- |
| Baseline (3fbb8aa) | 14.75G / 3.51G / 1.17 s | 29.99G / 7.37G | 258.4G / 61.8G | — |
| Sparse derived view in recycled per-thread buffers (4057dd5) | 6.75G / 1.74G / 0.57 s | 14.55G / 3.79G | 111.7G / 28.9G | yes |
| Allocation-free hot paths (inline `UidBuf`, stack event extras, `link_satisfied` without a list, `payment_choices` as a range) and pooled view work sets (7d34df4) | 5.57G / 1.43G / 0.45 s | 12.47G / 3.39G | 89.7G / 24.3G | yes |
| Trigger index of a cached view read off its hosts (8923a49) | 5.44G / 1.33G / 0.42 s | 12.21G / 2.78G | 87.8G / 19.9G | yes |
| mimalloc as the default allocator (dad841a) | 4.82G / 1.22G / 0.39 s | 11.02G / 3.01G | 81.2G / 23.1G | yes |
| Skip the confirming fixpoint pass when its reads repeat (0466d9a) | 4.65G / 1.25G / 0.40 s | 10.81G / 3.06G | 76.1G / 21.3G | yes |
| PGO (`scripts/build_pgo.sh`, f16f64b; opt-in `GCG_PGO=1` in the comparison) | 3.84G / 0.97G / 0.29 s | 8.57G / 2.37G | 61.2G / 17.4G | yes (opt-in build) |
| Determinised worlds built in recycled states | 4.66G / 1.22G | — | — | no: no change |
| Per-game memo of `link_satisfied` | 4.60G / 1.27G | — | 74.7G / 21.1G | no: about 1-2%, inside the noise of cycles |
| Per-definition PAIRED/HAND/TRASH entry flags for the base view's zone scans | 4.66G / 1.23G | — | — | no: no change |
| `-C target-cpu`: generic / apple-m2 (`dist`) / native | 4.66G / 4.65G / 4.64G | — | — | native kept (no measurable difference) |

The allocator row compares the two allocators on the engine of 8923a49: the system allocator
ran 5.43G instructions, 1.33-1.39G cycles and 49 MB of RSS, and mimalloc ran 4.81G, 1.22G
and 36 MB. On the baseline engine, mimalloc alone gave 14.75G → 11.50G.

The PGO row was measured back to back with the plain release build of 0466d9a, which ran 4.64G / 1.12G / 0.34 s, 10.80G / 2.65G and 76.0G / 20.7G at that time. PGO trains on seeds 7-11, never on the comparison's seed 1, with the three example decks and
both search families (`scripts/build_pgo.sh`). A `gcg-diff` built with the same profile
replays tier 1 (3804/3804) and tier 2 (4154/4154) unchanged.

### What the view changes are

- **Sparse view.** `Derived` holds one `ViewSlot` per uid it writes, reached through a per-uid
  index, instead of fifteen dense per-uid vectors. `same_values` compares slots only.
- **Buffer reuse.** Buffers come from a per-thread pool (`view::pool`). A view leaving a
  cache is recycled once nothing else holds it. Reuse resets a buffer to `Derived()`.
- **Trigger index from the hosts.** When the view asked about is the one the state has cached,
  the state's zones are the ones the view was computed from: every zone change touches, and
  clones copy the zones with the cache. So the index is read off the view's host list instead
  of two zone scans.
- **Read-set shortcut.** Every non-trivial compute ran exactly two passes. Pass 1 only
  confirmed pass 0 (318,871 computes per std game, 186,542 of them two passes, all
  converging at pass 1). During pass 0 the base buffer notes which groups of which uids its
  accessors read. When pass 0 made no grant and every noted read answers the same in pass 0's
  result, pass 1 would repeat pass 0 exactly. So pass 0's result is returned. The paranoid
  build runs the skipped pass and asserts equality: new check `skipped fixpoint pass`,
  105,974 runs in `paranoid-games`, tier 1 `--paranoid` 3804/3804.

### Where the time goes, before and after (samply, four std BO1 games)

Inclusive shares of the samples. The allocator and memmove rows count only the samples whose
leaf is in them.

| Category | Baseline (3fbb8aa) | HEAD (0466d9a, mimalloc, no PGO) |
| --- | ---: | ---: |
| engine `apply` | 86.8% | 81.5% |
| `view::derived` (cache hits and computes) | 62.5% | 43.3% |
| … base view | 20.3% | 21.8% |
| … `same_values` | 16.4% | 0.0% (pass skipped) |
| … dropping views | 12.0% | 0.0% (recycled) |
| trigger index | 2.2% | 3.7% |
| `emit` | 25.4% | 22.8% |
| determinize | 3.2% | 4.3% |
| playout policy (rollout `choose`) | 2.2% | 7.6% |
| `option_scores` | 1.9% | 5.4% |
| MCTS `select` + `add_children` | 1.7% | 2.9% |
| evaluation `score` | 0.5% | 0.9% |
| allocator (malloc/free, leaf) | 30.2% | 2.0% |
| memmove/memset (leaf) | 7.3% | 6.1% |

The baseline game made 4.81M heap allocations (a counting allocator in a scratch binary outside
the repository). HEAD makes 2.63M, startup included, about 150 per MCTS iteration. Zero
allocation per iteration is **not** reached. The rest is spread over about fifty sites: option
lists (`Vec<Action>`), determinisation pools, policy score lists, emit candidate lists, frames.
With mimalloc they cost about 2% of the samples. After these changes the profile is flat: no
function has more than 13% self time, and the line-level profile has no line above 1.5%.

### Not done, and why

- **NEON.** No data-parallel integer kernel stands out in the profile. The hot code is
  pointer-chasing over small slots and registry entries.
- **Facet-versioned revalidation (PLAN §11.4 step 4).** The read-set shortcut gets the measured
  share of the confirming pass without per-node read masks. The base view (22%) still runs
  once per touch, as in Python.
- **128-byte alignment.** It was already in place: `gcg_ai::Scratch` and the pool's queue head
  and failure counter are `#[repr(align(128))]`. The view pool is thread-local, so it shares
  no line.
- **`unsafe`.** None was added.

### Worker count and the tail on 6 P + 4 E cores (loaded machine)

Workload: `--matches 20 --format bo3 --reference-preset standard`, PGO binaries. The load
averages were 6.8-9.0 during the sweep, so the walls are upper bounds.

| Workers | Wall | User CPU | Longest match | Sum of match times |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 24.9 s | 24.8 s | 1.88 s | 24.9 s |
| 6 | 9.7 s | 49.0 s | 4.16 s | 53.9 s |
| 8 | 8.9 s | 63.3 s | 5.22 s | 63.3 s |
| 10 | 8.9 s | 70.5 s | 6.37 s | 78.1 s |

The outputs are byte-identical for 1, 6, 8 and 10 workers. One std game restricted to the
efficiency cores (`taskpolicy -c background`, measurement only) runs the same 3.86G
instructions in 1.77G cycles and 0.71 s of user time, against about 0.33 s on a performance
core. An E core is therefore about 2.1 times slower on this workload.

A match is the unit of work in the reference search. A BO1 match is one game, and game g+1 of
a BO3 needs game g's loser. So a long match that starts late on an efficiency core sets the
tail. `first_player_choice` (1.6% of the time) already runs as a help-first fork-join. That is
the only exact intra-match parallelism the reference allows: the tree search is sequential.
Beyond it, the tail is left to macOS thread migration, as PLAN §7.3 decides. `--workers auto`
stays the sum of `hw.perflevelN.logicalcpu` (10 here). On this loaded machine, 10 workers were
no slower than 8 and faster than 6.

### Apple M6 (no M6 available)

- **Detection.** `gcg_core::sys::topology::detect()` reads `hw.nperflevels` and every
  `hw.perflevelN.{logicalcpu,physicalcpu,name}`, plus `hw.cachelinesize` and the
  `hw.optional.arm.*` listing. `gcg-eval env` adds `is_aarch64_feature_detected!` for 28
  features and the features the binary was compiled for. Unit tests parse a listing captured
  on this M2 Pro and a synthetic three-tier listing in the M6 layout (2 + 4 + 6, `auto` = 12).
- **Build.** No code path needs an optional CPU feature. On this toolchain (rustc 1.99.1),
  `rustc --print target-cpus` knows `apple-m4` and `apple-m5` but no M6 name, and `native`
  resolves to `apple-m2` here. On an M6, build with the default `native` (LLVM detects the
  host's features) or with `RUSTFLAGS="-C target-cpu=apple-m5"`. `-C target-cpu` changed
  nothing measurable on the M2 Pro: generic, apple-m2 and native ran within 0.4% of each other
  in instructions, with identical outputs. `scripts/build_pgo.sh` takes `GCG_TARGET_CPU`.
- **Comparison procedure on an M6.** Use the same commands as below, with the Python baselines
  measured on that M6 and `best-workers` candidates 6 (super + performance) and 12 (all).

### GPU (Metal)

No stage is a candidate:

- The batchable stages are small: leaf evaluation (`score`) is 0.9% of the samples and policy
  scoring (`option_scores`) 5.4%. Even if both took no time at all, a game would be at most
  1/(1 − 0.063) ≈ 1.07 times faster.
- Neither stage can be batched in the reference search. Each iteration's selection needs the
  previous backup (`ai/mcts.py:135-151`), so there is one leaf evaluation per iteration
  (15,116 per std game, each well under a microsecond). Each playout decision's next state
  depends on the option it picks.
- `option_scores` runs about 300,000 times per game at about 70 ns a call (5.4% of 0.4 s).
  One Metal dispatch costs tens of microseconds.

So no Metal code was written; it would also need `unsafe` Objective-C bindings.

## Part 1: equal-work speedup (reference search)

Not measured yet. Phase 5 produced the binaries; the hyperfine comparison on an idle machine
is the lead's. `GCG_PGO=1 scripts/compare_python_vs_rust.sh part1 --best N` times the PGO
build.

## Part 2: shipped presets

Not measured yet (Phase 6).
