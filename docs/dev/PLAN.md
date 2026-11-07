# Rust implementation of gcg-sim: plan

Phase 1 deliverable for `docs/port/BRIEF.md` (step 5). Branch `rust-port` at `d9d41af` plus the
uncommitted port files (`Cargo.toml`, `Cargo.lock`, `crates/gcg-core`, `port/oracle`,
`scripts/compare_python_vs_rust.sh`, `scripts/normalize_outputs.py`, `docs/port/`). Inputs:
`docs/port/BRIEF.md` (binding), `docs/port/INVENTORY.md` and its twelve sections in
`docs/port/inventory/`, `docs/dev/WEAKNESSES.md` (164 items) and the Python reference in
`src/gcg_sim`. Machine-readable test register: `docs/dev/tests.json` (Appendix A maps every id to
its gate). Phase gates are recorded in `docs/port/progress.md`.

Conventions:

- `path:line` is relative to `src/gcg_sim/` unless it starts with `docs/`, `tests/`, `crates/`,
  `oracle/`, `scripts/` or is a repository-root file. "Inv NN §x" means
  `docs/port/inventory/NN-*.md`, section x, at commit 83a06c8. When an inventory section and the
  code disagree, the code wins.
- Nothing in this plan was measured by the planner. Every number is cited from the inventory, the
  code or `docs/port/progress.md`. Sizes marked "estimate" are design budgets that the Phase 2
  census replaces with measured values.
- Command listings use `text` fences on purpose. `scripts/check_doc_commands.py` executes every
  `bash`/`sh`/`shell` fence under `docs/**/*.md` (`scripts/check_doc_commands.py:1-8`, regex at
  `:28-32`), and these commands run for hours or need build artefacts.
- An implementer subagent can execute one workstream of §10 with this file, the inventory section
  named in its brief and the Python modules it ports. It does not need any other context.

## 0. Key decisions in one page

1. **IR export, not a compiler port** (§3). `oracle/export_ir.py` exports the built Python
   `Registry` as a hash-consed, versioned JSON IR. The IR is checked in at
   `crates/gcg-effects/ir/registry.json` and embedded with `include_str!`. Rust hand-ports the
   interpreter, the derived view, the 34 hooks and a CPython-compatible `repr` for content ids.
2. **Eleven crates** (§2): `gcg-core`, `gcg-data`, `gcg-effects`, `gcg-engine`, `gcg-ai`,
   `gcg-ai-reference`, `gcg-search`, `gcg-runner`, `gcg-cli` (bin `gcg-sim`), `gcg-eval`
   (bin `gcg-eval`) and `gcg-diff` (bin `gcg-diff`, development and gate tool). The reference
   search lives in its own crate, and `gcg-cli`'s dependency closure cannot contain it.
3. **State layout** (§4). `GameState` holds scalars, about twenty flat `Vec<T: Copy>` arenas and
   one `Arc<GameConst>`. `clone_from` into a recycled state is about twenty `memcpy`s with no
   allocation. Process-local registry numbers disappear: continuous effects are values
   (`ContRef`), once-per-turn keys are a typed `OnceKey`, and content ids come from a ported
   `repr`.
4. **Exact cache semantics by construction** (§4.6). The derived view is cached in
   `RefCell<Option<Arc<Derived>>>`, keyed exactly like `engine/view.py:223-239`.
   `view::derived(&GameState)` fills the cache of whatever state it is called on, including the
   AI's calls on the true state (`ai/actions.py:109,145,210,385`). `touch()` is the only
   invalidation and is mirrored call for call. Cache transparency is still proven, by a Python
   `--nocache` corpus run and the Rust `paranoid` feature, before Phase 5 changes the invalidation.
5. **Oracle first** (§8). Phase 2 builds exporters for the IR, the vectors, the records and three
   trace kinds (game, per-test, search), plus a tiered corpus. Tier 1 is committed as plain NDJSON
   (at most 45 MiB, A22). Tier 2 is regenerated against a committed manifest. The Rust differential
   runner `gcg-diff` stops at the first divergence and prints a readable diff. Drill-down levels go
   from game to decision, to MCTS iteration, to single `apply` call.
6. **Workstreams with frozen contracts** (§10): A data, B1 IR/registry, B2 view+interpreter+hooks,
   C engine core, D1 policy/evaluation, D2 search/agents, E runner/reports/CLI, F eval, G oracle and
   diff. Types and traits are committed first, in lead-owned `contracts.rs` files. Function
   signatures are frozen in `docs/dev/contracts.md`. Functions land only with real bodies (no
   `todo!` is ever committed). The engine lands layer by layer along Python's import DAG.
7. **Threads, not processes** (§7). `gcg_core::pool` claims matches with an atomic counter and has
   a shared fork-join queue. Idle workers take subtasks of running matches ("help-first", blocking
   waits, no spinning, no back-off). Results are stored by index, so outputs are identical for any
   `--workers`. `--workers auto` sums `hw.perflevelN.logicalcpu`.
8. **Strengthened search is a fork** (§12). `gcg-search` starts as a copy of the reference, and
   its all-mechanisms-off configuration replays the reference bit for bit (null-config test).
   Every Phase 6 mechanism is a `StrengthenedConfig` field, so every ablation row is a flag flip.
9. **New hazard found while planning** (§14 R1). The reference's equivalent-move fingerprint keeps
   a once-per-turn record when `uid in key` and rewrites every number equal to the uid
   (`ai/actions.py:308-310`). Lasting-rule once keys are `(tag, -1, effect_key, serial, uid, seq)`
   (`engine/core.py:524`, `:561`, with the rule key `(-1, effect_key, serial)` from
   `engine/view.py:451`), and `effect_key` is first-use numbering inside one Python process
   (`effects/registry.py:124-130`). So Python's root merges, and even its own
   `--workers 1`/`10` identity, can depend on which matches a worker ran before. Phase 2 adds a
   numbering-shift probe that measures whether this ever fires (§8.9).
10. **Builds are offline; one batched dependency request goes to the user** (A8, A32). The
    workspace uses the crates already in the local cargo cache, plus a short list that the user
    is asked to approve once at the M1 gate: `signal-hook` for SIGINT, `mimalloc` for the
    allocator measurement, a zstd encoder for W-99, and the `llvm-tools` rustup component for
    PGO. They are fetched once (`cargo fetch --locked`), and every build stays
    `--locked --offline`. Cheap in-house code stays in-house: argument parser, pool, schema
    validator, property harness, NFKC tables and small inline arrays. The workspace keeps
    `unsafe_code = "deny"`; the `erfc` FFI is the only exception.

## 0.1 Lead review at M1 (overrides the plan where they differ)

The lead reviewed this plan against `docs/port/BRIEF.md` on 2026-11-01 and accepted it with
these decisions. They replace the M1 user batch of A32 for every item that the brief already
authorises, so M2 starts without waiting.

| Item | Decision | Authority |
| --- | --- | --- |
| A8 / A32 (1): dependency fetch | Approved by the lead. `signal-hook` (+ `signal-hook-registry`), `mimalloc` (+ `libmimalloc-sys`, `cc`), one compression crate for W-99 (zstd or `flate2`), and the `llvm-tools` rustup component are fetched once, `--locked`. Builds and tests stay `--locked`; `--offline` is used once the lockfile and cache are complete. The in-house list of A8 stands (argument parser, pool, schema validator, property harness, Unicode tables from Python) | The brief requires the allocator comparison and PGO evaluation (step 18) and exit 130 on interrupt; it forbids only run-time network access (`BRIEF.md` constraints). The user authorised local, reversible actions; a build-time fetch is one. The crates `sha2`, `serde_json`, `zmij` and `libm` were fetched the same way in Phase 0 |
| A32 (2): SIGINT fallback | Not needed: `signal-hook` is approved | — |
| A32 (3): W-120 | Deferred to the user, asked once near the end of the port (it changes this project's Claude permission settings, which only the user may decide). Nothing else depends on it | Global instructions: permission changes belong to the user |
| A32 (4): §12.11 | Answer (a): the in-scope resolutions are done port-side; the package edits are listed as post-acceptance steps. `src/gcg_sim` and `tests/` stay untouched (A15) | The brief keeps the Python package untouched until the port is accepted |
| A32 (5): W-41 data | No tournament data is available; W-41 is resolved by the tool and its synthetic test, as planned | — |
| A32 (6): W-122 bytecode | Not deleted (the lead did not create it); W-122 is resolved by the lint, as planned | The brief: ask before deleting files you did not create |
| §15 lead items | W-100 default (A23) accepted. Probe outcome R1: decided when measured. D14a: decided at M11 | — |

Lead decisions at the end of Phase 6 (2026-11-04, after the gap-closing records):

| Item | Decision | Authority |
| --- | --- | --- |
| W-05 scenario bar | The measured break-even trick share (0.497 under `ROLE_WEIGHTS`, `gapa1_w05_fusion_values.rs`) replaces the 13-of-16 bar: at a share of one half the two moves differ by 0.0002 in the search's own evaluation, so the bar has no reliably right answer. The mechanism (`opponent_buckets`) measured null and is not adopted | Brief step 22: keep only changes with a measured non-negative effect, record the outcome |
| W-111 rules, FAQ, rulings | Tag-based coverage stands: every tagged Python test is replayed decision by decision against Python's own trace (3213/3213 per-test traces), so a passing tagged test reproduces Python's behaviour exactly. Per-rule code-path probes are not added | GAP-B's recommendation; the cost (about 395 rules over the whole engine) buys no equivalence evidence the traces do not give |
| W-63, W-67, T-154 | `oracle/templates/check.py --check` (in `scripts/verify_all_rust.sh --full`) is the assertion that every template reproduces its binding cards byte for byte, and `docs/dev/audit/templates.json` the checked report. The IR gets no `cards[].template` field: it would change the IR bytes, and so every corpus trace's `ir_sha256`, for a field no Rust code path reads | Rust runs the exported scripts, not the templates (§2.4); the IR is the equivalence artefact |
| W-137 item 3 | `portable_math` is not adopted in the presets: its rows are null, and adopting it after the final evaluation would change the evaluated presets' bits. The presets take `exp` and `ln` from the platform library; bit identity across platforms is claimed only with `portable_math` on. Item 2 (the Linux x86-64 run) is the `portable-math-linux` job of `.github/workflows/rust.yml`, which runs once the user approves a push | §12.4: adopt only with a measured non-negative row; the evaluated presets stay the shipped ones |

## 1. Assumptions and decisions

### 1.1 Deviations of the repository from the brief's orientation

| # | Brief says | Repository says | Plan |
| --- | --- | --- | --- |
| D1 | Python 3.12 | Python 3.15.1 under uv 0.13.x (`docs/port/progress.md`). `sum()` of floats is Neumaier-compensated (3.12+, Inv 09 §5.1), and argparse texts are 3.15's (Inv 01 §3.3) | The golden platform is Python 3.15.1 on this M2 Pro |
| D2 | reference presets `standard`, `strong` | three reference presets `fast`/`standard`/`strong` (`ai/config.py:80-111`), plus `BASELINE_PRESETS` `standard`/`strong` (`ai/baseline.py:18-73`) and `TUNING_CONFIG` (`ai/tuning.py:50-57`) | `gcg-eval --reference-preset fast\|standard\|strong`. "Both reference budgets" (Phase 4) is read as `standard` and `strong`; `fast` is added to every equivalence matrix (W-160). Baselines are reachable as the `gcg-eval` agent specs `ref:baseline-standard` and `ref:baseline-strong` |
| D3 | 1148 card numbers | 1152 card numbers and 1153 definitions, counting the synthetic `UNIT:GD05-089:3/3`. 302 are implemented by binding, 538 compiled, 313 vanilla (`docs/port/INVENTORY.md`, Inv 04 §2) | "1148/1148" is read as 1152/1152 real cards plus 1153/1153 definitions |
| D4 | about 2,900 rule/card tests | 3,985 collected tests; `tests/rules` 647 + `tests/cards` 2,245 = 2,892 (Inv 10 §0) | Per-test traces cover those 2,892 tests plus the engine-level AI tests |
| D5 | `clone` about 15 µs | 17.4 µs at probe states, 21.4 µs mean in game (Inv 11 §6) | Performance targets use the measured numbers |
| D6 | budgets are node counts | budgets are iteration counts: `clamp(ipo * n, min, max)` over the distinct root moves (`ai/config.py:55-57`). Each iteration adds at most one tree node (`ai/mcts.py:205-235`) | "node" = one MCTS iteration (§6.6) |
| D7 | vs greedy 289/400, Wilson [0.677, 0.764] | that is the pre-change result. The shipped preset scored 292/400, [0.684, 0.771] (`docs/python/AI.md:352-360`). Both runs used the AI test decks of `tests/ai/decks.py` (`tests/slow/test_ai_strength.py:28-53`) | The bar stays the brief's: strengthened `standard` vs greedy needs a lower bound > 0.764, gated on the test decks (like for like) and on the panel (A25) |
| D8 | process pool | `ProcessPoolExecutor(spawn)`, with SIGINT ignored in workers (`runner/run.py:88-106`) | Rust uses threads (§7). Behaviour is kept, and W-106/W-107 are resolved by construction |
| D9 | — | Python output does not depend on `PYTHONHASHSEED` (Inv 09 §1.4) | Oracle outputs are generated under hash seeds 1 and 2 and must agree (W-150) |
| D10 | "information-set MCTS" | the reference reads the true state at the root (`ai/mcts.py:89-120`) and is information-set honest only by convention (W-135) | The reference and greedy are ported against `&GameState`, because their `derived()` calls on the true state fill its view cache exactly as Python's do (§4.6), and the cache is part of byte equivalence. This is a deliberate deviation from W-135's plan text, which ports them against an `Observation`; P6-O records it in the W-135 entry. The strengthened search sees only an `Observation` built in its agent adapter (§12.3) |
| D11 | — | `_references` compares raw once-key numbers with uids (`ai/actions.py:308-310`), and those numbers include the process-local `effect_key` (`engine/core.py:524`, `:561`) | §8.9 numbering-shift probe; §14 R1; documented in `docs/dev/EQUIVALENCE.md` whatever it finds |
| D12 | Rust presets `standard` and `fast` | Python already has a `fast` preset with a smaller budget (clamp(7n, 16, 64)) | Rust `fast` starts from reference `standard` (brief), so "fast" names different budgets in the two binaries. `--help`, Rust `docs/AI.md` and results.json `ai.search.mapping` say so |
| D13 | "messages where tested" | the Python interrupt test asserts `communicate(timeout=60)` and an elapsed time under 30 s (`tests/cli/test_console_script.py:107-108`) | The Rust port of that test keeps every behavioural assertion (exit 130, message, empty stdout, no `results.json`, `:109-112`) and drops both timing assertions, as the brief requires |
| D14 | `docs/dev/ARCHITECTURE.md` | `docs/ARCHITECTURE.md` already describes the Python package | The Rust architecture goes to `docs/dev/ARCHITECTURE.md`; the Python file is untouched |
| D14a | "Rust docs/AI.md" | `docs/AI.md` describes the Python AI. The Python docstrings (`ai/config.py:15,61,113`, `ai/evaluation.py:85`, `ai/tuning.py:3`), the README, the benchmark skill, WEAKNESSES.md (56 citations), this plan and Inv 11 cite it, many by line number | At M11 the lead runs `git mv docs/AI.md docs/python/AI.md`, which keeps the Python text byte for byte, and rewrites every `docs/AI.md:N` citation under `docs/dev/` to `docs/python/AI.md:N` in the same commit. The Rust document is then written at `docs/AI.md`, starting with one line that points to `docs/python/AI.md` for the reference, so the Python docstrings still lead to the right text. `gcg-diff weaknesses --check` checks that every cited `path:line` exists. The W-129 test (T-130) parses `docs/AI.md` |
| D15 | GitHub Actions workflow | no `.github/workflows/` directory exists | Phase 8 creates `.github/workflows/rust.yml` |
| D16 | — | `Cargo.lock` lists `serde_derive`, `syn`, `quote` and `proc-macro2`, but they are not in the local registry cache (`~/.cargo/registry/cache` holds 20 crates) | No derive macros; JSON is read through `serde_json::Value` (A8) |
| D17 | `scripts/verify_all.sh` mirrors the Python gate | it clones the committed HEAD into a temporary directory first (`scripts/verify_all.sh:10-16`) and ends with `scripts/verify_wheel_offline.sh` (`:30`) | `scripts/verify_all_rust.sh` clones the same way and ends with `scripts/verify_binary_offline.sh`, the Rust counterpart of the wheel check (§9.11) |
| D18 | "Run the phases in order" | the oracle (Phase 2) needs the effect IR and the CardDb fingerprints to label traces and to check `ir_sha256` | The IR exporter and the registry loader (B1) and the data-layer exporters (A) start at M2, inside Phase 2. None of this is engine code. No engine code (C, B2) starts before the Phase 2 gate M3 has passed |

### 1.2 Defaults chosen by this plan

| # | Default | Reason |
| --- | --- | --- |
| A1 | Golden platform for byte equivalence: this M2 Pro, macOS 27.2, Apple libm, Python 3.15.1, rustc 1.99.1 / LLVM 23.1.1 | Libm results are platform-specific (W-137). The reference port is bit-exact on the golden platform only |
| A2 | `erfc` goes through the existing FFI declaration in `crates/gcg-core/src/libm.rs:11-19` | Lead decision (W-137 option a) |
| A3 | SIGINT: `gcg_core::sys::sigint` is safe code over the vetted `signal-hook` crate (`signal_hook::flag::register(SIGINT, flag)`, which stores `true` to an `AtomicBool`). The workspace keeps `unsafe_code = "deny"`. The crate is not cached, so it is part of the M1 dependency request to the user (A32). A hand-written `libc::sigaction` module is the fallback only if the user refuses the crate. It would be `unsafe` that no profile justifies, so it would be a recorded deviation from the brief (`BRIEF.md:70`) and needs the user's sign-off at the same gate | std has no signal API, and the default SIGINT disposition gives returncode −2, not 130 (`tests/cli/test_console_script.py:109`) |
| A4 | Topology comes from spawning `/usr/sbin/sysctl -n hw.nperflevels hw.perflevel{0,1,2}.logicalcpu hw.perflevel{0,1,2}.name hw.cachelinesize` (safe `std::process::Command`), with `std::thread::available_parallelism()` as the fallback | Avoids a third `unsafe` module for `sysctlbyname` |
| A5 | `--workers` keeps default `1` and Python's help text "worker processes" (`cli.py:68-95`). `--workers auto` is the additive value. The brief's "default worker count from detected topology" means what `auto` resolves to | The brief requires identical defaults and allows an additive `auto` |
| A6 | Only `results.json` `versions.package`, `versions.code_digest`, `ai.factory` and their `summary.md` renderings are normalised (`scripts/normalize_outputs.py:22-32`). `gcg-eval reference-benchmark` therefore writes Python's preset names and agent names byte for byte (`"preset": "standard"`, agents `"mcts-standard"`) | Lead's fixed decision |
| A7 | In Phase 2, `scripts/normalize_outputs.py` becomes byte-exact. Today it re-serialises `results.json` (`:22-27`: `json.loads`, then `json.dumps(indent=2, sort_keys=True, ensure_ascii=False)`), which hides float, key-order and `ensure_ascii` differences. W-160 makes it substitute the three masked values textually in the raw bytes. The equivalence matrix gains `fast`, a `--decision-log` dimension, stale-baseline detection and a negative-control self-test (W-160). The M3 self-test proves that a one-byte change outside the three masked values (`1e-5` vs `1e-05`, key order, `ü` vs `ü`) is reported | Re-serialising hides Rust formatting bugs |
| A8 | **Dependency policy.** Builds are `--locked --offline`. Allowed crates: those in the local cargo cache (`sha2` + `digest`, `block-buffer`, `crypto-common`, `generic-array`, `typenum`, `cpufeatures`; `zmij`; `serde_json` with `preserve_order` + `indexmap`, `hashbrown`, `foldhash`, `equivalent`, `itoa`, `memchr`, `serde_core`; `libc`; the pure-Rust `libm` 0.2.16), plus the crates of the M1 request if the user approves them (A32): `signal-hook` (+ `signal-hook-registry`), `mimalloc` (+ `libmimalloc-sys`, `cc` and its build dependencies), one zstd encoder (for W-99), and the `llvm-tools` rustup component. The request names the exact crate versions that `cargo tree` resolves, and asks whether to keep them in the cargo cache (`cargo fetch --locked`, the default) or also vendor them into `vendor/` with `cargo vendor` (adds their sources to the history). CI runs `cargo fetch --locked` before any `--offline` step. Cheap replacements stay in-house: argument parser, pool, schema validator, property harness, NFKC tables, small inline arrays. No proc-macro crates, `clap`, `rayon`, `regex`, `jsonschema`, `proptest` or `criterion`. If the user refuses a crate, the consequence is recorded and nothing is silently dropped: the allocator or PGO comparison is reported as not measured, with the reason (a deviation from brief step 18); W-99 falls back to an in-house gzip writer (§12.2 P6-M); SIGINT falls back per A3 | The brief forbids only run-time network access (`BRIEF.md:71`). CLAUDE.md reserves network use for `/gcg-refresh-data`, so a build-time fetch needs the user's approval |
| A9 | `gcg-sim` agent names are `search-standard` and `search-fast`, with `ai.factory` `gcg-search::make_agent('<preset>')`. `gcg-eval reference-benchmark` writes the factory `gcg-ai-reference::make_agent('mcts')` | Agent names are seed-path material (`ai/base.py:65`). Distinct names keep runs self-describing and never collide with `mcts-*` |
| A10 | `gcg-sim` writes results schema 1.2.0 (§12.7). `gcg-eval reference-benchmark` writes 1.1.0, byte-identical to Python | Brief step 24 |
| A11 | The `--ai-preset` help quotes the brief's wording for each value. Rust `docs/AI.md` puts the measured strength next to it | The brief requires the wording; measured numbers avoid unmeasured claims |
| A12 | Long tests (strength, 10,000-game fuzz, tier-2 corpus, 100-match e2e) are `[[test]]` targets with `required-features = ["slow"]`. `scripts/verify_all_rust.sh` runs them. Nothing uses `#[ignore]` | "never skip" and W-112 |
| A13 | Every WEAKNESSES item gets a row in the ablation table. AI mechanisms get individual and leave-one-out rows. Other items say "no AI effect", name the test that proves their behaviour, and never carry an invented measurement. A "no AI effect" row needs evidence. If the item changes code that the AI executes (engine, runner or shared AI paths), the evidence is identical decision traces with the item on and off: the `search` and `baselines` corpus sets and the seed-1 equivalence subset, plus the gcg-search scenario tests. Otherwise the row states why no AI-executed path changes (for example, a validator message or a doc) | The brief asks for a row per item, with individual and combined effects |
| A14 | Seed roles with disjoint master ranges: tuning `[10_000_000, 19_999_999]`, dev `[20_000_000, 29_999_999]`, confirm `[30_000_000, 30_999_999]`, eval `[90_000_000, 90_999_999]`, review `[91_000_000, 91_999_999]`. `gcg-eval --role <role>` rejects a master outside its range. `docs/dev/eval/seed-ledger.json` records every master used. A unit test checks that the ranges are disjoint | Disjoint seeds, checkable by the reviewer |
| A15 | `src/gcg_sim/` and `tests/` are never edited or added to before acceptance. Every gate checks `git diff --quiet d9d41af -- src/gcg_sim tests` and that `git status --porcelain --untracked-files=all -- src/gcg_sim tests` prints nothing (gitignored `__pycache__` is not listed). Items whose full resolution would edit the package follow §12.11: the work that needs no package edit is done in scope (port-owned templates, scans, Rust gates, tests under `oracle/tests/`), and the package edits are raised with the user at the M1 gate (A32). Fixes outside the package are done in scope: the refresh-skill script (W-153), `docs/*.md` corrections (W-129, W-130) | Scope rule of the brief. After acceptance (#6, #7) the package changes only in a data refresh (§12.11 (a)), and the gate's baseline moves to that refresh's package commit: 8f2ba29 since the 2026-11-06 refresh |
| A16 | Per-test traces are generated in one process without `-n` (Inv 10 §10.1). The exceptions are the 10 tests that call `testkit.card_text` or `registry.override_script` (Inv 10 §9.2). Each runs in its own fresh process, so the AI's process-global memo caches (`ai/targeting.py:93-110`, `ai/actions.py:167-199`), which `card_text` never clears (`testkit.py:203-217`), can never carry an entry computed under one test's override into another test. In that fresh process the plugin asserts that every AI memo table is empty when the override starts. If a test filled one before its override, the plugin records the entries the test then reads, and Rust preloads them into the test's `AiTables` | Registry numbering is reproducible, the run is simple, and Rust's per-registry `AiTables` (W-33) then match what Python read |
| A17 | `timing.json` keeps Python's keys; `"python"` holds `"none (Rust build)"`, and W-104 adds the deterministic counters. It is excluded from every comparison | CLI parity only |
| A18 | The Rust testkit `Scenario` ports `testkit.py:38-182` behind the cargo feature `testkit` of `gcg-engine` (it needs private state access) | Scenario tests and the native lead-example ports |
| A19 | Where Python's error text embeds CPython exception text (`GameFailure` "… {TypeName}: {exc}", `KeyError` reprs, `strerror`), Rust matches the prefix, the exit code and the file side effects, not the embedded text. Each difference is listed in `docs/dev/EQUIVALENCE.md` | Inv 01 §5 Q6 |
| A20 | Five new evaluation decks (one spare) are committed in `examples/decks/eval/` (§12.5) | The brief requires at least four |
| A21 | **Per-decision state in the trace corpus.** Each decision records the first 16 hex digits of `state_digest` (an exact check of the canonical snapshot), the game RNG state, `version`, `action_count`, `event_group` and `_rules_checked`. Full canonical snapshots are stored at game start and end, every 32nd decision, after any state change outside `apply`, and at every decision of the `full` set. `oracle/traces.py state --trace ID --decision K` regenerates any snapshot on demand for `gcg-diff explain`. So a readable state diff at a decision without a stored snapshot needs `uv`; the divergence report itself (first differing fingerprint field, last action, options) never does. Every tier-1 set stores the full option list at every decision (`options_mode: "full"`), so a legal-action diff is always readable from the committed corpus; only tier 2 may store `options_sha16` | The brief's step 6 asks for a snapshot per decision. A full snapshot (about 6 KB) everywhere would make the corpus about 1 GB. The digest checks the same bytes exactly |
| A22 | **Corpus tiers.** Tier 1 is committed as plain NDJSON, at most 45 MiB (47,185,920 bytes; raised by the lead from 30 MiB on 2026-11-01, when the per-test set measured 22 MB) in files of at most 8 MiB. Tier 2 is regenerated by Python into `build/oracle/tier2/` and must match `oracle/corpus/v1/MANIFEST-tier2.json`. No decompressor is cached, so nothing is gzipped | The brief asks for a compact, regenerable corpus |
| A23 | W-100: `gcg-eval reference-benchmark` keeps Python's reproduce line in `summary.md` byte for byte (`reports/writer.py:44-51`). `gcg-sim` prints the corrected line (factory, deck paths), because its outputs are never byte-compared with Python. The lead may override (§15) | Keeps the normaliser at exactly the three fields |
| A24 | **Forced play/draw balance.** In every gate run (E1-E4) and every ablation row, `gcg-eval head-to-head` answers the CHOOSE_FIRST decision itself with `go_first(0)`: seat 0 goes first in every game. With `mirrored_specs`' blocks of four (`ai/selfplay.py:88-110`: two deals, each played twice with the agents swapped), each agent is then on the play in exactly two games of every block, plays each deck once on the play and once on the draw, and both games of a deal put the same seat and the same shuffled deck on the play. The agents never see CHOOSE_FIRST in these runs. The agent-chosen protocol (`--choose-first agents`, Python's `mirrored_specs` behaviour, `engine/game.py:153`) stays available, and it is the W-44 diagnostic and the W-44 ablation row. Every report gives the realised play/draw counts per agent, and a unit test checks the balance (T-149) | The brief requires BO1 games balanced on-play/on-draw (`BRIEF.md:126`). With agent choice only the opportunity to choose would be balanced, and a strengthened agent that chose to draw would shift its on-play share |
| A25 | `standard` vs random and vs greedy are gated twice: on the AI test decks with the seeds of `tests/slow/test_ai_strength.py` (400 games each), and on the full evaluation panel (400 games per pair). The test-deck rows are labelled "like for like with the reference's 0.764 bar, not held-out". The AI test decks are never used in tuning, ablation or budget curves | The 0.764 bar was measured on the test decks; the brief's deck set covers the panel |
| A26 | "Thresholds met on held-out pairs" is read strictly for every gate. E1: the pooled 95% lower bound of `standard` vs reference `strong` reaches 0.55 both over all 28 pairs and over the 13 held-out pairs alone. E2: the `fast` pooled point estimate reaches 0.5 both ways. E3: 400/400 against random in every pair, held-out pairs included, each pair reported on its own. E4: the pooled Wilson lower bound against greedy exceeds 0.764 over all pairs and over the held-out pairs alone. The per-pair upper-bound rules apply to every pair | Covers both readings of the acceptance criterion |
| A27 | Unicode tables (NFKC decomposition and composition, canonical combining class, full casefold, `str.isspace`) are generated from Python 3.15.1's `unicodedata` by `oracle/export_unicode.py` and checked in. The exporter records `unicodedata.unidata_version` | `deck/names.py:29-38` uses `unicodedata.normalize("NFKC")`, `str.split()` and `casefold()`. Rust std has none of them, and no Unicode crate is cached |
| A28 | The Python baseline for both comparison parts is measured once by the lead in Phase 0 and reused (`scripts/compare_python_vs_rust.sh python --best N`) | Brief `<performance_comparison>` |
| A29 | Commits happen only at milestones (§13), on `rust-port`, with no push without the user's approval. The CI deliverable needs a push, so the push request is part of the M17 user batch. `scripts/ci_local.sh` runs the same steps as `.github/workflows/rust.yml` locally, so the workflow is exercised before any push | Brief constraints |
| A30 | **The `gcg-sim` command line stays identical.** Every new option that a WEAKNESSES resolution introduces lives in a `gcg-eval` subcommand, never in `gcg-sim`: `gcg-eval benchmark` is `gcg-sim benchmark`'s pipeline with the shipped presets plus the opt-in `--analysis` (W-90, W-98, W-147, and `--swiss-draws`, W-109), `--compress` (W-99), `--resume` (W-105), `--replays-extremes` (W-148), `--diagnostics` (W-46) and `--opponent-list` (W-134); `gcg-eval replay-game --games-ndjson FILE --results RESULTS --match M --game G` (W-148); `gcg-eval replay-check FILE [--results R]` (W-156); `gcg-eval validate DECK [--as-of DATE] [--region R] [--no-banlist] [--event-date DATE] [--scope]` (W-72, W-75, W-76); `gcg-eval data-status [--sources] [--freshness [--as-of DATE]] [--templates]` (W-88, W-145, W-63). Tests: `gcg-sim --help` and every subcommand's help and usage errors equal Python's except the `--ai-preset` choices and help and the `--workers auto` value (T-152), and `gcg-eval benchmark` with no opt-in option writes the same bytes as `gcg-sim benchmark` (T-153). Each P6 owner writes the new location into its WEAKNESSES entry (§12.10) | The brief allows only `--ai-preset` and an additive `--workers auto` to differ (`BRIEF.md:51`). Added flags would also change `gcg-sim`'s help text |
| A31 | `gcg-diff` is a development and gate tool that is never shipped or installed. It may link `gcg-ai-reference` (search traces, `fuzz --agent ref:*`). The brief's "reachable only through `gcg-eval`" applies to the shipped binaries: `gcg-sim` (whose closure check, §2.2, stays) and `gcg-eval` | The differential runner must run the reference search to replay `search` traces |
| A32 | **M1 user batch.** At the M1 gate the lead asks the user once, with a recommended option for each: (1) the dependency fetch of A8 (crate list and versions, cache or vendor); (2) if (1) is refused for `signal-hook`, sign-off on the `sigaction` fallback (A3); (3) W-120: apply the proposed `.claude/settings.json` change; (4) the Python-side items of §12.11: accept the port-side resolution with the package edits as listed post-acceptance steps (recommended), or grant an exception to edit `src/gcg_sim` for them in Phase 6, or record them as open; (5) W-41: real tournament data, if the user has any (optional; the tool is resolved without it); (6) W-122: whether to delete the untracked bytecode (optional; not needed to resolve it). The answers go into `docs/port/progress.md` and this table. A second batch at M17 asks for the push that the CI run needs (A29) | Approvals and package edits belong to the user, and asking once at a gate avoids blocking later phases |

### 1.3 Decisions (alternatives considered)

| # | Decision | Alternatives | Why |
| --- | --- | --- | --- |
| X1 | Export the registry as a JSON IR; hand-port the semantics | Port `effects/compiler/` (2,773 lines; `grammar.py` 2,300 lines of Unicode-`\w` regex templates) and the 302 bindings | Card behaviour must not change. The IR is Python's own output (no floats, no callables, Inv 04 §10.1). Content ids need the first registrant's labels and Python's dedupe. No regex crate is cached. New sets need the Python compiler anyway (§3.2) |
| X2 | IR as a hash-consed node arena with insertion-ordered keys and a `schema` table | Nested node objects written with `sort_keys=True` | The arena is smaller and makes `NodeId` equality equal Python `==` (asserted by the exporter). With `sort_keys`, field order would survive only through the class table |
| X3 | Reference search in `gcg-ai-reference`, strengthened search in `gcg-search`, shared exact components in `gcg-ai` | Strengthened search inside `gcg-ai` | `gcg-cli` must not reach the reference, and the shared components must stay frozen. Three crates make both properties checkable from `Cargo.toml` files |
| X4 | Derived-view cache `RefCell<Option<Arc<Derived>>>`, with `view::derived(&GameState)` filling the cache | (a) `derived(&mut)` plus a compute-without-store `view(&self)`; (b) an inline slot with `derived(&mut) -> &Derived` | (b) does not compile at Python's held-view sites: E0502 when a borrowed view is passed next to `&GameState`. (a) relies on cache transparency for the AI's true-state calls. The `RefCell` reproduces Python's sharing by construction (`state.py:570`) |
| X5 | Typed `OnceKey` whose Python integer positions are reproduced exactly. The `ContRef` slot never equals a uid. The numbering-shift probe checks whether Python depends on numbering | Packed `[i64; 6]` once keys | A `ContRef` cannot live in an `i64` slot, and a fake number would invent Python's process-local numbering |
| X6 | Tiered corpus, plain NDJSON: tier 1 ≤ 45 MiB committed (A22), tier 2 regenerated against a committed manifest | (a) 70-80 MB of plain NDJSON committed; (b) `*.ndjson.gz` | (a) is not compact and grows history on every regeneration. (b) needs a decompressor, and none is cached |
| X7 | Digest plus periodic snapshots per decision (A21) | A full snapshot at every decision | About 1 GB. The digest is an exact check |
| X8 | `gcg_core::pool`: atomic-counter match claiming plus one shared fork-join queue with help-first blocking waits (§7.1) | (a) a plain atomic counter; (b) per-worker deques with randomised stealing; (c) `rayon` | (a) leaves fast cores idle in the tail. (b) adds complexity: tasks are coarse (≥ 1 ms), so a shared queue under one mutex does not contend at ≤ 12 workers, and results are slot-indexed, so stealing order cannot matter. (c) is not cached. Idle workers do take ("steal") pending subtasks of other workers' matches, which is the work-stealing property the brief names |
| X9 | SIGINT through `signal-hook`'s safe `flag::register`, fetched once with the user's approval (A3, A32) | `ctrlc` (pulls in `nix`); a hand-written `sigaction` module; default disposition | `signal-hook` needs only `signal-hook-registry` and the cached `libc`, and keeps `unsafe` out of the workspace. A hand-written module is `unsafe` that no profile justifies (`BRIEF.md:70`), so it is only the fallback, with the user's sign-off. The default disposition fails the ported test |
| X10 | Topology from a `sysctl` subprocess | `sysctlbyname` FFI | Same data with no `unsafe` |
| X11 | mimalloc is measured in Phase 5. Fetching it (with `cc`) is part of the M1 user request (A32) | Banned outright; or "only if allocation stays visible" | Brief step 18 lists "allocator (mimalloc vs system)" as something to apply and measure. If the user refuses, BENCHMARKS.md records the measurement as not done and why |
| X12 | `gcg-search` v0 is a copy of the reference with a null-config replay test | Strengthened search written fresh | Every ablation row becomes a flag flip on one code base. The null config proves the starting point |
| X13 | The harness answers CHOOSE_FIRST (seat 0 first) in every gate and ablation run; the agent-chosen protocol is the W-44 diagnostic (A24) | Agents choose play/draw in every run | The brief requires games balanced on-play/on-draw. With agent choice, only the opportunity to choose is balanced |
| X14 | Agent names `search-*` | `smcts-*` | Short, and distinct from `mcts-*` |
| X15 | Seed roles with ranges (A14) plus a ledger | A ledger alone | Ranges make disjointness checkable from the seed value alone; the ledger records actual use |
| X16 | Held-out = every pair containing either of two held-out decks (13 of 28 pairs), pre-registered in `docs/dev/eval/panel.json` and `examples/decks/eval/PAIRS.md` | Four hand-picked held-out pairs | More held-out evidence, and the tuning decks stay fully separate |
| X17 | Long tests through `required-features = ["slow"]` | `test = false` targets run by name | Both avoid `#[ignore]`. Features make `cargo test --workspace --features slow` one command |
| X18 | Testkit as a `gcg-engine` feature | A separate `gcg-testkit` crate | The testkit writes private state. A feature reached only through dev-dependencies does not enter `gcg-cli`'s normal build (resolver 3) |
| X19 | `code_digest` computed by `build.rs` of the two binaries (shared repo-root `build-support/code_digest.rs`, outside the `crates/*` member glob) | `build.rs` in `gcg-data` | A digest in `gcg-data` would rebuild the whole workspace on every source change. A directory without a `Cargo.toml` under `crates/` would match `members = ["crates/*"]` and break the workspace load |
| X20 | `gcg_core::pool` (the AI needs fork-join for `first_player_choice` and root trees) | Pool in `gcg-runner` | `gcg-ai` cannot depend on `gcg-runner` |
| X21 | Compile-fail checks as rustdoc ```` ```compile_fail ```` doctests | `trybuild` | `trybuild` is not cached; rustdoc is built in |
| X22 | Portable math for the strengthened search from the cached pure-Rust `libm` crate | In-house fdlibm | Already cached and deterministic across platforms (W-137) |
| X23 | Engine split along Python's import DAG (state → view → core → battle → interp → game; observe → view), with two owners landing alternate layers | (a) one owner for 5,700 Python lines; (b) skeleton with `todo!` bodies | (a) is too large for one context. (b) commits placeholders, and `panic = "abort"` makes them uncatchable |
| X24 | W-100 fix only in `gcg-sim` (A23) | Behind a flag in both binaries | `gcg-sim` output is never byte-compared with Python |
| X25 | Facet-versioned view revalidation is a Phase 5 candidate behind `paranoid` | Adopted in Phase 3 | It needs the transparency proof first and must show a measured win |

## 2. Crate layout

### 2.1 Crates

| Crate | Kind | Responsibility | Python counterpart |
| --- | --- | --- | --- |
| `gcg-core` (exists) | lib | `SplitMix64`, `derive_seed!`, `pyrepr`, `pyfloat`, `pyjson`, `pymath` (Neumaier `py_sum`, `floordiv`, `pymod`, `first_max_by`/`first_min_by`, `py_max2`/`py_min2`), `libm` (`erfc` FFI, `pow`/`square` through `black_box`, `exp`, `ln`, `sqrt`). Adds: `small` (fixed-capacity inline arrays with an overflow error), `portable` (re-exports of the pure-Rust `libm` crate's `exp`/`log` for the strengthened search, W-137), `stable_hash` (SHA-256-based 64-bit keys, never `std::hash`), `prop` (property-test harness, §9.6), `pool` (§7.1), `sys::topology` (A4), `sys::sigint` (A3, safe code over `signal-hook`) | `rng.py`, CPython semantics |
| `gcg-data` | lib | Card model, `CardDb` (cards.ndjson + overrides + inline tokens + unit variants, def_id order), official data (construction rules, B&R, rules version, BO3 rules), deck parser and validator with Python's messages, generated Unicode tables (A27), `versions(package, code_digest)`, conflict resolutions, the `data status` payload | `cards/`, `deck/`, `reports/versions.py`, `cli.py:236-331` |
| `gcg-effects` | lib | DSL node types (137 classes and 14 enums, Inv 04 §3), program instructions (9 classes and `Program`, `effects/program.py:15-91`), the IR loader, `Registry`, `ContRef`, CPython `repr` and content ids, the golden-format writer, hook ids and typed hook parameters, the static string/var/prompt tables | `effects/dsl.py`, `program.py`, `registry.py`, `golden.py`, hook names in `effects/bindings/*` |
| `gcg-engine` | lib | `GameState` and records, `touch`/`version`, the derived view and its fixpoint, expression evaluation, the interpreter with resumable frames, the 34 hooks, zones/events/triggers/damage/rules management, setup and the step machine, legal actions, `apply`, battle, observe (`determinize`, `permute_hidden`, `information_set_key`), invariants, canonical JSON, `state_digest`, `from_canonical`; features `testkit` (A18) and `paranoid` (W-49) | `engine/*`, `effects/builtins.py`, hook bodies in `effects/bindings/*`, `runner/digest.py`, `testkit.py` |
| `gcg-ai` | lib | The `Agent` trait, `AgentCx`, `AgentCore` and the decision log; exact ports of move keys, canonicalisation, pruning, merging, fingerprints, the no-effect check, pre-game choices, playout policy, values, targeting, evaluation and weight tables, playouts; `RandomAgent`, `GreedyAgent` | `ai/base.py`, `actions.py`, `noop.py`, `setup.py`, `policy.py`, `values.py`, `targeting.py`, `evaluation.py`, `playout.py`, `agents.py:18-66` |
| `gcg-ai-reference` | lib | The reference ISMCTS, `ReferenceConfig` (the 24 `SearchConfig` fields with Python's defaults), presets `fast`/`standard`/`strong`, `BASELINE_PRESETS`, `TUNING_CONFIG`, `make_reference_agent` | `ai/mcts.py`, `config.py`, `baseline.py`, `agents.py:69-127` |
| `gcg-search` | lib | The strengthened search, `StrengthenedConfig` (no `Default`, W-136), presets `standard` and `fast`, the Phase 6 mechanisms. Phase 3 ships it as a copy of the reference whose null configuration replays the reference | new |
| `gcg-runner` | lib | Benchmark config, seeds, BO1/BO3 matches, game records, the DUT observer, every report (`results.json`, `summary.md`, `games.ndjson`, `replays/*.json`, `timing.json`), statistics and hypotheses, the schema-subset validator, replay verification, the test agents `test-random` and `test-illegal` (`tests/runner/random_agent.py`) | `runner/*`, `reports/*` |
| `gcg-cli` | bin `gcg-sim` | argparse-compatible parser; `validate`, `benchmark`, `data status`, `replay`; exit codes 0/1/2/130; progress output; the two-value `--ai-preset`; `build.rs` code digest | `cli.py` |
| `gcg-eval` | bin `gcg-eval` | `reference-benchmark` (every `gcg-sim benchmark` flag plus `--reference-preset fast\|standard\|strong` and `--agent test-random\|test-illegal`), `head-to-head`, `compare` (exact port of `ai/compare.py`), `preset-benchmark --search-config FILE` (§12.8), `env` (topology and CPU features), the Phase 6 tools (`ablation`, `budget-curve`, `fidelity`, `regret`, `tune`, `calibrate`, `compare-runs`, `transcripts`, `decks`, `gates`), and the opt-in user tools of A30 (`benchmark`, `replay-game`, `replay-check`, `validate`, `data-status`); `build.rs` code digest | `ai/compare.py`, `selfplay.py`, `tuning.py`, new |
| `gcg-diff` | lib + bin `gcg-diff` (gate tool, never shipped; A31) | Trace reader, differential replay of game/test/search traces, readable diffs, `trace-search` drill-down, `fuzz` (port of `tools/fuzz.py:28-166`, plus strategic agents, W-110), `paranoid-games` (the 16 random and 2 greedy games of `tests/test_engine_caches.py`), corpus manifest check, selftest, coverage census, traceability and `tests.json` checkers, schema-check, lints, the W-id partition check | new |

The brief's required crates map as follows: data → `gcg-data`; effects → `gcg-effects`;
engine → `gcg-engine`; ai → `gcg-ai`, `gcg-ai-reference`, `gcg-search`; runner/reports →
`gcg-runner`; cli → `gcg-cli`; eval → `gcg-eval`.

### 2.2 Dependency graph

```text
gcg-core
  └─ gcg-data
       └─ gcg-effects
            └─ gcg-engine
                 └─ gcg-ai ──┬─ gcg-ai-reference
                             ├─ gcg-search
                             └─ gcg-runner        (Agent trait, LogEntry only)
gcg-cli   = gcg-runner + gcg-search + gcg-data + gcg-effects + gcg-core
gcg-eval  = gcg-runner + gcg-ai + gcg-ai-reference + gcg-search + gcg-data + gcg-core
gcg-diff  = gcg-engine[testkit] + gcg-ai + gcg-ai-reference + gcg-search + gcg-runner
            (feature `paranoid` of gcg-diff enables `gcg-engine/paranoid`; off by default)
```

Reachability rule: `gcg-cli`'s dependency closure never contains `gcg-ai-reference` or
`gcg-diff`. `gcg-diff` links the reference, but it is a gate tool that is never shipped (A31). Two checks enforce it. `crates/gcg-cli/tests/no_reference.rs` reads the `Cargo.toml`
of every workspace crate in the closure and fails if any lists either crate; it needs no cargo
invocation and no network. `scripts/verify_all_rust.sh` also fails if
`cargo tree -p gcg-cli -e normal --offline` prints `gcg-ai-reference`. Cargo feature unification
cannot leak it, because no feature is involved.

Feature hygiene: Cargo unifies features across the packages of one build, so a feature that
`gcg-diff` enabled unconditionally would enter `gcg-sim` in a `--workspace` build. Therefore
`testkit` is purely additive (it adds constructors and has no effect on normal code paths), and
`paranoid` is a non-default feature of `gcg-diff` and `gcg-engine`, built into its own target
directory (`--target-dir target/paranoid`). Shipped and compared binaries are always built with
`-p gcg-cli -p gcg-eval` (`scripts/compare_python_vs_rust.sh:39`). `scripts/verify_all_rust.sh`
runs `cargo tree -p gcg-cli -e features --offline` and fails if `paranoid` appears.

### 2.3 Binaries

- `target/release/gcg-sim` from `gcg-cli`, and `target/release/gcg-eval` from `gcg-eval`. The
  compare script builds both packages by name (`scripts/compare_python_vs_rust.sh:39`).
- `target/release/gcg-diff` is a development and gate tool, not shipped.
- No `benches/` directory, no `[[bench]]` targets, no criterion (§9.9).

### 2.4 Where data, the IR and test artefacts live

| Artefact | Location | Embedded how |
| --- | --- | --- |
| Packaged sources: `gcgapi/cards.ndjson`, `gcgapi/manifest.json`, `gcgapi/LICENSE-DATA`, `overrides.json`, `curated_conflicts.json`, `official/*.json`, `rules/rules_index.json`, and the repo-root `data/SOURCES.lock.json` | unchanged in place (single source of truth, read-only; CLAUDE.md "cached sources are read-only") | `include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/gcg_sim/data/…"))` in `gcg-data/src/embedded.rs`. The list is every file the Python runtime reads (Inv 03 §0) |
| Effect IR | `crates/gcg-effects/ir/registry.json` + `registry.json.sha256` | `include_str!` in `gcg-effects/src/ir.rs`, parsed once into `OnceLock<Arc<Registry>>` through `serde_json::Value` (no derive macros) |
| Results schema 1.1.0 | `src/gcg_sim/reports/schema/results.v1.schema.json` (Python's file, not copied) | `include_str!` in `gcg-runner/src/reports/schema.rs` |
| Results schema 1.2.0 | `crates/gcg-runner/schema/results.v1_2.schema.json` (new, Phase 6) | same |
| Unicode tables | `crates/gcg-data/src/unicode/tables.rs` (generated, checked in) | compiled |
| Embedded deck sets: `tests` and `tuning` of `ai/compare.py:52-57`, `tests/ai/decks.py`, `ai/tuning.py:89-93` | `crates/gcg-eval/decks/*.txt` (generated by `oracle/export_decks.py`) | `include_str!` |
| Trace corpus | tier 1 `oracle/corpus/v1/tier1/**/*.ndjson` with `MANIFEST.json`; tier 2 `build/oracle/tier2/` (gitignored) checked against `oracle/corpus/v1/MANIFEST-tier2.json` | read by `gcg-diff` at test time, never embedded |
| Vectors | `crates/<crate>/tests/vectors/*.json` (`gcg-core` keeps `tests/vectors.json`) | read by tests |
| Evaluation decks and panel | `examples/decks/eval/*.txt`, `examples/decks/eval/PAIRS.md`, `docs/dev/eval/panel.json` | read at run time |
| Evidence (ablation rows, strength runs, notes) | `docs/dev/eval/` (plain JSON/NDJSON) | not embedded |
| Licences and notices | `src/gcg_sim/data/gcgapi/LICENSE-DATA` (ODbL v1.0) stays in place. The Rust README keeps the ODbL and Bandai/SOTSU·SUNRISE notices of `README.md:152-159`. `scripts/build_dist.sh` ships `LICENSE-DATA` and the notice next to distributable binaries | `include_str!` of LICENSE-DATA for `data status` |

`code_digest` (W-103): the `build.rs` of `gcg-cli` and of `gcg-eval` includes the shared
`build-support/code_digest.rs` at the repository root (outside the `crates/*` member glob, X19)
with `include!`. It hashes, in sorted path order, every file listed in
`build-support/digest-manifest.txt`: all crate sources, the embedded data and the IR. The
algorithm is Python's (`reports/versions.py:44-52`): for each file, `path`, then `b"\0"`, then
the bytes, all fed to one SHA-256, as lowercase hex. The result is exposed as
`env!("GCG_CODE_DIGEST")`, and the package version is `CARGO_PKG_VERSION`. Both are normalised
fields (A6).

## 3. IR versus compiler port

### 3.1 Decision

Export the Python registry as a versioned JSON IR. `oracle/export_ir.py` generates it, and
the result is checked in and embedded in the binary. Port the interpreter, the view semantics,
the engine primitives and the 34 custom hooks by hand. Do not port `effects/compiler/` (2,773
lines), `effects/text.py` or the 302 bindings. The shipped binaries run fully offline with no
Python at run time.

### 3.2 Trade-off

| Factor | IR export (chosen) | Compiler + bindings port |
| --- | --- | --- |
| Size of Rust work | IR loader, node types, `repr`, golden writer (estimate: about 3,000 lines) | `grammar.py` 2,300 lines of regex templates with Unicode `\w`/`\b` (`effects/text.py:14,83,88`), `nouns.py` 364, and 302 bindings in 17 modules that call `compile_parts`/`route_abilities`; plus the IR anyway |
| Libraries | `serde_json` (cached) | a regex engine with CPython semantics; none is cached (A8) |
| Program ids, aid numbering, dedupe, first registrant's label | exact by construction: the exporter writes Python's numbers (`effects/registry.py:115-122`, `:323-340`) | must re-derive the build order and the structural dedupe; 364 abilities run a program labelled with another card's number |
| `final_state_sha256` | content ids of all 626 programs are exported, so the Rust `repr` is checked against Python before any game runs | same `repr` port needed, with less verification data |
| New sets (W-145) | Python compiler and bindings, then `export_ir.py --write`; `/gcg-refresh-data` already runs the Python workflow | Rust compiler and Rust bindings, with templates that drift from Python's |
| Risk of silent behaviour change | low: the IR is Python's own data, and golden files are regenerated from it as a cross-check | high: two compilers that must agree on 1,152 cards |
| Python at run time | none (IR embedded) | none |

The deciding facts are four. Card behaviour must stay unchanged, and the Python package stays
the reference. The compiler's output is fully captured by the registry (Inv 04 §8, §10.1: every
node is plain data, with no floats and no callables). The digest needs Python's labels and
dedupe. The cost is that new sets require the Python compiler, which `/gcg-refresh-data` runs
anyway.

Two consequences follow, and the plan accepts both:

1. **The Python effect toolchain outlives the port.** After acceptance, `src/gcg_sim/effects`,
   `src/gcg_sim/cards` and `src/gcg_sim/data` stay in the repository as the IR generator
   (`oracle/export_ir.py` imports them). A data refresh or a new set (W-145) runs the
   Python compiler and bindings, then `export_ir.py --write`, then the Rust gates. Phase 8 writes
   this into CLAUDE.md and the refresh skill. Retiring the Python runtime (engine, AI, runner)
   is a separate later decision for the user, and it must keep these three directories.
2. **Some weakness resolutions would edit the frozen package.** W-63 and W-67 (general
   templates), W-66 and W-68 (fixing or retiring bindings), the docstring part of W-129 and the
   refresh-diff class of W-155 change `src/gcg_sim`, which the brief freezes until acceptance.
   §12.11 gives their resolution path: port-owned templates and gates in scope, proven
   golden-identical, and the package edits raised with the user at the M1 gate (A32).

### 3.3 IR schema (version 1)

The IR is one JSON object. Python writes it with
`json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n"`. Keys are
**insertion-ordered**: field order is semantic, so `sort_keys` is not used. The `schema` table
repeats the field order anyway, so a loader never depends on key order.

```text
{
  "ir_version": 1,
  "generator": {"exporter": "oracle/export_ir.py", "python": "3.15.1",
                "gcg_sim_code_digest": "<reports.versions.code_digest()>",
                "git": "<git rev-parse HEAD>", "hash_seeds_checked": [1, 2]},
  "inputs": {"<repo path>": "<sha256>", ...},  // every src/gcg_sim/**/*.py and every packaged data
                                               // file (the code_digest file set), plus the port
                                               // exporter and oracle/templates/**; this covers
                                               // engine/interp.py system programs, core.HAND_LIMIT
                                               // and engine/types.py Duration
  "data_lock_sha256": "<sha256 of data/SOURCES.lock.json>",
  "db_fingerprints": {"carddefs": "...", "def_ids": "...", "token_keys": "...",
                      "applied_overrides": "..."},                       // Inv 03 §9
  "schema": {"<ClassName>": [["<field>", "<annotation>", <default JSON or null>, <has_default>], ...]},
  "enums": {"<EnumName>": [["<MEMBER>", "<value>"], ...]},              // declaration order
  "printable_non_ascii": ["①", "Ⅱ", ...],     // every non-ASCII character in IR strings
  "strings": ["...", ...],                     // string table (StrId = index)
  "var_names": ["__cost0", ..., "card", "it", "looked", "pilot", "t", "unit", ...],
                                               // IR names, every LoopInit key (__loop{n}, which
                                               // also keys Frame.ints), and the names that engine
                                               // sites write: __todeck (interp.py:808), __play
                                               // (:1409), it and itN (:84-86), card/pilot/unit
                                               // (game.py:931,941), looked, t (builtins.py:16,68);
                                               // sorted by rendered string (__loop10 < __loop2)
  "nodes": [<node>, ...],                      // hash-consed arena, index = NodeId
  "node_content_ids": ["<16 hex>", ...],       // sha256(repr(node))[:16] per node
  "programs": [{"label": "<str>", "content_id": "<16 hex>", "instrs": [<NodeId>, ...]}, ...],
  "system_programs": {"deploy": 623, "pair": 624, "hand_limit": 625},
  "keyword_programs": {"repair": 0, "breach": 1, "support": 2},
  "abilities": [{"def_id": 0, "card_number": "...", "index": 0, "unit_text": false,
                 "ability": <NodeId>, "program_id": 17, "cost_program_id": -1,
                 "trigger_event": "<Ev value>" | null}, ...],           // index = aid
  "cards": [{"def_id": 0, "card_number": "...", "source": "binding|compiled|vanilla",
             "binding_module": "wp_..." | null, "text_hash": "...", "script": <NodeId> | null,
             "own": [aid], "unit": [aid], "command_aid": -1, "burst_aid": -1, "has_burst": false,
             "timings": ["<sorted>"], "field_entries": [aid], "hand_entries": [aid],
             "trash_entries": [aid], "paired_entries": [aid], "unit_entries": [aid],
             "gated": false, "events": ["<sorted Ev values>"],
             "printed_keywords": [["<Kw value>", 1]],
             "delayed": [[<n>, <DelayedTrigger NodeId>, <program_id>]],   // _walk_delayed order
             "template": "<port template id>" | null}, ...],              // §12.11; source unchanged
  "continuous": [{"node": <NodeId>, "cont": <ContRef JSON>}, ...],     // every Continuous node
  "filter_sets": [{"filters": [<NodeId>], "content_id": "<16 hex>"}, ...],
  "rule_mods": [{"node": <NodeId>, "repr": "RuleMod(...)"}, ...],      // ai/actions.py:360 sort key
  "aliases": [[<def_id>, ["<name>", ...]], ...],
  "always_events": ["destroys_by_battle", "turn_end"],
  "hp_reduction_possible": false,
  "token_keys": [["<token key>", <def_id>], ...],
  "unit_variants": [[<def_id>, <variant def_id>], ...],
  "hooks": {"step": [13 names], "cond": [7], "filter": [9], "value": [5]},
  "repr_vectors": [{"value": <runtime value JSON>, "repr": "..."}, ...],  // §5.7
  "counts": {"defs": 1153, "abilities": 1260, "programs": 626, "instrs": 1830, ...}
}
```

Node encoding: `{"_": "<ClassName>", "<field>": <value>, ...}`, with **every** field present in
declaration order (no default elision). Field values are encoded as follows:

- `int` → JSON integer; `bool` → `true`/`false`; `str` → JSON string (or a `{"s": StrId}`
  reference for strings in the string table); `None` → `null`;
- an enum field → the member's value string;
- a tuple → a JSON array (pair tuples are 2-element arrays; `params` items are
  `["<key>", <value>]`);
- a nested node → `{"$": <NodeId>}`.

A `Value` field holds either a JSON integer (literal) or a node reference. So `ValueRef::Lit`
and `ValueRef::Node` stay distinct, which `eff.ap != 0` needs (`engine/view.py:360-361`; Inv 05
§7 hazard 11).

Hash-consing key: `repr(node)`. Two nodes share an index if and only if their CPython `repr` is
equal. The exporter asserts four properties and fails otherwise:

1. `==` classes and `repr` classes coincide on the node universe. It groups by `hash`, compares
   `repr` within each group and `==` across equal reprs. So Rust may use `NodeId` equality
   wherever Python uses structural equality: `same_values` on `RuleMod` (`engine/view.py:94-106`),
   the delayed-trigger match (`engine/interp.py:1536`), `cont_key` dedupe
   (`effects/registry.py:124-130`), and the `repr(r.rule)` sort key (`ai/actions.py:360`).
2. None of the three system step tuples equals a card program. They register through
   `R.program` (`engine/interp.py:1589-1594`), which would otherwise dedupe them into a card's id
   and label. Their labels are therefore `system:<name>`.
3. There are no unimplemented entries and no binding errors (W-69), every `DeployToken.token_key`
   resolves (W-164), and every hook name is in the 34-name table.
4. No IR string contains a backslash or a control character, and every non-ASCII character is
   `isprintable()`. CPython's escaping of other characters is not needed (Inv 04 §12 Q5).

`continuous` lists every `Continuous` node with its canonical `ContRef` (§5.6), computed with
Python's own `R.cont_key` equality. Value-shaped literals become value variants. The exporter
also asserts that two nodes get the same `ContRef` if and only if `cont_key` interns them to one
`effect_key`.

`delayed` lists, per card, every `DelayedTrigger` in `core._walk_delayed` order over
`getattr(ab, "steps", ())` of the card's `own` then `unit` abilities (`engine/core.py:411-438`;
not the cost-prefixed ability steps). Each entry is `[n, dt_node_id, program_id]`. `_h_delayed`
(`interp.py:1528-1540`) takes, among the entries of the card's **current** `def_id`, the first
`n` whose `dt_node_id` equals the executing instruction's `NodeId`, and fails with Python's
`"delayed trigger not registered on its card"` if there is none. `_delayed_triggers`
(`core.py:359-403`) resolves the `AbilityGrant`'s card number through `db.get`, that is the base
definition and not a Unit variant, then looks the entry up by `n`. Python registers the delayed
program lazily with the label `"<number>#delayed"` and dedupes it by steps (`core.py:392`,
`registry.py:115-122`). So the exporter asserts that no two cards have delayed-trigger step
tuples that are equal to each other but not to a build-time program; otherwise the label, and so
the content id, would depend on which trigger fires first in a process.

### 3.4 Versioning, regeneration and checks

- `ir_version` is an integer. `gcg-effects` declares `SUPPORTED_IR_VERSION = 1` and refuses any
  other value with `IrError::Version`, naming the regeneration command. Any format change bumps
  it.
- The Rust loader compares its compiled-in schema with the IR's `schema` and `enums`: class names,
  field names, order, annotation strings and enum members. It fails on the first difference, so a
  DSL change in Python fails loudly.
- An unknown hook or node class fails the load with the card number (W-69: no silent
  unimplemented card).
- **Regenerate:** `uv run python oracle/export_ir.py --write` writes `registry.json` and
  its `.sha256`.
- **Check:** `uv run python oracle/export_ir.py --check` re-exports under `PYTHONHASHSEED=1`
  and `=2` and compares bytes with the committed file. On drift it exits 1 and prints the first
  differing JSON path.
- When to regenerate: after any change under `src/gcg_sim/{effects,cards,data}` or
  `oracle/templates/`, a Python version bump, or a `/gcg-refresh-data` run. The
  `cards[].template` field is part of schema version 1 from the start (`null` until P6-K adds
  the templates), so adding templates needs no version bump. Phase 8 adds this step and the corpus check to the refresh
  skill and to CLAUDE.md.

Rust tests (`crates/gcg-effects/tests/`):

1. `ir_current.rs`: every `inputs` hash equals the SHA-256 of the file in the repo;
   `data_lock_sha256` matches `data/SOURCES.lock.json`; the embedded data bytes equal what
   `gcg-data` embeds. A data refresh or a binding edit without a re-export fails `cargo test`,
   even without Python.
2. `content_ids.rs`: `sha256(repr(node))[:16]`, computed by the Rust `repr`, equals
   `node_content_ids[i]` for every node. `content_id(Program)` equals `programs[i].content_id`
   for all 626 programs (`runner/digest.py:26-27`), and every filter set id matches.
3. `golden.rs`: the golden writer regenerates all 27 files in `tests/effects/golden/` byte for
   byte from the IR. It follows `effects/golden.py:19-51` with the schema defaults, and
   `json.dumps(indent=1, sort_keys=True, ensure_ascii=False) + "\n"` (`tools/golden.py:36`).
4. `registry_counts.rs`: 1153 card entries; 0 without a script; 302/538/313 by source; 1260
   abilities; 623 build-time + 3 system programs; 1830 instructions; 3 aliases;
   `always_events == {destroys_by_battle, turn_end}`; 480 non-empty `events`;
   `hp_reduction_possible == false` (Inv 04 §2); every `HappenedThisTurn.kind` in the IR is a
   history kind that some `core.record` call site writes, or is listed as never recorded
   (§4.2 `HistKind`).
5. `repr_vectors.rs`: synthetic instances of all 137 classes with non-default fields, including
   strings with `'`, `"`, `Ⅱ` and `①`, give identical `repr` text. They are exported by
   `oracle/export_ir.py` into the IR's `repr_vectors`. So do the runtime values: frozen `StatMod`, `KeywordGrant`
   and `AbilityGrant`, filter tuples, and the quote switch.
6. `db_fingerprints.rs`: `gcg-data`'s four CardDb fingerprints equal `db_fingerprints`, and the
   def_id order equals `cards[].card_number`.

Test overrides (`testkit.card_text`, `registry.override_script`, `effects/registry.py:209-220`;
10 tests, Inv 10 §9.2): the per-test trace carries an **IR fragment** that is self-contained
against the committed base IR. It holds:

- the replaced `CardEntry`, with Python's exact aids and program ids;
- every program, ability and node the entry references that is not in the base IR, with its
  label as Python's registry holds it (`R.program` dedupes by steps and keeps the first
  registrant's label, `registry.py:115-122`; with A16's fresh process per override test there is
  no earlier test that could have registered it, and the exporter asserts this);
- the registry-global values that `override_script` recomputes through `_index_events`
  (`registry.py:209-220`, `:356-378`): `always_events`, the `events` of every definition, and
  `hp_reduction_possible` (d9d41af made this flag true in one override test, W-51);
- `printed_keywords` as Python's cache holds them when the test runs: `_printed_kw`
  (`registry.py:89-113`) is never invalidated, so the plugin reads `reg._printed_kw` at override
  time and exports each def's entry, or "absent" where Python will compute it fresh.

`Registry::with_fragment` applies these values verbatim and recomputes none of them. The
exporter re-exports each override test once more in isolation and requires identical fragment
bytes. The four `force_link` tests
(`tests/cards/test_wp_gd05_a.py:101-113`) carry a `forced_links` list, which the testkit applies
through a test-only `Option<Arc<TestOverrides>>` in `GameConst` (`None` in production).

## 4. Engine state layout

### 4.1 Principles

1. **Function-for-function port.** Every Python function in `engine/*.py` gets a Rust function of
   the same name in the same module, with its statements in the same order. Each doc comment
   cites the Python `path:line`. This keeps every `derived()` call site, every `touch()`
   (Inv 06a §4), every `next_group()` (Inv 06a hazard 13) and every iteration order.
2. **uid = index in `cards`.** Cards are never removed (`engine/core.py:119-131`). Tokens move to
   OUTSIDE and keep their slot.
3. **Observable order is Python's.** Zone lists, `lasting`, `delayed`, `pending_triggers`,
   `batches`, `frames`, `history` and the per-uid view lists all keep Python's order.
4. **All records are `Copy`.** Variable-size parts live in arenas (`Vec<T: Copy>`) addressed by
   `Span { start: u32, len: u32 }`. So `clone_from` is a set of `Vec::clone_from` calls, each a
   `memcpy` into existing capacity, plus two `Arc` increments.
5. **Shared immutable parts are reference-counted, as Python shares them.** That covers
   `Arc<GameConst>` (decklists, registry, limits) and `Arc<Derived>` (`engine/state.py:534-572`;
   `:570` shares `_derived`).
6. **Integers are signed wherever Python can produce a negative.** Data-dependent arithmetic is
   checked: an overflow becomes `EngineError::Overflow`, never a wrap.
7. **State is private to `gcg-engine`** (W-49). Mutation goes through `pub(crate)` helpers that
   mirror the Python call sites. A rustdoc `compile_fail` doctest shows that an outside crate
   cannot write a field.

### 4.2 Types

```rust
// crates/gcg-engine/src/contracts.rs (lead-owned; values = Python's, engine/types.py:8-155)
pub type Uid = u16;                                  // card index; 123 at setup, tokens append
#[repr(u8)] pub enum Zone { Deck = 0, ResourceDeck, ResourceArea, Battle, Shield, Base,
                            Removal, Hand, Trash, Resolving, Outside, Paired }
#[repr(u8)] pub enum Phase { Setup = 0, Start, Draw, Resource, Main, End, GameOver }
#[repr(u8)] pub enum Step { SetupChooseFirst = 0, SetupDraw = 1, /* … Python values … */ GameOver = 99 }
#[repr(u8)] pub enum DecisionKind { ChooseFirst, Redraw, Main, Block, ActionStep, OrderTrigger,
                                    Burst, YesNo, Select, Excess, Discard, Arrange, Payment }
#[repr(u8)] pub enum ActionKind { GoFirst, Keep, Redraw, PlayUnit, PlayBase, Pair, PlayCommand,
                                  Activate, Attack, EndMain, Block, NoBlock, Pass, Select, Done,
                                  Yes, No, Order }
#[repr(u8)] pub enum EndReason { BattleDamage, DeckOut, Concede, BothDefeated, TurnLimit }
pub const NO_ARG: i32 = -1;
pub const PLAYER_TARGET: i32 = -2;
// Every enum has as_str() -> &'static str with Python's value, and an exhaustive match.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Action { pub kind: ActionKind, pub a: i32, pub b: i32, pub c: i32 }   // 16 B, state.py:14-32
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span { pub start: u32, pub len: u32 }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CardInstance {          // 24 B; JSON order state.py:118-130 (bools as 0/1)
    pub def_id: DefId,             // u16; mutable (observe.py:76-80, interp.py:930)
    pub owner: u8, pub zone: Zone, pub rested: bool,
    pub known: u8,                 // bitmask 1 << player
    pub pair: i32,                 // -1 = none
    pub damage: i32, pub zone_seq: i32, pub entered_turn: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Battle {                // state.py:144-191 (bools as 0/1)
    pub attacker: i32, pub attacker_seq: i32, pub target: i32, pub target_seq: i32,
    pub defender: u8, pub battle_id: i32, pub blocked: bool, pub damage_only: bool,
    pub attack_triggers: bool, pub ended: bool,
}

/// A continuous effect value. Replaces Registry.continuous[effect_key]
/// (effects/registry.py:124-130), whose first-use numbering is process-local.
/// Equality of ContRef values equals Python's cont_key interning (§5.6). The derived Ord is
/// arbitrary but total (over NodeId, StrId and Kw declaration order); it exists only so that
/// _references and _fingerprint tuples can be sorted (ai/actions.py:308-329, :360), which Python
/// compares only for equality in the merge set (:386-392). Python's integer order is never
/// reproduced: equal multisets give equal sorted sequences under any total order consistent with
/// equality, and a unit test sorts two element orders to equal fingerprints.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ContRef {
    StatMod { ap: i32, hp: i32 },               // any StatMod with int fields (frozen, interp.py:1276-1283, or literal)
    KeywordGrant { kw: Kw, amount: i32 },       // any KeywordGrant with an int amount
    AbilityGrant { card: StrId, index: i32 },   // delayed-trigger key (interp.py:1549) or literal
    Node(NodeId),                               // every other Continuous node, hash-consed in the IR
}

#[derive(Clone, Copy)]
pub struct Lasting {               // state.py:194-262; JSON position 0 = effect, 9 = filters
    pub effect: ContRef, pub controller: u8, pub source_uid: i32,
    pub targets: Span,             // into GameState::lasting_targets: (Uid, zone_seq)
    pub duration: Duration, pub created_turn: i32, pub battle_id: i32, pub player: i8,
    pub uses: i32, pub filters: Option<FilterSetId>, pub aux: i32, pub serial: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EventPayload { len: u8, items: [(EvKey, i32); 10] }
// ("subject", s), ("player", p), ("group", g), *sorted(extra) (core.py:328-333). At most 8 pairs
// occur at any emit site (Inv 06a §5.4); a 9th is EngineError::Capacity, never a silent drop.

/// One element of a once-per-turn key tuple. Python keys are tuples of ints; the only
/// element Rust cannot represent as an int is the process-local effect_key.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum OnceElem { Int(i32), Cont(ContRef) }   // Ord is arbitrary but total (see below)
/// st.once_used entries with Python's element positions:
///   (aid, host, zone_seq)                            core.py:236-237, game.py:439,892
///   (tag, host, zone_seq)                            wp_gd05_a.py:106-107
///   (-8|-7, aid, host, uid, zone_seq)                core.py:524,561 with a Constant's r.key = (aid, host), view.py:436
///   (-8|-7, -1, Cont(effect), serial, uid, zone_seq) core.py:524,561 with a lasting r.key, view.py:451
///   (-9, uid, turn)                                  interp.py:648,674
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct OnceKey { len: u8, elems: [OnceElem; 6] }
impl OnceKey {
    /// Python's `uid in key` (ai/actions.py:309): any Int element equal to uid. A Cont element
    /// never equals a uid; §8.9 measures whether Python's effect_key ever did.
    pub fn contains_int(&self, x: i32) -> bool;
    /// `tuple(_SELF if x == uid else x for x in key)` (ai/actions.py:309), Cont kept.
    /// _SELF = -(1 << 40) (ai/actions.py:281) does not fit an i32, so the result has its own type.
    pub fn with_self(&self, uid: i32) -> FpOnceKey;
}
/// Fingerprint form of a once key: equality and a total order only, never serialised.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum FpOnceElem { Int(i32), Cont(ContRef), SelfRef }
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct FpOnceKey { len: u8, elems: [FpOnceElem; 6] }
// st.once_used is a Python set (state.py:497): GameState::once_add(k) pushes k only when no
// equal key is present. It is the only writer, used at every Python .add site (core.py:236-237,
// game.py:439,892, interp.py:137,674, wp_gd05_a.py:116-118). A blind push would store duplicates
// that change canonical_state and state_digest.

#[derive(Clone, Copy)]
pub struct TriggerInst {           // state.py:265-322 (bools as 0/1); JSON position 0 = program
    pub program: ProgramId, pub controller: u8, pub host: i32, pub host_seq: i32,
    pub card_uid: i32, pub ability_key: (i32, i32, u8),   // (def_id, aid, unit) core.py:280,400,721
    pub event: EventPayload, pub burst: bool, pub once_key: Option<OnceKey>, pub optional: bool,
}

#[derive(Clone, Copy)]
pub struct Frame {                 // state.py:325-344; JSON bools true/false; vars/ints sorted by key
    pub program: ProgramId, pub controller: u8, pub host: i32, pub host_seq: i32,
    pub card_uid: i32, pub kind: FrameKind, pub event: EventPayload, pub pc: u16,
    pub vars: Span,                // into frame_vars: VarEntry { var: VarId, items: Span into frame_items }
    pub did: bool, pub answer: bool, pub waiting: bool,
    pub buf: Span,                 // into frame_items: Choose/Discard picks, ToDeck/Arrange orders
    pub ints: Span,                // into frame_ints: IntEntry { key: IntKey, value: i32 }
    pub once_key: Option<OnceKey>, pub acted: bool,
}
#[derive(Clone, Copy)] pub struct VarEntry { pub var: VarId, pub items: Span }
#[derive(Clone, Copy)] pub enum IntKey { HoldRules, Loop(u8), RestI, RestAs(u16), ExUsed }

#[derive(Clone, Copy)]
pub struct HistoryEvent { pub kind: HistKind, pub player: i8, pub by: i8, pub uid: i32,
                          pub def_id: i32, pub source: i32 }       // state.py:414-430
// HistKind lists every kind a core.record call site writes (generated by scanning the call
// sites; a test fails on an unlisted literal). An IR HappenedThisTurn.kind decodes to
// HistKindRef { Known(HistKind), Other(StrId) }; Other never equals a recorded kind, as Python's
// string compare would find no match (view.py:914-930).

#[derive(Clone, Copy)]
pub struct DecisionHdr {           // state.py:35-68; options live in GameState::pending_options
    pub player: u8, pub kind: DecisionKind, pub prompt: PromptId,
    pub ctx_len: u8, pub context: [(CtxKey, i32); 4],       // ctx() = first match (state.py:45-49)
}

pub struct GameConst {             // shared by every clone of one game (Arc)
    pub reg: Arc<Registry>, pub db: &'static CardDb,
    pub decklists: [Box<[DefId]>; 2], pub resource_decklists: [Box<[DefId]>; 2],  // sorted, game.py:151
    pub turn_limit: i32, pub max_actions: i64,
    /// Exactly core.py:295-313: the registry's always_events, plus events[def] and
    /// events[unit_variant(def)] for every def in set(decklists + resource_decklists). The IR
    /// exports always_events verbatim (pinned by registry_counts.rs). Never a derived set:
    /// TURN_END and DESTROYS_BY_BATTLE must stay listened because the Repair and Breach keyword
    /// triggers (core.py:441-482) sit behind emit's early return (core.py:334-335). Testkit
    /// helpers that change the decklists recompute it with the same formula.
    pub listened: EvSet,
    /// The registry's global flag verbatim (core.py:866), from the IR or the test fragment.
    pub hp_reduction_possible: bool,
    pub test_overrides: Option<Arc<TestOverrides>>,          // force_link; None in production
}

pub struct GameState {
    // serialised (canon.rs writes them in to_json order, state.py:575-611)
    seed: u64, rng: SplitMix64, cards: Vec<CardInstance>, zones: ZoneLists,
    turn: i32, active: u8, first_player: u8, setup_chooser: u8, phase: Phase, step: Step,
    pending: Option<DecisionHdr>, pending_options: Vec<Action>,
    battles: Vec<Battle>, next_battle_id: i32, next_lasting_serial: i32, priority: u8, passes: i32,
    lasting: Vec<Lasting>, delayed: Vec<Lasting>, lasting_targets: Vec<(Uid, i32)>,
    triggers: Vec<TriggerInst>,    // [batch 0 | … | batch k | pending_triggers]
    batch_starts: Vec<u32>,        // start of each batch; pending_triggers = triggers[pending_start..]
    pending_start: u32,
    frames: Vec<Frame>, frame_vars: Vec<VarEntry>, frame_ints: Vec<IntEntry>, frame_items: Vec<i32>,
    once_used: Vec<OnceKey>, history: Vec<HistoryEvent>,
    winner: Option<i8>, end_reason: Option<EndReason>, konst: Arc<GameConst>,
    redraws: [bool; 2], action_count: i64, event_group: i64, effect_hits: Vec<(Uid, Uid)>,
    version: u64,
    // not serialised
    cache: RefCell<Option<Arc<Derived>>>,   // Python _derived (§4.6)
    rules_checked: i64,                     // Python _rules_checked; -1 initially (core.py:861)
    history_epoch: u32,                     // bumps when history is replaced (game.py:190, :336)
}

/// zones[p][z] as one arena: list i = 12 * p + z is items[offs[i]..offs[i + 1]].
#[derive(Clone)]
pub struct ZoneLists { items: Vec<Uid>, offs: [u16; 25] }
```

### 4.3 How Python's containers map

| Python | Rust | Order and semantics kept |
| --- | --- | --- |
| `zones[p][z]` lists (`state.py:479`) | `ZoneLists` segments | Insert at the segment start for DECK/SHIELD/RESOURCE_DECK unless `bottom`, else append (`core.py:102-106`). `list.remove` removes the first occurrence (`core.py:38-44`). OUTSIDE stays empty (`core.py:97-100`). Each edit is one `memmove` of at most a few hundred bytes plus offset updates. Callers that keep a slice while moving cards (`game.py:196`, `interp.py:1106,1121,1133,1144`) copy it into a scratch buffer first, as Python's slicing does (Inv 06a hazard 26) |
| `Lasting.targets` tuple | `Span` into `lasting_targets` | Order-preserving filters (`game.py:208-220`, `battle.py:241-258`, `game.py:840-871`) rebuild the list and compact the arena in the same pass |
| `pending_triggers`, `batches: list[list]` | one `triggers` Vec + `batch_starts` + `pending_start` | `_form_batch` (`game.py:667-670`) turns the pending tail into a new top batch. `batches[-1].pop(i)` is `triggers.remove(top_start + i)` (`game.py:813-815`). An empty top batch pops its start (`game.py:684-709`). Empty batches are representable (`[[]]`, Inv 05 §3.1) |
| `frames` with `vars`/`ints`/`buf` | `frames` + arenas `frame_vars`, `frame_ints`, `frame_items` | Regions of the top frame sit at the arena tails and grow in place. A write that grows a non-tail region relocates that region to the tail. Popping truncates the arenas to the remaining frames' maximum end, and compacts when garbage exceeds the live size. `Ctx` sharing `f.vars` (`interp.py:57-58`) becomes a frame index in `Ctx` |
| `Frame.vars: dict[str, tuple]` | `VarEntry` list keyed by `VarId` (the IR `var_names` table) | Used only for get, set, delete and membership, plus one order-free `any()` over the values (`ai/actions.py:292`). Serialised sorted by name (`state.py:372-392`) |
| `Frame.ints: dict[str, int]` | `IntEntry` list keyed by `IntKey` | `__hold_rules` is deleted at 0 (`interp.py:206-210`). The one iteration is an order-free set comprehension (`interp.py:686`). Serialised sorted by the rendered key string, so `__rest_as10` < `__rest_as2` as in Python |
| event tuples | `EventPayload` inline | `ctx.ev(key)` returns the first match (`view.py:45-49`). `EvKey` is a closed enum ordered by its string, so sorting extras by enum order equals sorting by string. `EvKey::Other(StrId)` covers an IR key that no emit produces (it never matches) |
| `Decision.context` | `[(CtxKey, i32); 4]` + length | first match (`state.py:45-49`) |
| prompts | `PromptId` into a static table: engine literals, every `AskMay` prompt or `"you may"` (`interp.py:213-216`), `"choose " + var` for every Choose var (`interp.py:439,467`) | serialised as the string |
| `once_used: set[tuple]` | `Vec<OnceKey>` used as an insertion-ordered set | Membership only in logic (Inv 06a hazard 11). Every insert goes through `once_add`, which skips a key that is already present, as `set.add` does; re-adds happen (`wp_gd05_a.py:116-118` adds without checking, and two abilities sharing a tag can both pass the `once_free` cond at queue time; `interp.py:137`, `:674`; `game.py:892` under `apply(check=False)` in AI worlds). Reset to empty at turn change (`game.py:335`). The canonical form sorts by JSON text (§4.7). A property test adds the same key twice and requires an unchanged `canonical_state`; `gcg-diff selftest` has a duplicated-once-key control |
| `effect_hits: dict` | `Vec<(Uid, Uid)>` | Overwriting keeps the first-insertion position; lookups, `clear` (`core.py:874,885`). Serialised sorted by uid (`state.py:607`) |
| `_destroy_battle`'s `victims` dict literal (`battle.py:193`) | inline insertion-ordered map of at most 2 entries, overwrite in place | `{to_target[0]: attacker, to_attacker[0]: target}`: if a redirect (`core.py:588-591`) sends both hits to one uid, the key keeps its first position and the last value wins. Iteration and lookups follow the dict (`battle.py:99-111`). A redirect-collision unit vector exported from Python checks it |
| `history` (shallow copy on clone) | `Vec<HistoryEvent>` (value copy) | Entries are never mutated in place: `_own_history` replaces them (`observe.py:86-95`). A reset bumps `history_epoch` (`game.py:190,336`), which replaces the list identity that `runner/instrument.py:81` uses |
| `pending` shared by identity | `DecisionHdr` + `pending_options` copied | Agents receive `&GameState` and cannot mutate it. The runner's identity check (`runner/game.py:89-92`) becomes a check that `version` and the pending decision are unchanged after `choose` |
| `decklists` tuples | `Arc<GameConst>` | shared, as Python shares them (`state.py:562`) |
| `Registry.continuous[k]` | `ContRef` value in `Lasting`, `RuleInst.key` and `OnceKey` | Value equality = Python's equality of interned keys (injective numbering, Inv 04 §11 hazard 2) |
| `Registry.filter_sets[k]` | `FilterSetId` into the IR `filter_sets` | The canonical form uses its content id |

Capacities: fixed inline arrays (`EventPayload`, `DecisionHdr.context`) and initial arena
capacities are set from the Phase 2 container census (§8.6), at p99.9 or above. Arenas grow
on demand, so a large value is slower, never wrong.

### 4.4 `touch` and `version`

`fn touch(&mut self) { self.version += 1; *self.cache.get_mut() = None; }` is `pub(crate)` and
mirrors `state.py:529-531`. Every mutation helper mirrors the Python call-site list in Inv 05 §3.2,
Inv 06a §4 and Inv 06b §15 **one for one**. That includes:

- the unconditional touches (`game.py:220`, `:237`);
- ACTIVE_STEP, which unrests without touching and touches once at the end (`game.py:227-237`);
- the double touch in `declare_attack` (`battle.py:16,28`) and in `damage_only_battle`
  (`battle.py:277,282`);
- `pair` (`interp.py:1368-1393`), and `pay_generic` (once per EX Resource plus once at the end);
- `move_card`, once per call even when a Pilot follows (`core.py:81`);
- `_h_custom`, after every custom step (`interp.py:1571-1575`).

`core.record` does **not** touch (`core.py:142-151`). A comment at each touch names its Python
line.

`crates/gcg-engine/tests/touch_catalogue.rs` checks the setup accounting of Inv 06b §6. The
version is 120 after `new_game`, +10 for SETUP_DRAW, +11 per redraw, +16 for SETUP_SHIELDS and +3
for turn 1 (active step, draw, resource). So the first MAIN decision has
**version = 149 + 11 × (number of players who redrew)**, which is 160 when exactly one player
redrew. The test covers all three redraw cases. It also checks the "always option 0" game of
Inv 06b §18 (seed 12345: final digest `5ce28ed9…31e9a`, version 357), and a per-primitive
version delta against Python vectors.

### 4.5 Clone and recycling

`GameState` implements `Clone` with value semantics (W-52): `pending` and history are copied,
and the cache `Arc` and `konst` are shared as Python shares them. `clone_from_state(&mut self,
src)` reuses the destination's buffers. Each `Vec<Copy>` is truncated and `extend_from_slice`d
into its existing capacity, `rules_checked` and `version` are copied, and the cache `Arc` is
cloned out of `src.cache`. The search never calls `clone()` on the hot path. It calls
`pool.checkout().clone_from_state(&world)`, where `StatePool` is a per-thread
`Vec<GameState>` of recycled states. Expected cost: about 3-6 KB of `memcpy` for 123 cards, the
zone arena and the small vectors, plus two `Arc` increments. `step(st, a)` = clone + apply
(`game.py:961-965`).

### 4.6 Derived view and its cache, exactly

```rust
pub struct Derived {                       // view.py:66-106, dense by uid (len = cards.len())
    key: ViewKey,                          // (active, turn, n_battles, Option<(battle_id, target, ended)>, n_history)
    present: BitSet,                       // ap/hp/kw/traits keys exist = card in BATTLE/BASE in the base view
    ap: Vec<i32>, hp: Vec<i32>,            // unclamped until _finalize (view.py:477-483)
    kw: Vec<KwAmounts>,                    // [Option<i16>; 7] per uid: key present with amount
    traits: Vec<TraitsRef>,                // Printed(DefId) | Pool(Span)
    rules: Vec<Span>, rule_pool: Vec<RuleInst>,           // per uid, any zone, application order
    cost_mod: Vec<Option<i32>>, level_mod: Vec<Option<i32>>,  // None = key absent
    linked: BitSet,
    abilities: Vec<Span>, ability_pool: Vec<AbilityId>,   // per host, zone-scan order, duplicates kept
    hosts: Vec<Uid>,                       // insertion order of view.abilities (view.py:245-311)
    by_event: OnceLock<TriggerIndex>,      // filled lazily by trigger_index (view.py:314-333)
}
pub struct RuleInst { pub rule: NodeId, pub controller: u8, pub source: i32, pub key: RuleKey, pub aux: i32 }
pub enum RuleKey { Constant { aid: AbilityId, host: Uid }, Lasting { effect: ContRef, serial: i32 } }
// crates/gcg-engine/src/view/mod.rs (B2)
pub fn derived(st: &GameState) -> Arc<Derived>;
```

Python's per-uid view maps are dicts read with `.get` and the printed value as the fallback
(`view.py:490-504`, `:560-566`). Every `Derived` accessor therefore treats `uid >= len` as an
absent key and returns that fallback; it never indexes out of bounds (`panic = "abort"`).
`Derived::empty()` is Python's shared `_UNREAD = Derived()` (`view.py:845-850`), which
`resolve_ref` passes when the reference selects no cards. Unit tests cover a uid created after
the view was computed and both `resolve_ref` branches.

`derived` reproduces `engine/view.py:232-239` exactly, in four steps:

1. Clone the cached `Arc` out of the `RefCell` and release the borrow.
2. Compute `view_key(st)` **before** any compute (`view.py:223-229`).
3. If the cache is empty or its key differs, run `_compute` with no borrow held. Re-entrant calls
   therefore behave as in Python: an inner `derived` stores its own result, and the outer call
   then overwrites it.
4. Store the result with its key and return it.

It takes `&GameState`, because the AI calls `derived` on the true state
(`ai/actions.py:109,145,210,385`). Those calls fill the true state's cache exactly as Python
fills `st._derived`. `touch()` clears the cache through `get_mut` (no runtime borrow). Nothing
else clears it.

Held views: Python sites that keep one view across mutations hold the `Arc` and keep reading
it, exactly as Python holds the object. The sites are listed in Inv 06a §14 (`_h_set_active`,
`_enter_rested`, `destroy`, `shields_receiving_damage`, `_h_return_to_hand`, `emit`, `pair`,
`_h_apply`) and Inv 06b §19 item 16 (ACTIVE_STEP `game.py:227`, `main_options`,
`_play_or_activate`, `damage_step`, `_exchange`, `_destroy_battle`). Queries take an explicit
`&Derived` next to `&GameState` (§10.3.3). `play_cost` calls `player_cost_mod`, which calls
`derived(st)` itself (`view.py:525-545`); Rust does the same.

`by_event` is a `OnceLock` inside the shared `Arc`. It is filled from the zones of whichever
state asks first, as Python fills the shared object (`view.py:314-333`; Inv 11 hazard H2). That
is exact, because every zone change touches.

Threads: `GameState` is `Send` but not `Sync` (because of the `RefCell`). `Derived` is
`Send + Sync`. A fork-join subtask therefore receives its own clone of the state (§7.1). A clone
copies the cache `Arc` as Python's clone shares `_derived`, so a subtask sees exactly the cache
state that Python's next sequential call would see.

Fixpoint (`view.py:409-463`, ported literally):

- base view (`view.py:245-311`), with `prev = None` and the partially built view used as the gate
  view (Inv 05 §5.5);
- early exit when nothing is lasting and no constant exists (`view.py:414-419`);
- Jacobi iteration: reads come from `prev` (or `base` in iteration 0), writes go to `cur`, with
  `_copy_base` semantics (`view.py:336-344`);
- at most 6 iterations, and never converged at iteration 0;
- convergence is `same_values` (`view.py:94-106`: ap, hp, kw, traits, cost_mod and level_mod as
  maps with presence; per-uid rule lists compared by `RuleMod` `NodeId`; per-uid aid lists) **and**
  `new_granted == granted`;
- at the cap the last `cur` wins, and `_finalize` clamps once at the end.

W-48 adds a non-convergence flag that `paranoid` and the fuzzer read; the output is unchanged.

Iteration orders:

- the base view goes p0 then p1, zones BATTLE then BASE for stats, and BATTLE, BASE, PAIRED, HAND,
  TRASH for abilities;
- constants iterate `hosts` in insertion order (`view.py:425`);
- lasting effects go in `st.lasting` order, with live `(uid, seq)` targets and the rule key
  `(-1, effect, serial)` (`view.py:437-452`);
- the trigger index goes active player first (`view.py:320-331`).

Cache transparency is **not assumed**, even though the policy above is exact. Two checks run on
the whole corpus before Phase 5 may change the invalidation scheme (W-124):

- the Python `--nocache` regeneration (§8.5);
- the Rust `paranoid` feature. On every cache hit it recomputes and compares (port of
  `tests/paranoid.py:59-90`). On every rules-management skip (`core.py:861`) it re-runs the scan
  on a clone and requires no change. It also checks that unlistened events have no candidates.

### 4.7 Canonical JSON and `state_digest`

- `canon::canonical_state(st) -> Json` builds `gcg_core::pyjson::Json` objects with the 34 keys
  of `state.py:575-611` in order. Records use the encodings of Inv 05 §2:
  - 0/1 for the bools of CardInstance, Battle and TriggerInst; `true`/`false` for Frame bools
    and `redraws`;
  - `zones` as 2×12 lists including OUTSIDE; `pending` as Decision JSON or `null`;
  - `effect_hits` as `[[uid, src], …]` sorted by uid; Frame `vars` and `ints` sorted by key
    string; `Lasting.duration` as its value string.

  It writes content ids directly where `runner/digest.py:36-49` substitutes them:
  - `lasting[i][0]` and `delayed[i][0]` get `content_id(ContRef)`;
  - `lasting[i][9]` gets the content id of the filter tuple, or the integer `-1`;
  - `frames[i].program_id` and position 0 of every trigger (pending and every batch) get
    `programs[id].content_id`;
  - once keys become lists. A `Cont` element is written as its content id, which happens only at
    position 2 of `(−8|−7, −1, …)` keys, matching `digest.py:30-33`. The list is sorted by each
    key's `json.dumps` text with default separators `", "`, `": "` and `ensure_ascii=True`
    (`digest.py:48`), as a plain code-point string sort.
- `canon::state_digest(st)` is
  `sha256(dumps(canonical, sort_keys=True, separators=(",", ":"), ensure_ascii=True))` as
  lowercase hex (`digest.py:52-56`). `ensure_ascii` escapes `①` as `①` (Inv 05 §2.2).
  Phase 5 may add a streaming writer that skips the `Json` tree; a test asserts that both paths
  give identical bytes on every corpus snapshot.
- Rust has no raw `to_json` with process-local ids. Only the canonical form is ever written or
  compared.
- `ContRef::content_id`:
  - `StatMod` → `repr` text `StatMod(ap=<ap>, hp=<hp>)`;
  - `KeywordGrant` → `KeywordGrant(keyword=<Kw.REPAIR: 'Repair'>, amount=<n>)`;
  - `AbilityGrant` → `AbilityGrant(card_number='<n>', ability_index=<i>)`;
  - `Node(id)` → `node_content_ids[id]`.

  Each goes through `gcg_core::pyrepr` and SHA-256, keeping the first 16 hex characters. The IR's
  `repr_vectors` check all four forms.

### 4.8 `from_canonical`

`canon::from_canonical(reg, snapshot) -> GameState` rebuilds a state from a trace snapshot. A
snapshot carries the canonical JSON plus the side tables that content ids cannot be inverted
from:

- `continuous: {content_id: IR node JSON or value}` and `filter_sets`, mapped back with
  `Registry::cont_by_content_id` and `filter_set_by_content_id`;
- program content ids, mapped back with `Registry::program_by_content_id`. The exporter asserts
  these ids are unique among programs;
- `rules_checked` (Python's `from_json` drops it, `state.py:614-651`, but it decides
  `effect_hits.clear()`, `core.py:857-892`);
- the decklists.

Gates: `canonical_state(from_canonical(s)) == s` byte for byte on every corpus snapshot, and
`state_digest` of every loaded snapshot equals the recorded digest.

## 5. Effects

### 5.1 DSL nodes in Rust

- One macro, `dsl_nodes! { … }` in `crates/gcg-effects/src/dsl/nodes.rs`, declares every class
  once: its Python name, its fields in declaration order, the Rust field types and the defaults.
  It generates the `Node` enum variant, the decoder from `serde_json::Value`, the CPython `repr`,
  the golden-JSON writer and the schema row that is compared with the IR's `schema` (§3.4). No
  proc-macro crate is needed.
- Typed handles: `NodeId(u32)` indexes `Registry::nodes`, and `ValueRef = Lit(i64) | Node(NodeId)`.
  Sub-kinds are checked at load: `CondId`, `FilterId`, `RefId`, `SelId`, `StepId` and
  `ContinuousId` are `NodeId` newtypes validated against the variant, so evaluators `match`
  without fallible casts.
- Strings (card numbers, names, traits, labels, prompts, var names, token keys) become `StrId`
  into the registry's string table. Evaluation compares ids; `repr` and JSON print the string.
- The enums `P`, `Side`, `Loc`, `Op`, `Kw`, `Stat`, `CardKind`, `RuleKind` (29 members),
  `DamageKind`, `Ev` (33), `Timing`, `Where`, `Gate` and `Duration` keep declaration order
  (`LOC_CODES`, `view.py:936`) and value strings. Sorting by value is always explicit, never a
  derived `Ord` (`Kw` sorts by value, `wp_gd04_a.py:78`; Inv 09 §3.3).
  `Kw::sorted_by_value()` gives Blocker, Breach, First Strike, High-Maneuver, Repair, Support,
  Suppression.

### 5.2 Programs, abilities, bindings

- `Program { label: StrId, content_id: ContentId, instrs: Box<[NodeId]> }`, exactly as exported.
  The layout is Python's `flatten` output (`effects/program.py:96-181`). Three things read that
  exact layout: `choice_group` adjacency (`interp.py:391-404`), `targets_available`
  (`interp.py:235-260`), and hook #33 (`wp_st12_14.py:91-93`, where `instrs[pc-1]` is a
  `Damage`).
- `AbilityEntry` and `CardEntry` come from the IR with Python's aids, program ids and entry lists.
  That includes `command_aid`/`burst_aid` (last occurrence, `registry.py:257-261`) and
  `printed_keywords` (`registry.py:89-113`, exported per card, so no cache is needed).
- Bindings do not exist in Rust; their output is in the IR. `source` and `binding_module` are kept
  for the `data status` coverage, the golden files and `gcg-diff explain`.
- System programs get the fixed ids 623, 624, 625 with labels `system:deploy`, `system:pair` and
  `system:hand_limit`. Python numbers them lazily, in first-use order (`interp.py:1589-1594`).
  That changes only raw ids, which never reach an output (the digest uses content ids) and only
  key pure memo caches in the AI (`ai/actions.py:180`, `ai/targeting.py:65,80`).
- `Registry::generation()` changes on every `with_fragment`. AI memo tables are keyed by
  registry generation and live in a per-registry `AiTables`, never in globals (W-33).

### 5.3 Hooks

```rust
pub enum StepHook { ReturnLookedBottom, DeployExBase, SupportUsed, LookOwnTop /* #6 and #7 share one body */,
    GD02bPayDeployCost, GD03aPayCostOf, GD03aApMinusCount, GD04aCopyKeywords,
    GD04aApDownByNamedTrashUnits, GD05aUseOnce, GD05bDestroyFirstShieldAreaCards,
    GD05bResolvingCommandToTrash }                       // 13 names, 12 bodies
pub enum CondHook { AttackTargetDamaged, GD01bDestroyedWithDamage, GD04bPlayedWithEx,
    GD05aOnceFree, GD05aDestroyedByEffect, GD05bPlayedWithEx, ST12DestroyedByEffectDamage } // 7
pub enum FilterHook { IsAttackTarget, GD03aHasDestroyedEffect, GD03aLowestHp, GD03bCondHolds,
    GD03bBattlingEnemyUnit, GD04aPilotCapable, GD04aHasPrintedKeyword, GD04aHasPartnerInTrash,
    GD05bCanPair }                                        // 9
pub enum ValueHook { GD03aApPer, GD03bUniqueNames, GD05aLowestEnemyUnitLevel,
    ST05HighestEnemyUnitLv, ST12LowestRestedEnemyLv }     // 5
```

- The IR's `CustomStep(name, params)` decodes to `{ hook, params: TypedParams, raw: NodeId }`.
  `raw` keeps the original `params` for `repr`. `TypedParams` is a per-hook struct (for example
  `LookOwnTop { player: P, var: VarId }`), validated at load.
- Placement follows the import DAG (§10.1):
  - the 21 pure hooks (cond, filter, value) live in `view/hooks.rs` with the signature
    `fn(st: &GameState, dv: &Derived, ctx: &Ctx, p: &TypedParams) -> Result<_, EngineError>`;
  - the 13 step hooks live in `interp/hooks/<python module>.rs` with
    `fn(st: &mut GameState, fi: usize, p: &TypedParams) -> Result<bool, EngineError>`.

  Each body cites its Python lines (table in Inv 04 §6.2).
- Semantics kept:
  - `_h_custom` sets `f.did` to the hook result and **always** touches afterwards
    (`interp.py:1571-1575`);
  - `params` are read per call with last-key-wins (`dict(params)`, `interp.py:1573`);
  - hooks #13 and #14 re-enter the interpreter through
    `interp::execute_synth(st, fi, SynthInstr::PayCost{..} | SynthInstr::Apply{..})`, which runs
    the same handler code (events, groups, serials) as an IR instruction;
  - #20 iterates `printed_keywords` in `Kw` value order; #28 slices both lists before destroying;
    #1 uses first-occurrence removal and `n - 1` `randrange` draws.
- W-64 consolidation is internal only: #6 and #7 share one body. #23 and #31 stay distinct,
  because they scan history differently (`wp_gd04_b.py:52-62` vs `wp_gd05_b.py:115-124`).

### 5.4 Interpreter with resumable frames

- `enum Status { Next, Jumped, Wait, Pushed }` (`interp.py:28-32`).
- `run_top_frame(st)` is a literal port of `interp.py:109-132`:
  - execute by instruction variant;
  - set `acted` per `is_non_action(instr)` (`interp.py:41-54`);
  - `pc += 1` on `Next`, then the `over(st)` checks;
  - run rules management unless the frame holds `__hold_rules`, **also after `Wait`**;
  - return on `Wait`/`Pushed`;
  - at the end, pop and `finish_frame` (`interp.py:135-150`).
- `resume(st, action)` is `interp.py:160-175`:
  - clear `waiting`;
  - dispatch to the resumer of the instruction at `pc` (AskMay, ModeSelect, Choose, Discard,
    DeployCard/DeployToken excess, Arrange, Rest, ToDeck);
  - the same `acted` rule, and rules management unless held, with no `over` check.

  "No resumer for this instruction" stays an `EngineError` with Python's message
  (`interp.py:156,166`).
- A frame is addressed by its index `fi` in `st.frames` and is never borrowed across calls.
  Handlers re-read `st.frames[fi]` after any call that can push frames, which is how Python's `f`
  stays valid after a sub-frame push.
- `Ctx { controller: u8, host: i32, card_uid: i32, vars: VarsSrc, event: EventPayload }`, with
  `card_uid = host` when `-1` is passed (`view.py:42`). `VarsSrc::Frame(fi)` reads the live frame
  vars, so hook writes are visible to later steps (`interp.py:57-58`, `view.py:43`).
- RNG use is exactly the four engine sites, in order (Inv 06a §15): `shuffle_deck`, random
  `Choose`, random `Discard` and `return_looked_bottom`. The RNG is consumed **before** the
  take-all shortcut (Inv 06a hazard 2).
- Decisions raised by effects use `decide(st, fi, player, kind, options, prompt, context)` with
  the option orders of Inv 06a §12.7.

### 5.5 Trigger, batch and frame semantics

Literal ports, each citing its Python lines:

- `emit` (`core.py:316-356`):
  - drop everything once `winner` is set;
  - take `group` from the extras or from the current `event_group`;
  - build the payload `subject, player, group, *sorted(extra)`;
  - take the `listened`/`delayed`/`lki` early return, using the per-game `listened` set, which
    follows Python's formula exactly (§4.2 `GameConst`; W-50 adds only checks);
  - use one view for every candidate check;
  - collect candidates in trigger-index order (active player first; BATTLE, BASE, PAIRED, HAND,
    TRASH), then LKI hosts sorted by `(owner != active, uid)` (`core.py:341`), then delayed
    triggers, then keyword triggers (Inv 06a §6.7).
- `queue_trigger` (`core.py:248-284`):
  - the simultaneous-event dedupe on `(aid, host, group)` (`core.py:262-269`);
  - the once check with `(aid, host, host_seq)` (`core.py:270-272`);
  - `_trigger_ok` in check order (`core.py:194-233`: `whose_turn` YOU means the host owner's turn,
    every other value means the opponent's turn);
  - delayed triggers (`core.py:359-403`), keyword triggers (`core.py:441-482`), and Burst triggers
    inside `destroy_shields` (`core.py:700-749`).
- `advance` (`game.py:734-760`), in this order:
  1. the action cap (strict `>`);
  2. frames;
  3. pending triggers, which form one batch pushed on the stack;
  4. batches, through `_start_next_trigger` (`game.py:684-709`): an empty top batch is popped;
     candidates are bursts first, else the active controller's, else all; if there is more than
     one candidate and **every** candidate equals the first on
     `(program_id, ability_key, burst, host)`, the list becomes the first candidate alone (no
     pairwise dedupe); a single candidate resolves directly, otherwise an ORDER_TRIGGER decision
     is raised with indices ascending;
  5. setup, or rules management and the next step.
- `apply` dispatches on `frames.last().waiting` **before** the decision kind (`game.py:790`). It
  raises `IllegalActionError` with `Action.__str__` text, which drops `-1` arguments by value
  (`state.py:30-32`, `game.py:785-786`).
- `rules_management` (`core.py:857-892`):
  - the `_rules_checked` short-circuit (`core.py:861`) is set only on the two Python paths
    (`:871`, `:875`);
  - `effect_hits` is cleared exactly where Python clears it.

### 5.6 Continuous-effect identity (`ContRef`)

Canonicalisation reproduces Python's structural interning (`effects/registry.py:124-130`). Two
effects that Python interns to one `effect_key` get the same `ContRef`, whatever their origin:

- an IR literal;
- `_frozen` (`interp.py:1276-1283`, non-int amounts are evaluated and frozen);
- a hook (`wp_gd03_a.py:111`, `wp_gd04_a.py:69-106`);
- `_h_delayed` (`interp.py:1525-1557`).

Value-shaped nodes are therefore always value variants. Every other continuous node is
`ContRef::Node` of the hash-consed IR node, deduplicated by Python `==` (asserted by the exporter,
§3.3). The AI compares `effect_key` and `filters_key` only for equality: `ai/noop.py:48-50`, the
`lasting` part of `_references` (`ai/actions.py:312-329`), and the merge set (`ai/actions.py:379-393`).
So `ContRef` equality reproduces them. The one non-equality use is `uid in key` over once keys
(§4.2 `OnceKey`, §8.9).

### 5.7 CPython `repr` for content ids

`gcg-effects/src/repr.rs` implements `repr` for every node from the macro table:

- dataclasses as `Name(f1=…, f2=…)` with `", "`, and classes without fields as `This()`;
- enums as `<Kw.REPAIR: 'Repair'>`;
- strings with CPython's quote choice (single quotes unless the string contains `'` and no `"`)
  through `gcg_core::pyrepr::repr_str`, extended to accept the IR's `printable_non_ascii`
  characters and to reject any other non-ASCII character;
- tuples as `()`, `(x,)` and `(x, y)`; `None`, `True` and `False`; ints in decimal.

Program reprs include the first registrant's label (`registry.py:116-121`), which the IR carries.
Runtime values (`ContRef::StatMod`, `KeywordGrant`, `AbilityGrant`) use the same printer. The
§3.4 tests 2 and 5 check every form against Python before any game runs.

## 6. AI

### 6.1 One player interface

```rust
// crates/gcg-ai/src/contracts.rs (lead-owned)
pub trait Agent: Send {
    /// Seed-path material and GameRecord.agents (ai/agents.py:26,45,81; ai/base.py:60-65).
    fn name(&self) -> &str;
    /// Choose an action for `player` at `st`'s pending decision. `st` is the true state and is
    /// read-only (W-135). Calls to view::derived(st) fill st's view cache, as in Python.
    fn choose(&mut self, st: &GameState, player: u8, cx: &mut AgentCx<'_>) -> Result<Action, AgentError>;
    /// Every entry logged so far (ai/base.py:79-100); the runner slices the new ones per seat
    /// (runner/game.py:96-97).
    fn decision_log(&self) -> &[LogEntry];
}
pub struct AgentCx<'a> {
    pub scratch: &'a mut Scratch,                 // per-thread buffers (§7.1), 128-byte aligned
    pub par: &'a gcg_core::pool::TaskCx<'a, Scratch>,   // fork-join handle (§7.1)
    pub cancel: &'a gcg_core::pool::Cancel,       // SIGINT flag, read between decisions and iterations
                                                  // (the runner checks lowest_failed itself, §7.1)
}
pub struct AgentCore { name: Box<str>, seed: u64, log_enabled: bool, entries: Vec<LogEntry>, decisions: u64 }
impl AgentCore {
    /// ai/base.py:60-65: error if no decision for `player`; decisions += 1 BEFORE seeding;
    /// rng = SplitMix64(derive_seed(seed, name, "decision", decisions)). Counts single-option
    /// decisions too.
    pub fn begin<'s>(&mut self, st: &'s GameState, player: u8) -> Result<(DecisionView<'s>, SplitMix64), AgentError>;
    /// ai/base.py:67-76: log (if enabled) from `st` before the action is applied.
    pub fn finish(&mut self, st: &GameState, player: u8, chosen: Action, alts: &[Alternative]) -> Action;
}
pub struct Alternative { pub action: Action, pub value: f64, pub visits: u32 }
pub struct LogEntry { /* turn, player, decision kind, chosen, label, all alternatives */ }
impl LogEntry { pub fn to_json(&self) -> Json; } // stable sort by (-visits, -value), top 5, round(value, 4), ai/base.py:79-100
```

Implementations:

- `RandomAgent` and `GreedyAgent` (`gcg-ai`), exact ports of `ai/agents.py:28-32` and `:48-66`;
- `ReferenceAgent` (`gcg-ai-reference`, `ai/agents.py:85-98`);
- `SearchAgent` (`gcg-search`);
- `TestRandomAgent` and `TestIllegalAgent` (`gcg-runner`, `tests/runner/random_agent.py`),
  reachable only through `gcg-eval --agent` and tests.

Agents are rebuilt for every game, so the decision counter restarts at 1
(`runner/game.py:62-76`). The runner, `gcg-eval`, `gcg-diff` and the scenario tests hold
`Box<dyn Agent>` per seat. The reference and the strengthened search sit side by side behind
this one trait.

### 6.2 Reference search: exact port (reachable only from `gcg-eval`)

`gcg-ai-reference/src/mcts.rs` ports `ai/mcts.py:82-339` statement by statement, following the
rules of Inv 07 §12 and §16. The implementer checks off this list:

1. **Root moves.** `prune_dominated(canonical(...))` with the config flags (it returns
   `kept or moves` internally, `actions.py:277`), then, whenever a spending move survives, the
   noop seed draw `rng.next_u64()` and the `without_effect` filter with its `... or root_moves`
   fallback (`mcts.py:96-108`), then `merge_equivalent` when the config enables it, which has no
   fallback (`mcts.py:109-110`). A single move returns the canonical representative with no
   further draws.
2. **RNG order.** All seeds are drawn up front: `W[0:B]`, `R[0:B]`, `W[B:cap]`, `R[B:cap]`
   (`mcts.py:121-127`). `cap = max(B, round_ties_even(B * target_extension))` applies only to
   target decisions (`mcts.py:165-178`).
3. **Worlds.** `worlds[k]` is created lazily with `determinize(st, player, W[k])` and never
   mutated; each visit plays in a `clone_from_state` of it (`mcts.py:154-162`). The k-th visit of
   any root child needs world k and visits grow by one, so `worlds` is a `Vec` filled by push.
4. **Tree.** An arena `Vec<NodeData>`. Children sit in a per-node list of
   `(ChildKey, ChildIdx)` with linear lookup; the Python dict is used for lookup only, never
   iterated (`mcts.py:47-62`). `ChildKey = (player, kind value or "", move key)`
   (`mcts.py:267-268`), and `setdefault` keeps the first prior (`mcts.py:317`).
5. **Priors.** `exp((x - top) / T) / py_sum(w)` in move order over the reduced options, computed
   at the root in `first_world` (`mcts.py:305-317`), with `gcg_core::libm::exp` and the Neumaier
   `py_sum`.
6. **Selection.** `avail += 1` before scoring; the first unvisited child by prior (first max);
   UCB `((mean + c*sqrt(ln(avail)/visits)) + (w*prior)/(visits+1))`, first max with strict `>`
   (`mcts.py:271-302`, `gcg_core::pymath::first_max_by`).
7. **`_iterate`/`_descend`** (`mcts.py:205-235`). Single-option decisions are applied without a
   node, and each iteration adds one fresh child. With `observation_keys`, the walk stays in the
   same node after an unobserved opponent pass. The backup path is a list with duplicates (W-06
   behaviour kept).
8. **Rollout.** `play_until_turn(s, horizon, rng, T, 250, knobs)` draws one `random()` per option
   per sampled decision and none for single options (`ai/policy.py:72-101`).
9. **Leaf** (`mcts.py:254-264`). A win scores `1.0 - 0.01 * (turn - root_turn)`, a loss
   `0.01 * Δ`, a draw 0.5; otherwise `0.05 + 0.9 * logistic(score / value_scale)`.
10. **Loop control** (`mcts.py:135-151`). `per_visit` stores `child.total - before`. `_settled`
    applies only when `done != limit`. `_close_call` uses `py_sum` and `gcg_core::libm::square`
    (`mcts.py:181-202`). The extension adds `budget` iterations, up to `cap`.
11. **Result.** The first max of `(visits, mean)` in root-move order (`mcts.py:331-339`).
12. **Pre-game choices** (`ai/setup.py:30-120`).
    - The mulligan uses one compounding shuffled pool and plain `+=` (`setup.py:63-83`).
    - Play/draw draws `games` seeds up front (`setup.py:98`). It applies with `check=True` and uses
      the same seed for determinisation and policy (`setup.py:114-120`). The paired test uses
      Neumaier sums and libm square (`setup.py:101-111`).
    - The `2 × games` simulations are pure functions of `(st, player, choice, seed, knobs)`. They
      may run through `cx.par.fork_join`, each on its own clone of `st`, with the sums taken
      afterwards in list order. This is exact (§7.3), and a test compares fork-join with serial
      on every play/draw decision of the `search` set.
13. **Decision log.** `round(value, 4)` via `gcg_core::pyfloat::round_f64`, a stable sort by
    `(-visits, -value)`, top 5, with labels from `describe`/`card_label` (`ai/base.py:79-122`).

The shared primitives in `gcg-ai` are exact ports of their Python modules: canonical keys,
`prune_dominated`, `merge_equivalent`, fingerprints with the `OnceKey` rules of §4.2,
`without_effect`, policy scorers, values, targeting and evaluation. `option_scores` takes the
option list explicitly (Inv 08 §1). Memo caches keyed by `(program_id, pc)` or `def_id` live in
the per-registry `AiTables` (W-33). Phase 6 never changes a shared component's behaviour at the
reference knob values. New behaviour goes into new functions or new knob fields, and the `search`
traces and the `components` vectors pin this.

### 6.3 Greedy and random

`RandomAgent` plays `options[rng.randrange(len)]` and draws even for one option. Its
alternatives are `1.0/n` with visits 0.

`GreedyAgent`:

- one `determinize` with `rng.next_u64()`;
- `burst_density` on the true state;
- each pruned canonical move played to turn + 1 with the argmax policy and `BASELINE_KNOBS`
  (200 decisions), then scored by `evaluate` with the given weights, first max;
- CHOOSE_FIRST goes first with no draw, and REDRAW uses 48 mulligan samples.

Both are frozen baselines: W-22 is resolved by evaluation tooling, not by changing greedy.

### 6.4 Presets

Reference presets, exact, reachable only through `gcg-eval` (`ai/config.py:80-111`,
`ai/baseline.py:18-73`, `ai/tuning.py:50-57`):

| field | fast | standard | strong | baseline-standard | baseline-strong | tuning |
| --- | --- | --- | --- | --- | --- | --- |
| agent name | `mcts-fast` | `mcts-standard` | `mcts-strong` | `mcts-standard` | `mcts-strong` | `mcts-tuning` |
| iterations `clamp(ipo*n, min, max)` | 7n in [16, 64] | 64n in [160, 640] | 128n in [320, 1280] | 32n in [80, 320] | 128n in [320, 1280] | 6n in [16, 48] |
| exploration | 0.15 | 0.3 | 0.15 | 0.3 | 0.15 | 0.3 |
| rollout_temperature | 0.3 | 0.5 | 0.3 | 0.5 | 0.3 | 0.5 |
| first_player_games / mulligan_samples | 8 / 96 | 24 / 96 | 48 / 192 | 24 / 96 | 48 / 192 | 8 / 32 |
| value_scale | 1.0 | 1.0 | 1.0 | 2.0 | 2.0 | 2.0 |
| burst score; noop branch, noop resource, strict attack prune, rest-target prune, merge, observation keys, rule table, 0-AP face | adopted: 4.0, all true | adopted | adopted | 1.0, all false | 1.0, all false | 1.0, all false |
| target_extension / z | 3.0 / 2.0 | 3.0 / 2.0 | 3.0 / 2.0 | 1.0 / 2.0 | 1.0 / 2.0 | 1.0 / 2.0 |
| prior_weight / prior_temperature / horizon_turns / max_rollout_decisions | 0.5 / 1.5 / 2 / 250 | same | same | same | same | same |
| evaluation weights | `ROLE_WEIGHTS` (`DEFAULT_WEIGHTS`, `evaluation.py:129`) | `ROLE_WEIGHTS` | `ROLE_WEIGHTS` | `TUNED_WEIGHTS` (`BASELINE_WEIGHTS`, `baseline.py:74`; `compare.py:89-94`) | `TUNED_WEIGHTS` | per `START_WEIGHTS` of `ai/tuning.py` |

Greedy and the MCTS agent default to `DEFAULT_WEIGHTS` = `ROLE_WEIGHTS` (`agents.py:43`, `:79`,
`:119`). A wrong pairing would silently change
`ref:baseline-*` and the `test_baseline` digest (`tests/ai/test_baseline.py:27-32`), so the
preset-dump test (T-060) compares the weights bit for bit as well.

`ReferenceConfig` reproduces Python's field defaults (W-136 keeps defaults in the reference type
only). It also reproduces the `__post_init__` horizon check with Python's message
(`config.py:48-53`) and `budget()` (`config.py:55-57`). A test compares every field of every
preset with `oracle/export_ai_vectors.py`'s dump of the live Python objects.

Shipped `gcg-sim` presets (strengthened search). These are the initial values; Phase 6 tunes them
(§12.8):

| field | `standard` | `fast` |
| --- | --- | --- |
| starts from | reference `strong` | reference `standard` |
| iterations | 128n in [320, 1280], then scaled up under the 2x floor | 64n in [160, 640], then scaled down while at least as strong as reference `standard` |
| exploration, rollout T, play/draw games, mulligan samples | 0.15, 0.3, 48, 192 | 0.3, 0.5, 24, 96 |
| horizon | 2, then set by ablation (W-01) | 2, then set by ablation |
| mechanisms | every adopted Phase 6 mechanism | the adopted subset that pays at its budget |
| auxiliary work cap per decision (solver, belief replays, look-ahead), in engine `apply` calls | set by ablation | set by ablation |
| help text (brief wording) | "uses the full search so that each player plays at the level of the strongest top-tier competitive player; for refining and polishing the final deck" | "trades search depth for throughput; for rapidly screening deck variants, sub-variants and hypothetical builds" |

The only source of this table in the shipped docs is Rust `docs/AI.md`, which also gives the
mapping to the reference presets. `gcg-search` holds the constants. A test parses the doc table
and compares it with them (W-129).

### 6.5 Why the reference can never leak into `gcg-sim`

1. **Separate types.** `ReferenceConfig` (in `gcg-ai-reference`) and `StrengthenedConfig` (in
   `gcg-search`) have no conversion between them. `StrengthenedConfig` has no `Default`, and
   every preset spells out every field (W-136). A rustdoc `compile_fail` doctest shows that
   `StrengthenedConfig { ..Default::default() }` does not compile.
2. **Separate crates.** `gcg-cli` does not depend on `gcg-ai-reference` (§2.2 test and the
   `cargo tree` check).
3. **Parser.** `--ai-preset` maps exactly `standard` (the default) and `fast` to
   `gcg_search::Preset`. Any other value, `strong` included, is the argparse-style usage error
   `gcg-sim benchmark: error: argument --ai-preset: invalid choice: 'strong' (choose from
   'standard', 'fast')` with exit 2 (`crates/gcg-cli/tests/presets.rs`; argparse 3.15 form,
   Inv 01 §3.3).
4. **Outputs.** Agent names differ (`search-*` vs `mcts-*`, A9), and a test asserts that no
   `gcg-sim` output contains `mcts-`. `gcg-sim` writes schema 1.2.0 with
   `ai.search.algorithm = "gcg-search"`.
5. **Reviewer check.** The reference constants are in `gcg-ai-reference/src/contracts.rs`
   (`PRESETS`, `BASELINE_PRESETS`, `TUNING_CONFIG`; as built, there is no `config.rs`), which the
   Phase 7 reviewer reads next to `ai/config.py:80-111` and `ai/baseline.py:18-73`.
   `gcg-search/src/presets.rs` (`mapped_reference`) repeats the budgets of reference `standard`
   and `strong` as the shipped presets' starting point, because `gcg-search` cannot depend on
   the reference crate; `crates/gcg-search/tests/null_config.rs` pins that copy to the
   reference (`reference_mapping` equals the renamed null configuration of each).

### 6.6 What a "node" is

A node is one MCTS iteration, which adds at most one new tree node, as in Python. Every search
also counts, per decision, engine `apply` calls by purpose (`tree`, `rollout`, `aux`), worlds
built and rollouts capped (W-15, W-21). These deterministic counts appear in `gcg-eval` traces
and in the results schema 1.2.0 totals. They are never used to stop a search, and no budget
reads a clock.

## 7. Parallelism model

### 7.1 Pool (`gcg_core::pool`, safe Rust on `std::thread::scope`)

```rust
pub struct Cancel { flag: Arc<AtomicBool> }        // set by SIGINT only (A3)
pub struct Pool { threads: NonZeroUsize }          // per run: lowest_failed: AtomicUsize (usize::MAX = none)
impl Pool {
    pub fn new(threads: NonZeroUsize) -> Pool;
    /// Runs task(i) for i in 0..n and returns the results in index order whatever the schedule.
    /// on_done(count) runs on the calling thread after each completion (counts, never indices).
    pub fn map_indexed<S: Default + Send, T: Send, E: Send>(
        &self, n: usize, cancel: &Cancel,
        task: impl Fn(usize, &mut S, &TaskCx<'_, S>) -> Result<T, E> + Sync,
        on_done: impl FnMut(usize),
    ) -> Result<Vec<T>, PoolError<E>>;   // PoolError::Task { index, error } = lowest failing index; ::Cancelled
}
pub struct TaskCx<'p, S> { /* handle to the running pool */ }
impl<S: Default + Send> TaskCx<'_, S> {
    /// Fork-join inside a task: queues inputs[1..] as jobs, runs job 0 itself, then runs queued
    /// jobs (its own first) until all are done. Returns results in input order.
    pub fn fork_join<I: Send, T: Send>(&self, inputs: Vec<I>, f: &(dyn Fn(usize, I, &mut S) -> T + Sync)) -> Vec<T>;
}
```

- **Top level.** With `threads == 1`, tasks run in index order on the calling thread, and
  `fork_join` runs its jobs inline in order. Otherwise `std::thread::scope` starts
  `min(threads, n)` workers, as `processes_used` does (`runner/run.py:83-85`). Each worker loops
  `i = next.fetch_add(1, Relaxed)` and stops when `i >= n`. Results go into slot `i` of a
  pre-sized `Vec<OnceLock<Result<T, E>>>`. A completion message goes over an `mpsc` channel to
  the calling thread, which calls `on_done(count)` for progress. Progress prints counts only
  (`tests/runner/test_pool.py:29-40`, `runner/run.py:67-80`).
- **Nested level.** `fork_join` pushes jobs to one shared `Mutex<VecDeque<Job>>` with a `Condvar`.
  A worker with no match left blocks on the condvar and wakes when a job is pushed or the run
  ends. A task waiting for its jobs runs queued jobs itself ("help first"), so no thread idles
  while work exists. Waiting is a plain OS blocking wait: no spinning, no sleep or back-off, no
  timeouts, no QoS classes, no core pinning (brief `<target_hardware>`).
- **Why not per-worker deques with stealing** (X8). Tasks are at least a millisecond long
  (whole matches, root trees, play/draw simulations). With at most 12 workers a single
  mutex-protected queue does not contend. Results are slot-indexed, so the order in which jobs
  are taken cannot affect them. Idle workers still take other workers' pending jobs, which is the
  property the brief asks of a work-stealing pool, without randomised victim selection. `rayon`
  is not available offline (A8).
- **Scratch.** Each worker owns one `S`. Each nested job borrows one from a free list
  (`Mutex<Vec<Box<S>>>`) that grows on demand and is kept for the whole run, so the search path
  allocates nothing once warm. `S` is `#[repr(align(128))]` (`hw.cachelinesize` = 128 on this
  M2 Pro). The counter and the queue head each sit on their own 128-byte line, so workers never
  share a written cache line.
- **Determinism.** Every task result is a pure function of its inputs: `(config, master seed,
  match index)` (`runner/config.py:64-80`) for matches, and `(state clone, seed)` for nested
  jobs. Nothing reads the thread id, the worker count, the completion order or the time. So the
  returned `Vec` is identical for any `--workers` and any schedule. Gates at `--workers 1/4/10`
  check this for every player (§9.11).
- **Failure** (W-149). Failure and interrupt use separate signals. When match `k` fails, the
  worker lowers the run's `lowest_failed: AtomicUsize` to `k` with `fetch_min`. Workers stop
  claiming indices above `lowest_failed`, abort only running matches whose index is above it
  (checked between decisions, like `Cancel`), and let every lower index finish. A lower index
  that fails later lowers the value again. The pool reports the final `lowest_failed`, which is
  what the serial path reports (`runner/run.py:45-53`). `Cancel` is reserved for SIGINT. The
  message is Python's: `match {m} game {g+1} (seed …) failed: {Class}: {msg}`
  (`runner/game.py:130-136`). A test injects failures at indices 3 and 7 under `--workers 10`
  (T-074).

### 7.2 Seeds independent of scheduling

| Seed | Derivation |
| --- | --- |
| game | `derive_seed(master, "match", m, "game", g)` (`runner/config.py:75-76`) |
| agent | `derive_seed(master, "match", m, "game", g, "agent", seat)` (`:79-80`); the deck under test sits in seat `m % 2` (`:64-66`) |
| per decision | `derive_seed(agent_seed, agent_name, "decision", k)`, k from 1 (`ai/base.py:60-65`) |
| hidden state | `"hidden-rng"` and `"permuted-rng"` paths (`engine/observe.py:56-83`, `:113-131`) |
| head-to-head | `mirrored_specs` and `comparison_specs` (`ai/selfplay.py:88-110`, `ai/compare.py:127-144`) |
| strengthened search | tree t: `derive_seed(decision_seed, "tree", t)`; belief candidate j of world k: `derive_seed(tree_seed, "belief", k, j)`. Never derived from a thread |

### 7.3 Task granularity on heterogeneous cores

- **One match per top-level task** for `gcg-sim benchmark` and `gcg-eval reference-benchmark`.
  In BO3, game g+1's chooser is game g's loser (`runner/match.py:19-46`), so a match cannot be
  split without speculation. Python has the same unit, so the comparison is fair.
- **Exact intra-match parallelism.** `first_player_choice` runs 24 or 48 independent games per
  match start, depending on the preset (`ai/setup.py:86-111`; seeds drawn up front at `:98`).
  They go through `fork_join` on per-job clones, and the sums run afterwards in list order. Phase 5
  keeps this only if the external comparison shows a win. It changes no output either way.
- **Strengthened presets** (Part 2). Each search decision forks K deterministic root trees
  (§12.3, S1), giving jobs of a few milliseconds. Workers without a match help running matches
  through `fork_join`. So in the tail every super, performance or efficiency core works on the
  last matches instead of idling behind a straggler. K is a preset constant, never derived from
  the thread count.
- **`gcg-eval head-to-head`** uses one BO1 game per top-level task.
- **No core-type logic.** macOS migrates threads freely and the brief forbids QoS tuning, so the
  scheduler never places work on a tier. Dynamic claiming plus fine nested jobs makes placement
  irrelevant. This works unchanged for 2 tiers (M2 Pro: 6P + 4E) and 3 tiers (M6: 2 super + 4
  performance + 6 efficiency).

### 7.4 `--workers auto` and topology

`gcg_core::sys::topology::detect()` runs `sysctl -n hw.nperflevels`, then for each level
`N < nperflevels` reads `hw.perflevelN.logicalcpu` and `hw.perflevelN.name`, plus
`hw.cachelinesize`. On the M2 Pro that gives two levels (6 Performance + 4 Efficiency); on the
M6, three (2 + 4 + 6). `auto` resolves to the sum over all levels, capped by the match count,
because every core is assumed to run at full speed (brief step 18). If `sysctl` fails, the
fallback is `std::thread::available_parallelism()`. The parser is unit-tested on captured `sysctl`
outputs for two and three tiers (fixtures, no live machine needed). `timing.json` and
`gcg-eval env` record the tiers. `gcg-sim --help` documents `auto`.

Phase 5 measures `auto` against 6, 8 and 10 with the external script. If a smaller count is
faster in every configuration, `auto` is redefined as a fixed rule over the tiers, with the
evidence in BENCHMARKS.md. The rule stays static topology arithmetic and never measures anything
at run time.

### 7.5 Interrupts (exit 130)

`gcg_core::sys::sigint::install(&cancel)` (A3) registers the `Cancel` flag with
`signal_hook::flag::register(SIGINT, …)`, so SIGINT sets it; the module is safe code. Workers check it at
match start and between decisions, and searches check it between iterations; each check is a
relaxed load. A worker that sees it ends its match with `RunError::Interrupted`. The coordinator
then stops dispatch, joins the workers, writes no report, prints
`interrupted: workers stopped, no reports written` to stderr and exits 130 (`cli.py:397-399`).

The flag is also checked before and after each report file. An interrupt during report writing
therefore prints the same message, as Python does even when some files were already written
(Inv 01 hazard 54). This is event handling, not timing logic.

The ported test (`crates/gcg-cli/tests/interrupt.rs`):

1. Start `gcg-sim benchmark … --matches 100000 --workers 2`.
2. Read stderr until the first `matches 1/100000` line.
3. Send SIGINT with `std::process::Command::new("kill").args(["-INT", pid])`.
4. Wait for exit (blocking, no timeout).
5. Assert exit code 130, the message, empty stdout and no `results.json`.

It has no elapsed-time or timeout assertion (D13).

## 8. Oracle and differential strategy (Phase 2, used through Phase 7)

### 8.1 Python-side tooling (`oracle/`, never under `src/gcg_sim/`)

| File | Owner | Exports |
| --- | --- | --- |
| `export_vectors.py` (exists) | lead | RNG, float and JSON vectors for `gcg-core` (`crates/gcg-core/tests/vectors.json`) |
| `export_stats.py` | F | Wilson, Newcombe and rounding vectors, and `summarize` outputs (`reports/stats.py`, `ai/compare.py:146-178`) |
| `export_ir.py` | B1 | `crates/gcg-effects/ir/registry.json` (§3.3) |
| `export_data.py` | A | All 1153 `CardDef`s field by field, the token map, unit variants and applied overrides. Deck-parser triples (text → messages/violations/digest) over the example decks and the `make_deck` invalid decks. `data status` text and `--json` bytes |
| `export_unicode.py` | A | NFKC decomposition/composition/CCC, full casefold and `str.isspace` tables from Python 3.15.1's `unicodedata` as `crates/gcg-data/src/unicode/tables.rs`, plus 10,000 random-string vectors (A27) |
| `export_ai_vectors.py` | D1/D2 | The `components` set (§8.3): per sampled state, `canonical`, `prune_dominated` per flag set, `merge_equivalent`, `without_effect` key sets, `option_scores` (float hex), `choose(rng=None)`, seeded `pick`, evaluation features/score/`evaluate`/`burst_density` (float hex), polarity per `(program, pc)`, `opening_score`, mulligan and play/draw results, and `search()` results with the RNG state after; dumps of every reference preset object (D2) |
| `export_records.py` | E | `BenchmarkRun` inputs as JSON (records, config, factory string) with the Python report files they produce, for at least 12 runs: bo1/bo3, decision log on/off, draws, all three presets. CLI transcripts (argv → stdout, stderr, exit code) for every message in Inv 01 §3.9 and every `tests/cli` invocation |
| `export_decks.py` | F | The `tests` and `tuning` deck sets as deck files |
| `traces.py` | G | Game traces (§8.2) for the sets `full`, `vanilla`, `fuzz`, `coverage`, `search`, `baselines`, `views`, `worlds`; `--write`, `--check`, `--tier 1\|2`, `--nocache` (§8.5); `state --trace ID --decision K` for `gcg-diff explain`; `--deep ID --decision K [--apply-level]` for drill-down levels 2-3 (§8.7); and the ad hoc mode `game --decks A B --seed S [--chooser P] --agents random\|policy\|ref:fast\|ref:standard\|ref:strong\|greedy [--snapshots all] --out FILE`, which writes one trace of any deck pair at any reference preset (brief step 6) for `gcg-diff corpus --file FILE` |
| `pytest_trace.py` | G | pytest plugin for the `tests` set, run as `uv run pytest tests/rules tests/cards tests/effects tests/ai -p oracle.pytest_trace --trace-out oracle/corpus/v1/tier1/tests` in one process without `-n`, except the override tests, which it runs one per fresh process (A16). It encodes DSL-object arguments of `call` and `probe` records into the test's IR fragment (§8.2) |
| `coverage_decks.py` | G | Covering-deck generator for the `coverage` set |
| `census.py` | G | Container-size statistics and the executed-program census over the corpus (§8.6) |
| `offset_hook/sitecustomize.py` | G | Numbering-shift probe (§8.9) |
| `export_tags.py` | G | `gcg_sim.tools.marks.scan` output (rule/card/faq/ruling tags per test id) for traceability |

Every exporter is deterministic, writes with explicit `json.dumps` options and has `--check`.
Exporters only observe. They call public functions, wrap `apply` transparently and never change
behaviour. The M3 gate shows this: wrapped runs give the same `final_state_sha256` and action
lists as plain runs, and identical bytes under `PYTHONHASHSEED=1` and `2`. Python is only needed
to regenerate. The committed outputs make every tier-1 Rust test runnable without Python.
`oracle` passes `uv run ruff check oracle`, `uv run ruff format --check oracle` and
`MYPYPATH=src uv run mypy --strict oracle` (W-154).

### 8.2 Trace format (NDJSON, one object per line, version 1)

```text
{"t":"header","v":1,"id":"fuzz/s1/g0017","kind":"game|test|search",
 "generator":{"git":"…","python":"3.15.1","code_digest":"…","ir_sha256":"…","data_lock":"…",
              "hash_seed":1,"effect_offset":0,"max_uid":131},
 "decks":[{"main":[…numbers in the caller's order],"res":[…]},{…}],"seed":<u64>,"chooser":null|0|1,
 "turn_limit":200,"max_actions":20000,"agents":["random","random"],"agent_seeds":[…],
 "preset":null|"fast"|"standard"|"strong"|"baseline-standard","options_mode":"full|hash"}
                                       // tier 1 is always "full"; "hash" only in tier 2 (A21)
{"t":"snapshot","i":0,"state":<canonical_state>,"continuous":{cid: node},"filter_sets":{cid:[…]},
 "rules_checked":812}                    // start, end, every 32nd decision, after out-of-band changes
{"t":"decision","i":<k>,"player":p,"kind":"main","prompt":"main phase","context":[["min",1]],
 "options":[["play_unit",2,-1,1],…] | "options_sha16":"…","n":7,
 "chosen":[…],"rng":"<game rng state u64>","version":812,"rc":811,"action_count":43,
 "event_group":201,"turn":9,"step":40,"digest16":"<first 16 hex of state_digest before applying>",
 "agent":{"k":3,"seed":"<per-decision seed>","rng_after":"<state after choose>",
          "iters":154,"budget":256,"cap":256,"roots":[[…],…],
          "alts":[[[…action],"0x1.b00be33f8f4adp-2",117],…]}}     // agent decisions only; ALL alternatives
{"t":"iter","decision":<k>,"n":<iteration>,"child":<root index>,"k":<visit>,"fresh":true,
 "path_len":5,"applies_walk":4,"applies_rollout":61,"leaf":"0x…","terminal":false,
 "total_before":"0x…","total_after":"0x…","world16":"…"}           // drill-down level 2 only
{"t":"apply_trace","decision":<k>,"n":<iteration>,"j":<index>,"action":[…],"after16":"…"} // level 3 only
{"t":"outcome","winner":1,"end_reason":"battle_damage","turn":20,"actions":115,
 "final_digest":"<full sha256>","state":<canonical_state>}
```

Per-test traces (`kind: "test"`) add these records:

- `{"t":"scenario","test":"tests/rules/test_turn.py::test_x","tags":[…],"calls":[…],"state":…,"continuous":…,"rules_checked":…}`
  — the Scenario method calls with their arguments, captured by wrapping every `testkit.Scenario`
  method (`testkit.py:38-182`), and the canonical state before `advance` in `Scenario.start`;
  or `new_game` with decks, seed and chooser.
- `{"t":"apply","action":[…],"check":true,"before16":"…","after16":"…","pending":…,"error":null|{"type":"IllegalActionError","message":"…"}}`
  — every `apply` outside an agent. The plugin patches every module alias, testing
  `sys.modules[m].apply is original` (Inv 10 hazard 18).
- `{"t":"call","fn":"core.destroy","args":[…],"result":…,"after16":"…"}` — direct calls from
  test modules into public engine functions (`core.*`, `B.*`, `I.*`, `observe.*`, `game.*`),
  wrapped by module alias like `apply`. Rust executes them through a name dispatcher in
  `gcg-diff`. A name without a dispatcher entry is a counted failure, never a skip.
  **Argument encoding.** Ints, strings, bools, `None`, tuples and lists are JSON. Engine enums are
  `{"enum": "<Name>", "value": "<value>"}`. Every DSL object a test builds itself (filter tuples
  passed to `V.matches`, `KeywordGrant`, `StatMod` or `RuleGrant` effects passed to `add_lasting`,
  as in `tests/rules/test_keywords.py:119`, `tests/cards/test_wp_gd05_b.py:1938`,
  `tests/rules/test_cardinfo.py:198`, `tests/cards/test_wp_gd04_a.py:1111`) is hash-consed into
  the test's IR fragment, and the argument is `{"$": <fragment NodeId>}`. The fragment also gets
  `continuous` and `filter_sets` entries (content id → node), so the content ids of such
  effects round-trip through `canonical_state`. G's M3 gate encodes and decodes one synthetic
  call per argument kind (T-157).
- `{"t":"state","state":…,"rules_checked":…,"derived":"none|fresh|stale", "view_from":…,"view_key":…}`
  — emitted whenever a test mutated the state directly between engine calls, detected by digest
  (for example `tests/cards/test_wp_gd03_a.py:47-48`, `tests/rules/test_zones.py:145`). With
  `"derived":"stale"`, Python still holds a cached view computed before the write
  (`view.py:232-239`: the cache stays valid while the view key matches). `view_from` is the
  canonical state at which that view was computed, and `view_key` its key. Rust computes the view
  from `view_from`, installs it in the loaded state's cache under `view_key`, and only then
  replays the next call. The census counts the tests that need this, so the mechanism is
  exercised and tested.
- `{"t":"view","sig":…}` — `tests/paranoid.py:24-39`'s view signature after each apply.
- `{"t":"probe","fn":"V.ap_of","args":[…],"result":…}` — testkit and `V.*` queries in call order.
  Probes call `derived()`, which matters for caching, so Rust evaluates them in the same order.
- `{"t":"override","fragment":<IR fragment>}`, `{"t":"force_link","units":[…]}`, and
  `{"t":"end","outcome":"passed|failed"}`.

Floats are written as `float.hex()` strings. Ints are JSON numbers, except u64 values of 2^53
and above, which are decimal strings, so nothing is lost. Lines are compact JSON with sorted keys
and `ensure_ascii=True`.

`options_sha16` (tier 2 only) is the first 16 hex characters of
`hashlib.sha256(json.dumps([o.to_json() for o in options], separators=(",", ":"), ensure_ascii=True).encode()).hexdigest()`,
with `Action.to_json() = [kind.value, a, b, c]` (`engine/state.py:21-22`). A vector in
`crates/gcg-diff/tests/vectors/` pins it.

### 8.3 Corpus composition, size and tiers

Tier 1 is committed in `oracle/corpus/v1/tier1/` (at most 45 MiB plain, A22; files of at most 8 MiB;
`MANIFEST.json` holds the SHA-256, record count and generator block per file). Tier 2 is
regenerated into `build/oracle/tier2/` and must match `oracle/corpus/v1/MANIFEST-tier2.json`.

| Set | Tier 1 content | Tier 2 adds | Agents (Python side) | Options | Size estimate (tier 1) |
| --- | --- | --- | --- | --- | --- |
| `full` | 2 vanilla-deck and 2 example-deck games with the canonical snapshot at every decision | — | random / policy | full | 3 MB |
| `vanilla` | 40 games of `testkit.vanilla_deck` colour pairs; every game's setup prefix up to its first MAIN decision | — | random | full | 1.5 MB |
| `fuzz` | `tools/fuzz.py` decks: seed 1 games 0-59 and seed 5 games 0-39 (`test_small_fuzz_smoke`), plus the per-game summary list and the summary JSON of seed 2026 games 0-9999 | seed 1 games 0-499 (the CLAUDE.md command) | the fuzz agent (`tools/fuzz.py:83-119`) | full (tier 1), hash (tier 2) | 6 MB |
| `coverage` | The three example decks in all 6 ordered pairings, 2 games each. Covering decks: every main-pool card number (956 main numbers + 108 resource numbers, Inv 10 §6.1) in at least one legal deck of 12-13 target numbers × 4 copies within one colour pair, each deck played once | 3 games per covering pairing; seeds added until every non-vanilla card has executed a program | the reference rollout policy `policy.choose(st, rng, 0.5, BASELINE_KNOBS)` for both seats, which plays effects far more often than random play | full | 6 MB |
| `tests` | Every test in `tests/rules` (647), `tests/cards` (2,245), the engine-level tests of `tests/effects`, and the AI tests that call agents or AI functions (puzzles, information sets, root moves, search options) | — | the tests themselves | full | 10 MB |
| `search` | Example pairings at the reference presets: `fast` 4 BO1 games, `standard` 2, `strong` 1, all with decision logs and the agent block. The `tests/ai/test_baseline.py` game (baseline-standard, seed 4242, digest `5cff789d…`). The three acceptance searches of Inv 07 §17 with level-2 iteration records | all 6 ordered pairings × fast/standard/strong × 3 seeds, BO3 included (chooser paths, mulligans, play/draw) | reference MCTS | full | 1.5 MB |
| `baselines` | 4 greedy-vs-greedy and 4 random-vs-random games with decision logs | — | greedy / random | full | 0.5 MB |
| `views` | 2,000 states sampled from `tests` and `coverage`: canonical state + view signature + per-card `ap/hp/keywords/traits/play_cost/play_level` + the trigger index | — | — | — | 1.5 MB |
| `components` | At every 5th multi-option decision of `search`, `baselines` and `coverage`: the `export_ai_vectors.py` outputs (§8.1) for both flag sets and both knob sets, floats as hex | — | — | — | 1.5 MB |
| `worlds` | 100 decisions sampled from `coverage`: `determinize(st, p, s)` worlds played to the end by the rollout policy at T = 0.5 with a seeded RNG. These reach engine paths that only occur inside determinised worlds: hidden-rng shuffles, `_own_history`, padded pools (`engine/observe.py:56-110`) | 1,000 worlds | policy | full | 0.5 MB |

Rust replays the recorded chosen actions, so any Python agent can generate engine traces. Only
`search` and `baselines` need the Rust agents to reproduce the choices. The Phase 2 census (§8.6)
replaces every size estimate with a measured value. If tier 1 exceeds 45 MiB, `coverage` drops to
covering decks only and `fuzz` to 40 games, and the cut moves to tier 2.

### 8.4 Regeneration and checks

- `uv run python oracle/traces.py --write --tier 1` regenerates tier 1 and its manifest.
  `--tier 2` regenerates tier 2 into `build/oracle/tier2/`; this takes several hours on 8
  cores (estimate).
- The generator runs everything twice, under `PYTHONHASHSEED=1` and `=2`, and refuses to write if
  the two differ (W-150, W-160).
- `--check` regenerates into a temporary directory and compares against the manifest.
- On the Rust side, the gate checks each manifest against the files, and each header's
  `ir_sha256` against the embedded IR. A corpus with a stale IR is refused.
- The corpus is regenerated only when the Python reference, the data or the trace format
  changes. Each regeneration is its own commit, and `docs/port/progress.md` records the reason.

### 8.5 Cache-transparency check (Python side)

`traces.py --check --nocache` regenerates `vanilla`, `fuzz`, `tests` and `search` with
`view.derived` replaced by always-compute in every `gcg_sim` module namespace. That is the method
of Inv 11 §7 (`nocache.py`). The traces must be byte-identical to the committed ones. Any
difference is a staleness site that Rust reproduces exactly (§4.6 does so by construction), and it
is recorded in `docs/dev/EQUIVALENCE.md`. Together with the Rust `paranoid` run over the whole
corpus, this establishes transparency before Phase 5 may change invalidation (W-124).

### 8.6 Container census and executed-program census

`census.py` replays every corpus game in Python and reports the p50/p99/p99.9/max of every
container:

- cards, each zone, battles, lasting, delayed, targets per lasting;
- pending triggers, batches and batch sizes;
- frames, vars per frame, list length per var, ints per frame;
- once_used, history, effect_hits, decision options and context, event payload length;
- view rule lists and ability lists.

C uses the report to fix the inline capacities and initial arena sizes (§4.3). The census also
counts fixpoint iterations per compute (Inv 11 Q4). It records, per card number, whether one of
its programs ran: a frame pushed, a trigger queued and resolved, an activation, a command play,
or a constant applied in a view. It also records whether the card was ever instantiated in a
traced state. `gcg-diff coverage` re-measures both during the Rust replay (§9.3).

### 8.7 The Rust differential runner (`gcg-diff`)

```text
gcg-diff corpus [--tier 1|2] [--only <set>[:<filter>] ...] [--file TRACE.ndjson] [--require-all] [--parse-only]
                [--roundtrip] [--paranoid] [--jobs N] [--report build/diff.json] [--junit build/rust-junit.xml]
gcg-diff explain <trace-id> <decision>            # at a stored snapshot: offline; elsewhere needs uv to regenerate
                                                  # Python's snapshot; prints a JSON-path diff (A21)
gcg-diff trace-search <trace-id> <decision> [--level 2|3]   # needs uv for Python's iteration records
gcg-diff selftest                                 # negative controls
gcg-diff fuzz --games N (--seed S | --seed-from-commit SHA) [--workers W|auto] [--compare FILE]
              [--agent random|greedy|ref:<preset>|search:<preset>[:field=value…]|mixed:<spec>,<spec>] [--paranoid]
gcg-diff paranoid-games                           # tests/test_engine_caches.py: 16 random games (seed 11), 2 greedy games
gcg-diff coverage --check | traceability --check|--write | tests-json --check
gcg-diff schema-check [--expect-invalid] FILE... | weaknesses --check FILE | lints
```

`--roundtrip` only loads every snapshot with `from_canonical`, re-serialises it, and compares the
canonical bytes and the recorded digest. That is the M5.1 gate, before replay exists.
`--parse-only` only decodes records and checks hashes (the M3 gate).

**Game trace replay.**

1. Build the decks from the header and call `new_game` with the recorded seed and chooser.
2. For each `decision` record, compare in order:
   - `pending` (player, kind, prompt, context);
   - the option list (or the SHA-256 prefix of the Python JSON of the list);
   - the fingerprint: `version`, `rc`, `rng`, `action_count`, `event_group`, `turn`, `step`,
     `digest16`.

   Then apply the chosen action with `check = true`.
3. At each `snapshot`, compare the full canonical state. At `outcome`, compare winner, end
   reason, turn, action count, final digest and the full final state.
4. Run invariants after every action (§9.4).

**Search and baselines traces** also run the Rust agent for the recorded seat with the recorded
agent seed. They compare the chosen action, every alternative (action, mean bits, visits), the
iteration count, the budget and cap, the root moves, the logged entry bytes and the RNG state
after the search.

**Per-test traces** start from the `scenario` record. The Rust testkit rebuilds the state from
the recorded Scenario calls and checks it against the recorded snapshot; this validates the Rust
testkit on about 2,900 tests. If the two differ, the trace fails, and the runner also reports
whether replay from the snapshot would pass. The replay then compares every `apply` and `call`
(digests, pending, error type and message), the `view` signatures and the `probe` results in
order, and loads `state` records without touching.

**Drill-down for the reference search.**

| Level | Content | Where |
| --- | --- | --- |
| 0 | actions and final digest per game | every game trace |
| 1 | per decision: the `agent` block (all alternatives, iterations, budget, cap, roots, rng_after) | `search` and `baselines` traces |
| 2 | per iteration of one decision: visit index, root child, fresh flag, path length, applies in walk and rollout, leaf value hex, terminal flag, child total before/after hex | committed for the Inv 07 §17 searches; on demand: `traces.py --deep ID --decision K` |
| 3 | per apply inside one iteration: action and digest after it | on demand: `--apply-level` |

`gcg-diff trace-search` runs both sides. It prints the first divergent iteration, then the first
divergent apply inside it, then the structural state diff at that point.

**Divergence report.** Each trace stops at its first divergence; the runner continues with the
other traces and exits 1 if any diverged.

```text
DIVERGED fuzz/s1/g0017 at decision 43 (turn 9, step MAIN, player 1, kind main)
  options       identical (7)
  fingerprint   python version=812 rng=…19 action_count=43 event_group=201 digest16=3f2a9c01aa77e2b1
                rust   version=813 rng=…19 action_count=43 event_group=201 digest16=9c01e5d0b2c4a1f7
  first field   version (+1): an extra or missing touch() since decision 42
  last action   activate(50,765,0) at decision 42 (GD02-096 ability 765)
  state diff    gcg-diff explain fuzz/s1/g0017 43
```

`explain` prints a JSON-path diff with card fields named (`cards[66].zone: python 8 (TRASH),
rust 7 (HAND)`). For options it prints a side-by-side list in `Action` display form
(`play_unit(2,1)`).

**Negative controls** (`gcg-diff selftest`, a gate item from M5). The runner must report a
divergence at the exact record for each of these mutated copies of a real trace: (1) one extra
`touch`; (2) two options swapped; (3) one RNG draw skipped; (4) one card's zone changed in a
snapshot; (5) a float mean off by one ulp in a search log; (6) a missing trailing newline in a
report file; (7) a once-per-turn key inserted twice (`once_used` set semantics, §4.3). Before
the engine exists (M3), the same controls run on synthetic traces against a `MockEngine` that
replays the recorded observations.

### 8.8 Phase 4 end-to-end equivalence

1. `gcg-diff corpus --tier 1 --require-all` and `gcg-diff corpus --tier 2 --require-all`: 100% of
   every set, including the search traces at `fast`, `standard` and `strong`, once plain and once
   with `--paranoid`.
2. `gcg-eval compare` against Python `python -m gcg_sim.ai.compare` for three configurations:
   stdout JSON and the `--out` file must be byte-identical. This exercises the `mirrored_specs`
   seeding.
3. Report parity without the AI: `gcg-eval reference-benchmark --agent test-random` must equal
   Python run with `GCG_SIM_AGENT_FACTORY=tests/runner/random_agent.py:make_random_agent`, in the
   `tests/reports/conftest.py` configuration and two more. Reports rebuilt from
   `export_records.py` records must be byte-identical too.
4. The matrix, run once plain and once with `--decision-log` (W-160), every configuration
   `IDENTICAL`:
   - seeds 1, 2, 3;
   - pairs `red-green-zeon:blue-white-federation`, `purple-white-tekkadan:red-green-zeon` and
     `blue-white-federation:purple-white-tekkadan`;
   - formats `bo1` and `bo3`; presets `fast`, `standard` and `strong`; `--matches 10`;
   - Rust at `--workers 1`, `4` and `10`.

   That is 54 Python baselines and 162 Rust runs per pass. It exceeds the brief's minimum of
   3 seeds × 2 pairs × bo1/bo3 × both reference budgets.
5. Headline: red-green-zeon vs blue-white-federation, bo3, seed 1, reference `standard`,
   `--matches 100`, at workers 1, 4 and 10.
6. Every saved replay verifies in both directions. Rust `gcg-sim replay` checks every Python and
   every Rust replay produced above, and Python `.venv/bin/gcg-sim replay` checks every Rust
   replay.
7. The numbering-shift probe (§8.9) results are recorded in `docs/dev/EQUIVALENCE.md`.
8. Codegen independence: the tier-1 corpus and a seed-1 subset of the matrix are re-run with a
   `-C target-cpu=apple-m2` build (`--profile dist`) and must stay identical (§11.4 step 5).

Any configuration that cannot be made byte-identical for a root-caused reason is documented in
`docs/dev/EQUIVALENCE.md`. It falls back to statistical equivalence (≥ 400 games per
configuration, overlapping 95% Wilson intervals) and is flagged prominently in the final report.
Unexplained divergence is never accepted.

### 8.9 Numbering-shift probe

`oracle/offset_hook/sitecustomize.py` is activated by `PYTHONPATH=oracle/offset_hook`
and `GCG_ORACLE_EFFECT_OFFSET=<n>`. Spawned workers import it too. Before the registry is first
used, it pre-registers `n` distinct dummy continuous effects through `Registry.cont_key`, which
shifts every real `effect_key` by `n`. Content ids are unaffected. At the end of each game it
records the largest uid, and the run fails if that uid reaches `n`.

`scripts/compare_python_vs_rust.sh equivalence --offset-probe 1000` generates two extra Python
runs for every Phase 4 configuration and compares them with the offset-0 baseline:

- offset 1000 at `--workers 10`;
- offset 0 at `--workers 1`, which checks Python's own `--workers 1`/`10` identity.

The outcomes are read as follows:

- **All equal.** The configuration does not depend on the process-local numbering. Rust's typed
  semantics (a `Cont` element never equals a uid) equal Python's at every offset ≥ 1 + max uid.
- **Any difference.** Python's output depends on worker history for that configuration (§14 R1).
  The offset-1000 baseline becomes the comparison target; it equals Rust's semantics by
  construction, because every uid is below 1000. `docs/dev/EQUIVALENCE.md` documents the root
  cause with the first differing decision, found with `traces.py` at both offsets. The lead adds
  the finding to WEAKNESSES.md as a new item and flags it in the final report.

The `search` corpus set is also regenerated at offset 1000 and compared with offset 0.

## 9. Test strategy

### 9.1 What is ported and what is covered transitively

| Python tests | Rust coverage | Owner |
| --- | --- | --- |
| `tests/rules` (647), `tests/cards` (2,245), engine parts of `tests/effects` | Per-test traces (`tests` set) replayed by `gcg-diff`. Native ports of `tests/rules/test_lead_examples.py` and `tests/cards/test_lead_examples.py` (12 tests) validate the Rust testkit | G (traces), C (testkit, native ports) |
| golden (27 files) | Regenerated byte for byte from the IR (§3.4 test 3) | B1 |
| `test_card_coverage`, `test_rules_coverage`, `test_faq_ruling_coverage` | Stay in Python. Rust adds `gcg-diff coverage --check` and `traceability --check` (§9.2, §9.3) | G |
| `test_no_placeholders` | Rust lints over `crates/` (§9.9) | G |
| engine caches / paranoid (`test_engine_caches.py`) | Cargo feature `paranoid`: cached view == fresh compute, unlistened events have no candidates, skipped rules checks change nothing. It runs on the whole corpus, on 16 fuzz games at seed 11 and on 2 greedy games (`TUNING_DECKS`, seeds 1, 2 and +10; Inv 10 §10) | B2, C |
| fuzz / robustness (`tests/slow/test_robustness.py`, `tools/fuzz.py`) | `gcg-diff fuzz` reproduces the `fuzz` set's action streams and the seed-2026 10,000-game summary JSON byte for byte, with invariants after every action | C, G |
| invariants | Port of `engine/invariants.py:29-116`, run after every action in fuzz and trace replay | C |
| `tests/ai/test_baseline.py` | Native port asserting digest `5cff789d2a8bb26cfef4db98b1ef1ef8f37c3bf63210114781d357bd215f3b96` | D2 |
| puzzles (52) | Native ports on the Rust testkit at the reference presets, plus full decision logs compared with Python exports | D2 |
| information-set tests (128 + 25 + 28) | Native ports (`permute_hidden`, `information_set_key`) plus the 25 exported mid-game states with their expected decisions | D2, C |
| AI internals (`test_root_moves`, `test_policy_knobs`, `test_search_options`, `test_agents`) | `components` vectors compared bit for bit; negative-control assertions ported once the outputs match | D1, D2 |
| `test_fast_preset.py` | Native port for the reference presets (budget ratio and settings parity); W-18's strengthened version in Phase 6 | D2 |
| `test_tuning.py`, `test_evaluation_roles.py` | Ported; fitted weights compared as float bits | D1 |
| slow AI strength (2) | Transitive. Rust outcome lists for seeds 7001 and 7002 must equal the exported Python outcomes, and the Python bars are asserted too (feature `slow`) | F |
| `test_throughput.py` (3) | Not ported: timing tests are forbidden. Throughput is the external comparison | — |
| `tests/runner`, `tests/reports`, `tests/cli` | Ported as Rust tests (messages, exit codes, schema, round trips), plus byte comparisons of reports from exported records and transcripts. The interrupt test is ported without its timing assertions (D13) | E |
| `tests/deck` | Ported; exported triples compared | A |
| `tests/rules_index`, `tests/sources`, `tests/tools` | Stay Python-only (data pipeline) | — |
| Hypothesis tests (5) | Rust property tests (§9.6) plus a fixed sample exported from Python | owners of the code |

### 9.2 Traceability (`docs/RULES_TRACEABILITY.md` for Rust)

`gcg-diff traceability --write` regenerates `docs/RULES_TRACEABILITY.md` from four sources:

- the rules index (`src/gcg_sim/data/rules/rules_index.json`: 579 entries, 395 testable, 184 N/A
  with reasons from `rules_na.json`);
- the Python tags (`oracle/export_tags.py`);
- the Rust JUnit of the per-test trace replay (`gcg-diff corpus --junit`, keyed by Python node id);
- native Rust tests tagged with a comment line directly above `#[test]`:
  `// gcg-tags: rule=8-5-2 card=GD01-001 faq=Q12 ruling=GD01-001:Q100`.

Each rule row keeps the Python columns and adds a "Rust" column. It lists native tests, plus
"trace `<test id>`" for every passing trace. A rule counts as tested for Rust if and only if it
has at least one native test or passing trace. `--check` is part of the gate. It fails when the
file differs from a regeneration, or when a testable rule has no passing Rust evidence.

The same generator writes the card (1152), FAQ (119) and ruling (368, 8 N/A) tables to
`docs/dev/TRACEABILITY_CARDS.md`. The Python generator (`gcg_sim.tools.traceability`) would drop
the Rust column, so Phase 8 updates CLAUDE.md to name the Rust generator, and the check tells
anyone who ran the Python one to rerun it.

### 9.3 Card coverage test (1152/1152)

- **Load time.** The IR has a script for all 1153 definitions, and an unknown hook or node fails
  the load (`crates/gcg-effects/tests/coverage.rs`). `data status` coverage equals Python's
  (`cli.py:236-248`), because it reads the IR's `source`, which the port templates of §12.11
  never change; `gcg-eval data-status --templates` reports the template coverage separately. Any
  unimplemented card is a hard failure, as in Python.
- **Behaviour.** `gcg-diff coverage --check` requires each of the 840 non-vanilla real cards to
  have either a passing per-test trace tagged with its number, or a corpus game in which one of
  its programs ran, according to the census re-measured by the Rust replay. Each of the 312
  vanilla real numbers must have been instantiated in some traced state. Tokens and EX cards count
  when an effect or setup creates them; vanilla tokens that nothing creates count through the
  load-time check and the DB fingerprints. The report lists any card failing both, and the gate
  fails on any.

### 9.4 Fuzz and invariants

`gcg-diff fuzz` ports `tools/fuzz.py:28-166`, with the same decks, seeds, agent and summary JSON.
That includes `deck_for`'s RNG use, colours sorted by value, the `Counter`-ordered `ends`, errors
as `ok=false` records, and the turn-limit end counted as a failure. Invariants run after every
action. W-151 adds the executed-program census. W-110 adds 1,000 strategic games over the same
generated decks, as its verification asks: 400 with `--agent greedy`, 400 with
`--agent search:fast` and 200 mixed (`--agent mixed:greedy,search:fast`), all with invariants and
the `paranoid` checks; `--agent ref:fast` is available for the reference. `--paranoid` uses the
`paranoid` build.

`scripts/verify_all_rust.sh` (§9.11) runs every fuzz and paranoid check:

- 10,000 games at seed 2026, compared with the exported Python summary (T-055);
- 2,000 games at a commit-derived seed (`derive_seed` of the HEAD sha, printed) that must pass
  invariants (T-056);
- the 1,000 W-110 strategic games in the `paranoid` build (T-056);
- `gcg-diff paranoid-games` in the `paranoid` build (T-054), and the `gcg-engine` tests with the
  `paranoid` and `testkit` features.

### 9.5 Schema validation

`gcg-sim benchmark` and `gcg-eval reference-benchmark` validate `results.json` before writing any
file, as Python does (`reports/writer.py:94-110`, `reports/results.py:48-50`). So every
successful run is a schema check. The validator is an in-house subset that supports exactly the
keywords the two schema files use. A test enumerates every keyword and every `pattern` in both
schemas and fails on any unsupported one, so the subset is provably sufficient.
`gcg-diff schema-check` re-validates written files. With `--expect-invalid` it runs negative
controls: documents in `crates/gcg-runner/tests/vectors/invalid-results/` that each violate one
keyword the schema uses.

### 9.6 Property tests

`gcg_core::prop` is an in-house harness of about 100 lines. Cases come from
`SplitMix64(derive_seed(TEST_SEED, name, i))`, with no shrinking; a failure prints the case index
and seed. Properties:

- RNG and `derive_seed` against the Python vectors (exists);
- float `repr`/`round`/format round trips; `py_sum` against an exact rational oracle;
- `ZoneLists` and the frame arenas against a naive `Vec<Vec<_>>` model;
- `clone_from_state(x) == clone(x)`, with equal digests;
- `from_canonical(canonical(s))` re-serialises identically;
- `determinize` is pure in `(st, observer, seed)` and keeps known cards and card multisets;
- `permute_hidden` invariance of reference decisions on the 25 exported mid-game states;
- NFKC + casefold + whitespace splitting against the exported random strings;
- small runs identical across 1, 4 and 10 workers for every agent kind;
- the five Hypothesis properties, plus a fixed sample exported from Python.

### 9.7 AI-strength tests

- **Reference** (feature `slow`). Reference `standard` vs random (400 games, seed 7001) and vs
  greedy (400 games, seed 7002), with the decks and seeds of `tests/slow/test_ai_strength.py`.
  The test asserts the Python bars (400/400; Wilson lower bound above 0.5 against greedy) **and**
  outcome-list equality with the exported Python outcomes.
- **Strengthened** (feature `slow`, Phase 6). The gates of §12.6 run on eval-role seeds. The full
  head-to-head evaluation is a recorded protocol (`gcg-eval gates --check`), not a unit test, and
  the reviewer reruns it.

### 9.8 Scenario tests

`crates/gcg-search/tests/scenarios/<w-id>_<name>.rs` holds one test per scenario-shaped weakness
(§12.2), built with the Rust testkit. Each test fixes the decklists and the true hidden cards and
runs agent seeds 1..N (N = 16 unless the entry says otherwise). It runs both the reference preset
named in WEAKNESSES.md and the strengthened preset. It asserts the reference's recorded solve
count, which must miss the bar for the scenario to count as a reproducer, and it pins that count
so a change in the reference is caught. It also asserts the strengthened bar.

### 9.9 Lints, clippy configuration and compile-fail checks

`clippy.toml` (workspace root):

- `disallowed-methods`: `f64::max`, `f64::min`, `f64::powi`, `f64::powf` (allowed only inside
  `gcg_core::libm`), `f64::mul_add`, `Iterator::max_by`, `Iterator::max_by_key`,
  `Iterator::min_by`, `Iterator::min_by_key`, `slice::sort_unstable*`,
  `std::time::Instant::now`, `std::time::SystemTime::now`, `std::thread::sleep`.
- `disallowed-types`: `std::collections::HashMap` and `HashSet`. A lookup-only
  `gcg_core::lookup::LookupMap` wraps `hashbrown` with a fixed `foldhash` state and has no
  iteration methods.
- Allowed exceptions carry `#[expect(clippy::disallowed_methods, reason = "…")]`. The only
  wall-clock exception is `crates/gcg-runner/src/clock.rs`, which only formats values into
  `timing.json`.

`gcg-diff lints` (also run as `crates/gcg-diff/tests/lints.rs`) scans `crates/**/*.rs`,
including tests, and fails on:

1. `todo!`, `unimplemented!`, `TODO`, `FIXME`, `XXX`, `HACK`, `#[ignore]`, and `should_panic`
   used to hide a failure (mirrors `tests/test_no_placeholders.py`);
2. outside `clock.rs`: any path into `std::time` or `core::time` (including `use` lists that
   import `Duration`, `Instant` or `SystemTime` from them), the identifiers `Instant` and
   `SystemTime`, `thread::sleep`, a `.elapsed(` method call, and any timeout argument
   (`recv_timeout`, `wait_timeout`, `park_timeout`). The engine's lasting-effect enum `Duration`
   (`engine/types.py:142-147`) and a local such as the port of `elapsed` in `_leaf_value`
   (`ai/mcts.py:259-262`) are allowed, because the lint matches paths and method calls, not bare
   identifiers. A lint self-test fixture (`crates/gcg-diff/tests/lint_fixtures/`) has both
   allowed uses and every banned form, and the lint must flag exactly the banned ones;
3. `rand`, `getrandom`, `[[bench]]`, `#[bench]`, `criterion`, and `sum::<f64>` (use `py_sum`);
4. `unsafe` outside `crates/gcg-core/src/libm.rs` (the `erfc` FFI), and any `unsafe` block
   without a `// SAFETY:` comment. A Phase 5 `unsafe` block that a profile justifies is added to
   the allowlist with its evidence (§11.4 step 11). The `sigaction` fallback of A3 is added only
   with the user's sign-off;
5. in `crates/gcg-search/src/`, only `agent.rs` may implement `Agent`, and its `choose` may use
   its true-state parameter only in calls to `AgentCore::begin`, `AgentCore::finish` and
   `Observation::new` (W-135, §12.3);
6. any reference to the untracked bytecode directories (`bench/`, `tests/bench`, `tests/perf`)
   in `crates/**`, `Cargo.toml`, `.cargo/config.toml` and the Rust gate scripts
   (`scripts/verify_all_rust.sh`, `scripts/verify_binary_offline.sh`, `scripts/ci_local.sh`,
   `scripts/compare_python_vs_rust.sh`) (W-122). A bare `__pycache__` is not such a reference:
   the reference's `code_digest` walk skips every directory of that name and its Rust ports must
   name it (lead decision at M3). The lint assembles these patterns from pieces, so its own
   source does not match them.

Compile-fail checks are rustdoc ```` ```compile_fail ```` doctests, run by
`cargo test --doc`:

- W-135: an `Agent` cannot mutate `&GameState`, and the strengthened search's `Observation`
  can neither reach hidden cards nor return the true `&GameState` it wraps;
- W-136: `StrengthenedConfig` cannot be built without every field;
- W-49: an outside crate cannot write a `GameState` field.

### 9.10 `docs/dev/tests.json`

The file is a JSON array of objects with exactly three keys: `{"id": "T-001", "name": "…",
"status": "passing" | "failing" | "not-started"}`. Ids are stable and never reused.

`gcg-diff tests-json --check` validates the file:

- it is an array, with unique `T-NNN` ids, exactly the three keys and allowed statuses;
- its id list equals Appendix A of this plan.

The lead updates statuses at each milestone from gate outputs recorded in
`docs/port/progress.md`. An entry becomes `passing` only with a recorded command and its output
in the same commit. It becomes `failing` when its gate ran and failed. Appendix A maps every id to
its milestone and its command.

### 9.11 `scripts/verify_all_rust.sh`

It mirrors `scripts/verify_all.sh`: a clean clone of the committed HEAD into a temporary
directory, then every gate, ending with the offline check of the distributable binary (the
counterpart of `scripts/verify_wheel_offline.sh`). `--full` adds the tier-2 corpus regeneration
and replay (needs `uv` and several hours); the Phase 4 and Phase 7 gates use it. Lines marked
`(from Mx)` join the script in the milestone that delivers their command, so the script passes on
a clean clone at every milestone.

```text
#!/usr/bin/env bash
# Full Rust gate on a clean clone of the committed HEAD. Mirrors scripts/verify_all.sh.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
branch="$(git -C "$repo" rev-parse --abbrev-ref HEAD)"
full=0; [[ "${1:-}" == "--full" ]] && full=1
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
git clone --quiet --no-hardlinks --branch "$branch" "$repo" "$work/repo"
cd "$work/repo"
echo "verifying $(git rev-parse --short HEAD) on $branch in $work/repo"
# SLOW = every <pkg>/slow, DIAG = every <pkg>/paranoid and <pkg>/testkit, read from cargo metadata.
# The helper exits 1 if any [[test]] or [[bin]] target requires a feature outside slow, paranoid
# and testkit, so no test target can stay unbuilt.
eval "$(cargo metadata --format-version 1 --no-deps --offline | python3 scripts/cargo_test_features.py)"
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --locked --offline --features "$SLOW,$DIAG" \
  --target-dir target/paranoid -- -D warnings
cargo build --release --locked --offline -p gcg-cli -p gcg-eval -p gcg-diff
cargo build --release --locked --offline -p gcg-diff --features paranoid --target-dir target/paranoid
cargo test --workspace --release --locked --offline
cargo test --workspace --release --locked --offline --features "$SLOW"
cargo test --release --locked --offline -p gcg-engine --features paranoid,testkit --target-dir target/paranoid
target/release/gcg-diff lints
target/release/gcg-diff selftest
target/release/gcg-diff corpus --tier 1 --require-all
target/paranoid/release/gcg-diff corpus --tier 1 --require-all --paranoid
target/release/gcg-diff coverage --check
target/release/gcg-diff traceability --check
target/release/gcg-diff tests-json --check
target/release/gcg-diff weaknesses --check docs/dev/WEAKNESSES.md
target/release/gcg-diff fuzz --games 10000 --seed 2026 --workers auto \
  --compare oracle/corpus/v1/tier1/fuzz/summary-2026-10000.json
target/release/gcg-diff fuzz --games 2000 --seed-from-commit "$(git rev-parse HEAD)" --workers auto
target/paranoid/release/gcg-diff paranoid-games
target/paranoid/release/gcg-diff fuzz --games 400 --seed 2026 --agent greedy --paranoid --workers auto
target/paranoid/release/gcg-diff fuzz --games 400 --seed 2026 --agent search:fast --paranoid --workers auto
target/paranoid/release/gcg-diff fuzz --games 200 --seed 2026 --agent mixed:greedy,search:fast \
  --paranoid --workers auto
if cargo tree -p gcg-cli -e normal --offline | grep -q gcg-ai-reference; then
  echo "gcg-cli depends on the reference search" >&2; exit 1; fi
if cargo tree -p gcg-cli -e features --offline | grep -q paranoid; then
  echo "gcg-sim would be built with the paranoid feature" >&2; exit 1; fi
for d in examples/decks/*.txt examples/decks/eval/*.txt; do target/release/gcg-sim validate "$d" >/dev/null; done
for w in 1 4 10; do
  target/release/gcg-sim benchmark examples/decks/red-green-zeon.txt \
    examples/decks/blue-white-federation.txt --matches 4 --seed 7 --ai-preset fast \
    --workers "$w" --decision-log --out "build/w$w" >/dev/null
  target/release/gcg-eval reference-benchmark examples/decks/red-green-zeon.txt \
    examples/decks/blue-white-federation.txt --matches 4 --seed 7 --reference-preset fast \
    --workers "$w" --decision-log --out "build/ref-w$w" >/dev/null
  target/release/gcg-eval head-to-head search:fast ref:fast --decks examples --games 24 \
    --seed 20000001 --role dev --workers "$w" --out "build/h2h-w$w" >/dev/null
done
for kind in w ref-w; do
  python3 scripts/normalize_outputs.py "build/${kind}1" "build/${kind}1.norm"
  for w in 4 10; do python3 scripts/normalize_outputs.py "build/$kind$w" "build/$kind$w.norm"
    diff -r "build/${kind}1.norm" "build/$kind$w.norm"; done
done
for w in 4 10; do diff -r build/h2h-w1 "build/h2h-w$w"; done
target/release/gcg-eval benchmark examples/decks/red-green-zeon.txt \
  examples/decks/blue-white-federation.txt --matches 4 --seed 7 --ai-preset fast \
  --workers 4 --decision-log --out build/eval-bench >/dev/null                      # (from M12.M)
diff -r -x timing.json build/w4 build/eval-bench                                     # (from M12.M) A30
for f in build/w1/replays/*.json build/ref-w1/replays/*.json; do target/release/gcg-sim replay "$f" >/dev/null; done
target/release/gcg-diff schema-check build/w1/results.json build/ref-w1/results.json
target/release/gcg-diff schema-check --expect-invalid crates/gcg-runner/tests/vectors/invalid-results/*.json
for p in strong STANDARD Fast bogus; do
  set +e
  target/release/gcg-sim benchmark examples/decks/red-green-zeon.txt \
    examples/decks/blue-white-federation.txt --matches 1 --ai-preset "$p" --out build/bad >/dev/null 2>&1
  rc=$?; set -e
  [[ $rc == 2 ]] || { echo "--ai-preset $p: exit $rc, expected 2" >&2; exit 1; }
done
RUSTFLAGS="-C target-cpu=apple-m2" cargo build --profile dist --locked --offline -p gcg-cli
scripts/verify_binary_offline.sh target/dist/gcg-sim
if [[ $full == 1 ]]; then
  uv sync --locked
  uv run python oracle/export_ir.py --check
  uv run python oracle/traces.py --check --tier 1
  uv run python oracle/traces.py --write --tier 2
  target/release/gcg-diff corpus --tier 2 --require-all
  uv run pytest oracle/tests -q                                                 # (from M12.K)
fi
if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
  echo "the gate wrote files into the tree:" >&2; git status --short >&2; exit 1; fi
echo "verify_all_rust passed"
```

`python3` there is the system interpreter running stdlib-only helpers (the normaliser and
`scripts/cargo_test_features.py`), not the package.

`scripts/verify_binary_offline.sh BINARY` is the Rust counterpart of
`scripts/verify_wheel_offline.sh`. It copies the `apple-m2` `--profile dist` binary and
`examples/decks/*.txt` into a fresh temporary directory outside the repository. It then runs, under
`sandbox-exec -p '(version 1)(allow default)(deny network*)'` and with `env -i HOME="$tmp"
PATH="$tmp/empty"` (so no `python`, `python3` or `uv` can be found): `data status`, `validate`,
`benchmark` of the example decks at `--workers 1` and `--workers 2` with `cmp` of the two
`results.json`, and `replay` of a saved replay. The binary reads only embedded data and calls
`/usr/sbin/sysctl` by absolute path (A4), so an empty `PATH` is enough. The script fails if any
command fails or the two `results.json` differ (T-145).

## 10. Workstreams (Phase 2 and Phase 3)

### 10.1 Split, and why it differs from the brief

The brief names six workstreams: A data, B effects, C engine, D AI, E runner/reports/CLI and
F eval. The code argues for four adjustments. Each keeps one writer per file.

- **B splits into B1 and B2.** B1 owns the static IR, the registry and `repr` (`gcg-effects`).
  B2 owns the view, the interpreter and the hooks (in `gcg-engine`). B1 is data only and can be
  verified completely against Python before any engine exists (content ids, golden files). B2 is
  entangled with the engine core.
- **The engine lands in five layers along Python's import DAG.** The order is state → view →
  core → battle → interp → game, with observe and invariants after view and core. We checked the
  imports at the top of each `engine/*.py`; `interp.py` imports `battle` lazily inside a
  function. C and B2 own alternate layers, and each writes its next layer against the frozen
  signatures while the other's layer lands:

  | Layer | Owner | Files | Python lines |
  | --- | --- | --- | --- |
  | L1 | C | `types.rs`, `state.rs`, `zones.rs`, `canon.rs`, `error.rs` | `engine/state.py` 651, `engine/types.py`, `runner/digest.py` |
  | L2 | B2 | `view/**` (derived view, queries, expression evaluation, 21 pure hooks) | `engine/view.py` 958 + the cond/filter/value hooks |
  | L3 | C | `core.rs`, `battle.rs` | `engine/core.py` 946, `engine/battle.py` 282 |
  | L4 | B2 | `interp/**` (frames, handlers, resumers, decisions, 13 step hooks, system programs) | `engine/interp.py` 1,660 + `effects/builtins.py` + step hooks |
  | L5 | C | `game.rs`, `observe.rs`, `invariants.rs`, `testkit/**`, `tests/**` | `engine/game.py` 965, `observe.py` 150, `invariants.py` 116, `testkit.py` |

- **D splits into D1 and D2.** D1 owns the policy, values, targeting, evaluation, playouts,
  greedy and random. D2 owns the agent core and decision log, move keys/pruning/merging, the
  no-effect check, pre-game choices, the reference ISMCTS and the `gcg-search` v0 copy. The two
  halves meet at a small interface (`option_scores`, `choose`, `play_until_turn`, `evaluate`),
  and each is about 1,200 lines of Python.
- **G is added** for the oracle and the differential runner. Phase 2 must exist before engine
  code, and the trace exporters plus `gcg-diff` are a self-contained unit.

| WS | Scope | Owns (exclusive writer) | Needs | Starts | First green gate |
| --- | --- | --- | --- | --- | --- |
| S | contracts (lead) | root `Cargo.toml`, every crate's `Cargo.toml`, `lib.rs`/`main.rs` module lists, `crates/*/src/contracts.rs`, `clippy.toml`, `.cargo/config.toml`, `docs/dev/contracts.md`, `gcg_core::{small, portable, stable_hash, prop, lookup}` | — | M2 | `cargo check --workspace` |
| A | data | `crates/gcg-data/**`, `oracle/export_data.py`, `oracle/export_unicode.py` | gcg-core | M2 | M4 |
| B1 | IR and registry | `crates/gcg-effects/**`, `oracle/export_ir.py` | gcg-core, gcg-data types | M2 | M2 |
| G | oracle and diff | `oracle/{traces,pytest_trace,coverage_decks,census,export_tags}.py`, `oracle/offset_hook/**`, `oracle/corpus/**`, `crates/gcg-diff/**`, `scripts/verify_all_rust.sh`, `scripts/verify_binary_offline.sh`, `scripts/cargo_test_features.py`; with the lead's agreement (the lead wrote them) `scripts/normalize_outputs.py` and `scripts/compare_python_vs_rust.sh`: the W-160 `equivalence` changes and the timing-side fixes of §11.5 (`need_rust`, best-count recording, role-paired `report` with the 2x check, `part2 --candidate`) | engine API (§10.3.3), runner API (§10.3.6) | M2 | M3 |
| C | engine core, layers L1, L3, L5 | `crates/gcg-engine/src/{types,state,zones,canon,error,core,battle,game,observe,invariants}.rs`, `src/procedures.rs`, `src/testkit/**`, `crates/gcg-engine/tests/**` | §10.3.2-§10.3.4 | M3 | M5.1 |
| B2 | view, interpreter, hooks, layers L2, L4 | `crates/gcg-engine/src/view/**`, `src/interp/**` | same | M3 | M5.2 |
| D1 | policy and evaluation | `crates/gcg-ai/src/{policy,values,targeting,evaluation,weights,playout,greedy,random}.rs`, `oracle/export_ai_vectors.py` (policy/evaluation part) | engine API, D2's `actions` | M3 (pure parts) | M6 |
| D2 | search and agents | `crates/gcg-ai/src/{agent,log,actions,fingerprint,noop,setup,scratch}.rs`, `crates/gcg-ai-reference/**`, `crates/gcg-search/**` (v0 copy), `export_ai_vectors.py` (search part) | engine API, D1's interface | M3 (pure parts) | M6 |
| E | runner, reports, CLI, pool, OS glue | `crates/gcg-runner/**`, `crates/gcg-cli/**`, `build-support/**` (repo root, X19), `oracle/export_records.py`, `gcg_core::{pool, sys}` | engine API, `Agent` trait | M3 (reports from exported records) | M7 |
| F | eval harness | `crates/gcg-eval/**`, `oracle/export_decks.py`, `oracle/export_stats.py` | runner API, `Agent`, reference and search factories | M3 (statistics) | M8 |

### 10.2 Contracts before parallel work (S)

The lead commits one contracts milestone (part of M2). It compiles and contains:

1. All eleven crates, with manifests, the dependency edges of §2.2 and module lists that name
   only files that exist.
2. Every public **type and trait** of §4.2, §4.6, §5.1, §5.3, §6.1, §7.1 and §10.3, with fields,
   derives and doc comments citing the Python lines. They live in lead-owned
   `crates/<crate>/src/contracts.rs` files, re-exported from `lib.rs`. A type definition is real
   code, not a placeholder.
3. `docs/dev/contracts.md`: every frozen **function signature** of §10.3, its owner, its
   Python `path:line`, and the change protocol (§10.6).

No function without a body is ever committed: no `todo!`, no `unimplemented!`, no stub (lint
§9.9; with `panic = "abort"` a stub could not be caught anyway). A provider commits its
functions with real bodies when its gate passes. Until a provider lands, a consumer works on the
parts that do not depend on it. If it needs to compile earlier, it may write a throwaway shim of
the frozen signatures in its own worktree. Shims are never handed back. The lead's integration
check rejects any diff outside the workstream's "Owns" list and any file matching
`*shim*`.

### 10.3 Frozen interfaces

Signatures are Rust 2024. Types not spelled out are defined in §4-§7.

#### 10.3.1 `gcg-data` (A)

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)] pub struct DefId(pub u16);
pub enum CardType { Unit, Pilot, Command, Base, Resource, UnitToken, ExBase, ExResource } // cards/model.py:10-34
pub enum Color { Blue, Green, Red, White, Purple }                                         // cards/model.py:40-45
pub struct LinkCondition { pub name_parts: Box<[Box<str>]>, pub traits: Box<[Box<str>]> }
pub struct CardDef {                                   // cards/model.py:92-141, same fields and order
    pub def_id: DefId, pub card_number: Box<str>, pub name: Box<str>, pub card_type: CardType,
    pub color: Option<Color>, pub level: i32, pub cost: i32, pub ap: i32, pub hp: i32,
    pub zones: Box<[Box<str>]>, pub traits: Box<[Box<str>]>, pub link: Option<LinkCondition>,
    pub effect: Box<str>, pub pilot_name: Option<Box<str>>, pub set_code: Box<str>,
    pub rarity: Box<str>, pub product_ids: Box<[Box<str>]>, pub canonical_product_id: Box<str>,
    pub source_title: Option<Box<str>>, pub extra_names: Box<[Box<str>]>,
}
impl CardDef { pub fn names(&self) -> &[Box<str>]; pub fn is_token(&self) -> bool;
    pub fn is_unit_card(&self) -> bool; pub fn is_pilot_capable(&self) -> bool; pub fn is_vanilla(&self) -> bool; }
pub struct CardDb { /* defs in def_id order (cards/db.py:119-158) */ }
impl CardDb {
    pub fn embedded() -> &'static CardDb;
    pub fn from_sources(cards_ndjson: &str, overrides: &str) -> Result<CardDb, DataError>; // W-157: no empty default
    pub fn len(&self) -> usize;
    pub fn by_id(&self, id: DefId) -> &CardDef;
    pub fn get(&self, number_or_product_id: &str) -> Option<&CardDef>;
    pub fn normalize_id(&self, s: &str) -> Option<&str>;
    pub fn real_cards(&self) -> impl Iterator<Item = &CardDef>;
    pub fn unit_variant(&self, id: DefId) -> Option<&CardDef>;
    pub fn base_def_id(&self, id: DefId) -> DefId;
    pub fn token_for_key(&self, key: &str) -> Option<&CardDef>;
    pub fn ex_base(&self) -> &CardDef;
    pub fn ex_resource(&self) -> &CardDef;
    pub fn applied_overrides(&self) -> &[AppliedOverride];
    pub fn fingerprints(&self) -> [String; 4];        // Inv 03 §9
}
/// Keeps the caller's order: new_game maps `main` in this order, then shuffles and cuts
/// (game.py:134-140), and maps `resources` in order to uids, so the order is RNG-relevant. Only
/// to_decklist(parse_deck(..)) yields sorted card numbers (deck/parse.py:104); fuzz, coverage
/// and testkit decks are unsorted (tools/fuzz.py:57-70). No constructor sorts or canonicalises.
/// A unit test: one multiset in two orders gives different shuffles, equal to Python vectors.
pub struct DeckList { pub main: Box<[DefId]>, pub resources: Box<[DefId]> }
pub struct Deck { pub name: Box<str>, pub source: Option<Box<str>>, pub main: Box<[Box<str>]>,
                  pub resources: Box<[Box<str>]>, pub digest: String, /* entries */ }
pub struct Violation { pub code: ViolationCode, pub message: String }       // 13 codes, deck/model.py:32-58
pub enum DeckError { Io { path: PathBuf, error: io::Error }, Decode { reason: &'static str, start: usize }, Invalid(Vec<Violation>) }
impl fmt::Display for DeckError { /* Python's text, deck/parse.py */ }
pub fn load_deck(path: &Path) -> Result<Deck, DeckError>;               // deck/parse.py:111-114
pub fn parse_deck(text: &str, name: &str, source: Option<&str>) -> Result<Deck, DeckError>;
pub fn to_decklist(deck: &Deck) -> DeckList;
pub fn validate(deck: &Deck) -> Vec<Violation>;                          // deck/validate.py, Python order
pub mod official { pub fn construction_rules() -> &'static ConstructionRules; pub fn banlist() -> &'static Banlist;
                   pub fn rules_version() -> &'static RulesVersion; pub fn bo3_rules() -> &'static Bo3Rules; }
pub fn versions(package: &str, code_digest: &str) -> Json;               // reports/versions.py:55-75
pub fn conflict_resolutions(decks: &[&Deck]) -> Json;                    // reports/versions.py:93-135
pub fn data_status_parts() -> DataStatusParts;                           // cli.py:263-291 without coverage
pub mod names { pub fn normalize_name(s: &str) -> String;                // deck/names.py:29-33
                pub fn names_match(written: &str, names: &[Box<str>]) -> bool; } // :36-38
```

#### 10.3.2 `gcg-effects` (B1)

```rust
pub const SUPPORTED_IR_VERSION: u32 = 1;
// Ord on ids is arbitrary but total; it is used only to sort for equality tests (§4.2 ContRef).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)] pub struct NodeId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)] pub struct ProgramId(pub u16);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)] pub struct AbilityId(pub u16);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)] pub struct StrId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct VarId(pub u16);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct PromptId(pub u16);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)] pub struct FilterSetId(pub u16);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct ContentId(pub [u8; 16]); // 16 hex chars
pub enum ValueRef { Lit(i64), Node(NodeId) }
pub enum Node { /* one variant per DSL/program class, generated by dsl_nodes! */ }
pub struct Program { pub label: StrId, pub content_id: ContentId, pub instrs: Box<[NodeId]> }
pub struct AbilityEntry { pub aid: AbilityId, pub card_number: StrId, pub def_id: DefId, pub index: u16,
    pub unit_text: bool, pub ability: NodeId, pub program: Option<ProgramId>,
    pub cost_program: Option<ProgramId>, pub trigger_event: Option<Ev> }
pub struct CardEntry { pub def_id: DefId, pub source: ScriptSource, pub binding_module: Option<StrId>,
    pub script: Option<NodeId>, pub own: Box<[AbilityId]>, pub unit: Box<[AbilityId]>,
    pub command_aid: Option<AbilityId>, pub burst_aid: Option<AbilityId>, pub has_burst: bool,
    pub timings: Box<[StrId]>, pub field_entries: Box<[AbilityId]>, pub hand_entries: Box<[AbilityId]>,
    pub trash_entries: Box<[AbilityId]>, pub paired_entries: Box<[AbilityId]>,
    pub unit_entries: Box<[AbilityId]>, pub gated: bool, pub events: EvSet,
    pub printed_keywords: KwAmounts, pub delayed: Box<[(u16, NodeId, ProgramId)]>,  // §3.3
    pub template: Option<StrId> }                                     // §12.11; never read by the engine
pub struct Registry { /* nodes, programs, abilities, cards, aliases, always_events,
                         hp_reduction_possible, keyword/system programs, tables, generation */ }
impl Registry {
    pub fn embedded() -> Arc<Registry>;
    pub fn from_ir(ir_json: &str, db: &'static CardDb) -> Result<Registry, IrError>;
    pub fn with_fragment(&self, fragment: &serde_json::Value) -> Result<Registry, IrError>;
    pub fn node(&self, id: NodeId) -> &Node;
    pub fn program(&self, id: ProgramId) -> &Program;
    pub fn ability(&self, aid: AbilityId) -> &AbilityEntry;
    pub fn card(&self, def: DefId) -> &CardEntry;
    pub fn str(&self, id: StrId) -> &str;
    pub fn var_name(&self, v: VarId) -> &str;
    pub fn prompt(&self, p: PromptId) -> &str;
    pub fn keyword_program(&self, kw: KeywordProgram) -> ProgramId;
    pub fn system_program(&self, s: SystemProgram) -> ProgramId;
    pub fn aliases(&self, def: DefId) -> &[StrId];
    pub fn always_events(&self) -> EvSet;
    pub fn events_of(&self, def: DefId) -> EvSet;
    pub fn hp_reduction_possible(&self) -> bool;
    pub fn cont_of(&self, n: ContinuousId) -> ContRef;               // canonical ContRef of an IR Continuous node
    pub fn cont_node(&self, c: ContRef) -> ContView<'_>;              // typed read access for _apply_cont
    pub fn program_by_content_id(&self, cid: &ContentId) -> Option<ProgramId>;
    pub fn cont_by_content_id(&self, cid: &ContentId) -> Option<ContRef>;
    pub fn filter_set_by_content_id(&self, cid: &ContentId) -> Option<FilterSetId>;
    pub fn node_content_id(&self, id: NodeId) -> ContentId;
    pub fn filter_set_content_id(&self, f: FilterSetId) -> ContentId;
    pub fn rule_mod_repr(&self, rule: NodeId) -> &str;               // ai/actions.py:360 sort key
    pub fn repr(&self, id: NodeId) -> String;
    pub fn golden_json(&self, def: DefId) -> Json;
    pub fn generation(&self) -> u32;                                  // changes on with_fragment (W-33)
}
impl ContRef { pub fn repr(&self, reg: &Registry) -> String; pub fn content_id(&self, reg: &Registry) -> ContentId; }
```

#### 10.3.3 `gcg-engine` public API (C and B2; consumed by D, E, F, G)

```rust
pub struct NewGameOptions { pub chooser: Option<u8>, pub turn_limit: i32, pub max_actions: i64 }
pub fn new_game(reg: &Arc<Registry>, decks: [&DeckList; 2], seed: u64, opts: NewGameOptions)
    -> Result<GameState, DeckConstructionError>;                             // game.py:107-158 (C, L5)
pub fn apply(st: &mut GameState, action: Action, check: bool) -> Result<(), ApplyError>; // game.py:780-818
pub fn legal_actions(st: &GameState) -> &[Action];                           // game.py:774-777
pub fn concede(st: &mut GameState, player: u8) -> Result<(), EngineError>;   // game.py:763-771
pub fn step(st: &GameState, a: Action) -> Result<GameState, ApplyError>;     // game.py:961-965
pub enum ApplyError { Illegal(IllegalActionError), Engine(EngineError) }
pub enum EngineError { /* each variant carries Python's class name and message: EngineError, ValueError,
                          UnimplementedCardError, RecursionError (W-143), Overflow, Capacity, NoProgress (W-47) */ }
impl GameState {                                                             // C, L1: read-only accessors
    pub fn pending(&self) -> Option<DecisionView<'_>>; pub fn card(&self, uid: Uid) -> &CardInstance;
    pub fn cards(&self) -> &[CardInstance]; pub fn zone(&self, p: u8, z: Zone) -> &[Uid];
    pub fn turn(&self) -> i32; pub fn active(&self) -> u8; pub fn first_player(&self) -> u8;
    pub fn phase(&self) -> Phase; pub fn step(&self) -> Step; pub fn winner(&self) -> Option<i8>;
    pub fn end_reason(&self) -> Option<EndReason>; pub fn battle(&self) -> Option<&Battle>;
    pub fn battles(&self) -> &[Battle]; pub fn frames(&self) -> &[Frame];
    pub fn frame_vars(&self, f: &Frame) -> FrameVars<'_>; pub fn frame_buf(&self, f: &Frame) -> &[i32];
    pub fn pending_triggers(&self) -> &[TriggerInst]; pub fn batches(&self) -> impl Iterator<Item = &[TriggerInst]>;
    pub fn lasting(&self) -> &[Lasting]; pub fn delayed(&self) -> &[Lasting];
    pub fn lasting_targets(&self, l: &Lasting) -> &[(Uid, i32)];
    pub fn once_used(&self) -> &[OnceKey]; pub fn history(&self) -> &[HistoryEvent];
    pub fn history_epoch(&self) -> u32; pub fn decklists(&self) -> [&[DefId]; 2];
    pub fn resource_decklists(&self) -> [&[DefId]; 2]; pub fn redraws(&self) -> [bool; 2];
    pub fn version(&self) -> u64; pub fn action_count(&self) -> i64; pub fn event_group(&self) -> i64;
    pub fn rng_state(&self) -> u64; pub fn rules_checked(&self) -> i64;
    pub fn registry(&self) -> &Arc<Registry>; pub fn db(&self) -> &'static CardDb;
    pub fn def_of(&self, uid: Uid) -> &CardDef;
    pub fn clone_from_state(&mut self, src: &GameState);                     // §4.5
}
pub mod view {          // B2, L2; view.py:223-239 and :490-570
    pub fn derived(st: &GameState) -> Arc<Derived>;                         // fills st's cache (§4.6)
    pub fn ap_of(st: &GameState, dv: &Derived, uid: Uid) -> i32;
    pub fn hp_of(st: &GameState, dv: &Derived, uid: Uid) -> i32;
    pub fn level_of(st: &GameState, uid: Uid) -> i32;
    pub fn cost_of(st: &GameState, uid: Uid) -> i32;
    pub fn play_cost(st: &GameState, dv: &Derived, uid: Uid) -> Result<i32, EngineError>; // calls player_cost_mod → derived(st)
    pub fn play_level(st: &GameState, dv: &Derived, uid: Uid) -> i32;
    pub fn keywords(dv: &Derived, uid: Uid) -> KwAmounts;
    pub fn has_kw(dv: &Derived, uid: Uid, kw: Kw) -> bool;
    pub fn kw_amount(dv: &Derived, uid: Uid, kw: Kw) -> i32;
    pub fn traits_of<'a>(st: &'a GameState, dv: &'a Derived, uid: Uid) -> Traits<'a>;
    pub fn rules_of<'a>(dv: &'a Derived, uid: Uid, kind: RuleKind) -> impl Iterator<Item = &'a RuleInst>;
    pub fn is_linked(dv: &Derived, uid: Uid) -> bool;
    pub fn abilities_of<'a>(dv: &'a Derived, uid: Uid) -> &'a [AbilityId];
    pub fn trigger_index<'a>(st: &GameState, dv: &'a Derived) -> &'a TriggerIndex;  // fills dv.by_event once
    pub fn link_satisfied(st: &GameState, unit: &CardDef, pilot: &CardDef) -> bool; // view.py:125; st only for test force_link
    pub fn names_of<'a>(reg: &'a Registry, cd: &'a CardDef) -> Names<'a>;            // view.py:471 (+ registry aliases)
    pub fn matches(st: &GameState, dv: &Derived, ctx: &Ctx, uid: Uid, filters: &[NodeId]) -> Result<bool, EngineError>;
    pub fn stat(st: &GameState, dv: &Derived, uid: Uid, s: Stat) -> i32;
    pub fn same_values(a: &Derived, b: &Derived) -> bool;                    // view.py:94-106 (AI trial worlds)
    pub fn signature(st: &GameState, dv: &Derived) -> Json;                  // tests/paranoid.py:24-39
}
pub mod observe {       // C, L5; observe.py
    pub const HIDDEN_ZONES: [Zone; 4];
    pub fn is_hidden_from(st: &GameState, uid: Uid, observer: u8) -> bool;
    pub fn unknown_instances(st: &GameState, observer: u8, owner: u8) -> Vec<Uid>;
    pub fn unknown_pools(st: &GameState, observer: u8, owner: u8) -> (Vec<DefId>, Vec<DefId>);
    pub fn determinize(st: &GameState, observer: u8, seed: u64) -> GameState;
    pub fn determinize_into(st: &GameState, observer: u8, seed: u64, out: &mut GameState);
    pub fn permute_hidden(st: &GameState, observer: u8, seed: u64) -> GameState;
    pub fn information_set_key(st: &GameState, observer: u8) -> InfoSetKey;   // Eq + Hash only
}
pub mod procedures {    // C, L3/L5; engine entry points the AI calls (read-only ones may fill the view cache)
    pub fn declare_attack(st: &mut GameState, attacker: Uid, target: i32) -> Result<(), EngineError>;   // battle.py:12-32
    pub fn action_step_options(st: &GameState, p: u8) -> Result<Vec<Action>, EngineError>;          // game.py:647-660
    pub fn main_options(st: &GameState) -> Result<Vec<Action>, EngineError>;                        // game.py:543-583
    pub fn activated_list(st: &GameState, p: u8, timing: Timing) -> Result<Vec<(Uid, i32, i32)>, EngineError>; // game.py:410-433: (host, aid or SUPPORT_AID, cost)
    pub fn command_playable(st: &GameState, p: u8, uid: Uid, timing: Timing) -> Result<bool, EngineError>; // game.py:395-407
    pub fn eligible_blockers(st: &GameState, b: &Battle) -> Result<Vec<Uid>, EngineError>;           // battle.py:47-73
    pub fn set_rested_for_trial(st: &mut GameState, uid: Uid, rested: bool);  // ai/actions.py:227-229 (field write + touch)
    pub fn touch_for_ai(st: &mut GameState);                                  // touches on AI-owned clones (observe.py:82,130)
}
pub mod canon {         // C, L1
    pub fn canonical_state(st: &GameState) -> Json;
    pub fn state_digest(st: &GameState) -> String;
    pub struct Snapshot { pub state: serde_json::Value, pub continuous: serde_json::Value,
                          pub filter_sets: serde_json::Value, pub rules_checked: i64 }
    pub fn from_canonical(reg: &Arc<Registry>, snap: &Snapshot) -> Result<GameState, CanonError>;
}
pub mod invariants {    // C, L5; invariants.py:29-116
    pub fn check(st: &GameState, initial: Option<&CardMultiset>) -> Result<(), InvariantViolation>;
    pub fn initial_multiset(st: &GameState) -> CardMultiset;
}
#[cfg(feature = "testkit")] pub mod testkit { pub struct Scenario { /* testkit.py:38-182 */ } /* + helpers of testkit.py:185-439 */ }
```

#### 10.3.4 Internal layer boundary inside `gcg-engine`

The boundary is exactly the set of names each Python module calls on the next. `engine/core.py`,
`game.py`, `battle.py` and `observe.py` call `V.*` and `I.*`; `interp.py` calls `core.*`, `V.*`
and lazily `battle.damage_only_battle`. Each name is declared as a `pub(crate) fn` with Python's
parameter order. Keyword-only arguments go into an options struct, and every function that can
raise in Python returns `Result<_, EngineError>`. The core of the boundary:

```rust
// L1 (C), state.rs
impl GameState { pub(crate) fn touch(&mut self); }                                                   // state.py:529-531
// L2 (B2), view/
pub(crate) fn value(st: &GameState, dv: &Derived, ctx: &Ctx, v: ValueRef) -> Result<i64, EngineError>;      // view.py:853-882
pub(crate) fn cond(st: &GameState, dv: &Derived, ctx: &Ctx, c: CondId, did: bool) -> Result<bool, EngineError>; // :885-933
pub(crate) fn resolve(st: &GameState, dv: &Derived, ctx: &Ctx, r: RefId) -> Result<UidBuf, EngineError>;     // :784-831
pub(crate) fn resolve_ref(st: &GameState, ctx: &Ctx, r: RefId) -> Result<UidBuf, EngineError>;              // :848-850 (calls derived only if the ref selects cards)
pub(crate) fn select(st: &GameState, dv: &Derived, ctx: &Ctx, sel: SelId) -> Result<UidBuf, EngineError>;    // :626-632
pub(crate) fn player_cost_mod(st: &GameState, uid: Uid) -> Result<i64, EngineError>;                        // :525-545
pub(crate) fn zone_loc_code(z: Zone) -> i32;                                                                  // :952-954
// L3 (C), core.rs and battle.rs (engine/core.py, engine/battle.py)
pub(crate) struct MoveOpts { pub bottom: bool, pub reveal: bool, pub rested: bool }
pub(crate) fn move_card(st: &mut GameState, uid: Uid, dst: Zone, o: MoveOpts) -> Result<(), EngineError>;  // core.py:55-81
pub(crate) fn remove_from_game(st: &mut GameState, uid: Uid) -> Result<(), EngineError>;                   // :109-116
pub(crate) fn new_card(st: &mut GameState, def: DefId, owner: u8, zone: Zone, rested: bool) -> Uid;        // :119-131
pub(crate) fn shuffle_deck(st: &mut GameState, p: u8);                                                     // :134-139
pub(crate) fn record(st: &mut GameState, kind: HistKind, player: i8, by: i8, uid: i32, source: i32);       // :142-151 (no touch)
pub(crate) fn next_group(st: &mut GameState) -> i64;                                                       // :158-160
pub(crate) fn queue_trigger(st: &mut GameState, a: AbilityId, host: Uid, controller: u8,
                            event: &EventPayload, card_uid: i32, host_seq: i32) -> Result<(), EngineError>; // :248-284
pub(crate) fn emit(st: &mut GameState, ev: Ev, subject: i32, player: i32, lki: Option<&Lki>,
                   extra: &[(EvKey, i32)]) -> Result<(), EngineError>;                                     // :316-356
pub(crate) fn ability_snapshot(st: &GameState, uids: &[Uid]) -> Lki;                                       // :494-500
pub(crate) struct DamageOpts { pub source: i32, pub battle: bool, pub by: i32 }                          // keyword-only args
pub(crate) fn damage_card(st: &mut GameState, uid: Uid, amount: i64, d: DamageOpts) -> Result<i64, EngineError>; // :570-575
pub(crate) fn apply_damage(st: &mut GameState, uid: Uid, amount: i64, d: DamageOpts) -> Result<i64, EngineError>; // :602-633
pub(crate) fn shields_receiving_damage(st: &mut GameState, p: u8, n: usize, d: DamageOpts) -> Result<UidBuf, EngineError>; // :681-697
pub(crate) fn destroy_shields(st: &mut GameState, p: u8, uids: &[Uid], d: DamageOpts) -> Result<(), EngineError>; // :700-749
pub(crate) fn damage_shield_area(st: &mut GameState, p: u8, amount: i64, cards: usize, d: DamageOpts) -> Result<(), EngineError>; // :752-771
pub(crate) fn destroy(st: &mut GameState, uids: &[Uid], battle: bool, by: i32, source: i32) -> Result<UidBuf, EngineError>; // :774-827
pub(crate) fn set_winner(st: &mut GameState, losers: &[u8], reason: EndReason);                             // :842-854
pub(crate) fn rules_management(st: &mut GameState) -> Result<(), EngineError>;                              // :857-892
pub(crate) fn over(st: &GameState) -> bool;                                                                 // :940-942
pub(crate) fn damage_only_battle(st: &mut GameState, attacker: Uid, target: i32) -> Result<(), EngineError>; // battle.py:261-290 (two touches, :277, :282)
// L4 (B2), interp/ (engine/interp.py)
pub(crate) fn push_frame(st: &mut GameState, program: ProgramId, f: PushFrame) -> usize;                    // interp.py:61-88
pub(crate) fn run_top_frame(st: &mut GameState) -> Result<(), EngineError>;                                 // :109-132
pub(crate) fn resume(st: &mut GameState, action: Action) -> Result<(), EngineError>;                        // :160-175
pub(crate) fn draw(st: &mut GameState, p: u8, n: i64, by_effect: bool) -> Result<i64, EngineError>;         // :500-512 (returns cards drawn)
pub(crate) fn place_resource(st: &mut GameState, p: u8, rested: bool, by: i32) -> Result<bool, EngineError>; // :1051-1059
pub(crate) fn place_ex_resource(st: &mut GameState, p: u8, rested: bool, by: i32) -> Result<bool, EngineError>; // :1033-1043
pub(crate) fn pay_generic(st: &mut GameState, p: u8, cost: i64, ex_used: i32) -> Result<bool, EngineError>; // :1442-1470 (ex_used = -1 default)
pub(crate) fn can_pair(st: &GameState, dv: &Derived, unit: Uid) -> bool;                                    // :1340-1344
pub(crate) fn pair(st: &mut GameState, pilot: Uid, unit: Uid, by: i32) -> Result<(), EngineError>;          // :1368-1393
pub(crate) fn targets_available(st: &GameState, program: ProgramId, start: u16, ctx: &Ctx) -> Result<bool, EngineError>; // :235-260 (read-only)
pub(crate) fn add_lasting(st: &mut GameState, eff: ContRef, controller: u8, source: i32,
                          targets: &[(Uid, i32)], duration: Duration, o: LastingOpts) -> Result<(), EngineError>; // :1286-1321
pub(crate) fn system_program(st: &GameState, s: SystemProgram) -> ProgramId;                               // :1589-1594 (fixed ids)
```

`UidBuf` is a reusable scratch list of uids (a `Vec<Uid>` from a per-thread pool). It replaces
Python's temporary tuples without allocating on the hot path.

**The Python function wins.** Each frozen signature names the Python function it ports. If an
implementer finds that the parameters, the return value or the line range differ from that
function, the function is the contract. The implementer reports the difference, and the lead
amends `docs/dev/contracts.md` under the change protocol (§10.6). Rust-only parameters are
allowed only where Rust needs them: an explicit `&Derived`, option structs for keyword
arguments, and `&GameState` for test overrides.

#### 10.3.5 `gcg-ai` internal boundary (D1 ↔ D2) and the agent API

```rust
// D1: policy.rs, evaluation.rs, playout.rs (ai/policy.py, evaluation.py, playout.py)
pub struct PolicyKnobs { pub burst_yes_score: f64, pub rule_polarity_table: bool, pub zero_ap_face: bool }
pub const BASELINE_KNOBS: PolicyKnobs;
pub fn option_scores(st: &GameState, dec: &DecisionView<'_>, options: &[Action], knobs: &PolicyKnobs) -> Result<Vec<f64>, EngineError>; // no `dv`: scorers compute the view where Python does (contracts.md change log)
pub fn choose(st: &GameState, rng: Option<&mut SplitMix64>, temperature: f64, knobs: &PolicyKnobs) -> Action;
pub fn pick(options: &[Action], scores: &[f64], rng: Option<&mut SplitMix64>, temperature: f64) -> usize;
pub struct Weights { /* feature set + values; ROLE_WEIGHTS, TUNED_WEIGHTS, HAND_WEIGHTS as exact f64 bit patterns, evaluation.py:62-129 */ }
pub fn burst_density(st: &GameState, observer: u8) -> f64;
pub fn score(st: &GameState, player: u8, w: &Weights, density: f64) -> f64;
pub fn logistic(z: f64) -> f64;
pub fn evaluate(st: &GameState, player: u8, w: &Weights, density: f64) -> f64;
pub fn play_until_turn(st: &mut GameState, turn: i32, rng: Option<&mut SplitMix64>, temperature: f64,
                       max_decisions: u32, knobs: &PolicyKnobs) -> Result<u32, EngineError>;
pub fn play_to_end(st: &mut GameState, rng: Option<&mut SplitMix64>, temperature: f64,
                   max_decisions: u32, knobs: &PolicyKnobs) -> Result<u32, EngineError>;
// D2: actions.rs, fingerprint.rs, noop.rs, setup.rs (ai/actions.py, noop.py, setup.py)
#[derive(Clone, Copy, PartialEq, Eq, Hash)] pub struct MoveKey { pub kind: ActionKind, pub a: i64, pub b: i64, pub variant: i32 }
pub struct PruneFlags { pub strict_attacks: bool, pub rest_targets: bool }
pub fn uid_argument(dec: &DecisionView<'_>) -> bool;
pub fn ex_used(a: Action) -> i32;
pub fn canonical(st: &GameState, dec: &DecisionView<'_>) -> Vec<(MoveKey, Action)>;
pub fn prune_dominated(st: &GameState, dec: &DecisionView<'_>, moves: Vec<(MoveKey, Action)>, f: PruneFlags) -> Result<Vec<(MoveKey, Action)>, EngineError>;
pub fn merge_equivalent(st: &GameState, dec: &DecisionView<'_>, moves: Vec<(MoveKey, Action)>) -> Vec<(MoveKey, Action)>;
pub struct NoopFlags { pub branch_choices: bool, pub resource_aware: bool }
pub fn without_effect(st: &GameState, player: u8, dec: &DecisionView<'_>, moves: &[(MoveKey, Action)], seed: u64,
                      knobs: &PolicyKnobs, f: NoopFlags, w: &Weights) -> Result<Vec<MoveKey>, EngineError>;
pub fn mulligan_choice(st: &GameState, player: u8, samples: u32, rng: &mut SplitMix64) -> (Action, Vec<Alternative>);
pub fn first_player_choice(st: &GameState, player: u8, games: u32, rng: &mut SplitMix64, knobs: &PolicyKnobs,
                           cx: &mut AgentCx<'_>) -> Result<(Action, Vec<Alternative>), EngineError>;
pub fn opening_score(hand: &[&CardDef], first: bool, reg: &Registry) -> f64;
// gcg-ai-reference (D2)
pub struct ReferenceConfig { /* the 24 SearchConfig fields, config.py:13-57, Python defaults */ }
pub fn reference_preset(name: &str) -> Option<&'static ReferenceConfig>;   // fast|standard|strong
pub fn baseline_preset(name: &str) -> Option<&'static ReferenceConfig>;    // standard|strong
pub const TUNING_CONFIG: ReferenceConfig;
pub fn search(st: &GameState, player: u8, cfg: &ReferenceConfig, w: &Weights, rng: &mut SplitMix64,
              cx: &mut AgentCx<'_>, tracer: &mut dyn SearchTracer) -> Result<SearchResult, EngineError>;
pub fn make_reference_agent(cfg: &'static ReferenceConfig, seed: u64, log: bool, w: &'static Weights) -> Box<dyn Agent>;
// gcg-search (D2, v0 copy; Phase 6 adds fields)
pub enum Preset { Standard, Fast }                                          // FromStr: exactly "standard", "fast"
pub struct StrengthenedConfig { /* every ReferenceConfig field + one field per mechanism; no Default */ }
pub fn preset_config(p: Preset) -> &'static StrengthenedConfig;
pub fn null_config(reference: &ReferenceConfig) -> StrengthenedConfig;      // all mechanisms off = the reference
pub fn make_agent(cfg: &StrengthenedConfig, seed: u64, log: bool) -> Box<dyn Agent>;
```

`SearchTracer` has per-iteration and per-apply hooks for drill-down levels 2-3. Normal runs use
`NoTrace`, whose methods are empty and inline away.

#### 10.3.6 `gcg-runner` (E) for `gcg-cli`, `gcg-eval` and `gcg-diff`

```rust
pub enum Role { Dut, Bench }
pub struct AgentSpec { pub role: Role, pub seat: u8, pub preset: &'static str, pub seed: u64, pub log: bool }
pub trait AgentFactory: Sync {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError>;
    fn describe(&self) -> String;                                  // results.json ai.factory
    fn preset_label(&self) -> &str;                                // results.json ai.preset
}
pub enum Format { Bo3, Bo1 }
pub enum Workers { Count(u32), Auto }
pub struct BenchmarkConfig { pub deck_under_test: Deck, pub benchmark_deck: Deck, pub matches: u32,
    pub seed: u64, pub workers: Workers, pub format: Format, pub out_dir: PathBuf, pub decision_log: bool,
    pub replays: u32, pub turn_limit: i32, pub max_actions: i64, pub schema: SchemaVersion,
    pub package: &'static str, pub code_digest: &'static str }       // runner/config.py:18-44
pub fn game_seed(master: u64, m: u32, g: u32) -> u64;               // runner/config.py:75-76
pub fn agent_seed(master: u64, m: u32, g: u32, seat: u8) -> u64;    // :79-80
pub fn dut_seat(m: u32) -> u8;                                      // :64-66
pub fn play_game(cfg: &BenchmarkConfig, m: u32, g: u32, chooser: Option<u8>, f: &dyn AgentFactory,
                 cx: &mut AgentCx<'_>) -> Result<GameRecord, RunError>;      // runner/game.py:106-166
pub fn play_match(cfg: &BenchmarkConfig, m: u32, f: &dyn AgentFactory, cx: &mut AgentCx<'_>) -> Result<MatchRecord, RunError>; // runner/match.py:19-46
pub fn run_benchmark(cfg: &BenchmarkConfig, f: &dyn AgentFactory, progress: &mut dyn FnMut(u32, u32),
                     cancel: &Cancel) -> Result<BenchmarkRun, RunError>;     // runner/run.py:109-140
pub mod reports {
    pub fn build_results(run: &BenchmarkRun) -> Json;                       // reports/results.py:145-189
    pub fn validate_results(doc: &Json, v: SchemaVersion) -> Result<(), SchemaError>;
    pub fn write_reports(run: &BenchmarkRun, out: &Path, cancel: &Cancel) -> Result<ReportPaths, ReportError>; // reports/writer.py:94-110
}
pub mod replay { pub fn verify_file(path: &Path, log: bool) -> Result<ReplayReport, ReplayError>; }   // runner/replayfile.py
pub mod test_agents { pub fn factory(kind: TestAgentKind) -> Box<dyn AgentFactory>; }                 // tests/runner/random_agent.py
pub struct MirroredSpec { /* ai/selfplay.py:88-110 */ }
pub fn mirrored_specs(games: u32, master: u64, challenger: &AgentDesc, opponent: &AgentDesc, decks: [&DeckList; 2]) -> Vec<MirroredSpec>;
pub fn comparison_specs(games: u32, seed: u64, challenger: &AgentDesc, opponent: &AgentDesc, decks: &[DeckList]) -> Result<Vec<MirroredSpec>, SpecError>; // ai/compare.py:127-144
```

#### 10.3.7 Trace reader (G) for all workstreams

```rust
pub enum Record { Header(Header), Snapshot(SnapshotRec), Decision(DecisionRec), Iter(IterRec), ApplyTrace(ApplyTraceRec),
                  Outcome(OutcomeRec), Scenario(ScenarioRec), Apply(ApplyRec), Call(CallRec), State(SnapshotRec),
                  View(ViewRec), Probe(ProbeRec), Override(serde_json::Value), ForceLink(Vec<String>), End(EndRec) }
pub fn read_trace(path: &Path) -> Result<Vec<Trace>, TraceError>;
pub fn replay_trace(t: &Trace, opts: &DiffOptions) -> Result<ReplayStats, Divergence>;   // Divergence: Display = §8.7 report
pub trait EngineUnderTest { /* new_game, load, pending, apply, observe, canonical, probe, call; MockEngine and the real engine */ }
```

### 10.4 Workstream briefs

Every brief below is self-contained for its subagent. Read the named inventory sections and Python
files first. Do not edit files outside "Owns" (§10.1). Return the report format of §10.6.

**A — data.**
- **Read:** Inv 03 (all); `cards/*.py`, `deck/*.py`, `reports/versions.py`, `cli.py:236-331`.
- **Deliver:** §10.3.1:
  - the loader with `_make_def` normalisation, inline tokens and unit variants in def_id order
    (`cards/db.py:119-272`);
  - every override form, including the two unused ones (W-164, with synthetic-data tests);
  - the deck grammar with Python's behaviour on exotic input: Unicode digits, U+001C-U+001F
    whitespace, and `int` counts read as `u64` with W-144 size violations;
  - NFKC + full casefold + `str.split` whitespace from the generated tables (A27);
  - the 13 violation messages in Python order; official data; `versions()`; conflict
    resolutions; the `data status` text and `--json` parts.
- **Gate (M4):**
  - every exported `CardDef` equal field by field (1153);
  - the four fingerprints equal;
  - every exported deck triple equal;
  - 10,000 NFKC/casefold vectors equal;
  - `data status` text and `--json` bytes equal to Python's, except `code_digest`/`package`
    (assembled with B1's coverage by E's CLI test);
  - `cargo test -p gcg-data` green, clippy clean.

**B1 — IR and registry.**
- **Read:** Inv 04 (all); `effects/dsl.py`, `program.py`, `registry.py`, `golden.py`,
  `tools/golden.py`, `runner/digest.py`.
- **Deliver:**
  - `oracle/export_ir.py` (§3.3), with the four exporter assertions and the two-hash-seed
    check;
  - §10.3.2, including the `dsl_nodes!` macro, typed sub-kind validation, `ContRef`
    canonicalisation, hook decoding, `repr`, content ids, the golden writer and fragments.
- **Gate (M2):** §3.4 tests 1-6 green; `uv run python oracle/export_ir.py --check` exits 0;
  `cargo test -p gcg-effects` green.

**G — oracle and differential runner.**
- **Read:** Inv 10 §6, §9, §10; Inv 11 §7; `tools/fuzz.py`, `testkit.py`, `tests/paranoid.py`,
  `runner/digest.py`, `scripts/compare_python_vs_rust.sh`, `scripts/normalize_outputs.py`.
- **Deliver:**
  - the exporters of §8.1 marked G, and the tier-1 corpus with both manifests;
  - `gcg-diff`: §8.7, §10.3.7, the generators and checks of §9.2-§9.4 and §9.9-§9.10, and the
    W-id partition check of §12.2;
  - `scripts/verify_all_rust.sh`;
  - the W-160 changes: raw-byte masking in the normaliser; `fast`, `--decision-log` and a third
    pair in the matrix; two hash seeds; stale-baseline detection by recorded Python commit and
    data lock; `--offset-probe N` (§8.9); `--selftest` for the normaliser's negative controls
    (`1e-5` vs `1e-05`, a missing newline, a changed key order);
  - the timing-side fixes of the comparison script (§11.5): `need_rust` always rebuilds, the best
    worker counts are recorded, `report` pairs runs by role and fails below 2x, and
    `part2 --candidate FILE` exists; `report --selftest` checks the pairing and the failure
    cases on fixture hyperfine files (T-158);
  - `scripts/verify_binary_offline.sh` and `scripts/cargo_test_features.py` (§9.11).
- **Gate (M3):**
  - `traces.py --check` reproduces the tier-1 manifest under both hash seeds;
  - `--nocache` gives identical bytes;
  - the wrapped-vs-plain check passes;
  - `gcg-diff corpus --parse-only` decodes every record into the Rust record types and checks
    the manifest and IR hashes;
  - the `MockEngine` replay passes, and all negative controls of §8.7 fire on synthetic traces;
  - the normaliser self-test passes;
  - the `call`/`probe` argument encoder round-trips one synthetic call per argument kind (T-157),
    and the override tests ran one per fresh process with their memo-table assertion (A16);
  - `compare_python_vs_rust.sh report --selftest` passes (T-158).

  `gcg-diff selftest` on real traces becomes a gate item from M5.

**C — engine core (layers L1, L3, L5).**
- **Read:** Inv 05 §1-§4; Inv 06a §1-§11; Inv 06b (all); `engine/state.py`, `types.py`,
  `core.py`, `battle.py`, `game.py`, `observe.py`, `invariants.py`, `testkit.py`,
  `runner/digest.py`.
- **Deliver:**
  - **L1:** the §4 state (arenas, touch, clone), canonical JSON, the digest and
    `from_canonical`.
  - **L3:** the core primitives, triggers, damage, destroy, rules management and battle.
  - **L5:**
    - setup and the step machine;
    - legal actions in Python's order;
    - `apply` with payment and alt-play decoding;
    - observe (`determinize`, `permute_hidden`, `information_set_key`) and invariants;
    - the `paranoid` rules-management check;
    - the testkit and the native ports of the 12 lead-example tests.
- **Gates:**
  - **M5.1 (L1):** the canonical round trip and digest on every tier-1 snapshot; the clone
    property tests.
  - **M5.3 (L3):** unit tests plus primitive vectors.
  - **M5.5 (with B2's L4):**
    - `gcg-diff corpus --only full,vanilla,fuzz,coverage,tests,worlds --require-all`;
    - the §4.4 vectors;
    - `gcg-diff fuzz --games 10000 --seed 2026` with the summary equal;
    - a green paranoid run;
    - `gcg-diff selftest`.

**B2 — view, interpreter, hooks (layers L2, L4).**
- **Read:** Inv 05 §5-§7; Inv 06a §12-§16; `engine/view.py`, `engine/interp.py`,
  `effects/builtins.py`, and the hook functions in `effects/bindings/*.py` (Inv 04 §6.2 lists
  them).
- **Deliver:**
  - **L2:** the base view, queries and expression evaluation, the 21 pure hooks, the fixpoint and
    `_apply_cont` for every effect kind, the cache of §4.6, and `paranoid` view checks;
  - **L4:** interpreter control flow, `Choose`, the zone and card handlers, decisions, `Apply`
    and lasting effects, `RuleGrant` consumers, the 13 step hooks and the system programs.
- **Gates:**
  - **M5.2 (L2):** `gcg-diff corpus --only views --require-all` (view signatures, probes,
    trigger index from snapshots).
  - **M5.5:** shared with C, as above.

**D1 — policy and evaluation.**
- **Read:** Inv 08 (all); `ai/policy.py`, `values.py`, `targeting.py`, `evaluation.py`,
  `playout.py`, `agents.py:18-66`.
- **Deliver:** §10.3.5 (the D1 part), the weight tables as exact `f64` bit patterns
  (`evaluation.py:62-129`), and greedy and random. Do the pure parts first (`pick`, `logistic`,
  weights, opening values) and test them on vectors before the engine is green.
- **Gate (M6):** every policy and evaluation vector of the `components` set bit-identical; the
  `baselines` traces 100%.

**D2 — search and agents.**
- **Read:** Inv 07 (all); `ai/mcts.py`, `config.py`, `baseline.py`, `actions.py`, `noop.py`,
  `setup.py`, `agents.py`, `base.py`, `runner/agents.py`.
- **Deliver:**
  - §6.1-§6.6 and §10.3.5 (the D2 part);
  - the `OnceKey` rules in fingerprints;
  - `export_ai_vectors.py` (search part);
  - the `gcg-search` v0 copy with `StrengthenedConfig` and `null_config`.
- **Gate (M6):**
  - every root-function vector equal;
  - the `search` traces 100% at `fast`, `standard` and `strong`, plus the Inv 07 §17
    acceptance searches;
  - the `test_baseline` digest;
  - the puzzle and information-set ports green;
  - fork-join `first_player_choice` equal to serial on every `search` play/draw decision;
  - the **null-config test**: `gcg-search` with `null_config(reference_preset(p))` makes the same
    decisions, alternatives and RNG consumption as the reference on every `search` decision, for
    all three presets.

**E — runner, reports, CLI.**
- **Read:** Inv 01 §2-§5; Inv 02 (all); `runner/*`, `reports/*`, `cli.py`, `tests/cli/*`,
  `tests/runner/*`, `tests/reports/*`.
- **Deliver:**
  - §10.3.6 and §7 (pool with the separate `lowest_failed` signal, topology, SIGINT through
    `signal-hook` once the user has approved the fetch, A3/A32);
  - the schema-subset validator (§9.5) and replay verification;
  - the test agents;
  - the argparse-compatible parser: usage and error texts for the fixed parser structure, the
    `int(text, 0)` grammar, prefix abbreviations, choices and difflib suggestions as in
    Python 3.15;
  - progress output (TTY and non-TTY, `cli.py:143-159`), exit codes 0/1/2/130, and the
    `build.rs` code digest.
- **Gate (M7):**
  - reports from every exported record set byte-identical (`results.json`, `summary.md`,
    `games.ndjson`, replays);
  - the CLI transcripts identical;
  - the ported CLI, runner and report tests green;
  - identical outputs at 1, 4 and 10 workers with the random test agent;
  - the interrupt test (exit 130, message, no reports);
  - the lowest-failing-index test (failures injected at indices 3 and 7, `--workers 10`);
  - `gcg-sim --help` and every subcommand's help and usage errors equal to Python's except the
    `--ai-preset` choices and help and the `--workers auto` value (A30, T-152).

**F — eval harness.**
- **Read:** Inv 08 §8-§10; `ai/selfplay.py`, `ai/compare.py`, `ai/tuning.py`; the brief's
  `<strength_evaluation>`.
- **Deliver:**
  - `reference-benchmark`: the runner with the reference factory, Python's preset and agent
    names, results 1.1.0, and `--agent test-random|test-illegal`;
  - `compare` (exact port);
  - `head-to-head`:
    - the agent-spec grammar `random|greedy|ref:<preset>|ref:baseline-<p>|search:<preset>|search:FILE.json`,
      with `:field=value` overrides;
    - mirrored blocks of four and `comparison_specs` seeding;
    - `--choose-first seat0|agents`: `seat0` (the default) makes the harness answer CHOOSE_FIRST
      with `go_first(0)`, which balances on-play/on-draw per agent and per deck (A24); `agents`
      is the W-44 diagnostic;
    - deck selection by set (`--decks examples|tests|tuning|eval|panel`, `--pairs all|tuning|held-out`)
      or by a repeatable `--pair DECK_A:DECK_B` (file stems of the deck sets or paths); every
      report row is labelled with the two deck names;
    - Wilson intervals per pair and pooled, pooled over held-out pairs separately (A26), and W-42
      paired statistics (Newcombe intervals, McNemar discordant counts);
  - the deck sets (`examples`, `tests`, `tuning`, `eval`, `panel`), the seed roles and the
    ledger, and `env`.
- **Gate (M8):**
  - `compare` outputs equal to Python's for three configurations;
  - head-to-head identical across 1, 4 and 10 workers;
  - the balance test: with `seat0`, each agent is on the play in exactly half the games of every
    block and plays each deck once on the play and once on the draw (T-149);
  - the Wilson/Newcombe vectors;
  - `reference-benchmark` smoke runs pass, and their replays verify.

### 10.5 Integration order behind the differential runner

```text
M2  S contracts ─┬─ A data (starts) ─────────┐
                 ├─ B1 IR/registry ──────────┼─ registry loads with CardDb; content ids, golden, counts verified
                 └─ G exporters (start) ─────┘
M3  G: tier-1 corpus + gcg-diff (parse-only, MockEngine, negative controls), W-160 harness, probe tool
M4  A data gate
M5.1 C  L1 state/canon      → tier-1 snapshots round-trip, digests equal
M5.2 B2 L2 view             → views set 100%
M5.3 C  L3 core/battle      → unit tests and primitive vectors
M5.4 B2 L4 interp/hooks     → compiles with L1-L3; per-handler unit tests
M5.5 C  L5 game/observe     → setup prefixes → vanilla → fuzz → coverage → tests (by module) → worlds; fuzz 10,000; paranoid
M6  D1 + D2  pure parts on vectors (from M3) │ then components → baselines → search; null-config test
M7  E   reports from exported records (from M3) │ then live runs, CLI, pool, interrupt
M8  F   statistics on vectors (from M3) │ then compare, head-to-head, reference-benchmark; first verify_all_rust.sh
M9  Phase 4: tier 2, e2e matrix, probe, replays, traceability, coverage
```

The lead merges each workstream's worktree as soon as its own tests pass and no previously passing
trace fails. A merge must never lower the corpus pass rate.

### 10.6 Rules for implementer subagents

- One writer per file. Each workstream runs in its own git worktree from the latest milestone
  commit (`isolation: worktree`). In the worktree it runs `cargo build --offline` (the registry
  cache is shared) and `uv sync --locked` if it runs exporters. It hands back uncommitted
  changes, and the lead commits per milestone.
- **Interface change protocol.** A subagent that needs a frozen signature or contract type
  changed stops that part. It writes the proposed change and its reason in its report, and
  continues on the parts that do not depend on it. Only the lead edits `contracts.rs` files and
  `docs/dev/contracts.md`, and the lead tells the affected workstreams.
- Never port from memory. Each function's doc comment cites its Python `path:line`.
- Never weaken, skip or special-case a test or a trace. A Python test that looks wrong is
  reported with evidence.
- No timing code, no `unsafe` (the `erfc` FFI is the only allowlisted module), no
  `HashMap`/`HashSet` iteration in logic, no dependency outside A8's list.
- The repository's `.gitignore` ignores directories named `build/`, `dist/`, `lib/`, `env/`,
  `var/`, `parts/` and `target/` anywhere in the tree, so no crate module directory may use one of
  these names (a file `env.rs` is fine; a directory `src/env/` would silently stay untracked).
- **Report format:** finished items with evidence (commands and outputs); corpus pass counts per
  set; files touched; interface change requests; open problems; everything left undone, stated
  as not done.

## 11. Performance (Phase 5), Apple silicon and M6, GPU

### 11.1 Targets and the only measurement

- **Part 1** (equal work, the reference search through `gcg-eval reference-benchmark`). Pair A is
  Python `--ai-preset standard` vs Rust `--reference-preset standard`; pair B is Python `strong` vs
  Rust `strong`. The configurations are the brief's: headline (`--matches 100` at the best worker
  count and at 10 workers), single-thread (`--matches 20 --workers 1`) and full search
  (`--matches 20` at the best worker count). The Python baseline is 13.3 s per reference `standard`
  BO1 game: 15,116 iterations at about 0.86 ms each (Inv 11 §2). The floor is 2x in every
  configuration. The aim is ≥ 10x, which is ≤ 86 µs per iteration, all costs included.
- **Part 2** (shipped presets through `gcg-sim`): `--ai-preset fast` vs Python `standard`, and
  `--ai-preset standard` vs Python `strong`. The floor is 2x in every configuration.
- **The only timing is `scripts/compare_python_vs_rust.sh`** (hyperfine, outside the crates):
  `env`, `python --best N` (once), `best-workers python|rust-reference|rust-presets`, `part1`,
  `part2 [--candidate FILE]` and `report`. It does ≥ 3 runs for long configurations and ≥ 5 for
  short ones (`:75-104`). Today the script has three defects that G fixes at M3 (§10.1):
  - `need_rust` builds only when a binary is missing (`:38-40`), so it can time a stale build. It
    must always run `cargo build --release --locked --offline -p gcg-cli -p gcg-eval` and record
    the binaries' `code_digest` (from `gcg-eval env`) in `env.txt`;
  - `report` pairs a Python run with the Rust run of the same name suffix (`:197-207`), so
    `py-standard-m100-w6` never meets `rs-ref-standard-m100-w8`, and `--best auto` gives `wauto`.
    It must pair by role, from the best counts that `python`, `part1` and `part2` record in
    `best-<impl>.txt`: for each of the four pairs (Part 1 A and B, Part 2 `fast` and `standard`),
    headline-best, headline-10 and single-thread for the screening pairs and full-search for the
    full-search pairs, eight configurations in all. It exits 1 if any of the eight is missing,
    below 2x, or timed with a Rust binary whose `code_digest` differs from `env.txt`;
  - `part2 --candidate FILE` does not exist. It times `gcg-eval preset-benchmark --search-config
    FILE` with the Part 2 configurations (§12.8).
- **Keep rule.** A change is kept only if its configuration's median improves by more than the
  min-max spread of the runs before it, and the tier-1 corpus plus the seed-1 equivalence subset
  stay identical. Every kept or rejected change gets a before/after row in
  `docs/dev/BENCHMARKS.md`, with the commands.

### 11.2 Where time is expected to go: design targets, not claims

Python's shares come from Inv 11 §2-§6: `apply`/`advance` 80%; `view.derived` 33% (1.71 M calls,
318,681 recomputes per game at 14 µs mean); legal actions about 20%; clone 3-4%; determinize
4-5%; policy 6%; evaluation 1.5%; dataclass allocation 3.5% of samples.

| Python cost | Rust mechanism | Design target per call |
| --- | --- | --- |
| `_compute` 14 µs | dense per-uid arrays, per-definition static tables, double-buffered fixpoint, recycled `Derived` buffers | ≤ 0.5 µs per compute |
| `derived` hit 0.126 µs | `RefCell` borrow, `Arc` clone, a 5-field key compare | ≤ 10 ns |
| `clone` 17-21 µs | about 20 `Vec<Copy>::clone_from` into a pooled state | ≤ 0.3 µs |
| `determinize` 78-96 µs | `clone_from_state`, per-owner unknown pools from per-def count arrays, two Fisher-Yates shuffles, history rewrite | ≤ 3 µs |
| `main_options` 24 µs | loop over precomputed per-def play data; payment choices from counters; activated list from view spans | ≤ 1 µs |
| `emit` 14 µs incl. view | per-game listened bitmask, per-view trigger buckets, stack candidate lists | ≤ 0.3 µs |
| policy `option_scores` 10 µs | per-def value tables; scores take the option slice (`ai/policy.py:104-108`) | ≤ 0.5 µs |
| dataclass allocation | `Copy` records, scratch buffers | 0 allocations per iteration once warm |
| one MCTS iteration 0.86 ms | all of the above | ≤ 30 µs (≥ 28x single-thread) |

These are design targets for the profile review. They are never asserted anywhere and never
written as results. BENCHMARKS.md reports only measured end-to-end times.

### 11.3 Profiling method

```text
samply record target/release/gcg-eval reference-benchmark examples/decks/red-green-zeon.txt \
  examples/decks/blue-white-federation.txt --reference-preset standard --format bo1 --matches 2 \
  --seed 1 --workers 1 --out build/prof
xcrun xctrace record --template 'Time Profiler' --output build/prof/tp.trace --launch -- \
  target/release/gcg-eval reference-benchmark … (same arguments)
xcrun xctrace record --template 'Allocations' --output build/prof/alloc.trace --launch -- \
  target/release/gcg-eval reference-benchmark … --matches 1
```

The release profile already carries line tables (`Cargo.toml` `[profile.release] debug =
"line-tables-only"`). Profiles stay in `build/` (gitignored) and are never committed. Findings
are summarised in BENCHMARKS.md: share per category, cost per iteration, allocations inside the
search call tree.

### 11.4 Optimisation steps, in order (each measured, kept only with a win)

1. **Zero heap allocation on the search path.** `StatePool` + `clone_from_state`, recycled
   `Derived` buffers (`Arc::get_mut` on a uniquely owned old view, held in a non-serialised spare
   slot), and reused option, candidate, path and `UidBuf` buffers in `Scratch`. Verification
   happens outside the crates: the Instruments Allocations trace above, filtered to
   `gcg_ai_reference::mcts::search` after the first decision, must show zero allocations (M10,
   T-104). After M15 the same check runs on `gcg_search`'s search call tree with
   `gcg-sim benchmark --ai-preset standard` and `--ai-preset fast` (T-148). Both results go into
   BENCHMARKS.md. There is no counting allocator in the crates, because a counting `GlobalAlloc`
   needs `unsafe`.
2. **Cheaper derived recompute, same results.** Per-definition static tables built at registry
   load (base AP/HP/traits, entry lists, `printed_keywords`, has-constant flags); the Python
   early exit (`view.py:414-419`); a fixpoint that reuses `cur`/`prev` buffers.
3. **Legal actions.** Reuse buffers. Memoise, per state version, the pieces that `main_options`
   computes more than once (`game.py:543-583`). Keep the option order.
4. **Facet-versioned revalidation** (W-124), only after the §4.6 transparency proof and only if
   derived recompute still dominates. Python recomputes after every touch, and 74% of the
   recomputes reproduce the previous view (`docs/python/AI.md:500-502`). The mechanism:
   - per-state facet counters cover each zone list, card `def_id`/`pair`/`rested`/`damage`/
     `entered_turn`, the lasting list, history, turn and active player, and the battle stack;
     every private mutator bumps its facet;
   - each compute records a read mask built from per-node facet masks in the IR. Hooks declare
     their masks, and an undeclared hook reads everything;
   - on an invalid cache, the view is revalidated without recomputing when no facet in the mask
     changed;
   - `version` counting is untouched (it is serialised).

   `paranoid` recomputes on every revalidation and asserts equality. The tier-1 corpus and the
   seed-1 equivalence subset run with revalidation on and off.
5. **Codegen.** The release profile already sets `opt-level = 3`, `lto = "fat"`,
   `codegen-units = 1` and `panic = "abort"`. `.cargo/config.toml`:

   ```toml
   # Local builds tune for the machine that runs them (M2 Pro: apple-m2 features).
   # Distributable builds: RUSTFLAGS="-C target-cpu=apple-m2" cargo build --profile dist
   # (scripts/build_dist.sh). On an M6 with a toolchain that has no M6 target name, keep
   # "native" or use the newest known Apple target (apple-m5).
   [build]
   rustflags = ["-C", "target-cpu=native"]
   ```

   `Cargo.toml` adds `[profile.dist] inherits = "release"`. Rust never contracts `a * b + c` into
   an FMA without `mul_add` (banned, §9.9), and libm calls are runtime calls. So `target-cpu`
   cannot change float results. M9 and M10 prove this by running the tier-1 corpus and the
   seed-1 equivalence subset with both builds.
6. **Alignment.** 128-byte alignment of `Scratch`, the pool counter and the queue head
   (`hw.cachelinesize` = 128).
7. **Allocator.** mimalloc against the system allocator, measured in Part 1 and Part 2. The crate
   is not cached and needs `cc` to build its C sources, so it is in the M1 user request (A32). It
   would be a dependency of the two binaries only. If the user refuses, BENCHMARKS.md records the
   measurement as not done, with the reason, as a deviation from brief step 18.
8. **PGO.** It needs the `llvm-tools` rustup component (in the M1 user request) and
   `-Cprofile-generate`/`-Cprofile-use` on the profiling workload. It is measured if approved;
   otherwise BENCHMARKS.md records it as not measured, with the reason.
9. **NEON.** Only for a profiled data-parallel integer kernel (zone filters, bitsets). Never for
   float reductions in the reference: Neumaier order and evaluation order stay sequential
   (Inv 11 H4).
10. **Worker count and tail.**
    - `best-workers` over 6, 8, 10 and `auto`.
    - `first_player_choice` fork-join (§7.3), measured in Part 1 and kept only if it wins. It is
      the only exact intra-match parallelism the reference allows: a match is the dispatch unit
      (a BO1 match is one game, and a BO3 game depends on the previous game's loser), and the
      tree search itself is sequential (`ai/mcts.py:135-151`). Beyond it, the tail on efficiency
      cores is left to the OS's thread migration; no further mitigation exists for the
      reference, and BENCHMARKS.md reports the measured tail as it is.
    - The strengthened presets get root-tree fork-join (§7.3, S1), measured in Part 2.
11. **`unsafe`.** Only where a profile proves a measurable win. Each block needs a `// SAFETY:`
    argument and unit tests, and a Miri run where feasible (Miri needs nightly, used as a test
    tool only; the build stays on stable). The lint allowlist (§9.9) is extended per block, with
    the evidence in BENCHMARKS.md.

If Part 1 lands below 10x, BENCHMARKS.md explains why from the profile: the share per category,
the cost per iteration, and what an equal-work run cannot remove. Examples are the reference's
up-front 2 × cap seed draws, the per-iteration world clone, and the lazily built worlds.

### 11.5 Part 1 and Part 2 procedure

```text
scripts/compare_python_vs_rust.sh env
scripts/compare_python_vs_rust.sh best-workers python          # once, Phase 0
scripts/compare_python_vs_rust.sh python --best <N_py>          # once, Phase 0; reused
scripts/compare_python_vs_rust.sh best-workers rust-reference
scripts/compare_python_vs_rust.sh part1 --best <N_rs>
scripts/compare_python_vs_rust.sh best-workers rust-presets    # Phase 6
scripts/compare_python_vs_rust.sh part2 --best <N_rs>
scripts/compare_python_vs_rust.sh report                        # median/min/max, speedup per role-paired
                                                                # configuration; exit 1 if any of the 8 is
                                                                # missing, below 2x or from a stale build
```

`python`, `part1` and `part2` write the worker count they used to `best-<impl>.txt` in
`$COMPARE_OUT`, so `report` pairs the headline-best runs by role even when Python's and Rust's
best counts differ.

BENCHMARKS.md reports median, min and max, the speedup ratios, CPU model, core counts, memory,
macOS, Python, uv, rustc, LLVM and hyperfine versions, and the exact commands (`record_env`,
`:43-62`). The 2x floor must hold in every configuration of both parts. The machine runs nothing
else during measurement, and there is no warm-up, power or thermal control (brief
`<target_hardware>`).

### 11.6 M6 support (no M6 available: build support, detection and procedure)

- **Runtime detection.** `gcg_core::sys::topology::detect()` reads `hw.nperflevels` (3 on an M6)
  and every `hw.perflevelN.{logicalcpu,physicalcpu,name}`, plus `hw.cachelinesize` and the
  `hw.optional.arm.FEAT_*` listing. `gcg-eval env` also prints
  `std::arch::is_aarch64_feature_detected!` for the features Rust names, all as JSON.
  `timing.json` records the tiers (W-104), and the compare script already prints
  `hw.perflevel2` when present (`scripts/compare_python_vs_rust.sh:48-52`). No code path requires
  an optional CPU feature, so an `apple-m2` distributable runs unchanged on an M6. Unit tests parse
  captured `sysctl` outputs for two-tier and three-tier machines.
- **Scheduling.** The pool needs no tier awareness (§7.3). `auto` = 2 + 4 + 6 = 12 workers.
- **Build.** rustc 1.99.1 / LLVM 23.1.1 knows `apple-m4` and `apple-m5` but no M6 name. On an M6,
  use `-C target-cpu=native` (LLVM detects the host features even for an unknown CPU) or
  `-C target-cpu=apple-m5`. Check with `rustc --print target-cpus | grep apple` and
  `rustc -C target-cpu=native --print cfg | grep target_feature`. BENCHMARKS.md documents all
  three builds.
- **Comparison procedure on an M6.** The same commands as §11.5, with Python baselines measured on
  that M6, and `best-workers` candidates extended to 6 (super + performance) and 12 (all).
  Results go to a separate BENCHMARKS.md section labelled with the machine. No M6 number is
  written anywhere until it has been measured on an M6.

### 11.7 GPU (Metal) evaluation

The decision is made by measurement and recorded in BENCHMARKS.md. From the Rust profile, take
the share of the only batchable stage: leaf evaluation, 1.5% in Python, plus policy scoring, 6%.
By Amdahl's law, even an infinitely fast GPU removes at most that share.

- The reference search cannot batch leaves at all: each iteration's selection depends on the
  previous backup (`ai/mcts.py:135-151`).
- One Metal dispatch costs tens of microseconds, against a sub-microsecond CPU evaluation.

Unless the profile shows a batchable stage above 10% of the strengthened search's time, no Metal
code is written; it would also need `unsafe` Objective-C bindings. BENCHMARKS.md then gives the
measured share and the bound. If Phase 6 adopts a larger evaluation model with leaf batches,
experiments run outside the repository, and any adopted path stays outside the reference search.

### 11.8 Nothing timing-related inside the crates

- Lint §9.9 item 2 and the `clippy.toml` bans enforce it. There are no `[[bench]]` targets.
- `timing.json` is written by `gcg-runner/src/clock.rs` only, for CLI parity, and nothing reads
  it back.
- `gcg-eval preset-benchmark` only runs a pipeline; hyperfine times it outside.
- The Phase 7 reviewer re-checks all of this by grep (§13 M16).

## 12. Phase 6: weaknesses and the strengthened search

### 12.1 Classes of the 164 items

The resolution types come from the WEAKNESSES.md summary table (`docs/dev/WEAKNESSES.md:95-260`).

| Class | Meaning | Packages | Items |
| --- | --- | --- | --- |
| S | Resolution type contains "AI (strengthened search)". A mechanism in `gcg-search` only. It needs a scenario test where scenario-shaped, plus individual and leave-one-out ablation rows | P6-A … P6-H | 43 |
| T | Evaluation tooling and methodology (resolution type "eval tooling" without an AI mechanism, plus W-150 and W-152). Prerequisites for measuring S | P6-I | 12 |
| N | Engine, effects, data, validator, reporting, runner, tooling, docs and design items. They must not change rules, card behaviour, data or any reference-equivalent output (additive outputs only; opt-in options only in `gcg-eval` subcommands, A30). Evidence is tests, and the ablation row says "no AI effect" with the evidence of A13 | P6-J … P6-O | 109 |

Scenario-shaped items (verification column contains "scenario", 17): W-01, W-03, W-05, W-07,
W-09, W-12, W-13, W-14, W-16, W-17, W-24, W-25, W-28, W-29, W-32, W-45, W-139.

Items whose verification names an ablation (30): W-01, W-03, W-04, W-05, W-06, W-07, W-09, W-11,
W-12, W-14, W-16, W-17, W-23, W-24, W-25, W-26, W-27, W-28, W-30, W-31, W-34, W-35, W-37,
W-38, W-45, W-138, W-139, W-140, W-141, W-142. Every other class-S item gets its individual and
leave-one-out rows as well (A13).

### 12.2 Work packages and the mapping of all 164 items

| Pkg | Class | Items | Mechanisms (config flags) or deliverables | Scenario tests | Depends on |
| --- | --- | --- | --- | --- | --- |
| P6-A horizon and leaf | S | W-01, W-02, W-15 | `horizon_turns` (start 3 for `standard`), `adaptive_horizon`, leaf constants `floor` and `decay = floor / (max_horizon + max_extension + 1)` with the constructor check, `capped_rollout_continuation` | W-01 | P6-B (unsettled-leaf test), P6-G (refit at the new leaf) |
| P6-B tactical solver | S | W-03, W-16, W-17, W-32, W-38 | `tactical_solver` (bounded exact attack-step solver on the determinised board), `lethal_root`, `threat_leaf`, `critical_extension`, `effective_blockers`, `solver_can_kill` | W-03, W-16, W-17, W-32 | — |
| P6-C hidden information | S | W-05, W-07, W-08, W-134, W-139 | `opponent_buckets{m}`, `belief_worlds{m, last_n}`, `exact_pools`, `opponent_view_share`, `opponent_list: Known\|Archetype` (opt-in) | W-05, W-07, W-139 | P6-F opponent model |
| P6-D tree and budget | S | W-04, W-06, W-11, W-18, W-138, W-140 | `root_trees{K}`, `visited_set_backup`, `in_tree_filters`, `transpositions`, `subtree_reuse`, `macro_multi_pick`, `prior_worlds{K}`; the `fast` budget mapping from reference `standard` and the `gcg-eval fidelity` check (W-18) | — | P6-I `budget-curve`, `fidelity`, `regret` |
| P6-E root filters and targets | S | W-09, W-10, W-12, W-13, W-14, W-141 | `ex_variants`, `typed_fingerprint`, `prune_worlds{K}`, `branch_all_selects`, `rest_prune_worlds`, `target_rollouts`, `real_idle_line` | W-09, W-12, W-13, W-14 | — |
| P6-F rollout policy | S | W-23, W-24, W-25, W-26, W-27, W-28, W-29, W-30, W-31, W-163 | `calibrated_policy`, `modal_lookahead`, `yes_no_lookahead`, `end_phase_actions`, `burst_decline_list`, `order_trigger_scorer`, `exhaustive_polarity`, `exact_fight`, `opponent_knobs`, `kill_aware_targeting` (re-measured; dropped if null) | W-24, W-25, W-28, W-29 | — |
| P6-G evaluation | S | W-34, W-35, W-36, W-37, W-39, W-142 | `refit_weights` (from `gcg-eval tune`), `phase_weights`, `keyword_features`, `ability_count_features`, `hand_slots`, `burst_shield_probability`, `no_tempo_feature`, `determinized_fit`, `leaf_density` | — | P6-A, P6-B (fit at the final leaf point), P6-I |
| P6-H pre-game | S | W-44, W-45 | `exclude_capped_play_draw`, `paired_mulligan` | W-45 | P6-F |
| P6-I eval tooling | T | W-19, W-20, W-21, W-22, W-40, W-41, W-42, W-43, W-123, W-127, W-150, W-152 | `head-to-head` paired statistics; `ablation`, `budget-curve`, `fidelity`, `regret`, `tune` (CV by game, λ by CV, held-out check, W-40), `calibrate` (W-41 is resolved by the tool, its schema and the synthetic-fixture e2e that its verification names; calibrating against real tournament data is a follow-up once the user supplies data, asked at M1), `compare-runs`, `transcripts`, `gates`; deck panel with overlap bounds; seed roles and ledger; evidence directory; determinism e2e for every preset at 1/4/10 workers; seat-symmetry e2e | — | Phase 4 |
| P6-J engine hardening | N | W-46, W-47, W-48, W-49, W-50, W-51, W-52, W-53, W-54, W-124, W-125, W-143, W-158, W-159, W-161, W-162 | cap kind in diagnostics (`gcg-eval benchmark --diagnostics`, A30); `EngineError::NoProgress`; fixpoint flag; private state + `paranoid`; W-50: the engine's listened set and W-51: its HP flag follow Python's formulas exactly (§4.2 `GameConst`), and the per-game refinements are checks, not engine values: an IR-derived `always` set must be a subset of Python's (W-50), a per-game HP flag computed from the game's cards drives a `paranoid` check that every skipped rules scan equals the full scan (W-51), and testkit cards added after start extend the listened set (W-50); P6-J records this narrowing of the W-50/W-51 plan text in both entries; value clones; content-addressed saved states; dead variants documented; facet masks (P5); `RecursionError` for redirect cycles; registry-build rejection of non-literal prevention thresholds; tripwires; LKI side table | — | Phase 3 (most hold by construction) |
| P6-K rules, effects and data preservation | N | W-55, W-56, W-57, W-58, W-59, W-60, W-61, W-62, W-63, W-64, W-65, W-66, W-67, W-68, W-69, W-70, W-79, W-80, W-81, W-82, W-83, W-84, W-85, W-86, W-87, W-88, W-89, W-116, W-145, W-146, W-155, W-157, W-164 | data tripwires and negative controls; interpretation ids on ported tests; pilot-routing audit table; parenthetical gate (W-155, Rust side; it runs in the refresh workflow from M17); miscompile pattern scan over the IR with a tagged Rust test and a port-owned Python test (`oracle/tests/`) for every reviewed hit (W-66); W-68 compiled-vs-bound comparison on HP/damage states; data-driven loader; `gcg-eval data-status --freshness` and `--sources` (W-145, W-88, A30); rules-text gate; fail-fast overrides; synthetic-data tests for dead loader paths; W-63/W-67 general templates in `oracle/templates/`, proven golden-identical by `export_ir.py` (§12.11, T-154), with `gcg-eval data-status --templates` reporting the bindings they reproduce | — | Phase 4 |
| P6-L validator | N | W-71, W-72, W-73, W-74, W-75, W-76, W-77, W-78, W-144 | starter-deck exception data hook; dated/regional B&R model with `gcg-eval validate --as-of`, `--region`, `--no-banlist`, `--event-date` and `--scope` (data unchanged; `gcg-sim validate` unchanged, A30); tripwires; scope reporting; parser hints; `u64` counts | — | A |
| P6-M reporting and runner | N | W-90, W-91, W-92, W-93, W-94, W-95, W-96, W-97, W-98, W-99, W-100, W-101, W-102, W-103, W-104, W-105, W-106, W-107, W-108, W-109, W-147, W-148, W-149, W-156 | all opt-in options in `gcg-eval benchmark` (A30; T-153: no option = `gcg-sim benchmark`'s bytes): `--analysis` → `analysis.json` (stratified rates, BO3 clustering, hypothesis families, never-played tests, mulligan strata, `--swiss-draws`); cross-AI comparison guard (`--allow-mixed`); `--compress` → `games.ndjson.zst` with the approved zstd encoder, or, if the user refuses the crate (A8), an in-house safe-Rust gzip writer (DEFLATE with fixed Huffman codes) → `games.ndjson.gz`, verified by `gzip -dc` giving the identical bytes, so W-99 is resolved either way; atomic report-set swap; build-time digest; W-104 timing fields; `--resume` checkpoints (written only with `--resume`); thread runner; no factory env var; `gcg-eval replay-game` (W-148); `--replays-extremes`; lowest failing index; `gcg-eval replay-check` with the version comparison (W-156; `gcg-sim replay` output unchanged); W-100 per A23 | — | E |
| P6-N tooling, docs, process | N | W-110, W-111, W-112, W-113, W-114, W-115, W-117, W-118, W-119, W-120, W-121, W-122, W-126, W-128, W-129, W-130, W-151, W-153, W-154, W-160 | strategic fuzz (1,000 games, §9.4); behaviour coverage; fast default tests; step-cap diagnostics in the testkit; doc-command handling for the port docs; research-artefact regeneration; reviewer audit sample; gate script; transcripts port; W-120: the settings change and its check, applied only if the user approves at M1 (otherwise the item is reported as not resolved, with the user's answer); refresh tooling notes; W-122: resolved by the lint of §9.9 item 6 (deleting the bytecode is an optional user decision, asked at M1); throughput tooling outside the crates; generated counts; checked interface docs; W-129 and W-130: the corrections in `docs/*.md` (SPEC, INTERFACES, PROGRESS, `docs/python/AI.md:326-327` after D14a, the research doc) are made in scope, while the `mirrored_specs` docstring (`ai/selfplay.py:95-97`) follows §12.11, and the `src/gcg_sim/data/official/floor_rules.json` wording stays unchanged (packaged data is out of scope) and is listed for the user, as W-130's plan says; executed-program census; W-153: the paging, fetch-all and flagging fix in `.claude/skills/gcg-refresh-data/scripts/fetch_official.py` (outside the package) with recorded-response tests in `oracle/tests/refresh_news/` (T-155) and the Rust gate's news-date line; W-154 lint/doc fixes before the first port commit; W-160 harness | — | G (code), lead (docs, skill script) |
| P6-O design decisions | N | W-33, W-131, W-132, W-133, W-135, W-136, W-137 | W-131, W-132, W-133 are admissible exceptions (no work; recorded). W-33: registry-owned caches. W-135: `Observation` type for the strengthened search, built in its agent adapter (§12.3, lint §9.9 item 5); the entry records D10's deviation (reference and greedy keep `&GameState` for exact cache semantics). W-136: separate config types. W-137: lint, golden platform, portable math for the strengthened search | — | Phase 3 |

Count check: S = 3 + 5 + 5 + 6 + 6 + 10 + 6 + 2 = 43; T = 12;
N = 16 + 33 + 9 + 24 + 20 + 7 = 109; total 164. `gcg-diff weaknesses --check` parses
WEAKNESSES.md and this table. It checks that every id from W-01 to W-164 appears exactly once,
that class S equals the set of rows whose resolution type contains "AI (strengthened search)",
that every scenario-shaped item has a scenario test id, and, from M15 on, that every entry has
a `Status` other than `open` with evidence.

Many class-N items are resolved by construction in Phases 2 and 3: W-33, W-49, W-50, W-51, W-52,
W-53, W-65, W-69, W-106, W-107, W-108, W-125, W-135, W-136, W-137, W-149, W-157 and W-160. The
Phase 2/3 owner implements them as part of the port. The Phase 6 package adds the evidence test
the entry names and records the status.

### 12.3 Strengthened search design

`gcg-search` starts as the v0 copy of the reference. Its null configuration replays the reference
(§10.4 D2). Phase 6 begins by giving the core loop explicit extension points. The P6-D owner
writes them once:

```rust
pub trait Mechanisms {   // every method's default is the reference behaviour
    fn root_moves(&self, cx: &mut RootCx) -> Result<Vec<(MoveKey, Action)>, EngineError>;   // P6-E, P6-B lethal
    fn budget(&self, cx: &RootCx, n_moves: usize) -> Budget;                               // P6-B critical extension, P6-D
    fn world(&self, cx: &mut WorldCx, k: usize, out: &mut GameState) -> Result<(), EngineError>; // P6-C
    fn priors(&self, cx: &mut NodeCx) -> Result<(), EngineError>;                          // P6-D K-world priors
    fn select(&self, cx: &mut NodeCx) -> ChildIdx;                                         // P6-C buckets
    fn rollout_choice(&self, cx: &mut RolloutCx) -> Result<Action, EngineError>;           // P6-F
    fn horizon(&self, cx: &LeafCx) -> HorizonDecision;                                     // P6-A
    fn leaf_value(&self, cx: &mut LeafCx) -> Result<f64, EngineError>;                     // P6-A, P6-B, P6-G
    fn backup(&self, cx: &mut BackupCx);                                                   // P6-D visited set
    fn pregame(&self, cx: &mut PregameCx) -> Result<(Action, Vec<Alternative>), EngineError>; // P6-H
}
```

Each package implements its pieces in `crates/gcg-search/src/mech/<pkg>/`, with one writer per
directory, and its scenario tests in `crates/gcg-search/tests/scenarios/`. The core loop and the
`Mechanisms` composition stay with P6-D. The strengthened search sees an `Observation<'_>` view:
the observer's own cards, public zones and public history. `SearchAgent::choose` in
`crates/gcg-search/src/agent.rs` is the adapter. It receives the true `&GameState` through the
shared `Agent` trait (§6.1), builds `Observation::new(st, player)` at once, and passes only the
`Observation` to the search; it uses `st` otherwise only for `AgentCore::begin` and `finish`
(the decision counter and the log). The `Observation` keeps the true state private: it can
neither name hidden identities nor return the `&GameState` (W-135, compile-fail doctest), and
lint §9.9 item 5 checks the adapter. Worlds are built from the `Observation`.

| Id | Mechanism | Items | Cost accounting (budget = iterations; aux work in engine applies, capped per decision) |
| --- | --- | --- | --- |
| S1 | **Deterministic root ensemble.** K trees per decision. Tree t is seeded with `derive_seed(decision_seed, "tree", t)` and has disjoint world and rollout seed streams. Each tree gets `ceil(B/K)` iterations. Root statistics merge in tree order: visits summed, totals summed with `py_sum` in tree order. The move is the first max of `(visits, mean)`. The close-call test uses the concatenated per-tree paired differences. Trees run through `fork_join`; K is a preset constant | W-04, W-05 | iterations |
| S2 | **Horizon and leaf.** Per-preset `horizon_turns`. Adaptive extension by one turn boundary when the leaf is unsettled: a pending battle or effect, or either side's race estimate ≤ 2, judged from the determinised world only. Generalised leaf constants with the constructor check. Capped rollouts finish the current step with the deterministic policy and are flagged | W-01, W-02, W-15 | rollout decisions count as applies |
| S3 | **Tactical solver and threat leaf.** Exact bounded search of one attack step on the determinised board: engine-correct blockers via `procedures::eligible_blockers` (CANT_BLOCK, NO_BLOCKER_VS), High-Maneuver, Shields, Base HP, AP per attacker, CANT_ATTACK, the opponent's determinised action-step tricks up to a bound. It is used at the root (lethal found in all of the first K worlds is played or gets a dominant prior), at leaves (the time-shaped loss value when the opponent has lethal), for critical-position extension (up to two extra budgets), and for `can_kill`/race features | W-03, W-16, W-17, W-32, W-38 | solver applies, capped |
| S4 | **Opponent modelling.** Opponent nodes split into at most 4 buckets by a deterministic abstraction of the opponent's determinised private state. A bucket drives selection once it has `m` visits (start 8, sweep {4, 8, 16, 32}). A per-iteration visited set backs a node up once per iteration (W-06). Opponent selection does not condition on the searcher's private cards: a share of worlds re-determinises the searcher's own hidden cards from the opponent's view (W-139). In-tree priors average K worlds (W-140) | W-05, W-06, W-25, W-139, W-140 | — |
| S5 | **Belief-weighted determinisation.** M candidate worlds per needed world, weighted by the likelihood of the opponent's last N public decisions under the opponent model. Systematic resampling uses the tree seed. Exact pools keep card conservation (W-08; the reference keeps `_fit` and gains a pad/trim counter). Opt-in `--opponent-list archetype` of `gcg-eval benchmark` and the agent-spec field `search:<preset>:opponent_list=archetype` (W-134, A30) | W-07, W-08, W-134 | candidate replays count as applies |
| S6 | **Root moves.** EX-payment variants kept when the resulting resource states differ in a way the follow-up line uses (an EX-sensitivity table from the IR). Uid-independent typed fingerprints. Pruning judged by majority over K common-random-number worlds with a fail-safe prior penalty. The no-effect test compares against an idle line that cannot replay the move. Branches over every SELECT up to a node cap. Target rollouts for close target choices | W-09, W-10, W-12, W-13, W-14, W-141 | trial applies |
| S7 | **Reuse.** The subtree of the observed transition is kept for the next own decision in the same turn. Transpositions within a decision are keyed by `stable_hash` of (the decider's observation, the public state) | W-11 | — |
| S8 | **Multi-pick macro moves.** Joint selection of small multi-card choices (≤ 3 cards, ≤ 20 combinations) | W-138 | — |
| S9 | **Policy.** A softmax rollout policy over per-option features (current scorer outputs, card values, engine facts). Its weights are fitted deterministically (fixed-order IRLS in f64) on Rust self-play with tuning seeds. Modal choices are scored by simulating their first step. Also: an ORDER_TRIGGER scorer; YES_NO and Burst by one-ply look-ahead, with a decline list checked by a data tripwire (W-163); end-phase action-step plays; exhaustive polarity; exact fights on clones; a separately fitted opponent policy; kill-aware targeting re-measured | W-23, W-24, W-25, W-26, W-27, W-28, W-29, W-30, W-31, W-163 | look-ahead applies |
| S10 | **Evaluation.** Refit per preset at the final leaf points on Rust self-play over the panel's tuning decks (W-34, W-36). Features for keywords, ability counts and Burst probability per Shield (W-37). Positions recorded at consistent leaf points (W-39). Burst density recomputed in the leaf world (W-142). A phase-conditioned or small nonlinear model only if its ablation row is positive (W-35). Portable `exp`/`log` from the `libm` crate, so results match across platforms (W-137) | W-34, W-35, W-36, W-37, W-39, W-142 | — |
| S11 | **Pre-game.** Play/draw from paired strengthened-policy simulations, with capped games scored by the evaluation instead of 0.5 (W-44). Mulligan by paired simulation of the opening turns on shared worlds, with 2 paired standard errors (W-45) | W-44, W-45 | simulations count as applies |
| S12 | **`fast` mapping and fidelity.** `fast`'s budget starts from reference `standard` and is tuned (§12.8). `gcg-eval fidelity` checks that `fast` and `standard` rank deck variants alike on the same deals | W-18 | — |

Determinism: every random choice comes from the per-decision `SplitMix64` or from a
`derive_seed` of it. Root-parallel trees merge in index order, and transposition keys use
`stable_hash`. Nothing reads a clock or the thread count. With portable math, the strengthened
search gives identical results on macOS arm64 and Linux x86-64; an optional CI matrix job checks
the tier-1 scenario tests on both.

### 12.4 Ablation rows

- **Individual row.** The reference configuration at the same iteration budget plus only this
  mechanism, against the reference.
- **Leave-one-out row.** The full strengthened bundle against the same bundle without the
  mechanism.
- Both rows run through `gcg-eval ablation` with mirrored blocks of four (W-42) and the forced
  play/draw balance of A24 on the 15 tuning pairs (§12.5) only: 400 games per row stratified
  over the pairs, on dev-role seeds. The AI test decks are not used, so the test-deck gates of
  E3/E4 stay unused by adoption decisions. Promising rows are confirmed with 1,600 games on
  confirm-role seeds, run once. The W-44 row uses `--choose-first agents`, because forced
  balance bypasses the play/draw decision it changes.
- Each row reports wins/N, the Wilson 95% interval, the paired difference with a Newcombe 95%
  interval, McNemar discordant counts, and iterations and applies per game.
- Adoption follows `docs/ASSUMPTIONS.md:92`: decision-level or scenario evidence of a real fix,
  no large harm in a 400-game guard, and the bundle measured at game level. Only mechanisms with a
  measured non-negative effect are kept (brief step 22).
- Class T and class N items get a row reading "no AI effect", with the test that proves their
  behaviour and the evidence of A13. For every class-N item in P6-J, P6-K and P6-M that touches
  an engine or runner code path the AI executes, that evidence is a run of the `search` and
  `baselines` corpus sets and the seed-1 equivalence subset with the item's change on and off
  (identical decision traces), plus the gcg-search scenario tests. For an item built in by
  construction, which has no off switch, the evidence is those sets at 100% against Python's
  traces, which shows that the AI's decisions are Python's. Every other row names the reason no
  AI-executed path changes.

### 12.5 Evaluation decks and the panel

`examples/decks/eval/` gets five new legal decks of distinct archetypes. The examples use
red-green, blue-white and purple-white. Candidates:

| File | Archetype | Role |
| --- | --- | --- |
| `blue-green-tempo.txt` | low curve, Breach, bounce effects | candidate |
| `red-purple-control.txt` | removal Commands, Blockers, late finishers | candidate |
| `green-white-link.txt` | Link Units with their named Pilots | candidate |
| `purple-blue-swarm.txt` | tokens, go-wide, Support | candidate |
| `blue-red-ramp.txt` | EX Resources, high-cost Units | candidate |

A deck-building subagent builds them from the packaged pool. It may change a colour pair if the
pool does not support the archetype. Each deck carries a `#` header naming its archetype and plan
(deck files allow comment lines, `docs/DECK_FORMAT.md`), passes `gcg-sim validate` (B&R
included), and uses only implemented cards (all are). `gcg-eval decks --overlap` computes
main-deck copy overlaps (W-20).

The **two held-out decks** are the candidates with the lowest maximum overlap whose overlap with
every tuning deck (`ai/tuning.py:89-93`) and every AI test deck (`tests/ai/decks.py`) is at most
10/50. If fewer than two qualify, the builder revises decks until two do.

The deck set is 3 examples + 5 new = 8 decks, so `itertools.combinations` gives 28 pairs
(`ai/compare.py:131`). Every pair containing a held-out deck (13 pairs) is a **held-out pair**,
never used in tuning, screening or budget curves. The other 15 are **tuning pairs**.
`docs/dev/eval/panel.json` and `examples/decks/eval/PAIRS.md` record the split, and they are
committed before any tuning run. `gcg-eval` refuses held-out pairs under `--role tuning|dev|confirm`.

### 12.6 Evaluation protocol and gates

- **Games.** BO1 in mirrored blocks of four (`ai/selfplay.py:88-110`): each block uses one game
  seed per deck assignment, and the challenger plays each deck from each seat. The harness
  answers CHOOSE_FIRST with `go_first(0)` (A24, `--choose-first seat0`), so each agent is on the
  play in exactly half the games of every block, with each deck once on the play and once on the
  draw. Every report includes the realised play/draw counts per agent. Runs use
  `gcg-eval head-to-head` with 400 games (100 blocks) per pair. The agent-chosen protocol runs
  only as the W-44 diagnostic and is never a gate.
- **Seeds.** Eval-role masters only (A14). Per pair, the master is
  `derive_seed(master, "pairing", p)` (`ai/compare.py:136-140`). The ledger records each one.
- **Statistics.** Wilson 95% intervals (`reports/stats.py:12-25`, z = 1.96) per pair, pooled over
  all pairs, and pooled over held-out pairs only (A26). Draws count as non-wins, as in Python
  (W-93). Paired Newcombe intervals and McNemar counts are reported where the same deals are
  shared.
- **Final evaluation** (after the budgets are frozen, §12.8). `gcg-eval gates --check`
  machine-checks these gates:

  | # | Comparison | Decks / pairs | Games | Gate |
  | --- | --- | --- | --- | --- |
  | E1 | `search:standard` vs `ref:strong` | panel, 28 pairs | 11,200 | pooled-all Wilson lower bound ≥ 0.55; pooled held-out lower bound ≥ 0.55; no held-out pair with upper bound < 0.5 |
  | E2 | `search:fast` vs `ref:standard` | panel, 28 pairs | 11,200 | pooled point estimate ≥ 0.5 (all and held-out); no pair with upper bound < 0.5 |
  | E3 | `search:standard` vs `random` | AI test decks (`tests/ai/decks.py`, the decks of `tests/slow/test_ai_strength.py`, with eval-role seeds; like for like, not held-out) and panel | 400 + 11,200 | 400/400 on the test decks and in every panel pair, each of the 13 held-out pairs reported on its own, or every loss documented with its replay |
  | E4 | `search:standard` vs `greedy` | AI test decks (like for like with the 0.764 bar, not held-out) and panel | 400 + 11,200 | Wilson lower bound > 0.764 on the test decks; pooled lower bound > 0.764 over all 28 panel pairs **and** over the 13 held-out pairs alone |
  | E5 | scenario tests | 17 items | per test | every test passes |
  | E6 | ablation table | 164 items | — | every item has its rows (`gcg-diff weaknesses --check`) |

- **Surprising results.** These are saved with `--replays-surprising` and inspected, with notes
  in `docs/dev/eval/notes.md` (T-144):
  - every loss to random;
  - every pair outside the 95% interval of the pooled rate;
  - every pair with a point estimate below 0.5;
  - the five largest paired swings.
- **Phase 7.** The reviewer reruns E1-E4 on review-role seeds (§13 M16).

### 12.7 Results schema 1.2.0 (`gcg-sim` only)

`gcg-sim benchmark` writes:

- `schema` `urn:gcg-sim:schema:results:1.2.0`, `schema_version` `1.2.0` and
  `versions.results_schema` `1.2.0`;
- `ai.preset` from the enum `["fast", "standard"]`;
- a new required `ai.search` object: `algorithm` `"gcg-search"`; `mapping` `"reference strong"`
  or `"reference standard"`; `budget` `{iterations_per_option, min_iterations, max_iterations,
  aux_apply_cap}`; `mechanisms` as a sorted list of enabled flags;
- `ai.effort`: totals per seat of `iterations`, `applies_tree`, `applies_rollout` and
  `applies_aux`;
- `summary.md`: one added "search" line, and the W-100 reproduce line (A23).

The schema file is `crates/gcg-runner/schema/results.v1_2.schema.json`: the 1.1.0 constraints
plus these fields, still with `additionalProperties: false`. Both schema files are embedded, and
each binary validates against its own version before writing. `gcg-eval reference-benchmark`
keeps 1.1.0 byte-identically. Replay files are unchanged (they re-simulate actions, independent of
the agent), so `gcg-sim replay` verifies replays from both binaries.

### 12.8 Budget tuning under the 2x floor

1. With the adopted bundle at the starting budgets, `gcg-eval budget-curve --role tuning`
   measures `search:standard` vs `ref:strong` and `search:fast` vs `ref:standard`. The scales are
   0.25x, 0.5x, 1x, 2x, 4x and 8x of the starting `(ipo, min, max)` (and K for S1), on the tuning
   pairs, 400 games per point.
2. For each candidate, `gcg-eval preset-benchmark --search-config FILE` runs the `gcg-sim`
   pipeline. The external script times it with `part2 --candidate FILE`, outside the crates.
3. **`standard`.** Choose the largest scale whose Part 2 full-search time (`--matches 20` at the
   best worker count) is at most Python `strong`'s median / 2.2, a 10% margin over the floor.
   If the budget curve shows no gain past a smaller scale, take that knee instead.
4. **`fast`.** Choose the smallest scale whose pooled point estimate vs `ref:standard` stays
   ≥ 0.53 on the tuning pairs, with no pair upper bound below 0.5 (a margin over the 0.5 bar).
   Then confirm ≥ 2x in the headline (best and 10 workers) and single-thread Part 2
   configurations against Python `standard`. If that confirmation fails, the next candidate is
   the next smaller scale, then the adopted bundle without its costliest mechanism (by applies
   per game in its ablation row), and so on, each candidate measured for both constraints, until
   one holds both the strength rule and the 2x floor. If none does, the conflict is reported in
   BENCHMARKS.md, Rust `docs/AI.md` and the final report with the measured numbers, and the gate
   fails; the search is never weakened below the strength rule to meet the floor.
   **`standard`** follows the same rule from step 3: if no scale both meets the 2x floor and
   passes the E1 bar on the tuning pairs, the conflict is reported the same way.
5. The chosen constants go into `gcg-search`'s presets and into the Rust `docs/AI.md` table, with
   the mapping to the reference presets (W-129 test). The final evaluation (§12.6) and the final
   Part 2 run with `gcg-sim` itself, only after the constants are fixed.

### 12.9 Order and parallelism of Phase 6

1. P6-I first (measurement tools, the panel, the seed ledger), together with P6-D's extension
   points (M11).
2. In parallel, each in its own worktree:
   - P6-J, P6-K, P6-L, P6-M, P6-N and P6-O (class N, files disjoint from `gcg-search`);
   - AI wave 1: P6-E, P6-F, P6-D mechanisms and P6-H (M12, M13).
3. AI wave 2: P6-B; then P6-C (uses P6-F's opponent model) and P6-A (uses P6-B); then P6-G last,
   because it fits at the final leaf point (M14).
4. The lead integrates the bundle, runs leave-one-out rows for every adopted mechanism, tunes the
   budgets (§12.8), and runs the final evaluation (§12.6) and Part 2 (M15).

Every Phase 6 package gate includes `gcg-diff corpus --tier 1 --require-all` and a reference
equivalence smoke (one configuration per reference preset), so no package can change the
reference.

### 12.10 Recording outcomes in WEAKNESSES.md

Each entry's `Status` becomes `resolved` (or `admissible exception` for W-131 to W-133). It is
followed by `Evidence:` (tests.json ids; scenario results with the reference's recorded count and
the strengthened count) and `Ablation:`. The ablation part gives the individual and leave-one-out
rows: wins/N, the Wilson interval, the paired difference with its Newcombe interval, discordant
counts, iterations and applies per game, and the evidence file in `docs/dev/eval/`. A mechanism
measured null or negative is recorded with its rows and left out of the presets; the item is still
resolved by the measurement and the documented decision.

Some entries need more than a status line:

- An entry whose resolution plan names a `gcg-sim` flag records where the option lives instead
  (A30).
- W-41 is resolved by its own verification (the `calibrate` tool, schema and synthetic-fixture
  e2e). Real-data calibration is listed as a follow-up.
- W-122 is resolved by its grep check (lint §9.9 item 6). Deletion stays a user option.
- W-120 depends on the user's M1 answer. If the user declines the settings change, the entry
  stays open with the answer quoted, and the final report says so.
- The entries of §12.11 record what was done in scope and the post-acceptance steps, as the
  user's M1 answer decides.
- W-50, W-51 and W-135 record the deviations from their plan text (§12.2 P6-J, D10).

No entry is closed silently.

### 12.11 Items whose full resolution edits the Python package

The brief freezes `src/gcg_sim` and `tests/` until acceptance (A15), but six items have a part
that would edit them. The plan resolves the rest of each item in scope and puts the package part
to the user at the M1 gate (A32 item 4).

| Item | Done in scope (evidence) | Package part |
| --- | --- | --- |
| W-63, W-67 | General templates for the compile-error families (W-63) and for "all players each …" and the listed single-player wordings (W-67) live in `oracle/templates/` (Python, outside the package; it imports the DSL classes read-only). `export_ir.py` runs them on every binding card whose text a template fully covers and asserts that `repr` of the template's `CardScript` equals the binding's, byte for byte. The IR stays byte-identical except for the new `cards[].template` field; `source` keeps Python's value, so `data status` stays equal to Python's (T-038). `gcg-eval data-status --templates` reports, per family, how many binding cards a general template reproduces: the "binding count falling" of W-63's verification. Golden files, the corpus and the equivalence runs show no behaviour change (T-154) | Moving the templates into `effects/compiler/` and retiring the reproduced bindings, then `export_ir.py --write` with an empty golden diff. W-67's single "each player in turn order" DSL node would need a Python DSL and interpreter change, so in scope the template emits the binding's existing node composition |
| W-66 | The IR pattern scan for every miscompile family runs in the Rust tests and lists zero unreviewed hits. Each reviewed hit has a tagged Rust testkit test and a port-owned Python test in `oracle/tests/` that runs the same scenario on the Python testkit ("passing in both implementations") | A real miscompile is never fixed in the port, because card behaviour must not change. It is reported to the user with evidence (brief: "report it with evidence"), and its fix is a binding change |
| W-68 | Rust maps `Stat.HP` and `REMAINING_HP` to one implementation. For each HP-fix binding card, the compiled script (produced by the Python compiler from `oracle/`) and the bound script give identical results on a set of HP/damage states, a Rust test | Retiring the redundant bindings and their stale notes |
| W-129 | Every correction in `docs/*.md` (P6-N) | The `mirrored_specs` docstring (`ai/selfplay.py:95-97`) |
| W-155 | The Rust parenthetical gate with its allowlist and snapshot (P6-K). From M17 it runs in the refresh workflow, so a reworded semantic parenthetical fails the refresh even though `classify_effect` calls it typography | The `parenthetical: review` class in `tools/refresh.py`'s diff |
| W-130 | The `docs/*.md` wording (P6-N) | None. The `floor_rules.json` wording is packaged data, which is out of scope, and is listed for the user |

The M1 question offers three answers:

- **(a) Recommended.** Accept the in-scope parts as each item's resolution, and list the package
  parts as post-acceptance steps. They run under `/gcg-refresh-data`'s binding workflow once the
  package is no longer the frozen oracle (§3.2: it stays as the IR generator).
- **(b)** Grant an exception to edit `src/gcg_sim` for exactly these parts during Phase 6. Each
  edit then has to leave `uv run pytest -n 8` and `scripts/verify_all.sh` green, regenerate the
  IR with an empty golden diff, and leave the corpus unchanged; A15's gate check gets the
  exception's file list.
- **(c)** Record the package parts as open at acceptance.

Until the user answers, the plan works on the in-scope parts, which all three answers need. Under
(a) each entry gets `Status: resolved` plus a `Post-acceptance:` line naming the package step and
the user's recorded answer. Under (c) the entries stay `open`, `gcg-diff weaknesses --check` fails
on them, and the final report states that T-007 is not met for them.

## 13. Milestones, commits and gate commands

Each milestone ends with one commit on `rust-port` (message below) and an updated
`docs/port/progress.md` gate entry. The entry records what was verified (commands and outcomes),
what remains, and the risks to the acceptance criteria. The same commit updates
`docs/dev/tests.json` from the gate outputs (§9.10). There is no push without the user's
approval.

The common Rust gate **RG** runs at every milestone from M2 on:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --release --locked --offline
target/release/gcg-diff lints
git diff --quiet d9d41af -- src/gcg_sim tests          # Python reference untouched (A15) ...
test -z "$(git status --porcelain --untracked-files=all -- src/gcg_sim tests)"   # ... and nothing added
git status --porcelain --untracked-files=all            # temp-file sweep: only the milestone's intended files
```

The last command's output is reviewed before the commit. Any scratch script, iteration output or
temporary file the milestone created is removed, not committed (brief working conventions; T-147).
Under answer (b) of §12.11, the two A15 checks exclude exactly the excepted files.

| M | Phase | Content | Commit message |
| --- | --- | --- | --- |
| M0 | 0 (gate) | inventory, weaknesses, Python baseline in BENCHMARKS.md, `gcg-core`; W-154 fixes (ruff on `oracle`, `doc-check: skip` markers on the shell blocks of `docs/port/inventory/10-tests.md` and `11-python-profile.md`) | `port: phase 0 discovery, inventory, weaknesses and Python baseline` |
| M1 | 1 (gate) | `docs/dev/PLAN.md`, `docs/dev/tests.json` | `port: phase 1 plan and test register` |
| M2 | 2 | contracts (S), `clippy.toml`, `.cargo/config.toml`, B1 IR and registry, vector exporters | `port: phase 2a contracts and effect IR` |
| M3 | 2 (gate) | G exporters, tier-1 corpus, tier-2 manifest, `gcg-diff` (parse-only, MockEngine, negative controls), W-160 harness, probe hook | `port: phase 2b differential oracle and trace corpus` |
| M4 | 3 | A data layer | `port: data layer, deck parser and validator` |
| M5.1 | 3 | C L1 state, canonical JSON, digest | `port(engine): state, canonical JSON and digest` |
| M5.2 | 3 | B2 L2 derived view, expressions, pure hooks | `port(engine): derived view and expressions` |
| M5.3 | 3 | C L3 core and battle | `port(engine): zones, events, triggers, damage and battle` |
| M5.4 | 3 | B2 L4 interpreter, step hooks, system programs | `port(engine): effect interpreter and hooks` |
| M5.5 | 3 (engine gate) | C L5 game, observe, invariants, testkit | `port(engine): turn procedure, legal actions, observe and invariants` |
| M6 | 3 | D1 + D2 AI, `gcg-search` v0 | `port: reference ISMCTS, greedy and random` |
| M7 | 3 | E runner, reports, CLI | `port: runner, reports and gcg-sim CLI` |
| M8 | 3 (gate) | F eval harness, first full `verify_all_rust.sh` | `port: gcg-eval harness (phase 3 complete)` |
| M9 | 4 (gate) | tier 2, equivalence matrix, probe, replays, traceability, coverage, EQUIVALENCE.md | `port: phase 4 byte equivalence` |
| M10.n | 5 | one commit per kept optimisation group | `port(perf): <change>` |
| M10 | 5 (gate) | Part 1, BENCHMARKS.md, M6 notes, GPU decision | `port: phase 5 performance and part 1 comparison` |
| M11 | 6 | P6-I tooling, P6-D extension points, evaluation decks, panel, seed ledger; `git mv docs/AI.md docs/python/AI.md`, the citation rewrite and the Rust `docs/AI.md` skeleton (D14a) | `port: phase 6 evaluation tooling and deck panel` |
| M12.J … M12.O | 6 | one commit per class-N package | `port: weaknesses <pkg> (<ids>)` |
| M13.E, .F, .D, .H | 6 | AI wave 1, one commit per package | `port: strengthened search <pkg> (<ids>)` |
| M14.B, .C, .A, .G | 6 | AI wave 2, one commit per package | `port: strengthened search <pkg> (<ids>)` |
| M15 | 6 (gate) | bundle, budget tuning, final evaluation, Part 2, schema 1.2.0, WEAKNESSES statuses | `port: phase 6 presets tuned and strength evaluation` |
| M16 | 7 (gate) | independent review, fixes, REVIEW.md | `port: phase 7 independent review` |
| M17 | 8 (gate) | documentation, CLAUDE.md, skill, CI | `port: phase 8 documentation and hand-off` |

### M0 (Phase 0 gate; the lead's, in progress)

```text
uv sync --locked
uv run ruff check . && uv run ruff format --check . && uv run mypy
uv run pytest -n 8 -q                                   # 3985 passed (recorded in progress.md)
uv run python scripts/check_doc_commands.py --list docs/dev/**/*.md   # no runnable block left unmarked
cargo test -p gcg-core --release --locked --offline     # 18 tests pass
scripts/compare_python_vs_rust.sh env
scripts/compare_python_vs_rust.sh best-workers python
scripts/compare_python_vs_rust.sh python --best <N>
```

The Python baseline (median, min and max per configuration, with `env.txt`) and the profile that
confirms where time goes (Inv 11: cProfile and py-spy shares) are recorded in
`docs/dev/BENCHMARKS.md` (T-136).

### M1 (Phase 1 gate)

```text
python3 -c "import json; d = json.load(open('docs/dev/tests.json')); assert all(set(e) == {'id','name','status'} for e in d)"
uv run python scripts/check_doc_commands.py --list docs/dev/PLAN.md   # lists no bash/sh/shell block
```

Then the lead's review against `docs/port/BRIEF.md`, and the M1 user batch of A32: the
dependency fetch (A8), the SIGINT fallback sign-off if needed (A3), W-120, the §12.11 question,
and the optional W-41 data and W-122 deletion. The answers are recorded in `docs/port/progress.md`
and in A32 before M2 starts. If (1) is approved, `cargo fetch --locked` runs once after the
crates are added to the manifests at M2.

### M2

```text
RG
uv run python oracle/export_ir.py --check
uv run python oracle/export_vectors.py --check
cargo test --release --locked --offline -p gcg-core -p gcg-effects     # ir_current, content ids, golden, counts, repr, fingerprints
uv run ruff check oracle && uv run ruff format --check oracle && MYPYPATH=src uv run mypy --strict oracle
```

### M3 (Phase 2 gate)

```text
RG
PYTHONHASHSEED=1 uv run python oracle/traces.py --check --tier 1
PYTHONHASHSEED=2 uv run python oracle/traces.py --check --tier 1
uv run python oracle/traces.py --check --tier 1 --nocache         # cache transparency (§8.5)
uv run pytest tests/rules tests/cards tests/effects tests/ai -q -p oracle.pytest_trace \
  --trace-check oracle/corpus/v1/tier1/tests
uv run python oracle/census.py --check
cargo test --release --locked --offline -p gcg-diff                    # MockEngine replay, synthetic negative controls
target/release/gcg-diff corpus --tier 1 --parse-only                   # every record decodes; manifest and IR hashes
scripts/compare_python_vs_rust.sh equivalence --selftest               # W-160 normaliser negative controls
```

### M4

```text
RG
uv run python oracle/export_data.py --check
uv run python oracle/export_unicode.py --check
cargo test --release --locked --offline -p gcg-data
```

### M5.1 … M5.4 (engine layers)

```text
RG
target/release/gcg-diff corpus --tier 1 --roundtrip --require-all                  # M5.1: every snapshot round-trips, digests equal
target/release/gcg-diff corpus --tier 1 --only views --require-all                 # M5.2
cargo test --release --locked --offline -p gcg-engine                               # M5.3, M5.4: unit tests and primitive vectors
```

### M5.5 (engine gate)

```text
RG
cargo test --release --locked --offline -p gcg-engine --features paranoid,testkit
target/release/gcg-diff selftest
target/release/gcg-diff corpus --tier 1 --only full,vanilla,fuzz,coverage,tests,views,worlds --require-all
cargo build --release --locked --offline -p gcg-diff --features paranoid --target-dir target/paranoid
target/paranoid/release/gcg-diff corpus --tier 1 --only full,vanilla,fuzz,coverage,tests,views,worlds --require-all --paranoid
target/release/gcg-diff fuzz --games 10000 --seed 2026 --workers auto \
  --compare oracle/corpus/v1/tier1/fuzz/summary-2026-10000.json
target/paranoid/release/gcg-diff fuzz --games 16 --seed 11 --paranoid
```

### M6

```text
RG
target/release/gcg-diff corpus --tier 1 --only components,baselines,search --require-all
cargo test --release --locked --offline -p gcg-ai -p gcg-ai-reference -p gcg-search   # vectors, baseline digest, puzzles, info sets, null config
```

### M7

```text
RG
cargo test --release --locked --offline -p gcg-runner -p gcg-cli      # records → reports, transcripts, pool, interrupt, presets, reachability
```

### M8 (Phase 3 gate)

```text
RG
target/release/gcg-eval reference-benchmark examples/decks/red-green-zeon.txt \
  examples/decks/blue-white-federation.txt --reference-preset fast --matches 2 --seed 1 --out build/m8
for f in build/m8/replays/*.json; do target/release/gcg-sim replay "$f"; done
scripts/compare_python_vs_rust.sh equivalence --matches 4 --seeds 1 --formats "bo1 bo3" \
  --presets "fast standard strong" --workers "1 4"                    # first e2e smoke
cargo test --release --locked --offline -p gcg-eval --features slow --test strength_reference
scripts/verify_all_rust.sh
```

### M9 (Phase 4 gate)

```text
scripts/verify_all_rust.sh --full                                     # includes tier 2
cargo build --release --locked --offline -p gcg-diff --features paranoid --target-dir target/paranoid
target/paranoid/release/gcg-diff corpus --tier 2 --require-all --paranoid
PAIRS="red-green-zeon:blue-white-federation purple-white-tekkadan:red-green-zeon blue-white-federation:purple-white-tekkadan"
for log in "" "--decision-log"; do
  scripts/compare_python_vs_rust.sh equivalence $log --matches 10 --seeds "1 2 3" \
    --formats "bo1 bo3" --presets "fast standard strong" --workers "1 4 10" --pairs "$PAIRS"
done
scripts/compare_python_vs_rust.sh equivalence --matches 100 --seeds 1 --formats bo3 \
  --presets standard --pairs red-green-zeon:blue-white-federation --workers "1 4 10"
scripts/compare_python_vs_rust.sh equivalence --offset-probe 1000 --matches 10 --seeds "1 2 3" \
  --formats "bo1 bo3" --presets "fast standard strong" --workers 1 --pairs "$PAIRS"
for f in build/comparison/equivalence/*/rust-w1/replays/*.json; do .venv/bin/gcg-sim replay "$f" >/dev/null; done
for f in build/comparison/equivalence/*/python/replays/*.json; do target/release/gcg-sim replay "$f" >/dev/null; done
mkdir -p build/cmp; i=0
for cfg in "fast greedy --games 48 --seed 11 --decks tests" \
           "standard random --games 24 --seed 12 --decks examples" \
           "baseline-standard fast --games 24 --seed 13 --decks tuning"; do
  i=$((i + 1))
  uv run python -m gcg_sim.ai.compare $cfg --workers 4 --out "build/cmp/py$i.json" > "build/cmp/py$i.stdout"
  target/release/gcg-eval compare $cfg --workers 4 --out "build/cmp/rs$i.json" > "build/cmp/rs$i.stdout"
  cmp "build/cmp/py$i.json" "build/cmp/rs$i.json" && cmp "build/cmp/py$i.stdout" "build/cmp/rs$i.stdout"
done
RUSTFLAGS="-C target-cpu=apple-m2" cargo build --profile dist --locked --offline
target/dist/gcg-diff corpus --tier 1 --require-all                    # codegen independence
target/release/gcg-diff traceability --check
target/release/gcg-diff coverage --check
```

### M10 (Phase 5 gate)

```text
scripts/verify_all_rust.sh
scripts/compare_python_vs_rust.sh best-workers rust-reference
scripts/compare_python_vs_rust.sh part1 --best <N>
scripts/compare_python_vs_rust.sh report                             # ≥ 2x in every Part 1 configuration
scripts/compare_python_vs_rust.sh equivalence --matches 10 --seeds 1 --formats "bo1 bo3" \
  --presets "fast standard strong" --workers "1 10"                   # optimised build still identical
RUSTFLAGS="-C target-cpu=apple-m2" cargo build --profile dist --locked --offline
target/dist/gcg-diff corpus --tier 1 --require-all
```

After every kept M10.n change: `target/release/gcg-diff corpus --tier 1 --require-all` and the
seed-1 equivalence subset above. BENCHMARKS.md at M10 also holds the Instruments Allocations
result for the reference search (T-104), the allocator, PGO, NEON and GPU records (T-105), and,
if Part 1 is below 10x, the profile-based explanation (T-137).

### M11-M14 (each Phase 6 package)

```text
RG
target/release/gcg-diff corpus --tier 1 --require-all
scripts/compare_python_vs_rust.sh equivalence --matches 4 --seeds 1 --formats bo1 \
  --presets "fast standard strong" --workers 4                         # reference untouched
# class S packages, per mechanism flag:
cargo test --release --locked --offline -p gcg-search --features slow -- scenarios::<pkg>
target/release/gcg-eval ablation --mechanism <flag> --rows individual,leave-one-out \
  --pairs tuning --games 400 --role dev --workers auto --out docs/dev/eval/ablation/<flag>.json
```

### M15 (Phase 6 gate)

```text
scripts/verify_all_rust.sh
target/release/gcg-eval budget-curve --pairs tuning --role tuning --games 400 --workers auto \
  --out docs/dev/eval/budget-curve.json
scripts/compare_python_vs_rust.sh best-workers rust-presets
scripts/compare_python_vs_rust.sh part2 --best <N>
scripts/compare_python_vs_rust.sh report                             # ≥ 2x in every Part 2 configuration
E=<eval-role master from docs/dev/eval/seed-ledger.json>
target/release/gcg-eval head-to-head search:standard ref:strong --decks panel --pairs all --games 11200 \
  --seed "$E" --role eval --workers auto --replays-surprising --out docs/dev/eval/final/e1.json
target/release/gcg-eval head-to-head search:fast ref:standard --decks panel --pairs all --games 11200 \
  --seed "$E" --role eval --workers auto --replays-surprising --out docs/dev/eval/final/e2.json
for opp in random greedy; do
  target/release/gcg-eval head-to-head search:standard "$opp" --decks tests --games 400 \
    --seed "$E" --role eval --workers auto --replays-surprising --out "docs/dev/eval/final/e-$opp-tests.json"
  target/release/gcg-eval head-to-head search:standard "$opp" --decks panel --pairs all --games 11200 \
    --seed "$E" --role eval --workers auto --replays-surprising --out "docs/dev/eval/final/e-$opp-panel.json"
done
cargo test --release --locked --offline -p gcg-search --features slow   # every scenario test
target/release/gcg-eval gates --check docs/dev/eval/final              # §12.6 E1-E4, all pairs and held-out pairs
target/release/gcg-diff weaknesses --check docs/dev/WEAKNESSES.md      # E6: every item resolved or admissible, with evidence
```

The head-to-head runs use the default `--choose-first seat0` (A24). M15 also records the
Instruments Allocations check on `gcg_search` (T-148) and the inspection notes of the saved
surprising-result replays in `docs/dev/eval/notes.md` (T-144). The authors' `report` output sets
T-100 to T-103; T-011 is set only by the reviewer's rerun at M16.

### M16 (Phase 7 gate; a reviewer subagent that wrote none of the code)

```text
REVIEW_DIR="$(mktemp -d)"
git clone --branch rust-port "$REPO_PATH" "$REVIEW_DIR/repo"           # REPO_PATH: this repository's path
cd "$REVIEW_DIR/repo"
scripts/verify_all_rust.sh --full
scripts/verify_all.sh                                                   # the Python gate still passes (W-154)
scripts/compare_python_vs_rust.sh equivalence --matches 10 --seeds "<3 review-role seeds>" \
  --formats "bo1 bo3" --presets "fast standard strong" --workers "1 4 10"
# E1-E4 of §12.6 with --role review and fresh review-role masters, then:
target/release/gcg-eval gates --check docs/dev/eval/review
# Performance comparison (brief step 26), on a quiet machine, against the recorded Python
# baselines of A28 (COMPARE_OUT points at the directory that holds them):
scripts/compare_python_vs_rust.sh env
scripts/compare_python_vs_rust.sh best-workers rust-reference
scripts/compare_python_vs_rust.sh best-workers rust-presets
scripts/compare_python_vs_rust.sh part1 --best <N_ref>
scripts/compare_python_vs_rust.sh part2 --best <N_presets>
scripts/compare_python_vs_rust.sh report                              # exit 0: all 8 configurations ≥ 2x (T-011)
grep -rnE 'todo!|unimplemented!|#\[ignore\]|std::time|Instant|SystemTime|sleep|criterion|\[\[bench\]\]' crates/
grep -rn 'unsafe' crates/
cargo tree -p gcg-cli -e normal --offline | grep -c gcg-ai-reference   # prints 0
```

The reviewer also does four checks by reading:

- `ai/config.py:80-111`, `ai/baseline.py:18-73` and `ai/tuning.py:50-57` next to
  `gcg-ai-reference/src/contracts.rs`;
- that `gcg-sim` accepts only `standard` and `fast`;
- `docs/dev/eval/seed-ledger.json` and `panel.json`, for disjoint seeds and pairs;
- every `unsafe` block and every WEAKNESSES.md status.

Every finding goes into `docs/port/REVIEW.md` and is fixed and re-verified until the reviewer
signs off.

### M17 (Phase 8 gate)

Deliverables:

- `README.md`: build and run on the M2 Pro and the M6; the two presets and when to use each;
  the `gcg-eval` user tools of A30; licences and notices kept (T-138).
- Rust `docs/AI.md` (D14a): the preset table, the mapping to the reference, the search design,
  the ablation table and the measured strength (T-130, T-139).
- `docs/dev/WEAKNESSES.md` (final status), `docs/dev/ARCHITECTURE.md` (T-140),
  `docs/dev/EQUIVALENCE.md` (methodology, every divergence with root cause and impact, and the
  golden-platform-only corpus sets; T-141) and `docs/dev/BENCHMARKS.md`.
- `CLAUDE.md`: Rust commands, the IR and corpus regeneration steps, the Rust traceability
  generator, and that the Python effect toolchain stays as the IR generator (§3.2) (T-142).
- `.claude/skills/gcg-benchmark`: the two-stage workflow (screen variants with `fast`, polish the
  final deck with `standard`) (T-142).
- `.claude/skills/gcg-refresh-data`: the `export_ir.py --write`, corpus and `cargo test -p
  gcg-data` steps, so the W-155 gate runs in every refresh (T-142).
- `.cargo/config.toml` with documented profiles, and `scripts/build_dist.sh` (T-143).
- `.github/workflows/rust.yml` on an arm64 macOS runner, with no timing jobs:
  1. `cargo fetch --locked` (the runner has no crate cache), then every later step `--offline`;
  2. `cargo test -p gcg-core --release --locked --offline --test vectors` first: the libm vectors
     decide whether the runner matches the golden platform (A1);
  3. fmt, clippy (both feature sets of §9.11), tests, `gcg-diff lints`;
  4. on a matching platform, `gcg-diff corpus --tier 1 --require-all`. On a mismatching one, the
     job runs the platform-independent sets (`--only full,vanilla,fuzz,coverage,tests,views,worlds`,
     which replay recorded actions without floats) and the portable-math `gcg-search` tests, then
     fails with `non-golden platform: libm differs (see docs/dev/EQUIVALENCE.md); search,
     components and baselines sets not checked`, instead of reporting false divergences.
     `docs/dev/EQUIVALENCE.md` lists the golden-platform-only sets.
- `scripts/ci_local.sh`: the same steps as the workflow, run locally (T-156).
- The final report (T-146), assembled by the lead from recorded gate outputs, with the brief's
  sections: the results table (Python vs Rust for every configuration of both parts, with
  speedups); the equivalence status (exact or statistical, every divergence); the preset table
  with budgets and measured strength; the WEAKNESSES status table with evidence per item; card
  coverage; each gate command with its outcome; the confirmation that the crates contain no
  timing-based code; known limitations (including any refused approval and the §12.11
  post-acceptance steps); and recommended next steps.

```text
scripts/verify_all_rust.sh
scripts/ci_local.sh
uv run python scripts/check_doc_commands.py                # the Python docs gate passes with the port docs
target/release/gcg-diff traceability --check && target/release/gcg-diff tests-json --check
git status --porcelain --untracked-files=all              # empty after the commit (T-147)
```

The M17 user batch asks for the push of `rust-port` that the workflow run needs (A29). T-133 is
set from that run. If the user declines the push, or the runner is not a golden platform,
T-133 stays failing with the reason, the final report says so, and `scripts/ci_local.sh` is the
evidence that the steps pass on the golden platform.

## 14. Risks and mitigations (top 10)

| # | Risk | Likelihood / impact | Mitigation | Early signal |
| --- | --- | --- | --- | --- |
| R1 | **Process-local numbering leaks into reference decisions.** `_references` (`ai/actions.py:308-310`) checks `uid in key` and rewrites every number equal to the uid, over every once-per-turn key. Prevention/reduction keys of lasting rules contain `effect_key` (`engine/core.py:524`, `:561`), whose value is first-use order within one Python process (`effects/registry.py:124-130`). When a coincidence changes a merge, Python's decisions depend on which matches a worker ran before, so Python itself may differ between `--workers 1` and `10` | low frequency, high impact (byte equivalence of whole runs) | `OnceKey` with a typed `Cont` element that never equals a uid (§4.2). The numbering-shift probe (§8.9) runs every equivalence configuration at offsets 0 and 1000, plus Python workers 1 vs 10. If anything differs, the offset-1000 baseline is the target, EQUIVALENCE.md documents the Python nondeterminism with its root cause, and the lead adds a WEAKNESSES item | probe output at M9 (tooling at M3) |
| R2 | **`version`/touch, `event_group` or `_rules_checked` drift** | high frequency during M5, high impact (every digest) | The touch catalogue with a Python line per call; per-decision fingerprints that localise the first divergent step; `selftest` proving that an extra touch is caught; per-primitive version vectors | M5 corpus pass rate |
| R3 | **Float semantics in the reference search**: Neumaier `sum`, libm `pow`/`exp`/`log`/`erfc`, first-max ties, `round` ties-even, `per_visit` differences, evaluation order, LLVM folding | medium, high (search traces) | The `gcg-core` primitives already verified on 300k+ vectors; `components` vectors with float hex for every scorer; drill-down levels 2-3 with `trace-search`; the `test_baseline` digest; the `clippy.toml` bans; corpus runs on two `target-cpu` builds | M6 vector pass rate |
| R4 | **Stale-view dependence**: Python reads a stale cached view somewhere the single-game `nocache` check (Inv 11 §7) did not cover | low, high | Rust reproduces the cache key and invalidation exactly instead of relying on transparency (§4.6); the Python `--nocache` corpus regeneration; Rust `paranoid` over the whole corpus and fuzz; per-test probes recorded in call order | M3 `--nocache` check, M5 `paranoid` runs |
| R5 | **The corpus misses behaviour**: random and policy agents never reach some card's effect, so a bug survives to Phase 4 or to the benchmark | medium, medium | Covering decks played by the policy agent; per-test traces of all 2,892 rule/card tests with `call` records; the `worlds` set; the executed-program census; `coverage --check` (840/840 executed or trace-tagged); the tier-2 extension; the BO3 e2e matrix | census report at M3 |
| R6 | **Hooks, layout-dependent code and test overrides mis-ported**: 34 hooks, `choice_group`, `targets_available`, hook #33 reading `instrs[pc-1]`, IR fragments, `force_link` | medium, medium | The IR keeps Python's flattened layout; each hook cites its lines; per-card traces exercise every hook's cards (Inv 04 §6.2); fragments carry Python's exact ids | per-test traces of hook cards at M5.5 |
| R7 | **Speed below 10x at equal work**, or the strengthened `standard` squeezed by the 2x floor | medium, medium (the 2x floor is likely safe) | Allocation-free state and search path by design (§11.2 targets); profile-driven Phase 5 steps with measured wins; facet revalidation only with `paranoid` proof; `first_player_choice` and root-tree fork-join for the tail; budget tuning with a 10% margin; honest analysis in BENCHMARKS.md | first `part1` run right after M8 |
| R8 | **Strength targets missed**: a pooled lower bound ≥ 0.55 vs reference `strong`, including on held-out pairs alone, given the documented plateau past 2x budget (W-04) | medium, high | Spend the speed on iterations and on model fixes (horizon, solver, belief, refit) rather than root rules that were null in Python; an early v0 budget curve at M11 sizes the headroom; ablate everything; pre-registered held-out pairs and disjoint seeds prevent overfitting; report honestly if a gate fails | budget curve at M11 |
| R9 | **Workstreams drift apart**: interface churn, conflicting edits, integration breaks, nondeterministic parallel code | medium, medium | Contracts first; one owner per file; worktrees; lead-only `contracts.rs`; layered engine landing; merge only when the corpus pass rate does not drop; per-workstream gates; 1/4/10-worker identity checks for every agent | merge failures |
| R10 | **Environment, dependencies and approvals**: macOS or Python updates change libm results or the baseline; crates cannot be fetched offline; `signal-hook`, mimalloc, a zstd encoder and `llvm-tools` need the user's approval; the user may decline W-120, the §12.11 package parts or the CI push; a GitHub runner's libm may differ from the golden platform; CLI texts that embed CPython exception messages | low to medium, medium | The golden platform is recorded in every trace header and in `env.txt`; regeneration is always deliberate; dependency policy A8 with in-house fallbacks and recorded consequences; one user batch at M1 (A32) and one at M17, before the work that depends on them; the CI platform check (M17); A19 for exception texts | header mismatch, build failure, a refused approval |

Smaller risks handled elsewhere: argparse text parity (E's transcripts), the size of the in-house
replacements (validator, parser, Unicode tables; each with vector tests), and the cost of the final
evaluation (about 45,000 games across E1-E4; estimate, recorded once measured).

## 15. Open decisions

For the user, asked by the lead in one batch at the M1 gate (A32), each with the recommended
option first:

1. **Dependencies** (A8). Approve one `cargo fetch --locked` of `signal-hook` (SIGINT, A3),
   `mimalloc` with `cc` (allocator measurement, X11), one zstd encoder (W-99), and the
   `llvm-tools` rustup component (PGO). Choose cache only (recommended) or `cargo vendor` into
   `vendor/`. The consequence of each refusal is in A8.
2. **SIGINT fallback**, only if `signal-hook` is refused: sign off the `sigaction` module as a
   recorded deviation from `BRIEF.md:70`. Without either, the ported interrupt test cannot pass,
   and T-075 is reported failing.
3. **W-120.** Apply the proposed `.claude/settings.json` change (host-restricted network rules;
   the `git -C` pattern without its trailing space).
4. **§12.11.** (a) accept the in-scope resolutions, with the package edits as post-acceptance
   steps (recommended); (b) grant an exception to edit `src/gcg_sim` for exactly those parts in
   Phase 6; (c) record them as open.
5. **Optional.** Real tournament data for W-41's calibration (a follow-up; W-41 is resolved
   without it), and whether to delete the untracked bytecode of W-122 (not needed to resolve
   it).

For the user at M17: the push of `rust-port` that the CI run needs (A29).

For the lead:

1. **W-100** (A23). The default fixes the reproduce line in `gcg-sim` only, and the reference
   stays byte-identical.
2. **Probe outcome** (R1). If Python depends on the numbering, accept the offset-1000 baseline as
   the equivalence target and add the finding to WEAKNESSES.md.
3. **D14a.** The `git mv` of `docs/AI.md` at M11 (reversible, the Python text is kept byte for
   byte). If the lead prefers, the Rust document can live at `docs/dev/AI.md` instead, with
   README pointing to it; D14a, T-130 and T-139 then change their path.

## Appendix A. Test register (`docs/dev/tests.json`)

Every id in `docs/dev/tests.json`, with the milestone whose gate sets it and the command or
evidence. `gcg-diff tests-json --check` compares this list with the file.

| Id | Milestone | Command / evidence |
| --- | --- | --- |
| T-001 | M16 | `scripts/verify_all_rust.sh --full` from a clean clone |
| T-002 | M9 | `compare_python_vs_rust.sh equivalence` matrix and headline: all `IDENTICAL` |
| T-003 | M9, M15 | 1/4/10-worker identity for reference, strengthened, greedy, random and test agents (§9.11, §8.8) |
| T-004 | M9 | Rust `gcg-sim replay` on every Python and Rust replay; Python replay on Rust replays |
| T-005 | M7 | `crates/gcg-cli/tests/presets.rs`; the verify-script exit-2 loop |
| T-006 | M16 | reviewer reading of `gcg-ai-reference/src/contracts.rs` vs `ai/config.py`, `ai/baseline.py`, `ai/tuning.py`; reachability test |
| T-007 | M15 | `gcg-diff weaknesses --check docs/dev/WEAKNESSES.md` |
| T-008 | M16 | E1-E4 on review-role seeds; `gcg-eval gates --check docs/dev/eval/review` |
| T-009 | M9 | `crates/gcg-effects/tests/coverage.rs` + `gcg-diff coverage --check` |
| T-010 | M9 | `gcg-diff traceability --check` |
| T-011 | M16 | the reviewer's rerun of `compare_python_vs_rust.sh` (env, best-workers, part1, part2, report): `report` exits 0, all 8 role-paired configurations ≥ 2x |
| T-012 | M16 | lints, `clippy.toml` bans and the reviewer's grep |
| T-013 | M17 | M6 build notes, detection fixtures, BENCHMARKS.md without unmeasured numbers |
| T-014 | M16 | `docs/port/REVIEW.md` sign-off |
| T-015 | M9 | `docs/dev/EQUIVALENCE.md`: every divergence root-caused, none unexplained |
| T-016 | M0 | `cargo test -p gcg-core` (18 tests) |
| T-017 | M2 | `cargo fmt --all --check` |
| T-018 | M2 | `cargo clippy --workspace --all-targets -- -D warnings` with `clippy.toml` |
| T-019 | M2 | `gcg-diff lints`: no placeholders, no `#[ignore]` |
| T-020 | M2 | `gcg-diff lints`: no timing code (path-based; lint self-test fixture), no benches |
| T-021 | M2 | `gcg-diff lints`: `unsafe` only in `gcg-core/src/libm.rs` (plus profile-justified or user-approved additions) with `SAFETY` |
| T-022 | M7 | `crates/gcg-cli/tests/no_reference.rs` + `cargo tree` check |
| T-023 | M7 | `cargo test --doc` compile-fail doctests (W-49, W-135, W-136) |
| T-024 | M2 | `git diff --quiet d9d41af -- src/gcg_sim tests` and an empty `git status --porcelain --untracked-files=all -- src/gcg_sim tests` (after acceptance the baseline is the package commit of the latest data refresh, 8f2ba29 since 2026-11-06, A15) |
| T-025 | M2 | `export_ir.py --check` (hash seeds 1 and 2) |
| T-026 | M2 | `export_vectors.py --check` and the other exporters' `--check` |
| T-027 | M3 | `traces.py --check --tier 1` under `PYTHONHASHSEED=1` and `2` |
| T-028 | M3 | `traces.py --check --tier 1 --nocache` |
| T-029 | M3 | wrapped vs plain runs: equal digests and action lists |
| T-030 | M3 | `gcg-diff corpus --tier 1 --parse-only` |
| T-031 | M5.5 | `gcg-diff selftest` (synthetic controls at M3) |
| T-032 | M3 | `compare_python_vs_rust.sh equivalence --selftest` |
| T-033 | M9 | `--offset-probe 1000` and Python workers 1 vs 10; result in EQUIVALENCE.md |
| T-034 | M3 | `census.py --check`; capacities set from the census |
| T-035 | M4 | `CardDef` field-by-field equality (1153) and the 4 fingerprints |
| T-036 | M4 | deck parser/validator triples |
| T-037 | M4 | Unicode vectors (10,000) |
| T-038 | M4 | `data status` text and `--json` bytes |
| T-039 | M4 | `versions`, `conflict_resolutions` and `code_digest` vectors |
| T-040 | M2 | `ir_current.rs` and schema/enum catalogue check |
| T-041 | M2 | `content_ids.rs` (all nodes, 626 programs, filter sets) |
| T-042 | M2 | `golden.rs` (27 files byte-identical) |
| T-043 | M2 | `registry_counts.rs` |
| T-044 | M2 | `repr_vectors.rs` |
| T-045 | M5.1 | snapshot round trip and digest on every tier-1 snapshot |
| T-046 | M5.5 | `touch_catalogue.rs` (120; 149 + 11 × redraws; `5ce28ed9…` v357) |
| T-047 | M5.2 | `gcg-diff corpus --only views` |
| T-048 | M5.5 | `gcg-diff corpus --only full` |
| T-049 | M5.5 | `gcg-diff corpus --only vanilla` (setup prefixes included) |
| T-050 | M5.5 | `gcg-diff corpus --only fuzz` |
| T-051 | M5.5 | `gcg-diff corpus --only coverage` |
| T-052 | M5.5 | `gcg-diff corpus --only tests` |
| T-053 | M5.5 | `gcg-diff corpus --only worlds` |
| T-054 | M5.5 | `paranoid` corpus run + `gcg-diff paranoid-games` (16 random games at seed 11, 2 greedy games) + `gcg-engine` tests with `paranoid,testkit`; in `verify_all_rust.sh` from M8 |
| T-055 | M5.5 | `gcg-diff fuzz --games 10000 --seed 2026 --compare …` |
| T-056 | M9 | commit-seed fuzz (2,000 games) and 1,000 strategic paranoid fuzz games (greedy, `search:fast`, mixed; W-110); in `verify_all_rust.sh` |
| T-057 | M5.5 | testkit lead-example ports (12) and Scenario rebuild of every per-test trace |
| T-058 | M5.5 | engine property tests (§9.6) |
| T-059 | M6 | `gcg-diff corpus --only components` |
| T-060 | M6 | reference preset objects, including their evaluation weights, equal Python's dump |
| T-061 | M6 | Inv 07 §17 acceptance searches + `test_baseline` digest `5cff789d…` |
| T-062 | M6 | `search` traces at reference `fast` |
| T-063 | M6 | `search` traces at reference `standard` |
| T-064 | M6 | `search` traces at reference `strong` |
| T-065 | M6 | `gcg-diff corpus --only baselines` |
| T-066 | M6 | puzzle ports (52) with full decision logs |
| T-067 | M6 | information-set ports (128 + 25 + 28) |
| T-068 | M6 | null-config replay test |
| T-069 | M6 | `first_player_choice` fork-join equals serial |
| T-070 | M8 | `strength_reference` (seeds 7001/7002 outcome lists, slow) |
| T-071 | M7 | reports from exported records byte-identical |
| T-072 | M7 | CLI transcripts identical |
| T-073 | M9 | `reference-benchmark --agent test-random` vs Python factory hook |
| T-074 | M7 | pool determinism at 1/4/10 workers; lowest failing index with failures injected at 3 and 7 under `--workers 10` |
| T-075 | M7 | `crates/gcg-cli/tests/interrupt.rs` |
| T-076 | M7 | schema 1.1.0 validation + `schema-check --expect-invalid` |
| T-077 | M7 | topology parsing fixtures (2 and 3 tiers); `--workers auto` |
| T-078 | M7 | ported `tests/runner`, `tests/reports`, `tests/cli`, `tests/deck` |
| T-079 | M9 | `gcg-eval compare` vs `python -m gcg_sim.ai.compare` (3 configurations) |
| T-080 | M8 | head-to-head 1/4/10-worker identity; Wilson/Newcombe vectors |
| T-081 | M8 | seed-role disjointness test; ledger enforcement |
| T-082 | M8 | `reference-benchmark` names and schema 1.1.0 |
| T-083 | M9 | e2e equivalence family bo1 × reference `fast` |
| T-084 | M9 | e2e equivalence family bo3 × reference `fast` |
| T-085 | M9 | e2e equivalence family bo1 × reference `standard` |
| T-086 | M9 | e2e equivalence family bo3 × reference `standard` |
| T-087 | M9 | e2e equivalence family bo1 × reference `strong` |
| T-088 | M9 | e2e equivalence family bo3 × reference `strong` |
| T-089 | M9 | e2e matrix with `--decision-log` |
| T-090 | M9 | headline: 100 matches, bo3, `standard`, workers 1/4/10 |
| T-091 | M9 | `gcg-diff corpus --tier 2 --require-all --paranoid` |
| T-092 | M9 | `apple-m2` dist build: tier-1 corpus identical |
| T-093 | M9 | `gcg-diff coverage --check` |
| T-094 | M9 | `gcg-diff traceability --check` |
| T-095 | M1 | `gcg-diff tests-json --check` (JSON check at M1) |
| T-096 | M10 | Part 1 pair A headline at best workers ≥ 2x |
| T-097 | M10 | Part 1 pair A headline at 10 workers ≥ 2x |
| T-098 | M10 | Part 1 pair A single-thread ≥ 2x |
| T-099 | M10 | Part 1 pair B full search ≥ 2x |
| T-100 | M15 | Part 2 `fast` headline at best workers ≥ 2x |
| T-101 | M15 | Part 2 `fast` headline at 10 workers ≥ 2x |
| T-102 | M15 | Part 2 `fast` single-thread ≥ 2x |
| T-103 | M15 | Part 2 `standard` full search ≥ 2x |
| T-104 | M10 | Instruments Allocations: zero allocations in `gcg_ai_reference::mcts::search` after warm-up |
| T-105 | M10 | allocator, PGO, NEON and GPU evaluations recorded in BENCHMARKS.md |
| T-106 | M15 | W-id partition and ablation-table completeness |
| T-107 | M13/M14 | scenario test W-01 (`crates/gcg-search/tests/scenarios/`) |
| T-108 | M13/M14 | scenario test W-03 (`crates/gcg-search/tests/scenarios/`) |
| T-109 | M13/M14 | scenario test W-05 (`crates/gcg-search/tests/scenarios/`) |
| T-110 | M13/M14 | scenario test W-07 (`crates/gcg-search/tests/scenarios/`) |
| T-111 | M13/M14 | scenario test W-09 (`crates/gcg-search/tests/scenarios/`) |
| T-112 | M13/M14 | scenario test W-12 (`crates/gcg-search/tests/scenarios/`) |
| T-113 | M13/M14 | scenario test W-13 (`crates/gcg-search/tests/scenarios/`) |
| T-114 | M13/M14 | scenario test W-14 (`crates/gcg-search/tests/scenarios/`) |
| T-115 | M13/M14 | scenario test W-16 (`crates/gcg-search/tests/scenarios/`) |
| T-116 | M13/M14 | scenario test W-17 (`crates/gcg-search/tests/scenarios/`) |
| T-117 | M13/M14 | scenario test W-24 (`crates/gcg-search/tests/scenarios/`) |
| T-118 | M13/M14 | scenario test W-25 (`crates/gcg-search/tests/scenarios/`) |
| T-119 | M13/M14 | scenario test W-28 (`crates/gcg-search/tests/scenarios/`) |
| T-120 | M13/M14 | scenario test W-29 (`crates/gcg-search/tests/scenarios/`) |
| T-121 | M13/M14 | scenario test W-32 (`crates/gcg-search/tests/scenarios/`) |
| T-122 | M13/M14 | scenario test W-45 (`crates/gcg-search/tests/scenarios/`) |
| T-123 | M13/M14 | scenario test W-139 (`crates/gcg-search/tests/scenarios/`) |
| T-124 | M15 | E1 `standard` vs reference `strong` |
| T-125 | M15 | E2 `fast` vs reference `standard` |
| T-126 | M15 | E3 `standard` vs random (test decks; every panel pair, held-out pairs reported on their own) |
| T-127 | M15 | E4 `standard` vs greedy (test decks; panel pooled over all pairs and over held-out pairs) |
| T-128 | M11 | evaluation decks legal, overlap bounds, panel pre-registered |
| T-129 | M15 | schema 1.2.0 validation; `gcg-sim replay` on `gcg-sim` replays |
| T-130 | M15 | `docs/AI.md` preset table equals code constants (W-129) |
| T-131 | M15 | `gcg-eval fidelity` (W-18) |
| T-132 | M15 | budget curve and Part 2 candidate timings recorded |
| T-133 | M17 | `.github/workflows/rust.yml` green on a golden-platform runner, after the user approves the push |
| T-134 | M17 | `uv run python scripts/check_doc_commands.py` passes |
| T-135 | M16 | `scripts/verify_all.sh` (Python gate) passes with the port committed (W-154) |
| T-136 | M0 | Python baseline (comparison protocol, `env.txt`) and the cProfile/py-spy profile recorded in `docs/dev/BENCHMARKS.md` |
| T-137 | M10 | BENCHMARKS.md explains from the profile why Part 1 is below 10x, or records the measured ≥ 10x |
| T-138 | M17 | README: build and run on M2 Pro and M6, the two presets and when to use each, the `gcg-eval` user tools, licences and notices |
| T-139 | M17 | Rust `docs/AI.md` complete (preset table, mapping, search design, ablation table, measured strength); Python text at `docs/python/AI.md` (D14a) |
| T-140 | M17 | `docs/dev/ARCHITECTURE.md` |
| T-141 | M17 | `docs/dev/EQUIVALENCE.md` final: methodology, every divergence with root cause and impact, golden-platform-only sets |
| T-142 | M17 | `CLAUDE.md`, `.claude/skills/gcg-benchmark` (two-stage workflow) and `.claude/skills/gcg-refresh-data` (IR, corpus and Rust data-gate steps) updated |
| T-143 | M17 | `scripts/build_dist.sh` builds the `apple-m2` dist binaries; `.cargo/config.toml` profiles documented |
| T-144 | M15 | surprising-result replays saved and inspected; notes in `docs/dev/eval/notes.md` |
| T-145 | M8 | `scripts/verify_binary_offline.sh`: dist `gcg-sim` outside the repo, no network, no Python on `PATH`; workers 1 vs 2 `results.json` identical; replay verifies |
| T-146 | M17 | final report assembled with every section of the brief's `<final_report>` |
| T-147 | every milestone | temp-file sweep: `git status --porcelain --untracked-files=all` shows only intended files; none left after the M17 commit |
| T-148 | M15 | Instruments Allocations: zero allocations in `gcg_search`'s search call tree after warm-up (`standard` and `fast`) |
| T-149 | M8 | head-to-head `--choose-first seat0`: each agent on the play in exactly half the games of every block, each deck once on the play and once on the draw (A24) |
| T-150 | M5.5 | `once_used` set semantics: `once_add` skips present keys; property test; `selftest` duplicated-key control |
| T-151 | M5.5 | `new_game` keeps the caller's deck order: one multiset in two orders gives different shuffles, equal to Python vectors |
| T-152 | M7 | `gcg-sim` help and usage of every subcommand equal Python's except `--ai-preset` and `--workers auto` (A30) |
| T-153 | M12.M | `gcg-eval benchmark` with no opt-in option writes the same bytes as `gcg-sim benchmark` (except `timing.json`) |
| T-154 | M12.K | `export_ir.py` asserts that every port template reproduces its binding cards' scripts byte for byte (W-63, W-67); IR otherwise unchanged |
| T-155 | M12.N | W-153 recorded-response tests of the refresh skill's news paging (`uv run pytest oracle/tests/refresh_news`) |
| T-156 | M17 | `scripts/ci_local.sh` (the `rust.yml` steps, run locally) passes |
| T-157 | M3 | per-test `call`/`probe` argument encoder: one synthetic call per argument kind round-trips through the fragment, `continuous` and `filter_sets` |
| T-158 | M3 | `compare_python_vs_rust.sh report --selftest`: role pairing, and exit 1 on a missing, below-2x or stale-build configuration |

## Review log

Two adversarial checkers reviewed this plan after its first draft. Every reported gap was checked
against the brief and the code. All were accepted and fixed in the sections named; none was
rejected outright. Where the fix differs from the one a checker proposed, the reason is below.

- **Dependency vendoring (A8).** The checker proposed vendoring the new crates with `cargo vendor`.
  The plan makes cache-only (`cargo fetch --locked` once) the recommended option and offers
  vendoring to the user. Builds stay `--offline` either way, and vendoring would add the C
  sources of `mimalloc` and zstd to the history. The user decides.
- **W-99 fallback (P6-M).** The checker asked that a refusal of the zstd crate be recorded as a
  consequence. The plan instead gives W-99 a safe in-house gzip writer as the fallback, so the
  item is resolved whatever the user answers.
- **Rust AI document (D14a).** Of the two proposed locations, the plan takes the brief's path
  (`docs/AI.md`) with the Python text moved to `docs/python/AI.md`. It keeps `docs/dev/AI.md`
  as the lead's alternative (§15), because the move rewrites about 65 citations.
- **W-135 lint (§9.9 item 5).** The checker proposed banning the name `GameState` in
  `gcg-search/src/` outside the adapter. That cannot work, because the search's own determinised
  worlds are `GameState` values. The lint restricts instead what the adapter may do with the
  *true* state, and a compile-fail doctest shows that the `Observation` cannot return it.
- **Duration lint.** Two checkers raised this gap. The plan keeps Python's enum name `Duration`,
  so the Rust type names match the IR's `schema`/`enums` tables one to one, and makes the lint
  match paths and method calls only.
- **AI memo caches across override tests (A16).** Of the two proposed fixes, the plan takes the
  fresh process per override test, plus an emptiness assertion and preloading as a backstop,
  instead of snapshotting cache keys in the shared process. It is simpler and covers stale
  entries that leak from one test into later ones.
- **W-50 and W-51 (§4.2, P6-J).** The listened-set fix is applied as proposed. The same reasoning
  applies to W-51's HP flag, which decides `_rules_checked` (`core.py:866-875`) and so the
  traced `rc`. So the engine uses the registry-global flag verbatim, and the per-game flag
  becomes a `paranoid` check. P6-J records the narrowing in both entries.
- **Tail latency (§11.4 step 10).** The fork-join of the reference's `first_player_choice` was
  already step 10's second bullet. The fix removes the inapplicable BO1 rule and states that no
  further mitigation exists for the reference.
- **Delayed-trigger labels (§3.3).** An addition beyond the reported gap: Python registers
  delayed programs lazily and dedupes them by steps, so their labels could depend on firing
  order. The exporter now asserts that this cannot happen.
