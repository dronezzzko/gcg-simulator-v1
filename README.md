# gcg-sim — Gundam Card Game deck benchmark simulator

`gcg-sim` measures how a Gundam Card Game (English) deck performs against a benchmark deck. It
plays many matches between two search-based AI players under a deterministic implementation of
the Comprehensive Rules (Ver. 1.9.0, 2026-09-11). It then writes reproducible matchup reports
with honest error bars, which you can use to tune the deck.

- Every card in the packaged card data is implemented (1148/1148 card numbers), and each
  non-vanilla card has a behaviour test.
- Every 1v1 rule is covered by a tagged test or has a written N/A reason
  ([`docs/RULES_TRACEABILITY.md`](docs/RULES_TRACEABILITY.md)).
- Everything runs offline from data packaged with the tool.

## Install

Requires [uv](https://docs.astral.sh/uv/) and Python 3.12 or later (uv installs it if needed).

```bash
uv sync --locked
uv run gcg-sim --help
```

To use it outside the repository, build a wheel and install it into any environment. The wheel
contains the card data, rules and B&R list, so it needs no network access:

```bash
uv build
```

Then run `pip install dist/gcg_sim-0.1.0-py3-none-any.whl` in the target environment
(`scripts/verify_wheel_offline.sh` does this in a fresh venv with networking disabled).

## Usage

Check that a deck is legal:

```bash
uv run gcg-sim validate examples/decks/red-green-zeon.txt
```

Benchmark a deck under test against a benchmark deck:

```bash
uv run gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt \
    --matches 2 --seed 1 --workers 2 --out /tmp/gcg-sim-demo
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--matches N` | required | Number of matches. Use hundreds for decisions. |
| `--seed S` | 0 | Master seed; the same seed gives byte-identical `results.json` whatever `--workers` is. |
| `--workers W` | 1 | Worker processes. |
| `--format bo3\|bo1` | `bo3` | Best-of-three (official BO3 match rules, no sideboarding, no clock) or single games. |
| `--ai-preset standard\|strong` | `standard` | AI search budget ([`docs/AI.md`](docs/AI.md)). |
| `--out DIR` | `gcg-sim-out` | Report directory. |
| `--decision-log` | off | Record the AI's per-decision search statistics. |
| `--replays K` | 3 | Representative wins and losses to save. |

Other commands:

```bash
uv run gcg-sim data status
uv run gcg-sim replay "$(ls /tmp/gcg-sim-demo/replays/*.json | head -n 1)"
```

`data status` prints the packaged data, rules, B&R versions, implementation coverage and
conflict counts. `replay` re-simulates a saved game from its seeds and verifies its outcome.
Errors print a clear message and exit non-zero: 1 for an invalid deck, data or replay, 2 for a
usage error, 130 for an interrupt.

## Deck files

```text
# comment lines start with "#"
4 GD01-008 Guntank          <- <count> <card_number> [name]
2 GD01-001_p1               <- alternate-art product ids normalize to the card number
10 R-001                    <- RESOURCE cards form the 10-card resource deck
```

Validation checks main and resource deck sizes, the 4-copy limit, one or two colors, card
types, the banned/restricted list with banned pairs, unknown ids, and names. Every violation is
reported with a specific message. Full format: [`docs/DECK_FORMAT.md`](docs/DECK_FORMAT.md).
Three legal example decks are in [`examples/decks/`](examples/decks/).

## Reports

A benchmark writes these files to `--out`:

| File | Content |
| --- | --- |
| `summary.md` | Match and game win rates with 95% Wilson intervals, on-play/on-draw and game 1/2/3 splits, game length and end reasons, per-card statistics, benchmark cards most associated with losses, tuning hypotheses with their sample sizes, versions, seeds, AI settings and the conflict resolutions that affect either deck. |
| `results.json` | The same data, validated against a versioned JSON Schema. |
| `games.ndjson` | One record per game. |
| `replays/*.json` | Representative games, replayable with `gcg-sim replay`. |
| `timing.json` | Wall-clock information, kept separate so the other files stay reproducible. |

Read [`docs/REPORTS.md`](docs/REPORTS.md) for every field and for how to interpret the numbers.
The [`gcg-benchmark`](.claude/skills/gcg-benchmark/SKILL.md) Claude Code skill walks through a
tuning workflow with adequate sample sizes.

## How it works

- **Engine.** A deterministic state machine with cheap cloning, legal-action generation and
  JSON serialization.
- **Card effects.** Card text is compiled into a typed effect DSL. Irregular cards use per-card
  bindings.
- **AI.** Information-set Monte Carlo tree search (determinized, with fixed node budgets) plus
  random and greedy baselines.
- **Runner.** A process pool that derives every seed from the master seed.

Details: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/AI.md`](docs/AI.md).

Where sources disagree or card text is ambiguous, the decision is recorded:
[`docs/CONFLICTS.md`](docs/CONFLICTS.md) lists every conflict and its resolution (reviewable in
`src/gcg_sim/data/overrides.json`), and [`docs/ASSUMPTIONS.md`](docs/ASSUMPTIONS.md) lists the
engine's interpretation decisions.

## Sources and versions

| Source | Version |
| --- | --- |
| Comprehensive Rules | Ver. 1.9.0, updated 2026-09-11 |
| Card data | [gcg-api](https://github.com/yzRobo/gcg-api) data files at commit `f57b7c0`, dataset `25-676f1bf`, built 2026-09-21 |
| Banned & restricted list | effective 2026-09-25 |
| BO3 match rules, deck construction, floor rules | gundam-gcg.com/en pages retrieved 2026-09-25 |

Every source's URL, version, retrieval time and SHA-256 is locked in `data/SOURCES.lock.json` and
summarized in [`docs/SOURCES.md`](docs/SOURCES.md). Cached source files are never edited. To
update them, use the [`gcg-refresh-data`](.claude/skills/gcg-refresh-data/SKILL.md) skill: it
diffs a new snapshot, flags new or changed cards, and runs the full verification gate.

## Development

```bash
uv sync --locked
uv run ruff check .
uv run ruff format --check .
uv run mypy
uv run pytest -n 8
```

`scripts/verify_all.sh` runs the whole gate on a clean clone: lint, types, the full test suite
including the slow robustness and AI-strength tests, `uv build`, and an offline wheel benchmark.

## Licences and notices

- Contains data from gcg-api (https://gcgapi.com), made available under the Open Database
  License (ODbL) v1.0; see `src/gcg_sim/data/gcgapi/LICENSE-DATA`. The packaged card database,
  including the resolutions in `src/gcg_sim/data/overrides.json`, is a derivative database
  offered under the ODbL v1.0.
- Gundam Card Game card names, card text and trademarks belong to BANDAI and SOTSU·SUNRISE.
  This is an unofficial fan project. It is not affiliated with or endorsed by Bandai. It
  contains no card images.

## Limitations

- **1v1 only.** Multiplayer (rules section 12) is out of scope; its rules are marked N/A.
- **No sideboarding or clock** in BO3, as the task specifies. Games that reach the engine's
  safety cap (200 turns or 20,000 decisions) are reported as `turn_limit` draws.
- **The AI is strong but not perfect.** Decks that need long, precise lines may be underrated.
  Check replays before trusting a surprising result; [`docs/AI.md`](docs/AI.md) lists known
  weaknesses.
- **Card behaviour follows the packaged data and the resolutions in `docs/CONFLICTS.md`.** Where
  no official ruling exists, the most defensible reading was chosen and recorded there.
- **Two official deck-building exceptions are not applied**: unmodified ST02/ST05 starter decks.
  The validator cannot tell that a deck is an unmodified starter deck, so it reports them as
  violations and says so.
