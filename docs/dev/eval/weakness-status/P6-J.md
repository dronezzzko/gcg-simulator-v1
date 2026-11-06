# P6-J status: engine hardening (class N)

Package P6-J of docs/dev/PLAN.md §12.2: W-46, W-47, W-48, W-49, W-50, W-51, W-52, W-53,
W-54, W-124, W-125, W-143, W-158, W-159, W-161, W-162 (16 items). Written by the P6-J
implementer for the lead, who records the outcomes in docs/dev/WEAKNESSES.md (§12.10). This
file does not edit WEAKNESSES.md. Branch `worktree-agent-a64dcb143f9debf26`, from `4c1024d`
("Port Phase 5").

## What P6-J changed

Rules, card behaviour, data and every reference-equivalent output are unchanged: each source
change is an accessor, a counter that is never read by the engine, a check compiled only into
debug or `paranoid` builds, or a registry-build error for an IR that no packaged card produces.

| File | Change | Items |
| --- | --- | --- |
| `crates/gcg-engine/src/diag.rs` (new, `pub mod diag`) | `CapKind`, `GameState::cap_kind`; the per-thread count `diag::non_converged_views`; the W-54 assertions `assert_live_decision`/`assert_live_step` (debug and `paranoid` only, `cfg!`) | W-46, W-48, W-54 |
| `crates/gcg-engine/src/lib.rs` | `pub mod diag;`, `pub use diag::CapKind;` | W-46 |
| `crates/gcg-engine/src/view/derived.rs` | `compute_in` counts a fixpoint that stops at `FIXPOINT_PASSES` without converging (the view is unchanged: the last pass, as before) | W-48 |
| `crates/gcg-engine/src/state.rs` | `set_pending` asserts no PAYMENT decision; the `paranoid` side table parallel to the trigger list (`TriggerArena` keeps it in step on push, take, pop and clear; `clone_from_state`, `from_body` and `new` fill it) | W-54, W-162 |
| `crates/gcg-engine/src/game.rs` | `advance` asserts no ATTACK_TRIGGERS step each iteration; `resolve_trigger` reports a moved host (`paranoid`) | W-54, W-162 |
| `crates/gcg-engine/src/contracts.rs` | one `#[cfg(feature = "paranoid")] pub(crate) trigger_seqs` field on `GameState` (never serialised; see the interface notes) | W-162 |
| `crates/gcg-engine/src/paranoid.rs` | `MovedHost`, `moved_hosts()`, `TriggerSeqs`, `trigger_resolves` (the `Check::ALL` list is unchanged, so `gcg-diff paranoid-games` is unaffected) | W-162 |
| `crates/gcg-engine/src/core.rs` | `paranoid::{per_game_hp_shortcut, game_lowers_hp, lowers_hp}` and their unit tests (`paranoid` only) | W-51 |
| `crates/gcg-effects/src/ir.rs` | `check_prevention_threshold` in `Arena::decode_nodes` (full IR and fragments) | W-158 |
| `crates/gcg-engine/tests/p6j_engine.rs` (new, 16 tests, one `paranoid`-only) | testkit evidence pinned to Python | W-46, W-47, W-48, W-50, W-51, W-53, W-125, W-143, W-159, W-161, W-162 |
| `crates/gcg-effects/tests/p6j_ir_tripwires.rs` (new, 11 tests) | IR tripwires with negative controls | W-50, W-143, W-158, W-159, W-161 |
| `crates/gcg-diff/tests/p6j_runs.rs` (new, 3 tests, one `slow`) | the W-143 e2e benchmark; fuzz diagnostics over Python's fuzz games | W-46, W-47, W-48, W-49, W-54, W-143, W-162 |

The test-only scripts of `p6j_engine.rs` (W-47 loop, W-48 oscillation, W-161 ToDeck, W-162
moved host) are IR fragments of `registry.override_script` with a DSL script, exported by
`oracle/export_ir.py` `build_fragment` in a fresh Python process (the method of
`oracle/p6k_vectors.py` `hp_fragment`), and embedded in the test file with the script's
`repr`; each test quotes the Python testkit result of the same position, produced in the same
process.

## Gate (run on this branch after the last source edit, M2 Pro under heavy load)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release --no-fail-fast` | 1014 passed, 0 failed (121 test targets; baseline at `4c1024d`: 984) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804; search 9/9 (771 decisions, 771 agent blocks), baselines 8/8 (625, 625) |
| `target/release/gcg-diff corpus --tier 2 --require-all` (tier 2 regenerated with `uv run python oracle/traces.py --write --tier 2`) | 4154/4154 (files match `MANIFEST-tier2.json`); search 144/144 (12,131 searched decisions, 12,131 agent blocks) |
| `target/paranoid/release/gcg-diff corpus --tier 1 --require-all --paranoid` (`cargo build --release -p gcg-diff --features paranoid --target-dir target/paranoid`) | 3804/3804, with the new W-51, W-54 and W-162 checks active |
| `cargo test --release -p gcg-engine --features paranoid,testkit --target-dir target/paranoid` | 244 passed, 0 failed |
| `cargo test --release -p gcg-diff --features paranoid,slow --test p6j_runs --target-dir target/paranoid -- --nocapture` | 3 passed (the 10,000-game run and its output are quoted under the items) |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 IDENTICAL (`fast`, `standard`, `strong` reference budgets) |
| `target/debug/gcg-diff lints`, `tests-json --check` | passed |
| `target/debug/gcg-diff traceability --check` | 2 problems, both present before P6-J: `docs/RULES_TRACEABILITY.md` and `docs/dev/TRACEABILITY_CARDS.md` lack the rows of P6-K's tagged tests (`p6k_behaviour.rs`; a `--write` adds only `p6k_*` entries). The P6-J tests carry no tags, so they change neither file. Lead files, not regenerated here |

## Ablation rows (A13)

P6-J changes code the AI executes (the engine), so the A13 evidence applies. No item has an
off switch in a release build: the accessors and the counter change no value the engine or
the AI reads, the W-54 assertions and the W-51/W-162 checks are compiled out of release builds
(`cfg!(debug_assertions)` or `feature = "paranoid"`), and the W-158 check rejects only IR that
the packaged data does not contain (the embedded registry loads unchanged). The evidence is
therefore the by-construction form: with the change in, the `search` and `baselines` corpus
sets and the whole tier-1 and tier-2 corpora replay Python's traces at 100% (every decision,
alternative, statistic and RNG state of the reference search and the baselines), the seed-1
equivalence subset is byte-identical to Python, and the gcg-search scenario and null-config
tests pass in `cargo test --workspace --release` (gate above). The common row, repeated per item
as "Ablation: no AI effect (P6-J common row)":

> **No AI effect.** The change alters no value that the engine, the reference search or the
> strengthened search reads in a release build. Evidence (A13, built in, no off switch):
> `gcg-diff corpus --tier 1 --require-all` and `--tier 2 --require-all` at 100% with the
> change (the `search` and `baselines` sets included), the seed-1 equivalence subset 3/3
> identical, and the gcg-search tests passing; behaviour evidence: the tests of the item.

## Items

### W-46 — Safety caps share one end reason — resolved (engine side; the `--diagnostics` CLI is P6-M's)

- **Resolution as implemented:** every reference-equivalent output keeps `turn_limit` (no
  change). `GameState::cap_kind() -> Option<CapKind>` (`crates/gcg-engine/src/diag.rs:39`)
  names the cap that ended the game, `CapKind::Turns` or `CapKind::Decisions` (`as_str()`
  `"turns"`/`"decisions"`), or `None` for a running game or any other ending. It needs no
  state: `advance` checks the decision cap first in every iteration and `action_count`
  changes only in `apply` (`game.rs` `advance`, `apply`), so the turn cap of `next_turn` can
  fire only while `action_count <= max_actions` and the decision cap only when it is
  greater. Both values are serialised, so a reloaded state answers the same.
- **Evidence:** `p6j_engine.rs::w46_both_caps_report_turn_limit_and_name_the_cap`
  (`max_actions = 10` gives `turn_limit` and `CapKind::Decisions`; `turn_limit = 2` gives
  `CapKind::Turns`; each final state's digest equals Python's game with the same decks,
  seed and random play, quoted in the test; the reloaded snapshot keeps the kind);
  `w46_other_endings_have_no_cap_kind` (battle damage, concede);
  `p6j_runs.rs::check_fuzz` (no fuzz game hits a cap).
- **Not done here (other packages):** the opt-in `gcg-eval benchmark --diagnostics` side file
  and listing capped games in head-to-head summaries (P6-M, P6-I; A30). Interface for them:
  call `st.cap_kind()` on a game's final state (the runner has it when the game ends; a
  `games.ndjson` record does not carry it, so a side file must be written while the final
  state is alive, or from a replay).
- **Ablation:** no AI effect (P6-J common row); the AI never calls `cap_kind`.

### W-47 — No guard against decision-free loops in advance — resolved

- **Resolution as implemented:** landed in L5 and kept: `advance` returns
  `EngineError::NoProgress { steps: NO_PROGRESS_STEPS }` after 1,000,000 loop iterations
  without a decision (`crates/gcg-engine/src/game.rs:1411-1421`,
  `contracts.rs` `NO_PROGRESS_STEPS`); the fuzzer treats it as a failure (an engine error
  fails the game). Legal games never come near it.
- **Evidence:** `p6j_engine.rs::w47_mutually_retriggering_delayed_triggers_end_in_no_progress`:
  a test-only Command arms two delayed triggers that re-trigger each other ("when a friendly
  Unit is rested, set it as active" / "when it is set as active, rest it") and rests a Unit;
  Python's `advance` loops forever (its run was stopped after 2,000 RESTED events, with
  `action_count` 1), Rust returns `NoProgress` with `action_count` 1. The older
  `l5_procedure.rs::advance_without_progress_fails_instead_of_looping` covers a setup step
  with no row. Fuzz: `p6j_runs.rs::p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games` (features `slow` and `paranoid`, `cargo test --release -p gcg-diff --features paranoid,slow --test p6j_runs --target-dir target/paranoid -- --nocapture`): all 10,000 games of Python's `fuzz --games 10000 --seed 2026` replay to Python's per-game rows (856,034 actions), and no game returned `NoProgress` (an engine error fails the test).
- **Ablation:** no AI effect (P6-J common row); error path only.

### W-48 — The derived-view fixpoint is silently capped — resolved

- **Resolution as implemented:** the six-pass loop is unchanged (the last pass wins at the
  cap); `Derived::converged()` (landed in L2) is false then, and `compute_in` now counts each
  such view in the per-thread `gcg_engine::diag::non_converged_views()` (always compiled; the
  increment runs only at the cap), which fuzz and paranoid runs read before and after their
  games.
- **Evidence:** `p6j_engine.rs::w48_gd02_076_with_a_lasting_ap_plus_one_gains_blocker`
  (GD02-076 with a lasting AP+1 has AP 5 and <Blocker>, converged, counter unchanged; Python
  `before 4 []`, `after 5 ['Blocker']`);
  `w48_an_oscillating_constant_is_reported` (a test-only "while this Unit has 5 or more AP,
  it gets AP-1" on a 5-AP Unit: AP 5, HP 4 as in Python, `converged() == false`, counter
  incremented); `diag::tests::the_counter_belongs_to_its_thread`. Fuzz:
  `p6j_runs.rs::p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games` (features `slow` and `paranoid`, `cargo test --release -p gcg-diff --features paranoid,slow --test p6j_runs --target-dir target/paranoid -- --nocapture`): all 10,000 games of Python's `fuzz --games 10000 --seed 2026` replay to Python's per-game rows (856,034 actions), with 0 non-converged views (the counter read before and after each game on its own thread); the first 200 games run in the default suite with the same result. Any future dependency-ordered view (W-124) must reproduce these.
- **Ablation:** no AI effect (P6-J common row).

### W-49 — The view cache relies on manual touch() — resolved (by construction)

- **Resolution as implemented (Phase 3, confirmed):** `GameState` fields are `pub(crate)`
  (`contracts.rs:539-622`); other crates read through accessors, and the compile-fail doctest
  at `contracts.rs:33-39` shows that an outside crate cannot write a field. The cache key is
  `ViewKey` plus `touch` as the only invalidation (`view::derived`, `state.rs` `touch`). The
  `paranoid` feature recomputes every cached view, re-runs every skipped rules check, checks
  every unlistened event and every view-free reference and the skipped fixpoint pass
  (`paranoid.rs`, `view/paranoid.rs`, `core.rs` `mod paranoid`).
- **Evidence:** the doctest (`cargo test --workspace --release` runs doctests);
  `cargo test --release -p gcg-engine --features paranoid,testkit`; `gcg-diff corpus --tier 1
  --require-all --paranoid`; paranoid fuzz: `p6j_runs.rs::p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games` (features `slow` and `paranoid`, `cargo test --release -p gcg-diff --features paranoid,slow --test p6j_runs --target-dir target/paranoid -- --nocapture`): all 10,000 games of Python's `fuzz --games 10000 --seed 2026` replay to Python's per-game rows (856,034 actions) on the paranoid build, every cached view recomputed and compared and every skip checked, with no mismatch.
- **Ablation:** no AI effect (P6-J common row).

### W-50 — The listened-events fast path depends on the decklists — resolved, with the recorded narrowing

- **Deviation from the plan text (PLAN §12.2 P6-J, §12.10, §15 note on W-50/W-51):** the plan
  text proposed an `always` set derived from the IR and a set extended by every
  card-creation path as engine values. The port keeps Python's formula exactly as the
  engine value (`GameConst.listened`, `state.rs` `listened_events`: the registry's
  `always_events` plus `events_of` of every decklist def and its Unit variant, computed per
  game, no global cache), because the listened set decides when `emit` computes the view
  and so the cache states the traces record. The per-game refinements are checks instead.
- **Resolution as implemented:** no global cache (one set per game in its `GameConst`);
  testkit cards added after start join the decklists, and the set is recomputed
  (`testkit::with_decklists`, unchanged).
- **Evidence:** `p6j_ir_tripwires.rs::w50_the_derived_always_set_is_within_pythons` (the
  IR-derived set: the keyword-trigger events of `core.keyword_triggers`, every
  DelayedTrigger event in a card script and every token's events; it is a subset of Python's
  `always_events`, and today equal to it, `{destroys_by_battle, turn_end}`);
  `w50_a_new_delayed_trigger_event_is_detected` (negative control);
  `p6j_engine.rs::w50_a_card_added_after_start_triggers` (ST04-002 added after start
  deploys and its 【Deploy】 trigger runs: deck 19 -> 18, a 2-option DISCARD, as in Python,
  whose listened set gains `deployed` at the add); the `paranoid` unlistened-event check
  (`Check::UnlistenedEvent`) over the paranoid corpus and fuzz.
- **Ablation:** no AI effect (P6-J common row).

### W-51 — The HP shortcut depends on a registry-global flag — resolved, with the recorded narrowing

- **Deviation from the plan text (same notes as W-50):** the engine keeps the registry's flag
  verbatim (`GameConst.hp_reduction_possible`), because the flag decides `_rules_checked`
  (`core.py:866-875`), which the traces record. The per-game flag is a `paranoid` check.
- **Resolution as implemented:** `core.rs` `paranoid::game_lowers_hp` computes Python's
  formula (`registry.py:368-370`: a StatMod whose `hp` is not a literal int `>= 0`, read from
  each script's CPython `repr`) over the cards that can appear in the game (decklists,
  resource decklists, their Unit variants, every token, and every card an `AbilityGrant`
  among them names). When the registry flag is true and the per-game flag false, every scan
  that the printed-HP shortcut would skip is shown to destroy nothing
  (`per_game_hp_shortcut`, counted as `Check::NoPrintedLethalDamage`).
- **Evidence:** `core::paranoid::tests::per_card_flags_agree_with_the_registry` (no
  packaged script lowers HP, matching the IR's `hp_reduction_possible = false`; the GD01-064
  override does); `statmod_reprs_follow_the_registry_formula`;
  `p6j_engine.rs::w51_an_hp_lowering_override_leaves_no_lethal_damaged_unit` (the d9d41af
  regression of `tests/rules/test_damage.py:791`); `w51_the_per_game_flag_drives_the_printed_hp_check`
  (`paranoid`: a game without DINN runs the check, a game with DINN in a decklist does not);
  paranoid corpus and fuzz.
- **Ablation:** no AI effect (P6-J common row).

### W-52 — GameState.clone shares pending, the view and history — resolved (by construction)

- **Resolution as implemented (Phase 3):** `#[derive(Clone)] GameState`
  (`contracts.rs:557-622`): cards, zones, battles, lasting, triggers, frames, `once_used`,
  history (`HistoryEvent` is `Copy`), the pending header (`Copy`) and its options are owned
  values, copied by `clone` and `clone_from_state` (exhaustive destructuring, `state.rs`).
  The shared parts are immutable: `konst: Arc<GameConst>` (no interior mutability) and the
  cached `Arc<Derived>`, whose only cell, `by_event`, is a `OnceLock` filled once with the
  index of the key it was computed for (any zone change touches and drops the clone's cache,
  which is the clone's own `RefCell`). `observe::determinize` takes `&GameState` and returns a
  new state; `determinize_into` writes only its `out`.
- **Evidence:** `crates/gcg-search/tests/ai_search_side_effects.rs` (port of
  `tests/ai/test_search_side_effects.py`: determinized and permuted copies and a whole
  reference decision leave the true state's digest unchanged);
  `crates/gcg-engine/tests/engine_properties.rs::clone_from_state_equals_clone_on_game_states`
  (writing a copy leaves its source alone); the Phase 4 replay checks (every Python replay
  verifies under Rust byte for byte, P4-D).
- **Ablation:** no AI effect (P6-J common row).

### W-53 — Registry indices are process-local — resolved (content-addressed saved states)

- **Resolution as implemented (Phase 3, confirmed):** the saved form (`canon::canonical_state`
  plus `StateSnapshot` side tables) names programs, IR continuous effects and filter sets by
  content id (SHA-256 of the CPython `repr`, `runner/digest.py:26-27`) and spells value effects
  (`StatMod`, `KeywordGrant`, `AbilityGrant`) inline; `canon::from_canonical` resolves them
  against whatever registry it is given (PLAN §4.8). Rust registry ids are fixed by the IR
  file, and runtime effects are values (`ContRef`), so nothing is interned in first-use
  order.
- **Evidence:** `p6j_engine.rs::w53_a_saved_state_reloads_in_a_fresh_process` (a mid-game
  state of a random example-deck game, with lasting effects, once keys or a resolving
  program, saved with `testkit::snapshot`; a child process, this test binary, builds a fresh
  registry from the IR text, reloads the state and plays the remaining actions: its final
  digest equals the uninterrupted game's); `w53_reload_in_this_process` (the same through a
  second `Registry` instance in one process); the whole corpus, whose snapshots Python
  saved and Rust reloads (tier 1 and tier 2 at 100%, gate); `engine_properties.rs`
  round trips.
- **Ablation:** no AI effect (P6-J common row).

### W-54 — Dead enum values PAYMENT and ATTACK_TRIGGERS — resolved

- **Resolution as implemented:** both variants stay for serialisation (generated
  `enums.rs`). `diag::assert_live_decision` (in `set_pending`) and `diag::assert_live_step`
  (each `advance` iteration) panic if the engine ever produces them, in debug and `paranoid`
  builds; the `c` encoding of the play action (EX used + `ALT_PLAY_BASE * (i + 1)`) is
  documented at `diag.rs:51-60`.
- **Evidence:** `p6j_runs.rs::check_fuzz` counts both on every decision of Python's fuzz games
  (`p6j_runs.rs::p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games` (features `slow` and `paranoid`, `cargo test --release -p gcg-diff --features paranoid,slow --test p6j_runs --target-dir target/paranoid -- --nocapture`): all 10,000 games of Python's `fuzz --games 10000 --seed 2026` replay to Python's per-game rows (856,034 actions): 0 PAYMENT decisions and 0 ATTACK_TRIGGERS steps over the 856,034 decisions); the tier-1 corpus has no `payment` decision and no snapshot at step
  50 (`grep -r '"payment"' oracle/corpus/v1/tier1` finds nothing; the snapshot steps are 0, 2,
  3, 12, 40, 51, 52, 54, 57, 60, 65, 66, 99); the paranoid corpus run executes the
  assertions on every replayed decision.
- **Ablation:** no AI effect (P6-J common row); the assertions are compiled out of release
  builds.

### W-124 — Derived-view invalidation by mutation kind is deferred — resolved (measured; mutation-kind masks not adopted)

- **Resolution as implemented (Phase 5):** the output-identical view work of PLAN §11.4,
  measured by instructions retired (docs/dev/BENCHMARKS.md "Phase 5 optimisation log"):
  the sparse derived view in recycled per-thread buffers (14.75G -> 6.75G instructions per
  standard game), the trigger index read off the cached view's hosts, and the skip of the
  confirming fixpoint pass when its reads repeat (-> 4.65G), with the `paranoid`
  skipped-pass check (`Check::SkippedPass`, 105,974 runs in `paranoid-games`, tier 1
  `--paranoid` 3804/3804). `view::derived` fell from 62.5% to 43.3% of the samples, the
  confirming `same_values` pass from 16.4% to 0 and dropping views from 12.0% to 0; the base
  view (21.8%) still runs once per `touch`, as in Python.
- **Decision on per-mutation-kind invalidation:** not adopted. What remains for it is the
  base view recomputed after mutations that leave the view unchanged (Python measured 74% of
  recomputations; the bound in Rust is the 21.8% base-view share plus the pass work). It
  needs a dependency analysis of every condition, filter and value, and the 21 view hooks
  and 13 step hooks are opaque to it, while the measured speed already clears the brief's
  floor by a wide margin on the rough loaded-machine runs (progress.md, Phase 5). It stays a
  Phase 5 candidate if the Part 1/Part 2 hyperfine runs need more; any version must keep the
  `paranoid` comparison with a full recompute and reproduce the W-48 cases above.
- **Evidence:** BENCHMARKS.md (Phase 5 log and profile tables); the paranoid checks; the
  corpus at 100%.
- **Ablation:** no AI effect (P6-J common row; Phase 5's changes are output-identical by the
  same corpus evidence).

### W-125 — Action and Decision construction cost — resolved (by construction)

- **Resolution as implemented (Phase 3):** `Action` is a 16-byte `Copy` value
  (`#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]`, `contracts.rs:96-102`) and the
  pending decision is a `Copy` header plus options in the state's reused `pending_options`
  buffer (`set_pending` clears and extends it, `state.rs`).
- **Evidence:** `p6j_engine.rs::w125_actions_compare_and_hash_by_value` (equal fields give
  equal values and equal hashes, a different field differs, `Copy`, 16 bytes). The
  micro-benchmark of the verification text is not added: the brief forbids benchmark code in
  the crates; the external comparison of BENCHMARKS.md is the only measurement.
- **Ablation:** no AI effect (P6-J common row).

### W-143 — Battle-damage redirect cycles recurse without bound — resolved

- **Resolution as implemented:** landed in L3 and kept: `core::follow_redirects`
  (`core.rs:1206-1227`) walks the REDIRECT_BATTLE_DAMAGE chain with the visited list and
  returns `EngineError::RedirectCycle { uids }` (the chain and the repeated Unit; `py_class`
  `RecursionError`) where Python recurses until `RecursionError`; every chain without a cycle
  lands where Python's does. The runner fails the game, and the benchmark aborts with exit
  status 1 and no reports, as Python does. The rules question (does each substitution apply
  at most once per damage event?) stays with the Python conflict workflow; the behaviour is
  unchanged.
- **Evidence:**
  - Unit: `p6j_engine.rs::w143_two_lunamarias_build_a_cycle_that_fails_the_game` (two
    GD04-062 and two GD04-095 on the testkit: `RedirectCycle [0, 1, 0]`; Python
    `RecursionError: maximum recursion depth exceeded` with the same calls);
    `w143_a_redirect_chain_lands_on_its_last_unit` (A -> B -> C lands on C with 2 damage, the
    enemy trashed, as in Python); `core::tests::redirect_chains_land_and_cycles_fail`; the
    `scenario/redirect-{chain,cycle}` core vectors.
  - Tripwire: `p6j_ir_tripwires.rs::w143_redirect_sources_are_reviewed` (exactly GD04-087 and
    GD04-095) with the negative control `w143_a_third_redirect_source_fails_the_tripwire`
    (a third card given GD04-095's script in a temporary copy); the registry-level twin
    `core::tests::redirect_sources_are_reviewed`.
  - e2e: `p6j_runs.rs::w143_a_benchmark_that_builds_the_cycle_fails_without_reports`: a
    mono-purple Minerva Squad deck (in the test) mirrored, `test-random` agents, bo1, seed 4,
    47 matches, 1 worker: `simulation failed: match 46 game 1 (seed 13255759802130423377)
    failed: RecursionError: battle damage redirect cycle through units [55, 96, 55]`, no
    report written; 46 matches complete with every report (control). The binaries, run by
    hand: `target/debug/gcg-eval reference-benchmark minerva.txt minerva.txt --agent
    test-random --format bo1 --matches 47 --seed 4 --workers 1 --out DIR` exits 1 with that
    message and no DIR; Python `GCG_SIM_AGENT_FACTORY=tests/runner/random_agent.py:make_random_agent
    uv run gcg-sim benchmark minerva.txt minerva.txt --format bo1 --matches 47 --seed 4
    --workers 1 --out DIR` exits 1 with `error: simulation failed: match 46 game 1 (seed
    13255759802130423377) failed: RecursionError: maximum recursion depth exceeded` and no
    DIR. Only the exception text differs.
  - The strengthened search's `engine_error_leaf` (P6-O) scores such a line inside a search
    instead of failing the real game.
- **Ablation:** no AI effect (P6-J common row); error path only.

### W-158 — A non-literal damage-prevention threshold reads as unlimited prevention — resolved

- **Resolution as implemented:** literal thresholds behave as before. The registry build
  refuses a CANT_RECEIVE_DAMAGE `RuleMod` whose `amount` is a node, with
  `IrError::Malformed { path: "$.nodes[i].amount", detail: "<card>: a CANT_RECEIVE_DAMAGE
  threshold that is not a literal int would be read as unlimited prevention (W-158)" }`
  (`crates/gcg-effects/src/ir.rs:884-911`, called from `Arena::decode_nodes`, so the full IR
  and every fragment are checked; the existing `Malformed` variant is used, so the frozen
  `IrError` enum is unchanged).
- **Evidence:** `p6j_ir_tripwires.rs::w158_no_prevention_threshold_is_computed` (0
  non-literal amounts; 125 RuleMods in the card scripts, 25 CANT_RECEIVE_DAMAGE on 24
  definitions, as the entry counted); `w158_a_counted_threshold_fails_the_tripwire_and_the_build`
  (negative control: GD05-123's threshold replaced by a `Count` node in a temporary copy is
  listed, and `Registry::from_ir` fails naming GD05-123); `w158_the_packaged_registry_builds`;
  GD05-123's literal threshold: the ported Q416 test in the traces (tier 1 at 100%).
- **Ablation:** no AI effect (P6-J common row); the packaged registry is unchanged.

### W-159 — Turn-history conditions match the card's current state — resolved (port-preserve + tripwire)

- **Resolution as implemented:** behaviour kept exactly (a rules interpretation, for the
  Python conflict workflow).
- **Evidence:** `p6j_engine.rs::w159_a_unit_that_is_a_pilot_again_does_not_count`: the pool
  reaches the case, so no test-only binding is needed: GD05-089 is deployed by its Burst as
  an (AP3･HP3) Unit (the Unit variant, def 1152), GD05-057 Gyunei's Jagd Doga (Neo Zeon)
  destroys it, and GD05-129 Axis's condition does not count it (the history entry is
  recorded, but the UNIT filter reads the Pilot in the trash): GD02-050 stays in the hand;
  destroying a printed Unit instead deploys it. Both equal Python's run, quoted in the test.
  Tripwire `p6j_ir_tripwires.rs::w159_history_filters_on_characteristics_are_reviewed`: the
  `HappenedThisTurn` filters reading a characteristic are exactly GD03-069 (PairedTo),
  GD03-081, GD04-069 (IsKind, HasTrait), GD05-089 (HasTrait), GD05-129 (IsKind; source
  HasTrait); GD03-001's identity-only `IsRef` is excluded. Negative control
  `w159_a_trait_filter_on_a_discard_history_fails_the_tripwire`.
- **Ablation:** no AI effect (P6-J common row).

### W-161 — ToDeck can offer duplicate ARRANGE options — resolved (port-preserve + tripwire)

- **Resolution as implemented:** behaviour kept exactly, duplicates included.
- **Evidence:** tripwire `p6j_ir_tripwires.rs::w161_no_todeck_can_return_a_unit_with_its_pilot`:
  of the 7 ToDeck steps, none can draw a paired Pilot (the selectors are the deck, the trash,
  the hand, one battle-area Unit, or the card itself), so the list is empty and the case is
  unreachable with today's pool; negative control `w161_a_unit_and_its_pilot_fail_the_tripwire`.
  Testkit regression on a test-only Command ("return an enemy Unit and its paired Pilot to
  the top of the deck"): `p6j_engine.rs::w161_duplicate_arrange_options_are_pythons` gives
  Python's ARRANGE options `[select(3), select(4), select(4)]`, a second ARRANGE
  `[select(4), select(4)]` when the Unit is chosen first, Python's decision counts (3 and 2
  actions) and deck order. Since the list is empty, no pool case goes to the lead as a
  Python bug; the test-only case shows the behaviour that a future card would hit.
- **Ablation:** no AI effect (P6-J common row).

### W-162 — Serialised-but-unread trigger fields — resolved (port-preserve + fuzz diagnostic)

- **Resolution as implemented:** every field and its serialised position stay. In `paranoid`
  builds the engine keeps, outside the game's value, each trigger's host `zone_seq` at queue
  time (`GameState.trigger_seqs`, parallel to the trigger list), and when a trigger resolves
  (past its once-key check) it records the host if its `zone_seq` changed since
  (`gcg_engine::paranoid::moved_hosts()`: card number, queued and resolved `zone_seq`). A
  【Destroyed】 trigger whose card stays in the trash is not recorded (its queued value is the
  post-move one). The unread fields (`TriggerInst.host_seq` beyond the once key,
  `TriggerInst.optional`, `Frame.host_seq`, `Frame.answer`, `Battle.blocked`, `damage_only`,
  `attack_triggers`, `vars["__play"]`) are serialised for digest identity and not read.
- **Evidence:** `p6j_engine.rs::w162_a_host_that_moves_before_its_trigger_resolves` (a
  test-only "【Destroyed】Choose 1 Unit card from your trash. Return it to your hand." on two
  copies destroyed together: the second copy's trigger resolves from the hand, recorded as
  `ST07-003` 1 -> 2 under `paranoid`; one copy alone is not recorded; decisions and zones equal
  Python's). Fuzz: `p6j_runs.rs::p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games` (features `slow` and `paranoid`, `cargo test --release -p gcg-diff --features paranoid,slow --test p6j_runs --target-dir target/paranoid -- --nocapture`): all 10,000 games of Python's `fuzz --games 10000 --seed 2026` replay to Python's per-game rows (856,034 actions). The diagnostic recorded **395 triggers in 350 of the 10,000 games whose host moved again before they resolved**, on 29 cards: GD03-052, GD03-058, GD03-060, GD03-065, GD03-066, GD03-067, GD03-068, GD03-076, GD04-052, GD04-053, GD04-054, GD04-056, GD04-058, GD04-059, GD04-060, GD04-062, GD04-063, GD04-126, GD05-018, GD05-034, GD05-050, GD05-051, GD05-052, GD05-053, GD05-056, GD05-060, ST07-006, ST13-001, ST13-003 (the first 200 games: 15 triggers in 11 games, on GD03-052, GD04-054, GD04-060, GD04-126, GD05-050). Where the hosts were: a one-off replay of the first 400 games on a `paranoid` debug build (a scratch test, not committed) found 21 such triggers; when each resolved, every card with the host's number and resolved `zone_seq` was in the trash in 20 cases (one case matched a card in the hand and one in the trash), and the decision before was a MAIN decision in 14 cases (its action was not recorded; an attack runs its whole battle inside that apply), an ORDER_TRIGGER at DAMAGE_DONE in 5, a BLOCK in 1 and an ACTION_STEP in 1. With the card texts ("when this Unit deals battle damage to an enemy Unit, ..." on GD03-052, GD04-054, GD05-050, GD03-076; keyword or Pilot-granted triggers on the vanilla hosts) this reads as battle-damage triggers whose host the same damage step destroyed before they resolved (`zone_seq` +1, into the trash); Python resolves them with the host in the trash. Per the resolution plan these go to the Python conflict workflow as a rules question (new location, last-known information, or nothing); the behaviour is unchanged. Final states equal Python's per-game rows (actions, end, turns); Python's fuzz rows carry no digests.
- **Ablation:** no AI effect (P6-J common row); the side table exists only in `paranoid`
  builds.

## Interface notes for the lead

- New public API (additive): `gcg_engine::diag` (`CapKind`, re-exported as
  `gcg_engine::CapKind`; `GameState::cap_kind`; `non_converged_views`), and under feature
  `paranoid` `gcg_engine::paranoid::{MovedHost, moved_hosts}`. docs/dev/contracts.md
  (gcg-engine) needs these lines; I did not edit it.
- `crates/gcg-engine/src/contracts.rs` (lead-owned) gained one private field,
  `#[cfg(feature = "paranoid")] pub(crate) trigger_seqs: crate::paranoid::TriggerSeqs`, on
  `GameState`. It is not part of `canonical_state`, `to_json`, any equality or any release
  build; `clone_from_state`'s exhaustive destructuring copies it. Please confirm or move it.
- `IrError` is unchanged: W-158 uses `IrError::Malformed`.
- For P6-M's `gcg-eval benchmark --diagnostics` (W-46): the cap kind is available on the final
  `GameState` of every game; nothing in `gcg-runner` was changed.
- `docs/dev/tests.json` has no ids for the new tests (lead file).

## Not done

- The `--diagnostics` side file and the head-to-head listing of capped games (W-46): P6-M and
  P6-I (outside this package's files).
- WEAKNESSES.md status lines (the lead's, §12.10).
