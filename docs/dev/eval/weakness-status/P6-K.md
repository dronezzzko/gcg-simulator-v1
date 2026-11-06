# P6-K status: rules, effects and data preservation (class N)

Package P6-K of docs/dev/PLAN.md §12.2: W-55 to W-70, W-79 to W-89, W-116, W-145, W-146,
W-155, W-157, W-164 (33 items). Written by the P6-K implementer for the lead, who records the
outcomes in docs/dev/WEAKNESSES.md (§12.10). This file does not edit WEAKNESSES.md.

## What P6-K changed

Only new test files, port tooling and audit tables. No file under `crates/*/src`,
`src/gcg_sim`, `tests/`, the cached sources or the packaged data changed, so rules, card
behaviour, data and every reference-equivalent output are unchanged.

| File | Items |
| --- | --- |
| `crates/gcg-data/tests/p6k_data_tripwires.rs` (22 tests) | W-55, W-58, W-60, W-79, W-80, W-82, W-83, W-84, W-85, W-86, W-87, W-88, W-89, W-116, W-145, W-146, W-155, W-157, W-164 |
| `crates/gcg-effects/tests/p6k_ir_scans.rs` (11 tests) | W-55, W-59, W-62, W-64, W-65, W-66, W-69, W-80, W-81 |
| `crates/gcg-engine/tests/p6k_behaviour.rs` (6 tests, testkit) | W-60, W-66, W-68 |
| `crates/gcg-engine/tests/p6k_audit.rs` (4 tests) | W-56, W-57, W-58, W-60, W-61, W-62, W-66, W-68, W-70 |
| `oracle/p6k_vectors.py` (`--write`/`--check`) and `oracle/tests/vectors/p6k_*.json` | W-60/W-155 parenthetical vector, W-65 vocabulary vector, W-68 compiled-script fragments |
| `oracle/p6k_audit.py` (`--write`/`--check`) | W-56/W-58, W-57/W-70, W-59 tables |
| `oracle/templates/__init__.py`, `oracle/templates/check.py` (`--write`/`--check`) | W-63, W-67 |
| `oracle/tests/test_p6k_*.py` (16 tests) | W-60, W-63/W-67, W-66, W-81 |
| `docs/dev/audit/{interpretations,regressions}.{json,md}`, `pilot-routing.md`, `templates.json`, `data-provenance.md` | W-56/W-58, W-57/W-70, W-59, W-63/W-67, W-88/W-89 |

Each tripwire is a pure function over the packaged sources or a modified copy held in memory
(the "temporary data copy" of the entries), with a negative control in the same test.

## Gate (run on this branch)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release` | see the structured report (all pass) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 |
| `target/release/gcg-diff lints` | lints passed |
| `uv run pytest oracle/tests -q` | 16 passed |
| `uv run python oracle/p6k_vectors.py --check`, `p6k_audit.py --check`, `templates/check.py --check` | exit 0 |
| `uv run ruff check . && uv run ruff format --check .`; `MYPYPATH=src:oracle uv run mypy --strict oracle/templates oracle/tests oracle/p6k_*.py` | clean |

## Ablation rows (A13)

Every P6-K item is class N and changes no code the AI executes: P6-K adds tests, port-side
tooling and documents only (`git diff --stat 3fbb8aa..HEAD -- crates/*/src src tests` is empty).
So each item's ablation row reads: **"No AI effect: P6-K changes no engine, runner, effect or AI
source file (no `crates/*/src` change); the AI's decisions are Python's as the `search` and
`baselines` corpus sets and the whole tier-1 corpus at 3804/3804 show. Behaviour evidence: the
tests named in the item's row."** The row text is repeated per item below as "Ablation: no AI
effect (P6-K common row)".

## Items

### W-55 — resolved
- **Resolution:** tripwires for every pool-dependent decision. Bases: every Base and EX Base
  has AP 0. Rule 4-3-3: no IR step selects from the resource deck, and no `PlaceResource` runs
  in a loop or twice in one `Simultaneous`; no card text names the resource deck or the
  outside of the game. Token supply (5-17-4-1, 5-17-4-3, 5-17-4-4, 6-1-2-1): the DSL has no
  location outside the game (an exhaustive match over `Loc` plus the fixed value list). Every
  N/A reason of `rules_na.json` that depends on the pool is listed with its tripwire.
- **Evidence:** `p6k_data_tripwires.rs::w55_bases_have_zero_ap_with_negative_control` (a
  synthetic Base with AP 1 trips it); `p6k_ir_scans.rs::w55_resource_deck_moves_and_token_supply_with_negative_control`
  (synthetic "place 2 Resources" `Repeat` and a resource-deck `Choose` trip it);
  `p6k_ir_scans.rs::w55_every_pool_dependent_na_reason_has_a_tripwire` (4-3-3, 5-17-4-1,
  5-17-4-3, 5-17-4-4, 6-1-2-1). All pass.
- **Ablation:** no AI effect (P6-K common row).

### W-56 — resolved (one deviation, stated)
- **Resolution:** 27 interpretation ids (`docs/dev/audit/interpretations.{json,md}`, I-01 to
  I-27: the ASSUMPTIONS/CONFLICTS decisions of the entry plus W-58's major conflicts), each with
  its anchor, the tags that pin it, its Python tests and the per-test traces that replay them
  through the Rust engine. The re-review list: every gcg-api ruling on an interpretation's
  cards is in its snapshot; a new one is printed and fails the test, so a data refresh lists it
  for re-review.
- **Evidence:** `p6k_audit.rs::w56_every_interpretation_id_has_passing_rust_traces` (every id
  has passing traces among its own Python tests; 2 to 149 traces per id),
  `p6k_audit.rs::w56_new_rulings_on_interpretation_cards_are_listed_for_re_review`;
  `uv run python oracle/p6k_audit.py --check` exit 0. Differential evidence: the corpus
  replays 518 coverage games built to exercise every card (tier 1 50, tier 2 468) and 600 fuzz
  games (tier 1 100, tier 2 500) with a state digest at every decision (tier 1 3804/3804 on this
  branch; tier 2 4154/4154 at the Phase 4 gate), and T-055 (10,000 fuzz games over decks that
  rotate through the whole pool, summary byte-equal to Python). **Deviation:** a fresh
  10,000-game run comparing per-game final digests, as the entry words it, was not run; the
  per-decision digests of the corpus games are a stronger check per game, on fewer games.
- **Ablation:** no AI effect (P6-K common row).

### W-57 — resolved
- **Resolution:** `docs/dev/audit/regressions.{json,md}`: the 90 historical strict-xfail tests
  (89 marks; `_DEVELOPMENT_XFAIL` marked two tests) from an AST scan of 102e72b (27), 7c47b86
  (22), ddb9f4f (3), 0e1d3e8 (16), 0ee591d (22) and their parents, and the 34 fixed bugs of the
  entry, each mapped to its Python tests, their per-test traces and named native Rust tests.
- **Evidence:** `p6k_audit.rs::w57_w70_every_historical_fix_has_a_rust_test` (fails if any xfail
  or bug lacks passing Rust evidence; the native tests named must exist). One bug row has no
  Rust counterpart by design and says so: `tuning --rounds 0` (the weight-tuning tool is not
  ported; P6-I's `gcg-eval tune` must carry the test).
- **Ablation:** no AI effect (P6-K common row).

### W-58 — resolved
- **Resolution:** a data-loading test of the conflicts report and an end-to-end check of the
  report entry that lists a deck's conflict ids.
- **Evidence:** `p6k_data_tripwires.rs::w58_conflict_counts_and_policies` (402 conflicts, none
  unresolved, invalid, stale or orphaned; policy counts 110 most_defensible_reading, 192
  ruling_overrides_text, 45 canonical_printing, 16 faq_clarifies_rules, 11
  rules_xref_intended_target, 6 banlist_most_defensible and the 7 smaller policies; one
  resolution per conflict); `w58_a_deck_with_st13_013_lists_its_conflict_ids` (`gcg_data::
  conflict_resolutions`, the function `results.json` uses, lists
  `ambiguous:ST13-013:deploy-1-to-2-tokens`); the major conflicts are interpretation ids I-10,
  I-21, I-25, I-26, I-27 on the W-56 re-review list.
- **Ablation:** no AI effect (P6-K common row).

### W-59 — resolved
- **Resolution:** `docs/dev/audit/pilot-routing.md`: for each of the 127 Pilot cards, the
  abilities the Pilot card keeps and the abilities the paired Unit gains, generated from the
  Python registry by `oracle/p6k_audit.py` and regenerated from the Rust IR by the test.
- **Evidence:** `p6k_ir_scans.rs::w59_pilot_routing_table_regenerates_from_the_ir` (the IR table
  equals the committed Python table line for line, so the Rust and Python splits agree; every
  kept ability is a 【Burst】, alias, play modifier or hand/trash ability);
  `p6k_audit.py --check` exit 0.
- **Ablation:** no AI effect (P6-K common row).

### W-60 — resolved
- **Resolution:** a native behaviour test per card in Rust and in Python, and the stripped
  parenthetical list (W-155's gate).
- **Evidence:** Rust `p6k_behaviour.rs::w60_gd05_089_deployed_as_a_unit_is_not_a_pilot`,
  `w60_st12_013_you_choose_first_then_your_opponent`, `w60_st14_014_does_not_count_itself`;
  Python `oracle/tests/test_p6k_parentheticals.py` (same three scenarios), all pass;
  `p6k_data_tripwires.rs::w155_parenthetical_gate_passes_on_the_packaged_data` (the
  non-reminder stripped parentheticals are exactly on GD05-089, ST12-013, ST14-014).
- **Ablation:** no AI effect (P6-K common row).

### W-61 — resolved
- **Resolution:** the eight cards keep their per-card behaviour (no compiler port, so no
  tolerance list).
- **Evidence:** `p6k_audit.rs::pinned_cards_have_passing_rust_behaviour_tests` (ST09-004,
  GD02-086, GD05-101, GD04-100, T-014, T-021, T-022, T-029 each have passing per-test traces of
  their tagged Python tests); the golden IR equals Python's
  (`crates/gcg-effects/src/tests/golden.rs::all_golden_files_regenerate_byte_for_byte`).
- **Ablation:** no AI effect (P6-K common row).

### W-62 — resolved
- **Resolution:** a reviewed table of the 18 "instead" cards: 12 conditional overrides and 6
  rule 10-1-9 substitutions. In the IR an override is an `If` branch or a conditional value;
  a substitution is a substitution rule modification (`rest_substitute`, GD03-079, GD05-124;
  `redirect_battle_damage`, GD04-087, GD04-095), which only the engine's substitution
  procedures read, or a replaced destination (GD05-089, ST12-012). No override uses a
  substitution node.
- **Evidence:** `p6k_ir_scans.rs::w62_every_instead_text_maps_to_exactly_one_kind` (the pool's
  "instead" cards equal the table; each card's IR has its kind's signature);
  `p6k_audit.rs::pinned_cards_have_passing_rust_behaviour_tests` (all 18 have passing traces).
- **Ablation:** no AI effect (P6-K common row).

### W-63 — open (in-scope part partly done)
- **Resolution so far:** `oracle/templates/`: a template mechanism (compile the line with
  the unchanged compiler, append the steps of a recognised trailing wording) and the template
  `reduce-received-damage-this-turn` (the "conditional damage reduction" family). The checker
  runs every template on all 302 binding cards and on every compiled card.
- **Evidence:** `uv run python oracle/templates/check.py --check` exit 0;
  `docs/dev/audit/templates.json`: 6 bindings reproduced byte for byte in total
  (GD05-094, GD05-125 by this template; GD05-112, GD05-113, GD05-121, GD05-122 by W-67's), no
  mismatch, no compiled card changed, GD05-018 covered in part (its trigger wording has no
  template); `oracle/tests/test_p6k_templates.py` passes.
- **Not done:** templates for the other compile-error families (selector suffixes, delayed
  "during this turn, when …", inline token definitions, the EX-Resource trigger, "are deployed
  rested", "it" without antecedent, "exile this card in your trash", "It can't activate
  <Blocker>"); the IR's `cards[].template` field from `export_ir.py` and
  `gcg-eval data-status --templates` (both outside P6-K's files; interface notes).
  Post-acceptance (§12.11): move the templates into `effects/compiler/` and retire the
  reproduced bindings with an empty golden diff.
- **Ablation:** no AI effect (P6-K common row).

### W-64 — open (partial)
- **Resolution so far:** a checked purpose table of all 34 custom hooks
  (`p6k_ir_scans.rs::HOOK_PURPOSES`): every hook is classified once; within a family no two
  hooks have the same distinction except the declared identical alias
  (`wp_eb01_a_look_own_top`/`wp_eb01_b_look_top_own`, one engine body); every name-normalised
  collision is that alias or a family with a stated distinction (`played_with_ex`: most recent
  play from the hand vs most recent play this turn).
- **Evidence:** `p6k_ir_scans.rs::w64_no_two_hooks_share_a_purpose` passes. The engine already
  consolidates the look-own-top pair and the enemy-level family (`enemy_unit_level` with
  parameters).
- **Not done:** one parametrised implementation each for the `played_with_ex` pair and the
  destroyed-cause conditions (`gcg-engine` source; interface note); the entry's "custom steps
  cannot request choices" stays as in Python.
- **Ablation:** no AI effect (P6-K common row).

### W-65 — resolved
- **Resolution:** the Rust keyword and timing enums are the single source; the test derives
  each Python vocabulary from them with exhaustive matches.
- **Evidence:** `p6k_ir_scans.rs::w65_python_vocabularies_derive_from_the_rust_enums`
  (`dsl.Kw`, `abilities._KW`, `nouns.KW_RE`, `grammar._KWS`, `KEYWORD_VOCABULARY` equal
  `Kw::ALL`; `STACKING_KEYWORDS` equals the keyword programs; `dsl.Timing` equals `Timing::ALL`;
  `TIMING_VOCABULARY` equals the marker table derived by exhaustive matches over `Timing` and
  `Gate`, so a new variant does not compile until handled), against
  `oracle/tests/vectors/p6k_vocabulary.json` (`p6k_vectors.py --check` exit 0).
- **Ablation:** no AI effect (P6-K common row).

### W-66 — resolved (no real miscompile found)
- **Resolution:** a static scan of the 538 compiled cards' IR for eight families from the
  binding notes: optional choice without a minimum, Pilot read through the destroyed Unit,
  unguarded attack-target change, event payload outside a triggered frame, two-target step
  without an all-targets guard, chosen target never used, ■ modes not all compiled, "each
  enemy player" compiled as either side. Five hits, all reviewed: GD01-103, GD02-055,
  GD03-116, GD04-114, GD05-117 (two-target steps). They are not miscompiles: the engine's
  multi-target pre-check (rules 10-1-8-1-1, 10-2-2, 10-3-3-1, in force since 102e72b) makes
  the Commands unplayable and the trigger do nothing without both targets.
- **Evidence:** `p6k_ir_scans.rs::w66_miscompile_scan_lists_zero_unreviewed_hits`,
  `w66_scan_finds_the_binding_fixed_miscompiles_in_their_compiled_form` (negative control: the
  binding-fixed shapes in compiled form are found); Rust
  `p6k_behaviour.rs::w66_two_target_commands_need_both_targets`,
  `w66_gd02_055_deploy_needs_both_targets`; Python
  `oracle/tests/test_p6k_miscompile.py` (5 tests). All pass.
- **Ablation:** no AI effect (P6-K common row).

### W-67 — open (in-scope part partly done)
- **Resolution so far:** the general template `pair-from-trash-after-main` for "After activating
  this card's 【Main】, you may pair this card from your trash with one of your (Trait) Units",
  reproducing GD05-112, GD05-113, GD05-121 and GD05-122 byte for byte (same checker and report
  as W-63).
- **Not done:** the "each player in turn order" step and the single-player wordings
  ("All players each choose 1 of their Resources", "They may draw 1", "reveal 1 … return it to
  the top", "all enemy players each choose", "All players each look at the top card", "place
  the top card of your deck into your trash"); post-acceptance steps as W-63.
- **Ablation:** no AI effect (P6-K common row).

### W-68 — resolved
- **Resolution:** for each of the eight HP-fix binding cards (GD01-004, ST01-004, ST01-010,
  ST02-014, ST04-001, ST04-013, GD05-072, GD05-083) the Python compiler's script differs from
  the binding only in `Stat.HP` vs `Stat.REMAINING_HP`; Rust runs both on the same HP/damage
  states and gets identical decisions and final states, and the targets are exactly the Units
  with the stated current HP. Rust maps both stats to one meaning (`view.stat`).
- **Evidence:** `p6k_behaviour.rs::w68_compiled_and_bound_hp_filters_agree_on_hp_damage_states`
  (compiled scripts from `oracle/tests/vectors/p6k_hp_fragments.json`, built by
  `p6k_vectors.py` with `compiler.compile_card` and `export_ir.build_fragment`); the tagged
  Python tests of the eight cards replay in the corpus (`pinned_cards_have_passing_rust_behaviour_tests`).
  Post-acceptance (§12.11): retire the eight bindings and their stale notes.
- **Ablation:** no AI effect (P6-K common row).

### W-69 — resolved
- **Resolution:** built in by construction: a script the loader cannot build fails
  `Registry::from_ir` with an error naming the card; "unimplemented" is not a load state.
- **Evidence:** `p6k_ir_scans.rs::w69_a_broken_binding_fails_the_build_naming_the_card` (a
  GD04-015 ability with an unknown class fails, naming the class and GD04-015; the packaged IR
  counts 302 binding + 538 compiled + 313 vanilla = 1153 definitions).
- **Ablation:** no AI effect (P6-K common row).

### W-70 — resolved
- **Resolution and evidence:** as W-57: the W-70 rows of `docs/dev/audit/regressions.json`
  (Then/If scoping, Choose 2, the two "instead" fixes, multi-target pre-checks, the cards that
  did not compile, deployed-rested statics, Pilot deployed as a Unit, start-phase freezes, the
  integration repairs) all have passing traces; the golden records equal Python's.
- **Ablation:** no AI effect (P6-K common row).

### W-79 — resolved
- **Resolution:** a tripwire parses every errata notice of the official-source data
  (`official-en-errata-*`: GD04-067, ST12-001, T-013) and requires the corrected wording in the
  effective card, and either gcg-api's applied erratum or an `errata_supersedes_print`
  override.
- **Evidence:** `p6k_data_tripwires.rs::w79_errata_are_applied_with_negative_control`
  (ST12-001 has 【Once per Turn】; removing its override in a copy fails with two findings
  naming ST12-001).
- **Ablation:** no AI effect (P6-K common row).

### W-80 — resolved
- **Resolution:** a scan of every printing for keywords outside `<>` and ASCII cost separators
  (the detector of `sources/conflicts.py:152-158`, `:904-907`), each hit to be covered by a
  `text_format_correction` override that removes the defect.
- **Evidence:** `p6k_data_tripwires.rs::w80_text_format_defects_are_exactly_the_known_cards_and_covered`
  (exactly GD02-053, GD03-015, GD03-035; a synthetic card with both defects is found);
  `p6k_ir_scans.rs::w80_gd02_053_has_suppression_in_rust`.
- **Ablation:** no AI effect (P6-K common row).

### W-81 — resolved
- **Resolution:** no Rust path reads `keyword_effects`, `timing_markers` or `link_refs`.
- **Evidence:** `p6k_ir_scans.rs::w81_blanking_keyword_effects_timing_markers_and_link_refs_changes_nothing`
  (6,000+ fields blanked: identical CardDb fingerprints, the IR loads over the copy, identical
  golden records and content ids for every card); Python
  `oracle/tests/test_p6k_blank_fields.py` (the Python registry, the IR generator, gives
  identical golden records from the blanked copy).
- **Ablation:** no AI effect (P6-K common row).

### W-82 — resolved
- **Evidence:** `p6k_data_tripwires.rs::w82_null_ap_printings_are_reviewed_with_negative_control`
  (109 Base, 33 EX Base, 13 Unit and 3 Unit-token printings with null AP, all reviewed; a
  synthetic null-AP Unit fails).
- **Ablation:** no AI effect (P6-K common row).

### W-83 — resolved
- **Resolution:** every printing whose AP/HP deviate from its canonical printing, and every
  Resource with stats, must be an Edition Beta record superseded by a `divergent:` resolution
  that picks another printing (a superset of the "previous record" shift heuristic).
- **Evidence:** `p6k_data_tripwires.rs::w83_edition_beta_stat_records_are_all_superseded`
  (exactly R-001_p4, T-001_p1, T-002_p1, T-003_p1, T-006_p1, all covered; removing
  `divergent:T-002` uncovers T-002_p1).
- **Ablation:** no AI effect (P6-K common row).

### W-84 — resolved
- **Evidence:** `p6k_data_tripwires.rs::w84_link_and_trait_anomalies_equal_the_known_set`
  (GD05-026's link name "Ericht Samaya" matches no Pilot; "(Dianna Counter)" on GD04-069 and
  GD04-120 is a trait no card carries; no malformed link shape: the GD01-051_p1
  "Trait [Enhanced Human]" record of the older snapshot is not in the packaged one, and the
  shape check rejects it). GD05-026 links as in Python
  (`crates/gcg-data/tests/carddefs.rs::every_card_def_equals_python_field_by_field`).
- **Ablation:** no AI effect (P6-K common row).

### W-85 — resolved
- **Evidence:** `p6k_data_tripwires.rs::w85_curated_conflicts_overrides_and_lock_agree_with_negative_control`
  (333 curated conflicts all resolved and equal to the report; every resolution resolves a
  conflict and equals the report's; policies agree; every derived file and every input of
  `data/conflicts.json` matches its recorded SHA-256; an edited decision text and an orphaned
  resolution each fail with one finding).
- **Ablation:** no AI effect (P6-K common row).

### W-86 — resolved
- **Evidence:** `p6k_data_tripwires.rs::w86_printing_divergences_equal_the_curated_set` (20
  printings of 13 card numbers differ from their base printing after normalisation, each
  covered by a `canonical_printing` resolution; a synthetic divergent printing is found and has
  no resolution).
- **Ablation:** no AI effect (P6-K common row).

### W-87 — resolved
- **Evidence:** `p6k_data_tripwires.rs::w87_every_rules_cross_reference_resolves` (the
  detector's pattern finds 32 references in the 1.9.0 file, 11 of them broken, all 11 mapped by
  a `rules_xref_intended_target` resolution to an existing rule; 0 unresolved; the entry's "34"
  is the research document's count, `docs/research/RULES_INDEX.md:172`, of an older text).
- **Ablation:** no AI effect (P6-K common row).

### W-88 — open (listing done; CLI option not done)
- **Resolution so far:** the listing logic: every source of `data/sources.d/*.json` classed
  packaged, text-only or hash-only (`docs/dev/audit/data-provenance.md`).
- **Evidence:** `p6k_data_tripwires.rs::w88_provenance_listing_covers_every_source` (49 sources
  equal to the lock's; only the Asia floor-rules PDF is text-only; every packaged file's
  SHA-256 equals the lock's).
- **Not done:** `gcg-eval data-status --sources` (the entry's e2e verification): a `gcg-eval`
  and `gcg-data` source change outside P6-K's files (interface note).
- **Ablation:** no AI effect (P6-K common row).

### W-89 — resolved
- **Evidence:** `p6k_data_tripwires.rs::w89_dataset_version_names_the_code_commit_not_the_data_commit`;
  `crates/gcg-data/tests/status.rs::versions_equal_python`; documentation in
  `docs/dev/audit/data-provenance.md` (to fold into the Rust report docs in Phase 8).
- **Ablation:** no AI effect (P6-K common row).

### W-116 — resolved
- **Evidence:** `p6k_data_tripwires.rs::w116_the_canonical_fallback_never_decides_a_non_vanilla_card`
  (no card number needs the lexicographic fallback; the three without a base printing, EXB-001,
  EXR-001, R-001, are decided by overrides; a synthetic `_p10`/`_p2` pair shows the fallback's
  `_p10` pick and trips the wire); Rust and Python pick the same canonical printing for every
  card (`carddefs.rs::every_card_def_equals_python_field_by_field`).
- **Ablation:** no AI effect (P6-K common row).

### W-145 — open (loader and listing logic done; CLI option not done)
- **Resolution so far:** sets, products and card numbers are data: a synthetic copy of the
  snapshot with ten new GD06 cards loads, and the numbers resolve, with no code change. The
  freshness logic lists products released after the snapshot's build date with no card
  numbers.
- **Evidence:** `p6k_data_tripwires.rs::w145_new_sets_are_data_and_the_freshness_listing_names_gd06`
  (since the 2026-11-06 data refresh `..._names_gd07`, over EVX10 and GD07) (as of 2026-11-01 the listing is exactly GD06, 2026-10-30; GD07, due 2027-01-29, is not
  listed; with the synthetic GD06 cards the listing is empty).
- **Not done:** `gcg-eval data-status --freshness [--as-of DATE]`, and validating a deck against
  a non-embedded snapshot (the public `parse_deck`/`validate` use the embedded database):
  `gcg-data`/`gcg-eval` source changes (interface notes). Running `/gcg-refresh-data` for GD06
  is the lead's.
- **Ablation:** no AI effect (P6-K common row).

### W-146 — resolved
- **Resolution:** each of the 566 rules is compared, text against text, between the markdown
  and the cached PDF extraction, after whitespace, markdown and extraction normalisation (no
  space next to a hyphen, inside `【】`, around `･`, before `：` or after `“`); one reviewed
  artefact remains in the allowlist (10-3-3 "ta rgets").
- **Evidence:** `p6k_data_tripwires.rs::w146_rules_markdown_matches_the_pdf_rule_by_rule_with_negative_control`
  (0 differences, allowlist size 1; changing one word of 1-2-4 in a copy fails with exactly
  that rule).
- **Ablation:** no AI effect (P6-K common row).

### W-155 — resolved (Rust side)
- **Resolution:** the parenthetical gate: the Rust port of `normalize`'s stripping
  (checked against every card's `text_hash` and against Python's own stripping) extracts every
  stripped parenthetical; 13 reviewed rule and keyword reminders are allowlisted with their
  rule; the other three are a per-card snapshot.
- **Evidence:** `p6k_data_tripwires.rs::the_text_port_reproduces_every_cards_text_hash`,
  `w155_stripped_parentheticals_equal_python` (16 distinct texts),
  `w155_parenthetical_gate_passes_on_the_packaged_data`,
  `w155_parenthetical_gate_negative_controls` (rewording GD05-089's fails naming GD05-089; a
  curly apostrophe and extra spaces in a keyword reminder pass; a reworded keyword reminder
  fails). Post-acceptance (§12.11): the `parenthetical: review` class in `tools/refresh.py`;
  running the gate inside the refresh workflow from M17 is the lead's.
- **Ablation:** no AI effect (P6-K common row).

### W-157 — resolved
- **Resolution:** built in by construction: the overrides file is embedded with `include_str!`
  (a missing file fails the build), and `CardDb::from_sources` fails on unparsable or
  ill-shaped overrides naming `overrides.json`.
- **Evidence:** `p6k_data_tripwires.rs::w157_packaged_overrides_load_every_card_data_resolution`
  (402 resolutions; every resolution that changes card data is applied to each of its cards;
  empty and truncated overrides fail naming the file);
  `crates/gcg-data/tests/synthetic.rs::overrides_never_fall_back_to_empty`.
- **Ablation:** no AI effect (P6-K common row).

### W-164 — resolved
- **Resolution:** the dead loader paths are implemented and tested on synthetic data
  (`crates/gcg-data/tests/synthetic.rs` with Python's expected values); the tripwire fails when
  the packaged data first takes one.
- **Evidence:** `p6k_data_tripwires.rs::w164_dead_loader_paths_stay_dead_with_negative_controls`
  (passes on the packaged data; the synthetic cases `synthetic-token`, `token-tie`,
  `nested-field-overrides` and `singular-card-number` each trip it naming the path);
  `synthetic.rs::synthetic_sources_load_as_in_python`; the id-inferred resolutions resolve the
  same card data as Python (`carddefs.rs::every_card_def_equals_python_field_by_field`).
- **Ablation:** no AI effect (P6-K common row).

## Interface notes for the lead

1. `gcg-eval data-status --sources` (W-88): move `provenance_listing` of
   `p6k_data_tripwires.rs` into `gcg-data` (embed `data/sources.d/*.json`) and print it.
2. `gcg-eval data-status --freshness [--as-of DATE]` (W-145): `freshness` of the same test, with
   `products.json` and the manifest embedded in `gcg-data`.
3. `gcg-data`: public `parse_deck`/`validate` over a given `CardDb` (the crate-private
   `parse_deck_with` and `Validator::validate`), so a refreshed snapshot's decks validate in a
   test without rebuilding (W-145's "a deck using the new cards validates").
4. `export_ir.py` and `gcg-eval data-status --templates` (W-63, W-67): set `cards[].template` to
   the template id that `oracle/templates/check.py` proves byte-identical (the IR slot and
   the loader already exist, `CardEntry::template`); this changes the IR bytes and so the
   corpus `ir_sha256`, which is why P6-K did not do it.
5. `gcg-engine` (W-64): one implementation with a scope parameter for
   `gd04b_played_with_ex`/`gd05b_played_with_ex`, and one parametrised predicate for the three
   destroyed-cause conditions, behaviour-identical (the per-test traces of GD04-106, GD04-108,
   GD05-* pin them).
6. Gate: add `uv run python oracle/p6k_vectors.py --check`, `oracle/p6k_audit.py
   --check`, `oracle/templates/check.py --check` and `uv run pytest oracle/tests -q`
   to `scripts/verify_all_rust.sh`; add the new tests to `docs/dev/tests.json`.
7. Post-acceptance package steps (§12.11, A32 item 4 answer (a)): W-63/W-67 move the templates
   into the compiler and retire the 6 reproduced bindings; W-68 retire the 8 HP-fix bindings and
   their notes; W-155 add `parenthetical: review` to the refresh diff.
