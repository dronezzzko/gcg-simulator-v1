# Progress log

Machine-readable status: `docs/status.json`. This log survives context compaction; each phase
records what was done, evidence, and what remains.

## Phase 0 — Preflight and scaffold (done)
- Preflight checks recorded in `docs/ASSUMPTIONS.md`. Branch `feat/gcg-sim`.
- uv package scaffolded (`uv init --package`, Python 3.12, ruff, mypy strict, pytest, hypothesis).

## Phase 1 — Research (done; commit e240669)
- Data ingestion profile, conflicts subsystem, rules index (579 entries), official sources
  (B&R 2026-09-25, floor/BO3/deck rules), effect-template clustering and 19 work packages.
  Outputs under `docs/research/`, `data/`.

## Phase 2 — Foundations (done; commits 816f9e4, 25d43df, 102e72b, 7a797fb)
- Engine core, DSL, compiler, bindings API, testkit, golden files, coverage gates,
  determinization.
- Rule-tagged suites from 10 parallel rules agents (turn, zones, battle, damage, effects,
  keywords, pairing, card info, FAQ, lead examples); 26 engine bugs they surfaced were fixed.
- Rules index: 395 testable rules, 184 N/A with written reasons (multiplayer section 12, physical
  play, tournament procedure, grouping headings).

## Phase 3 — Card effects (done; merges 13422d9..ddb9f4f, fixes 0e1d3e8, 0ee591d)
- 17 per-set work packages ran in isolated worktrees and returned structured JSON (985 cards
  reported, 111 readings of ambiguous text, 69 engine/DSL gaps, 85 grammar notes).
- The lead merged every package, then closed all 69 gaps in the engine/DSL: the 38 strict
  xfails that documented them were removed once they passed.
- Evidence: `tests/effects/test_card_coverage.py` (1148/1148 card numbers implemented, every
  non-vanilla card has a `@pytest.mark.card` test); `tests/test_faq_ruling_coverage.py` (all 119
  rules-FAQ entries and 368 card rulings tested or N/A); random fuzz 3000 games / 254,490
  actions / 0 invariant failures covering all 1063 pool cards.
- Card-agent readings were curated into `curated_conflicts.json` with verified quotes and
  `most_defensible_reading` resolutions (commit da570b3): `docs/CONFLICTS.md` lists 395
  conflicts, 0 unresolved/invalid/stale/orphaned.

## Phase 4 — AI, runner, CLI, reports (in progress)
- Deck/runner/reports/CLI package merged (5174e95): deck parser and validator with every
  violation class, 3 legal example decks, seeded BO3/BO1 runner with a process pool,
  replayable game records, schema-validated reports, `gcg-sim` CLI (`benchmark`, `validate`,
  `data status`, `replay`).
- AI package (random, greedy, determinized MCTS, presets, decision log): agent still running.

## Phase 5 — Skills and docs (in progress)
- `gcg-refresh-data` skill merged (06349a3); `gcg-benchmark` skill drafted.
- `RULES_TRACEABILITY.md` generated from the full JUnit run: 395 tested, 184 N/A, 0 missing.

## Phase 6 — Verification and adversarial review (pending)
