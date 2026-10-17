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

## Phase 4 — AI, runner, CLI, reports (done; merges 5174e95, AI branch 81a45d9)
- Deck/runner/reports/CLI package: deck parser and validator with every violation class,
  3 legal example decks, seeded BO3/BO1 runner with a process pool, replayable game records,
  schema-validated reports, `gcg-sim` CLI (`benchmark`, `validate`, `data status`, `replay`).
- AI package: random, greedy and determinized ISMCTS agents with `standard`/`strong`
  presets, tuned evaluation, decision log, tactical puzzles, information-set tests.
- Integration: the mulligan puzzles now use a legal deck (the engine validates decks); a
  coverage test checks that every modal effect offers every printed mode; dominated-move
  pruning (free add-to-hand Bursts, pointless 0-AP attacks) from the replay review.
- Measured on the integrated engine (15ae2eb): AI vs random 400/400, vs greedy 289/400
  (Wilson 95% [0.677, 0.764], draws counted as non-wins, 0 draws); 100 BO3 matches of the example decks take about 6–11 minutes on 8 cores.

## Phase 5 — Skills and docs (done)
- `gcg-refresh-data` and `gcg-benchmark` skills (`claude plugin validate --strict .claude`
  passes), README, CLAUDE.md, docs (ARCHITECTURE, RULES_TRACEABILITY, AI, REPORTS,
  DECK_FORMAT, SOURCES, CONFLICTS, ASSUMPTIONS), `scripts/verify_all.sh`,
  `scripts/verify_wheel_offline.sh`, `scripts/check_doc_commands.py` (18 blocks pass, 1 skipped
  with a reason: it needs a network clone).

## Phase 6 — Verification and adversarial review (done)
- Review 1 (63 agents): stratified rules sample (54 rules), 23 N/A reasons, 64 cards; 14
  findings confirmed by at least 2 of 3 skeptics and fixed with regression tests (e911333,
  aa0dd30), 2 refuted.
- Review 2 (25 agents): 12 AI-vs-AI replays audited move by move; no rules or card
  deviations; 7 AI-quality findings (info): dominated moves (free Bursts, 0-AP attacks) and
  plays without effect (AP-3 on a Unit that cannot battle, resting rested Units, paying a
  cost for an effect that does nothing) are now pruned with tests; choosing the weaker of
  two targets is documented in docs/AI.md as a remaining AI weakness.
- Review 3 (105 agents): deliverables vs the request through three lenses; 28 findings
  confirmed (all minor or info), 6 refuted; all 28 fixed (ef05730, 32e9f84).
- Recheck (25 fresh agents, one per finding): 23 resolved, 2 open (cost-only activations
  still played while other Units could attack; one docstring) — both fixed in 15ae2eb, along
  with the small follow-ups the recheck listed.
- Seat-symmetry checks: 4000 random-agent mirror games (first-player win rate 0.551 seat 0,
  0.539 seat 1); AI mirror 96 BO1 games, seat 0 won 50.
- Refresh-skill dry run (criterion 9), 2026-09-25 on commit ef05730, network reads only:
  - `git status --short` before: empty; `tools.sources --check`: the lock verifies.
  - `fetch_gcgapi.sh`: `GCGAPI_COMMIT=f57b7c0b0ebc4c13d359649de19750c793ecbefc` (the pin).
  - `fetch_official.py fetch`: 36/36 pages; `compare`: 30 unchanged (bytes), 2 unchanged
    (text), 2 not cached (news listing pages 2–3, expected), 2 CHANGED (`en_news_index`,
    `en_top`: new PRODUCTS entries for ST11–ST14 and EVX08/09 only); `rules: up to date`
    (Ver. 1.9.0, 2026-09-11); B&R links unchanged (US July 24, Asia July 25).
  - `news`: 6 uncached entries, all PRODUCTS; none relevant to rules, errata or B&R.
  - `refresh diff`: 0 card, ruling, FAQ, errata, product or set changes (the known ST12-001
    erratum stays covered by `errata-unapplied:ST12-001:news-02_193`); `apply --dry-run`:
    "no changes: the packaged gcg-api snapshot and its lock entries are current".
  - `refresh banlist`: banlist OK; `official_normalize.py check`: 53 quotes, 0 failures;
    `refresh coverage`: 1148 card numbers, nothing unimplemented, changed or untested.
  - `git status --short` after: empty.


## Phase 7 — AI quality and the `fast` preset (in progress, branch `feat/ai-quality`)

Goal: measured AI improvements against every known weakness in `docs/AI.md`, a `fast`
screening preset, output-identical engine speedups. Proxies for "closer to skilled human play":
head-to-head against the pre-change AI, tactical puzzles, blinded replay audits, budget scaling.

- Baseline on `main` (bc38ab4): ruff, format, mypy clean; regular suite 3754 passed, 1 failed:
  `tests/runner/test_pool.py::test_interrupt_terminates_workers_and_returns_nothing` is flaky
  (5 failures in 80 isolated runs). Root cause: the caller and the executor's manager thread
  both joined the terminated workers and raced for `waitpid`; fixed in daf73c5 (0 in 80).
- A/B harness (c27e523): `make_agent(..., config=, weights=)`, `AgentSpec.config/weights`,
  `gcg_sim.ai.baseline` (`baseline-standard` replays a game recorded on bc38ab4, decision logs
  included: `tests/ai/test_baseline.py`; 24 mirrored games vs greedy on seed 31337 are
  identical to `main`'s), `python -m gcg_sim.ai.compare A B --games N --seed S`.
- Seed plan: investigations 20000–29999; implementation screening 30000–39999; `strong` vs
  `standard` 8401 (baseline and final, same deals); held-out final confirmations 90001+
  (run once each). Raw outputs under `build/aiq2/` (gitignored).
