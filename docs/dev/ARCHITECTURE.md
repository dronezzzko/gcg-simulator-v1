# Architecture of the Rust implementation

The Rust workspace replaces the Python runtime of gcg-sim: deck parsing and validation, the card
database, the effect programs, the rules engine, the AI players, the multi-threaded runner, the
reports, replay verification and the CLI. The Python package (`src/gcg_sim`, described in
[`docs/ARCHITECTURE.md`](../ARCHITECTURE.md)) stays as the oracle that the port is verified
against, and as the generator of the effect IR. The design and its decisions are in
[`docs/dev/PLAN.md`](PLAN.md); where the plan and the code differ, the code is right and this
file follows the code. Python paths are relative to `src/gcg_sim/`.

## Crates

Eleven crates, listed in dependency order in the workspace `Cargo.toml`:

| Crate | Kind | Responsibility |
| --- | --- | --- |
| `gcg-core` | lib | `SplitMix64` and `derive_seed` (`src/rng.rs`); CPython-compatible `repr`, float formatting, `round`, Neumaier `sum` and `json.dumps` (`pyrepr.rs`, `pyfloat.rs`, `pymath.rs`, `pyjson.rs`); the C math library calls (`libm.rs`, the workspace's only `unsafe`, for `erfc`); the thread pool (`pool.rs`); fixed-capacity inline arrays (`small.rs`); SHA-256-based stable keys (`stable_hash.rs`) and a hash map without iteration (`lookup.rs`); the property-test harness (`prop.rs`); CPU topology and SIGINT (`sys/topology.rs`, `sys/sigint.rs`) |
| `gcg-data` | lib | Card model and `CardDb` (`model.rs`, `db.rs`); the official data: construction rules, Banned & Restricted list, rules and BO3 versions (`official.rs`, `legality/`); deck parsing and validation with Python's messages (`deck.rs`, `validate.rs`, `names.rs`, the Unicode tables of `unicode/`); `versions()` and the `data status` parts (`versions.rs`); the packaged sources embedded with `include_bytes!` (`embedded.rs`) |
| `gcg-effects` | lib | The effect DSL node types, generated from the Python dataclasses by `oracle/gen_dsl.py` (`dsl.rs`); the JSON IR loader and the `Registry` (`ir.rs`, `codec.rs`, `registry.rs`); continuous-effect identity and CPython `repr` for content ids (`pyrepr.rs`); the golden-file writer (`golden.rs`); the 36 custom-hook ids with typed parameters (`hooks.rs`) |
| `gcg-engine` | lib | `GameState` and its records (`contracts.rs`, `state.rs`, `zones.rs`), the derived view (`view/`), events, triggers, damage and rules management (`core.rs`), battle (`battle.rs`), the interpreter and the step hooks (`interp/`), setup, the step machine, legal actions and `apply` (`game.rs`), information sets (`observe.rs`), invariants (`invariants.rs`), canonical JSON and `state_digest` (`canon.rs`); features `testkit` (`testkit/`) and `paranoid` (`paranoid.rs`, `view/paranoid.rs`) |
| `gcg-ai` | lib | The `Agent` trait, `AgentCore` and the decision log (`agent.rs`, `log.rs`); the shared exact components: move keys, canonicalisation, pruning and merging (`actions.rs`, `fingerprint.rs`), the no-effect check (`noop.rs`), pre-game choices (`setup.rs`), the playout policy (`policy.rs`, `playout.rs`), values and targeting (`values.rs`, `targeting.rs`), the evaluation and its weight tables (`evaluation.rs`); the random and greedy players (`random.rs`, `greedy.rs`); per-thread scratch states (`scratch.rs`) |
| `gcg-ai-reference` | lib | The reference ISMCTS (`mcts.rs`, `agent.rs`) and its exact presets `fast`, `standard`, `strong`, the baseline presets and `TUNING_CONFIG` (`contracts.rs`) |
| `gcg-search` | lib | The strengthened search behind the two `gcg-sim` presets: the core loop (`mcts.rs`, `tree.rs`), the configuration and flags (`contracts.rs`, `flags.rs`, `presets.rs`), the observation (`observation.rs`), the agent adapter (`agent.rs`) and the Phase 6 mechanisms (`mech/`) |
| `gcg-runner` | lib | Benchmark configuration and seeds (`config.rs`), games and BO1/BO3 matches (`game.rs`, `matches.rs`), the run on the pool (`run.rs`), game records and the deck-under-test observer (`records.rs`, `instrument.rs`), every report (`reports/`), replay verification (`replay.rs`), the opt-in analysis and checkpoints of `gcg-eval benchmark` (`analysis.rs`, `checkpoint.rs`), the test agents (`test_agents.rs`), the argparse-compatible parser shared by both binaries (`argparse/`), the `timing.json` clock (`clock.rs`) |
| `gcg-cli` | bin `gcg-sim` | The command line (`cli.rs`), the four commands (`commands.rs`), the preset agent factory (`factory.rs`), the build-time code digest (`build.rs`) |
| `gcg-eval` | bin `gcg-eval` | `reference-benchmark`, `head-to-head`, `compare` and every evaluation and tuning tool, the opt-in user tools, seed roles and the ledger (`cli.rs` and one module per command) |
| `gcg-diff` | lib + bin `gcg-diff` | The gate tool, never shipped: trace reader and differential replay (`reader.rs`, `replay.rs`, `corpus.rs`, `manifest.rs`), drill-down (`explain.rs`, `tracesearch.rs`), fuzzing (`fuzz.rs`, `paranoid.rs`), coverage, traceability and register checks (`coverage.rs`, `traceability.rs`, `testsjson.rs`, `weaknesses.rs`), schema and lint checks (`schema.rs`, `lints.rs`), the public API snapshot (`apisnapshot.rs`), negative controls (`selftest.rs`, `mock.rs`) |

Every library crate keeps its contract types in a `contracts.rs`; the frozen function
signatures are listed in [`docs/dev/contracts.md`](contracts.md).

## Dependency graph

The workspace dependencies of each crate (`[dependencies]` of its `Cargo.toml`):

```text
gcg-core
  └─ gcg-data
       └─ gcg-effects
            └─ gcg-engine
                 └─ gcg-ai ──┬─ gcg-ai-reference
                             ├─ gcg-search
                             └─ gcg-runner
gcg-cli   = gcg-runner + gcg-search + gcg-ai + gcg-effects + gcg-data + gcg-core
gcg-eval  = gcg-runner + gcg-search + gcg-ai-reference + gcg-ai + gcg-engine + gcg-effects
            + gcg-data + gcg-core
gcg-diff  = gcg-runner + gcg-search + gcg-ai-reference + gcg-ai + gcg-engine (feature testkit)
            + gcg-effects + gcg-data + gcg-core
```

`gcg-sim` (`gcg-cli`) never reaches the reference search: `crates/gcg-cli/tests/no_reference.rs`
reads the manifests of its dependency closure, and `scripts/verify_all_rust.sh` checks `cargo
tree -p gcg-cli -e normal` and that the `paranoid` feature is not in its feature tree.
`gcg-search` uses `gcg-ai-reference` only as a dev-dependency, for the null-configuration tests.

## Data flow

```text
src/gcg_sim/data (cards.ndjson, overrides, official/*.json, rules index)   [read-only sources]
        │ include_bytes! (gcg-data/src/embedded.rs)
        ▼
gcg-data: CardDb ──────────────► deck parse + validate (gcg-sim validate)
        │
        │   oracle/export_ir.py (Python compiler + bindings) ──► crates/gcg-effects/ir/registry.json
        ▼                                                              │ include_str!
gcg-effects: Registry (programs, abilities, cards, content ids) ◄──────┘
        ▼
gcg-engine: GameState, apply, legal actions, derived view, interpreter
        ▲   │
        │   ▼ Agent::choose(&GameState, player)
gcg-ai / gcg-ai-reference / gcg-search: random, greedy, reference ISMCTS, strengthened search
        ▼
gcg-runner: seeds, BO1/BO3 matches on gcg_core::pool, records, reports, replay
        ▼
gcg-sim (gcg-cli) and gcg-eval: command lines; results.json, summary.md, games.ndjson,
replays/*.json, timing.json
```

The binaries embed every packaged source and the IR, so they run with no file outside the
binary except the deck files and the report directory, and with no Python.

## Engine

### State layout and clone

`GameState` (`crates/gcg-engine/src/contracts.rs`) holds scalars and flat arenas of `Copy`
records: the card instances (a uid is the index in `cards`; cards are never removed, tokens move
to OUTSIDE and keep their slot), the zone lists as segments of one arena (`ZoneLists`), the
battles, the lasting and delayed effects with one shared arena of their targets, the triggers as
one vector with batch starts and the start of the pending tail, the effect frames with three
arenas for their variables, integers and item buffers, the once-per-turn keys, the history and
the pending decision with its options. Shared immutable parts sit behind one `Arc<GameConst>`
(decklists, registry, limits). Variable-size parts are addressed by `Span { start, len }` into
the arenas. The fields are private to `gcg-engine` (W-49); other crates read accessors and
change the state only through `apply` and the testkit.

`clone_from_state` (`crates/gcg-engine/src/state.rs`) truncates and refills every arena of a
recycled destination, which is a set of `memcpy`s into existing capacity plus two `Arc` clones.
The search builds its worlds in per-thread recycled states (`crates/gcg-ai/src/scratch.rs`).
`touch()` is Python's `state.touch()`: it bumps `version` and clears the view cache, and it is
called at exactly Python's call sites, because `version` reaches the canonical state.

### Derived view and its cache

`view::derived(&GameState)` (`crates/gcg-engine/src/view/`) reproduces `engine/view.py:223-239`:
the cache is a `RefCell<Option<Arc<Derived>>>` keyed by Python's view key (active player, turn,
battles, history length), filled by whichever call computes, and cleared only by `touch()`. The
AI's calls on the true state fill its cache exactly as Python's do, which matters for byte
equivalence. The fixpoint (`view/derived.rs`) is a literal port: base view, Jacobi passes over
constant and lasting effects, at most six passes, convergence by `same_values`.

`Derived` is sparse (Phase 5): one `ViewSlot` per uid the view wrote, reached through a per-uid
index, in buffers recycled through a per-thread pool (`view/pool.rs`). The trigger index of the
cached view is read off its host list. When the first fixpoint pass made no grant and every read
it noted repeats, the confirming pass is skipped; the `paranoid` build runs the skipped pass and
asserts equality, and also recomputes every cache hit and compares
(`docs/dev/BENCHMARKS.md`, Phase 5 log).

### Effects: the IR and the interpreter

The Python text compiler and the per-card bindings are not ported. `oracle/export_ir.py`
exports the built Python registry as a versioned, hash-consed JSON IR
(`crates/gcg-effects/ir/registry.json` and its `.sha256`, PLAN §3): node arena, programs (the
flattened instruction lists with Python's numbering, labels and content ids), abilities, card
entries, continuous effects, filter sets, hooks and `repr` vectors. `gcg-effects` embeds it and
checks it against its compiled-in schema at load (`ir.rs`); an unknown node class or hook fails
the load with the card number. Unit tests in `crates/gcg-effects/src/tests/` check that the IR
is current against the packaged data and the Python sources (`ir_current.rs`), that the Rust
`repr` reproduces every content id (`content_ids.rs`, `repr_vectors.rs`), and that the golden
writer regenerates `tests/effects/golden/` byte for byte (`golden.rs`). The
registry holds <!-- count:cards.definitions -->1,291<!-- /count --> card definitions for
<!-- count:cards.numbers -->1,290<!-- /count --> card numbers.

The interpreter (`crates/gcg-engine/src/interp/`) executes one instruction of the top frame at a
time (`interp/mod.rs`). An instruction that needs a choice sets the pending decision and marks
its frame as waiting; `apply` resumes it. A frame is addressed by its index and re-read after
any call that can push frames. The 13 custom step hooks live in `interp/hooks/` (one file per
Python binding module) and the 23 condition, filter and value hooks in `view/hooks.rs`. The
engine draws from the game RNG at Python's four sites in Python's order.

### Triggers, batches and frames

`emit` and `queue_trigger` (`crates/gcg-engine/src/core.rs`) are literal ports of
`engine/core.py`: candidates are collected in trigger-index order (active player first; BATTLE,
BASE, PAIRED, HAND, TRASH), then last-known hosts, delayed triggers and keyword triggers, with
Python's simultaneous-event dedupe and once checks. Queued triggers form a batch when the current
effect ends, and the newest batch resolves first; within a batch Bursts go first, then the active
player's triggers in the order they choose, then the standby player's. `advance`
(`crates/gcg-engine/src/game.rs`) follows `engine/game.py:734-760`: the action cap, frames,
pending triggers, batches, then setup or rules management and the next step. `apply` dispatches
on a waiting frame before the decision kind, as Python does. Rules management keeps Python's
`rules_checked` short-circuit and its `effect_hits` handling.

### Information sets, invariants and the canonical form

`observe.rs` ports `determinize`, `permute_hidden` and `information_set_key`. `invariants.rs`
checks card conservation and legal states after every action in fuzz runs. `canon.rs` writes
the canonical JSON of `engine/state.py:575-611` with content ids where `runner/digest.py`
substitutes them, `state_digest` as its SHA-256, and `from_canonical` to rebuild a state from a
trace snapshot.

## AI

All players implement `gcg_ai::Agent` (`crates/gcg-ai/src/contracts.rs`): `choose` receives the
true `&GameState` read-only, the player and an `AgentCx` with the thread's scratch buffers, the
fork-join handle and the SIGINT flag. Agents are rebuilt for every game, and the decision counter
seeds each decision's generator (`AgentCore::begin`, `crates/gcg-ai/src/agent.rs`). The players,
the presets and the mechanisms are described in [`docs/AI.md`](../AI.md).

| | Reference search (`gcg-ai-reference`) | Strengthened search (`gcg-search`) |
| --- | --- | --- |
| Purpose | the equivalence anchor: Python's decisions, byte for byte | the shipped presets `standard` and `fast` |
| Reached through | `gcg-eval` only (`reference-benchmark`, `ref:<preset>`) | `gcg-sim --ai-preset`, `gcg-eval` (`search:<preset>`) |
| Input | the true state, read-only, as Python's search reads it; its `derived()` calls fill the true state's cache as Python's do (PLAN D10) | an `Observation` (the observer's information-set state), built by the agent adapter; the true state never reaches the search (W-135) |
| Configuration | `ReferenceConfig`: Python's `SearchConfig` fields and defaults, the exact presets | `StrengthenedConfig`, no `Default`, every field spelled out, plus `MechanismFlags`; no conversion between the two types (W-136) |
| Loop | statement-by-statement port of `ai/mcts.py:82-339` | the same loop with the `Mechanisms` extension points (`mech/mod.rs`); the null configuration replays the reference bit for bit (`tests/null_config.rs`) |
| Outputs | agent names `mcts-<preset>`, results schema 1.1.0 | agent names `search-<preset>`, results schema 1.2.0 with `ai.search` and `ai.effort` |

The shared components in `gcg-ai` are exact ports used by both; new behaviour for the
strengthened search goes into new functions or flags, never into a shared component at the
reference's settings.

## Parallelism and the determinism contract

`gcg_core::pool` (`crates/gcg-core/src/pool.rs`) runs on `std::thread::scope`. `map_indexed`
runs one task per index, workers claim indices in increasing order from one atomic counter, and
results are stored by index, so they are identical for any thread count and schedule. Inside a
task, `fork_join` pushes jobs to one shared queue; idle workers and the waiting task run them
("help first"). Waiting is a plain blocking `Condvar` wait with no spinning, sleep, back-off,
timeout, QoS class or core pinning. Each worker and each nested job has its own scratch state
(128-byte aligned). A failing task lowers `lowest_failed`, so the run reports the lowest failing
match, as the serial Python path does (W-149); SIGINT is a separate flag (exit 130, no reports).

The unit of work is one match for `benchmark` and `reference-benchmark` (game g+1 of a BO3 needs
game g's loser), and one BO1 game for `head-to-head`. The play/draw simulations at the start of a
match run through `fork_join` (`crates/gcg-ai/src/setup.rs`, `crates/gcg-search/src/mech/p6h/sim.rs`). `--workers auto` is the sum of the logical CPUs of every tier but
the slowest (`sys/topology.rs`), at most one per match.

The contract:

- Every seed derives from the master seed through `derive_seed` (SHA-256 of the CPython `repr`
  of the seed path) feeding `SplitMix64` (`crates/gcg-core/src/rng.rs`): games from `(master,
  "match", m, "game", g)`, agents from the same path plus `"agent", seat`, decisions from
  `(agent seed, agent name, "decision", k)`, head-to-head games from `ai/selfplay.py`'s
  `mirrored_specs` paths (PLAN §7.2).
- Nothing in the crates reads a clock, the thread id, the worker count or completion order
  (`gcg-diff lints` rules 2 and 3, `clippy.toml` bans). Hash containers never reach an output:
  `HashMap` and `HashSet` are banned in favour of `gcg_core::lookup::LookupMap`, which cannot be
  iterated. Sorts are stable.
- `timing.json` is written for CLI parity only and is never compared.
- The reference search keeps Python's RNG consumption order and tie-breaking (first maximum,
  `py_sum`, libm calls), so `gcg-eval reference-benchmark` is byte-identical to Python on the
  golden platform ([`docs/dev/EQUIVALENCE.md`](EQUIVALENCE.md)).

## Reports

`crates/gcg-runner/src/reports/` writes every file of a run: `results.rs` (the `results.json`
document), `summary.rs` (`summary.md`), `writer.rs` and `set.rs` (the report set, written
atomically), `aggregate.rs`, `stats.rs` and `hypotheses.rs` (rates, Wilson intervals,
per-card statistics, tuning hypotheses), `replay_doc.rs` (replay files) and `schema.rs`, a
validator for the subset of JSON Schema the two schemas use. JSON goes through
`gcg_core::pyjson`, which reproduces `json.dumps` (key order, indentation, float `repr`,
`ensure_ascii`). `gcg-sim` validates against results schema 1.2.0
(`crates/gcg-runner/schema/results.v1_2.schema.json`); `gcg-eval reference-benchmark` validates
against Python's 1.1.0 file (`src/gcg_sim/reports/schema/results.v1.schema.json`), embedded
unchanged. `replay.rs` re-simulates a replay from its seeds and actions and checks the outcome
and final state digest.

## Oracle and differential runner

`oracle/` holds the Python-side tooling. It is separate from the package and never changes
its behaviour:

- `export_ir.py` writes the effect IR; `gen_dsl.py` and `gen_engine_enums.py` generate
  `crates/gcg-effects/src/dsl.rs` and `crates/gcg-engine/src/enums.rs` from the Python classes;
  `export_vectors.py`, `export_data.py`, `export_unicode.py`, `export_engine_vectors.py`,
  `export_ai_vectors.py`, `export_policy_vectors.py`, `export_records.py`, `export_stats.py`,
  `export_cli.py`, `export_parity.py` and the other `export_*.py` scripts write the vector files
  the Rust tests compare with (each has `--check`).
- `traces.py` records game, search, fuzz, coverage, view and world traces, and
  `pytest_trace.py` records one trace per Python rule, card, effect and AI test. Each decision
  carries the state digest prefix, the RNG state, `version`, `action_count`, `event_group`, the
  rules-check marker, the ordered legal options and the chosen action, with full snapshots at
  intervals (PLAN §8.2). Tier 1 (<!-- count:corpus.tier1.traces -->4,044<!-- /count -->
  traces) is committed in `oracle/corpus/v1/tier1/` with its `MANIFEST.json`; tier 2 is
  regenerated into `build/oracle/tier2/` and checked against `MANIFEST-tier2.json`.
  `census.py` writes the executed-program census.

`gcg-diff corpus` (`crates/gcg-diff/src/corpus.rs`, `replay.rs`) replays every trace through the
Rust engine and stops at the first divergence with a readable diff of state and legal actions;
`gcg-diff explain` and `trace-search` drill down to one decision, one MCTS iteration or one
`apply`. `gcg-diff selftest` proves that the comparison catches injected faults, `--mock` and
`--parse-only` check the reader alone, and `--paranoid` replays with the paranoid engine.
`gcg-diff traceability --write` generates `docs/RULES_TRACEABILITY.md` and
`docs/dev/TRACEABILITY_CARDS.md` with a Rust column (native tests tagged `// gcg-tags:` plus
passing per-test traces). End-to-end equivalence of whole runs is
`scripts/compare_python_vs_rust.sh equivalence` with `scripts/normalize_outputs.py`.

## Where each Python module went

| Python | Rust |
| --- | --- |
| `rng.py` | `crates/gcg-core/src/rng.rs` |
| `cards/model.py`, `cards/db.py`, `cards/tokens.py` | `crates/gcg-data/src/model.rs`, `db.rs` |
| `deck/model.py`, `deck/parse.py`, `deck/names.py`, `deck/validate.py` | `crates/gcg-data/src/deck.rs`, `names.rs`, `validate.rs`, `unicode/` |
| `deck/official.py` | `crates/gcg-data/src/official.rs`, `legality/` |
| `reports/versions.py` | `crates/gcg-data/src/versions.rs`; the code digest in `build-support/code_digest.rs` |
| `effects/dsl.py`, `effects/program.py`, `effects/scripts.py` | `crates/gcg-effects/src/dsl.rs` (generated), `contracts.rs`, `codec.rs` |
| `effects/registry.py` | the IR export (`oracle/export_ir.py`) and `crates/gcg-effects/src/ir.rs`, `registry.rs` |
| `effects/text.py`, `effects/compiler/`, `effects/bindings/*.py` | not ported: their output is the IR; the 36 hook bodies of the bindings are in `crates/gcg-engine/src/interp/hooks/` and `view/hooks.rs` |
| `effects/builtins.py` | `crates/gcg-engine/src/interp/hooks/builtins.rs` |
| `effects/golden.py`, `tools/golden.py` | `crates/gcg-effects/src/golden.rs` (the writer; the golden files stay Python's) |
| `engine/types.py` | `crates/gcg-engine/src/enums.rs` (generated), `types.rs` |
| `engine/state.py` | `crates/gcg-engine/src/contracts.rs`, `state.rs`, `zones.rs`, `canon.rs` |
| `engine/view.py` | `crates/gcg-engine/src/view/` |
| `engine/core.py` | `crates/gcg-engine/src/core.rs` |
| `engine/interp.py` | `crates/gcg-engine/src/interp/` |
| `engine/battle.py` | `crates/gcg-engine/src/battle.rs`, `procedures.rs` |
| `engine/game.py` | `crates/gcg-engine/src/game.rs` |
| `engine/observe.py` | `crates/gcg-engine/src/observe.rs` |
| `engine/invariants.py` | `crates/gcg-engine/src/invariants.rs` |
| `runner/digest.py` | `crates/gcg-engine/src/canon.rs` (`state_digest`) |
| `testkit.py` | `crates/gcg-engine/src/testkit/` (feature `testkit`) |
| `ai/base.py` | `crates/gcg-ai/src/agent.rs`, `log.rs` |
| `ai/actions.py` | `crates/gcg-ai/src/actions.rs`, `fingerprint.rs` |
| `ai/noop.py`, `ai/setup.py`, `ai/policy.py`, `ai/playout.py`, `ai/values.py`, `ai/targeting.py`, `ai/evaluation.py` | `crates/gcg-ai/src/noop.rs`, `setup.rs`, `policy.rs`, `playout.rs`, `values.rs`, `targeting.rs`, `evaluation.rs` |
| `ai/agents.py` | `crates/gcg-ai/src/random.rs`, `greedy.rs`; `crates/gcg-ai-reference/src/agent.rs` |
| `ai/mcts.py` | `crates/gcg-ai-reference/src/mcts.rs` (exact); its fork `crates/gcg-search/src/mcts.rs` |
| `ai/config.py`, `ai/baseline.py` | `crates/gcg-ai-reference/src/contracts.rs` |
| `ai/compare.py`, `ai/selfplay.py`, `ai/tuning.py` | `crates/gcg-eval/src/compare.rs`, `selfplay.rs`, `specs.rs`, `tune.rs` |
| `ai/throughput.py` | `scripts/throughput.py` (outside the crates; it times `gcg-eval head-to-head`) |
| `runner/config.py`, `runner/match.py`, `runner/game.py`, `runner/run.py` | `crates/gcg-runner/src/config.rs`, `matches.rs`, `game.rs`, `run.rs`, on `crates/gcg-core/src/pool.rs` |
| `runner/records.py`, `runner/instrument.py`, `runner/replayfile.py` | `crates/gcg-runner/src/records.rs`, `instrument.rs`, `replay.rs`, `reports/replay_doc.rs` |
| `runner/agents.py` | `crates/gcg-cli/src/factory.rs`, `crates/gcg-eval/src/factories.rs` |
| `reports/results.py`, `reports/summary.py`, `reports/writer.py`, `reports/aggregate.py`, `reports/hypotheses.py`, `reports/stats.py`, `reports/schema/` | `crates/gcg-runner/src/reports/` |
| `cli.py` | `crates/gcg-cli/src/cli.rs`, `commands.rs`; the argparse port in `crates/gcg-runner/src/argparse/` |
| `tools/fuzz.py` | `crates/gcg-diff/src/fuzz.rs` |
| `tools/traceability.py` | `crates/gcg-diff/src/traceability.rs` |
| `tools/explain.py`, `tools/marks.py`, `tools/refresh.py`, `tools/sources.py`, `sources/conflicts.py`, `rules/index.py` | not ported: data and card tooling of the refresh workflow, which stays in Python |
| `tests/paranoid.py` | the `paranoid` feature of `gcg-engine` and `gcg-diff paranoid-games` |
