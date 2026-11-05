# Final report: the Rust port of gcg-sim

Branch `rust-port` (from `main` at d9d41af). Specification: `docs/port/BRIEF.md`. Every number
below was produced by a command run in this project and is recorded in the file cited next to
it. Status: **not complete against the brief**: two acceptance criteria are not met (E4 against
greedy on the AI test decks, and "every WEAKNESSES item resolved"), the reviewer's sign-off is
partial, and CI on GitHub has never run because pushing needs the user's approval.

## Results: Python vs Rust (M2 Pro, idle machine)

Workload `benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt
--format bo3 --seed 1`; hyperfine through `scripts/compare_python_vs_rust.sh` (outside the
crates); Python baselines measured once (commit d9d41af, best worker count 10). Final session:
binaries of 8c89b5e (`docs/port/BENCHMARKS.md`); the reviewer's rerun at HEAD 2d425db is within
0.4 s of it (`docs/port/REVIEW.md` §6.1).

| Part | Configuration | Python | Python median | Rust | Rust median | Speedup | Reviewer at HEAD |
| --- | --- | --- | ---: | --- | ---: | ---: | ---: |
| 1 A (equal work) | headline, best workers | `standard` m100 w10 | 972.0 s | ref `standard` m100 w6 | 41.3 s | 23.5x | 23.59x |
| 1 A | headline, 10 workers | `standard` m100 w10 | 972.0 s | ref `standard` m100 w10 | 47.8 s | 20.3x | 20.34x |
| 1 A | single-thread | `standard` m20 w1 | 1269.5 s | ref `standard` m20 w1 | 28.0 s | 45.3x | 45.31x |
| 1 B (equal work) | full search | `strong` m20 w10 | 356.0 s | ref `strong` m20 w6 | 14.4 s | 24.7x | 24.66x |
| 2 (shipped presets) | `fast` headline, best workers | `standard` m100 w10 | 972.0 s | `fast` m100 w6 | 82.1 s | 11.8x | 11.82x |
| 2 | `fast` headline, 10 workers | `standard` m100 w10 | 972.0 s | `fast` m100 w10 | 93.8 s | 10.4x | 10.37x |
| 2 | `fast` single-thread | `standard` m20 w1 | 1269.5 s | `fast` m20 w1 | 60.0 s | 21.2x | 21.25x |
| 2 | `standard` full search | `strong` m20 w10 | 356.0 s | `standard` m20 w6 | 148.6 s | 2.40x | 2.40x |

`report`: "all 8 configurations present, current and at least 2x" (both sessions). `standard`
spends 8x the reference `strong` budget plus the adopted mechanisms, so it is the one
configuration below 10x; every equal-work configuration is above 20x.

## Equivalence: exact

- Engine: the differential corpus replays Python's traces through the Rust engine with no
  divergence: tier 1 3804/3804 (17,601 decisions), tier 2 4154/4154 (123,726 decisions, 12,131
  reference-search decisions), also in the paranoid build and the apple-m2 dist build.
- Reference search, byte-identical normalised `results.json`, `games.ndjson`, `replays/*.json`
  and `summary.md`: the Phase 4 matrix 324/324 (3 seeds x 3 pairings x bo1/bo3 x
  fast/standard/strong x decision log off/on x workers 1/4/10), the 100-match headline 3/3, and
  the reviewer's rerun at HEAD on fresh review seeds 129/129 (43 configurations; the full matrix
  was not re-run at HEAD). Every saved replay verifies.
- No statistical fallback is used. Known differences D1-D7 (`docs/port/EQUIVALENCE.md`) change
  no compared byte: CPython-internal error texts (prefix matched), a redirect-cycle error message
  instead of `RecursionError`, u32 limits on `--matches`/`--replays` above 4.29e9, fixed-width
  replay fields, JSON parser edge cases (`NaN`), `timing.json` fields (excluded), and keyword
  order in the derived view (unobservable). Equivalence is defined on the golden platform
  (macOS arm64, Apple libm).

## Presets

| Preset | Starts from | Budget (per option / min / max) | Mechanisms | Measured strength (final evaluation 2) |
| --- | --- | --- | --- | --- |
| `standard` (default) | reference `strong` | 1024 / 2560 / 10240 (8x) | adaptive_horizon, capped_rollout_continuation, tactical_solver, exclude_capped_play_draw, engine_error_leaf, kill_aware_targeting, tree_lethal_select | vs ref `strong` 0.572 [0.563, 0.581]; vs random 400/400 and 11186/11200; vs greedy 0.783 [0.776, 0.791] (panel), 0.760 [0.716, 0.799] (test decks) |
| `fast` | reference `standard` | 96 / 240 / 960 (1.5x) | the same | vs ref `standard` 0.558 [0.549, 0.568] |

Strength gates (`docs/port/eval/final-2/README.md`, eval masters 90300001-6; reviewer's
review masters in `docs/port/eval/review/`):

| Gate | Final evaluation 2 | Review seeds | Brief bar |
| --- | --- | --- | --- |
| E1 `standard` vs ref `strong` | 0.5721 [0.5630]; held-out [0.5480] | 0.5708 [0.5616]; held-out [0.5528] | met (pooled LB >= 0.55, no held-out pair UB < 0.5); the plan's held-out-only reading misses by 0.002 in final 2 |
| E2 `fast` vs ref `standard` | 0.5584 [0.5492] | 0.5679 [0.5588] | met |
| E3 `standard` vs random | 400/400; 11186/11200, 14 losses replayed and verified | 400/400; 11181/11200, 19 losses replayed and verified | met (every loss documented with its replay) |
| E4 `standard` vs greedy | panel [0.7757], held-out [0.7670]; test decks 304/400 [0.7158] | panel [0.7743], held-out [0.7623]; test decks 305/400 [0.7184] | **not met on the AI test decks** (bar LB > 0.764, on three seed sets including final 1's 0.7237) |

Every Phase 6 scenario test passes, and every mechanism has individual and leave-one-out
ablation rows (`docs/AI.md`, `docs/port/WEAKNESSES.md`).

## WEAKNESSES.md (164 items)

128 resolved, 30 resolved by measurement (not adopted: the mechanism was implemented,
scenario-tested and measured null or negative, so the weakness remains in the shipped AI),
3 admissible exceptions (multiplayer, sideboarding, clock), 3 open: W-18 (`fast`'s fidelity on
one weakened variant, -0.075 [-0.146, -0.003]), W-19 (E4) and W-137 (the Linux x86-64 run).
W-120 was resolved with the user's approval of the settings change on 2026-11-05.
`gcg-diff weaknesses --check` passes; `--require-resolved` lists those 3.

| Item | Title | Status | Evidence (first cited) |
| --- | --- | --- | --- |
| W-01 | Two-turn rollout horizon: plans that pay off later rest on the evaluat | resolved | `crates/gcg-search/tests/scenarios/p6a_w01_two_turn_plan.rs` |
| W-02 | Leaf-value constants cap the horizon at four turns | resolved (not adopted) | `crates/gcg-search/src/mech/p6a/tests.rs` |
| W-03 | Lethal-threat defence: the audited "keep the last Blocker" position is | resolved | `crates/gcg-search/tests/scenarios/p6b_w03_last_blocker.rs` |
| W-04 | Budget plateau: strength is model-limited past about 2x budget; `stron | resolved | `crates/gcg-search/tests/scenarios/p6d_determinism.rs` |
| W-05 | Strategy fusion: opponent nodes pool statistics across hidden worlds ( | resolved (not adopted) | `docs/port/eval/GAP-A1/weakness/census-report.json` |
| W-06 | Observation keys count one child twice when the opponent passes twice  | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-07 | Determinization samples hidden cards uniformly: no inference from the  | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6c_w07_open_resources.rs::w07_belief_worlds_hold_back_from_the_inferred_trick` |
| W-08 | Determinization pads or trims pools when effects move cards between pl | resolved (not adopted) | `crates/gcg-engine/tests/gapb_exact_pools.rs::w08_a_card_in_the_other_players_deck_is_padded_by_the_reference_and_exact_here` |
| W-09 | Move canonicalization never pays with an EX Resource on purpose | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6e_w09_ex_payment.rs::ex_sensitive_plays_keep_their_ex_payment` |
| W-10 | Equivalent-move merge fingerprint is conservative and depends on insta | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-11 | Pruning and merging run only at the root; no transposition or tree reu | resolved (not adopted) | `docs/port/eval/P6-D/regret-*.json` |
| W-12 | Root pruning is judged in one fixed determinization by the determinist | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6e_w12_hidden_burst.rs::the_no_effect_check_keeps_a_move_that_matters_in_some_worlds` |
| W-13 | The "while rested" Rest-target exception was added after the measureme | resolved | `crates/gcg-search/tests/scenarios/p6e_w13_freedom.rs::both_searches_rest_the_rested_unit_and_keep_the_lethal` |
| W-14 | Close target choices stay noisy: the <Repair> ping puzzle is solved ab | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6e_w14_target_critic.rs::the_target_critic_solves_the_repair_ping` |
| W-15 | Undocumented rollout cap of 250 decisions: capped rollouts are evaluat | resolved | `docs/port/eval/weakness-status/P6-A.md` |
| W-16 | Budget is fixed by option count; only effect-target decisions may exte | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6b_w16_fast_lethal.rs` |
| W-17 | Remaining implausible plays: passivity, questionable attacks, missed l | resolved | `docs/port/eval/final-2/audit/` |
| W-18 | `fast` rates are not `standard` rates; fidelity rests on one weakened  | open (verification not met) | `crates/gcg-search/tests/scenarios/p6d_fast_preset.rs` |
| W-19 | Strength acceptance bars are thin floors | open (E4 not met) | `crates/gcg-eval/tests/phase6.rs::w19_gates_compute_e1_to_e4_and_fail_below_a_bar` |
| W-20 | Strength evidence rests on few decks, and the tuning decks overlap the | resolved | `crates/gcg-eval/tests/phase6.rs::w20_overlaps_reproduce_the_recorded_values` |
| W-21 | The decision log keeps the top 5 alternatives and does not record sear | resolved | `crates/gcg-eval/tests/phase6.rs::w21_decision_traces_change_no_report_and_count_every_iteration` |
| W-22 | Greedy baseline: one determinization, baseline knobs, fixed choices, s | resolved | `crates/gcg-eval/tests/phase6.rs::w22_greedy_cap_hits_are_counted` |
| W-23 | Hand-written playout policy: Commands under-cast, abilities over-activ | resolved (not adopted) | `docs/port/eval/P6-F/policy-fit.json` |
| W-24 | Modal choices are scored flat | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6f_w24_modal.rs` |
| W-25 | The opponent is modelled with the searcher's own policy | resolved | `crates/gcg-search/tests/scenarios/gapa1_w25_over_extension.rs::the_reference_over_extends_and_a_competent_opponent_model_does_not` |
| W-26 | Kill-aware and damage-payoff targeting were measured and removed | resolved | `docs/port/eval/GAP-A1/weakness/damage-regret-report.json` |
| W-27 | YES_NO answers say no about 12% of the time; the policy always goes fi | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-28 | End-phase action-step plays are under-explored by rollouts | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6f_w28_end_phase.rs` |
| W-29 | No policy scorer for ORDER_TRIGGER decisions | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6f_w29_order.rs` |
| W-30 | Target polarity classification is coarse | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-31 | Heuristic card and fight values are rough | resolved (not adopted) | `docs/port/eval/GAP-A2/w31_keyword_worth.py` |
| W-32 | AI Blocker counts ignore CANT_BLOCK and NO_BLOCKER_VS | resolved | `crates/gcg-search/tests/scenarios/p6b_w32_cant_block.rs` |
| W-33 | AI module-level caches are unbounded and not invalidated on script ove | resolved | `crates/gcg-search/tests/ai_caches.rs` |
| W-34 | Evaluation fitted on pre-change search games; refit and per-preset fit | resolved (not adopted) | `docs/port/eval/P6-G/fit-round1.json` |
| W-35 | The evaluation is linear; interaction terms and other candidate featur | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-36 | The evaluation was trained on three tuning decks and transfers weakly  | resolved (not adopted) | `docs/port/eval/P6-G/fit-deck-transfer.json` |
| W-37 | The evaluation ignores most keywords, card text and effects | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-38 | can_kill and race features are crude board heuristics | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-39 | The negative tempo weight is an artifact of when positions are recorde | resolved | the entry in WEAKNESSES.md |
| W-40 | Weight-tuning methodology limits | resolved | `crates/gcg-eval/tests/phase6.rs::w40_tune_is_deterministic_and_refits_recorded_positions` |
| W-41 | Matchup estimates are not calibrated against real tournament results | resolved | `crates/gcg-eval/tests/phase6.rs::w41_calibrate_on_synthetic_fixtures` |
| W-42 | No paired analysis of same-seed runs; compare reports a pooled Wilson  | resolved | `crates/gcg-eval/tests/vectors.rs::newcombe_matches_the_python_reference` |
| W-43 | Decision-level regret (oracle replay) tooling was never committed | resolved | `crates/gcg-eval/tests/phase6.rs::w43_regret_of_an_agent_against_itself_is_zero` |
| W-44 | Play/draw choice: policy-vs-policy simulations, biased to going first, | resolved | `crates/gcg-search/tests/scenarios/p6h_pregame.rs::w44_unfinished_play_draw_games_leave_the_paired_estimate` |
| W-45 | Mulligan heuristic: greedy four-turn opening score with a fixed 90% th | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6h_pregame.rs` |
| W-46 | Safety caps: 200 turns and 20,000 decisions share one end reason; the  | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w46_both_caps_report_turn_limit_and_name_the_cap` |
| W-47 | No guard against decision-free loops inside advance() | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w47_mutually_retriggering_delayed_triggers_end_in_no_progress` |
| W-48 | The derived-view fixpoint is silently capped at six passes | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w48_gd02_076_with_a_lasting_ap_plus_one_gains_blocker` |
| W-49 | The derived-view cache relies on manual touch() calls and a heuristic  | resolved | `crates/gcg-engine/src/contracts.rs` |
| W-50 | The listened-events fast path depends on the decklists; the listen cac | resolved | `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w50_the_derived_always_set_is_within_pythons` |
| W-51 | The rules-management HP shortcut depends on a registry-global "no HP-l | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w51_an_hp_lowering_override_leaves_no_lethal_damaged_unit` |
| W-52 | GameState.clone shares pending, the derived view and history entries ( | resolved | `crates/gcg-search/tests/ai_search_side_effects.rs` |
| W-53 | Registry indices are process-local, so saved states do not reload fait | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w53_a_saved_state_reloads_in_a_fresh_process` |
| W-54 | Dead enum values: the PAYMENT decision kind and Step.ATTACK_TRIGGERS | resolved | `crates/gcg-diff/tests/p6j_runs.rs` |
| W-55 | Interpretations and N/A rules that hold only for the current card pool | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w55_bases_have_zero_ap_with_negative_control` |
| W-56 | Rules-interpretation decisions the port must keep, pinned by rulings a | resolved | `crates/gcg-engine/tests/p6k_audit.rs::w56_every_interpretation_id_has_passing_rust_traces` |
| W-57 | Historical engine, validator, runner and tooling bugs fixed in Python: | resolved | `crates/gcg-engine/tests/p6k_audit.rs::w57_w70_every_historical_fix_has_a_rust_test` |
| W-58 | Most-defensible readings of ambiguous card text and major ruling overr | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w58_conflict_counts_and_policies` |
| W-59 | Pilot text routing is inferred, not encoded in the data | resolved | `crates/gcg-effects/tests/p6k_ir_scans.rs::w59_pilot_routing_table_regenerates_from_the_ir` |
| W-60 | Explanatory parentheticals are stripped but carry semantics the bindin | resolved | `crates/gcg-engine/tests/p6k_behaviour.rs::w60_gd05_089_deployed_as_a_unit_is_not_a_pilot` |
| W-61 | Grammar slips and duplicated token clauses in card text break template | resolved | `crates/gcg-engine/tests/p6k_audit.rs::pinned_cards_have_passing_rust_behaviour_tests` |
| W-62 | 'instead' wording: conditional overrides vs true substitutions | resolved | `crates/gcg-effects/tests/p6k_ir_scans.rs::w62_every_instead_text_maps_to_exactly_one_kind` |
| W-63 | The regex compiler covers about 64% of non-vanilla cards; 302 per-card | resolved | `docs/port/audit/templates.json` |
| W-64 | Card-specific custom hooks, several duplicated; custom steps cannot re | resolved | `crates/gcg-effects/tests/p6k_ir_scans.rs::w64_no_two_hooks_share_a_purpose` |
| W-65 | The keyword and timing vocabulary is duplicated across many files | resolved | `crates/gcg-effects/tests/p6k_ir_scans.rs::w65_python_vocabularies_derive_from_the_rust_enums` |
| W-66 | The compiler miscompiled some cards silently; bindings fixed the cases | resolved | `crates/gcg-effects/tests/p6k_ir_scans.rs::w66_miscompile_scan_lists_zero_unreviewed_hits` |
| W-67 | No templates for "all players each …" and related multi-player-step wo | resolved | `docs/port/audit/templates.json` |
| W-68 | Stale "compiled to printed HP" binding notes: Stat.HP already means cu | resolved | `crates/gcg-engine/tests/p6k_behaviour.rs::w68_compiled_and_bound_hp_filters_agree_on_hp_damage_states` |
| W-69 | The registry build swallows binding errors; the card silently becomes  | resolved | `crates/gcg-effects/tests/p6k_ir_scans.rs::w69_a_broken_binding_fails_the_build_naming_the_card` |
| W-70 | Historical compiler and DSL bugs fixed in Python: regression risk for  | resolved | `crates/gcg-engine/tests/p6k_audit.rs::w57_w70_every_historical_fix_has_a_rust_test` |
| W-71 | The ST02/ST05 unmodified starter-deck exceptions are not applied | resolved | `crates/gcg-data/tests/p6l_validator.rs::w71_starter_deck_exception_hook` |
| W-72 | One current B&R list: no history and no region | resolved | `crates/gcg-data/tests/p6l_validator.rs` |
| W-73 | Vanilla banned pair: the predicate and the enumerated list can diverge | resolved | `crates/gcg-data/tests/p6l_tripwires.rs::w73_vanilla_predicate_equals_the_enumerated_list_with_negative_control` |
| W-74 | Validator predicates that hold only for today's pool: "without effects | resolved | `crates/gcg-data/tests/p6l_tripwires.rs` |
| W-75 | The B&R list is enforced by default although the Comprehensive Rules n | resolved | `crates/gcg-data/tests/p6l_validator.rs::w75_no_banlist_accepts_a_banned_card` |
| W-76 | Tournament-only deck rules, limited formats, tournament procedures and | resolved | `crates/gcg-data/tests/p6l_validator.rs` |
| W-77 | Deck file format limits | resolved | `crates/gcg-data/tests/p6l_validator.rs::w77_ex_token_hints` |
| W-78 | Legality depends on simulator support (UNIMPLEMENTED) | resolved | `crates/gcg-data/tests/p6l_validator.rs::w78_simulator_support_is_reported_apart` |
| W-79 | Official errata missing from gcg-api are applied by hand (ST12-001) | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w79_errata_are_applied_with_negative_control` |
| W-80 | Card text format defects corrected by overrides (GD02-053, GD03-015, G | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w80_text_format_defects_are_exactly_the_known_cards_and_covered` |
| W-81 | gcg-api structure fields (keyword_effects, timing_markers, link_refs)  | resolved | `crates/gcg-effects/tests/p6k_ir_scans.rs::w81_blanking_keyword_effects_timing_markers_and_link_refs_changes_nothing` |
| W-82 | A null printed AP is read as 0 (inference); EX Base AP is null in the  | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w82_null_ap_printings_are_reviewed_with_negative_control` |
| W-83 | Edition Beta stat records are defective (one-row shift in the source l | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w83_edition_beta_stat_records_are_all_superseded` |
| W-84 | Link, name and trait anomalies in the card data | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w84_link_and_trait_anomalies_equal_the_known_set` |
| W-85 | Derived data files are curated by hand; curated conflict texts drifted | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w85_curated_conflicts_overrides_and_lock_agree_with_negative_control` |
| W-86 | Printing text divergences resolved to the base printing; a wrong rule  | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w86_printing_divergences_equal_the_curated_set` |
| W-87 | The Comprehensive Rules file has wrong cross-references and typos (the | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w87_every_rules_cross_reference_resolves` |
| W-88 | Provenance gaps: some official sources were read but not stored; cards | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w88_provenance_listing_covers_every_source` |
| W-89 | dataset_version's sha is the code commit that produced the data, not t | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w89_dataset_version_names_the_code_commit_not_the_data_commit` |
| W-90 | Per-card and benchmark-card statistics are correlations inside AI-vs-A | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w90_a_pure_length_effect_vanishes_within_length_strata` |
| W-91 | Results are specific to the AI and preset, and not comparable across A | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w91_runs_of_different_ais_are_not_compared` |
| W-92 | Game-level intervals and tests treat games within a BO3 match as indep | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w92_match_clustered_intervals_cover_and_plain_ones_do_not` |
| W-93 | Draws count as non-wins | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w93_every_rate_tool_uses_the_one_wins_only_interval` |
| W-94 | Hypothesis error control is per kind of comparison; thresholds are han | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w94_run_level_holm_keeps_the_false_positive_rate_at_alpha` |
| W-95 | "Never played" signals have no significance test or cap, and cost disc | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w95_never_played_copies_are_split_by_fate` |
| W-96 | Benchmark-card "seen" covers only played, Burst-revealed or public car | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w96_a_benchmark_card_only_discarded_is_observed_not_seen` |
| W-97 | The results schema file name stays v1 at version 1.1.0 | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w97_schema_files_carry_the_version_they_validate` |
| W-98 | Distribution statistics are integer nearest-rank values and rounded ra | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w98_unrounded_statistics_beside_the_reference_values` |
| W-99 | Decision logs make outputs large; reports and logs truncate | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w99_gzip_output_decompresses_to_the_same_bytes` |
| W-100 | The summary's reproduce command omits the agent factory and can name a | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w100_the_corrected_reproduce_line_of_results_1_2` |
| W-101 | conflict_resolutions lists only resolutions whose id names a deck card | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w101_a_suppression_card_lists_the_suppression_interpretation` |
| W-102 | The report directory is not updated atomically as a set | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w102_an_abort_after_any_file_keeps_the_complete_old_set` |
| W-103 | code_digest hashes every file of the installed package; the version ca | resolved | `crates/gcg-cli/tests/code_digest.rs` |
| W-104 | timing.json is wall-clock based and the only non-reproducible report | resolved | `crates/gcg-eval/tests/p6m_cli.rs::t153_benchmark_writes_gcg_sims_bytes_and_ignores_the_factory_variable` |
| W-105 | Interrupted or failed runs discard all work: no checkpoint or resume | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w105_a_resumed_run_writes_the_uninterrupted_reports` |
| W-106 | Worker termination: waitpid race, a private executor attribute, AI too | resolved | `crates/gcg-runner/tests/p6m_interrupt.rs::w106_two_hundred_interrupts_return_no_results` |
| W-107 | Spawn start-method constraints on the Python API | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w107_a_custom_factory_runs_on_ten_workers` |
| W-108 | The GCG_SIM_AGENT_FACTORY test hook can replace the AI in a benchmark | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w108_no_crate_reads_an_agent_factory_variable` |
| W-109 | Simultaneous defeat and the chooser after a drawn game rest on unconfi | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w109_a_double_defeat_decider_is_a_swiss_draw_in_the_analysis_only` |
| W-110 | Robustness fuzzing uses only random agents | resolved | `docs/port/eval/GAP-D/w110-strategic-fuzz.txt` |
| W-111 | Coverage gates count tags, not behaviour | resolved | `docs/port/PLAN.md` |
| W-112 | The long slow suite is not deselected by default | resolved | `crates/gcg-diff/tests/p6n_tooling.rs::w112_the_slow_tier_is_the_listed_one_and_the_gate_runs_both_tiers` |
| W-113 | Test helpers stop silently at step caps | resolved | the entry in WEAKNESSES.md |
| W-114 | The documented-command checker: shell blocks only, a network-dependent | resolved | `scripts/check_doc_commands.py` |
| W-115 | Research artifacts are pinned to an old snapshot and outside the gate; | resolved | `crates/gcg-diff/tests/p6n_tooling.rs::w115_w153_the_gate_summary_flags_stale_research_and_shows_the_news_scan_date` |
| W-116 | Research scripts disagree with the engine on canonical printing and re | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w116_the_canonical_fallback_never_decides_a_non_vanilla_card` |
| W-117 | The adversarial review was a sample | resolved | the entry in WEAKNESSES.md |
| W-118 | verify_all.sh gate limits | resolved | `scripts/verify_all_rust_selftest.sh` |
| W-119 | blind_transcripts.py uses a private helper and path-dependent ids, unt | resolved | `crates/gcg-eval/tests/gapc_tools.rs::w119_one_id_per_game_wherever_it_lives_and_no_search_statistics` |
| W-120 | Project Claude permissions are broader than the "only the refresh skil | resolved | `scripts/check_claude_settings.py` |
| W-121 | Refresh tooling: only the offline core is tested; official fetches nee | resolved | `crates/gcg-diff/tests/p6n_tooling.rs::w121_the_card_loader_accepts_recorded_records_and_names_a_renamed_field` |
| W-122 | Untracked bytecode of an unmerged later branch (bench/, tests/bench, t | resolved | `crates/gcg-diff/tests/lints.rs` |
| W-123 | compare.py usability limits | resolved | `crates/gcg-eval/tests/phase6.rs::w123_named_deck_sets_work_from_any_directory` |
| W-124 | Search is engine-bound; invalidating the derived view by mutation kind | resolved | `docs/port/BENCHMARKS.md` |
| W-125 | Action and Decision are frozen dataclasses (slow construction on the h | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w125_actions_compare_and_hash_by_value` |
| W-126 | Throughput measurement counts wall time x workers on one pairing; mach | resolved | `docs/port/eval/GAP-D/throughput-A.json` |
| W-127 | Investigation evidence is not reproducible: raw outputs were gitignore | resolved | `crates/gcg-eval/tests/phase6.rs::w127_evidence_check_resolves_commits_and_paths` |
| W-128 | Stale counts across status.json and the research docs | resolved | `crates/gcg-diff/tests/p6n_tooling.rs::w128_marked_counts_are_regenerated_from_the_data` |
| W-129 | Hand-maintained docs drift from the code | resolved | `crates/gcg-eval/tests/vectors.rs::mirrored_specs_match_selfplay` |
| W-130 | The shuffle-cut and chooser-procedure wording disagree with the engine | resolved | `crates/gcg-engine/tests/l5_setup.rs::new_game_shuffles_cuts_and_numbers_cards_as_python` |
| W-131 | Multiplayer (rules section 12) is out of scope | admissible exception | the entry in WEAKNESSES.md |
| W-132 | No sideboarding in BO3 | admissible exception | the entry in WEAKNESSES.md |
| W-133 | No clock: time limits, extra turns and tiebreaks are not modelled | admissible exception | the entry in WEAKNESSES.md |
| W-134 | Both players know both exact decklists | resolved (not adopted) | `crates/gcg-eval/tests/p6m_cli.rs::w134_opponent_list_reaches_the_search_flag` |
| W-135 | Information-set honesty is a convention, not enforced by the API | resolved | `crates/gcg-search/tests/observation.rs::the_strengthened_decision_is_invariant_under_hidden_permutation` |
| W-136 | SearchConfig defaults reproduce the pre-change AI; baseline presets li | resolved | `crates/gcg-ai-reference/tests/baseline_game.rs::baseline_standard_replays_the_game_recorded_before_the_changes` |
| W-137 | The determinism contract; cross-platform identity depends on the C mat | open (Linux run after the push) | `crates/gcg-core/tests/vectors.rs::libm_parity_matches` |
| W-138 | Multi-card choices are sequential single picks | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-139 | Opponent nodes are tuned against the searcher's true private cards | resolved (not adopted) | `crates/gcg-search/tests/scenarios/p6c_w139_held_trick.rs::w139_held_trick_decisions_are_recorded` |
| W-140 | In-tree priors come from the first world that reaches each node | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-141 | The no-effect branch check compares a branch with a policy line that m | resolved (not adopted) | the entry in WEAKNESSES.md |
| W-142 | The evaluation is fitted on true-state features but scores determinize | resolved (not adopted) | `docs/port/eval/P6-G/fit-round1.json` |
| W-143 | Battle-damage redirect cycles recurse without bound and crash the game | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w143_two_lunamarias_build_a_cycle_that_fails_the_game` |
| W-144 | The deck parser accepts unbounded counts: a huge count crashes with a  | resolved | `crates/gcg-data/tests/decks.rs::huge_counts_validate_without_expanding` |
| W-145 | The packaged card data predates GD06 (released 2026-10-30), so its car | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w145_new_sets_are_data_and_the_freshness_listing_names_gd06` |
| W-146 | The rules markdown is checked against the official PDF only by its set | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w146_rules_markdown_matches_the_pdf_rule_by_rule_with_negative_control` |
| W-147 | The mulligan-outcome hypothesis is confounded by the AI's own redraw r | resolved | `crates/gcg-runner/tests/p6m_analysis.rs::w147_the_redraw_gap_is_the_ai_rule_not_the_deck` |
| W-148 | Saved replays are median-length games, and the CLI cannot replay other | resolved | `crates/gcg-eval/tests/p6m_cli.rs::w148_replay_game_verifies_every_game_of_a_run` |
| W-149 | Which failure a parallel run reports depends on completion timing | resolved | `crates/gcg-runner/tests/p6m_reports.rs::w149_the_lowest_failing_match_is_reported` |
| W-150 | Determinism acceptance tests run only with the random test agent | resolved | `crates/gcg-eval/tests/phase6_slow.rs::w150_every_preset_is_identical_at_1_4_and_10_workers` |
| W-151 | Fuzz coverage counts cards in decks, not cards executed; tokens and EX | resolved | `scripts/fuzz_census.py` |
| W-152 | Seat and first-player symmetry was checked once and has no regression  | resolved | `crates/gcg-eval/tests/phase6_slow.rs::w152_seats_and_first_player_are_symmetric` |
| W-153 | News and errata discovery in the refresh skill reads only a few listin | resolved | `crates/gcg-diff/tests/p6n_tooling.rs::w115_w153_*` |
| W-154 | Untracked port artifacts would break the project's Python gates once c | resolved | `docs/port/` |
| W-155 | Change detection ignores edits inside stripped parentheticals, so a re | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::the_text_port_reproduces_every_cards_text_hash` |
| W-156 | Replay files and games.ndjson records carry no data, rules or code ver | resolved | `crates/gcg-eval/tests/p6m_cli.rs::w156_replay_check_names_version_differences` |
| W-157 | The card database silently loads with no overrides when overrides.json | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w157_packaged_overrides_load_every_card_data_resolution` |
| W-158 | A damage-prevention threshold that is not a literal integer is silentl | resolved | `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w158_no_prevention_threshold_is_computed` |
| W-159 | Turn-history conditions match their filters against a card's current s | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w159_a_unit_that_is_a_pilot_again_does_not_count` |
| W-160 | The Python-vs-Rust equivalence harness compares results.json only up t | resolved | the entry in WEAKNESSES.md |
| W-161 | ToDeck can offer duplicate ARRANGE options when a targeted Unit's Pilo | resolved | `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w161_no_todeck_can_return_a_unit_with_its_pilot` |
| W-162 | Trigger and frame fields that are serialised but never read (host_seq, | resolved | `crates/gcg-engine/tests/p6j_engine.rs::w162_a_host_that_moves_before_its_trigger_resolves` |
| W-163 | The Burst acceptance score assumes that no Burst should be declined, w | resolved | the entry in WEAKNESSES.md |
| W-164 | Token-definition fallbacks and two override-loader forms are never exe | resolved | `crates/gcg-data/tests/p6k_data_tripwires.rs::w164_dead_loader_paths_stay_dead_with_negative_controls` |

## Card coverage

1152/1152 card numbers implemented (`gcg-sim data status`); `gcg-diff coverage --check`: 840/840
non-vanilla cards with execution evidence from a passing tagged trace, 312 vanilla (255
instantiated by the Rust replay, 57 tokens); 3213/3213 per-test traces and 311/311 census games
pass. The brief's "1148" predates the current data (PLAN D3). `docs/RULES_TRACEABILITY.md` is
generated by `gcg-diff traceability --write` and checked by `--check` (passes).

## Gates

| Command | Outcome |
| --- | --- |
| `scripts/verify_all_rust.sh --full` (reviewer, clean clone of 6fbdb1e) | exit 0: fmt, both clippy runs, 1333 default, 1357 slow, 263 paranoid, 146 zstd, 190 no-default-features, 1334 tier-2 slow tests (0 failed, 0 ignored); corpus tier 1 3804/3804 in every mode, tier 2 4154/4154; fuzz 0 failures; schema checks; documented commands 20 run, 19 skipped, 0 failed |
| `scripts/verify_all.sh` (Python gate, reviewer, clean clone of 7b7ab2f) | exit 0: 3985 Python tests, ruff, format, mypy, golden, sources, conflicts, wheel |
| `scripts/ci_local.sh` (reviewer, HEAD of round 2) | exit 0, 1340 tests |
| `gcg-diff lints`, `traceability --check`, `tests-json --check`, `weaknesses --check`, `coverage --check`, `api-snapshot --check`, `corpus --tier 1 --require-all` (lead, HEAD) | all pass; corpus 3804/3804 |
| `gcg-sim benchmark ... --ai-preset strong|STANDARD|bogus` | exit 2 each |
| `cargo tree -p gcg-cli -e normal --offline \| grep -c gcg-ai-reference` | 0 |
| `git diff --quiet d9d41af -- src/gcg_sim tests` | the Python oracle is untouched |
| `gcg-eval gates --losses-documented ... --check` (final 2 and review) | E2, E3 pass; E4 fails (test decks); E1 fails only the plan's held-out-only check in final 2 |
| `.github/workflows/rust.yml` on GitHub (T-133) | **never run**: the push needs the user's approval |

tests.json: 149 passing, 8 failing (T-007, T-008, T-014, T-104, T-124, T-127, T-131, T-148),
1 not started (T-133, CI on GitHub).

## No timing code in the crates

`gcg-diff lints` and `clippy.toml` ban clocks, sleeps, `HashMap`/`HashSet` and unstable sorts in
the crates; the reviewer's grep for `std::time`, `Instant`, `SystemTime`, `sleep`, `criterion`
and `[[bench]]` finds only the lint's negative-control fixtures. The only timing activity is
`scripts/compare_python_vs_rust.sh`, outside the crates; `timing.json` exists for CLI parity
only and is excluded from every comparison (reviewer-verified, `docs/port/REVIEW.md`).

## Known limitations

- E4: on the two AI test decks the strengthened `standard` beats `greedy` about 4 points more
  often than the reference (about 305 against 289 of 400); the bar needs about 323/400. On the
  8-deck panel it clears the bar.
- 30 weaknesses remain in the shipped AI because their remedies measured null or negative
  (e.g. strategy fusion W-05, close target choices W-14, evaluation transfer W-36). The blinded
  audit (W-17) still finds about 1.3 clear mistakes per game in `standard` (2.5 for the
  reference), mostly early passivity; 12 of the 120 sample games were fully read.
- `fast` tracks `standard`'s ranking but not every variant's rate (W-18): confirm screened
  survivors with `standard`.
- Zero heap allocation on the search path is not reached (about 150 allocations per MCTS
  iteration; allocator time is about 2% of samples) (T-104, T-148).
- The one `unsafe` block is the `erfc` FFI declaration in `crates/gcg-core/src/libm.rs`, needed
  for byte equivalence with CPython's libm, not for speed; it has a safety argument and vector
  tests, and Miri cannot run foreign calls.
- Bit identity across platforms holds for the strengthened search only with `portable_math`
  (not adopted); the reference search is equivalent on the golden platform only.
- No Apple M6 was available: M6 support is build notes, runtime topology detection and tests on
  a synthetic three-tier listing; nothing was measured on an M6.

## Recommended next steps

1. Approve a push of `rust-port` so CI runs (closes T-133 and W-137's Linux job, or shows what
   fails).
2. Decide on E4: accept the panel result, or fund a strength round aimed at `greedy` on tuning
   seeds, followed by a third final evaluation and a review run on fresh seeds (days of machine
   time; budget gains have plateaued).
3. Clean up the 51 agent worktrees and branches, tagging the four docs-cited commits first.
