# P6-O weakness status: design decisions, the Mechanisms extension points, the engine-error leaf

Package P6-O (docs/port/PLAN.md §12.2) with the `gcg-search` extension points of §12.3 (the P6-D
core) and the engine-error leaf for W-143/W-110. Written by the P6-O implementer on 2026-11-02
in worktree branch `worktree-wf_ab32af5e-78f-3`, from `3fbb8aa`. WEAKNESSES.md itself is not
edited here (the lead records the status lines from this file, §12.10).

Gate run after the last edit (commands and results at the end of this file): `cargo fmt --all
--check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test
--workspace --release`, the `gcg-search` slow target, and `gcg-diff corpus --tier 1
--require-all`.

## Shared work: the extension points (PLAN §12.3)

- `crates/gcg-search/src/mech/mod.rs`: the `Mechanisms` trait with `root_moves`, `budget`,
  `world`, `priors`, `select`, `rollout_choice`, `horizon`, `leaf_value`, `backup`, `pregame`,
  plus `error_leaf` (P6-O, below). Every default is the reference behaviour, split line by line
  out of the reference search in `crates/gcg-search/src/mech/reference.rs`. The contexts
  (`RootCx`, `WorldCx`, `NodeCx`, `RolloutCx`, `LeafCx`, `BackupCx`, `PregameCx`) carry only the
  observer's information-set state or worlds determinised from it (W-135). `Composed` dispatches
  by the flags; `validate` refuses a configuration that enables a mechanism no package has
  implemented (`mechanism <name> (<pkg>) is not available in this build`), so a flag is never
  silently ignored. Each package adds `mech/<pkg>/` with a `PROVIDES` list and one line in
  `available`.
- `crates/gcg-search/src/mcts.rs`: the core loop over `M: Mechanisms` (static dispatch), with
  the reference's exactness rules (seed order `W[0:B]`, `R[0:B]`, `W[B:cap]`, `R[B:cap]`, first
  maxima, `py_sum`, per-visit differences, duplicate backup paths). `HorizonDecision::Extend`
  continues a stopped rollout; after the first stop an extension is followed only while the
  continued rollout applies something, so a mechanism cannot loop the core.
- `crates/gcg-search/src/flags.rs`: `MechanismFlags`, one field per planned mechanism of
  §12.2/§12.3 (52 fields, grouped by package; P6-A `adaptive_horizon`, `generalized_leaf`,
  `capped_rollout_continuation`; P6-B six flags; P6-C `opponent_buckets`, `belief_worlds`,
  `belief_last_n`, `exact_pools`, `opponent_view_share`, `opponent_list`; P6-D seven; P6-E
  seven; P6-F ten; P6-G nine; P6-H two; P6-O `portable_math`, `engine_error_leaf`), all off in
  the null configuration, no `Default`. Reflection by name (`FIELDS`, `get`, `set`, `set_text`,
  `from_values`, `enabled`) lets `gcg-eval` files and `search:<preset>:field=value` overrides
  flip any flag without per-field code. `StrengthenedConfig::mechanisms` holds them.
- `crates/gcg-search/src/presets.rs`: `standard` = reference `strong` budgets, `fast` =
  reference `standard` budgets (renamed, `ROLE_WEIGHTS`), every field spelled out, all
  mechanisms off except `engine_error_leaf`.
- `crates/gcg-search/src/tree.rs`: the tree arenas, shared by the core and the mechanisms.
- Outside the owned files (needed for `cargo test --workspace` to compile; flagged to the
  lead): `crates/gcg-eval/src/search_config.rs` reads and writes the `mechanisms` object (every
  field required, unknown fields refused), and `crates/gcg-eval/src/agent_spec.rs` falls back to
  `MechanismFlags::set_text` for `search:` overrides and checks `check_mechanisms`; tested by
  `search_config::tests::round_trip_and_strictness` (extended) and
  `agent_spec::tests::search_overrides_reach_the_mechanism_flags` (new).

Evidence (null configuration replays the reference bit for bit):

| Test | What it checks | Result |
| --- | --- | --- |
| `crates/gcg-search/tests/null_config.rs::the_copy_replays_the_reference_on_every_vector_decision` | 213 searches of the D2 vectors (`fast`, `standard`, `strong`, `baseline-standard`): action, alternative bits, stats, RNG state equal the reference; the same with `engine_error_leaf` on, 0 error leaves | pass |
| `tests/null_config.rs::the_copy_replays_the_reference_pregame_choices` | 12 REDRAW + 6 CHOOSE_FIRST runs through `gcg_search::pregame` on the observation equal `mulligan_choice`/`first_player_choice` on the true state (which equal Python, `crates/gcg-ai/tests/search_components.rs`, `search_engine.rs`) | pass (18) |
| `tests/null_config.rs::null_configurations_copy_every_reference_field` | exhaustive field copy; the shipped presets are the mapped reference budgets plus `engine_error_leaf` | pass |
| `tests/null_config_traces.rs` | the 19 searched decisions of the tier-1 `search` traces that have a snapshot | pass |
| `tests/null_config_search_set.rs` (feature `slow`, new) | every one of the 661 searched decisions of the tier-1 `search` set, replayed along each trace (digest prefix checked before every decision): recorded action, alternatives, iterations, budget, cap, roots and RNG state, for the reference, the copy and the copy with `engine_error_leaf`; every REDRAW/CHOOSE_FIRST decision through `pregame` | pass (73 s) |
| `src/mech/tests.rs` (7 tests) | defaults = composed null configuration; error leaf off/on; unimplemented card never absorbed; errors outside simulated lines propagate; a no-op extension terminates with the reference result; an extension rolls out further; `portable_math` keeps roots and budgets | pass |

## W-33 — AI module-level caches are unbounded and not invalidated on script override

- **Status:** resolved.
- **Resolution as implemented:** by construction in Phase 3, confirmed here. The port has no
  process-global AI table: `command_harms_enemy_units`, `choice_polarity`,
  `choice_rule_lasts_this_battle` (`crates/gcg-ai/src/targeting.rs:7-11`) and
  `chosen_only_to_rest` (`crates/gcg-ai/src/actions.rs:348-389`) recompute from the `&Registry`
  they are given (contracts O10). Every registry, each `with_fragment` override included, has a
  fresh `generation` (`crates/gcg-effects/src/registry.rs:24-35`, `:107-117`, `:130-133`), so a
  per-registry table, if Phase 5 ever adds one, has a key. No `gcg-ai` source was edited.
- **Evidence:** `crates/gcg-search/tests/ai_caches.rs` (new, 2 tests, pass):
  `per_instruction_classifications_follow_each_registrys_program` builds two ST01-014 overrides
  that give the Command the same program id (626) with different instructions ("choose 1 enemy
  Unit, rest it" and "... if you do, draw 1") and checks, in the orders A-B-A and B-A-B, that the
  rest-target prune drops the rested target only under the first (a `(program_id, pc)` memo
  would answer the second with the first's result); `every_override_is_a_new_registry_generation`
  checks fresh generations for four overrides and that the base registry's harm classification
  of ST01-014 is unchanged by anything computed on them. Both searches' outputs on the
  equivalence seeds: `gcg-diff corpus --tier 1 --require-all` 100% (search and baselines sets
  included) and the null-config tests above.
- **Ablation row:** no AI effect. Built in by construction (no off switch); evidence per A13:
  the `search` and `baselines` corpus sets at 100% against Python's traces (gate below), plus
  the null-config tests.

## W-131 — Multiplayer (rules section 12) is out of scope

- **Status:** admissible exception (design decision of the reference: two-player Standard play
  only). No work.
- **Ablation row:** no AI effect: an admissible exception, no code changed.

## W-132 — No sideboarding in BO3

- **Status:** admissible exception (design decision: BO3 without sideboarding, "as the task
  specifies"). No work.
- **Ablation row:** no AI effect: an admissible exception, no code changed.

## W-133 — No clock: time limits, extra turns and tiebreaks are not modelled

- **Status:** admissible exception (design decision: simulated games have no wall-clock
  dimension; budgets and determinism forbid one). No work.
- **Ablation row:** no AI effect: an admissible exception, no code changed.

## W-135 — Information-set honesty is a convention, not enforced by the API

- **Status:** resolved, with the D10 deviation recorded below.
- **Resolution as implemented:**
  - `crates/gcg-search/src/observation.rs`: `Observation<'a>` holds no reference to the true
    state. `Observation::new(st, player)` stores the observer's information-set state
    `determinize(st, player, OBSERVATION_SEED)`: every identity hidden from the observer is
    replaced from the observer's own pools, every history entry renamed to its card's new
    identity, the game generator replaced and the view cache cleared, so the hidden
    identities, the hidden history and the true generator are not in it. The field and its
    accessor are private to the module (`pub(crate) fn info`); the public API is `new` and
    `player`. The lifetime ties an observation to its decision.
  - Worlds are built from the observation: `determinize(info, player, seed)` is the world the
    reference builds from the true state with that seed (same cards, history, generator,
    `rules_checked`), with `version` one higher (one more `touch`). The engine reads `version`
    only through `rules_checked == version` (`core.rs:1702`), false at a world's first check
    either way (`rules_checked < version`, asserted), so every decision is the reference's.
  - `crates/gcg-search/src/agent.rs`: `choose`/`choose_traced` use the true state only in
    `AgentCore::begin`, `Observation::new` and `AgentCore::finish` (lint §9.9 item 5); REDRAW,
    CHOOSE_FIRST (`gcg_search::pregame`) and the search read only the observation.
  - D10 deviation from the plan text: the reference search and greedy keep `&GameState`,
    because their `derived()` calls fill the true state's view cache exactly as Python's do and
    the cache is part of byte equivalence; the shared `Agent` trait (`gcg-ai`, frozen) still
    passes `&GameState` to every agent. The plan's "agent code cannot name `GameState`" is
    therefore met for the strengthened search's code path (the search and the mechanisms get
    only the observation), not for every agent.
- **Evidence:**
  - Compile-fail doctests in `observation.rs` (pass), each confirmed to fail with exactly the
    stated error by compiling the snippets in a scratch example (stable rustdoc does not check
    codes): private field `E0616`, private accessor `E0624`, no identity method `E0599`, no
    `AsRef<GameState>` `E0277`, an observation cannot outlive its decision `E0515`.
  - `observation::tests::worlds_from_the_observation_are_the_reference_worlds` (unit, pass): 452
    states (144 D2 vector snapshots, 37 tier-1 `search` snapshots, 271 tier-1 `worlds`
    snapshots) x 2 observers x 3 seeds: canonical JSON of the observation's world equals the
    reference world's with `version` + 1, `rules_checked` equal.
  - `observation::tests::the_observation_depends_on_the_information_set_alone` (unit, pass): for
    the same states, observers and seeds, two other true states with the same information set
    (hidden identities resampled; hidden main deck permuted) give the identical observation,
    and `information_set_key` of the observation equals the true state's.
  - `crates/gcg-search/tests/observation.rs::the_strengthened_decision_is_invariant_under_hidden_permutation`
    (pass): a `search-fast` agent makes the same move with the same logged statistics on the 25
    mid-game states of `test_information_set.py` and 3 hidden permutations each, and leaves the
    true state's digest unchanged.
  - Lint rule 5: `crates/gcg-diff/tests/lints.rs::the_repository_passes_every_lint` (pass, in
    the workspace run).
  - The null-config tests above (every search-set decision bit for bit).
- **Ablation row:** no AI effect. The strengthened search now reads an observation instead of
  the true state; under the null configuration it makes the reference's decisions on every
  searched decision of the tier-1 `search` set (661), the 213 vector searches and the 18 vector
  pregame runs (tests above); the reference is unchanged (corpus 100%).

## W-136 — SearchConfig defaults reproduce the pre-change AI; baseline presets live in code

- **Status:** resolved.
- **Resolution as implemented:** `ReferenceConfig` (`gcg-ai-reference`, Python's fields and
  defaults, reachable only through `gcg-eval`) and `StrengthenedConfig`
  (`crates/gcg-search/src/contracts.rs`) are separate types with no conversion in shipped code.
  Neither `StrengthenedConfig` nor its `MechanismFlags` has a `Default`; both shipped presets
  (`presets.rs`) and the null configuration (`tests/common/mod.rs`) spell out every field, and
  a `gcg-eval` configuration file must name every field and every mechanism
  (`crates/gcg-eval/src/search_config.rs`).
- **Evidence:** compile-fail doctests (pass; codes confirmed as for W-135): `contracts.rs`
  `StrengthenedConfig { ..Default::default() }` `E0277`; a literal without `mechanisms`
  `E0063`; `make_agent(&gcg_ai_reference::STANDARD, ..)` `E0308`; `let _: StrengthenedConfig =
  gcg_ai_reference::STRONG.clone()` `E0308`; `flags.rs` `MechanismFlags { ..Default::default()
  }` `E0277`. `tests/null_config.rs::null_configurations_copy_every_reference_field` destructures
  `ReferenceConfig` exhaustively. `gcg-eval` `search_config::tests::round_trip_and_strictness`
  (pass) refuses a file without a mechanism field, with an unknown one or a wrongly typed one.
  `presets::tests::unavailable_mechanisms_are_refused` (pass). The reference side of the
  verification: `crates/gcg-ai-reference/tests/baseline_game.rs::baseline_standard_replays_the_game_recorded_before_the_changes`
  (bc38ab4 game) and `presets_vectors.rs::every_reference_configuration_equals_the_live_python_object`
  (d9d41af presets), both pass in the workspace run.
- **Ablation row:** no AI effect: types and construction only; the shipped presets' field
  values are the mapped reference values (`null_configurations_copy_every_reference_field`) and
  the null configuration replays the reference (tests above).

## W-137 — The determinism contract; cross-platform identity depends on the C math library

- **Status:** open (the parts in this package's scope are done; three remain, listed below).
- **Resolution as implemented here:** the `portable_math` mechanism
  (`crates/gcg-search/src/mech/p6o/math.rs`): with the flag on, the strengthened search's own
  math (prior softmax `exp`, UCB `ln`, the close-call squares as `x * x`, the logistic leaf, the
  rollout policy's Gumbel `ln(-ln(u))`) uses `gcg_core::portable` (the pure-Rust `libm` crate);
  with it off (null configuration and both shipped presets today) the platform libm, exactly the
  reference. Already in place from earlier phases: the lint (`gcg-diff lints` rules 2-3,
  `clippy.toml` bans), the named golden platform (PLAN A1), `erfc` through the FFI of
  `crates/gcg-core/src/libm.rs` (lead decision A2, option a), identical outputs for any worker
  count.
- **Evidence:** `mech::p6o::math::tests::platform_is_the_reference_and_portable_is_the_libm_crate`
  (platform functions are bit-identical to `gcg_core::libm`/`gcg_ai::logistic`, portable ones to
  `gcg_core::portable` and `x * x`); `portable_exp_and_ln_are_within_one_ulp_of_the_golden_platform`
  (8,000 recorded `exp`/`log` inputs of `port/oracle/export_vectors.py`: every portable result
  within one ulp of Apple libm, 7,358 bit-identical); `mech::tests::portable_math_runs_the_same_search_with_portable_functions`
  (same roots and budgets, the same move on at least 3 of 4 vector decisions); all pass. Existing:
  `crates/gcg-core/tests/vectors.rs::libm_parity_matches`,
  `crates/gcg-cli/tests/benchmark.rs::needs_engine_outputs_are_identical_for_any_worker_count`,
  `crates/gcg-eval/tests/e2e.rs::head_to_head_is_identical_at_1_4_and_10_workers` (slow),
  `crates/gcg-diff/tests/lints.rs`.
- **Open:**
  1. With `portable_math` on, shared `gcg-ai` code still calls the platform libm inside the
     strengthened search: the no-op filter compares two `evaluate` values
     (`crates/gcg-ai/src/noop.rs:196`, logistic at `evaluation.rs:271-277`) and the play/draw
     simulations use `square` (`setup.rs:283`) and the Gumbel `ln` of `policy.rs:114`. Making
     them portable needs a math parameter in `gcg-ai` (interface change, `gcg-ai` is frozen and
     under Phase 5) or the P6-E/P6-H replacements of those paths.
  2. The cross-platform check (strengthened presets bit-identical on macOS arm64 and Linux
     x86-64, CI matrix) needs a Linux runner (M17).
  3. The shipped presets do not enable `portable_math` yet; adopting it is a preset decision
     with its own ablation row (PLAN §12.4, §12.8).
- **Ablation row:** no AI effect for the shipped presets (`portable_math` off; null-config tests
  above). The flag's individual and leave-one-out rows are not measured: they need the
  `gcg-eval ablation` tool (P6-I) and the `gcg-ai` paths of open item 1.

## Engine-error leaf (contribution to W-143 and W-110; both items belong to other packages)

- **Status:** this package's part is done; W-143's status is P6-J's (the `RedirectCycle`
  error, the tripwire) and W-110's is P6-N's (the strategic fuzz gate, T-056).
- **Resolution as implemented:** `engine_error_leaf` (`crates/gcg-search/src/mech/p6o/mod.rs`,
  the `Mechanisms::error_leaf` hook and the core's catch in `mcts.rs` `Search::iterate`): an
  `EngineError` raised anywhere inside a simulated line (the root move in world k, the tree
  descent, the priors at a node, the rollout policy and applies, the leaf evaluation) is scored
  as a loss of the searching player at the turn reached (the reference's loss value
  `WIN_DECAY * (turn - root_turn)`), backed up along the path (the failing move included) and
  counted in `MechanismEffort::engine_error_leaves` (per search through
  `gcg_search::search_with_effort`, per agent through `SearchAgent::mechanism_effort`). Errors
  outside simulated lines (root moves, world building, the observation) and the real game's own
  `apply` still propagate, and `UnimplementedCardError` is never absorbed. On in both shipped
  presets; off in the null configuration.
- **Evidence:**
  - `crates/gcg-search/tests/engine_error_leaf.rs` (pass, 6-8 s):
    `the_failing_fuzz_game_completes_with_the_shipped_preset` runs
    `gcg_diff::fuzz::play_one(index 221)` of `gcg-diff fuzz --games 400 --seed 2026 --agent
    search:fast` (game seed 11012090770908643750): it completes, and the same game through
    concrete `SearchAgent`s ends with the same number of actions and a positive
    `engine_error_leaves`; `without_the_flag_the_cycle_fails_the_game_as_before` (negative
    control): with the flag off the same game fails with `RedirectCycle [36, 3, 36]` and no
    error leaf.
  - `src/mech/tests.rs` (unit, pass): the error fails the search without the flag; with it
    every failing line is counted; an unimplemented card propagates; root-move and world
    errors propagate with the flag on.
  - Strategic fuzz (not paranoid; T-056's paranoid runs are P6-N's):
    `target/release/gcg-diff fuzz --games 400 --seed 2026 --agent search:fast --workers auto`:
    400 games, 27,490 actions, 400 battle-damage ends, 0 failures (game 221 failed before);
    `--games 200 --seed 2026 --agent mixed:greedy,search:fast`: 200 games, 14,558 actions,
    0 failures. Both print `fuzz passed`.
- **Ablation row:** the flag changes a decision only where a simulated line fails, where the
  reference fails the whole game instead. Evidence of no effect elsewhere: with the flag on, the
  decisions, alternatives, statistics and draws equal the reference's on every searched
  decision of the tier-1 `search` set (661) and the 213 vector searches, with 0 error leaves
  (`null_config_search_set.rs`, `null_config.rs`).

## Interface notes for the lead

- `gcg-search` public API added: `MechanismFlags`, `FlagValue`, `OpponentList`,
  `MechanismEffort`, `StrengthenedConfig::mechanisms` and `check_mechanisms`,
  `search_with_effort`, `pregame`, `make_search_agent`, `SearchAgent::mechanism_effort`.
  Changed: `Observation::new` is `pub fn new(st, player) -> Result<Self, EngineError>` (it
  determinises; it was a `const fn` returning `Self`), and the crate-private `state()` became
  `info()` (the information-set state, not the true state). `search`'s signature is unchanged.
  docs/port/contracts.md (gcg-search section) needs these lines.
- `gcg_ai::SearchEffort` (frozen) has no engine-error counter, so the counter lives in
  `gcg_search::MechanismEffort`; results schema 1.2.0 can report it if the lead adds it.
- The two `gcg-eval` edits listed above are outside this package's files.
- `docs/port/tests.json` has no ids for the new tests (lead file).

## Gate (run after the last source edit, 2026-11-02, M2 Pro)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release` | 895 passed, 0 failed (baseline at `3fbb8aa`: 864) |
| `cargo test -p gcg-search --release --features slow` | every target passes, `null_config_search_set` included (661 searched decisions) |
| `cargo test -p gcg-eval --release --features slow --test e2e` | 8 passed, 0 failed (`preset_benchmark_runs_a_configuration_file`: the `mechanisms` object round-trips through `preset-benchmark --search-config`; `workers::head_to_head_is_identical_at_1_4_and_10_workers_for_every_player`) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces; search 9/9 (771 decisions, 771 agent blocks), baselines 8/8 (625, 625) |
| `target/release/gcg-diff fuzz --games 400 --seed 2026 --agent search:fast --workers auto` | 0 failures |
| `target/release/gcg-diff fuzz --games 200 --seed 2026 --agent mixed:greedy,search:fast --workers auto` | 0 failures |
