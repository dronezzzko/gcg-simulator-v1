# P6-L status: validator (class N)

Package P6-L of docs/dev/PLAN.md §12.2: W-71, W-72, W-73, W-74, W-75, W-76, W-77, W-78 and
W-144 (9 items). Written by the P6-L implementer for the lead, who records the outcomes in
docs/dev/WEAKNESSES.md (§12.10). This file does not edit WEAKNESSES.md.

## What P6-L changed

No packaged data, cached source, `src/gcg_sim` or `tests/` file changed, and no file of
gcg-engine, gcg-ai, gcg-ai-reference, gcg-search, gcg-runner, gcg-effects or gcg-core.
`gcg-sim validate` is unchanged (A30). Every new option lives in `gcg-eval validate`.

| File | Items |
| --- | --- |
| `crates/gcg-data/src/legality/mod.rs` (new): `check`, `LegalityOptions`, `LegalitySources`, `LegalityReport`, `Hint`, `text_hints`, `file_hints` | W-71, W-72, W-75, W-76, W-77, W-78 |
| `crates/gcg-data/src/legality/lists.rs` (new): `BanlistHistory`, `BanlistEntry`, `ProductDecklist`, `ProductException`, `AppliedException`, `SelectError` | W-71, W-72 |
| `crates/gcg-data/src/legality/releases.rs` (new): `ReleaseDates`, `EventCheck`, `NewCardViolation`, `number_set`, `NEW_CARD_DAYS` | W-76 |
| `crates/gcg-data/src/legality/scope.rs` (new): `Scope` (and its `analysis.json` block `Scope::to_json`), `FORMAT_APPLIED`, `UNSUPPORTED_FORMATS` | W-75, W-76 |
| `crates/gcg-data/src/legality/date.rs` (new): `IsoDate` (no clock) | W-72, W-76 |
| `crates/gcg-data/src/validate.rs`: `Rules` gains the per-deck exception state (`Exceptions`); empty, it is Python's behaviour | W-71 |
| `crates/gcg-data/src/official.rs`: `parse_banlist` split into `banlist_document` + `parse_banlist_value` (same result), helpers made `pub(crate)` | W-72 |
| `crates/gcg-data/src/deck.rs`: `comment_hints` (new function; parsing unchanged) | W-77 |
| `crates/gcg-eval/src/validate_cmd.rs` (new; 10 tests) and the dispatch lines in `crates/gcg-eval/src/cli.rs`, `lib.rs`. It embeds `gcgapi/products.json` and `official/edition_language.json` (read-only; `packaged_releases`, `packaged_scope`) | W-71 to W-78, W-144 |
| `crates/gcg-data/tests/p6l_validator.rs` (15 tests), `crates/gcg-data/tests/p6l_tripwires.rs` (4 tests) | all items |

`crates/gcg-data/src/embedded.rs` is unchanged. Its list is exactly the files the Python
runtime reads (PLAN §2.4, pinned by `gcg-effects` `ir_current::the_embedded_data_bytes_equal_gcg_datas`).
The two extra files the report needs are embedded by the opt-in tool, and gcg-data's API takes
their text from its caller (`ReleaseDates::from_json`, `Scope::new`).

`gcg-eval validate DECK [--as-of DATE] [--region R] [--no-banlist] [--no-implemented]
[--event-date DATE] [--scope]` prints the report. Its first line, for a legal deck, is `gcg-sim
validate`'s line. A failing deck prints the report on stdout and `error: {path}: {n}
violation(s)` on stderr (exit 1). A deck that does not parse prints its hints and `gcg-sim
validate`'s error (exit 1). Date and option-combination errors are usage errors (exit 2).
`--no-implemented` is the one option beyond A30's list: W-78's plan needs the switch on the
command line.

### Data hooks (schema-ready, data unchanged)

`banlist.json` is read as the newest entry of a history. Two optional keys, absent today, are
read when a data refresh adds them (documented in `legality/lists.rs`):

- `history`: older lists, each with the top-level list's keys plus `regions`.
- `official_product_decklists`: `{product: {main, resources, source_url, retrieved_at}}`.
  `source_url` and `retrieved_at` are required (provenance). An exception whose `condition` is
  `{"deck_equals_official_product_decklist": product}` exempts a deck from exactly the
  restriction that lists it, when the main deck and the resource deck equal the decklist.

Region codes are the official site's region path segment: `en` (North America, Latin America,
Europe, Oceania) and `asia-en` (English Asia). The current list's regions are its `regions` key,
else the region segments of its `source_urls`. Today both sites' current-list page (01_279) is
in `source_urls`, and the `region` field says the contents are identical, so the current list
covers `asia-en` and `en`.

## Gate (run on this branch)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0, no warning |
| `cargo test --workspace --release` | exit 0: 123 suites, 1011 passed, 0 failed, 0 ignored (980 before P6-L, plus 31 new: 15 `p6l_validator`, 4 `p6l_tripwires`, 2 `legality::date`, 10 `validate_cmd`). This includes `gcg-effects` `ir_current::the_embedded_data_bytes_equal_gcg_datas` (gcg-data still embeds exactly the 8 Python-runtime files) and the `MechanismFlags::NULL` tests |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 (baselines 8/8, coverage 50/50, full 4/4, fuzz 100/100, search 9/9, tests 3213/3213, vanilla 40/40, views 280/280, worlds 100/100) |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` (`COMPARE_OUT=build/p6l-compare`) | `equivalence: 3/3 identical (fast 1/1; standard 1/1; strong 1/1; decision-log off 3/3;)` |
| `target/release/gcg-diff lints` | `lints passed`, exit 0 |

## Ablation rows (A13)

Every P6-L item is class N and changes no code the AI executes. The AI plays games from a
`DeckList` (`to_decklist`, unchanged) on the effect registry and the engine. Deck validation runs
once, before any game (`legal_deck`). P6-L adds code to that validation path, and with the
default state (no decklist hook, the current list) it gives Python's results: the whole Python
deck corpus matches. P6-L adds no other runtime path. `git diff --stat` against the base commit
shows no change under `crates/gcg-engine`, `gcg-ai`, `gcg-ai-reference`, `gcg-search`,
`gcg-runner`, `gcg-effects`, `gcg-core`, `src` or `tests`. So each item's ablation row reads:
**"No AI effect: P6-L changes deck validation, deck-file hints and a `gcg-eval` subcommand, none
of which runs during a game. The AI's decisions are Python's: the tier-1 corpus is at 3804/3804
(the `search` set 9/9 with 771 agent decisions, the `baselines` set 8/8 with 625), and the
reference equivalence smoke is 3/3 identical (fast, standard, strong). Behaviour evidence: the
tests named in the item."** Below,
each item says "Ablation: no AI effect (P6-L common row)".

## Items

### W-71 — resolved (data hook; the decklists are a data task)
- **Resolution:** the exception logic sits behind a data hook (`official_product_decklists`,
  `legality/lists.rs`; applied in `validate.rs` `Exceptions`). If the deck equals the official
  decklist card for card, the restriction that lists the exception is not checked, and the
  report lists the exception as applied. If the data has the decklist and the deck differs,
  the note says so: "Exception 'st02-unmodified-starter-deck' is not applied: the deck is not
  the unmodified official ST02 decklist." Without a decklist (the packaged data today) the
  message is Python's, with `NOT_APPLIED`.
- **Evidence:** `p6l_validator.rs::w71_starter_deck_exception_hook`. With the packaged data,
  the synthetic ST02 deck (3x ST02-016) gives exactly `validate_deck`'s RESTRICTED violation
  with Python's note, and the ST05 deck gives ATTRIBUTE_PAIR. With a synthetic decklist
  fixture, both exact lists are legal and list their exception. A one-card variation gets
  COPY_LIMIT plus RESTRICTED with the "not the unmodified" note. A swap gives RESTRICTED alone.
  The ST05 exception stays Python's while only the ST02 decklist exists. A modified ST05 deck
  is never exempt. `w71_decklists_need_provenance`: a decklist without `source_url`, or with
  a count of 0, fails to load. e2e (`target/release/gcg-eval validate st02.txt`): exit 1,
  `[RESTRICTED] 3 copies of ST02-016 Corsica Base; ... Exception 'st02-unmodified-starter-deck'
  is not applied: the simulator cannot verify an unmodified starter deck.`, and `gcg-sim
  validate` on the same file is byte-identical to Python's.
- **Handed to the lead:** adding the official ST02/ST05 decklists with provenance is a
  `/gcg-refresh-data` task. Neither the official pages nor gcg-api has them today.
- **Ablation:** no AI effect (P6-L common row).

### W-72 — resolved
- **Resolution:** `BanlistHistory` holds dated, region-tagged lists. The packaged data provides
  one, the current list, which equals `banlist()`. `--as-of DATE` and `--region R` select the
  newest list in effect on that date in that region. With neither option, the current list
  applies (`validate_deck`'s). With a date and no region, every region must apply the same
  list, or the command fails and asks for `--region`. A date before every list fails with an
  error that names the dates the data covers. `--region` offers the regions in the data
  (`{asia-en,en}`). Malformed histories fail when the data loads: two lists of one region on
  the same day, a history list after the current list, a list with no region, a bad date.
- **Evidence:** `w72_default_check_equals_validate_deck_on_the_python_corpus`: over the
  `decks.json` corpus, 651 validations (default, `banlist=False`, `implemented=False`) equal
  Python's row for row. `w72_packaged_model_is_one_current_list`: one list, `asia-en, en`, the
  error texts. `w72_two_entry_fixture_selects_by_date`: the July list (dated 2026-07-24,
  without ST11-008 and ST12-010) applies on 2026-08-01, with its date in the messages and the
  membership note for ST11-008. The current list applies from 2026-09-25 and equals
  `validate_deck`. 2026-07-23 is an error. `w72_regional_lists_need_a_region_where_they_differ`
  (US 07-24, Asia 07-25). `w72_malformed_histories_are_load_errors`. e2e: `--as-of 2026-10-01
  --region asia-en` gives exit 0 and names the list. `--as-of 2026-09-24` gives exit 1 with
  `error: no Banned & Restricted list in the data was in effect on 2026-09-24; the lists in
  the data take effect on 2026-09-25 (asia-en, en). Older lists are added by a data refresh
  (/gcg-refresh-data)`. `--as-of 2026-9-24` gives exit 2 (usage).
- **Handed to the lead:** adding historical lists (the July lists of 2026-07-24 US and
  2026-07-25 Asia) is a data-refresh task. The port does not change `banlist.json`.
- **Ablation:** no AI effect (P6-L common row).

### W-73 — resolved
- **Resolution:** a data-gate tripwire (`p6l_tripwires.rs::vanilla_divergence`). Over the
  packaged pool, the vanilla predicate's matches must equal the enumerated list, and both must
  equal the data's own `predicate_matches_in_card_data`. The validator's run-time behaviour
  (predicate OR list, a note on divergence) is unchanged.
- **Evidence:** `w73_vanilla_predicate_equals_the_enumerated_list_with_negative_control`. The
  packaged data passes with 22 matches and 22 listed. Negative control 1: a synthetic
  vanilla Lv.2/cost 1/2 AP/2 HP Unit `SY01-001` in a data copy fails it ("matches the predicate
  but is not enumerated"; the recorded matches say 22 but the pool has 23). Negative control
  2: an edited copy that enumerates GD01-001 fails it ("is enumerated but does not match").
- **Ablation:** no AI effect (P6-L common row).

### W-74 — resolved (one correction to the entry's premise, stated)
- **Resolution:** three tripwires, with the validator's behaviour unchanged.
  - (a) `effect_reading_findings`: a card is flagged if an attribute rule's predicate
    classifies it differently when keywords are not effects (effect text made only of
    `<Keyword>` tokens and reminder parentheticals) or when a link requirement alone is an
    effect.
  - (b) `alias_findings`: a `name_contains` value is flagged if it matches an alias of a card
    but not the card's primary name. **Correction:** the data has no `extra_names` (every
    `CardDef.extra_names` is empty). The rule 2-2-4 aliases are the 【Pilot】 names of the 68
    Commands (`CardDef.names`) and the text "This card's name is also treated as [X]" of
    GD02-098 (Char Aznable), ST12-011 (Zechs Merquise) and ST12-012 (Marida Cruz). The
    tripwire reads both sources and pins them.
  - (c) `colour_findings`: a printing is flagged if its colour is not exactly one of the five
    colours, and a UNIT, PILOT, COMMAND or BASE card is flagged if it has no colour.
- **Evidence:** `w74_without_effects_reading_with_negative_controls`: the packaged pool
  passes. Synthetic `SY01-002` (`<Blocker>` and its reminder only) and `SY01-003` (link
  `[Amuro Ray]`, effect `-`) each trip it. `w74_name_contains_misses_no_alias_with_negative_control`:
  the packaged list has no `name_contains`. An edited copy with `name_contains` set to a
  Command's 【Pilot】 name, or to "Zechs Merquise", trips it on that Command and on ST12-011.
  `w74_one_colour_per_card_with_negative_controls`: the pool passes. A `"Blue/Red"` printing
  trips it, by name (the loader also rejects it, but without naming the colour rule), and so
  does a colourless synthetic UNIT.
- **Ablation:** no AI effect (P6-L common row).

### W-75 — resolved in scope; the `analysis.json` part is handed to P6-M
- **Resolution:** the default stays tournament play (the B&R list applied). `gcg-eval validate
  --no-banlist` is the library's `banlist=False` (`LegalityOptions.banlist = None`). Only that
  run's report header carries `WARNING: no Banned & Restricted list applied (--no-banlist):
  only the Comprehensive Rules and simulator support are checked, but official and sanctioned
  tournaments enforce the list`. Every report names the format (`format: Constructed`) and
  the list applied (date, regions). `gcg-sim validate` and the benchmark reports are
  unchanged.
- **Evidence:** `p6l_validator.rs::w75_no_banlist_accepts_a_banned_card`. 4x GD01-020 (Blue,
  in place of 4x GD01-008) gives BANNED by default and is legal with no list.
  `validate_cmd::tests::w75_no_banlist_warns_in_the_header`: the header warning, and the
  default run's BANNED, exit 1, with `gcg-sim validate`'s error. e2e: Python and Rust `gcg-sim
  validate` give identical stdout, stderr and exit codes on 8 decks (below).
- **Handed to P6-M:** the `scope` block of `analysis.json` (written only with `--analysis`,
  W-90). `gcg_eval::validate_cmd::packaged_scope().to_json(Some(list))` returns it (list:
  `BanlistHistory::packaged().current()`, or `None` for no list), with keys `format`,
  `banlist{applied, effective_date, regions, format}`, `unsupported_formats`,
  `unchecked_tournament_rules`, `unmodelled_tournament_procedures` and `edition_beta`.
  `p6l_validator.rs::w76_scope_statement` and
  `validate_cmd::tests::embedded_sources_and_the_analysis_scope_block` test it. Wiring it
  into `analysis.json` is P6-M's (P6-L owns no runner or benchmark file).
- **Ablation:** no AI effect (P6-L common row).

### W-76 — resolved in scope; the `analysis.json` block is handed to P6-M, Sealed data to the lead
- **Resolution:**
  - `--event-date DATE` checks the 7-day new-card rule (TRM 2.4.1). A card number first released
    on day R may be used from R + 7. R is the earliest dated product (`gcgapi/products.json`)
    of the set the card number starts with. The canonical printing's `set_code` is not used,
    because reprints move it: R-001's canonical printing is the SC01 box of 2026-07-24. Card
    numbers whose set has no dated product (`R`, `RP`, `T`, `EXB`, `EXBP`, `EXR`, `EXRP`) are
    skipped and listed in a note. A card not yet released on the event date is reported as
    such.
  - `--scope` lists:
    - the format applied (Constructed, with `banlist.json` `format` and `applies_to`);
    - Sealed and Draft as unsupported, with their TRM 3.3 definitions;
    - the tournament rules not checked: `deck_construction.json`
      `tournament_only_rules_not_modelled`, the Asia Newtype Challenge 7-day clause and the
      sideboard scope;
    - the tournament procedures not modelled (`bo3_match_rules.json` `ignored_by_simulator`,
      including `intentional_draws_concessions_penalties: Not modelled.`);
    - the Edition Beta facts (`edition_language.json`: no official statement; EXBP-001 and
      RP-001 are Edition Beta only), flagging the deck's Beta-only numbers.
  - Without the options the report has neither section. `gcg-sim validate` is unchanged.
- **Evidence:** `p6l_validator.rs::w76_event_date_seven_day_rule`. The blue-white example
  with 2x ST11-016 (released 2026-09-25) is legal without the option. With `--event-date
  2026-09-28` (3 days later) it fails with one NEW_CARD violation (usable from 2026-10-02),
  with R-001 listed as not checked. It also fails on 2026-10-01, passes on 2026-10-02, and is
  "not released until" on 2026-09-20. `w76_release_dates_cover_every_main_deck_set`: every
  main-deck card number has a date, and GD06 is dated 2026-10-30. `w76_scope_statement`.
  e2e (`validate_cmd::tests::w76_*` and the binary): exit 1 with `[NEW_CARD] ST11-016 Mad
  Angler was first released on 2026-09-25 (Aquatic Assault [ST11]), 3 day(s) before the event
  on 2026-09-28; ...`, exit 0 without the option. `--scope` on a deck with RP-001 prints
  `FLAG: this deck uses Edition Beta-only card numbers, whose tournament legality is
  undecided: RP-001`, Constructed applied, and Sealed and Draft unsupported.
- **Handed on:** the `analysis.json` `scope` block goes to P6-M (`Scope::to_json`, as in
  W-75). The byte-identity of the benchmark outputs without `--analysis` holds because P6-L
  changes no runner or report file (the equivalence smoke is below). Supporting Sealed needs
  its rules page (`news/sealed-battle.html`) in the data through `/gcg-refresh-data`: a lead
  task.
- **Ablation:** no AI effect (P6-L common row).

### W-77 — resolved
- **Resolution:** the parser keeps the format and the violations. Hints are additive
  (`LegalityReport.hints`, `legality::text_hints`/`file_hints`, `deck.rs::comment_hints`).
  - An EX Resource or EX Base listed in a deck gets a hint under its CARD_TYPE violation,
    naming rule 6-1-2 and the size it counts toward (RESOURCE_SIZE or MAIN_SIZE).
  - A line whose `#` follows a card gets a hint when the text before the `#` is a valid entry:
    a NAME_MISMATCH whose name before `#` is empty or matches, or an UNKNOWN_ID whose id
    before `#` resolves.
- **Evidence:** the violation codes and messages equal Python's over the whole deck-format
  corpus (`w72_default_check_equals_validate_deck_on_the_python_corpus`; `decks.rs`
  unchanged and passing). `w77_ex_token_hints`: the CARD_TYPE violation equals
  `validate_deck`'s, and the hint ends "count toward the resource deck's 10 cards
  (RESOURCE_SIZE)" or "... main deck's 50 cards (MAIN_SIZE)". `w77_comment_hints`: hints on
  lines 1-3, the parse violations equal to Python's wording, no hint on a real name mismatch
  (line 5) or on the example decks. e2e: `gcg-eval validate hash.txt` prints `hints:\n  - line
  7: 'Guntank' matches GD01-008 and the text from '#' is read as part of the name: ...` and
  `gcg-sim validate`'s error, exit 1.
- **Ablation:** no AI effect (P6-L common row).

### W-78 — resolved (one addition to A30's option list, stated)
- **Resolution:** the check stays. The report separates "rules (Comprehensive Rules and Banned
  & Restricted list)" from "simulator support (not playable by this simulator version)"
  (`LegalityReport.rules` and `.simulator`). `gcg-eval validate --no-implemented` exposes
  `implemented=False` for validation-only use. **Addition:** A30 does not list
  `--no-implemented`, but W-78's plan requires the switch on the CLI, and it lives in
  `gcg-eval` as A30 requires.
- **Evidence:** `p6l_validator.rs::w78_simulator_support_is_reported_apart`. A synthetic
  unimplemented GD01-008 gives no rules violation and one UNIMPLEMENTED under simulator
  support. With the switch the deck is legal and simulator support is not checked.
  `validate_cmd::tests::w78_simulator_support_heading` checks the same under the report
  headings, with `--no-implemented` exiting 0.
- **Ablation:** no AI effect (P6-L common row).

### W-144 — resolved (landed at M4; P6-L adds the e2e and memory evidence)
- **What landed** (M4, rust-port at `a4977be`, squashed into commit 83a06c8; workstream A: `crates/gcg-data/src/deck.rs`, `contracts.rs`):
  - counts are read as `u64` with Python's grammar (Unicode digits);
  - a count longer than `int()` accepts (4,300 digits) is a SYNTAX violation naming the line,
    as is a count that does not fit `u64` when nothing else stops the parse;
  - decks are run-length (`CardCount`), sizes and copy limits come from the counts, and
    `to_decklist` refuses totals above 65,535 (`DeckError::TooLarge`).
- **Evidence:** existing tests: `deck.rs::tests::huge_counts_are_syntax_violations`,
  `tests/decks.rs::huge_counts_validate_without_expanding`, and the `decks.json` corpus
  (random lines with counts up to 1,000; `deck_cases_equal_python`). New in P6-L:
  - `p6l_validator.rs::w144_huge_counts_in_files_and_the_extended_check`: a 5,000-digit count
    in a deck file gives a SYNTAX violation; 10^12 gives MAIN_SIZE and COPY_LIMIT in the
    extended check.
  - `validate_cmd::tests::w144_huge_count_exits_one_with_syntax`: exit 1 and the SYNTAX
    message from both `gcg-sim validate`'s body (`cli_support::validate_command`) and
    `gcg-eval validate`.
  - On the binaries: Python `gcg-sim validate big.txt` exits 1 with a traceback
    (`ValueError: Exceeds the limit (4300 digits) ...`). Rust `gcg-sim` and `gcg-eval` exit 1
    with `line 1: count must be at most 18446744073709551615 for GD01-001`.
  - Memory: Python `gcg-sim validate huge.txt` (10^12 copies) was killed by the system
    (exit 137) in the first run. It was not re-run, to spare a shared, loaded machine. With
    the final binaries, `/usr/bin/time -l` gives a peak RSS of 45,268,992 bytes for Rust
    `gcg-sim` and 45,432,832 for `gcg-eval` on that file, against 46,104,576 and 45,236,224
    for the 50-card example. No list is expanded per copy (10^12 entries could never be
    allocated).
- **Ablation:** no AI effect (P6-L common row).

## End-to-end runs (release binaries, this branch)

`gcg-sim validate`, Python (`.venv/bin/gcg-sim`) against Rust (`target/release/gcg-sim`):
identical stdout, stderr and exit code on `bw` (exit 0), `banned` (1), `newcard` (0), `beta`
(0), `exr` (1), `hash` (1), `st02` (1). This held in two runs, before and after the final
edit. On `huge`, Python was killed (137) and Rust exits 1 (W-144; first run only, as above).
On `big`, Python raises and Rust reports SYNTAX (W-144). The deck files are the blue-white
example with one change each, plus the synthetic ST02 list and the two count files. The
`gcg-eval validate` runs and their exit codes are quoted in the items above.

## Interface notes for the lead

- `crates/gcg-eval/src/validate_cmd.rs` embeds `src/gcg_sim/data/gcgapi/products.json`,
  which `build-support/digest-manifest.txt` does not list, so `code_digest` (W-103) does not
  cover it yet. Add the line `src/gcg_sim/data/gcgapi/products.json`.
  `official/edition_language.json` is already covered by `official/*.json`. PLAN §2.4 could
  gain a row: `gcg-eval` embeds these two non-runtime files for the opt-in `validate`
  report. gcg-data's embedded set stays the Python runtime's.
- `crates/gcg-eval/src/cli.rs` gains 8 lines: the `validate` subcommand entry,
  `Command::Validate`, and two match arms. P6-K and P6-M add subcommands in the same places.
- Interpretations to record in `docs/ASSUMPTIONS.md` (lead-owned):
  - region codes are site path segments, and the current list's regions come from its
    `source_urls`;
  - without `--region`, a date on which the regions disagree is an error;
  - a card number's release is that of the set its number starts with;
  - the starter-deck exemption needs an exact main and resource deck match.
  `docs/DECK_FORMAT.md` could point to `gcg-eval validate`.
