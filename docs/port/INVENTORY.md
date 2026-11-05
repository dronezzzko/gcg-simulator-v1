# Inventory of the Python reference for the Rust port

Exact behaviour of the Python reference (`src/gcg_sim`, branch `rust-port` from `d9d41af`,
Python 3.15.1) that the Rust port must reproduce. It has twelve sections. Each was written by
reading the code and checking claims with short scripts, and each cites `path:line`, lists its
hazards and ends with open questions. The facts below are the cross-cutting ones every
implementer needs. When a section and the code disagree, the code wins: fix the section.

| Section | Scope |
| --- | --- |
| [01 RNG, runner, CLI](inventory/01-rng-runner-cli.md) | SplitMix64, `derive_seed` and all 17 call sites; BO1/BO3 orchestration, seats, choosers; process pool, ordering, interrupts; game records; every CLI flag, message and exit code |
| [02 Reports](inventory/02-reports.md) | `summary.md`, `results.json` (schema 1.1.0), `games.ndjson`, replays, `timing.json`; JSON options; every statistic; `final_state_sha256`; implementation-specific fields |
| [03 Data and decks](inventory/03-data-deck.md) | Card DB load, overrides, def_id order, tokens, B&R, deck grammar, all 13 violation messages, `data status`, fingerprints |
| [04 Effects and IR](inventory/04-effects-ir.md) | 137 DSL classes and 14 enums, program model, registry id order, the 34 custom hooks, golden format, the JSON IR assessment |
| [05 State and derived view](inventory/05-engine-state-view.md) | Enums, records, `clone`, `touch`/`version`, `to_json`, the derived-view fixpoint and cache |
| [06a Core and interpreter](inventory/06a-engine-core-interp.md) | Zone moves, events, trigger collection and batches, rules management, every instruction and decision raised by effects |
| [06b Game, battle, observe](inventory/06b-engine-game-battle-observe.md) | `new_game`, setup, the Step machine, legal-action order per decision kind, action steps, battle, caps, `determinize`, `permute_hidden`, `information_set_key` |
| [07 Search](inventory/07-ai-search.md) | Reference ISMCTS step by step: seeding, RNG draw order, worlds, UCB, tree keys, rollout, leaf value, target extension, root canonicalisation, pruning, no-effect test, pre-game choices, decision log |
| [08 Policy and evaluation](inventory/08-ai-policy-eval.md) | Playout policy scorers and Gumbel-max sampler, evaluation features and weights (decimal and hex), greedy, random, compare, selfplay, tuning |
| [09 Python hazards](inventory/09-python-hazards.md) | Cross-cutting semantic traps with the verified Rust equivalent |
| [10 Tests](inventory/10-tests.md) | All 3985 tests by module, tags, coverage gates, puzzles, testkit, port-or-trace recommendation |
| [11 Python profile](inventory/11-python-profile.md) | Where time goes in one reference `standard` game, with micro-measurements |

## Cross-cutting facts

**Determinism chain.** `SplitMix64` has one `u64` of state. `randrange(n)` uses rejection
against `2^64 - 2^64 mod n`, computed in 128-bit arithmetic, and always draws at least once,
even for `n = 1`. `random()` is `(x >> 11) * 2^-53`. `shuffle` is Fisher-Yates from the top
index down. `derive_seed(master, *path)` takes the SHA-256 of the UTF-8 `repr()` of the tuple
`(master & MASK64, *path)` and reads the first 8 bytes big-endian. Implemented and verified
against Python vectors in `crates/gcg-core` (`tests/vectors.rs`).

**Seed paths.**
- Game: `derive_seed(master, "match", m, "game", g)`.
- Agent: `(..., "agent", seat)`. Agent seeds are keyed by seat, and the deck under test sits in
  seat `m % 2`.
- Per decision: `derive_seed(agent_seed, agent_name, "decision", k)`, where `k` counts from 1
  and includes single-option decisions. Agents are rebuilt every game. The agent name is
  `mcts-{config.name}`; the baseline configs reuse the names `standard`/`strong`.
- Hidden-state RNG: `"hidden-rng"` and `"permuted-rng"`.

**Identity and digests.**
- A card's uid is its index in `st.cards`.
- Main decks get uids after the seeded shuffle and cut: p0 `0..49`, then p1 `50..99`. The
  resource decks follow: p0 `100..109`, p1 `110..119`. Setup then assigns the EX Base and EX
  Resource ids `120..122`.
- `final_state_sha256` hashes `GameState.to_json()`, which includes `version`, so every
  `touch()` must be mirrored. Registry indices are replaced by
  `content_id = sha256(repr(node))[:16]`. The Rust port therefore carries the precomputed
  content ids of build-time programs in the IR, plus a CPython-compatible `repr` of DSL nodes
  for continuous effects created at run time.

**Reference search presets (`ai/config.py:80-111`).**
- Budgets are `clamp(iterations_per_option * n, min, max)` over `n` distinct root moves.
- `fast` is clamp(7n, 16, 64) with exploration 0.15, rollout temperature 0.3, 8 play/draw
  pairs and 96 mulligan samples.
- `standard` is clamp(64n, 160, 640) with exploration 0.3, rollout temperature 0.5, 24 pairs and
  96 samples.
- `strong` is clamp(128n, 320, 1280) with exploration 0.15, rollout temperature 0.3, 48 pairs
  and 192 samples.
- All three use the adopted settings: `value_scale` 1.0, Burst score 4.0, the no-op and
  pruning refinements, merged equivalent moves, observation keys, target extension 3.0 at
  z = 2.0, and the policy rule table and 0-AP face rule.
- Fixed parameters: `prior_weight` 0.5, `prior_temperature` 1.5, `horizon_turns` 2,
  `max_rollout_decisions` 250.
- The baseline presets (`ai/baseline.py`) and `TUNING_CONFIG` are tabulated in section 07,
  §2.1.
- The Rust port keeps every reference preset unchanged behind `gcg-eval`.

**Floating point.**
- The engine and the effect code are integer-only. Floats appear only in `ai/` and `reports/`.
- `sum()` of floats is CPython's Neumaier summation (`gcg_core::pymath::py_sum`). Plain `+=`
  loops stay uncompensated.
- `x ** 2` is libm `pow`, which differs from `x * x` on about 0.1% of inputs. Use
  `gcg_core::libm::square`.
- `math.exp`/`log`/`sqrt` equal Rust `exp`/`ln`/`sqrt` on the same platform.
- `math.erfc` needs the system libm (`gcg_core::libm::erfc`, the one FFI declaration). The
  `libm` crate differs from Apple's `erfc` on 24% of inputs.
- Python `max()`/`min()` keep the first tie (`first_max_by`); Rust's `max_by` keeps the last.
- `repr(float)` and `json.dumps` use shortest digits with ties to even (`zmij`) laid out
  Python-style. Rust's `{:e}` rounds 123 of about 300k exact ties up.
- `format(x, ".Nf")` and `round(x, n)` equal Rust `{:.N}`, and parsing it back.

**Ordering.** Set iteration never affects behaviour: all 117 set sites are sorted or used in
an order-free way (09 §1). `StrEnum` members sort by value, not by declaration
(`wp_gd04_a.py:78`). Dicts keep insertion order. JSON outputs sort keys:
- `results.json` and replays use `indent=2, ensure_ascii=False`;
- `games.ndjson` and the state digest use compact separators with `ensure_ascii=True`;
- the digest's `once_used` list is sorted by its default-separator `json.dumps` text.

**Where time goes.** One reference `standard` BO1 game (seed 1, red-green-zeon vs
blue-white-federation) takes 13.3 s in Python. It makes 15,116 MCTS iterations at about
0.86 ms each and 140,925 `apply` calls. Engine `apply`/`advance` takes about 80% of the time,
`view.derived` about 33% (1.7M calls, 18.6% misses), main-phase legal actions about 20%,
`clone` 3–4%, `determinize` 4–5%, the policy about 6% and the evaluation about 1.5%.

**Card coverage.** The data has 1152 card numbers, so the README is right and "1148" is stale.
There are 1153 definitions, counting the synthetic `UNIT:GD05-089:3/3` variant. All are
implemented: 302 by binding, 538 compiled and 313 vanilla. The registry has 1260 abilities and
623 build-time programs (1830 instructions), plus 3 system programs registered lazily.
