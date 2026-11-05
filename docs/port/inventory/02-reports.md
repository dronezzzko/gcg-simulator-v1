# 02 — Reports and output files (Python reference inventory)

Scope: `src/gcg_sim/reports/*` (`results.py`, `schema/results.v1.schema.json`, `summary.py`,
`aggregate.py`, `stats.py`, `hypotheses.py`, `versions.py`, `writer.py`) and, as they feed output
files, `src/gcg_sim/runner/{records,instrument,replayfile,digest}.py`, plus the glue in
`runner/{game,match,run,config,agents}.py` and `cli.py`. All paths below are relative to
`src/gcg_sim/` unless they start with `docs/`, `tests/` or `/`.

Everything here was read from the code on branch `rust-port` (HEAD `d9d41af`, clean tree apart
from untracked port files) and checked against one sample run (section 13). Rust-side facts were
checked with rustc 1.99.1 and serde_json 1.0.154 (offline cache) in
`/private/tmp/claude-501/p0-scratch/{rs,sj}`.

---

## 1. Pipeline and write order

1. `cli.cmd_benchmark` (`cli.py:188-229`): `_legal_deck` both decks (a deck's `name` is the file
   stem, its `source` is `str(Path(arg))`, `deck/parse.py:111-114`), `_writable_dir(args.out)`,
   then build `BenchmarkConfig` (`runner/config.py:18-44`). The CLI never sets `turn_limit` or
   `max_actions`, so they are always `200` (`DEFAULT_TURN_LIMIT`, `runner/config.py:15`) and
   `20000` (`MAX_ACTIONS_DEFAULT`, `engine/game.py:33`).
2. `run_benchmark` (`runner/run.py:109-140`) returns `BenchmarkRun(config, matches, timing,
   agent_factory)` (`runner/records.py:198-207`). Matches are always returned in match-index order,
   whatever the worker count (`run.py:106`). `BenchmarkRun.games` is
   `tuple(g for m in matches for g in m.games)`: match order, then game order.
3. `reports.write_reports(run, out_dir)` (`reports/writer.py:94-110`). The order of operations is:
   1. `results = build_results(run)` (`results.py:145-189`).
   2. `validate_results(results)` (`results.py:48-50`). On failure,
      `jsonschema.ValidationError` propagates **uncaught** (the CLI only catches `OSError`,
      `cli.py:210-216`). The result is a traceback, exit code 1, and **no file written**.
   3. `_write_replays` (`writer.py:79-91`), run while the `ReportPaths` is built: it writes each
      selected replay, then deletes stale replay files.
   4. `games.ndjson`, 5. `summary.md`, 6. `timing.json`, 7. `results.json`, which is last
      (`writer.py:106-109`).
4. `write_atomic(path, text)` (`writer.py:37-41`) runs `mkdir -p parent`, writes
   `text.encode("utf-8")` to `parent/.{name}.{os.getpid()}.tmp`, then `os.replace(tmp, path)`. The
   bytes are exact: UTF-8, no BOM, `\n` newlines, no platform newline translation.
5. CLI stdout after success (`cli.py:217-228`):
   ```
   {dut.name} (deck under test) vs {bench.name} (benchmark)
   matches: DUT {dut_wins}-{bench_wins} (draws {draws}), win rate {100*rate:.1f}% [95% CI {100*lo:.1f}%-{100*hi:.1f}%] over {n}
   games: DUT ...same format...
   reports:
     results  {out}/results.json        <- f"  {label:8} {path}" (label left-aligned, width 8)
     summary  {out}/summary.md
     games    {out}/games.ndjson
     timing   {out}/timing.json
     replays  {len(paths.replays)} file(s) in {out}/replays
   ```
   Progress goes to stderr (`cli.py:143-159`). On a TTY it prints `\rmatches {done}/{total}`, with
   a final `\n` when done. Otherwise it prints a line `matches {done}/{total}` whenever
   `10*done//total` changes.

`MatchRecord.to_json` (`records.py:176-184`) is **not** written to any output file.

---

## 2. Serialisation rules shared by every file

### 2.1 `json.dumps` options per file

| File | Call | Options | Trailing |
| --- | --- | --- | --- |
| `results.json` | `to_json_text` `writer.py:33-34` | `indent=2, sort_keys=True, ensure_ascii=False` (default separators with indent: `","` + newline, `": "`) | one `"\n"` |
| `timing.json` | same `to_json_text` | same | one `"\n"` |
| `replays/*.json` | same `to_json_text` (`writer.py:83`) | same | one `"\n"` |
| `games.ndjson` | `writer.py:54-57` | `sort_keys=True, separators=(",", ":")`, **`ensure_ascii=True` (default)** | `"\n"` after every line, including the last |
| `summary.md` | text, `"\n".join(lines)` (`summary.py:230-241`) | n/a | ends in exactly one `"\n"`, because the last section ends with `""` |
| state digest blob | `digest.py:55` | `sort_keys=True, separators=(",", ":")`, `ensure_ascii=True` | none (hashed) |
| `once_used` sort key | `digest.py:48` | `json.dumps(list)`: **default separators `", "` and `": "`** | n/a |
| deck digest blob | `deck/parse.py:128` | `sort_keys=True, separators=(",", ":")` | none (hashed) |

- **Key order.** Every JSON file sorts keys recursively by Python `str` ordering (code points).
  Rust `String` `Ord` (UTF-8 byte order) gives the same order. Dict insertion order in the Python
  builders is therefore irrelevant to the output. **The workspace `Cargo.toml` enables
  serde_json's `preserve_order`.** Verified in the scratch crate: `json!({"b":1,"a":[],"c":{}})`
  prints in `b, a, c` order. Rust must therefore sort keys explicitly, using a `BTreeMap` or a
  recursive sort.
- **Pretty layout.** Python `indent=2` output matches serde_json `to_string_pretty`: 2-space
  indent, `"key": value`, empty list `[]`, empty object `{}`. This was verified in the scratch
  crate. The two differ only in floats (2.2) and escaping (2.3).
- **Integers.** Python ints are unbounded. Seeds go up to `2**64-1`, and in the sample
  `agent_seeds[0] = 15091393010796579730 > i64::MAX`. Use `u64` for `seed`, `agent_seeds`,
  `seeds.master`, `replays[].seed` and `rng` state. Booleans are `true`/`false` and `None` is
  `null`.
- **NaN/Infinity.** `allow_nan` keeps its default of `True`, but no code path in scope can produce
  NaN or Inf. Every division is guarded (`stats.py:33,37-39,53,57`; `writer.py:72-73`).

### 2.2 Float text = CPython `float.__repr__`

`json.dumps` writes floats with `float.__repr__`. f-strings like `f"{x}"` or `str(x)` use the same
repr: `summary.py:176` (alpha) and `summary.py:196` (p-value). The rules are:
- Use the shortest round-trip digits.
- Use exponent form iff the decimal exponent `e` (for `d.ddd × 10^e`) is `< -4` or `>= 16`.
  - The mantissa has a `.` only when it has more than one digit.
  - The exponent always has a sign and at least 2 digits, as in `1e-05`, `1.2e-05`, `5e-06` and
    `1e+16`.
- Otherwise use positional form with at least one fractional digit (`13.0`, `0.0001`,
  `1000000000000000.0`).
- `-0.0` prints as `-0.0`.

Verified outputs:

| value | Python `json.dumps` | Rust `{:?}` (1.99.1) | serde_json 1.0.154 (zmij) |
| --- | --- | --- | --- |
| 0.0001 | `0.0001` | `0.0001` | `0.0001` |
| 1e-05 | `1e-05` | `1e-5` | `0.00001` |
| 1.2e-05 | `1.2e-05` | `1.2e-5` | `0.000012` |
| 3.2e-05 | `3.2e-05` | `3.2e-5` | `0.000032` |
| 5e-06 | `5e-06` | `5e-6` | `5e-6` |
| 13.0 | `13.0` | `13.0` | `13.0` |
| 1e15 | `1000000000000000.0` | same | same |
| 1e16 | `1e+16` | `1e16` | `1e+16` |

Rust `{:?}` has the same digits and the same switch thresholds as Python. Only the exponent
spelling differs: Rust `e-5` against Python `e-05`, and Rust `e16` against Python `e+16`.
serde_json has **different thresholds**. Small p-values such as `3.2e-05` really do occur (see the
section 6.8 sample). The port therefore needs a custom `serde_json::ser::Formatter`, or a
hand-written writer that formats `f64` as `{:?}` and then rewrites the exponent to Python style.

### 2.3 String escaping

Verified with Python 3.15.1:
- **`ensure_ascii=False`** (results, replays, timing) escapes only `"`, `\`, and the control
  characters below U+0020. `\b \f \n \r \t` use their short escapes; the others use `\u00XX`
  with lowercase hex. It does **not** escape `/`, DEL (U+007F), U+2028/9 or any non-ASCII
  character, which is written as raw UTF-8. serde_json behaves identically: `"Zaku Ⅱ\u{7f}"` came
  out raw.
- **`ensure_ascii=True`** (games.ndjson, digest) also escapes every character outside
  `' '..'~'`. That includes **DEL as `\u007f`**. Non-BMP characters become UTF-16 surrogate pairs
  (`😀`), with lowercase hex. serde_json has no such mode, so the port must write it
  by hand. Today games.ndjson only contains ASCII unless `--decision-log` is on. AI log labels
  contain card names like `Zaku Ⅱ`, which come out as `Zaku Ⅱ`.

### 2.4 Rounding: `rnd(x) = round(x, 6)` (`stats.py:9,28-29`)

CPython `round(float, 6)` rounds correctly from the exact binary value. It rounds half to even
only on exact decimal ties, and returns the nearest double to the rounded decimal. In Rust,
`format!("{:.6}", x).parse::<f64>()` gave identical results on every vector tried:

| x | Python `round(x,6)` | Rust `{:.6}` → parse |
| --- | --- | --- |
| 5e-07 | `0.0` | `0.0` |
| 1.5e-06 | `2e-06` | `2e-6` (same value) |
| 2.5e-06 | `3e-06` | `3e-6` |
| 0.1234565 | `0.123456` | `0.123456` |
| 0.1234575 | `0.123457` | `0.123457` |
| 1/3 | `0.333333` | `0.333333` |
| 2/3 | `0.666667` | `0.666667` |
| 0.0045455 | `0.004546` | `0.004546` |

`round()` can return `-0.0`: `round(-4e-07, 6)` is `-0.0` and serialises as `-0.0`. In scope this
cannot happen. Every rounded difference (`delta`, `loss_rate_lift`) is a difference of two values
that were already rounded, so it is either exactly `0.0` or at least `1e-06` in magnitude.

### 2.5 Fixed-precision text (`summary.md`, hypothesis statements, CLI)

`f"{v:.1f}"`, `f"{v:.0f}"` and `f"{v:+.1f}"` round correctly from the exact binary value, with
ties to even. Rust `{:.1}`, `{:.0}` and `{:+.1}` matched on every vector below:

| v | `.1f` | `.0f` | `+.1f` |
| --- | --- | --- | --- |
| 0.25 | 0.2 | 0 | +0.2 |
| 0.35 | 0.3 | 0 | +0.3 |
| 2.5 | 2.5 | 2 | +2.5 |
| 3.5 | 3.5 | 4 | +3.5 |
| 100*0.0005 (=0.05) | 0.1 | 0 | +0.1 |
| 66.65 | 66.7 | 67 | +66.7 |
| 0.665*100 (=66.5 exactly) | 66.5 | **66** | +66.5 |
| 12.25 | 12.2 | 12 | +12.2 |
| 100*0.6125 (=61.25000000000001) | **61.3** | 61 | +61.3 |
| 100*0.793457 | 79.3 | 79 | +79.3 |

Always compute `100.0 * x` in `f64` first and then format, exactly as the Python code does.
Integer or decimal shortcuts give different text (the `61.3` and `12.2` rows).

---

## 3. Records (`runner/records.py`)

Role strings (`records.py:11-13`) are `DUT = "dut"`, `BENCH = "bench"` and `DRAW = "draw"`.
Result labels from the DUT's view (`aggregate.py:20-21`) are `WIN = "win"` and `LOSS = "loss"`,
and `RESULTS = ("win", "loss", "draw")`. This tuple order drives every per-result iteration.

### 3.1 `GameRecord` (`records.py:74-162`), written by `to_json` (`records.py:107-132`)

The table lists fields in declaration order; JSON output re-sorts them by key. All fields are set
in `runner/game.py:139-166`.

| field | type | value |
| --- | --- | --- |
| `match_index` | int | 0-based match number |
| `game_index` | int | 0-based game number in the match |
| `seed` | u64 | `derive_seed(master, "match", m, "game", g)` (`config.py:75-76`) |
| `agent_seeds` | [u64,u64] | `derive_seed(master, "match", m, "game", g, "agent", seat)` for seat 0, 1 (`config.py:79-80`) |
| `agents` | [str,str] | `agent.name` per seat; MCTS gives `f"mcts-{preset}"` (`ai/agents.py:81`) |
| `dut_seat` | int | `match_index % 2` (`config.py:64-66`) |
| `chooser` | int\|null | argument passed to `new_game`: `null` = seeded die roll; else the seat of the previous game's match-scoring loser (`match.py:30-38`, `game.py:54-59`) |
| `chooser_seat` | int | `st.setup_chooser` |
| `first_player` | int | `st.first_player` |
| `dut_on_play` | bool | `st.first_player == dut` |
| `winner` | str | `"draw"` if `st.winner < 0`, else `"dut"` or `"bench"` (`game.py:38-39`) |
| `end_reason` | str | `EndReason` value: `battle_damage`, `deck_out`, `concede`, `both_defeated`, `turn_limit` (`engine/types.py:150-155`) |
| `match_point` | str\|null | `game.py:42-51`: winner role if `st.winner >= 0`; else if `fmt == "bo3"` and `both_defeated`, the role of seat `1 - st.active` (the turn player loses); else `null` |
| `turns` | int | `st.turn` at the end |
| `active_at_end` | int | `st.active` |
| `actions` | [[str,int,int,int]] | each applied action `Action.to_json()` = `[kind.value, a, b, c]` (`engine/state.py:23-24`), in order (`game.py:101-102`) |
| `redraws` | [bool,bool] | `st.redraws[0]`, `st.redraws[1]` |
| `dut_cards` | object | `DutCardEvents.to_json()` (3.2) |
| `bench_seen` | [str] | **`tuple(sorted(observer.bench_seen))`** (`game.py:161`), unique card numbers |
| `final_state_sha256` | str | `state_digest(st)` (section 11) |
| `turn_limit` | int | config value (200 from the CLI) |
| `max_actions` | int | config value (20000 from the CLI) |
| `decision_log` | null\|[obj] | `None` unless `--decision-log`; otherwise the concatenation of both agents' new log entries, in decision order (`game.py:94-97`). The entry format is `ai/base.py:79-100` (AI scope; contains `round(value, 4)` floats). |

`from_json` (`records.py:134-162`) is the inverse. Python `int()` and `bool()` coercions accept
looser input than Rust would.

### 3.2 `DutCardEvents` (`records.py:34-71`) and `PlayEvent` (`records.py:18-31`)

| key | value |
| --- | --- |
| `initial_hand` | sorted card numbers of the DUT hand **before** its REDRAW decision |
| `redrew` | `action.kind is REDRAW` at the DUT's REDRAW decision |
| `opening_hand` | sorted card numbers of the DUT hand right **after** that decision |
| `drawn` | sorted card numbers, one per uid that entered tracking (3.3) |
| `played` | chronological list of `[card_number, turn, own_turn]` (**not sorted**) |
| `never_played` | sorted card numbers of `drawn` uids that are not in `played_uids` |
| `in_hand_at_end` | sorted card numbers of the DUT hand at game end |

`own_turn(turn, seat, first_player)` is `(turn + 1) // 2` if `seat == first_player`, else
`turn // 2` (`config.py:83-85`; turns are at least 0, so floor division is safe).

### 3.3 `GameObserver` (`runner/instrument.py:32-107`)

The observer sees only the real game. `_drive` calls `before` and then `after` around every
`apply` (`game.py:98-100`). Its constants are:
- `PLAY_KINDS = {PLAY_UNIT, PLAY_BASE, PLAY_COMMAND, PAIR}`
- `IN_PLAY_ZONES = (BATTLE, PAIRED, BASE, RESOLVING)`, in that order
- `DRAW_EVENTS = {"draw", "shield_to_hand"}`
- `SEEN_EVENTS = {"play", "deployed", "paired", "burst_revealed", "command_activated"}`

These are defined at `instrument.py:20-25`.

- `before(st, decision, action)` (`49-60`):
  1. If `decision.kind is REDRAW and decision.player == dut`, set
     `initial_hand = tuple(hand uids)`.
  2. If `action.kind in PLAY_KINDS`, let `number = card_number(st, action.a)`. For PAIR, `a` is
     the pilot-capable card.
     - If `decision.player == dut`, append `PlayEvent(number, st.turn, own_turn(...))` and add
       `action.a` to `played_uids`.
     - Otherwise call `_see_bench(st, action.a)`.
- `after(...)` (`62-73`):
  1. If this was the DUT's REDRAW decision, set `redrew`, `opening_hand = hand` and
     `tracking = True`.
  2. Call `_scan_history`.
  3. If tracking, run `drawn.setdefault(uid)` for each uid in the DUT hand, in zone order.
  4. For each zone in `IN_PLAY_ZONES` and each uid in that zone of the bench seat, call
     `_see_bench`.
- `_see_bench(uid)`: add `cdef.card_number` to `bench_seen` iff
  `st.cards[uid].owner == bench and not cdef.is_token`. `is_token` is true for `UNIT TOKEN`,
  `EX BASE` and `EX RESOURCE` (`cards/model.py:21-22`).
- `_scan_history` (`80-92`):
  1. **If `st.history is not self._history` (identity test), restart at index 0.** The engine
     *replaces* the list at turn start (`engine/game.py:190`, `engine/game.py:336`).
  2. For each new event, skip it if `h.uid < 0`. Otherwise let `owner = st.cards[h.uid].owner`
     and:
     - if `tracking` and the kind is in `DRAW_EVENTS` and `owner == dut`, run
       `drawn.setdefault(uid)`;
     - elif the kind is in `SEEN_EVENTS` and `owner == bench`, call `_see_bench`.
- `drawn` is a `dict[int, None]` used as an ordered set. Its order is irrelevant because the
  output is sorted by card number. `played_uids` and `bench_seen` are sets, used only for
  membership tests and sorted output.

### 3.4 `MatchRecord`, `RunTiming`, `BenchmarkRun`

- `MatchRecord.winner` (`match.py:11-16`) is `"dut"` if `dut_points > bench_points`, `"bench"`
  if `bench_points > dut_points`, and `"draw"` otherwise.
- A match stops when `max(points.values()) >= wins_needed` (`match.py:36`).
- `RunTiming` (`records.py:187-195`) has `started_at`, `finished_at`, `wall_seconds`,
  `match_seconds` and `workers`. It goes only into timing.json.

---

## 4. Statistics helpers (`reports/stats.py`)

Every operation below is on IEEE `f64`, in exactly this order.

- **`wilson(s, n, z=1.96)`** (`12-25`):
  1. Raise `ValueError` unless `n >= 0` and `0 <= s <= max(n, 0)`. If `n == 0`, return
     `(0.0, 1.0)`.
  2. Compute:
     ```
     p = s / n                       # Python int/int true division (correctly rounded)
     z2 = z * z                      # 1.96*1.96 in f64
     denom = 1 + z2 / n
     centre = (p + z2 / (2 * n)) / denom          # 2*n is an int product
     half = z * sqrt(p * (1 - p) / n + z2 / (4 * n * n)) / denom   # (p*(1-p))/n ; 4*n*n int
     lo = 0.0 if s == 0 else max(0.0, centre - half)
     hi = 1.0 if s == n else min(1.0, centre + half)
     ```
  3. `max(0.0, v)` returns `v` iff `v > 0.0`, else `0.0`. `min(1.0, v)` returns `v` iff
     `v < 1.0`, else `1.0`. Implement these comparisons literally.
  4. Sample values: `wilson(0,1) = (0.0, 0.7934567085261071)` and
     `wilson(1,1) = (0.20654329147389294, 1.0)`, which round to `0.793457` and `0.206543`.
- **`rate(s, n)`** (`36-41`): if `n == 0`, return
  `{"successes": s, "n": 0, "rate": None, "ci95": None}`. Otherwise return
  `{"successes": s, "n": n, "rate": rnd(s/n), "ci95": [rnd(lo), rnd(hi)]}`.
- **`ratio(num, den)`** (`32-33`): `rnd(num/den)` if `den` is truthy, else `None`.
- **`distribution(values)`** (`81-101`) on ints:
  1. If empty, return all of `n=0, mean, median, min, max, p10, p90 = None`.
  2. Otherwise `s = sorted(values)`, and:
     - `mean = rnd(sum(s)/len(s))`. The sum is an exact int; the division is correctly rounded.
     - `median = percentile(s, 0.5)` and `p10` / `p90 = percentile(s, 0.1 / 0.9)`.
     - `min = s[0]` and `max = s[-1]`.
- **`percentile(sorted, q)`** (`75-78`) is nearest rank:
  `rank = max(1, ceil(q * len))` computed in **f64** (`q*len` is a float multiply), then return
  `sorted[rank-1]`. So `median` is the **lower** median for even `n`: `distribution([1,4])`
  gives `median = 1`. No float-versus-exact divergence was found for `q ∈ {0.1, 0.5, 0.9}` and
  `n ≤ 200000` (checked in Python), but Rust should still compute `(q * n as f64).ceil()`.
- **`intervals_overlap(a, b)`** (`44-47`): `True` if either `ci95` is `None`, else
  `a.lo <= b.hi and b.lo <= a.hi`. It uses the **rounded** CI values.
- **`two_proportion_p(a, b)`** (`50-60`) is a pooled two-proportion z-test, two-sided:
  1. Return `1.0` if `n1 == 0 or n2 == 0`.
  2. `pooled = (s1+s2)/(n1+n2)` and `se = sqrt(pooled*(1-pooled)*(1/n1 + 1/n2))`. Return `1.0`
     if `se == 0`.
  3. `z = abs(s1/n1 - s2/n2) / se` (raw counts, not the rounded rates), and the result is
     `erfc(z / sqrt(2))`.
  4. `math.erfc` is the **platform libm** `erfc`. On this Mac, 200,000 random arguments in
     [0,8) gave results bit-identical to `ctypes` `libm.erfc`. The Rust port should call the
     same C `erfc` through FFI, not the `libm` crate.
- **`holm(p_values, alpha)`** (`63-72`) is Holm-Bonferroni:
  1. Sort the indices by `(p_i, i)`.
  2. Walk them in that order, with `rank` counting from 0. Stop at the first index where
     `p_i > alpha / (m - rank)` (an f64 division). Every earlier index passes.
  3. **Equality passes.** Sample: `holm([0.01,0.04,0.03], 0.05) = [True, False, False]`.

---

## 5. `games.ndjson`

- One line per `GameRecord` in `run.games` order: match index, then game index
  (`writer.py:54-57`).
- Each line is
  `json.dumps(g.to_json(), sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n"`.
- The line-level key order is `actions, active_at_end, agent_seeds, agents, bench_seen, chooser,
  chooser_seat, decision_log, dut_cards, dut_on_play, dut_seat, end_reason, final_state_sha256,
  first_player, game_index, match_index, match_point, max_actions, redraws, seed, turn_limit,
  turns, winner`.
- The `dut_cards` key order is `drawn, in_hand_at_end, initial_hand, never_played, opening_hand,
  played, redrew`.
- Round trip: `GameRecord.from_json(json.loads(line))` re-serialises to the identical line
  (checked on the sample).
- Any record can be replayed with `runner.replay(record, config)` (`game.py:185-194`).

---

## 6. `results.json`

Built by `build_results` (`results.py:145-189`). Every block below is written with sorted keys.

### 6.1 Top level

The keys, as built: `schema, schema_version, versions, config, seeds, ai, decks, results,
splits, game_length, end_reasons, draws, mulligan, cards, benchmark_cards, hypotheses,
hypothesis_thresholds, conflict_resolutions, replays, notes`.

The output order is sorted: `ai, benchmark_cards, cards, config, conflict_resolutions, decks,
draws, end_reasons, game_length, hypotheses, hypothesis_thresholds, mulligan, notes, replays,
results, schema, schema_version, seeds, splits, versions`.

| key | content |
| --- | --- |
| `schema` | `SCHEMA_ID = "urn:gcg-sim:schema:results:1.1.0"` (`versions.py:16-17`) |
| `schema_version` | `"1.1.0"` |
| `versions` | 6.2 |
| `config` | `{"matches", "format": fmt, "max_games_per_match": 3\|1, "wins_needed": 2\|1, "turn_limit", "max_actions", "replays_per_result": K}` (`results.py:109-119`). No seed, workers, out_dir or preset here. |
| `seeds` | `results.py:133-142`, constant strings except `master`. See the verbatim list after this table. |
| `ai` | `{"factory": run.agent_factory, "preset", "dut_agent": games[0].agents[games[0].dut_seat], "bench_agent": games[0].agents[bench_seat], "decision_log": bool}` (`results.py:122-130`) |
| `decks` | `{"deck_under_test": deck, "benchmark": deck}`. See 6.3. |
| `results` | `{"matches": outcomes(match winners), "games": outcomes(game winners)}` |
| `splits` | `{"on_play": outcomes(dut_on_play games), "on_draw": ..., "by_game_number": {"1".."G": outcomes(game_index == i-1)}}`, where G = `games_per_match`, so BO3 always has keys "1","2","3", even with n=0 (`aggregate.py:47-55`) |
| `game_length` | `{"turns": distribution(all turns), "by_result": {"win","loss","draw": distribution}, "histogram": [{"turns": t, "games": count}] sorted by t}` (`aggregate.py:75-83`) |
| `end_reasons` | for `r` in `sorted(set(end_reason))` (alphabetical), `{"reason": r, **outcomes(games with r)}` (`aggregate.py:58-62`) |
| `draws` | `{"games": #winner=="draw", "both_defeated": #drawn with that reason, "turn_limit": #drawn with that reason, "scored_for_match_by_turn_player_rule": #drawn with match_point != null}` (`aggregate.py:65-72`) |
| `mulligan` | 6.4 |
| `cards` | 6.5 |
| `benchmark_cards` | 6.6 |
| `hypotheses` | 6.7 |
| `hypothesis_thresholds` | `{"min_games": 20, "alpha": 0.05, "multiple_comparisons": "holm"}` |
| `conflict_resolutions` | 6.9 |
| `replays` | 9.1 |
| `notes` | the 4 constant strings `NOTES` (`results.py:28-38`), verbatim |

The `seeds` strings, verbatim:
- `"prng": "SplitMix64"`
- `"game_seed": "derive_seed(master, 'match', m, 'game', g)"`
- `"agent_seed": "derive_seed(master, 'match', m, 'game', g, 'agent', seat)"`
- `"dut_seat": "m % 2"`
- `"game_one_chooser": "seeded die roll inside new_game (rule 6-2-1-4)"`
- `"later_game_chooser": "loser of the previous game (BO3 Match Rules, TRM 4.6)"`

The `outcomes(winners)` function (`aggregate.py:28-36`) returns
`{"n": len, "dut_wins": #"dut", "bench_wins": #"bench", "draws": #"draw", "dut_win_rate": rate(#"dut", len)}`.
Draws count as non-wins in the rate.

### 6.2 `versions` (`versions.py:55-75`, cached per process)

| key | source | implementation-specific? |
| --- | --- | --- |
| `package` | `importlib.metadata.version("gcg-sim")`, or `"unknown"` on `PackageNotFoundError` (`versions.py:25-29`). Currently `"0.1.0"`. | **yes** (Python dist metadata) |
| `code_digest` | `code_digest()` (`versions.py:43-52`), see below | **yes: always differs** |
| `results_schema` | `"1.1.0"` | no |
| `dataset_version` | `data/gcgapi/manifest.json["dataset_version"]`, currently `28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42` | no (data) |
| `data_source_commit` | manifest `["source_commit"]` | no |
| `data_built_at` | manifest `["built_at"]`, currently `2026-10-12T14:37:38.154Z` | no |
| `rules_version` | `data/official/rules_version.json["latest_version"]` (`1.9.0`) | no |
| `rules_effective_date` | same file, `["effective_date"]` | no |
| `banlist_effective_date` | `data/official/banlist.json["effective_date"]` | no |
| `deck_rules_effective_date` | `data/official/deck_construction.json["effective_date"]` | no |
| `bo3_rules_effective_date` | `data/official/bo3_match_rules.json["effective_date"]` | no |

`code_digest` works as follows:
1. Walk `importlib.resources.files("gcg_sim")` recursively (`_package_files`,
   `versions.py:32-40`). Skip any entry named `__pycache__` and any name ending in `.pyc`.
   - Relative paths use `/` and include every other file: `.py`, data JSON, `cards.ndjson`,
     `LICENSE-DATA`, `schema.sql`, and any stray file such as a `.DS_Store` if one appears.
   - This uv install is editable, so the root is `src/gcg_sim`. 122 files are hashed.
2. `sorted()` the `(path, file)` tuples. Paths are unique, so this is a sort by the **full
   relative path string** in code-point order, not component-wise: `/` (0x2F) sorts after `.`
   (0x2E).
3. Run SHA-256 over, for each file in that order, `path.encode("utf-8") + b"\0" + file bytes`.
   There are no length prefixes. The result is lowercase hex.
4. The sample value is `3a11556b77c867330bb652d1f2468903f8dc7174a02e03b8b752760239b49c0b`.

### 6.3 Deck block (`results.py:53-69`)

| key | value |
| --- | --- |
| `name` | `Deck.name` (file stem) |
| `digest` | `deck_digest(deck)` = `sha256(json.dumps({"main": counts, "resources": counts}, sort_keys=True, separators=(",",":")))`. `counts` is `dict(sorted(Counter(cards).items()))` (`deck/parse.py:121-129`). The blob looks like `{"main":{"GD01-030":4,...},"resources":{"R-001":10}}`. |
| `main_count` / `resource_count` | `len(deck.main)` / `len(deck.resources)` |
| `colors` | `sorted({db[n].color.value for n in main if color is not None})`, a subset of `Blue, Green, Red, White, Purple` (`cards/model.py:40-45`) |
| `main` / `resources` | `[{"card_number": n, "name": db[n].name, "count": c}]` for `n` in sorted card numbers |

### 6.4 `mulligan` (`aggregate.py:86-97`)

Let `redrew` be the games where `g.dut_cards.redrew`, and `kept` the rest.
- `dut.redraw_rate = rate(len(redrew), len(games))`
- `dut.win_rate_after_redraw = rate(#dut-wins in redrew, len(redrew))`
- `dut.win_rate_after_keep = rate(#dut-wins in kept, len(kept))`
- `bench.redraw_rate = rate(sum(g.redraws[g.bench_seat]), len(games))`

### 6.5 `cards[]`: DUT per-card statistics (`aggregate.py:100-194`)

The loop runs `for number in sorted(Counter(deck_under_test.main))`. Only the main deck counts,
not resources. For each number, a fresh `_CardTally` is fed every game in order (`add`,
`aggregate.py:118-139`). For each game, with `c = g.dut_cards` and `win = (g.winner == "dut")`:
```
drawn = c.drawn.count(number);  turns = [p.own_turn for p in c.played if p.card_number == number]
drawn_copies += drawn;  drawn_games += drawn>0;  drawn_wins += drawn>0 and win
opening_games += number in c.opening_hand
played_games += bool(turns);  played_wins += bool(turns) and win
drawn_unplayed_games += drawn>0 and not turns;  drawn_unplayed_wins += (that) and win
plays += len(turns);  own_turns.extend(turns)
never = c.never_played.count(number);  never_copies += never;  never_games += never>0
end_hand_copies += c.in_hand_at_end.count(number)
if number in c.initial_hand: initial_games += 1; redrawn_games += c.redrew
```
In the entry formulas below (`aggregate.py:142-181`), `n = len(games)` and
`wins = #games with winner == "dut"`.

| key | value |
| --- | --- |
| `card_number`, `name`, `card_type` (`CardType` value, e.g. `UNIT`, `PILOT`, `COMMAND`, `BASE`), `level`, `cost`, `copies` | from the card DB and the main-deck count |
| `drawn` | `{"games": drawn_games, "game_rate": ratio(drawn_games, n), "copies": drawn_copies, "copies_per_game": ratio(drawn_copies, n)}` |
| `opening_hand` | `{"games": opening_games, "game_rate": ratio(opening_games, n)}` |
| `played` | `{"games": played_games, "plays": plays, "rate_when_drawn": ratio(played_games, drawn_games), "own_turn": distribution(own_turns), "by_own_turn": [{"own_turn": k, "plays": count}] sorted by k}` |
| `win_rate_when_drawn` | `rate(drawn_wins, drawn_games)` |
| `win_rate_when_not_drawn` | `rate(wins - drawn_wins, n - drawn_games)` |
| `win_rate_when_played` | `rate(played_wins, played_games)` |
| `win_rate_when_drawn_not_played` | `rate(drawn_unplayed_wins, drawn_unplayed_games)` |
| `held_never_played` | `{"copies": never_copies, "rate_of_drawn_copies": ratio(never_copies, drawn_copies), "games": never_games, "in_hand_at_end_copies": end_hand_copies}` |
| `mulligan` | `{"in_initial_hand_games": initial_games, "redrawn_games": redrawn_games, "rate": ratio(redrawn_games, initial_games)}` |

### 6.6 `benchmark_cards[]` (`aggregate.py:197-237`)

The loop runs over `sorted(Counter(benchmark.main))`. For each card, `seen` is the games with
`number in g.bench_seen` and `unseen` is the rest. The entry is:
- `card_number`, `name`, `card_type`, `copies`
- `seen_games = len(seen)` and `seen_rate = ratio(len(seen), len(games))`
- `dut_loss_rate_when_seen = rate(#winner=="bench" in seen, len(seen))`
- `dut_loss_rate_when_not_seen = rate(#winner=="bench" in unseen, len(unseen))`
- `loss_rate_lift = rnd(seen.rate - unseen.rate)` if both rates are non-null, else `null`.
  It uses the rates that were already rounded.
- `rank_eligible = seen_games >= 20 and unseen_n >= 20`. This key is added after the other keys
  are built.

The output is sorted by the key
`(not rank_eligible, -(lift if lift is not None else -2.0), -seen_games, card_number)`. The
`-2.0` sentinel puts a null lift after every real lift. Equal floats, **including `-0.0` and
`0.0`**, compare equal and fall through to the next key element.

### 6.7 `hypotheses[]` (`reports/hypotheses.py`)

The constants (`18-24`) are:
- `MIN_GAMES = 20`
- `MIN_REDRAWS = 10`
- `MIN_DELTA = 0.05`
- `MIN_SPLIT_DELTA = 0.10`
- `DEAD_CARD_RATE = 0.5`
- `PER_DIRECTION = 3`
- `ALPHA = 0.05`

`_pct(x)` is `"n/a"` if `None`, else `f"{100*x:.0f}%"` (**no decimals**; `summary.pct` has
one).

Each hypothesis (`_hypothesis`, `31-41`) is `{"id", "kind", "label": "hypothesis", "statement",
"sample_size", "evidence"}`.

`_compare(a, b)` (`44-51`) builds the evidence:
`{"a": a, "b": b, "delta": rnd(a.rate - b.rate), "ci95_separated": not intervals_overlap(a,b),
"p_value": rnd(two_proportion_p(a,b))}`. `a` and `b` are the same rate dicts as in
`cards[]`/`splits`.

`_significant(found, comparisons)` (`54-65`) works as follows:
1. `family = max(comparisons, len(found))`.
2. `p = [h.evidence.p_value for h in found] + [1.0] * (family - len(found))`. These are the
   **rounded** p-values.
3. `keep = holm(p, 0.05)`.
4. Set `evidence["comparisons"] = family` on every found hypothesis, kept or not. Return the
   kept ones in `found` order.

`hypotheses()` (`193-206`) assembles the list in this order:
1. **Card drawn deltas.**
   1. `found = [_card_delta(c) for c in cards]` (card-number order), dropping `None`.
   2. `_card_delta` (`77-100`) returns None unless `drawn.n >= 20 and not_drawn.n >= 20`, and
      None if `abs(delta) < 0.05`.
   3. `comparisons = _sampled_cards(cards)`: the number of cards meeting both sample minimums,
      whatever their delta (`68-74`).
   4. After `_significant`, `_top_by_delta` (`125-132`) keeps the top 3 with `delta > 0` and
      the top 3 with `delta < 0`, each sorted by `(-abs(delta), id)`. The output is
      `pos[:3] + neg[:3]`.
   5. The id is `card-drawn-delta:{card_number}` and the kind is `card_drawn_win_rate`. The
      sample size is `drawn.n + not_drawn.n`.
2. **Dead cards** (`103-122`), in card order, with no significance test:
   1. Condition: `drawn.copies >= 20` (MIN_GAMES used as a copy count), `rate_of_drawn_copies`
      is not None, and `rate_of_drawn_copies >= 0.5`.
   2. The id is `dead-card:{n}` and the kind is `held_never_played`. The sample size is
      `drawn_copies`.
   3. Evidence is `{"drawn_copies": drawn, **held_never_played}`, with no `p_value` and no
      `comparisons`.
3. **Play/draw** (`135-148`):
   1. Uses `split.on_play.dut_win_rate` against `on_draw`. It needs both `n >= 20` and
      `abs(delta) >= 0.10`.
   2. Then `_significant([h], 1)`, so it passes iff the rounded p is at most 0.05.
   3. The id is `play-draw` and the kind is `play_draw_split`.
4. **Redraw** (`151-164`):
   1. Uses `mull.dut.win_rate_after_redraw` against `after_keep`. It needs both `n >= 10` and
      `abs(delta) >= 0.10`.
   2. Then `_significant([h], 1)`.
   3. The id is `redraw` and the kind is `redraw_outcome`.
5. **Benchmark threats** (`167-184`):
   1. Iterate `bench_cards` in its **ranked** order (6.6).
   2. Condition: `rank_eligible`, a non-null lift, and `lift >= 0.10`. There is no negative
      direction.
   3. Evidence is `_compare(seen, unseen)`. Then
      `_significant(found, #rank_eligible bench cards)[:3]`.
   4. The id is `bench-threat:{n}` and the kind is `benchmark_card_threat`.

The statement templates, verbatim (with `{pct}` = `_pct`):
- card_drawn_win_rate:
  `"Games where {name} ({num}) was drawn were won {pct(a)} of the time vs {pct(b)} when it was
  not (n={a.n}/{b.n}). Hypothesis: {advice}. Correlation only: longer games also draw more
  cards."`. The advice is:
  - for `delta > 0`: `"more copies or more ways to find it may raise the win rate"`;
  - otherwise: `"it may underperform in this matchup; test cutting a copy"`.
- held_never_played:
  `"{pct(rate)} of drawn copies of {name} ({num}) were never played (n={drawn} copies;
  {in_hand_at_end_copies} still in hand at game end). Hypothesis: it is often uncastable or low
  priority in this matchup; consider fewer copies or a lower curve."`
- play_draw_split:
  `"The deck under test wins {pct(play)} on the play vs {pct(draw)} on the draw
  (n={play.n}/{draw.n}). Hypothesis: it is built for {going first|going second}; its curve and
  early interaction are the first things to test."`
- redraw_outcome:
  `"After a redraw the deck under test won {pct(r)} vs {pct(k)} after keeping (n={r.n}/{k.n}).
  Hypothesis: opening-hand consistency matters here; the early curve may need smoothing."`
- benchmark_card_threat:
  `"The deck under test lost {pct(seen)} of games where the benchmark's {name} ({num}) was seen
  in play vs {pct(unseen)} otherwise (n={seen.n}/{unseen.n}). Hypothesis: answers to it
  (removal, blockers, or racing it) may be worth testing."`

(Each template is a single line in the output; the wraps above are for readability.)

### 6.8 Synthetic hypothesis sample

Produced by calling `hypotheses()` on hand-built rate blocks in a Python snippet (no
benchmark run). The real sample run had `hypotheses: []`.
```json
{"evidence": {"a": {"ci95": [0.027866, 0.301038], "n": 20, "rate": 0.1, "successes": 2}, "b": {"ci95": [0.531295, 0.88814], "n": 20, "rate": 0.75, "successes": 15}, "ci95_separated": true, "comparisons": 1, "delta": -0.65, "p_value": 3.2e-05}, "id": "redraw", "kind": "redraw_outcome", "label": "hypothesis", "sample_size": 40, "statement": "After a redraw the deck under test won 10% vs 75% after keeping (n=20/20). Hypothesis: opening-hand consistency matters here; the early curve may need smoothing."}
```
Note `"p_value": 3.2e-05`. serde_json would print `0.000032` (2.2).

### 6.9 `conflict_resolutions[]` (`versions.py:82-135`)

The input is the dict `{"deck_under_test": dut.main + dut.resources, "benchmark":
bench.main + bench.resources}` (`results.py:181-186`).
1. Build `in_decks: card_number -> [labels]` by iterating the decks in that dict order, over
   `set(numbers)`. The set order changes only key insertion order, which does not reach the
   output.
2. For each `o` in `CardDB.applied_overrides` (load order: card numbers sorted, then conflict
   ids sorted, `cards/db.py:84-146`) whose card is in a deck:
   `out[(card, cid)] = entry(fields=list(o.fields), changes_card_data=True,
   decision=o.decision)`. Assignment means the last duplicate wins. `o.decision` is
   `res.get("decision", res.get("policy", ""))`.
3. For each `(cid, res)` in `sorted(overrides["resolutions"].items())`, skipping a `res` that is
   not a mapping, and for each `n` in `sorted(_resolution_cards(cid, res) & in_decks.keys())`:
   `out.setdefault((n, cid), entry(decision=str(res.get("decision","")), fields=[],
   changes_card_data=False))`. Applied overrides win.
   - `_resolution_cards` (`93-99`) is the union of `res.card_numbers`, `res.card_number`,
     **every** `cid.split(":")[1:]` part (for example `ruling:GD01-001:Q119` gives
     `{GD01-001, Q119}`), and the `card_numbers` of the curated conflict with that id
     (`data/curated_conflicts.json`).
4. The output is `[out[k] for k in sorted(out)]`, sorted by `(card_number, conflict_id)`.
5. The entry (`102-113`) is `{"card_number", "name": db[n].name, "conflict_id",
   "decks": sorted(labels), "decision", "fields", "changes_card_data"}`. `decks` is sorted, so it
   reads `["benchmark", "deck_under_test"]` when a card is in both decks.

---

## 7. The JSON Schema

- **Path:** `src/gcg_sim/reports/schema/results.v1.schema.json` (517 lines). It is loaded with
  `importlib.resources` and cached (`results.py:26,41-45`).
- **Header:** `$schema: https://json-schema.org/draft/2020-12/schema` and
  `$id: urn:gcg-sim:schema:results:1.1.0`. The description notes that 1.1.0 added the `fast`
  preset.
- **Validation call:** `jsonschema.validate(obj, schema, format_checker=jsonschema.FormatChecker())`
  (`results.py:50`), with jsonschema 4.26.0 and the `Draft202012Validator`.
  - `format: date` is checked as `^\d{4}-\d{2}-\d{2}$` fullmatch plus `date.fromisoformat`.
  - `pattern` uses Python `re.search`.
- **Notable constraints:**
  - `additionalProperties: false` on almost every object; `outcomes` and `end_reasons` items use
    `unevaluatedProperties: false`.
  - `schema` and `schema_version` are `const` values.
  - `code_digest` and deck `digest` match `^[0-9a-f]{64}$`.
  - `ai.preset` is one of `fast`, `standard`, `strong`.
  - `config.format` is one of `bo3`, `bo1`.
  - `by_game_number` property names are limited to "1", "2", "3", and "1" is required.
  - `end_reasons[].reason` is limited to the 5 EndReason values.
  - `histogram[].games >= 1`.
  - Probabilities lie in [0,1]. `copies_per_game` and `rate_when_drawn` have no upper bound.
    `loss_rate_lift` lies in [-1,1].
  - `replays[].file` matches `^replays/(win|loss|draw)-m[0-9]{4,}-g[1-3]\.json$`.
  - Hypothesis `kind` is one of the 5 kinds, `label` is `const "hypothesis"`,
    `sample_size >= 1`, and `evidence` is a free object.
- Validation is only a gate: Python output always validates. A Rust port can validate with a
  draft 2020-12 validator in tests.

---

## 8. `summary.md` (`reports/summary.py`)

`render_summary(results, command)` is a pure function of the results dict and the command.
Checked: re-rendering from the loaded sample `results.json` gives byte-identical output.

The output is `"\n".join` of these sections in order: headline, results, endings, mulligan,
cards, bench, hypotheses, provenance. Every section ends with `""`, so sections are separated
by one blank line and the file ends with one `"\n"`.

The helpers:
- `pct(x)`: `"n/a"` or `f"{100*x:.1f}%"`.
- `num(x, d=1)`: `"-"` or `f"{x:.{d}f}"`. Ints are formatted as floats, so `13` with `.0f`
  gives `13`.
- `points(x)`: `"-"` or `f"{100*x:+.1f} pp"`.
- `rate_text(r)`: `"n/a (n=0)"` if `rate` is None, else `f"{pct(rate)} [{pct(lo)}, {pct(hi)}] (n={n})"`.
- `_table(header, rows)` gives these lines, with `str()` of each cell:
  ```
  "| " + " | ".join(header) + " |"
  "|" + "---|" * len(header)
  "| " + " | ".join(str(c) for c in row) + " |"     (one per row)
  ```
- `OUTCOME_HEADER = ("", "n", "DUT wins", "Bench wins", "Draws", "DUT win rate [95% CI]")`. The
  empty first header produces the line `|  | n | ...`, with two spaces.
- An outcome row is `[label, n, dut_wins, bench_wins, draws, rate_text(dut_win_rate)]`.

The sections:
1. **Headline** (`43-55`):
   ```
   # {dut name} vs {bench name}

   Deck under test (DUT) **{dut}** against benchmark **{bench}**: {matches} {FORMAT.upper()} matches, master seed {master}, AI preset `{preset}` (agents `{dut_agent}` / `{bench_agent}`, factory `{factory}`).

   Reproduce: `{command}`
   ```
   The word `matches` is never singularised.
2. **`## Result`** has rows `Matches` and `Games`. **`## Splits`** has rows `On the play`,
   `On the draw`, then `Game {k}` for each `k` in `sorted(by_game_number)` (`58-75`).
3. **`## How games end`** (`78-96`):
   - A table with header `("Reason", *OUTCOME_HEADER[1:])` and one row per `end_reasons` entry.
   - The line: `Game length (turns): mean {num(mean)}, median {median}, p10 {p10}, p90 {p90},
     range {min}-{max}. Median by DUT result: win {num(win.median,0)}, loss {...}, draw
     {...}.` The median, p10, p90, min and max are written with plain `str()`.
   - The line: `Draws: {games} ({both_defeated} simultaneous defeat, {turn_limit} turn limit);
     {scored} scored for BO3 by the turn-player-loses rule (TRM 5.2).`
4. **`## Redraws`** (`99-109`):
   `DUT redraw rate {rt}; DUT win rate after a redraw {rt}, after keeping {rt}. Benchmark
   redraw rate {rt}.`
5. **`## Deck-under-test cards`** (`112-146`):
   - Header: `Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when
     drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when
     in initial hand`.
   - Row cells, in order:
     - `"{num} {name}"`
     - `copies`
     - `"{drawn.games} ({pct(drawn.game_rate)})"`
     - `played.games`
     - `num(own_turn.mean)`
     - `rate_text` ×3
     - `"{held.copies} ({pct(held.rate_of_drawn_copies)})"`
     - `pct(mulligan.rate)`
6. **`## Benchmark cards most associated with DUT losses`** (`149-166`):
   - The rows are `[b for b in benchmark_cards if rank_eligible][:10]`.
   - The header is `Benchmark card | Seen games | DUT loss rate when seen | when not seen | Lift`,
     and the lift cell is `points(lift)`.
   - With no eligible card, the section shows the single line
     `No benchmark card has {min_games}+ games both seen and unseen yet.`
7. **`## Tuning hypotheses (to test, not conclusions)`** (`169-198`):
   - The gate text is `a two-proportion z-test significant at {alpha} after a
     {"holm".capitalize()} correction per kind of comparison, with at least {min_games} games per
     side (10 for redraws)`. `{alpha}` is the float repr, `0.05`. The `10` is literal text.
   - With no hypotheses: `No signal cleared the noise gate ({gate}). Run more games.`
   - Otherwise: `Comparison signals (win-rate differences) cleared the noise gate: {gate}.
     'Never played' signals are descriptive.`, then a blank line, then one
     `- {statement} {note}` per hypothesis.
   - `note` is `(sample: {sample_size})` when the evidence has no `p_value`. Otherwise it is
     `(sample: {s}; p = {p_value}, {comparisons or 1} comparison(s) of this kind)`, where
     **`{p_value}` is the float repr**, as in `p = 3.2e-05`.
8. **Provenance** (`201-227`):
   - `## Versions`, then: `gcg-sim {package}; results schema {results_schema}; card data
     {dataset_version} (built {data_built_at}); Comprehensive Rules {rules_version}
     ({rules_effective_date}); Banned & Restricted list {banlist_effective_date}; BO3 rules
     {bo3_rules_effective_date}.` `code_digest` and `deck_rules_effective_date` are **not**
     shown.
   - `## Conflict resolutions affecting these decks`: one
     `` - `{conflict_id}` ({card_number} {name}; {", ".join(decks)}): {decision}`` per entry,
     or `- none`.
   - `## Replays`: one
     `` - `{file}`: {result}, match {match_index} game {game_number}, {turns} turns, {end_reason}``
     per entry, or `- none`.
   - `## Notes`: one `- {note}` per entry.

The reproduce command (`writer.py:44-51`) is
`shlex.join(["gcg-sim", "benchmark", dut.source or dut.name, bench.source or bench.name,
"--matches", str(matches), "--seed", str(seed), "--format", fmt, "--ai-preset", preset,
"--replays", str(K)] + (["--decision-log"] if decision_log))`.
- It never includes `--workers` or `--out`, and the seed is always decimal.
- `shlex.quote` (Python 3.15 source) returns `''` for an empty string. It returns the string
  unchanged iff it is ASCII and made only of `%+,-./0-9:=@A-Z_a-z`. Otherwise it returns
  `"'" + s.replace("'", "'\"'\"'") + "'"`.
- `source` is `str(Path(argv))`. pathlib collapses `//` and `./`, but not `..`. Checked:
  `./examples//decks/./a.txt` becomes `examples/decks/a.txt`.

---

## 9. `replays/*.json` (`results.py:72-106`, `writer.py:79-91`, `runner/replayfile.py`)

### 9.1 Selection (`--replays K`, default 3, `cli.py:83-85`)

1. `select_replays(run)` (`results.py:86-91`):
   1. `by_result[r] = [g for g in run.games if dut_result(g) == r]` in run order.
   2. The quotas are `{"win": K, "loss": K, "draw": min(K, 1)}` (`DRAW_REPLAYS = 1`).
   3. The output is every pick for win, then loss, then draw.
2. `_pick(games, k)` (`results.py:72-79`):
   1. Return `[]` if `games` is empty or `k <= 0`.
   2. `median = sorted(turns)[(len-1)//2]`, the lower median.
   3. Rank by `(abs(turns - median), match_index, game_index)` and take the first k.
   4. **Re-sort** those k by `(match_index, game_index)`.
3. The file name is `f"replays/{label}-m{match_index:04d}-g{game_index+1}.json"`
   (`results.py:82-83`). The match index is zero-padded to at least 4 digits and can have more.
4. The `results.json` `replays[]` entry (`results.py:94-106`) is `{"file", "result": label,
   "match_index", "game_number": game_index+1, "turns", "end_reason", "seed"}`.
5. After writing, `_write_replays` deletes each file in `out/replays/` that matches
   `^(win|loss|draw)-m\d{4,}-g[1-3]\.json$` and was not written this run. It only does this if
   the directory exists. With K=0, no directory is created.

### 9.2 Document (`replay_document`, `replayfile.py:104-150`)

The document is built by **re-simulating** the record with `replay_with_log`, to produce the
log and `winner_seat`. `turns` and `final_state_sha256` in `expected` are copied from the
record, not recomputed. Nothing at write time checks the re-simulation against the record.

| key | value |
| --- | --- |
| `format` | `"gcg-sim-replay"` |
| `version` | `1` |
| `label` | `"win"`, `"loss"` or `"draw"` |
| `match_index`, `game_index`, `seed`, `chooser`, `turn_limit`, `max_actions` | from the record |
| `seats` | list for seat 0, 1: `{"seat", "role": "dut"\|"bench", "deck": deck name, "agent": agents[seat], "agent_seed": agent_seeds[seat], "main": {card: count} sorted, "resources": {...}}`. The decks come from `seat_decks(config, match_index)`. |
| `actions` | the record's actions as lists |
| `expected` | `{"winner": record.winner (dut/bench/draw), "winner_seat": st.winner of the re-simulation (-1 for a draw), "end_reason", "turns", "final_state_sha256"}` |
| `log` | list of str (below) |
| `decision_log` | `null` or the record's list |

The log (`replay_with_log`, `75-101`) has one line per action, built **before** `apply`:
`f"T{st.turn} P{dec.player} {dec.kind.value}: {describe_action(st, dec, action)}"`. The last
line is `f"T{st.turn} game over: winner P{st.winner} ({reason})"`. `reason` is
`st.end_reason.value`, or `"not finished"`; a draw prints `winner P-1`, and an unfinished game
prints `winner PNone`.

`describe_action` (`45-65`) returns `f"{text} [{action}]"`. `str(Action)` (`engine/state.py:30-32`)
is `f"{kind.value}({','.join(str(x) for x in (a,b,c) if x != -1)})"`, as in `keep()`,
`play_unit(7,0)` or `attack(6,-2)`. `_label(uid)` is `str(uid)` if the uid is out of range, else
`f"{card_number} {name}"`. The `text` depends on the action kind:

| kind | text |
| --- | --- |
| `play_unit`, `play_base`, `play_command` | `play {label(a)}` |
| `pair` | `pair {label(a)} with {label(b)}` |
| `activate` | `activate {label(a)}` |
| `attack` | `attack the player with {label(a)}` if `b == -2`, else `attack {label(b)} with {label(a)}` |
| `block` | `block with {label(a)}` |
| `go_first` | `P{a} goes first` |
| `select` | See the rules after this table. |
| anything else | `kind.value` |

The `select` text (`37-42`) is chosen in this order:
1. If `dec.kind is ARRANGE` and `b != -1`: `"bottom: X"` if `a` is nonzero, else `"top: X"`,
   with `X = label(b)`.
2. Elif `dec.kind ∈ {SELECT, DISCARD, EXCESS}` and `ctx("mode") == -1` and
   `ctx("rest_sub") == -1`: `select {label(a)}`.
3. Otherwise `select option {a}`.

### 9.3 Verification: `gcg-sim replay FILE [--log]`

`cli.cmd_replay` (`cli.py:338-370`) calls `check_replay` (`replayfile.py:164-205`):
1. If `format` is not `gcg-sim-replay` or `version` is not `1`, raise
   `ReplayError("not a gcg-sim-replay v1 document")`.
2. Sort the seats by `int(seat)`. `_decklist` expands each `{card: count}` dict into
   `tuple(sorted(...))` (`68-72`).
3. Re-run `new_game(decks, int(seed), chooser, turn_limit, max_actions)` and apply the actions.
   The possible errors are:
   - `UnimplementedCardError` becomes
     `"the replay uses a card this version cannot play: {exc}"`;
   - no pending decision becomes
     `"the replay has more actions than the game had decisions"`;
   - `IllegalActionError` becomes
     `"action {index} {str(action)} is not legal in the replayed game"`;
   - `KeyError`, `TypeError`, `IndexError` or `ValueError` becomes
     `"malformed replay document: {type(exc).__name__}: {exc}"`. These carry Python exception
     texts.
4. `actual = {"winner_seat": st.winner, "end_reason": value or None, "turns": st.turn,
   "final_state_sha256": state_digest(st)}`. The comparison is in this key order and uses
   `expected.get(k) != v`. `expected.winner` is **not** compared.
5. Each mismatch is `f"{k}: expected {expected.get(k)!r}, replay gave {v!r}"`. It uses Python
   `repr`: `'battle_damage'` in single quotes, `None`, and ints.

The CLI output is:
1. The log, if `--log`.
2. Then `seat {seat}: {role} {deck} ({agent})` for each seat, sorted by seat.
3. Then `result: winner seat {winner_seat} ({end_reason}) after {turns} turns`.
4. On a match, `replay matches the recorded outcome and final state`, exit 0.
5. Otherwise stderr gets `error: replay does NOT match the recorded outcome:\n  ` with the
   mismatches joined by `"\n  "`, and the exit code is 1.

The CLI also catches `ReplayError`, `ValueError` and `LookupError` as
`error: cannot replay {path}: {exc}` (exit 1), plus the read, UTF-8 and JSON errors at
`cli.py:339-352`.

---

## 10. `timing.json` (`writer.py:60-76`): never reproducible

The keys, in output (sorted) order:

| key | value |
| --- | --- |
| `finished_at` | `datetime.now(UTC).isoformat(timespec="seconds")`, e.g. `2026-11-01T07:33:54+00:00` (`run.py:29-30`) |
| `games` | `len(run.games)` |
| `games_per_second` | `round(len(games)/wall, 3)`, or `null` if `wall == 0` |
| `match_seconds` | `[round(s, 3)]` in match order (`perf_counter` deltas) |
| `matches` | `len(run.matches)` |
| `mean_match_seconds` | `round(sum(seconds)/len, 3)`. This is a float `sum()`, which uses Neumaier compensation in CPython 3.12+. |
| `note` | `"wall-clock measurements; the only non-reproducible report file"` |
| `platform` | `platform.platform()`, e.g. `macOS-27.2-arm64-arm-64bit-Mach-O` |
| `python` | `platform.python_version()`, e.g. `3.15.1` |
| `started_at` | as `finished_at` |
| `wall_seconds` | `round(wall, 3)` |
| `workers` | `processes_used(config)`: `1` if `workers <= 1 or matches == 1`, else `min(workers, matches)` (`run.py:83-85`) |

---

## 11. `final_state_sha256`: the state digest (`runner/digest.py`)

`state_digest(st)` is
`sha256(json.dumps(canonical_state(st), sort_keys=True, separators=(",",":")).encode())`, as
lowercase hex (`digest.py:52-56`). It is used in games.ndjson, `replays[].expected` and
`check_replay`.

`canonical_state` (`digest.py:36-49`) starts from `d = st.to_json()`
(`engine/state.py:575-611`). That dict holds 34 keys, from `seed, rng, cards, zones, ...` to
`version`. Its structure belongs to the engine inventory. The function then replaces
process-local registry indices with `content_id(obj) = sha256(repr(obj).encode())[:16]` (16 hex
characters, `digest.py:26-27`):
- For each Lasting list `le` in `d["lasting"]` and `d["delayed"]`:
  `le[0] = content_id(reg.continuous[le[0]])`, and if `le[9] >= 0`,
  `le[9] = content_id(reg.filter_sets[le[9]])`. The positions come from `Lasting.to_json`:
  index 0 is `effect_key` and index 9 is `filters_key` (`engine/state.py:231-245`).
- `f["program_id"] = content_id(reg.programs[...])` for each frame.
- `t[0] = content_id(reg.programs[t[0]])` for each pending trigger and each trigger in
  `batches`.
- `once_used`:
  1. Each key `k` with `len(k) >= 3`, `k[0] ∈ {-7 (REDUCE_ONCE_TAG), -8 (PREVENT_ONCE_TAG)}` and
     `k[1] == -1` becomes `[k0, k1, content_id(reg.continuous[k2]), *k[3:]]`.
  2. The whole list is then **re-sorted with `key=json.dumps`**, meaning by the text with
     default separators.
  3. Python gives `sorted([[1,10],[1,2],[-7,-1,"ab",3]], key=json.dumps)` →
     `[[-7, -1, "ab", 3], [1, 10], [1, 2]]`. This is not numeric order.

**`repr()` is part of the hash.** Content ids hash CPython reprs of DSL dataclasses:
- `Program(instrs=(...), label='...')`, `StatMod(ap=-3, hp=0)`, and tuples of `Filter`s;
- 137 dataclasses in `effects/dsl.py`, 10 in `effects/program.py`;
- nested `StrEnum` members, which repr as `<Kw.REPAIR: 'Repair'>`.

Sample check: the final state of the sample game has one lasting effect, whose id is
`5c807dae66b001af`. `printf 'StatMod(ap=-3, hp=0)' | shasum -a 256 | cut -c1-16` gives
`5c807dae66b001af`.

For Rust to match `final_state_sha256`, it must emit **exactly** CPython's repr for every
registry object that can be live at the end of a game:
- the dataclass field order;
- `Name(field=value, ...)`;
- `str` repr quoting: single quotes unless the string contains `'` and no `"`, plus the escape
  rules;
- `()`, `(x,)` and `(a, b)` tuple forms;
- `None`, `True` and `False`;
- negative ints;
- enum repr.

No DSL dataclass holds a set or frozenset, so repr order does not depend on hashing. The only
match is the module constant `STACKING_KEYWORDS`, `dsl.py:72`.

---

## 12. Implementation-specific fields: normalise these in Python-vs-Rust comparisons

| file | field | why |
| --- | --- | --- |
| `results.json` | `versions.code_digest` | SHA-256 of the Python package tree (6.2). It always differs. |
| `results.json` | `versions.package` | Python dist metadata (`"0.1.0"`, or `"unknown"` without metadata). The Rust workspace version is also `0.1.0` (`Cargo.toml`), but only by coincidence. |
| `results.json` | `ai.factory` | `describe_factory` (`runner/agents.py:106-114`): the default factory gives the Python-specific `"gcg_sim.ai.make_agent('mcts')"`, built with `repr`. Under the `GCG_SIM_AGENT_FACTORY` test hook it is the `module:attr` or `file.py:attr` reference. Rust must emit the same literal, or the field must be normalised. |
| `summary.md` | the `gcg-sim {package}` line, `factory \`...\`` in the headline | the same two values, rendered |
| `timing.json` | the whole file | wall clock; also `python` and `platform` |
| replay and ndjson | none by design | `final_state_sha256` is meant to be implementation-neutral, but it depends on CPython repr (section 11) |

Everything else must be byte-identical for the same decks, seed, options and data:
`results.json` minus the three fields above, `games.ndjson`, `replays/*.json`, and `summary.md`
minus two substrings. The output is independent of `--workers` and `--out`. `--out` is not
recorded anywhere except in CLI stdout.

---

## 13. Sample run

The command (4.1 s wall, exit 0):
```
uv run gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt \
  --matches 1 --format bo1 --ai-preset fast --seed 3 --out /private/tmp/claude-501/p0-sample
```
stdout:
```
red-green-zeon (deck under test) vs blue-white-federation (benchmark)
matches: DUT 0-1 (draws 0), win rate 0.0% [95% CI 0.0%-79.3%] over 1
games: DUT 0-1 (draws 0), win rate 0.0% [95% CI 0.0%-79.3%] over 1
reports:
  results  /private/tmp/claude-501/p0-sample/results.json
  ...
  replays  1 file(s) in /private/tmp/claude-501/p0-sample/replays
```
stderr: `matches 1/1`.

The files written were `results.json` (55,385 B, 2,517 lines), `summary.md` (8,893 B),
`games.ndjson` (2,808 B, 1 line), `timing.json` (407 B) and `replays/loss-m0000-g1.json`
(10,665 B). `uv run gcg-sim replay .../replays/loss-m0000-g1.json` printed
`seat 0: dut red-green-zeon (mcts-fast)`, `seat 1: bench blue-white-federation (mcts-fast)` and
`result: winner seat 1 (battle_damage) after 13 turns`, then
`replay matches the recorded outcome and final state` (exit 0).

**results.json** excerpts (raw file text):
```json
{
  "ai": {
    "bench_agent": "mcts-fast",
    "decision_log": false,
    "dut_agent": "mcts-fast",
    "factory": "gcg_sim.ai.make_agent('mcts')",
    "preset": "fast"
  },
  "benchmark_cards": [
    {
      "card_number": "GD01-001",
      "card_type": "UNIT",
      "copies": 2,
      "dut_loss_rate_when_not_seen": {
        "ci95": null,
        "n": 0,
        "rate": null,
        "successes": 0
      },
      "dut_loss_rate_when_seen": {
        "ci95": [
          0.206543,
          1.0
        ],
...
  "replays": [
    {
      "end_reason": "battle_damage",
      "file": "replays/loss-m0000-g1.json",
      "game_number": 1,
      "match_index": 0,
      "result": "loss",
      "seed": 7429747955130320708,
      "turns": 13
    }
  ],
```
More values, compacted:
- `config`:
  `{"format":"bo1","matches":1,"max_actions":20000,"max_games_per_match":1,"replays_per_result":3,"turn_limit":200,"wins_needed":1}`.
- `results.games.dut_win_rate`: `{"ci95":[0.0,0.793457],"n":1,"rate":0.0,"successes":0}`.
- `cards` entry for `GD01-035 Zaku Ⅱ`: `played.own_turn` is
  `{"max":4,"mean":2.5,"median":1,"min":1,"n":2,"p10":1,"p90":4}`, which shows the lower median.
  `by_own_turn` is `[{"own_turn":1,"plays":1},{"own_turn":4,"plays":1}]`.
- The name is stored as raw UTF-8, `"name": "Zaku Ⅱ"`.
- `decks.deck_under_test`: `colors ["Green","Red"]`,
  `digest 28f6f68033524715506b921fb1af660d16613023bbc78d40634a644a7cea3825`, `main_count 50`,
  `resource_count 10`.
- `conflict_resolutions` has 17 entries; the first is
  `ruling:GD01-001:Q119` for `GD01-001`, decks `["benchmark"]`. `hypotheses` is `[]`.

**games.ndjson** (one line, excerpt):
```
{"actions":[["go_first",1,-1,-1],["keep",-1,-1,-1],["redraw",-1,-1,-1],["end_main",-1,-1,-1],["play_unit",7,-1,0],...],"active_at_end":1,"agent_seeds":[15091393010796579730,6117447680882809788],"agents":["mcts-fast","mcts-fast"],"bench_seen":["GD01-001","GD01-004",...],"chooser":null,"chooser_seat":1,"decision_log":null,"dut_cards":{"drawn":["GD01-035","GD01-035","GD01-059",...],"in_hand_at_end":["ST03-010"],"initial_hand":["GD01-044","GD01-125","GD04-040","ST03-008","ST03-013"],"never_played":["ST03-010"],"opening_hand":["GD01-035","GD01-059","GD01-092","GD02-048","ST03-008"],"played":[["GD01-035",2,1],["ST03-008",2,1],["GD01-059",4,2],...],"redrew":true},"dut_on_play":false,"dut_seat":0,"end_reason":"battle_damage","final_state_sha256":"de38d2a385e8d9d032e66d3a82a88576ff997c2c938664228c6c20c262968e52","first_player":1,"game_index":0,"match_index":0,"match_point":"bench","max_actions":20000,"redraws":[true,false],"seed":7429747955130320708,"turn_limit":200,"turns":13,"winner":"bench"}
```
The game had 74 actions.

**replays/loss-m0000-g1.json** (excerpts; `actions` are pretty-printed one integer per line):
```json
  "chooser": null,
  "decision_log": null,
  "expected": {
    "end_reason": "battle_damage",
    "final_state_sha256": "de38d2a385e8d9d032e66d3a82a88576ff997c2c938664228c6c20c262968e52",
    "turns": 13,
    "winner": "bench",
    "winner_seat": 1
  },
  "format": "gcg-sim-replay",
  "game_index": 0,
  "label": "loss",
  "log": [
    "T0 P1 choose_first: P1 goes first [go_first(1)]",
    "T0 P1 redraw: keep [keep()]",
    "T0 P0 redraw: redraw [redraw()]",
    "T1 P1 main: end_main [end_main()]",
    "T2 P0 main: play GD01-035 Zaku Ⅱ [play_unit(7,0)]",
    "T4 P0 main: attack the player with ST03-008 Zaku Ⅱ [attack(6,-2)]",
    "T6 P0 main: pair GD01-092 M'Quve with GD01-035 Zaku Ⅱ [pair(5,7,0)]",
    "T13 P0 select: select GD04-008 Gundam [select(51)]",
    "T13 game over: winner P1 (battle_damage)"
  ],
  ...
  "seats": [ { "agent": "mcts-fast", "agent_seed": 15091393010796579730, "deck": "red-green-zeon",
               "main": { "GD01-030": 4, ... }, "resources": { "R-001": 10 }, "role": "dut", "seat": 0 }, ... ],
  "seed": 7429747955130320708,
  "turn_limit": 200,
  "version": 1
```

**summary.md** excerpts:
```
# red-green-zeon vs blue-white-federation

Deck under test (DUT) **red-green-zeon** against benchmark **blue-white-federation**: 1 BO1 matches, master seed 3, AI preset `fast` (agents `mcts-fast` / `mcts-fast`, factory `gcg_sim.ai.make_agent('mcts')`).

Reproduce: `gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt --matches 1 --seed 3 --format bo1 --ai-preset fast --replays 3`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |
...
Game length (turns): mean 13.0, median 13, p10 13, p90 13, range 13-13. Median by DUT result: win -, loss 13, draw -.
...
| GD01-035 Zaku Ⅱ | 3 | 1 (100.0%) | 1 | 2.5 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
...
No benchmark card has 20+ games both seen and unseen yet.
...
No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.
...
gcg-sim 0.1.0; results schema 1.1.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.
...
- `divergent:R-001` (R-001 Resource; benchmark, deck_under_test): Use R-001_p7: a Resource card has no AP or HP. The Edition Beta record R-001_p4 (AP3/HP1) is a data error.
...
- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 13 turns, battle_damage
```

**timing.json** (complete):
```json
{
  "finished_at": "2026-11-01T07:33:54+00:00",
  "games": 1,
  "games_per_second": 0.281,
  "match_seconds": [
    3.56
  ],
  "matches": 1,
  "mean_match_seconds": 3.56,
  "note": "wall-clock measurements; the only non-reproducible report file",
  "platform": "macOS-27.2-arm64-arm-64bit-Mach-O",
  "python": "3.15.1",
  "started_at": "2026-11-01T07:33:50+00:00",
  "wall_seconds": 3.56,
  "workers": 1
}
```

---

## 14. Hazards (Python semantics the Rust port must reproduce)

1. **Float repr in JSON and text.** `json.dumps` and `str(float)` use the CPython repr, with
   exponents like `1e-05` and `1e+16`. Rust `{:?}` writes `1e-5`. serde_json 1.0.154 writes
   `0.00001` and `0.000032` (2.2). This affects `writer.py:34`, `writer.py:56`,
   `summary.py:176` and `summary.py:196`, and every float in results and replays.
2. **`ensure_ascii` differs per file.** games.ndjson uses `ensure_ascii=True` (`writer.py:56`).
   results, replays and timing use `False` (`writer.py:34`). The digest blob uses `True`
   (`digest.py:55`). In `True` mode DEL becomes `\u007f` and non-BMP characters become
   surrogate pairs.
3. **`sort_keys=True` everywhere.** The workspace serde_json has `preserve_order` enabled
   (`Cargo.toml`), so Rust must sort explicitly (`writer.py:34,56`, `digest.py:55`,
   `deck/parse.py:128`).
4. **`round(x, 6)`** rounds correctly with half-even on exact ties (`stats.py:28-29`). It is
   applied to already-rounded values again in `_compare` (`hypotheses.py:48`) and the lift
   (`aggregate.py:205`). Rust `{:.6}` → parse matched every vector tried, but verify further.
5. **Fixed-precision formatting.** `f"{100*x:.1f}"` (`summary.py:12`), `.0f` (`hypotheses.py:28`,
   `summary.py:16`) and `:+.1f` (`summary.py:20`) round half to even on exact ties, and the
   `100*x` multiply happens in f64 first. Rust matched on every vector tried (2.5).
6. **`max(0.0, v)` / `min(1.0, v)`** in `wilson` (`stats.py:23-24`) return the first argument on
   ties. Rust `f64::max` and `f64::min` treat NaN and signed zero differently, so write the
   comparison out.
7. **Float sort keys with `-0.0 == 0.0`.** These appear in the bench ranking
   (`aggregate.py:229-236`, with the `-2.0` sentinel), `_by_delta` (`hypotheses.py:131-132`) and
   the Holm order `(p, i)` (`stats.py:66`). Use IEEE `partial_cmp`, never `f64::total_cmp`.
8. **The Holm threshold uses `>`**, so equality passes, with `alpha/(m-rank)` an f64 division
   (`stats.py:69`). It is fed the **rounded** p-values (`hypotheses.py:58`).
9. **`math.erfc` is platform libm** (`stats.py:60`). It is bit-identical to the macOS libm
   `erfc` (verified on 200k samples). Use FFI to the same libm. Linux glibc may differ by an
   ulp, which can only show at a 6-digit rounding boundary.
10. **`percentile`'s `ceil(q*len)` is in f64** (`stats.py:77`). Median and p10 are nearest-rank,
    so the median is the lower median (`stats.py:96`).
11. **Python int/int `/` is correctly rounded.** This matters in `stats.py:18`, `stats.py:33`,
    `stats.py:41`, `stats.py:55-59` and `stats.py:95`. It is equal to `a as f64 / b as f64`
    only while both values are below 2^53, which holds in practice.
12. **Ints above `i64::MAX`** (seeds, agent seeds) appear in every output file
    (`records.py:111-112`, `replayfile.py:124,134`, `results.py:103,135`). Use u64.
13. **`code_digest`** (`versions.py:43-52`) sorts the full relative path string, includes every
    non-pyc file (stray files too), and depends on the install layout. Normalise it.
14. **`describe_factory` uses `repr`**, giving `'mcts'` with single quotes (`agents.py:108`).
    `package_version` uses an exception fallback (`versions.py:26-29`). Both are
    implementation-specific.
15. **The digest hashes CPython `repr()` of DSL dataclasses and enums** (`digest.py:26-27`), and
    the `once_used` sort uses `key=json.dumps` text with `", "` separators (`digest.py:48`). The
    final state of the sample game already depends on this (11).
16. **Observer history-identity test** (`instrument.py:81`). It relies on the engine replacing
    the `history` list at turn start (`engine/game.py:190,336`). A Rust engine that clears in
    place needs an explicit generation counter.
17. **`bench_seen` is a set**, sorted only when the record is built (`game.py:161`). `drawn` is a
    dict used as an ordered set, sorted by card number in `dut_events` (`instrument.py:95-106`).
    Card-number sort is code-point string order. `played` stays chronological.
18. **Bool arithmetic in tallies.** `x += drawn > 0 and win` (`aggregate.py:124-139`) and
    `sum(bool)` (`aggregate.py:69-71,89,93-94`) treat True as 1.
19. **`set(numbers)` iteration** in `conflict_resolutions` (`versions.py:122`) is hash-order
    dependent. The output is still deterministic because of the later `sorted` calls
    (`versions.py:109,132,135`). Do not "simplify" those sorts away.
20. **Last-wins and first-wins dict writes** in `conflict_resolutions`. Applied overrides use
    `out[key] =`, last wins (`versions.py:128`). Resolutions use `setdefault`, first wins
    (`versions.py:134`).
21. **Hypotheses mutate their evidence** (`_significant`, `hypotheses.py:61-62`): it adds
    `comparisons`, even to dropped hypotheses. `bench_card_stats` adds `rank_eligible` after
    construction (`aggregate.py:226-228`). Neither affects output, because keys are sorted.
22. **`shlex.join` quoting and `str(Path(arg))` normalisation** shape the summary's Reproduce
    line (`writer.py:44-51`, `deck/parse.py:111-114`). CLI integers use `int(text, 0)`, which
    accepts `0x10`, `1_000` and surrounding spaces, and rejects `010` (`cli.py:61-65`). The line
    always prints the seed in decimal.
23. **`Path.stem` is the deck name** (`deck/parse.py:114`): `a.b.txt` gives `a.b`, and `.txt`
    gives `.txt`.
24. **Exceptions as control flow** (`replayfile.py:84-98,168-183`, `cli.py:339-356`). The
    messages embed Python exception text (`type(exc).__name__`, `KeyError` repr) and `!r`
    reprs (`replayfile.py:193`).
25. **Schema validation failure is an uncaught exception** before any file is written
    (`writer.py:96-97`). The schema regexes use Python `$`, which also matches before a final
    `\n` (`results.v1.schema.json` patterns; `writer.py:20` stale-replay regex, where `\d` is a
    Unicode digit).
26. **Float `sum()` is Neumaier-compensated** in CPython 3.12+ (`writer.py:72`). It only
    reaches timing.json, which is not reproducible. No float `sum`, `fsum`, `max` or `min` over
    float sequences feeds a reproducible file. All other sums are over ints or bools
    (`stats.py:95`, `aggregate.py`, `hypotheses.py:69`).
27. **`_pick` uses the lower median `turns[(len-1)//2]`**, ranks by absolute distance, then
    re-sorts by `(match, game)` (`results.py:76-79`). Draws get at most one replay.
28. **`matches` is never pluralised correctly.** `"{c['matches']} {FMT} matches"`
    (`summary.py:49`) prints `1 BO1 matches`. `str.upper()` and `str.capitalize()` apply to
    `"bo1"` and `"holm"` (`summary.py:177`).
29. **Replay documents do not check themselves.** `expected.turns` and
    `expected.final_state_sha256` are copied from the record, while `winner_seat` and `log` come
    from the re-simulation (`replayfile.py:105-149`).

---

## 15. Open questions

1. **`code_digest` and `versions.package`.** What should Rust emit? A hash over the Rust
   sources plus the packaged data? A constant? The comparison harness must mask
   `versions.code_digest`. Decide whether it should also mask `versions.package` and the summary
   `gcg-sim {package}` substring.
2. **`ai.factory`.** Should Rust emit the literal `"gcg_sim.ai.make_agent('mcts')"` (byte
   identity) or a Rust-specific string (masked)? The `GCG_SIM_AGENT_FACTORY` test hook loads
   Python modules (`agents.py:66-103`). How do Rust tests inject agents, and what string goes
   into `ai.factory` then?
3. **`final_state_sha256`.**
   - Option A: emulate CPython repr for every DSL node that can be live in an end-of-game state
     (lasting, delayed, frames, pending triggers, batches, once_used). That is about 147
     dataclasses and their enums.
   - Option B: export a table from Python mapping each registry object to its content id, keyed
     by something Rust can compute. Python registers programs, continuous effects and filter
     sets lazily (`effects/registry.py:115-129`), and I could not determine whether every
     possible entry can be enumerated ahead of time.
   - Option C: change the digest definition in both implementations, which changes Python
     goldens.
4. **erfc across platforms.** CPython uses the platform libm, so Python itself may differ
   between macOS and Linux in rare cases at the 6-digit boundary. Is byte-identity required
   only on the same machine? This doc assumes yes.
5. **CLI text parity.** Are CLI stdout and stderr texts (progress lines, error messages with
   Python exception text, `!r` reprs) part of the byte-identical surface, or only the report
   files?
6. **`timing.json` in Rust.** Should Rust keep the `python` key (and with what value), or rename
   it? The file is excluded from comparisons either way.
7. **`decision_log` entry format** (`ai/base.py:79-100`, which includes `round(value, 4)` floats
   and labels) is owned by the AI inventory. Confirm that it is documented there with the same
   float and `ensure_ascii` rules as here.
8. **`GameState.to_json()` and `CardInstance.to_json()` field layouts**, which feed the digest,
   belong to the engine inventory. Not repeated here.
9. **Rust rounding coverage.** Rust `{:.N}` and `{:.6}` → parse matched Python on every vector
   tried (2.4, 2.5). A larger randomised differential test, for example over all `k/n` with
   `n ≤ 5000`, has not been run.
