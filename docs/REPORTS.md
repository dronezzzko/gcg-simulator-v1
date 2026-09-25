# Benchmarks and reports

## Running a benchmark

```bash
uv run gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt \
    --matches 2 --seed 1 --workers 2 --out /tmp/gcg-sim-demo
```

| Option | Default | Meaning |
| --- | --- | --- |
| `DECK_UNDER_TEST BENCHMARK_DECK` | required | Deck files (see `docs/DECK_FORMAT.md`). Both must be legal. |
| `--matches N` | required | Number of matches. Use hundreds for decisions; the demo above only shows the output. |
| `--seed S` | 0 | Master seed. Every game and agent seed is derived from it. |
| `--workers W` | 1 | Worker processes. The results do not depend on it. |
| `--format bo3\|bo1` | bo3 | Best-of-three matches or single games. |
| `--ai-preset standard\|strong` | standard | Search budget of the AI that plays both seats (see `docs/AI.md`). |
| `--out DIR` | `gcg-sim-out` | Report directory. It is created if needed, and existing report files are replaced. |
| `--decision-log` | off | Record the AI's decision log in `games.ndjson` and in the replays. This makes them large. |
| `--replays K` | 3 | Representative wins and losses to save, K of each, plus one draw if any. |

Progress goes to stderr. When the run finishes, the command prints the match and game win rates
of the deck under test (DUT) and the report paths. Exit codes: 0 success, 1 invalid deck, data,
or replay, or a failed simulation, 2 usage error, 130 interrupted.

**Ctrl-C** stops the worker processes, writes no reports, and exits with 130. Each report file is
written to a temporary name and then renamed. `results.json` is written last, so a report file
is either complete or absent.

### Match procedure

- **Seats.** In match `m` the deck under test sits in seat `m % 2`.
- **Seeds.** Each game's seed is `derive_seed(master, "match", m, "game", g)`. Each agent's seed
  is `derive_seed(master, "match", m, "game", g, "agent", seat)`. Randomness uses SplitMix64.
- **BO3 (official BO3 Match Rules).** The first player to 2 game wins takes the match.
  - In game 1, a seeded die roll picks who decides Player One (rule 6-2-1-4, FAQ Q9).
  - In games 2 and 3, the loser of the previous game decides.
- **Simultaneous defeat** (`both_defeated`) is a draw in the game statistics. For BO3 match
  scoring, the turn player at the end loses that game (TRM 5.2).
- **Turn limit.** A game that reaches the engine's safety cap (200 turns or 20,000 decisions)
  ends as a `turn_limit` draw. No one scores a match point, and the next game's chooser is again
  decided by the seeded die roll. These games are always counted and reported.
- **Match draws.** A BO3 match can end as a draw only when turn-limit games leave the score
  level after three games.
- **BO1.** Each match is one game, and a drawn game is a drawn match.

## Output files

| File | Content | Reproducible |
| --- | --- | --- |
| `results.json` | All statistics. It is validated against the packaged JSON Schema. | byte-identical for the same decks, seed, options and versions, whatever `--workers` is |
| `summary.md` | A readable view of `results.json` with a reproduce command. | yes |
| `games.ndjson` | One `GameRecord` per line, in match and game order. | yes |
| `replays/*.json` | Self-contained replays of representative games. | yes |
| `timing.json` | Wall-clock times, workers, Python version and platform. | no; it is the only such file |

### results.json

- **Schema.** The schema is `src/gcg_sim/reports/schema/results.v1.schema.json`, with `$id`
  `urn:gcg-sim:schema:results:1.0.0`. Every document repeats the id in `schema` and the version
  in `schema_version`. Load the schema with `gcg_sim.reports.load_schema()` and check a document
  with `gcg_sim.reports.validate_results(obj)`.
- **Win rates.** All rates are from the DUT's point of view. A rate block is
  `{"successes", "n", "rate", "ci95"}`. `ci95` is a 95% Wilson score interval, and `rate` and
  `ci95` are `null` when `n` is 0. Draws count as non-wins and are reported separately.

| Key | Content |
| --- | --- |
| `versions` | Package version, `code_digest` (SHA-256 over every file of the installed package, so reports from different builds are distinguishable), results schema, gcg-api `dataset_version`, the dataset manifest's `data_source_commit` (the upstream card-data source; the gcg-api repository commit is pinned in `data/SOURCES.lock.json`) and build time, Comprehensive Rules version and date, B&R effective date, deck-rules and BO3-rules dates. |
| `config`, `seeds`, `ai` | Match count, format, turn and decision caps, replays per result; the master seed and derivation formulas; the agent factory, preset, agent names and decision-log flag. |
| `decks` | For each deck: name, sha256 digest of the card counts, colors, and the main and resource listings. |
| `results` | Match and game outcomes: `n`, `dut_wins`, `bench_wins`, `draws`, and `dut_win_rate`. |
| `splits` | Outcomes on the play and on the draw, and by game number (`"1"`, `"2"`, `"3"`). |
| `game_length`, `end_reasons`, `draws` | Turn-count distribution overall and by DUT result, a histogram, outcomes by end reason, and draw counts (simultaneous defeats, turn limits, and games scored by the TRM 5.2 rule). Every distribution block (here and in `cards[].played.own_turn`) uses integer nearest-rank statistics: `median` is the ⌈n/2⌉-th smallest value (the lower middle value when n is even), `p10`/`p90` the ⌈0.1n⌉-th/⌈0.9n⌉-th smallest. |
| `mulligan` | DUT redraw rate and win rate after redrawing or keeping; benchmark redraw rate. |
| `cards` | Per-card statistics for the DUT main deck (see below). |
| `benchmark_cards` | For each benchmark card: games in which it was seen in play, the DUT loss rate when it was seen and when it was not, and the difference (`loss_rate_lift`). Cards seen and unseen in at least `hypothesis_thresholds.min_games` games (`rank_eligible`) come first, ordered by lift. These are the benchmark cards most associated with DUT losses. |
| `hypotheses` | Tuning signals. Each has `"label": "hypothesis"`, a statement, its `sample_size`, and the evidence behind it. A comparison becomes a hypothesis only with `hypothesis_thresholds.min_games` games on each side and a two-proportion z-test significant at `alpha` after a Holm correction over every comparison of its kind (`evidence.p_value`, `evidence.comparisons`), so pure noise yields no signals. |
| `conflict_resolutions` | Resolutions from `src/gcg_sim/data/overrides.json` that touch a card in either deck: card-data overrides (`changes_card_data: true`) and interpretations whose conflict names the card. |
| `replays` | The saved replay files and the games they hold. |
| `notes` | Caveats that apply to every run. |

### Per-card statistics (`cards[]`)

The runner watches the real game at every decision, never the AI's search copies. Plays come
from the actions taken. Draws come from the hand and the engine's per-turn event log.

| Field | Meaning |
| --- | --- |
| `drawn` | Games in which at least one copy entered the hand after the redraw decision, counting the kept opening hand, draws, searches and Burst "add to hand". Also the copies drawn in total and per game. |
| `opening_hand` | Games in which the card was in the kept 5-card hand. |
| `played` | Games in which the card was played from the hand, the number of plays, the play rate among games where it was drawn, and the distribution of the DUT's own turn number when it was played. A play is deploying a Unit or Base, activating a Command, or pairing a Pilot or a pilot-capable Command. |
| `win_rate_when_drawn`, `win_rate_when_not_drawn`, `win_rate_when_played`, `win_rate_when_drawn_not_played` | Game win rates for each condition. |
| `held_never_played` | Drawn copies that were never played, their share of drawn copies, and how many were still in hand at the end. |
| `mulligan` | Games in which the card was in the initial 5 cards, and how often that hand was redrawn. |

These are correlations inside AI-vs-AI games. A card drawn in longer games looks better or
worse for reasons that have nothing to do with the card. Treat `hypotheses` as a list of what to
test next, with its sample sizes, not as conclusions.

### Replays

Each replay is self-contained. It holds both decklists, the game seed, the chooser, and the
action list. It also holds the expected winner, end reason, turn count and a canonical
final-state sha256, and a readable log. To re-simulate a replay and check that it reaches the
same final state:

```bash
uv run gcg-sim replay "$(ls /tmp/gcg-sim-demo/replays/*.json | head -n 1)" --log
```

The command exits 0 when the replay matches and 1 when it does not. The final-state digest
replaces the engine's process-local registry numbers with content ids, so it is the same in
every process. Every `GameRecord` in `games.ndjson` can be replayed the same way with
`gcg_sim.runner.replay(record, config)`.

## Data status

```bash
uv run gcg-sim data status
uv run gcg-sim data status --json
```

This prints the card-data snapshot (`dataset_version`, build time, counts), the Comprehensive
Rules version and date, the B&R list date and contents, per-card-type implementation coverage
from the effect registry, and the number of conflict resolutions and card-data overrides.

## Python API

```python
from pathlib import Path
from gcg_sim.deck import load_deck
from gcg_sim.runner import BenchmarkConfig, run_benchmark
from gcg_sim.reports import write_reports

if __name__ == "__main__":  # required: workers are started with the "spawn" method
    config = BenchmarkConfig(
        deck_under_test=load_deck("examples/decks/red-green-zeon.txt"),
        benchmark_deck=load_deck("examples/decks/blue-white-federation.txt"),
        matches=200, seed=1, workers=8,
    )
    run = run_benchmark(config, progress=lambda done, total: None)
    paths = write_reports(run, Path("gcg-sim-out/zeon-vs-federation"))
```

## Test hook: `GCG_SIM_AGENT_FACTORY`

The test suite sets the environment variable `GCG_SIM_AGENT_FACTORY` to replace the search AI
with a seeded random agent. The value is `package.module:attr` or `/path/to/file.py:attr`, and
it names a callable that takes an `AgentSpec` and returns an agent. `results.json` records the
factory under `ai.factory`. Do not set it for real benchmarks. In Python, pass
`agent_factory=...` to `run_benchmark`; with `workers > 1` it must be picklable, for example a
module-level function or `gcg_sim.runner.FactoryRef("module:attr")`.
