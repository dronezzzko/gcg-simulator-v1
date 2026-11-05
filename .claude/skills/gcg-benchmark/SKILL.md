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

`gcg-sim` is the Rust binary `target/release/gcg-sim`. Build it once from the repository root
(after `cargo fetch --locked` on a fresh machine):

```bash
cargo build --release --locked --offline -p gcg-cli
```

## 1. Validate both decks first

```bash
target/release/gcg-sim validate examples/decks/red-green-zeon.txt
target/release/gcg-sim validate examples/decks/blue-white-federation.txt
```

Deck file format (details: `docs/DECK_FORMAT.md`): one entry per line, `<count> <card_number> [name]`,
`#` comment lines, RESOURCE cards form the 10-card resource deck. Fix every violation before
running; the benchmark refuses illegal decks (exit code 1 with one line per violation).

## 2. Run a benchmark

```bash
target/release/gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt \
    --matches 2 --seed 1 --workers auto --ai-preset fast --out /tmp/gcg-benchmark-demo
```

For a real measurement use your own deck files, `--matches 100` or more and `--workers auto`
(one worker per performance-core CPU; the efficiency cores would only lengthen the tail), and
keep each run's `--out` directory (e.g. `runs/<dut>-vs-<benchmark>-<preset>-s1`).

- `--ai-preset fast` screens: it trades search depth for throughput, for rapidly screening deck
  variants, sub-variants and hypothetical builds. `--ai-preset standard` (the default) polishes:
  it uses the full search so that each player plays at the level of the strongest top-tier
  competitive player, for refining and polishing the final deck. No other value is accepted.
- `--format bo3` (default) plays best-of-three matches per the official BO3 rules; `--format bo1`
  plays single games. A game costs the same in either format, but a BO3 match averages about 2.5
  games, so BO1 gives about 2.5x more match results per minute, each from an independent game;
  use it for screening.
- Same `--seed` ⇒ byte-identical `results.json`, `games.ndjson` and replays regardless of
  `--workers`.
- Measured cost on an Apple M2 Pro (`docs/dev/BENCHMARKS.md`, Part 2; red-green-zeon against
  blue-white-federation, BO3, 6 workers): 100 matches took 82.1 s with `fast`; 20 matches took
  148.6 s with `standard`. `standard`'s budget is about 10.7 times `fast`'s (1024 / 2560 /
  10240 against 96 / 240 / 960 iterations per option / minimum / maximum, `docs/AI.md`); with
  the M15 mechanisms it executed 128.5G instructions per game on this workload against `fast`'s
  15.5G (`docs/dev/eval/M15/README.md`, "Cost of each scale").

## 3. Read the report

Files in `--out` (full guide: `docs/REPORTS.md`):

| File | Use |
| --- | --- |
| `summary.md` | Human summary: win rates with 95% Wilson intervals, splits, per-card table, hypotheses, and a search line (preset, mapping, budget, mechanisms, weights). |
| `results.json` | Machine-readable results, validated against results schema 1.2.0 before it is written. |
| `games.ndjson` | One line per game: seeds, seats, first player, winner, end reason, length. |
| `replays/*.json` | Representative wins/losses (`win-m0001-g1.json`, …); `target/release/gcg-sim replay FILE --log` re-simulates one and prints its turns. |
| `timing.json` | Wall-clock and CPU tiers only (kept separate so results stay reproducible). |

Schema 1.2.0 is schema 1.1.0 plus the AI fields of the strengthened search:

- `ai.preset`: `fast` or `standard`; `ai.dut_agent` and `ai.bench_agent`: `search-fast` or
  `search-standard`; `ai.factory`.
- `ai.search`: `algorithm` (`gcg-search`), `preset`, `mapping` (`reference standard` for
  `fast`, `reference strong` for `standard`), `budget` (`iterations_per_option`,
  `min_iterations`, `max_iterations`, `aux_apply_cap`), `mechanisms` (the enabled mechanism
  flags, sorted) and `weights` (the evaluation weights' identity).
- `ai.effort`: one object per seat with `games`, `decisions`, `searches`, `iterations`,
  `applies_tree`, `applies_rollout`, `applies_aux`, `worlds`, `rollouts_capped`, and the
  per-game distributions `iterations_per_game` and `applies_per_game`. These are deterministic
  counts of the search's work; a non-zero `rollouts_capped` means some rollouts hit the
  250-decision cap.
- `versions.code_digest` identifies the build; `versions.results_schema` is `1.2.0`.

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

## 4. Iterate on deck changes: screen with `fast`, polish with `standard`

Use the same benchmark deck, preset, and a fixed seed schedule. Change one thing at a time.

Sample sizes (95% confidence, win rate near 50%; formulas in `references/statistics.md`):

| Goal | Games needed |
| --- | --- |
| Win rate ±10 points | ~100 games |
| Win rate ±5 points | ~385 games |
| Detect a 10-point difference between two variants (80% power) | ~390 games per variant |
| Detect a 5-point difference between two variants | ~1,570 games per variant |

A BO3 match yields ~2.5 games; the report gives both match-level and game-level rates.

The two-stage workflow for many variants (sub-variants, card swaps, hypothetical builds):

1. **Screen** every variant with `--ai-preset fast --format bo1`, the same benchmark deck and
   the same seed schedule, a few hundred games each (BO1: one game per match). Discard the
   variants that are clearly worse; keep every variant whose interval overlaps the best one.
2. **Polish** the survivors, and the current list as the baseline, with `--ai-preset standard`
   (BO3, the default) on **different seeds** from the screen (`--seed 2`, `--seed 3`, …), with
   enough games to resolve the differences you care about (table above).
3. **Never compare or pool across presets**: a `fast` rate and a `standard` rate are different
   measurements. Pool only runs with identical settings (same preset, format and
   `versions.code_digest`).

Reports with results schema 1.0.0 or 1.1.0 (`gcg-eval reference-benchmark`, or earlier
versions of the tool) are not comparable with `gcg-sim` reports: the reference search (1.1.0)
or the AI as it was before the October 2026 update (1.0.0) played them, different players
(`docs/AI.md` has the measured comparison), and the reference `fast` preset is a smaller budget
than `gcg-sim`'s. A report from another `versions.code_digest` may also play
differently. Re-run the baseline list with the current build before comparing.

Workflow for a card swap:
1. Baseline: run the current list (e.g. 400 BO1 games with `fast`) and record the game win
   rate interval.
2. Variant: edit the deck file (keep it legal), validate, run with the same settings.
3. Compare: if the intervals overlap heavily, the change is not distinguishable at this size —
   either run more games or keep the simpler list.
4. Check per-card stats of the swapped cards (drawn/played counts, win rate when played).
5. Confirm the winner against the baseline with `standard` on new seeds.
6. Record the decision, presets, seeds, and report paths.

For a paired comparison of two same-seed runs (the same deals, game by game), use
`target/release/gcg-eval compare-runs`; `target/release/gcg-eval benchmark --analysis` adds
stratified and clustered statistics in `analysis.json`.

## 5. Limitations to keep in mind

- The AI is strong but not perfect; decks whose power depends on long, precise lines may be
  underrated. Check a few replays of losses for implausible plays before trusting a surprising
  result (`docs/AI.md` and `docs/dev/WEAKNESSES.md` list the known weaknesses). Rerun a few
  matches with `--decision-log` to see the values and visit counts of the alternatives the AI
  considered at each decision.
- Measured strength of the presets is only what `docs/AI.md` reports.
- Results reflect the implemented card behaviour and conflict resolutions (`docs/CONFLICTS.md`),
  the current B&R list, and no sideboarding or clock.

References: `references/statistics.md` (formulas used for the intervals and sample sizes).
