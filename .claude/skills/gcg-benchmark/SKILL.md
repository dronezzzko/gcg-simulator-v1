---
name: gcg-benchmark
description: Benchmark a Gundam Card Game deck against a benchmark (meta) deck with gcg-sim, read the matchup reports, and iterate on deck changes with statistically adequate sample sizes. Use when asked to evaluate, compare, or tune a GCG deck, to test a card swap, or to interpret results.json/summary.md from a gcg-sim run.
argument-hint: "<deck-under-test.txt> <benchmark-deck.txt> [--matches N] [--seed S]"
---

# Benchmarking and tuning a deck with gcg-sim

gcg-sim plays the deck under test (DUT) against a benchmark deck for N matches between two
search-based AI players under the official rules (Comprehensive Rules Ver. 1.9.0, current B&R
list), then writes reproducible reports. Treat every conclusion as a measurement with an error
bar, not a verdict.

## 1. Validate both decks first

```bash
uv run gcg-sim validate examples/decks/red-green-zeon.txt
uv run gcg-sim validate examples/decks/blue-white-federation.txt
```

Deck file format (details: `docs/DECK_FORMAT.md`): one entry per line, `<count> <card_number> [name]`,
`#` comment lines, RESOURCE cards form the 10-card resource deck. Fix every violation before
running; the benchmark refuses illegal decks (exit code 1 with one line per violation).

## 2. Run a benchmark

```bash
uv run gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt \
    --matches 2 --seed 1 --workers 2 --out /tmp/gcg-benchmark-demo
```

For a real measurement use your own deck files, `--matches 100` or more, and `--workers` equal
to your core count, and keep each run's `--out` directory (e.g. `runs/<dut>-vs-<benchmark>-s1`).

- `--format bo3` (default) plays best-of-three matches per the official BO3 rules; `--format bo1`
  plays single games. A game costs the same in either format, but a BO3 match averages about 2.5
  games, so BO1 gives about 2.5× more match results per minute, each from an independent game;
  use it for quick screening.
- `--ai-preset standard` (default) or `strong` (slower, stronger). Compare variants only under the
  same preset.
- Same `--seed` ⇒ byte-identical `results.json` regardless of `--workers`.
- Throughput and preset costs are measured in `docs/AI.md`.

## 3. Read the report

Files in `--out` (full guide: `docs/REPORTS.md`):

| File | Use |
| --- | --- |
| `summary.md` | Human summary: win rates with 95% Wilson intervals, splits, per-card table, hypotheses. |
| `results.json` | Machine-readable results (validated by the published JSON Schema). |
| `games.ndjson` | One line per game: seeds, seats, first player, winner, end reason, length. |
| `replays/*.json` | Representative wins/losses (`win-m0003-g2.json`, …); `uv run gcg-sim replay FILE --log` re-simulates one and prints its turns. |
| `timing.json` | Wall-clock only (kept separate so results stay reproducible). |

Read in this order:
1. **Match win rate and its 95% Wilson interval.** If the interval crosses 50%, the matchup is
   not resolved at this sample size.
2. **On-play vs on-draw and game-1/2/3 splits.** Large play/draw gaps point to tempo problems.
3. **How games end and game length.** Losing to battle damage on turn 6–7 vs late decking are
   different problems.
4. **Per-card stats** (drawn, played, turn played, win rate when drawn / when played,
   held-but-never-played, mulligan rate). A card with high "held but never played" is a
   candidate cut; a card with a high "win rate when played" relative to baseline is a candidate
   to add copies of — but check its sample size first.
5. **Benchmark cards most associated with losses** — what you need answers for.
6. **Tuning signals** are *hypotheses*, each with a sample size. Never act on a signal backed
   by fewer than ~50 observations.
7. **Applied conflict resolutions** listed in the report: if a card you depend on has a
   resolution (ruling/errata/ambiguity), read `docs/CONFLICTS.md` before trusting its numbers.

## 4. Iterate on deck changes

Use the same benchmark deck, preset, and a fixed seed schedule. Change one thing at a time.

Sample sizes (95% confidence, win rate near 50%):

| Goal | Games needed |
| --- | --- |
| Win rate ±10 points | ~100 games |
| Win rate ±5 points | ~385 games |
| Detect a 10-point difference between two variants (80% power) | ~390 games per variant |
| Detect a 5-point difference between two variants | ~1,570 games per variant |

A BO3 match yields ~2.5 games; the report gives both match-level and game-level rates. Screen
variants with `--format bo1` and several hundred games, then confirm the winner with BO3 at the
same sample size. Use different seeds for the confirmation run so you do not overfit one
schedule (`--seed 1`, `--seed 2`, …), and pool results only across runs with identical settings.

Workflow for a card swap:
1. Baseline: run the current list (e.g. 400 BO1 games) and record the game win rate interval.
2. Variant: edit the deck file (keep it legal), validate, run with the same settings.
3. Compare: if the intervals overlap heavily, the change is not distinguishable at this size —
   either run more games or keep the simpler list.
4. Check per-card stats of the swapped cards (drawn/played counts, win rate when played).
5. Record the decision, seeds, and report paths.

## 5. Limitations to keep in mind

- The AI is strong but not perfect; decks whose power depends on long, precise lines may be
  underrated. Check a few replays of losses for implausible plays before trusting a surprising
  result (`docs/AI.md` lists known weaknesses). Rerun a few matches with `--decision-log` to see
  the values and visit counts of the alternatives the AI considered at each decision.
- Results reflect the implemented card behaviour and conflict resolutions (`docs/CONFLICTS.md`),
  the current B&R list, and no sideboarding or clock.

References: `references/statistics.md` (formulas used for the intervals and sample sizes).
