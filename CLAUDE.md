# gcg-sim

A deterministic Gundam Card Game (Comprehensive Rules Ver. 1.9.0) engine, AI and deck benchmark
CLI. Start with `README.md`. Architecture: `docs/ARCHITECTURE.md`.

## Commands

```bash
uv sync --locked
uv run ruff check . && uv run ruff format --check . && uv run mypy
uv run pytest -n 8
uv run python -m gcg_sim.tools.explain GD01-001
uv run python -m gcg_sim.tools.golden --check
uv run python -m gcg_sim.tools.fuzz --games 500 --workers 8
uv run python -m gcg_sim.sources.conflicts --check
uv run python -m gcg_sim.tools.sources --check
```

- `explain` shows a card's text, compiled script, rulings and applied resolutions.
- After a deliberate behaviour change, review the golden diff, then run
  `uv run python -m gcg_sim.tools.golden --write`.
- After changing rule tags, regenerate the traceability doc:

  ```bash
  uv run pytest -n 8 --junitxml=build/junit.xml
  uv run python -m gcg_sim.tools.traceability --junit build/junit.xml
  ```
- `scripts/verify_all.sh` runs the full acceptance gate on a clean clone.

## Rules of the codebase

- **Determinism.** All randomness goes through `gcg_sim.rng` (`SplitMix64`, `derive_seed`). No
  wall-clock, `random`, or `hash()` of strings in game logic. Iterate in explicit orders.
- **Cached sources are read-only.** Never hand-edit `src/gcg_sim/data/gcgapi/*`,
  `data/official_raw/*` or the Comprehensive Rules markdown
  (`src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md` and its repo-root copy);
  only `/gcg-refresh-data` replaces them, whole. The derived files `src/gcg_sim/data/official/*.json`
  and `src/gcg_sim/data/rules/rules_na.json` are curated by hand with verbatim quotes, and
  `rules_index.json` is generated (`python -m gcg_sim.rules.index --write`); re-hash with
  `python -m gcg_sim.tools.sources --write` after changing any of them. Resolve source
  disagreements in `src/gcg_sim/data/overrides.json` (plus `curated_conflicts.json` with
  verified quotes), then regenerate `docs/CONFLICTS.md` with `python -m gcg_sim.sources.conflicts`.
- **Card effects.** The text compiler (`effects/compiler/`) handles common wording. Per-card
  bindings in `effects/bindings/*.py` (`@card("GD01-001")`, auto-discovered) handle the rest, and
  a binding wins over the compiler. Prefer a general compiler template or a DSL/engine feature
  over a card-specific hook.
- **Tests carry traceability tags.** Use `@pytest.mark.rule("8-5-2")`, `@pytest.mark.card(...)`,
  `@pytest.mark.faq("Q12")` and `@pytest.mark.ruling("GD01-001:Q100")`. Every card, every
  testable rule, and every FAQ entry and ruling needs a tagged passing test or a written N/A
  reason (`tests/meta/`, `src/gcg_sim/data/rules/rules_na.json`).
- **No gaps in `src/`.** No TODO, FIXME, `NotImplementedError` or placeholder bodies
  (`tests/test_no_placeholders.py`). Never skip, weaken or xfail a test to get green.
- Record interpretation decisions in `docs/ASSUMPTIONS.md`, and in `docs/CONFLICTS.md` when
  sources disagree.

## Skills

- `/gcg-benchmark`: benchmark and tune a deck, and interpret its reports.
- `/gcg-refresh-data`: refresh the card data, rules and B&R snapshot; implement new cards; run
  the verification gate. It is the only workflow that uses the network.
