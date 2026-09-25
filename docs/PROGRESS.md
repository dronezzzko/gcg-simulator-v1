# Progress log

Machine-readable status: `docs/status.json`. This log survives context compaction; each phase
records what was done, evidence, and what remains.

## Phase 0 — Preflight and scaffold (done)
- Preflight checks recorded in `docs/ASSUMPTIONS.md`. Branch `feat/gcg-sim`.
- uv package scaffolded (`uv init --package`, Python 3.12, ruff, mypy strict, pytest, hypothesis).

## Phase 1 — Research (done; commit e240669)
- Data ingestion profile, conflicts subsystem (291 conflicts, all resolved), rules index
  (579 entries, 399 testable), official sources (B&R 2026-09-25, floor/BO3/deck rules),
  effect-template clustering and 19 work packages. Outputs under `docs/research/`, `data/`.

## Phase 2 — Foundations (in progress)
- Engine core, DSL, compiler (645/840 non-vanilla cards compile), bindings API, testkit,
  golden files, coverage gates, determinization.
- Pending: rule-tagged test suites (Phase 2b workflow), engine bug fixes they surface.

## Phase 3 — Card effects (pending)
## Phase 4 — AI, runner, CLI, reports (pending)
## Phase 5 — Skills and docs (pending)
## Phase 6 — Verification and adversarial review (pending)
