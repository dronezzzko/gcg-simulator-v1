# GAP-B weakness status: engine interface items

Residual items of `docs/port/eval/weakness-gaps.md` §2-§3: W-08, W-134, W-139 (the engine
identity API and the P6-C mechanisms on it), W-64 (consolidated hooks), W-101
(interpretation-tagged code paths) and W-111 (execution coverage of rules, FAQ entries and
rulings). Written by the GAP-B implementer on 2026-11-03 in worktree branch
`worktree-wf_9f38bb2b-c03-3`, from `1007ca9`. WEAKNESSES.md and tests.json are not edited here
(the lead records the status lines from this file, PLAN §12.10). The shipped presets
(`crates/gcg-search/src/presets.rs` `SHIPPED_MECHANISMS`) are unchanged; no mechanism here is
on in them or in the null configuration.

## Summary

| Item | Status | Game-level effect | Verification |
| --- | --- | --- | --- |
| W-08 | resolved by measurement (not adopted) | none: no card can lie in the other player's zones in a real game; 0 pads and 0 trims of `_fit` in 10,000 random games and the 126 vector searches | testkit scenario met (exact 64/64 worlds hold the true multisets, the reference 0/64 with 1 pad + 1 trim each, pinned); fuzz invariant over 10,000 games passes |
| W-134 | resolved by measurement (not adopted), with one deviation (the one-list e2e runs at the search level, see below) | individual -0.015 (null); high budget -0.005 (null) | information-set test passes (63 decisions identical; the `known` control changes at 56); one-list prior = true list reproduces `known` bit for bit (126 vector searches, 64 searched decisions of two full games) |
| W-139 | resolved by measurement (not adopted) | individual +0.025 (null); high budget **-0.070** [-0.139, +0.000] (harmful at the edge) | scenario: reference 0/16 (bar ≤ 8: met); strengthened `standard` 16/16 (bar ≥ 12: met) with or without the mechanism (16/16 with 0.25; 15/16 in the variant with a castable Unit), so the bar does not isolate the mechanism; under the null configuration the mechanism keeps 0/16 |
| W-64 | resolved | no AI effect (corpus 100%, behaviour-identical) | unit equivalence with the pre-W-64 bodies (64 event combinations, 9,331 histories); tier-1 corpus 3804/3804; differential fuzz 10,000 games, 0 failures, equal to Python |
| W-101 | open: partial (10 of 27 interpretations engine-recorded: 8 of the 14 rule-level ones) | no AI effect (a thread-local counter nothing reads; corpus 100%) | `p6m_analysis.rs::w101_a_rule_level_interpretation_the_engine_records_is_listed` passes |
| W-111 | open: the lead's decision (recommendation and evidence below) | no AI effect | — |

No mechanism measured clearly positive: no confirm row was warranted (no dev row reached a
paired difference of +0.03 with a lower Newcombe bound above 0).

## What was built

| File | Content |
| --- | --- |
| `crates/gcg-engine/src/observe.rs` | W-08's `_fit` pad/trim counter (`fit_adjustments`, thread-local, read-only); `HiddenPools` and `exact_pools(st, observer, owner)` (each owner's hidden cards wherever they lie, the pools by conservation, a conservation failure an `EngineError::Value`); `assign_hidden(s, observer, ids)` (P6-C interface request 1: sets hidden identities only, rewrites the history, touches); `determinize_exact_into(st, observer, seed, keep, out)` and `determinize_exact` (determinize's draws with exact pools; `keep` cards keep their identity and leave their owner's pool); `with_decklists(st, main, resource)` (request 1: a state whose game uses other decklists, sorted, test overrides kept). All additive: the reference and `determinize` are unchanged except for the counter |
| `crates/gcg-search/src/mech/p6c/mod.rs` | `PROVIDES` adds `exact_pools` and `opponent_list`; the world of index k is built from the source state (the information-set state, or with `archetype` that state with the drawn candidate's opponent decklists), with `belief_worlds` the weighed candidate, each from `determinize` or with `exact_pools` `determinize_exact_into`, then the opponent-view redraw; `density` (the posterior's under `archetype`); `note_root` keeps `view_ok` at every decision with `opponent_view_share > 0` and records the cards the root options name (`view_keep`); diagnostics `view_kept_worlds` |
| `mech/p6c/archetype.rs` | W-134: the built-in prior (the 8 packaged deck lists, equal weights), the posterior (multivariate hypergeometric likelihood of the opponent's seen cards; inconsistent lists dropped unless none is consistent; each kept list patched to hold the seen cards), the candidate draw `resample(weights, random(derive_seed(W[k], "archetype")))`, the mixture density |
| `crates/gcg-search/decks/prior-*.txt` | byte-for-byte copies of `examples/decks/*.txt` and `examples/decks/eval/*.txt` (so `code_digest` covers the prior, `crates/*/decks/*.txt`); `archetype::tests::the_packaged_prior_is_the_eight_panel_decks` checks the copies |
| `mech/p6c/view.rs` | W-139 at every decision: `named_hidden` (the cards hidden from the opponent that root options name); `redraw` keeps them (exact pools from the opponent's view) and redraws everything else; with nothing named and `exact_pools` off it is P6-C's redraw bit for bit |
| `mech/p6c/belief.rs` | `world` takes the candidate builder of `p6c::world` (behaviour-identical for the reference builder) |
| `mech/p6c/gapb_tests.rs`, `mech/p6c/tests.rs` | the GAP-B unit tests (below); `tests.rs` helpers made `pub(super)`, the `belief::world` calls pass the reference builder, and `settings_that_do_nothing_are_refused` now checks that `exact_pools` and `archetype` are accepted |
| `crates/gcg-engine/src/view/hooks.rs` | W-64: one `destroyed(st, ctx, Cause)` predicate for #8/#27/#33 and one `played_with_ex(st, ctx, ExScope)` scan for #23/#31, each card's exact semantics a parameter; the pre-W-64 bodies are kept verbatim in the test module as oracles |
| `crates/gcg-engine/src/diag.rs` | W-101: `Interpretation` (I-01, I-02, I-03, I-05, I-06, I-10, I-14, I-15, I-16, I-18), `interpretation_runs()` (thread-local counters, read-only) |
| `battle.rs`, `core.rs`, `game.rs`, `interp/deploy.rs`, `interp/cards.rs`, `interp/choose.rs` (gcg-engine) | W-101 anchors: one `note_interpretation` call at each interpretation-tagged code path (I-01 `attack_base`; I-18 a <Suppression> attack that hits a Base; I-16 a 0-AP attack on a player with no Base and no Shields; I-02 a token leaving the game; I-15 a card placed on top of the Shields; I-05 `new_game` after the shuffle and cut; I-03 the Resource and EX Resource caps; I-06 owner-ordered or shuffled placement into decks; I-14 a targeted choice from the removal area; I-10 the destroyed-cause predicate) |
| `crates/gcg-runner/src/analysis.rs` | W-101: `GameFacts::interpretations` (the anchored ids whose path ran while `observe_game` re-simulated the game); `interpretations_used` lists an interpretation by its cards or by the engine record, with `card_games`, `engine_games` (null without an engine path), `engine_recorded`, and `not_traceable_from_games` for interpretations with neither (additive fields of the opt-in `analysis.json`) |
| `crates/gcg-engine/tests/gapb_exact_pools.rs`, `gapb_interpretations.rs` | the engine tests (below) |
| `docs/port/eval/GAP-B/` | the ablation reports with every game (five rows and the placebo row), `seed-ledger.json` |

Wiring outside the owned files (flagged to the lead):

- `crates/gcg-search/src/mech/mod.rs`, `mech/reference.rs`, `mcts.rs`: a `Mechanisms::density`
  extension point (default `reference::density` = `burst_density(st, player)`, the line it
  replaces in `prepare`), with `Composed` calling P6-C under `opponent_list = archetype`.
  The root density is the only information-set read of the opponent's decklist outside the
  worlds, so the information-set test needs it. Behaviour-identical with the flag off.
- Tests that pinned the refusal of `archetype` (the request changes the expected behaviour;
  each stays as strict): `crates/gcg-search/src/presets.rs` test
  `unavailable_mechanisms_are_refused` (its test module only; the presets are untouched) now
  checks that `archetype` is accepted; `crates/gcg-eval/src/agent_spec.rs` test
  `search_overrides_reach_the_mechanism_flags` checks that the spec parses to
  `OpponentList::Archetype`; `crates/gcg-eval/tests/p6m_cli.rs`
  `w134_opponent_list_reaches_the_search_flag` runs `gcg-eval benchmark --opponent-list
  archetype` to completion and checks the factory text and the listed mechanism. With W-08 and
  W-134 provided, every `MechanismFlags` field is now available, so no real flag reaches the
  "not available in this build" error any more (the error path stays).
- `crates/gcg-runner/tests/p6m_analysis.rs`: the existing W-101 test now checks that I-01 is
  engine-recorded and I-04 untraceable (it checked I-01 untraceable), and the new
  `w101_a_rule_level_interpretation_the_engine_records_is_listed`.

## W-08 — Determinization pads or trims pools when effects move cards between players' decks

- **Status:** resolved by measurement (not adopted). The engine keeps `_fit` bit-exact and
  counts every pad and trim (`observe::fit_adjustments`); `exact_pools` determinises every world
  with conservation-exact pools, never padded, a conservation failure an engine error.
- **Finding:** the engine places every card in its *owner's* zone list (`core::move_single`:
  `st.zones.insert(owner, ...)`/`push(owner, ...)`), so no rule puts a card into the other
  player's deck, and `_fit` pads or trims only in hand-built states. Measured: 0 pads and 0
  trims over 10,000 seeded random games (every 16th decision, both observers: the fuzz test)
  and over the 126 searched vector decisions; every exact search equals the reference's on all
  126 (`w08_exact_searches_are_the_reference_searches_on_the_vectors`).
- **Evidence** (all pass):
  - `crates/gcg-engine/tests/gapb_exact_pools.rs::w08_a_card_in_the_other_players_deck_is_padded_by_the_reference_and_exact_here`
    (the testkit scenario: a card of player 0 moved into player 1's deck by a canonical edit;
    the reference pads and trims once per determinisation, pinned (1, 1), and holds the wrong
    multisets; the exact world holds each owner's true hidden multiset and keeps the
    observer's information set).
  - `gapb_exact_pools.rs::w08_fuzz_exact_worlds_keep_every_owners_hidden_card_counts`: 10,000
    seeded random games, 71,507 sampled states over 909,011 decisions (143,014 exact worlds), both observers: every exact world's hidden
    card counts equal the true state's per owner; `_fit` pads 0, trims 0.
  - `gapb_exact_pools.rs::exact_worlds_are_the_reference_worlds_where_nothing_crosses` (24
    random games, every 7th decision, 2 observers x 2 seeds: the exact world's digest, RNG,
    version and history epoch are `determinize`'s), `assign_hidden_sets_only_hidden_identities_and_rewrites_the_history`,
    `kept_cards_keep_their_identities_and_the_rest_stays_exact`,
    `with_decklists_moves_the_pools_and_the_same_lists_change_nothing` (and the conservation
    error).
  - `mech::p6c::gapb_tests::w08_exact_pool_worlds_hold_the_true_multisets_where_the_reference_pads`
    (the search's worlds in the scenario: exact 64/64, reference 0/64, 64 pads and 64 trims).
- **Ablation** (iterations / applies per game are challenger vs opponent):
  - Individual (dev 22800001): 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.061, +0.091],
    discordant 32/29, 9771 vs 9657 / 118823 vs 117258
    (`docs/port/eval/GAP-B/individual-exact_pools-true.json`). Game by game against the placebo row of the same master (the same challenger with `exact_pools=false`, `individual-placebo-exact_pools-false.json`: 203/400, the same discordant counts, iterations and applies): **0 of 400 games changed** (`rows_table.py versus`), so `exact_pools` has no game-level effect, as the probe predicted. Against the same placebo, archetype changed 75 games (11 / 17 wins, -0.015 [-0.041, +0.011]) and `opponent_view_share=0.25` 382 (53 / 51, +0.005 [-0.045, +0.055]).
  - Leave-one-out: not applicable (not in the shipped bundle).

## W-134 — Both players know both exact decklists

- **Status:** resolved by measurement (not adopted). `opponent_list = archetype` draws the
  opponent's unseen cards from a posterior over the packaged deck lists, never from its true
  decklist; it is accepted by `validate` and reachable through `gcg-eval benchmark
  --opponent-list archetype` and `search:<preset>:opponent_list=archetype` (A30). `gcg-sim` and
  the defaults are unchanged.
- **Deviation:** the plan's e2e is "a benchmark in archetype mode with a one-list prior equal
  to the true list reproduces known mode exactly". The prior can only be chosen inside the
  search: carrying a prior from a `gcg-eval` option to the agent needs a field in
  `StrengthenedConfig` or `SearchAgent` (lead-owned `contracts.rs`; interface request below).
  The e2e therefore runs at the search level over full games (every searched decision of two
  full games, the archetype search with a one-list prior of the true list against the `known`
  search, bit for bit) and on all 126 searched vector decisions, and the CLI runs with the
  built-in prior.
- **Not covered:** pre-game decisions (`Mechanisms::pregame`): REDRAW reads only the
  searcher's own decklist; CHOOSE_FIRST simulations still determinise from the decklists of the
  state. The forced play/draw protocol of every row (A24) never asks for CHOOSE_FIRST.
- **Evidence** (all pass):
  - Information-set test `mech::p6c::gapb_tests::w134_archetype_searches_read_no_true_decklist`:
    at 63 searched vector decisions, a second true state whose opponent decklist is another
    packaged list patched to hold the seen cards (and whose opponent hidden cards are drawn
    from it) gives the identical archetype search (alternatives' values and visits, action,
    iterations, posterior); the `known` search changes at 56 of them (the negative control).
  - `w134_a_one_list_prior_of_the_true_list_reproduces_known_mode_on_the_vectors` (126
    decisions) and `w134_a_one_list_prior_reproduces_known_mode_over_full_games` (64 searched
    decisions of two full games, red-green-zeon vs blue-white-federation and blue-green-ramp
    vs purple-blue-swarm).
  - `w134_the_posterior_of_one_true_list_is_the_reference` (weight 1.0, the reference density
    bit for bit), `w134_the_packaged_posterior_follows_the_seen_cards` (narrowed at 119 of 126
    decisions; the true list a candidate at all 126), `archetype::tests::*` (likelihoods,
    patches, the packaged prior), `every_gapb_flag_runs_on_the_vector_decisions_and_reads_the_information_set_only`
    (honesty under a changed hidden card and permuted hidden cards).
  - `crates/gcg-eval/tests/p6m_cli.rs::w134_opponent_list_reaches_the_search_flag` (the
    benchmark runs in archetype mode).
- **Ablation:**
  - Individual (dev 22800001): 197/400 (0.492, [0.444, 0.541]), -0.015 [-0.084, +0.054],
    discordant 24/27, 9812 vs 9605 / 119463 vs 115978
    (`docs/port/eval/GAP-B/individual-opponent_list-archetype.json`).
  - High budget (dev 22800002), `search:standard` at 2x its budget with the mode against the
    same with `known`: 199/400 (0.497, [0.449, 0.546]), -0.005 [-0.073, +0.063], discordant 24/25, 32034 vs 32122 / 581838 vs 580772 (`docs/port/eval/GAP-B/high-budget-opponent_list-archetype.json`).
  - The prior holds the 8 panel decks, so on the tuning pairs the true list is a candidate:
    this measures the cost of not knowing which panel list the opponent plays.

## W-139 — Opponent nodes are tuned against the searcher's true private cards

- **Status:** resolved by measurement (not adopted). `opponent_view_share` now builds
  opponent-view worlds at every decision (P6-C: 11 of 32 vector decisions; now 32 of 32): the
  cards the root options name keep their identities and everything else hidden from the
  opponent is redrawn from the opponent's view with exact pools, so every root move plays the
  card it names. At decisions whose options name no hidden card the redraw is P6-C's, bit for
  bit. Residue: in those worlds the opponent's nodes still see the named cards (the cards the
  searcher could play now).
- **Scenario** (T-123): the binding position of `tests/scenarios/p6c_w139_held_trick.rs`
  (re-run unchanged: reference 0, share 0.25 0, share 0.5 0, as P6-C recorded; no root option
  names a hidden card there) and, new, the same position with a castable GD01-060 in hand
  (one named hidden card), 16 agent seeds each, a keep = the trick in hand with an open
  Resource after the move:
  - without the Unit: null 0, share 0.25 0, 0.5 0; shipped `standard` 16, `standard` + 0.25 16;
  - with the Unit: null 0, share 0.25 0, 0.5 0; shipped `standard` 16, `standard` + 0.25 15.
  The reference keeps it in at most 8 (0: met) and strengthened `standard` in at least 12 (16:
  met), but the shipped bundle meets the bar without the mechanism, and the mechanism alone does
  not move the null configuration (`mech::p6c::gapb_tests::w139_held_trick_decisions_at_every_decision_are_recorded`).
- **Evidence** (all pass): `w139_opponent_view_worlds_keep_the_named_cards_at_every_decision`
  (50 opponent-view worlds of 200 keep the playable card's identity, the unnamed trick is
  redrawn (kept in 1 of 50), the opponent's information set and every owner's hidden multiset
  from the opponent's view are the usual world's); P6-C's
  `w139_opponent_view_worlds_hide_the_held_trick` (unchanged result: usual 300/300, opponent-view
  2/100), `opponent_view_worlds_need_root_options_the_opponent_can_identify`,
  `an_opponent_view_iteration_trains_only_the_opponent`; the honesty runs (28 of 32 vector
  decisions changed, 672 kept-card worlds; identical results from other true states of the
  same information set).
- **Ablation:**
  - Individual (dev 22800001), `opponent_view_share=0.25`: 205/400 (0.512, [0.464, 0.561]),
    +0.025 [-0.043, +0.093], discordant 27/22, 9983 vs 9957 / 122091 vs 121324
    (`docs/port/eval/GAP-B/individual-opponent_view_share-0_25.json`).
  - High budget (dev 22800002), `search:standard` at 2x its budget (the shipped bundle) with
    0.25 against the same without it: 186/400 (0.465, [0.417, 0.514]), -0.070 [-0.139, +0.000],
    discordant 19/33, 32527 vs 32491 / 605086 vs 599816
    (`docs/port/eval/GAP-B/high-budget-opponent_view_share-0_25.json`).
  - Leave-one-out: not applicable (not in the shipped bundle). Not recommended: the high-budget
    row is negative at the edge of significance.

## W-64 — Card-specific custom hooks, several duplicated

- **Status:** resolved. The engine's duplicated families are one implementation each, with each
  card's exact semantics as a parameter: `played_with_ex(st, ctx, ExScope)` (#23 GD04
  `AfterNewestPlay`, #31 GD05 `NewestOfTheCard`) and `destroyed(st, ctx, Cause)` (#8
  `WithDamage`, #27 `ByEffect { by_you }`, #33 `ByEffectDamage`). The look-own-top pair and the
  enemy-level family were already one body each (P6-K). "Custom steps cannot request choices"
  stays as in Python (the stated decision).
- **Evidence** (all pass): `view::hooks::tests::w64_the_destroyed_cause_predicate_is_the_three_hooks`
  (64 combinations of `battle`, `by`, the controller and the frames, against the pre-W-64
  bodies kept verbatim), `w64_the_played_with_ex_scan_is_the_two_hooks` (all 9,331 histories
  of up to 5 records over 2 uids and 3 kinds), the existing hook tests (the #33 frame test now
  checks the new predicate against the old body on every call), the per-test traces of the
  GD04/GD05/GD01/ST12 cards in the tier-1 `tests` set (3213/3213), `gcg-diff corpus --tier 1
  --require-all` 3804/3804, the differential fuzz `gcg-diff fuzz --games 10000 --seed 2026 --workers 4 --compare port/corpus/v1/tier1/fuzz/summary-2026-10000.json` (10,000 games, 856,034 actions, 0 failures, equal to Python's summary), and
  `crates/gcg-effects/tests/p6k_ir_scans.rs::w64_no_two_hooks_share_a_purpose` (unchanged).
  Tier 2 was not run: `build/oracle/tier2/` is not present in this worktree (it is regenerated
  by Python).
- **Ablation:** no AI effect (A13): behaviour-identical by construction, with no off switch;
  the `search` (9/9) and `baselines` (8/8) sets and the whole tier-1 corpus at 100%, the
  seed-1 equivalence smoke (3/3 identical), and the gcg-search scenario tests pass.

## W-101 — interpretation-tagged code paths

- **Status:** open: partial. The engine records when an interpretation-tagged code path runs
  (`gcg_engine::diag::interpretation_runs`, read-only, thread-local, in no compared output),
  and `analysis.json` `interpretations_used` lists an interpretation by the engine record as
  well as by its cards. Anchored: 10 of 27 (I-01, I-02, I-03, I-05, I-06, I-10, I-14, I-15,
  I-16, I-18), of which 8 of the 14 rule-level ones (I-01, I-03, I-05, I-06, I-10, I-14, I-15,
  I-16). Remaining: the rule-level I-04 (lasting "all your Units"), I-11 (modal Commands), I-13
  (deploy as an APx･HPy Unit), I-17 (【When Linked】 on pairing), I-21 ("If"/"Then"), I-24
  (Pilot text routing, a compile-time reading with no engine path), and the card-level ones,
  which keep the card trace (I-07, I-08, I-09, I-12, I-20, I-22, I-23, I-27; I-19, I-25 and
  I-26 are compile-time or data readings with no engine path).
- **Option location (A30):** `gcg-eval benchmark --analysis` (`analysis.json`).
- **Evidence** (all pass): `crates/gcg-runner/tests/p6m_analysis.rs::w101_a_rule_level_interpretation_the_engine_records_is_listed`
  (4 random BO1 games re-simulated: I-05 and I-15, rule-level without cards, listed with
  `engine_games` = 4 and `card_games` = 0; the card-level trace unchanged),
  `w101_a_suppression_card_lists_the_suppression_interpretation` (I-18 by its card;
  `conflict_resolutions` untouched), `crates/gcg-engine/tests/gapb_interpretations.rs`
  (300 random games, each played twice with identical records and states: I-01 300/300 games,
  I-02 300/300, I-05 300/300 (once each), I-15 300/300 (12 runs each), I-16 2/300, I-18 8/300;
  I-03, I-06, I-10 and I-14 need cards the example decks lack, I-03 is shown by a full resource
  area), `diag::tests::interpretation_counters_belong_to_their_thread_and_count_by_id`.
- **Ablation:** no AI effect (A13): a counter that nothing in the engine, the AI or any
  compared output reads; corpus at 100% (3804/3804 including `search` 9/9 and `baselines`
  8/8), the seed-1 equivalence smoke (3/3 identical).

## W-111 — Coverage gates count tags, not behaviour

- **Status:** open: the lead's decision. Execution evidence is in place for cards (P6-N). For
  rules, FAQ entries and rulings the gap entry allows engine probes or the lead's decision
  that tag-based coverage stands; the decision is the lead's, so this records the evidence
  and a recommendation and builds the probe mechanism the first option needs.
- **Evidence for the decision:**
  - Scale: 395 testable rules (579 in `rules_index.json`, 184 N/A), each needing a code-path id
    in the engine procedure that implements it; the rule procedures are spread over
    `core.rs`, `game.rs`, `battle.rs`, `interp/*` and `view/*` (about 14,600 lines of
    `gcg-engine/src`). Rulings and FAQ entries are mostly card-specific and already ride on
    the card execution evidence of their tagged cards.
  - What tags already guarantee in the port: every tagged Python test is replayed by the Rust
    engine decision by decision against Python's trace (`gcg-diff coverage --check`: 3213/3213
    per-test traces and 311/311 census games pass), so a passing tagged test reproduces
    Python's behaviour exactly; a test that tags a rule it never exercises is a Python
    test-quality gap, not a port gap.
  - The probe mechanism exists now: `gcg_engine::diag::Interpretation` and
    `interpretation_runs` (W-101) are per-code-path execution records; extending them rule by
    rule and reading them in `crates/gcg-diff/src/coverage.rs` while the per-test traces
    replay is the probe route (L effort; `coverage.rs` is outside this package).
- **Recommendation:** record that tag-based coverage stands for rules, FAQ entries and rulings,
  with execution evidence for cards, because the per-test trace replay already pins each
  tagged test's behaviour to Python's; keep the interpretation probes as the mechanism for any
  rule that needs execution evidence later.

## Rows and seeds

Every row is `gcg-eval ablation` with 400 BO1 games in mirrored blocks of four over the 15
tuning pairs of the panel, forced play/draw (`--choose-first seat0`, A24), `--workers 4`:

- individual rows: `ablation individual --mechanism <flag> --reference standard --seed 22800001
  --role dev` (`search:null-standard:<flag>` against `ref:standard`), and the placebo
  `--mechanism exact_pools=false` on the same master;
- high-budget rows: `ablation leave-one-out --bundle
  search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,<flag on>
  --mechanism <flag off> --seed 22800002 --role dev` (the shipped bundle at twice its budget).

Masters (dev role only; no tuning, confirm, eval or review master): 22800001 (four rows) and
22800002 (two rows), all in `docs/port/eval/GAP-B/seed-ledger.json`. The tables are
`uv run python docs/port/eval/M15/rows_table.py rows|versus ...`.

## Interface change requests (lead)

1. **`contracts.rs` (gcg-search): a prior for `opponent_list = archetype`.** A field
   `opponent_prior: Option<Arc<Prior>>` in `StrengthenedConfig` (or a `SearchAgent`
   constructor) and a `gcg-eval benchmark --opponent-prior FILE` option (A30) would make the
   CLI one-list e2e of W-134 possible; today the prior is the built-in packaged lists.
2. **`contracts.rs` (gcg-search) `MechanismEffort`:** counters for `view_kept_worlds` (W-139)
   and the archetype candidates drawn (W-134), which `p6c::Search` keeps per decision.
3. **`crates/gcg-diff/src/coverage.rs`:** read `interpretation_runs` while the per-test traces
   replay, if the lead chooses the probe route for W-111.

## Gate

Every step ran after the last source edit, from this worktree, in this order (logs kept by the
implementer; at most 4 game threads at a time):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass (clippy.toml bans included) |
| `cargo build --release --locked --offline --workspace` | pass |
| `cargo test --workspace --release` | pass: 134 test targets and doc-test sets, 1260 tests, 0 failed |
| `cargo test --release -p gcg-search --features slow` | pass: 285 tests, 0 failed |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 (`search` 9/9, `baselines` 8/8, `worlds` 100/100, `tests` 3213/3213) |
| `target/release/gcg-diff fuzz --games 10000 --seed 2026 --workers 4 --compare port/corpus/v1/tier1/fuzz/summary-2026-10000.json` | 10,000 games, 856,034 actions, 0 failures, fuzz passed |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 identical (fast, standard, strong) |
| `uv run ruff check .`, `uv run mypy` | pass (no Python file changed) |
| `uv run ruff format --check .` | 1 file would be reformatted: `port/oracle/export_cli.py`, unchanged here (last changed in `d750b6c`), so the failure predates this package |
| `target/release/gcg-diff coverage --check` | pass (W-111 counts above) |

The ablation rows ran a `gcg-eval` built from this tree before the W-64/W-101 edits and two
behaviour-neutral edits (`sort_unstable` to `sort` on integer keys, `max_by` to `first_max_by`
with the same tie rule); every search path of the rows is the committed one. Check: the archetype
individual row re-run with the final release binary (same master, scratch ledger) changes 0 of
400 games against the recorded row (`rows_table.py versus`).

## tests.json

- T-123 (W-139 scenario): the scenario test passes as a record, and the new record
  `w139_held_trick_decisions_at_every_decision_are_recorded` asserts the entry's bars (reference
  at most 8, strengthened `standard` at least 12) and passes.
- No other register id names these items; the gate evidence also bears on T-018 (clippy),
  T-051 and T-053 (corpus sets) and T-058 (engine property tests, extended by the exact-pools
  tests).
