# What to update for each kind of change

In the tables below, "gen" means a generated file: regenerate it with the command shown and
never edit it by hand.

## Card data (gcg-api)

| Change | Files | How |
| --- | --- | --- |
| New gcg-api commit | `src/gcg_sim/data/gcgapi/*` (9 files), `data/sources.d/gcgapi.json` | `python -m gcg_sim.tools.refresh apply --new-data <clone> --retrieved-at <ts>` |
| Lock and summary | `data/SOURCES.lock.json`, `docs/SOURCES.md` (gen) | `python -m gcg_sim.tools.sources --write` |
| Conflicts report | `data/conflicts.json`, `docs/CONFLICTS.md` (gen) | `python -m gcg_sim.sources.conflicts` |
| Conflict resolutions | `src/gcg_sim/data/overrides.json` | by hand, see `conflict-policies.md` |
| Curated conflicts, reviewed rulings/FAQ | `src/gcg_sim/data/curated_conflicts.json` | by hand; fingerprints come from the conflicts report |
| New or changed card behaviour | `src/gcg_sim/effects/bindings/<package>.py` (`<package>` = lowercase work-package id, e.g. `wp_gd06_a`) or the compiler (`src/gcg_sim/effects/compiler/`) | `card-checklist.md` |
| Card tests | `tests/cards/test_<package>.py` (`@pytest.mark.card`, `@pytest.mark.ruling`, `@pytest.mark.faq`) | `card-checklist.md` |
| Ruling/FAQ N/A reasons | `tests/meta/rulings_na/<package>.json`, `tests/meta/faq_na.json` (`{"CARD:Qn": "reason"}` / `{"Qn": "reason"}`) | by hand |
| Golden compiled behaviour | `tests/effects/golden/<PREFIX>.json` (gen) | `python -m gcg_sim.tools.golden --write --prefix <PREFIX>` after review |
| Vanilla-pair list, new cards | `src/gcg_sim/data/official/banlist.json` | by hand; verify with `refresh banlist` |
| Research profile (optional) | `docs/research/data_profile.json` (gen), `scripts/ingest_profile.py` `PINNED_COMMIT` | bump the constant; `uv run python scripts/ingest_profile.py --full-clone <clone>` |
| Effect templates (optional) | `docs/research/effect_templates.json`, `work_packages.json`, `EFFECT_TEMPLATES.md` (gen), `scripts/cluster_effects.py` `GCG_API_COMMIT` | bump the constant; `uv run python scripts/cluster_effects.py` |
| Example decks | `examples/decks/*` | only if a card they use changed or was banned; `gcg-sim validate` |

## Official pages

| Change | Files | How |
| --- | --- | --- |
| Any fetched page changed | `data/official_raw/<slug>.html|pdf` + `<slug>.txt` | copy from the fetch directory (raw files are replaced whole, never edited) |
| Its provenance | `data/sources.d/official.json` | `scripts/official_normalize.py fragment --retrieved-at <ts>`; new pages need an entry first |
| B&R list | `src/gcg_sim/data/official/banlist.json` | by hand: `banned`, `restricted`, `banned_pairs`, `attribute_pair_rules`, `effective_date`, `source_urls`, `ambiguities`, `verification` |
| Deck construction | `src/gcg_sim/data/official/deck_construction.json` | by hand; every `{"quote", "local_text"}` is verbatim |
| Shuffle, first player, redraw | `src/gcg_sim/data/official/floor_rules.json` | by hand |
| BO3 procedure | `src/gcg_sim/data/official/bo3_match_rules.json` | by hand |
| Language, edition, errata notices | `src/gcg_sim/data/official/edition_language.json` (`errata.official_notices`) | by hand; add each new errata notice here, plus an override if gcg-api lacks it |
| Research notes | `docs/research/OFFICIAL_SOURCES.md` | by hand: source map, B&R tables, US/Asia differences, ambiguities |
| Checks | none | `scripts/official_normalize.py check`, `python -m gcg_sim.tools.refresh banlist` |

The normalized files are hashed in the lock's `derived` list, so re-run
`python -m gcg_sim.tools.sources --write` after editing them. `rules_version.json` and
`banlist.json` are also inputs of the conflicts report, so re-run
`python -m gcg_sim.sources.conflicts` as well.

## Comprehensive Rules

| Change | Files | How |
| --- | --- | --- |
| New version | `src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md` and the byte-identical repo-root copy | transcribe from the PDF text (see SKILL.md step 3) |
| Rules index | `src/gcg_sim/data/rules/rules_index.json` (gen) | `python -m gcg_sim.rules.index --write`, then `--check` |
| N/A rules | `src/gcg_sim/data/rules/rules_na.json` | by hand: `"rules": {"n-n-n": {"status": "na", "reason": "..."}}` |
| Rule tests | `tests/rules/...` (`@pytest.mark.rule("n-n-n")`) | new or changed rules |
| Version record | `src/gcg_sim/data/official/rules_version.json`, `data/sources.d/rules.json` | by hand; `official_normalize.py check` verifies it |
| Traceability | `docs/RULES_TRACEABILITY.md` (gen) | `pytest -m "" --junitxml=build/junit.xml`, then `python -m gcg_sim.tools.traceability --junit build/junit.xml --check` |
| Version mentions | `docs/SPEC.md` (Goal), README and other docs | `grep -rn "Ver\. 1\." docs README.md` |

## Engine or DSL

See `engine-extension.md`. Those changes touch `src/gcg_sim/effects/dsl.py`, the compiler, the
engine (`src/gcg_sim/engine/`), `src/gcg_sim/sources/conflicts.py` (keyword and timing
vocabularies) and the tests.
