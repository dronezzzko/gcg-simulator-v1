# 11 — Python reference profile: where time goes under search

Scope: measure the Python reference (`src/gcg_sim`, branch `rust-port` at `d9d41af`, Python
3.15.1) while it plays one benchmark game, MCTS against MCTS, at the reference `standard`
preset. The point is to show which code the Rust port has to make fast, and which hot code it
must not "optimize" in ways that change results. All profiling code lives outside the repo, in
`/private/tmp/claude-501/p0-prof/`, and is reproduced verbatim in the appendix. No repo file
was modified.

Notation: `mcts.py:122` means `src/gcg_sim/ai/mcts.py` line 122, and `view.py:232` means
`src/gcg_sim/engine/view.py` line 232. Percentages always name their base: the profiled
`play_game` wall time (cProfile), the sample count (sampler), or the instrumented wall time
(wrappers).

Machine: Apple M2 Pro, 10 cores, 32 GB, macOS 27.2. Measured 2026-11-01. The headline runs
were made at load average 1.2–1.9. Runs made earlier, at load average about 7 while other
agents were busy, were 30% slower and are only listed for context.

---

## 1. What was run

### 1.1 How the runner builds the game and agents (mimicked exactly)

`gcg-sim benchmark DUT BENCH --matches 1 --format bo1 --seed 1 --ai-preset standard` runs
this chain:

1. `cli.cmd_benchmark` (cli.py:188-205) loads and validates both decks and builds
   `BenchmarkConfig(deck_under_test, benchmark_deck, matches=1, seed=1, workers=1, fmt="bo1",
   ai_preset="standard", ...)`. Validation does not change the game.
2. `run_benchmark` (runner/run.py:110-140) → `_run_serial` → `play_match(config, 0)`
   (runner/match.py:19-46) → `play_game(config, 0, 0, chooser=None)` (runner/game.py:106).
3. `play_game` makes the game seed `game_seed(1, 0, 0) = derive_seed(1, "match", 0, "game", 0)`
   (runner/config.py:75-76). It seats the decks with `seat_decks` (DUT in seat
   `match_index % 2 = 0`, runner/config.py:64-72). It calls
   `new_game(decks, seed, chooser=None, turn_limit=200, max_actions=20000)`
   (runner/config.py:15, game.py:33).
4. `_make_agents` (runner/game.py:62-76) → `default_agent_factory` (runner/agents.py:56-63) →
   `gcg_sim.ai.make_agent("mcts", preset="standard", seed=agent_seed(1, 0, 0, seat),
   log=False)` → `MctsAgent(seed, config=PRESETS["standard"])` (ai/agents.py:104-127).
5. `_drive` (runner/game.py:79-103) loops `agent.choose(st, player)` then `apply(st, action)`.

The profiling script calls `gcg_sim.runner.game.play_game(config, 0, 0, None)` with that config
(appendix `common.py`). It skips only report writing. `fmt` affects only `match_point`
(runner/game.py:42-52), so this game is also game 1 of BO3 match 0 for master seed 1.

`standard` preset (ai/config.py:93-97, then `_adopted` at ai/config.py:59-77): budget
`clamp(64·n, 160, 640)` iterations per decision; `exploration=0.3`, `prior_weight=0.5`,
`prior_temperature=1.5`, `horizon_turns=2`, `rollout_temperature=0.5`, `value_scale=1.0`,
`max_rollout_decisions=250`, `first_player_games=24`, `mulligan_samples=96`,
`target_extension=3.0`, `target_extension_z=2.0`, `policy_burst_yes_score=4.0`, and all eight
adopted boolean flags set to `True`.

The run did **not** need the `fast` fallback: one `standard` game takes 37.0 s under cProfile,
far below the 8-minute limit. A `fast` game was also profiled for comparison (§3.3).

### 1.2 The profiled game (identical in every run mode)

| Field | Value |
| --- | --- |
| decks | seat 0 (DUT) `examples/decks/red-green-zeon.txt`, seat 1 `examples/decks/blue-white-federation.txt` |
| game seed | 9194479550923448490 |
| agent seeds (seat 0, seat 1) | 17449794008030232776, 7315535594491859178 |
| agents | `mcts-standard`, `mcts-standard` |
| first player | 1 |
| result | winner `dut` (seat 0), end reason `battle_damage`, 20 turns, 90 recorded actions |
| `final_state_sha256` | `69c66131630c663ce80375fa432efafdc48cc5cf53fade9553907e1e90ff7e4e` |
| `fast` preset, same seeds | winner `bench`, `battle_damage`, 13 turns, 73 actions, sha256 `3d9949351ba8d537858a5fad7151efa857b2da430ebcbfc647a78b47dbb1d07b` |

Every mode below (plain, cProfile, category wrappers, stack sampler, derived-cache disabled,
GC callbacks) produced this digest and, where compared, an identical action list. Profiling
does not change the game. A Rust port can use these two digests as end-to-end checks for this
seed.

### 1.3 Commands (re-run from any directory)

<!-- doc-check: skip needs the profiling scratch scripts from the appendix outside the repository -->
```bash
P=/private/tmp/claude-501/p0-prof           # recreate the scripts from the appendix if missing
R=/Users/drone/dev/src/personal/gcg-simulator-v1
OUT=$P/out
run() { uv run --project "$R" python "$P/run_game.py" "$@" --seed 1 --out "$OUT"; }

run --mode plain      --preset standard     # unprofiled wall time; writes *-summary.json, *-actions.json
run --mode cprofile   --preset standard     # writes $OUT/standard-s1-cprofile.pstats
run --mode sample     --preset standard     # in-process stack sampler; *-sample.json, *-collapsed.txt
run --mode categories --preset standard     # timing wrappers; *-categories.json
run --mode nocache    --preset standard     # view.derived always recomputes (cache-transparency check)
run --mode gc         --preset standard     # cyclic-GC time via gc.callbacks
run --mode plain      --preset fast
run --mode cprofile   --preset fast

uv run --project "$R" python "$P/analyze.py" "$OUT/standard-s1-cprofile.pstats"   # tables in §3-§5
uv run --project "$R" python "$P/micro.py"   "$OUT/standard-s1-plain-actions.json" # §6
cmp "$OUT/standard-s1-nocache-actions.json" "$OUT/standard-s1-plain-actions.json"  # §7

# raw pstats views, if wanted:
uv run --project "$R" python -c "import pstats; s=pstats.Stats('$OUT/standard-s1-cprofile.pstats'); s.sort_stats('tottime').print_stats(40); s.sort_stats('cumtime').print_stats(40)"
hyperfine --runs 3 "uv run --project $R python $P/run_game.py --mode plain --preset standard --seed 1 --out $OUT/hf"
```

The scripts import `gcg_sim` from the project's `uv` environment. `common.warm()` calls
`get_registry()` and imports `gcg_sim.ai` before the timer starts, so card-DB loading and
compilation (0.11 s) are not in any game figure.

---

## 2. Wall-clock baselines

| Run (standard, seed 1 unless noted) | Game wall s | vs plain | Notes |
| --- | ---: | ---: | --- |
| plain, 5 runs at load 1.2–1.9 | 13.09–13.55 | 1.00 | runs: 13.550, 13.493, 13.336, 13.167 (gc mode), 13.086 (hyperfine) |
| hyperfine, whole process, 3 runs | 13.336 ± 0.032 | — | includes interpreter start and the 0.11 s warm-up; 57.1 MiB max RSS |
| cProfile | 37.044 | 2.78× | the base of §3–§5 |
| category wrappers (34 functions wrapped) | 15.789 | 1.18× | 4,340,896 wrapped calls, about 342 ns overhead each |
| in-process stack sampler | 13.671 | 1.02× | 5,996 samples, 438.6 Hz effective |
| `view.derived` cache disabled | 29.405 | 2.20× | same 1,711,701 `derived` calls, all recomputed |
| plain at load avg ~7 (discarded) | 17.617 | — | cProfile at that load: 44.692; wrappers: 22.764 |
| `fast` plain / cProfile | 2.159 / 6.214 | — | 1,839 MCTS iterations |

Cyclic GC: 702 collections (generation 0: 638, generation 1: 59, generation 2: 5) took 0.123 s,
**0.93%** of a 13.167 s game (thresholds `(2000, 10, 10)`). GC is not a factor.

### 2.1 Per-game work counters (deterministic for this seed and preset)

These counts come from the wrapper run and match cProfile's `ncalls` exactly. Engine and
search counts (apply, iterations, determinizations, evaluations) are fixed by game semantics,
so a Rust port that keeps the same call structure can assert them to find the first point of
divergence. `derived`, `_compute` and `clone` counts depend on code structure, not semantics.

| Counter | Count | Source |
| --- | ---: | --- |
| agent decisions (`MctsAgent.choose`) | 90 | ai/agents.py:85 |
| `mcts.search` calls | 72 | mcts.py:82; the other 18 decisions had one option, a REDRAW or a CHOOSE_FIRST; 2 searches returned before iterating (one root move left after pruning) |
| MCTS iterations (`_iterate`) | 15,116 | mcts.py:205; mean 210 per search |
| `_descend` calls | 14,833 | mcts.py:220; the other 283 iterations were a root child's first visit (`fresh`) |
| `_select` calls | 53,701 | mcts.py:271 |
| `_add_children` (prior softmax) | 8,875 | mcts.py:305 |
| `engine.apply` | 140,925 | game.py:780; 9.3 per iteration |
| — from `play_until_turn` (rollouts) | 76,384 | playout.py:12 |
| — from `_descend` (tree walk) | 45,146 | mcts.py:220 |
| — from `search` (root move) | 15,116 | mcts.py:141 |
| — from `play_to_end` (first-player games) | 4,046 | playout.py:29 |
| — from `_drive` (the real game) | 90 | runner/game.py:99 |
| — from `noop` / `setup._result` | 95 / 48 | ai/noop.py, ai/setup.py:114 |
| `engine.advance` | 140,926 | game.py:734 (one extra from `new_game`) |
| `core.rules_management` | 1,204,924 | core.py:857 |
| `interp.run_top_frame` | 108,849 | interp.py:109 |
| `main_options` / `action_step_options` / `eligible_blockers` | 111,599 / 141,522 / 39,309 | game.py:543, :647, battle.py:47 |
| `view.derived` calls | 1,711,701 | view.py:232 |
| `view._compute` (cache misses) | 318,681 | view.py:409; 18.6% miss rate |
| `GameState.clone` | 23,326 | state.py:534; 15,116 from `mcts._world`, 8,184 from `determinize`, 26 from AI filters |
| `observe.determinize` | 8,184 | observe.py:56; MCTS worlds 8,125 (8,055 via `_world`, 70 `worlds[0]` at mcts.py:128), first-player games 48, root filters 11 (noop 8, actions 3) |
| `policy.choose` / `policy.option_scores` / `policy.pick` | 80,471 / 71,717 / 62,842 | policy.py:72, :104, :86; `option_scores` is called 62,842 times from `choose` and 8,875 from `_add_children` |
| `evaluation.score` | 13,934 | evaluation.py:310; 1,182 leaves had finished games and skip it |
| `actions.canonical` | 45,218 | actions.py:72 |
| `prune_dominated` / `merge_equivalent` / `without_effect` | 72 / 72 / 8 | |
| `first_player_choice` / `mulligan_choice` | 1 / 2 | ai/setup.py:86, :63 |

---

## 3. Where time goes: the requested shares

### 3.1 Summary across three measurement methods

Three methods were used because each one distorts differently:

- **cProfile** (`run --mode cprofile`): deterministic, but adds a fixed cost to every Python
  call. That inflates call-heavy code such as `view.py` helpers and builtins.
- **Stack sampler** (`sampler.py`, `--mode sample`): 2% overhead. It samples at eval-breaker
  points, so time inside a C builtin is charged to the Python frame that called it.
- **Wrappers** (`categories.py`, `--mode categories`): exact call counts and timers. Each
  wrapped call adds about 342 ns, which inflates the parents of very frequent callees such as
  `derived` and `rules_management`.

"Inclusive" means the function or anything it calls is on the stack. Inclusive rows overlap.
"Exclusive (sampler)" charges each sample to the innermost category on its stack, and that
column sums to 100%.

| Category (functions) | cProfile incl. % | sampler incl. % | sampler excl. % | wrappers incl. % | calls/game | mean cost per call, unprofiled |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| **Engine apply/advance** (`game.apply` and everything under it) | 80.7 | 80.9 | 33.3 (not in another row) | 79.8 | 140,925 | ≈76 µs (13.34 s × 0.807 / 140,925) |
| **Legal-action generation** (`main_options`, `action_step_options`, `eligible_blockers`) | 23.7 (20.1 + 3.2 + 0.4) | 20.9 | 13.0 | 20.8 | 292,430 | `main_options` 24 µs incl. (wrappers 2.705 s / 111,599) |
| **`view.derived`** (cache check and recompute) | 33.1 | 34.7 | 34.7 | 32.0 | 1,711,701 | hit 0.126 µs (§6); miss = `_compute` |
| — of which `_compute` (recompute) | 29.6 | 33.6 | — | 28.4 | 318,681 | 14.0 µs mean in-game (wrappers 4.477 s / 318,681); 3.5–10.9 µs at probe states (§6) |
| — of which called from engine code | 32.6 | 34.6 (26.7 under apply, 7.9 under legal-action) | — | — | 1,425,363 | |
| — of which called from AI code (policy, evaluation, actions) | 0.5 | 0.1 | — | — | 286,338 | nearly all hits: the engine already computed the view |
| **`GameState.clone`** | 3.2 | 3.7 | 3.7 | 3.2 | 23,326 | 21.4 µs mean in-game (wrappers); 17.4 µs at probe states (§6) |
| **`observe.determinize`** (including its clone) | 3.9 | 5.5 | 4.3 (without clone) | 5.0 | 8,184 | 95.7 µs in-game including clone, 77.7 µs without; 78–90 µs at probe states |
| **Policy** (`policy.choose`, `option_scores`) | 6.8 | 6.1 | 5.9 | 5.6 | 80,471 / 71,717 | `option_scores` 10.2 µs mean in-game; 2.4–6.0 µs for a MAIN decision at probe states |
| **Evaluation** (`evaluation.score`, `role_features`) | 1.7 | 1.3 | 1.3 | 1.3 | 13,934 | 15.0 µs mean in-game; 7.1–8.9 µs at probe states |
| **MCTS bookkeeping** (`mcts.py` and `playout.py` own code: selection, UCB, prior softmax, backup, seed lists, `_settled`, `_close_call`) | 2.3 (tottime of mcts.py 2.1 + playout.py 0.2) | — | 2.7 | 4.7 (sum of exclusive; inflated) | — | ≈0.86 ms per iteration, everything included (13.34 s × 0.972 / 15,116) |
| AI move canonicalization and pruning (`canonical`, `prune_dominated`, `merge_equivalent`) | 0.8 | 0.8 | 0.8 | 0.7 | 45,218 | `canonical` 2.4 µs mean in-game |
| First-player choice (24 game pairs played to the end) | 2.7 | 2.7 | ≈0 (own code) | 2.5 | 1 | 0.98 s under cProfile, about 0.35–0.40 s unprofiled |
| `noop.without_effect`, `mulligan_choice` | <0.1 | <0.1 | <0.1 | <0.1 | 8, 2 | |
| Agent and runner glue (`_drive`, `MctsAgent.choose`, `state_digest`) | ≈0.1 | — | 0.2 | ≈0 | | |
| Cyclic GC | — | — | — | — | 702 | 0.93% of wall (§2) |

### 3.2 Reading the numbers

- About **80%** of search time is inside the engine's `apply`. The Python docs say the same
  ("80–83% of search time was inside the engine's `apply`", docs/AI.md:298-299). Rollouts
  (`play_until_turn`, policy included) take 51%, of which `apply` is 44.5%. Tree descent
  takes 27%, and applying the root move about 10%.
- **`view.derived` recomputation is the biggest single hotspot: about 30–34% of the total.**
  Almost all of it runs inside engine code: `core.emit` 10.4%, `main_options` 6.8%,
  `battle._destroy_battle` 2.9%, `interp._enter_rested` 2.8%, `_turn_step` 2.3% (cProfile edge
  cumtime, §5.5). Of the 1.71 M calls, 81.4% hit the cache (`_view_key` matches,
  view.py:223-238). Inside `_compute`, the main costs are `_base_view` (11.7% cumulative;
  only `_compute` calls it), its `_host_abilities` (3.9%), `cond` (4.7%; 99% of its calls
  come from `_compute`), and `resolve` (2.8%). The filter matchers `select` → `matches` →
  `_match1` (7.3% / 8.6% / 5.6%) mostly serve engine target enumeration
  (`interp._choose_candidates`, `choice_short`), not `_compute`.
- Legal-action generation is about 21–24%. `main_options` alone is 20%, and a third of that is
  its own `derived` call. `_activated_list` takes 3.7% and `_attack_options` 2.7%.
- Clone and determinize together are about 6–8% (cProfile 3.2 + 3.9 − 1.1 overlap; sampler
  3.7 + 4.3). The world cache in `mcts._world` (mcts.py:154-162) makes the k-th
  determinization once per search, and every visit, the first included, plays in a clone of it.
- Policy is about 6%, evaluation about 1.5%, and MCTS bookkeeping about 2–3%. A
  straightforward Rust MCTS will not be the bottleneck. A Rust port is fast only if its engine
  (`apply`/`advance`, triggers, interp) and derived view are fast.
- Allocation of dataclass objects (`Action`, `Decision`, `_Child`, … built by
  `dataclasses.__create_fn__`, so their `co_filename` is `"<string>"`) is 3.5% of samples as
  leaf frames. Most come from `game.py` generators that build option tuples,
  `interp.push_frame`, `_turn_step`, `_attack_options`, and `policy._board`. cProfile does not
  see this cost (Hazard H1).

### 3.3 `fast` preset (same seeds) for comparison

cProfile of the `fast` game (6.214 s profiled, 2.159 s plain; 1,839 iterations, 19,685
applies): `apply` 82.8% inclusive, `view.derived` 42.2% (`_compute` 38.8%), `main_options`
22.9%, policy 9.7%, `first_player_choice` 5.4%, clone 2.0%, determinize 2.0%,
`evaluation.score` 1.2%, `_select` 1.3%. The shape is the same as `standard`. Fixed per-game
costs (first-player games, the real game's own applies) are a larger share at the small
budget.

---

## 4. cProfile tables (standard, seed 1; base = `play_game` cumtime 37.044 s)

`play_game cumtime 37.044s (the % base below); sum of tottime 36.538s; 129152071 calls (129151273 primitive)`

`ncalls` shows `total/primitive` when the function recursed. Functions are named
`path-relative-to-src/gcg_sim:line(name)`. Builtins are named `<builtin> …`.

### 4.1 Top 40 by tottime

| # | ncalls | tottime s | tottime % | cumtime s | cumtime % | function |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | 318681 | 1.900 | 5.1 | 4.318 | 11.7 | `engine/view.py:245(_base_view)` |
| 2 | 318681 | 1.653 | 4.5 | 10.949 | 29.6 | `engine/view.py:409(_compute)` |
| 3 | 18814709 | 1.391 | 3.8 | 1.396 | 3.8 | `<builtin> <method 'get' of 'dict' objects>` |
| 4 | 18426231 | 1.187 | 3.2 | 1.200 | 3.2 | `<builtin> <built-in method builtins.isinstance>` |
| 5 | 1301261 | 1.079 | 2.9 | 2.072 | 5.6 | `engine/view.py:708(_match1)` |
| 6 | 1471220 | 0.945 | 2.6 | 1.456 | 3.9 | `engine/view.py:180(_host_abilities)` |
| 7 | 460722 | 0.861 | 2.3 | 6.451 | 17.4 | `engine/core.py:316(emit)` |
| 8 | 111599 | 0.680 | 1.8 | 7.430 | 20.1 | `engine/game.py:543(main_options)` |
| 9 | 1054050 | 0.663 | 1.8 | 17.008 | 45.9 | `engine/game.py:223(_turn_step)` |
| 10 | 140926 | 0.656 | 1.8 | 25.728 | 69.5 | `engine/game.py:734(advance)` |
| 11 | 253121 | 0.653 | 1.8 | 1.361 | 3.7 | `engine/game.py:410(_activated_list)` |
| 12 | 187633 | 0.639 | 1.7 | 0.988 | 2.7 | `engine/view.py:314(trigger_index)` |
| 13 | 872623 | 0.604 | 1.6 | 3.194 | 8.6 | `engine/view.py:702(matches)` |
| 14 | 1711701 | 0.588 | 1.6 | 12.274 | 33.1 | `engine/view.py:232(derived)` |
| 15 | 397006 | 0.560 | 1.5 | 0.692 | 1.9 | `engine/core.py:917(_damage_scan)` |
| 16 | 2868954 | 0.549 | 1.5 | 0.722 | 2.0 | `engine/state.py:104(copy)` |
| 17 | 443639/443092 | 0.523 | 1.4 | 1.731 | 4.7 | `engine/view.py:885(cond)` |
| 18 | 1711701 | 0.500 | 1.3 | 0.737 | 2.0 | `engine/view.py:223(_view_key)` |
| 19 | 372922 | 0.486 | 1.3 | 0.598 | 1.6 | `engine/view.py:336(_copy_base)` |
| 20 | 7786700 | 0.475 | 1.3 | 0.475 | 1.3 | `cards/db.py:289(by_id)` |
| 21 | 2173884 | 0.459 | 1.2 | 2.532 | 6.8 | `engine/view.py:705(<genexpr>)` |
| 22 | 111599 | 0.453 | 1.2 | 0.996 | 2.7 | `engine/game.py:475(_attack_options)` |
| 23 | 418587 | 0.421 | 1.1 | 0.612 | 1.7 | `engine/view.py:125(link_satisfied)` |
| 24 | 23326 | 0.420 | 1.1 | 1.173 | 3.2 | `engine/state.py:534(clone)` |
| 25 | 5227021 | 0.372 | 1.0 | 0.372 | 1.0 | `<builtin> <built-in method builtins.len>` |
| 26 | 996703 | 0.372 | 1.0 | 0.372 | 1.0 | `rng.py:23(next_u64)` |
| 27 | 186466 | 0.370 | 1.0 | 0.428 | 1.2 | `engine/view.py:94(same_values)` |
| 28 | 505576 | 0.368 | 1.0 | 0.582 | 1.6 | `engine/view.py:347(_apply_cont)` |
| 29 | 2273695 | 0.364 | 1.0 | 0.519 | 1.4 | `engine/view.py:565(rules_of)` |
| 30 | 1204924 | 0.348 | 0.9 | 1.331 | 3.6 | `engine/core.py:857(rules_management)` |
| 31 | 889387/889187 | 0.276 | 0.7 | 1.044 | 2.8 | `engine/view.py:784(resolve)` |
| 32 | 167573 | 0.266 | 0.7 | 2.708 | 7.3 | `engine/view.py:626(select)` |
| 33 | 1108109 | 0.262 | 0.7 | 0.328 | 0.9 | `engine/view.py:23(cdef)` |
| 34 | 1572045 | 0.261 | 0.7 | 0.524 | 1.4 | `<builtin> <method 'extend' of 'list' objects>` |
| 35 | 178409 | 0.250 | 0.7 | 0.531 | 1.4 | `engine/core.py:84(_move_single)` |
| 36 | 1418722 | 0.247 | 0.7 | 0.321 | 0.9 | `engine/view.py:414(<genexpr>)` |
| 37 | 2692671 | 0.241 | 0.6 | 0.241 | 0.6 | `<builtin> <method 'append' of 'list' objects>` |
| 38 | 1312853 | 0.231 | 0.6 | 0.400 | 1.1 | `cards/model.py:125(is_token)` |
| 39 | 108849 | 0.228 | 0.6 | 7.003 | 18.9 | `engine/interp.py:109(run_top_frame)` |
| 40 | 1939222 | 0.223 | 0.6 | 0.424 | 1.1 | `<builtin> <built-in method builtins.max>` |

### 4.2 Top 40 by cumtime

| # | ncalls | tottime s | tottime % | cumtime s | cumtime % | function |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | 1 | 0.000 | 0.0 | 37.044 | 100.0 | `common.py:43(play_one)` |
| 2 | 1 | 0.000 | 0.0 | 37.044 | 100.0 | `runner/game.py:106(play_game)` |
| 3 | 1 | 0.000 | 0.0 | 37.043 | 100.0 | `runner/game.py:79(_drive)` |
| 4 | 90 | 0.030 | 0.1 | 37.025 | 99.9 | `ai/agents.py:85(choose)` |
| 5 | 72 | 0.113 | 0.3 | 36.003 | 97.2 | `ai/mcts.py:82(search)` |
| 6 | 140925 | 0.105 | 0.3 | 29.883 | 80.7 | `engine/game.py:780(apply)` |
| 7 | 15116 | 0.035 | 0.1 | 29.759 | 80.3 | `ai/mcts.py:205(_iterate)` |
| 8 | 140926 | 0.656 | 1.8 | 25.728 | 69.5 | `engine/game.py:734(advance)` |
| 9 | 15116 | 0.064 | 0.2 | 18.955 | 51.2 | `ai/playout.py:12(play_until_turn)` |
| 10 | 1054050 | 0.663 | 1.8 | 17.008 | 45.9 | `engine/game.py:223(_turn_step)` |
| 11 | 1711701 | 0.588 | 1.6 | 12.274 | 33.1 | `engine/view.py:232(derived)` |
| 12 | 318681 | 1.653 | 4.5 | 10.949 | 29.6 | `engine/view.py:409(_compute)` |
| 13 | 14833 | 0.089 | 0.2 | 10.104 | 27.3 | `ai/mcts.py:220(_descend)` |
| 14 | 111599 | 0.680 | 1.8 | 7.430 | 20.1 | `engine/game.py:543(main_options)` |
| 15 | 108849 | 0.228 | 0.6 | 7.003 | 18.9 | `engine/interp.py:109(run_top_frame)` |
| 16 | 460722 | 0.861 | 2.3 | 6.451 | 17.4 | `engine/core.py:316(emit)` |
| 17 | 124610 | 0.086 | 0.2 | 6.073 | 16.4 | `engine/interp.py:153(execute)` |
| 18 | 39549 | 0.066 | 0.2 | 5.493 | 14.8 | `engine/battle.py:196(damage_step)` |
| 19 | 318681 | 1.900 | 5.1 | 4.318 | 11.7 | `engine/view.py:245(_base_view)` |
| 20 | 23888 | 0.149 | 0.4 | 3.984 | 10.8 | `engine/battle.py:99(_destroy_battle)` |
| 21 | 17200 | 0.058 | 0.2 | 3.605 | 9.7 | `engine/battle.py:165(_exchange)` |
| 22 | 872623 | 0.604 | 1.6 | 3.194 | 8.6 | `engine/view.py:702(matches)` |
| 23 | 110099 | 0.068 | 0.2 | 3.034 | 8.2 | `engine/game.py:821(_apply_main)` |
| 24 | 167573 | 0.266 | 0.7 | 2.708 | 7.3 | `engine/view.py:626(select)` |
| 25 | 2173884 | 0.459 | 1.2 | 2.532 | 6.8 | `engine/view.py:705(<genexpr>)` |
| 26 | 80471 | 0.053 | 0.1 | 2.522 | 6.8 | `ai/policy.py:72(choose)` |
| 27 | 71717 | 0.037 | 0.1 | 2.385 | 6.4 | `ai/policy.py:104(option_scores)` |
| 28 | 15116 | 0.013 | 0.0 | 2.230 | 6.0 | `ai/mcts.py:154(_world)` |
| 29 | 38764 | 0.098 | 0.3 | 2.230 | 6.0 | `engine/battle.py:12(declare_attack)` |
| 30 | 50821 | 0.080 | 0.2 | 2.185 | 5.9 | `ai/policy.py:158(_main)` |
| 31 | 1301261 | 1.079 | 2.9 | 2.072 | 5.6 | `engine/view.py:708(_match1)` |
| 32 | 30755 | 0.086 | 0.2 | 1.957 | 5.3 | `engine/interp.py:923(_h_deploy_card)` |
| 33 | 443639/443092 | 0.523 | 1.4 | 1.731 | 4.7 | `engine/view.py:885(cond)` |
| 34 | 24926 | 0.161 | 0.4 | 1.728 | 4.7 | `engine/core.py:774(destroy)` |
| 35 | 37813 | 0.044 | 0.1 | 1.693 | 4.6 | `engine/interp.py:314(_choose_candidates)` |
| 36 | 30465 | 0.082 | 0.2 | 1.655 | 4.5 | `engine/interp.py:956(deploy_cards)` |
| 37 | 21594 | 0.069 | 0.2 | 1.620 | 4.4 | `engine/interp.py:407(_h_choose)` |
| 38 | 278120 | 0.093 | 0.3 | 1.555 | 4.2 | `ai/policy.py:164(_main_option)` |
| 39 | 8184 | 0.109 | 0.3 | 1.462 | 3.9 | `engine/observe.py:56(determinize)` |
| 40 | 1471220 | 0.945 | 2.6 | 1.456 | 3.9 | `engine/view.py:180(_host_abilities)` |

---

## 5. cProfile breakdowns

### 5.1 Inclusive time of key functions (cumtime; overlapping)

| function | ncalls | cumtime s | % of total | tottime s | us per call (cum) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `engine/game.py:780(apply)` | 140925 | 29.883 | 80.7 | 0.105 | 212.1 |
| `engine/game.py:734(advance)` | 140926 | 25.728 | 69.5 | 0.656 | 182.6 |
| `engine/game.py:223(_turn_step)` | 1054050 | 17.008 | 45.9 | 0.663 | 16.1 |
| `engine/game.py:342(_action_step)` | 210951 | 1.245 | 3.4 | 0.074 | 5.9 |
| `engine/game.py:543(main_options)` | 111599 | 7.430 | 20.1 | 0.680 | 66.6 |
| `engine/game.py:647(action_step_options)` | 141522 | 1.168 | 3.2 | 0.167 | 8.3 |
| `engine/game.py:475(_attack_options)` | 111599 | 0.996 | 2.7 | 0.453 | 8.9 |
| `engine/game.py:372(payment_choices)` | 271935 | 0.211 | 0.6 | 0.139 | 0.8 |
| `engine/battle.py:47(eligible_blockers)` | 39309 | 0.131 | 0.4 | 0.051 | 3.3 |
| `engine/core.py:857(rules_management)` | 1204924 | 1.331 | 3.6 | 0.348 | 1.1 |
| `engine/interp.py:109(run_top_frame)` | 108849 | 7.003 | 18.9 | 0.228 | 64.3 |
| `engine/interp.py:160(resume)` | 26264 | 0.970 | 2.6 | 0.033 | 36.9 |
| `engine/state.py:534(clone)` | 23326 | 1.173 | 3.2 | 0.420 | 50.3 |
| `engine/observe.py:56(determinize)` | 8184 | 1.462 | 3.9 | 0.109 | 178.7 |
| `engine/view.py:232(derived)` | 1711701 | 12.274 | 33.1 | 0.588 | 7.2 |
| `engine/view.py:409(_compute)` | 318681 | 10.949 | 29.6 | 1.653 | 34.4 |
| `ai/policy.py:72(choose)` | 80471 | 2.522 | 6.8 | 0.053 | 31.3 |
| `ai/policy.py:104(option_scores)` | 71717 | 2.385 | 6.4 | 0.037 | 33.3 |
| `ai/evaluation.py:310(score)` | 13934 | 0.632 | 1.7 | 0.019 | 45.4 |
| `ai/evaluation.py:280(role_features)` | 13934 | 0.567 | 1.5 | 0.047 | 40.7 |
| `ai/evaluation.py:132(burst_density)` | 71 | 0.004 | 0.0 | 0.000 | 55.6 |
| `ai/actions.py:72(canonical)` | 45218 | 0.308 | 0.8 | 0.096 | 6.8 |
| `ai/actions.py:242(prune_dominated)` | 72 | 0.002 | 0.0 | 0.000 | 23.4 |
| `ai/actions.py:379(merge_equivalent)` | 72 | 0.003 | 0.0 | 0.000 | 40.5 |
| `ai/noop.py:128(without_effect)` | 8 | 0.018 | 0.0 | 0.000 | 2244.3 |
| `ai/setup.py:63(mulligan_choice)` | 2 | 0.007 | 0.0 | 0.000 | 3558.4 |
| `ai/setup.py:86(first_player_choice)` | 1 | 0.984 | 2.7 | 0.000 | 984437.6 |
| `ai/agents.py:85(choose)` | 90 | 37.025 | 99.9 | 0.030 | 411387.5 |
| `ai/mcts.py:82(search)` | 72 | 36.003 | 97.2 | 0.113 | 500040.3 |
| `ai/mcts.py:271(_select)` | 53701 | 0.969 | 2.6 | 0.183 | 18.0 |
| `ai/mcts.py:305(_add_children)` | 8875 | 0.450 | 1.2 | 0.066 | 50.7 |
| `ai/mcts.py:205(_iterate)` | 15116 | 29.759 | 80.3 | 0.035 | 1968.7 |
| `ai/mcts.py:220(_descend)` | 14833 | 10.104 | 27.3 | 0.089 | 681.2 |
| `ai/mcts.py:254(_leaf_value)` | 15116 | 0.651 | 1.8 | 0.012 | 43.1 |
| `ai/mcts.py:154(_world)` | 15116 | 2.230 | 6.0 | 0.013 | 147.5 |
| `ai/mcts.py:320(_settled)` | 15115 | 0.052 | 0.1 | 0.011 | 3.4 |
| `ai/mcts.py:181(_close_call)` | 11 | 0.000 | 0.0 | 0.000 | 41.8 |
| `ai/playout.py:12(play_until_turn)` | 15116 | 18.955 | 51.2 | 0.064 | 1254.0 |
| `runner/digest.py:52(state_digest)` | 1 | 0.000 | 0.0 | 0.000 | 151.8 |

### 5.2 Exclusive time (tottime) by source file (partition of the sum of tottime; % of play_game cumtime)

| file | tottime s | % | calls |
| --- | ---: | ---: | ---: |
| `engine/view.py` | 14.632 | 39.5 | 30472162 |
| `<builtins>` | 4.828 | 13.0 | 59312800 |
| `engine/game.py` | 4.550 | 12.3 | 4832701 |
| `engine/core.py` | 3.544 | 9.6 | 7314293 |
| `engine/interp.py` | 1.853 | 5.0 | 2090517 |
| `engine/state.py` | 1.149 | 3.1 | 3807771 |
| `ai/policy.py` | 0.928 | 2.5 | 1279357 |
| `ai/mcts.py` | 0.768 | 2.1 | 1311144 |
| `rng.py` | 0.734 | 2.0 | 2075048 |
| `cards/db.py` | 0.546 | 1.5 | 8262246 |
| `cards/model.py` | 0.535 | 1.4 | 3699935 |
| `engine/battle.py` | 0.510 | 1.4 | 323223 |
| `ai/values.py` | 0.505 | 1.4 | 1050228 |
| `ai/evaluation.py` | 0.412 | 1.1 | 642000 |
| `engine/observe.py` | 0.323 | 0.9 | 81982 |
| `ai/actions.py` | 0.301 | 0.8 | 650110 |
| `<stdlib/other>` | 0.277 | 0.7 | 1653416 |
| `ai/playout.py` | 0.067 | 0.2 | 15164 |
| `ai/agents.py` | 0.030 | 0.1 | 94 |
| `engine/types.py` | 0.021 | 0.1 | 214714 |
| `ai/targeting.py` | 0.010 | 0.0 | 32435 |
| `effects/builtins.py` | 0.008 | 0.0 | 4361 |
| `ai/base.py` | 0.004 | 0.0 | 10054 |
| `effects/registry.py` | 0.003 | 0.0 | 11851 |
| `ai/setup.py` | 0.001 | 0.0 | 1373 |
| `ai/noop.py` | 0.001 | 0.0 | 2331 |
| `runner/instrument.py` | 0.001 | 0.0 | 639 |
| `runner/game.py` | 0.000 | 0.0 | 9 |
| `ai/config.py` | 0.000 | 0.0 | 70 |
| `runner/digest.py` | 0.000 | 0.0 | 4 |
| `effects/program.py` | 0.000 | 0.0 | 10 |
| `runner/config.py` | 0.000 | 0.0 | 25 |
| `runner/agents.py` | 0.000 | 0.0 | 2 |
| `deck/parse.py` | 0.000 | 0.0 | 2 |

### 5.3 Exclusive time by package (engine / effects / ai / runner / builtins)

| package | tottime s | % |
| --- | ---: | ---: |
| `engine` | 26.581 | 71.8 |
| `<builtins>` | 4.828 | 13.0 |
| `ai` | 3.026 | 8.2 |
| `cards` | 1.081 | 2.9 |
| `rng.py` | 0.734 | 2.0 |
| `<stdlib/other>` | 0.277 | 0.7 |
| `effects` | 0.010 | 0.0 |
| `runner` | 0.001 | 0.0 |
| `deck` | 0.000 | 0.0 |

### 5.4 `view.derived` calls and cumtime by caller package

| caller package | calls | cumtime s | % of total |
| --- | ---: | ---: | ---: |
| `engine` | 1425363 | 12.095 | 32.6 |
| `ai` | 286338 | 0.180 | 0.5 |

### 5.5 Direct callers of `engine/view.py:232(derived)` (edge cumtime)

| caller | ncalls | cumtime s | % of total |
| --- | ---: | ---: | ---: |
| `engine/core.py:316(emit)` | 187591 | 3.865 | 10.4 |
| `engine/game.py:543(main_options)` | 111599 | 2.527 | 6.8 |
| `engine/battle.py:99(_destroy_battle)` | 23888 | 1.069 | 2.9 |
| `engine/interp.py:979(_enter_rested)` | 30465 | 1.032 | 2.8 |
| `engine/game.py:223(_turn_step)` | 29743 | 0.852 | 2.3 |
| `engine/interp.py:1249(_h_apply)` | 23498 | 0.513 | 1.4 |
| `engine/interp.py:1368(pair)` | 9687 | 0.459 | 1.2 |
| `engine/interp.py:1347(_h_pair)` | 9687 | 0.374 | 1.0 |
| `engine/interp.py:371(choice_short)` | 50197 | 0.257 | 0.7 |
| `engine/game.py:874(_play_or_activate)` | 43611 | 0.188 | 0.5 |
| `engine/game.py:647(action_step_options)` | 141522 | 0.183 | 0.5 |
| `engine/game.py:410(_activated_list)` | 253121 | 0.166 | 0.4 |
| `ai/actions.py:106(_attack_can_matter)` | 209722 | 0.126 | 0.3 |
| `engine/interp.py:314(_choose_candidates)` | 37813 | 0.126 | 0.3 |
| `engine/core.py:857(rules_management)` | 2595 | 0.102 | 0.3 |

### 5.6 Direct callers of `engine/view.py:409(_compute)` (edge cumtime)

| caller | ncalls | cumtime s | % of total |
| --- | ---: | ---: | ---: |
| `engine/view.py:232(derived)` | 318681 | 10.949 | 29.6 |

### 5.7 Direct callers of `engine/state.py:534(clone)` (edge cumtime)

| caller | ncalls | cumtime s | % of total |
| --- | ---: | ---: | ---: |
| `ai/mcts.py:154(_world)` | 15116 | 0.776 | 2.1 |
| `engine/observe.py:56(determinize)` | 8184 | 0.395 | 1.1 |
| `ai/noop.py:179(continue_and_replay)` | 10 | 0.000 | 0.0 |
| `ai/noop.py:128(without_effect)` | 8 | 0.000 | 0.0 |
| `ai/actions.py:214(_resting_changes_more_than_the_unit)` | 5 | 0.000 | 0.0 |
| `ai/noop.py:77(_branches)` | 3 | 0.000 | 0.0 |

### 5.8 Direct callers of `engine/game.py:780(apply)` (edge cumtime)

| caller | ncalls | cumtime s | % of total |
| --- | ---: | ---: | ---: |
| `ai/playout.py:12(play_until_turn)` | 76384 | 16.503 | 44.5 |
| `ai/mcts.py:220(_descend)` | 45146 | 8.901 | 24.0 |
| `ai/mcts.py:82(search)` | 15116 | 3.609 | 9.7 |
| `ai/playout.py:29(play_to_end)` | 4046 | 0.838 | 2.3 |
| `runner/game.py:79(_drive)` | 90 | 0.017 | 0.0 |
| `ai/noop.py:110(_replay)` | 43 | 0.006 | 0.0 |
| `ai/noop.py:99(_continue)` | 31 | 0.005 | 0.0 |
| `ai/noop.py:67(_resolve)` | 20 | 0.003 | 0.0 |
| `ai/setup.py:114(_result)` | 48 | 0.002 | 0.0 |
| `ai/noop.py:77(_branches)` | 1 | 0.000 | 0.0 |

---

## 6. Micro-benchmarks: one `GameState.clone()` and one `view.derived()` on mid-game states

Method (`micro.py`): the game is rebuilt with `new_game` from the recorded seed, and the
recorded actions are re-applied. Three probe states are taken: the first MAIN decision with
more than one option at or after turns 6, 10 and 16. Each measurement is `n` calls in a
`time.perf_counter_ns` loop, repeated 5 times, after one warm-up call. The table shows the
minimum per-call time; the median was within 1.6%, except one rollout median at turn 6 (+3.5%).
GC stayed enabled. Each probe state has 123 card instances, no lasting effects, and one
history entry.

| Operation | turn 6 (7 options) | turn 10 (4 options) | turn 16 (4 options) | n per repeat |
| --- | ---: | ---: | ---: | ---: |
| `GameState.clone()` | **17.45 µs** | **17.40 µs** | **17.46 µs** | 20,000 |
| `view.derived(st)`, cache hit (`_view_key` compare only) | **0.126 µs** | **0.126 µs** | **0.126 µs** | 200,000 |
| `view.derived(st)` after `st._derived = None` (full recompute) | **10.75 µs** | **3.45 µs** | **3.73 µs** | 5,000 |
| `observe.determinize(st, player, 12345)` | 90.2 µs | 85.0 µs | 77.9 µs | 2,000 |
| `determinize` + first `derived` on the world | 104.7 µs | 89.4 µs | 82.9 µs | 2,000 |
| `ai.actions.canonical(st, MAIN decision)` | 2.20 µs | 1.36 µs | 1.13 µs | 20,000 |
| `policy.option_scores(st, MAIN decision)` (derived cached) | 5.96 µs | 2.39 µs | 5.49 µs | 10,000 |
| `evaluation.score(st, player, DEFAULT_WEIGHTS, density)` (derived cached) | 8.92 µs | 7.07 µs | 7.79 µs | 10,000 |
| `clone()` + `apply(policy move, check=False)` up to the next decision | 64.4 µs (`play_unit(9,0)`) | 46.7 µs (`play_unit(39,0)`) | 45.8 µs (`play_unit(30,0)`) | 2,000 |
| one rollout iteration: `determinize` + `apply` + `play_until_turn(turn + 2, T=0.5, 250)` | 1103 µs | 733 µs | 690 µs | 200 distinct seeds |

Zone sizes `[p0, p1]` at the probes:
turn 6: deck 36/36, hand 6/5, battle 1/1, shield 6/6, base 1/1, resources 3/3, trash 1/1.
turn 10: deck 33/34, hand 4/2, battle 0/1, shield 6/4, base 0/1, resources 5/5, trash 7/7.
turn 16: deck 30/31, hand 1/0, battle 1/2, shield 5/1, base 0/0, resources 8/8, trash 13/16.

Notes:

- `clone()` costs the same at every probe (17.4 µs). Its cost is dominated by the 123
  `CardInstance.copy()` calls and the 2×12 zone lists (state.py:534-572), not by game
  progress. In-game the mean is 21.4 µs, because in-game states also carry frames, battles,
  lasting effects and triggers.
- A recompute costs 3.5–4 µs when there are no lasting effects and no constant abilities: the
  fast path at view.py:414-419 returns after `_base_view` (turns 10 and 16: `has_constants`
  False, checked). It costs 10.7 µs at turn 6, where a constant ability is on the board
  (`has_constants` True, checked) and the fixpoint runs (view.py:421-463, at most 6 rounds).
  The in-game mean is 14.0 µs, which suggests that states seen during search often carry
  lasting effects or constants (not measured per call; Q4).
- In-game means come from the wrapper run, whose clone and `_compute` timers wrap no other
  timed function. They are slightly inflated by load and cache effects, not by nested wrapper
  overhead.

---

## 7. Check: is the `derived` cache semantically transparent?

`nocache.py` replaces `view.derived` in every `gcg_sim` module namespace with a function that
always calls `_compute`, sets `dv.key = _view_key(st)`, and stores the result. Result for
standard, seed 1: the same 1,711,701 `derived` calls, all recomputed, in 29.405 s instead of
13.3 s. The 90 actions were byte-identical (`cmp` of the action JSON) and so was
`final_state_sha256` (`69c66131…`).

So for this game, a stale cached view is never read in a way that changes a decision. The
cache only saves time: 55% of the game's wall time. The Rust port may use a different (or no)
invalidation scheme and still match, *provided the Python code never depends on a stale view*.
This was shown for one game only (Open question Q3).

---

## Hazards

Traps for anyone who uses these numbers, or who ports the hot paths for speed.

- **H1 — cProfile drops dataclass-generated methods (verified).** Methods made by
  `dataclasses` (e.g. the `__init__` of `Action`, `Decision`, `_Child`) have
  `co_filename "<string>"` and `co_firstlineno 2`, and all share the label
  `('<string>', 2, '__init__')`. pstats keys its entries by that label, and the last code
  object's entry overwrites the others. A test with two dataclasses (1,000 and 3,000
  instances) recorded only 3,000 calls. As a result, the sum of tottime (36.538 s) is
  0.506 s below the root cumtime (37.044 s), and dataclass allocation (3.5% of sampled leaf
  frames) is missing from the cProfile tables. Never use pstats counts of `<string>` entries as
  parity or work metrics.
- **H2 — The cached derived view is shared between clones.** `clone()` copies the reference
  (`s._derived = self._derived`, state.py:570). `derived()` reuses the cached object when
  `dv.key == _view_key(st)` (view.py:223-238; the key is `(active, turn, len(battles),
  (battle_id, target, ended) | (), len(history))`). `touch()` clears it (state.py:529-531).
  `Derived.by_event` is filled lazily *on the shared object* by `trigger_index`
  (view.py:314-333), from the `zones` of whichever state asks first. A Rust port that shares
  the view (e.g. `Rc<Derived>`) must treat it as immutable apart from this memo, or it must
  recompute. §7 shows that always recomputing gives the same game for seed 1.
- **H3 — `clone()` shares, rather than copies, some members.** `pending` (state.py:546),
  every `HistoryEvent` in `history` (state.py:558, a shallow list copy), and the
  `decklists`/`resource_decklists` tuples are shared. The code avoids mutating shared entries:
  `observe._own_history` builds new `HistoryEvent`s instead (observe.py:86-95). A Rust deep
  copy is equivalent only because nothing mutates these in place. Do not add in-place
  mutation of them.
- **H4 — Float reductions on the hot path must keep CPython semantics, even when they are
  hot.** These run thousands of times per game, so they tempt optimization. None may be
  reordered, vectorized or made naive:
  - `sum(weights)` in the prior softmax (mcts.py:315, 8,875 calls/game) and `math.exp((x -
    top) / prior_temperature)` (mcts.py:314);
  - `sum(w * x for …)` in `evaluation.score` (evaluation.py:314, :317; 13,934 calls/game);
  - `sum(diffs) / n` and the variance `sum(...)` in `_close_call` (mcts.py:198-199);
  - in `_race`, `mean_ap = sum(through) / len(through)` (evaluation.py:274) and
    `math.ceil(base_hp / mean_ap)` (evaluation.py:275), for each of the 13,934 evaluated
    leaves per game.

  CPython 3.12+ `sum()` of floats uses Neumaier compensated summation. The integer `sum`s at
  evaluation.py:165, :249, :273 are exact. But the `_race` division must stay two f64
  divisions in that order. Integer ceil-division gives different answers: with
  `sum(through) = 7`, `len(through) = 5` and `base_hp = 21`, Python computes
  `21 / 1.4 = 15.000000000000002`, so `ceil` gives 16, while exact arithmetic gives 15. A
  brute-force check over sums 1–399, lengths 1–12 (sum ≥ length) and base HP 0–59 found 11
  such cases.
- **H5 — First-maximal `max()` on the hot path.** In `_select` (mcts.py:288 unvisited by
  prior, mcts.py:301 UCB, 53,701 calls/game), `_close_call` leader (mcts.py:187), `_result`
  (mcts.py:338) and `policy.pick` (policy.py:93), Python `max` keeps the FIRST maximal element.
  Rust `Iterator::max_by` keeps the LAST. A faster Rust selection loop must keep strict `>`
  comparisons in the original iteration order.
- **H6 — Gumbel sampling in the rollout policy** (policy.py:94-101; `pick` runs 62,842 times per game):
  `u = rng.random() or 1e-300`, then `s / temperature - math.log(-math.log(u))` with strict `>`.
  One RNG draw per option, in option order, even for options that cannot win. Do not skip
  draws for speed.
- **H7 — Dict insertion order in the derived fixpoint.** `_compute` iterates
  `list(view.abilities.items())` (view.py:425) and `st.lasting` in list order, and applies
  continuous effects in that order. It caps the fixpoint at 6 rounds (view.py:421) with a
  `same_values` + `new_granted == granted` stop test (view.py:94, :458).
  `trigger_index` orders hosts: active player first, then zones BATTLE, BASE, PAIRED, HAND,
  TRASH, then list order (view.py:320-331). A Rust port that uses a `HashMap` for speed changes
  effect application order. Use insertion-ordered maps or `Vec`s.
- **H8 — `id()` and `hash()` on the hot path are lookup-only.** `slot = {id(child): i …}`
  (mcts.py:133, :145) maps children to rows. `functools.cache` on `knobs_for`
  (policy.py:63-64) hashes a frozen `SearchConfig`. Neither affects any iteration order, so
  Rust may use indices. Keep it that way.
- **H9 — Wall clock is read only for reporting.** `time.perf_counter` and `datetime.now` appear
  in runner/run.py:29-38 and :122-131, and the values reach only `timing.json`
  (reports/writer.py:60-73). Exclude `timing.json` from byte-identical comparison. Everything
  in this document was timed from scripts outside the repo.
- **H10 — Seeds exceed `i64`.** The game seed 9194479550923448490 and the agent seed
  17449794008030232776 are above 2^63, so use `u64` end to end. Python ints are unbounded
  (runner/config.py:59 checks `0 <= seed < 2**64`).
- **H11 — Profiling caveats, not semantics.** The in-process sampler lowers
  `sys.setswitchinterval` to 0.5 ms and samples only at eval-breaker points. The wrapper run
  monkeypatches module globals by identity (it finds every `from x import f` alias). Neither
  changed the game (identical digests, §1.2). The wrapper timings of parents of very frequent
  callees are inflated by about 342 ns per child call.

---

## Open questions

- **Q1.** The out-of-process sampler (`python -m profiling.sampling`, "Tachyon") needs root on
  macOS ("Unable to access process memory"), so it was not run. Its native-frame mode
  (`--native`) would split time inside C builtins (dict/list/isinstance: 13% of cProfile
  tottime) more exactly. It could be run with `sudo -E` or on Linux.
- **Q2.** Only one game, one seed, and one deck pair were profiled (plus `fast` for the same
  seed). Shares should be stable (docs/AI.md reports the same 80% engine share), but other
  decks, with more lasting effects and triggers, could raise the `derived` and interp shares.
  `strong` was not profiled.
- **Q3.** §7 shows the derived cache is transparent for one game. Whether every game is (i.e.
  whether any engine mutation skips `touch()` while changing a view input outside
  `_view_key`) needs the engine-wide audit in inventories 05/06. A fuzz run with `nocache.py`
  across many seeds would test it empirically.
- **Q4.** The split of in-game `_compute` cost between the fast path (view.py:417-419) and the
  fixpoint (rounds per call) was not measured. Counting rounds per call would show whether
  incremental invalidation (deferred in docs/AI.md:500-502) is worth doing in Rust.
- **Q5.** No Rust per-call targets are set here. Python's per-call costs (clone 17–21 µs,
  recompute 3.5–14 µs, about 0.86 ms per MCTS iteration) are the reference to beat.

---

## Appendix: the profiling scripts (verbatim)

All files are in `/private/tmp/claude-501/p0-prof/`. `run_game.py` imports its siblings, so
run it by absolute path (see §1.3).

### `common.py`

```python
"""Shared setup for the gcg-sim profiling scripts (scratch, not part of the repo)."""

from __future__ import annotations

import json
import time
from pathlib import Path

from gcg_sim.deck import load_deck
from gcg_sim.effects.registry import get_registry
from gcg_sim.runner.config import BenchmarkConfig
from gcg_sim.runner.game import play_game
from gcg_sim.runner.records import GameRecord

REPO = Path("/Users/drone/dev/src/personal/gcg-simulator-v1")
DUT_DECK = REPO / "examples/decks/red-green-zeon.txt"
BENCH_DECK = REPO / "examples/decks/blue-white-federation.txt"


def make_config(preset: str, seed: int) -> BenchmarkConfig:
    """Exactly what ``gcg-sim benchmark DUT BENCH --matches 1 --format bo1 --seed S
    --ai-preset P`` builds (cli.cmd_benchmark), minus the report directory."""
    return BenchmarkConfig(
        deck_under_test=load_deck(DUT_DECK),
        benchmark_deck=load_deck(BENCH_DECK),
        matches=1,
        seed=seed,
        workers=1,
        fmt="bo1",
        ai_preset=preset,
    )


def warm() -> float:
    """Load the card DB and compile the registry before timing (one-off process cost)."""
    t0 = time.perf_counter()
    get_registry()
    import gcg_sim.ai  # noqa: F401  (the runner imports it lazily on first agent)

    return time.perf_counter() - t0


def play_one(config: BenchmarkConfig) -> GameRecord:
    """Match 0, game 0, chooser None: what run_benchmark -> play_match -> play_game does."""
    return play_game(config, 0, 0, None)


def summary(record: GameRecord, wall: float, warm_s: float, preset: str) -> dict[str, object]:
    return {
        "preset": preset,
        "seed": record.seed,
        "agent_seeds": list(record.agent_seeds),
        "agents": list(record.agents),
        "first_player": record.first_player,
        "winner": record.winner,
        "end_reason": record.end_reason,
        "turns": record.turns,
        "actions": len(record.actions),
        "final_state_sha256": record.final_state_sha256,
        "game_wall_seconds": round(wall, 3),
        "warmup_seconds": round(warm_s, 3),
    }


def write_json(path: Path, obj: object) -> None:
    path.write_text(json.dumps(obj, indent=2) + "\n")
```

### `run_game.py`

```python
"""Play one BO1 game (red-green-zeon vs blue-white-federation, MCTS vs MCTS) in one mode:
plain, cProfile, category wrappers, stack sampler, derived cache disabled, or GC timing.

  uv run --project REPO python /private/tmp/claude-501/p0-prof/run_game.py \
      --mode {plain,cprofile,categories,sample,nocache,gc} --preset standard --seed 1 --out OUTDIR
"""

from __future__ import annotations

import argparse
import cProfile
import json
import time
from pathlib import Path

from common import make_config, play_one, summary, warm, write_json


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", choices=("plain", "cprofile", "categories", "sample", "nocache", "gc"), required=True)
    ap.add_argument("--preset", default="standard")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    warm_s = warm()
    config = make_config(args.preset, args.seed)
    tag = f"{args.preset}-s{args.seed}-{args.mode}"

    if args.mode == "categories":
        import categories

        categories.install()
        t0 = time.perf_counter()
        record = play_one(config)
        wall = time.perf_counter() - t0
        categories.uninstall()
        write_json(args.out / f"{tag}-categories.json", categories.report(wall))
    elif args.mode == "gc":
        import gc

        gc_ns = [0, 0, {0: 0, 1: 0, 2: 0}]
        started = [0]

        def cb(phase: str, info: dict) -> None:
            if phase == "start":
                started[0] = time.perf_counter_ns()
            else:
                gc_ns[0] += time.perf_counter_ns() - started[0]
                gc_ns[1] += 1
                gc_ns[2][info["generation"]] += 1

        gc.callbacks.append(cb)
        t0 = time.perf_counter()
        record = play_one(config)
        wall = time.perf_counter() - t0
        gc.callbacks.remove(cb)
        print(f"gc: {gc_ns[1]} collections {gc_ns[2]}, {gc_ns[0] / 1e9:.3f}s "
              f"({100 * gc_ns[0] / 1e9 / wall:.2f}% of {wall:.3f}s); thresholds {gc.get_threshold()}")
    elif args.mode == "nocache":
        import nocache

        nocache.install()
        t0 = time.perf_counter()
        record = play_one(config)
        wall = time.perf_counter() - t0
        print(f"derived calls (all recomputed): {nocache.calls}")
    elif args.mode == "sample":
        import sampler

        t0 = time.perf_counter()
        with sampler.Sampler(0.001):
            record = play_one(config)
        wall = time.perf_counter() - t0
        write_json(args.out / f"{tag}-sample.json", sampler.report(wall))
        sampler.dump_collapsed(str(args.out / f"{tag}-collapsed.txt"))
    elif args.mode == "cprofile":
        prof = cProfile.Profile()
        t0 = time.perf_counter()
        prof.enable()
        record = play_one(config)
        prof.disable()
        wall = time.perf_counter() - t0
        prof.dump_stats(str(args.out / f"{tag}.pstats"))
    else:
        t0 = time.perf_counter()
        record = play_one(config)
        wall = time.perf_counter() - t0

    info = summary(record, wall, warm_s, args.preset)
    write_json(args.out / f"{tag}-summary.json", info)
    (args.out / f"{tag}-actions.json").write_text(
        json.dumps([list(a) for a in record.actions]) + "\n"
    )
    print(json.dumps(info))


if __name__ == "__main__":
    main()
```

### `categories.py`

```python
"""Category timers: wrap selected gcg_sim functions so every nanosecond of the game is charged
to the innermost wrapped function on the stack (exclusive) and to the outermost activation of
each category (inclusive). The wrappers only time and call through; game results are unchanged
(checked by comparing final_state_sha256 with the plain run).
"""

from __future__ import annotations

import sys
import time
from collections import defaultdict
from typing import Any

import gcg_sim.ai.actions as actions
import gcg_sim.ai.evaluation as evaluation
import gcg_sim.ai.mcts as mcts
import gcg_sim.ai.noop as noop
import gcg_sim.ai.playout as playout
import gcg_sim.ai.policy as policy
import gcg_sim.ai.setup as setup
import gcg_sim.engine.battle as battle
import gcg_sim.engine.core as core
import gcg_sim.engine.game as game
import gcg_sim.engine.interp as interp
import gcg_sim.engine.observe as observe
import gcg_sim.engine.view as view
import gcg_sim.runner.digest as digest
from gcg_sim.engine.state import GameState

pc = time.perf_counter_ns

TARGETS: list[tuple[str, Any, str]] = [
    ("engine.apply", game, "apply"),
    ("engine.advance", game, "advance"),
    ("engine.rules_management", core, "rules_management"),
    ("engine.interp.run_top_frame", interp, "run_top_frame"),
    ("engine.interp.resume", interp, "resume"),
    ("engine.legal.main_options", game, "main_options"),
    ("engine.legal.action_step_options", game, "action_step_options"),
    ("engine.legal.eligible_blockers", battle, "eligible_blockers"),
    ("state.clone", GameState, "clone"),
    ("observe.determinize", observe, "determinize"),
    ("view.derived", view, "derived"),
    ("view._compute", view, "_compute"),
    ("policy.choose", policy, "choose"),
    ("policy.option_scores", policy, "option_scores"),
    ("evaluation.score", evaluation, "score"),
    ("evaluation.burst_density", evaluation, "burst_density"),
    ("ai.actions.canonical", actions, "canonical"),
    ("ai.actions.prune_dominated", actions, "prune_dominated"),
    ("ai.actions.merge_equivalent", actions, "merge_equivalent"),
    ("ai.noop.without_effect", noop, "without_effect"),
    ("ai.setup.mulligan_choice", setup, "mulligan_choice"),
    ("ai.setup.first_player_choice", setup, "first_player_choice"),
    ("mcts.search", mcts, "search"),
    ("mcts._select", mcts, "_select"),
    ("mcts._add_children", mcts, "_add_children"),
    ("mcts._iterate", mcts, "_iterate"),
    ("mcts._descend", mcts, "_descend"),
    ("mcts._leaf_value", mcts, "_leaf_value"),
    ("mcts._settled", mcts, "_settled"),
    ("mcts._close_call", mcts, "_close_call"),
    ("mcts._world", mcts, "_world"),
    ("playout.play_until_turn", playout, "play_until_turn"),
    ("playout.play_to_end", playout, "play_to_end"),
    ("runner.state_digest", digest, "state_digest"),
]

excl: dict[str, int] = defaultdict(int)
incl: dict[str, int] = defaultdict(int)
calls: dict[str, int] = defaultdict(int)
depth: dict[str, int] = defaultdict(int)
stack: list[list[int]] = []
_patched: list[tuple[Any, str, Any]] = []


def _wrap(cat: str, fn: Any) -> Any:
    def wrapper(*a: Any, **k: Any) -> Any:
        frame = [0]
        stack.append(frame)
        depth[cat] += 1
        t0 = pc()
        try:
            return fn(*a, **k)
        finally:
            dt = pc() - t0
            depth[cat] -= 1
            stack.pop()
            excl[cat] += dt - frame[0]
            calls[cat] += 1
            if depth[cat] == 0:
                incl[cat] += dt
            if stack:
                stack[-1][0] += dt

    wrapper.__wrapped__ = fn  # type: ignore[attr-defined]
    return wrapper


def install() -> None:
    for cat, owner, name in TARGETS:
        orig = owner.__dict__[name] if isinstance(owner, type) else getattr(owner, name)
        w = _wrap(cat, orig)
        if isinstance(owner, type):
            _patched.append((owner, name, orig))
            setattr(owner, name, w)
            continue
        for mod_name, mod in list(sys.modules.items()):
            if not mod_name.startswith("gcg_sim") or mod is None:
                continue
            for attr, val in list(vars(mod).items()):
                if val is orig:
                    _patched.append((mod, attr, orig))
                    setattr(mod, attr, w)


def uninstall() -> None:
    for owner, attr, orig in reversed(_patched):
        setattr(owner, attr, orig)
    _patched.clear()


def _overhead_ns() -> float:
    def noop_fn() -> None:
        return None

    w = _wrap("__calibration__", noop_fn)
    n = 200_000
    t0 = pc()
    for _ in range(n):
        noop_fn()
    base = pc() - t0
    t0 = pc()
    for _ in range(n):
        w()
    wrapped = pc() - t0
    for d in (excl, incl, calls, depth):
        d.pop("__calibration__", None)
    return (wrapped - base) / n


def report(wall: float) -> dict[str, Any]:
    wall_ns = wall * 1e9
    accounted = sum(excl.values())
    rows = []
    for cat, _, _ in TARGETS:
        rows.append(
            {
                "category": cat,
                "calls": calls.get(cat, 0),
                "exclusive_s": round(excl.get(cat, 0) / 1e9, 3),
                "exclusive_pct": round(100 * excl.get(cat, 0) / wall_ns, 2),
                "inclusive_s": round(incl.get(cat, 0) / 1e9, 3),
                "inclusive_pct": round(100 * incl.get(cat, 0) / wall_ns, 2),
            }
        )
    return {
        "wall_s": round(wall, 3),
        "unwrapped_remainder_s": round((wall_ns - accounted) / 1e9, 3),
        "unwrapped_remainder_pct": round(100 * (wall_ns - accounted) / wall_ns, 2),
        "wrapper_overhead_ns_per_call": round(_overhead_ns(), 1),
        "total_wrapped_calls": sum(calls.values()),
        "rows": rows,
    }
```

### `sampler.py`

```python
"""In-process stack sampler (the out-of-process ``profiling.sampling`` needs root on macOS).

A daemon thread wakes every ``interval`` seconds, takes the GIL (the switch interval is lowered
so the main thread hands it over within ~0.5 ms), and records the main thread's Python stack.
Samples land at eval-breaker points of the main thread, so time inside a C call is charged to
the Python frame that made it. Each stack is classified into one category by its innermost
categorised frame (exclusive shares) and by every category present (inclusive shares).
"""

from __future__ import annotations

import sys
import threading
import time
from collections import Counter
from typing import Any

SRC = "/src/gcg_sim/"

# (file relative to src/gcg_sim, function name) -> category. Innermost match wins.
CATEGORY_OF: dict[tuple[str, str], str] = {
    ("engine/game.py", "apply"): "engine apply/advance (other engine work)",
    ("engine/game.py", "advance"): "engine apply/advance (other engine work)",
    ("engine/game.py", "new_game"): "engine apply/advance (other engine work)",
    ("engine/game.py", "main_options"): "legal-action generation",
    ("engine/game.py", "action_step_options"): "legal-action generation",
    ("engine/battle.py", "eligible_blockers"): "legal-action generation",
    ("engine/view.py", "derived"): "view.derived (cache check + recompute)",
    ("engine/state.py", "clone"): "GameState.clone",
    ("engine/observe.py", "determinize"): "determinize (excl. clone)",
    ("ai/policy.py", "choose"): "policy (choose/option_scores)",
    ("ai/policy.py", "option_scores"): "policy (choose/option_scores)",
    ("ai/evaluation.py", "score"): "evaluation (score/evaluate)",
    ("ai/evaluation.py", "evaluate"): "evaluation (score/evaluate)",
    ("ai/evaluation.py", "burst_density"): "evaluation (score/evaluate)",
    ("ai/actions.py", "canonical"): "AI move canonicalization/pruning",
    ("ai/actions.py", "prune_dominated"): "AI move canonicalization/pruning",
    ("ai/actions.py", "merge_equivalent"): "AI move canonicalization/pruning",
    ("ai/noop.py", "without_effect"): "AI no-effect filter (excl. nested)",
    ("ai/setup.py", "mulligan_choice"): "AI setup (mulligan/first player, excl. nested)",
    ("ai/setup.py", "first_player_choice"): "AI setup (mulligan/first player, excl. nested)",
    ("ai/mcts.py", "search"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_select"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_add_children"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_iterate"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_descend"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_leaf_value"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_world"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_settled"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_close_call"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/mcts.py", "_result"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/playout.py", "play_until_turn"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/playout.py", "play_to_end"): "MCTS bookkeeping (mcts.py + playout loop)",
    ("ai/agents.py", "choose"): "agent/runner glue",
    ("runner/game.py", "_drive"): "agent/runner glue",
    ("runner/game.py", "play_game"): "agent/runner glue",
}
OTHER = "outside gcg_sim categories"

stacks: Counter[tuple[tuple[str, str], ...]] = Counter()


def _rel(path: str) -> str:
    return path.split(SRC, 1)[1] if SRC in path else path.rsplit("/", 1)[-1]


def _run(main_id: int, interval: float, stop: threading.Event) -> None:
    sleep = time.sleep
    frames = sys._current_frames
    while not stop.is_set():
        sleep(interval)
        f = frames().get(main_id)
        out = []
        while f is not None:
            code = f.f_code
            out.append((code.co_filename, code.co_name))
            f = f.f_back
        stacks[tuple(out)] += 1


class Sampler:
    def __init__(self, interval: float = 0.001) -> None:
        self.interval = interval
        self.stop = threading.Event()
        self.thread = threading.Thread(
            target=_run, args=(threading.main_thread().ident, interval, self.stop), daemon=True
        )
        self.old_switch = sys.getswitchinterval()

    def __enter__(self) -> Sampler:
        sys.setswitchinterval(0.0005)
        self.thread.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop.set()
        self.thread.join()
        sys.setswitchinterval(self.old_switch)


def dump_collapsed(path: str) -> None:
    """Brendan-Gregg collapsed stacks (root first), e.g. for flamegraph.pl or speedscope."""
    with open(path, "w") as fh:
        for stack, n in stacks.most_common():
            frames = ";".join(f"{_rel(p)}:{name}" for p, name in reversed(stack))
            fh.write(f"{frames} {n}\n")


def report(wall: float) -> dict[str, Any]:
    total = sum(stacks.values())
    excl: Counter[str] = Counter()
    incl: Counter[str] = Counter()
    derived_by_parent: Counter[str] = Counter()
    compute_samples = 0
    leaf_fn: Counter[str] = Counter()
    string_parent: Counter[str] = Counter()
    for stack, n in stacks.items():
        cats_inner_first = []
        rel_stack = [(_rel(p), name) for p, name in stack]
        for key in rel_stack:
            cat = CATEGORY_OF.get(key)
            if cat is not None:
                cats_inner_first.append(cat)
        innermost = cats_inner_first[0] if cats_inner_first else OTHER
        excl[innermost] += n
        for cat in set(cats_inner_first):
            incl[cat] += n
        if innermost.startswith("view.derived"):
            parent = next((c for c in cats_inner_first[1:] if not c.startswith("view.derived")), OTHER)
            derived_by_parent[parent] += n
            if any(k == ("engine/view.py", "_compute") for k in rel_stack):
                compute_samples += n
        if rel_stack:
            leaf_fn[f"{rel_stack[0][0]}:{rel_stack[0][1]}"] += n
            if rel_stack[0][0] == "<string>" and len(rel_stack) > 1:
                string_parent[f"{rel_stack[0][1]} <- {rel_stack[1][0]}:{rel_stack[1][1]}"] += n

    def pct(c: Counter[str]) -> list[list[Any]]:
        return [[k, v, round(100 * v / total, 2)] for k, v in c.most_common()]

    return {
        "wall_s": round(wall, 3),
        "samples": total,
        "effective_rate_hz": round(total / wall, 1),
        "exclusive": pct(excl),
        "inclusive": pct(incl),
        "derived_by_enclosing_category": pct(derived_by_parent),
        "derived_recompute_samples": [compute_samples, round(100 * compute_samples / total, 2)],
        "top_leaf_frames": pct(leaf_fn)[:25],
        "dataclass_generated_leaf_by_caller": pct(string_parent)[:15],
    }
```

### `analyze.py`

```python
"""Summarize a cProfile dump of one gcg-sim game.

  uv run --project REPO python /private/tmp/claude-501/p0-prof/analyze.py OUT/standard-s1-cprofile.pstats
"""

from __future__ import annotations

import pstats
import sys
from collections import defaultdict

SRC = "/src/gcg_sim/"

KEY_FUNCS = [
    ("engine/game.py", "apply"),
    ("engine/game.py", "advance"),
    ("engine/game.py", "_turn_step"),
    ("engine/game.py", "_action_step"),
    ("engine/game.py", "main_options"),
    ("engine/game.py", "action_step_options"),
    ("engine/game.py", "_attack_options"),
    ("engine/game.py", "payment_choices"),
    ("engine/battle.py", "eligible_blockers"),
    ("engine/core.py", "rules_management"),
    ("engine/interp.py", "run_top_frame"),
    ("engine/interp.py", "resume"),
    ("engine/state.py", "clone"),
    ("engine/observe.py", "determinize"),
    ("engine/view.py", "derived"),
    ("engine/view.py", "_compute"),
    ("ai/policy.py", "choose"),
    ("ai/policy.py", "option_scores"),
    ("ai/evaluation.py", "score"),
    ("ai/evaluation.py", "role_features"),
    ("ai/evaluation.py", "burst_density"),
    ("ai/actions.py", "canonical"),
    ("ai/actions.py", "prune_dominated"),
    ("ai/actions.py", "merge_equivalent"),
    ("ai/noop.py", "without_effect"),
    ("ai/setup.py", "mulligan_choice"),
    ("ai/setup.py", "first_player_choice"),
    ("ai/agents.py", "choose"),
    ("ai/mcts.py", "search"),
    ("ai/mcts.py", "_select"),
    ("ai/mcts.py", "_add_children"),
    ("ai/mcts.py", "_iterate"),
    ("ai/mcts.py", "_descend"),
    ("ai/mcts.py", "_leaf_value"),
    ("ai/mcts.py", "_world"),
    ("ai/mcts.py", "_settled"),
    ("ai/mcts.py", "_close_call"),
    ("ai/playout.py", "play_until_turn"),
    ("runner/digest.py", "state_digest"),
]


def label(func: tuple[str, int, str]) -> str:
    path, line, name = func
    if SRC in path:
        path = path.split(SRC, 1)[1]
    elif path == "~":
        return f"<builtin> {name}"
    else:
        path = path.rsplit("/", 1)[-1]
    return f"{path}:{line}({name})"


def file_of(func: tuple[str, int, str]) -> str:
    path = func[0]
    if SRC in path:
        return path.split(SRC, 1)[1]
    if path == "~":
        return "<builtins>"
    return "<stdlib/other>"


def main(path: str) -> None:
    st = pstats.Stats(path)
    stats = st.stats  # type: ignore[attr-defined]
    sum_tt = st.total_tt  # type: ignore[attr-defined]
    root = next(f for f in stats if f[2] == "play_game")
    total = stats[root][3]
    print(
        f"play_game cumtime {total:.3f}s (the % base below); sum of tottime {sum_tt:.3f}s; "
        f"{st.total_calls} calls ({st.prim_calls} primitive)"  # type: ignore[attr-defined]
    )

    def table(key: int, title: str) -> None:
        print(f"\n### Top 40 by {title}\n")
        print("| # | ncalls | tottime s | tottime % | cumtime s | cumtime % | function |")
        print("| ---: | ---: | ---: | ---: | ---: | ---: | --- |")
        rows = sorted(stats.items(), key=lambda kv: kv[1][key], reverse=True)[:40]
        for i, (func, (cc, nc, tt, ct, _)) in enumerate(rows, 1):
            calls = f"{nc}" if cc == nc else f"{nc}/{cc}"
            print(
                f"| {i} | {calls} | {tt:.3f} | {100 * tt / total:.1f} | {ct:.3f} | "
                f"{100 * ct / total:.1f} | `{label(func)}` |"
            )

    table(2, "tottime")
    table(3, "cumtime")

    print("\n### Inclusive time of key functions (cumtime; overlapping)\n")
    print("| function | ncalls | cumtime s | % of total | tottime s | us per call (cum) |")
    print("| --- | ---: | ---: | ---: | ---: | ---: |")
    index = defaultdict(list)
    for func in stats:
        if SRC in func[0]:
            index[(func[0].split(SRC, 1)[1], func[2])].append(func)
    for f, name in KEY_FUNCS:
        funcs = index.get((f, name), [])
        if not funcs:
            print(f"| `{f}:{name}` | 0 | - | - | - | - |")
            continue
        for func in funcs:
            cc, nc, tt, ct, _ = stats[func]
            print(
                f"| `{label(func)}` | {nc} | {ct:.3f} | {100 * ct / total:.1f} | {tt:.3f} | "
                f"{1e6 * ct / max(nc, 1):.1f} |"
            )

    print("\n### Exclusive time (tottime) by source file (partition of the sum of tottime; % of play_game cumtime)\n")
    per_file: dict[str, float] = defaultdict(float)
    per_file_calls: dict[str, int] = defaultdict(int)
    for func, (cc, nc, tt, ct, _) in stats.items():
        per_file[file_of(func)] += tt
        per_file_calls[file_of(func)] += nc
    print("| file | tottime s | % | calls |")
    print("| --- | ---: | ---: | ---: |")
    for f, tt in sorted(per_file.items(), key=lambda kv: kv[1], reverse=True):
        print(f"| `{f}` | {tt:.3f} | {100 * tt / total:.1f} | {per_file_calls[f]} |")

    print("\n### Exclusive time by package (engine / effects / ai / runner / builtins)\n")
    per_pkg: dict[str, float] = defaultdict(float)
    for f, tt in per_file.items():
        per_pkg[f if f.startswith("<") or "/" not in f else f.split("/", 1)[0]] += tt
    print("| package | tottime s | % |")
    print("| --- | ---: | ---: |")
    for p, tt in sorted(per_pkg.items(), key=lambda kv: kv[1], reverse=True):
        print(f"| `{p}` | {tt:.3f} | {100 * tt / total:.1f} |")

    dfunc = index[("engine/view.py", "derived")][0]
    by_pkg: dict[str, list[float]] = defaultdict(lambda: [0, 0.0])
    for caller, (cc, nc, tt, ct) in stats[dfunc][4].items():
        pkg = file_of(caller).split("/", 1)[0]
        by_pkg[pkg][0] += nc
        by_pkg[pkg][1] += ct
    print("\n### `view.derived` calls and cumtime by caller package\n")
    print("| caller package | calls | cumtime s | % of total |")
    print("| --- | ---: | ---: | ---: |")
    for pkg, (nc, ct) in sorted(by_pkg.items(), key=lambda kv: -kv[1][1]):
        print(f"| `{pkg}` | {nc} | {ct:.3f} | {100 * ct / total:.1f} |")

    for f, name in (("engine/view.py", "derived"), ("engine/view.py", "_compute"),
                    ("engine/state.py", "clone"), ("engine/game.py", "apply")):
        funcs = index.get((f, name), [])
        if not funcs:
            continue
        func = funcs[0]
        callers = stats[func][4]
        print(f"\n### Direct callers of `{label(func)}` (edge cumtime)\n")
        print("| caller | ncalls | cumtime s | % of total |")
        print("| --- | ---: | ---: | ---: |")
        for caller, (cc, nc, tt, ct) in sorted(callers.items(), key=lambda kv: kv[1][3], reverse=True)[:15]:
            print(f"| `{label(caller)}` | {nc} | {ct:.3f} | {100 * ct / total:.1f} |")


if __name__ == "__main__":
    main(sys.argv[1])
```

### `micro.py`

```python
"""Per-call costs of hot operations on mid-game states of the profiled game.

  uv run --project REPO python /private/tmp/claude-501/p0-prof/micro.py OUT/standard-s1-plain-actions.json

The game is re-created with runner.game.replay_game from its seed and recorded actions; the
probe states are the first MAIN decisions (more than one option) at or after turns 6, 10, 16.
"""

from __future__ import annotations

import json
import statistics
import sys
import time
from typing import Any, Callable

from common import make_config, warm

from gcg_sim.ai.actions import canonical
from gcg_sim.ai.config import PRESETS
from gcg_sim.ai.evaluation import DEFAULT_WEIGHTS, burst_density, score
from gcg_sim.ai.playout import play_until_turn
from gcg_sim.ai.policy import choose, knobs_for, option_scores
from gcg_sim.engine import view as V
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import DecisionKind, Zone
from gcg_sim.rng import SplitMix64
from gcg_sim.runner.config import game_seed, seat_decks

REPEATS = 5
CFG = PRESETS["standard"]
KNOBS = knobs_for(CFG)


def bench(fn: Callable[[], Any], n: int) -> dict[str, float]:
    fn()
    per_call = []
    for _ in range(REPEATS):
        t0 = time.perf_counter_ns()
        for _ in range(n):
            fn()
        per_call.append((time.perf_counter_ns() - t0) / n)
    return {
        "n_per_repeat": n,
        "min_us": round(min(per_call) / 1000, 3),
        "median_us": round(statistics.median(per_call) / 1000, 3),
    }


def probe_states(actions: list[list[Any]], turns: tuple[int, ...]) -> list[GameState]:
    config = make_config("standard", 1)
    st = new_game(
        seat_decks(config, 0),
        game_seed(config.seed, 0, 0),
        chooser=None,
        turn_limit=config.turn_limit,
        max_actions=config.max_actions,
    )
    wanted = list(turns)
    out = []
    for a in actions:
        dec = st.pending
        if (
            wanted
            and dec is not None
            and st.turn >= wanted[0]
            and dec.kind is DecisionKind.MAIN
            and len(dec.options) > 1
        ):
            out.append(st.clone())
            wanted.pop(0)
        apply(st, Action.from_json(a))
    return out


def describe(st: GameState) -> dict[str, Any]:
    zones = {
        z.name: [len(st.zones[p][z]) for p in (0, 1)]
        for z in (Zone.DECK, Zone.HAND, Zone.BATTLE, Zone.SHIELD, Zone.BASE, Zone.RESOURCE_AREA, Zone.TRASH)
    }
    dec = st.pending
    assert dec is not None
    return {
        "turn": st.turn,
        "active": st.active,
        "decision": dec.kind.value,
        "player": dec.player,
        "options": len(dec.options),
        "cards": len(st.cards),
        "lasting": len(st.lasting),
        "history": len(st.history),
        "zones[p0,p1]": zones,
    }


def measure(st: GameState) -> dict[str, Any]:
    dec = st.pending
    assert dec is not None
    player = dec.player
    density = burst_density(st, player)
    V.derived(st)
    out: dict[str, Any] = {"state": describe(st)}

    out["GameState.clone()"] = bench(st.clone, 20_000)
    out["view.derived() cache hit"] = bench(lambda: V.derived(st), 200_000)

    def recompute() -> None:
        st._derived = None
        V.derived(st)

    out["view.derived() recompute (st._derived=None first)"] = bench(recompute, 5_000)
    V.derived(st)
    out["observe.determinize()"] = bench(lambda: determinize(st, player, 12345), 2_000)
    out["determinize() + first derived()"] = bench(
        lambda: V.derived(determinize(st, player, 12345)), 2_000
    )
    out["ai.actions.canonical(MAIN)"] = bench(lambda: canonical(st, dec), 20_000)
    out["policy.option_scores(MAIN) (derived cached)"] = bench(
        lambda: option_scores(st, dec, KNOBS), 10_000
    )
    out["evaluation.score() (derived cached)"] = bench(
        lambda: score(st, player, DEFAULT_WEIGHTS, density), 10_000
    )
    move = choose(st, None, 0.0, KNOBS)

    def clone_apply() -> None:
        s = st.clone()
        apply(s, move, check=False)

    out[f"clone() + apply({move}) to next decision"] = bench(clone_apply, 2_000)
    horizon = st.turn + CFG.horizon_turns
    seeds = [SplitMix64(7).next_u64() + i for i in range(200)]

    def iteration() -> None:
        for s_ in seeds:
            w = determinize(st, player, s_)
            apply(w, move, check=False)
            play_until_turn(
                w, horizon, SplitMix64(s_), CFG.rollout_temperature, CFG.max_rollout_decisions, KNOBS
            )

    r = bench(iteration, 1)
    out["one rollout iteration: determinize + apply + play_until_turn(horizon 2 turns)"] = {
        "n_per_repeat": len(seeds),
        "min_us": round(r["min_us"] / len(seeds), 1),
        "median_us": round(r["median_us"] / len(seeds), 1),
    }
    return out


def main(path: str) -> None:
    warm()
    actions = json.loads(open(path).read())
    states = probe_states(actions, (6, 10, 16))
    print(json.dumps([measure(st) for st in states], indent=2))


if __name__ == "__main__":
    main(sys.argv[1])
```

### `nocache.py`

```python
"""Replace view.derived with an always-recompute version (cache disabled) to check that the
derived-view cache is semantically transparent for a given game."""

from __future__ import annotations

import sys

import gcg_sim.engine.view as view

_orig = view.derived
calls = 0


def derived_nocache(st):  # type: ignore[no-untyped-def]
    global calls
    calls += 1
    dv = view._compute(st)
    dv.key = view._view_key(st)
    st._derived = dv
    return dv


def install() -> None:
    for name, mod in list(sys.modules.items()):
        if not name.startswith("gcg_sim") or mod is None:
            continue
        for attr, val in list(vars(mod).items()):
            if val is _orig:
                setattr(mod, attr, derived_nocache)
```
