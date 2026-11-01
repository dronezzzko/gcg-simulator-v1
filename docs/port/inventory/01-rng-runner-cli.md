# 01 — RNG, runner and CLI (Python reference inventory for the Rust port)

Scope: `src/gcg_sim/rng.py` (59 lines), `src/gcg_sim/runner/*` (`__init__`, `agents`, `config`,
`digest`, `game`, `instrument`, `match`, `records`, `replayfile`, `run`; 1185 lines) and
`src/gcg_sim/cli.py` (399 lines). Python 3.15.1 (`uv run python -c "import sys; print(sys.version)"`).
Every number and string below was read from the code or printed by a short `uv run python`
snippet; test vectors were computed with the real modules.

Cross-references to other inventories (engine, AI, effects, reports) are marked **[xref]**; they
are described here only as far as the runner/CLI depends on them.

---

## 1. `rng.py` — SplitMix64 and `derive_seed`

### 1.1 SplitMix64 (`src/gcg_sim/rng.py:15-49`)

State is a single unsigned 64-bit integer (`__slots__ = ("state",)`, rng.py:18).
`_MASK64 = (1 << 64) - 1` (rng.py:12).

| Method | Exact behaviour | Line |
|---|---|---|
| `__init__(seed)` | `state = seed & MASK64` (Python int: negative seeds wrap two's-complement, seeds ≥ 2^64 keep the low 64 bits) | rng.py:20-21 |
| `next_u64()` | `state = (state + 0x9E3779B97F4A7C15) mod 2^64`; `z = state`; `z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) mod 2^64`; `z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) mod 2^64`; return `z ^ (z >> 31)` | rng.py:23-28 |
| `randrange(n)` | if `n <= 0`: `raise ValueError("randrange() requires n > 0")`. `limit = 2^64 - (2^64 mod n)`. Loop: `x = next_u64()`; if `x < limit` return `x % n`; else draw again | rng.py:30-38 |
| `random()` | `(next_u64() >> 11) * (1.0 / (1 << 53))` → f64 in [0,1), exact (53-bit integer times 2^-53) | rng.py:40-41 |
| `shuffle(items)` | Fisher–Yates, high to low: `for i in range(len-1, 0, -1): j = randrange(i + 1); items[i], items[j] = items[j], items[i]`. Lists of length 0/1 consume nothing; length L consumes exactly L-1 `randrange` calls | rng.py:43-46 |
| `copy()` | `SplitMix64(self.state)` (state already masked → exact clone) | rng.py:48-49 |

RNG consumption facts the port must keep:
- `randrange(1)` **still consumes one `next_u64`** (returns 0). Verified: two `randrange(1)` calls from
  seed 0 leave `state = 4354685564936845354` (= 2 × golden-gamma mod 2^64).
- `randrange(n)` with n a power of two has `limit = 2^64` (never rejects). 2^64 doesn't fit in a
  u64: compute `rem = 2^64 mod n` (e.g. `((1u128 << 64) % n as u128) as u64`); accept all if
  `rem == 0`, else accept iff `x < 2^64 - rem`.
- Rejection only matters for huge n; all game uses have small n, but the loop must be exact.

Rust reference (behaviour-identical):

```rust
pub struct SplitMix64 { pub state: u64 }
impl SplitMix64 {
    pub fn new(seed: u64) -> Self { Self { state: seed } }          // caller masks (i128 -> u64 via `as` for negatives)
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn randrange(&mut self, n: u64) -> u64 {                     // Python raises ValueError for n <= 0
        assert!(n > 0, "randrange() requires n > 0");
        let rem = ((1u128 << 64) % n as u128) as u64;
        loop {
            let x = self.next_u64();
            if rem == 0 || x < 0u64.wrapping_sub(rem) { return x % n; }
        }
    }
    pub fn random(&mut self) -> f64 { (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64) }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() { let j = self.randrange(i as u64 + 1) as usize; v.swap(i, j); }
    }
}
```

### 1.2 SplitMix64 test vectors (computed with `gcg_sim.rng`)

| Input | Output |
|---|---|
| seed 0, `next_u64` ×5 | `0xe220a8397b1dcdaf, 0x6e789e6aa1b965f4, 0x06c45d188009454f, 0xf88bb8a8724c81ec, 0x1b39896a51a8749b` (decimal first three: 16294208416658607535, 7960286522194355700, 487617019471545679) |
| seed 12345, `randrange(10)` ×10 | `4, 7, 5, 0, 3, 6, 6, 8, 3, 3` |
| seed 12345, `random()` ×3 | `0.1330796686614273, 0.20481663336165912, 0.11954258300911547` |
| seed 0, `random()` ×2 (hex) | `0x1.c4415072f63b9p-1, 0x1.b9e279aa86e58p-2` |
| seed 0, `randrange(8)` ×5 | `7, 4, 7, 4, 3` |
| seed 0, `randrange(2**63+1)` ×3 (rejection: raw draws 1 and 4 rejected, limit 9223372036854775809) | `7960286522194355700, 487617019471545679, 1961750202426094747`; final state 1663341875487337577 |
| seed 7, `shuffle(list(range(10)))` | `[8, 1, 5, 9, 0, 4, 3, 2, 6, 7]`, state after 10372713005361028292 |
| seed 99, `shuffle(['a','b','c','d','e'])` | `['e', 'c', 'b', 'a', 'd']` |
| `SplitMix64(-1).state` / `SplitMix64(2**64).state` | 18446744073709551615 / 0 |

### 1.3 `derive_seed(master, *path)` (`src/gcg_sim/rng.py:52-59`)

```python
material = repr((master & _MASK64, *path)).encode()          # str.encode() = UTF-8
return int.from_bytes(hashlib.sha256(material).digest()[:8], "big")
```

1. Build the Python tuple `(master & MASK64, p1, p2, ...)`. **Only `master` is masked**; path
   elements are used as-is.
2. `repr()` of that tuple, encoded UTF-8.
3. SHA-256; take the first 8 digest bytes; interpret **big-endian** → u64.

Exact `repr` rules for every type that can appear:

| Type | repr | Notes |
|---|---|---|
| tuple | `"(" + ", ".join(repr(e)) + ")"`; one element → `"(x,)"`; empty → `"()"` | Every call site has master + ≥1 path element, so the outer tuple never has the trailing comma |
| int | decimal, no separators, `-` prefix for negatives (`repr(-1) == '-1'`, `repr(-0) == '0'`), unbounded | master is always in [0, 2^64) after masking; all path ints at call sites are ≥ 0 |
| bool | `True` / `False` | bool is an int subclass but repr differs: `derive_seed(m, True) != derive_seed(m, 1)`. No call site passes a bool (selfplay/tuning `swap`, `swap_decks` come from `divmod` → int) |
| None | `None` | never passed |
| str | see algorithm below | all strings at call sites are ASCII `[a-z-]` → `'text'` |
| nested tuple | recursive, same rules (`(1, (2, 'x'), None, False)`) | never passed |

CPython `str.__repr__` algorithm (only the first row matters for current call sites):
- Quote: `"` if the string contains `'` and no `"`; otherwise `'`.
- Per character: the chosen quote char and `\` → backslash-escaped; `\t` `\n` `\r` → `\t` `\n` `\r`;
  other chars `< 0x20` and `0x7f` → `\xhh`; other ASCII literal; non-ASCII → literal if
  `str.isprintable()` (Unicode categories other than Cc, Cf, Cs, Co, Cn, Zl, Zp, Zs) else `\xhh`
  (≤0xff), `\uhhhh` (≤0xffff), `\Uhhhhhhhh`; hex is lowercase.
- Verified: `"a'b"` → `"a'b"`; `'a"b'` → `'a"b'`; `'a\'b"c'` → `'a\'b"c'`; `'\xa0'` → `'\xa0'`;
  `'​'` → `'​'`; `'Ⅱ'` → `'Ⅱ'`; `'😀'` → `'😀'`.

Rust reference for the call sites (ints and quote-free ASCII strings):

```rust
pub enum Part<'a> { I(i64), S(&'a str) }     // extend with u64/bool/None only if a new call site needs them
pub fn derive_seed(master: u64, path: &[Part]) -> u64 {
    let mut s = format!("({}", master);
    for p in path {
        s.push_str(", ");
        match p { Part::I(i) => s.push_str(&i.to_string()), Part::S(t) => { s.push('\''); s.push_str(t); s.push('\''); } }
    }
    s.push(')');                                    // path is never empty at any call site
    let d = sha2::Sha256::digest(s.as_bytes());
    u64::from_be_bytes(d[..8].try_into().unwrap())
}
```

### 1.4 `derive_seed` test vectors

| Call | repr material | Result |
|---|---|---|
| `derive_seed(0, "match", 0, "game", 0)` | `(0, 'match', 0, 'game', 0)` (sha256 `185bc56a63d1888d…`) | 1755213540522821773 |
| `derive_seed(0, "match", 0, "game", 0, "agent", 0)` | `(0, 'match', 0, 'game', 0, 'agent', 0)` | 17560793672738724444 |
| `derive_seed(0, "match", 0, "game", 0, "agent", 1)` | | 3625517813756555173 |
| `derive_seed(42, "match", 3, "game", 2)` | | 9989741335869793347 |
| `derive_seed(2**64-1, "match", 0, "game", 0)` | `(18446744073709551615, 'match', 0, 'game', 0)` | 15581457116308326643 |
| `derive_seed(0, "hidden-rng")` | `(0, 'hidden-rng')` | 9792815011237632761 |
| `derive_seed(123, "permuted-rng")` | | 12472435383953816382 |
| `derive_seed(1, "mcts-standard", "decision", 1)` | | 3015446995358700438 |
| `derive_seed(7, "fuzz-deck", 0)` | | 16597344226867410948 |
| `derive_seed(17560793672738724444, "mcts-standard", "decision", k)`, k=1,2,3 | `(17560793672738724444, 'mcts-standard', 'decision', 1)` | 1730150544371678657, 7983323677775487309, 13530545509583188929 |
| same agent seed, `"mcts-fast"` k=1 / `"random"` k=1 | | 6258787009636402503 / 15004951909873229026 |
| `derive_seed(-1, "a") == derive_seed(2**64-1, "a")`; `derive_seed(2**64, "a") == derive_seed(0, "a")` | | both True (master masking) |

Runner seed table (`game_seed`, `agent_seed` seat 0/1):

| master | m | g | game_seed | agent seat 0 | agent seat 1 |
|---|---|---|---|---|---|
| 0 | 0 | 0 | 1755213540522821773 | 17560793672738724444 | 3625517813756555173 |
| 0 | 0 | 1 | 11117260813679417908 | 4606025368261046392 | 3744458380849835207 |
| 0 | 0 | 2 | 2725400204125086469 | 12774484642438428779 | 2504943837467662436 |
| 0 | 1 | 0 | 5158128127646006833 | 5028792643498733028 | 11580996422662842674 |
| 0 | 1 | 1 | 14405975034222960214 | 7536605293191719025 | 18161531460840985700 |
| 0 | 1 | 2 | 5059158579877036774 | 3992460120206294369 | 5537143511045388390 |
| 42 | 0 | 0 | 15635978022290065305 | 365629232304113289 | 14807187455025436052 |
| 42 | 0 | 1 | 10295357725465482734 | 3622691246023910963 | 4787632887371661982 |
| 42 | 0 | 2 | 83305754255542225 | 2112858189604232061 | 10798352042605320795 |
| 42 | 1 | 0 | 9241416547407311729 | 18399113491244045881 | 12569962269386454829 |
| 42 | 1 | 1 | 10570157879355154231 | 14104248644073941611 | 4310705078660887066 |
| 42 | 1 | 2 | 941313297602106248 | 790292032970715220 | 15006280272993640690 |

### 1.5 Every `derive_seed` call site in `src/` (grep `derive_seed` over `src/gcg_sim`)

All path elements are ASCII string literals or non-negative Python ints.

| # | Site | Exact call | repr template | What it seeds |
|---|---|---|---|---|
| 1 | runner/config.py:76 (`game_seed`) | `derive_seed(master, "match", match_index, "game", game_index)` | `(M, 'match', m, 'game', g)` | the game seed passed to `new_game` → `GameState(seed)` → `st.rng = SplitMix64(seed)` (engine/state.py:475-477). `g` is 0-based |
| 2 | runner/config.py:80 (`agent_seed`) | `derive_seed(master, "match", match_index, "game", game_index, "agent", seat)` | `(M, 'match', m, 'game', g, 'agent', s)` | `AgentSpec.seed` for the agent in **seat** `s` (0/1) — keyed by seat, not role |
| 3 | ai/base.py:65 (`AgentCore._begin`) | `derive_seed(self.seed, self.name, "decision", self._decisions)` | `(A, 'mcts-standard', 'decision', k)` | a fresh `SplitMix64` for each decision of an agent. `A` = agent seed (masked), `k` = 1-based count of `choose()` calls on this agent instance (incremented before use, ai/base.py:64). Names: `"random"` (ai/agents.py:26), `"greedy"` (ai/agents.py:45), `f"mcts-{config.name}"` (ai/agents.py:81) with `config.name` ∈ `"fast"`, `"standard"`, `"strong"` (ai/config.py:84,95,100), `"tuning"` (ai/tuning.py:51), baseline `"standard"`/`"strong"` (ai/baseline.py:21,47) |
| 4 | engine/observe.py:81 (`determinize`) | `derive_seed(seed, "hidden-rng")` | `(S, 'hidden-rng')` | replacement game RNG of the determinized state (after hidden cards were resampled with `SplitMix64(seed)`, observe.py:64) |
| 5 | engine/observe.py:129 (`permute_hidden`) | `derive_seed(seed, "permuted-rng")` | `(S, 'permuted-rng')` | replacement game RNG of the permuted state (identities shuffled with `SplitMix64(seed)`, observe.py:117) |
| 6 | tools/fuzz.py:51 (`deck_for`) | `derive_seed(seed, "fuzz-deck", index)` | `(S, 'fuzz-deck', i)` | RNG for fuzz deck composition (`1 + rng.randrange(4)` copies, fuzz.py:61) |
| 7 | tools/fuzz.py:86 (`play_one`) | `derive_seed(seed, "fuzz-game", index)` | `(S, 'fuzz-game', i)` | fuzz game seed for `new_game` |
| 8 | tools/fuzz.py:87 (`play_one`) | `derive_seed(seed, "fuzz-agent", index)` | `(S, 'fuzz-agent', i)` | RNG choosing uniform random options (fuzz.py:97) |
| 9 | ai/compare.py:140 (`comparison_specs`) | `derive_seed(seed, "pairing", p)` | `(S, 'pairing', p)` | per-deck-pair master seed for `mirrored_specs` (only when >1 pair; a single pair uses `seed` itself, compare.py:136-137) |
| 10 | ai/selfplay.py:104 (`mirrored_specs`) | `derive_seed(master_seed, "block", block, "decks", swap_decks)` | `(S, 'block', b, 'decks', 0\|1)` | game seed; `block, variant = divmod(i, MIRROR_BLOCK)`, `swap_decks, challenger_seat = divmod(variant, 2)` (ints) |
| 11 | ai/selfplay.py:106-107 | `derive_seed(master_seed, "agent", i, 0)` / `(…, "agent", i, 1)` | `(S, 'agent', i, 0)` | agent seeds seat 0 / seat 1 of comparison game `i` |
| 12 | ai/tuning.py:125 (`tuning_games`) | `derive_seed(master, "tuning-game", i)` | `(S, 'tuning-game', i)` | tuning game seed |
| 13 | ai/tuning.py:127-128 | `derive_seed(master, "agent", i, 0)` / `(…, 1)` | `(S, 'agent', i, s)` | tuning agent seeds |
| 14 | ai/tuning.py:266 (`duel_games`) | `derive_seed(master, "duel", block, swap)` | `(S, 'duel', b, 0\|1)` | duel game seed (`swap, seat = divmod(variant, 2)`, ints) |
| 15 | ai/tuning.py:268-269 | `derive_seed(master, "duel-agent", i, 0)` / `(…, 1)` | `(S, 'duel-agent', i, s)` | duel agent seeds |
| 16 | ai/tuning.py:324 (`tune`) | `derive_seed(seed, "round", r)` | `(S, 'round', r)` | per-round master for `tuning_games` |
| 17 | ai/tuning.py:357 (`validate`) | `derive_seed(seed, "validate")` | `(S, 'validate')` | master for `duel_games` |

`reports/results.py:136-138` writes the *strings* `"SplitMix64"`, `"derive_seed(master, 'match', m, 'game', g)"`,
`"derive_seed(master, 'match', m, 'game', g, 'agent', seat)"` into results.json (documentation, not calls).

Other masters: tools that take `--seed` use `type=int` (base 10, negatives allowed — masked by
`derive_seed`): ai/throughput.py:56, tools/fuzz.py:144, ai/compare.py:204, ai/tuning.py:384.

### 1.6 Other SplitMix64 construction / consumption sites (for the engine/AI inventories) [xref]

- engine/state.py:477 `GameState.__init__`: `self.seed = seed` (unmasked) and `self.rng = SplitMix64(seed)`;
  state.py:537 `clone` → `self.rng.copy()`; state.py:616 `from_json` → `SplitMix64(int(d["rng"]))`
  (`to_json` stores `rng.state`, state.py:578).
- engine/game.py:130-153 `new_game`, consumption order: for p in (0, 1): `rng.shuffle(ids)` of the
  main-deck def_ids in decklist order (decklists are sorted card numbers, deck/parse.py:104), then if
  `len(ids) > 1`: `k = 1 + rng.randrange(len(ids) - 1)`; `ids = ids[k:] + ids[:k]` (cut). Resource
  decks are not shuffled here. Then `st.setup_chooser = chooser if chooser is not None else rng.randrange(2)` —
  **the die roll consumes RNG only when `chooser is None`**.
- engine/core.py:136 (`shuffle_deck`), effects/builtins.py:23, engine/interp.py:426 and :534 (`randrange` picks).
- engine/observe.py:64,74-75,117,125.
- ai/agents.py:30,57; ai/mcts.py:102,122-126,143; ai/setup.py:78,98,117; ai/policy.py:97 (`rng.random() or 1e-300`).
- tools/fuzz.py:51,87; testkit.py:421-428.

No use of `random`, `hash()`, `os.urandom`, `uuid` or `time.time()` exists in `src/gcg_sim`
(grep). Wall clock appears only in runner/run.py:30,36-38,122-131 (timing).

---

## 2. Runner (`src/gcg_sim/runner/`)

### 2.1 Public API (`runner/__init__.py:3-31`)

`AgentFactory, AgentSpec, FactoryRef, GameAgent, BenchmarkConfig, agent_seed, dut_seat, game_seed,
state_digest, GameFailure, play_game, replay, replay_game, play_match, BenchmarkRun, DutCardEvents,
GameRecord, MatchRecord, run_benchmark`.

### 2.2 Configuration (`runner/config.py`)

Constants: `FORMATS = ("bo3", "bo1")` (config.py:13), `AI_PRESETS = ("fast", "standard", "strong")`
(config.py:14), `DEFAULT_TURN_LIMIT = 200` (config.py:15), `MAX_ACTIONS_DEFAULT = 20000`
(engine/game.py:33).

`@dataclass(frozen=True) class BenchmarkConfig` (config.py:18-44), field order and defaults:

| # | Field | Type | Default |
|---|---|---|---|
| 1 | `deck_under_test` | `Deck` | required |
| 2 | `benchmark_deck` | `Deck` | required |
| 3 | `matches` | int | required |
| 4 | `seed` | int | 0 |
| 5 | `workers` | int | 1 |
| 6 | `fmt` | str | `"bo3"` |
| 7 | `ai_preset` | str | `"standard"` |
| 8 | `out_dir` | Path | `Path("gcg-sim-out")` |
| 9 | `decision_log` | bool | False |
| 10 | `replays` | int | 3 |
| 11 | `turn_limit` | int | 200 |
| 12 | `max_actions` | int | 20000 |

Derived: `games_per_match = 3 if fmt == "bo3" else 1` (config.py:39-40); `wins_needed = 2 if fmt == "bo3" else 1` (config.py:43-44).

`__post_init__` (config.py:33-36) raises `ValueError("; ".join(problems))`, problems in this order
(config.py:48-61; included only when the check fails):
1. `matches must be at least 1 (got {matches})`
2. `workers must be at least 1 (got {workers})`
3. `format must be one of bo3, bo1 (got {fmt!r})`
4. `AI preset must be one of fast, standard, strong (got {ai_preset!r})`
5. `replays must be 0 or more (got {replays})`
6. `turn limit must be at least 1 (got {turn_limit})`
7. `action limit must be at least 1 (got {max_actions})`
8. `seed must be in [0, 2**64) (got {seed})`

(`!r` = Python repr, e.g. `'bo5'`.) The CLI validates first, so these are not reachable from the CLI.

Seats and seeds:
- `dut_seat(m) = m % 2` (config.py:64-66): even matches DUT in seat 0, odd matches DUT in seat 1.
- `seat_decks(config, m)` (config.py:69-72): `(dut, bench)` if `dut_seat(m) == 0` else `(bench, dut)`,
  each `to_decklist(deck)` = `DeckList(main=deck.main, resources=deck.resources)` (deck/parse.py:117-118);
  `Deck.main`/`resources` are sorted tuples of card numbers, one per copy (deck/parse.py:104, deck/model.py:22) [xref deck].
- `game_seed`, `agent_seed`: §1.5 rows 1–2.
- `own_turn(turn, seat, first_player) = (turn + 1) // 2 if seat == first_player else turn // 2` (config.py:83-85); operands are ≥ 0.

### 2.3 Agents (`runner/agents.py`)

- `AgentSpec` (frozen, slots; agents.py:28-36), field order: `role: str` (`"dut"`/`"bench"`),
  `seat: int`, `preset: str`, `seed: int`, `log: bool`.
- `GameAgent` protocol (agents.py:39-46): attribute `name: str`; `choose(st, player) -> Action`;
  `decision_log() -> list[dict]`.
- `default_agent_factory(spec)` (agents.py:56-63): imports `gcg_sim.ai`
  (`AI_MODULE`, agents.py:24); `ImportError` → `AgentUnavailableError(f"the AI package {AI_MODULE!r} is not available")`
  = `the AI package 'gcg_sim.ai' is not available`. Returns
  `ai.make_agent("mcts", preset=spec.preset, seed=spec.seed, log=spec.log)` (`DEFAULT_AGENT_KIND = "mcts"`,
  agents.py:25). **role and seat are ignored.** The MCTS agent name is `"mcts-" + preset name`.
- `resolve_factory(ref)` (agents.py:80-93, `functools.cache`d per process): `target, sep, attr = ref.rpartition(":")`;
  if `sep`, `target` or `attr` is empty → `ValueError(f"agent factory must look like 'module:attr' or 'file.py:attr': {ref!r}")`;
  `target` ending in `.py` → load file via `_load_file_module(Path(target).resolve())` (module name
  `"_gcg_sim_agent_factory_" + sha256(str(path)).hexdigest()[:12]`, agents.py:66-77) else
  `importlib.import_module(target)`; non-callable attribute → `ValueError(f"agent factory {ref!r} is not a callable attribute")`.
- `FactoryRef(ref)` (agents.py:96-103): picklable; `__call__` resolves then calls.
- `describe_factory(factory)` (agents.py:106-114) — written to results.json `ai.factory` (reports/results.py:125):
  - default factory → `gcg_sim.ai.make_agent('mcts')` (literal, with Python repr quotes)
  - `FactoryRef` with `.py` target → `f"{Path(target).name}:{attr}"` (e.g. `random_agent.py:make_random_agent`)
  - `FactoryRef` with module target → the ref string unchanged
  - other callables → `f"{__module__}.{__qualname__}"` (`"?"` if no module).

### 2.4 Playing one game (`runner/game.py:106-166`)

`play_game(config, match_index, game_index, chooser, *, agent_factory=None) -> GameRecord`.
Exact order of operations:
1. `factory = agent_factory or default_agent_factory`; `dut = match_index % 2`;
   `seed = game_seed(config.seed, match_index, game_index)` (game.py:115-117).
2. Inside `try` (game.py:118-129):
   1. `st = new_game(seat_decks(config, m), seed, chooser=chooser, turn_limit=config.turn_limit, max_actions=config.max_actions)`
      (consumes game RNG as in §1.6; validates deck construction).
   2. `agents = _make_agents(...)` (game.py:62-76): builds `AgentSpec(role=_role(seat, dut), seat, preset=config.ai_preset, seed=agent_seed(config.seed, m, g, seat), log=config.decision_log)`
      for seat 0 then seat 1 and calls `factory(spec0)`, then `factory(spec1)`. New agent objects every game
      (so the per-decision counter `k` of §1.5 row 3 restarts at 1 every game).
   3. `observer = GameObserver(st, dut)`; `log = [] if config.decision_log else None`.
   4. `actions = _drive(st, agents, observer, log)`.
3. Exceptions (game.py:130-136): `GameFailure` and `AgentUnavailableError` propagate unchanged
   (`except GameFailure, AgentUnavailableError:` is PEP 758 syntax = tuple). Any other `Exception`
   (not `BaseException`: KeyboardInterrupt passes through) becomes
   `GameFailure(f"match {match_index} game {game_index + 1} (seed {seed}) failed: {type(exc).__name__}: {exc}")`
   — game number is **1-based** in this message. Example observed:
   `match 0 game 1 (seed 1755213540522821773) failed: ValueError: agent factory must look like 'module:attr' or 'file.py:attr': 'bogus'`.
   An illegal agent action surfaces as `...failed: IllegalActionError: ...` (tests/runner/test_pool.py:43).
4. If `st.winner is None or st.end_reason is None`:
   `GameFailure(f"match {match_index} game {game_index + 1} stopped without a result")` (game.py:137-138).
5. Build the `GameRecord` (game.py:139-166, §2.8). `observer.dut_events(st)` and `state_digest(st)` run
   **outside** the try (their exceptions are not wrapped).

`_drive(st, agents, observer, log)` (game.py:79-103):

```
actions = []; logged = [0, 0]
while st.pending is not None:
    decision = st.pending; version = st.version
    agent = agents[decision.player]
    action = agent.choose(st, decision.player)
    if st.version != version or st.pending is not decision:      # identity check
        raise GameFailure(f"agent {agent.name!r} mutated the game state")
    if log is not None:
        entries = agent.decision_log()                             # full list copy
        log.extend(entries[logged[decision.player]:]); logged[decision.player] = len(entries)
    observer.before(st, decision, action)
    apply(st, action)
    observer.after(st, decision, action)
    k, a, b, c = action.to_json(); actions.append((str(k), int(a), int(b), int(c)))
```

`action.to_json()` = `[kind.value, a, b, c]` (engine/state.py:23-24). Every applied action is
recorded, including setup (`go_first`, `keep`/`redraw`). The decision log is the interleaving of
both agents' new entries in decision order.

`result_label(winner_seat, dut)` (game.py:38-39): `"draw"` if `winner_seat < 0` else role of the seat.

`match_point(st, dut, fmt)` (game.py:42-51):
- `st.winner is None` → `GameFailure("the game has not ended")`.
- `st.winner >= 0` → role of the winner.
- `fmt == "bo3"` and `st.end_reason is EndReason.BOTH_DEFEATED` → role of seat `1 - st.active`
  (the turn player loses, TRM 5.2).
- otherwise `None` (turn limit / action limit draws; BO1 simultaneous defeat).

End reasons (`EndReason` values): `battle_damage`, `deck_out`, `concede`, `both_defeated`,
`turn_limit`. `st.winner = -1` for `both_defeated` (engine/core.py:842-847) and for `turn_limit`, which
is also used when `st.action_count > st.max_actions` (engine/game.py:327-329, 735-738) [xref engine].

`match_loser_seat(record)` (game.py:54-59): `None` if `record.match_point is None`; else
`1 - (dut_seat if match_point == "dut" else bench_seat)`.

### 2.5 Match sequencing (`runner/match.py:19-46`)

```
games = []; points = {"dut": 0, "bench": 0}; chooser = None
for game_index in range(config.games_per_match):              # 3 for bo3, 1 for bo1
    record = play_game(config, match_index, game_index, chooser, agent_factory=...)
    games.append(record)
    if record.match_point is not None: points[record.match_point] += 1
    if max(points.values()) >= config.wins_needed: break       # 2 for bo3, 1 for bo1
    chooser = match_loser_seat(record)
winner = "dut" if dut_points > bench_points else "bench" if bench_points > dut_points else "draw"   # match.py:11-16
```

Who chooses Player One:
- Game 1 (`game_index 0`): `chooser=None` → seeded die roll `randrange(2)` on that game's RNG, after
  both deck shuffles/cuts (engine/game.py:153). The seat `st.setup_chooser` gets a `CHOOSE_FIRST`
  decision with options `(GO_FIRST(chooser), GO_FIRST(1 - chooser))` in that order (engine/game.py:162-168);
  the agent decides who actually goes first (`st.first_player`).
- Later games: the previous game's match-scoring loser (seat). If the previous game gave no match
  point (draw), `chooser=None` again → die roll with the new game's RNG.
- Game seeds depend only on `(master, m, g)`; outcomes only affect `chooser`.
- BO3 can end with fewer than 2 points after 3 games (draws): e.g. DUT win + 2 draws → DUT wins the
  match 1–0; 1–1 or 0–0 → `"draw"`.

`MatchRecord(match_index, dut_seat=m % 2, games=tuple(games), winner, dut_points, bench_points)`.

### 2.6 Running a benchmark (`runner/run.py`)

`run_benchmark(config, progress=None, *, agent_factory=None) -> BenchmarkRun` (run.py:109-140):
1. `factory = agent_factory or default_agent_factory`; `started_at = _utc_now()`
   (`datetime.now(UTC).isoformat(timespec="seconds")`, e.g. `2026-11-01T12:00:00+00:00`, run.py:29-30);
   `start = perf_counter()`.
2. `processes_used(config)` (run.py:83-85) `= 1 if workers <= 1 or matches == 1 else min(workers, matches)`.
   If 1 → serial, else parallel.
3. Serial (run.py:45-53): for `m` in `0..matches-1`: `_timed_match` (play_match + perf_counter
   duration); then `progress(m + 1, matches)`.
4. Parallel (run.py:88-106): `ProcessPoolExecutor(max_workers=processes_used, mp_context=get_context("spawn"), initializer=_ignore_sigint)`
   (workers set `SIGINT` to `SIG_IGN`, run.py:41-42). Submit **all** matches up front in index order
   (`{executor.submit(_timed_match, config, m, factory): m for m in range(matches)}`).
   `_collect` (run.py:67-80): `pending = set(futures)`; loop `done, pending = wait(pending, FIRST_COMPLETED)`;
   for each `fut in sorted(done, key=match_index)`: `results[idx] = fut.result()` (re-raises the
   worker's exception), then `progress(len(results), total)` — progress receives the *count* of
   finished matches (1, 2, …, total), never a match index (tests/runner/test_pool.py:29-40).
   On any `BaseException` (worker exception, KeyboardInterrupt, exception from `progress`):
   `_terminate` = `executor.terminate_workers()` then join the executor's manager thread with
   timeout `TERMINATE_TIMEOUT_S = 10.0` (run.py:26, 56-64), then re-raise. No partial results.
   On success `executor.shutdown(wait=True)` and return `[results[m] for m in range(matches)]`.
5. `RunTiming(started_at, finished_at=_utc_now(), wall_seconds, match_seconds (per match, match order), workers=processes_used)`
   (records.py:187-195) — the only non-deterministic data, written only to timing.json [xref reports].
6. `BenchmarkRun(config, matches=tuple(match records in index order), timing, agent_factory=describe_factory(factory))`;
   `BenchmarkRun.games` = all games flattened in (match, game) order (records.py:205-207).

Determinism contract: each match is a pure function of `(config, master seed, match index)`;
serial and parallel runs give identical `matches` and identical results.json
(tests/runner/test_pool.py:18-26). Spawned workers each build their own effect registry in
first-use order, which is why `final_state_sha256` uses the canonical digest (§2.9).

Failures: serial → the first failing match in index order raises. Parallel → whichever failing
future is first observed (completion batches, sorted by index within a batch) — the reported
message can vary between runs when several matches fail.

### 2.7 Per-card instrumentation (`runner/instrument.py`)

Constants: `PLAY_KINDS = {PLAY_UNIT, PLAY_BASE, PLAY_COMMAND, PAIR}` (instrument.py:20-22);
`IN_PLAY_ZONES = (BATTLE, PAIRED, BASE, RESOLVING)` (instrument.py:23; Zone ints 3, 11, 5, 9);
`DRAW_EVENTS = {"draw", "shield_to_hand"}` (:24);
`SEEN_EVENTS = {"play", "deployed", "paired", "burst_revealed", "command_activated"}` (:25).

`GameObserver(st, dut_seat)` state (instrument.py:35-47): `dut`, `bench = 1 - dut`,
`initial_hand = ()`, `redrew = False`, `opening_hand = ()`, `tracking = False`,
`drawn: dict[int, None]` (ordered set of uids), `plays: list[PlayEvent]`, `played_uids: set[int]`,
`bench_seen: set[str]`, `_history = st.history` (the list object), `_history_done = 0`.

`before(st, decision, action)` — called after `choose`, before `apply` (instrument.py:49-60):
1. If `decision.kind is REDRAW and decision.player == dut`: `initial_hand = tuple(st.zones[dut][HAND])`.
2. If `action.kind not in PLAY_KINDS`: return.
3. `number = card_number(st, action.a)` (`V.cdef(st, uid).card_number`).
4. If `decision.player == dut`: append `PlayEvent(number, st.turn, own_turn(st.turn, dut, st.first_player))`; `played_uids.add(action.a)`.
   Else `_see_bench(st, action.a)`.

`after(st, decision, action)` — after `apply` (instrument.py:62-73):
1. If `decision.kind is REDRAW and decision.player == dut`: `redrew = action.kind is REDRAW`;
   `opening_hand = tuple(hand)`; `tracking = True`.
2. `_scan_history(st)`:
   - if `st.history is not self._history` (the engine **replaced** the list: end of setup
     engine/game.py:190, each new turn engine/game.py:336): adopt the new list and set `_history_done = 0`.
   - for each `h` in `st.history[_history_done:]`: skip if `h.uid < 0`; `owner = st.cards[h.uid].owner`;
     if `tracking and h.kind in DRAW_EVENTS and owner == dut`: `drawn.setdefault(h.uid)`;
     elif `h.kind in SEEN_EVENTS and owner == bench`: `_see_bench(st, h.uid)`.
   - `_history_done = len(st.history)`.
3. If `tracking`: every uid in DUT's hand → `drawn.setdefault(uid)`.
4. For zone in `IN_PLAY_ZONES`, for uid in `st.zones[bench][zone]`: `_see_bench(st, uid)`.

`_see_bench(st, uid)` (instrument.py:75-78): add `cdef.card_number` iff
`st.cards[uid].owner == bench and not cdef.is_token`.

`dut_events(st)` (instrument.py:94-107): `numbers(uids) = tuple(sorted(card_number(u) for u in uids))`;
`never = [u for u in drawn if u not in played_uids]`; returns `DutCardEvents(initial_hand=numbers(initial_hand), redrew, opening_hand=numbers(opening_hand), drawn=numbers(drawn), played=tuple(plays), never_played=numbers(never), in_hand_at_end=numbers(st.zones[dut][HAND]))`.
All card-number outputs are sorted (string order); `played` keeps chronological order.

### 2.8 Records (`runner/records.py`)

Role strings: `DUT = "dut"`, `BENCH = "bench"`, `DRAW = "draw"` (records.py:11-13).
`ActionJson = tuple[str, int, int, int]`.

`PlayEvent(card_number: str, turn: int, own_turn: int)`; `to_json` → `[card_number, turn, own_turn]` (records.py:18-31).

`DutCardEvents` (records.py:34-71), field order = `to_json` key order:
`initial_hand` (list[str]), `redrew` (bool), `opening_hand`, `drawn`, `played` (list of
`[str,int,int]`), `never_played`, `in_hand_at_end`.

`GameRecord` (records.py:74-132) — one record per played game. Field order = `to_json` key order:

| # | Key | JSON type | Value |
|---|---|---|---|
| 1 | `match_index` | int | m |
| 2 | `game_index` | int | g, 0-based |
| 3 | `seed` | int (u64) | `game_seed(master, m, g)` |
| 4 | `agent_seeds` | [int, int] | `agent_seed(master, m, g, 0)`, `agent_seed(…, 1)` (recomputed at game.py:143-146) |
| 5 | `agents` | [str, str] | `agents[0].name`, `agents[1].name` (seat order), e.g. `"mcts-standard"` |
| 6 | `dut_seat` | int | m % 2 |
| 7 | `chooser` | int or null | the `chooser` argument given to `new_game` (null = die roll) |
| 8 | `chooser_seat` | int | `st.setup_chooser` |
| 9 | `first_player` | int | `st.first_player` |
| 10 | `dut_on_play` | bool | `first_player == dut_seat` |
| 11 | `winner` | str | `result_label(st.winner, dut)` — actual game winner, `"draw"` for winner −1 |
| 12 | `end_reason` | str | `st.end_reason.value` |
| 13 | `match_point` | str or null | `match_point(st, dut, fmt)` |
| 14 | `turns` | int | `st.turn` |
| 15 | `active_at_end` | int | `st.active` |
| 16 | `actions` | [[str,int,int,int], …] | every applied action |
| 17 | `redraws` | [bool, bool] | `st.redraws[0]`, `st.redraws[1]` |
| 18 | `dut_cards` | object | `DutCardEvents.to_json()` |
| 19 | `bench_seen` | [str] | `sorted(observer.bench_seen)` |
| 20 | `final_state_sha256` | str | `state_digest(st)` (64 lowercase hex) |
| 21 | `turn_limit` | int | config.turn_limit |
| 22 | `max_actions` | int | config.max_actions |
| 23 | `decision_log` | list or null | null unless `--decision-log`; else the agents' entry dicts in decision order [xref AI] |

Property `bench_seat = 1 - dut_seat`. `from_json` (records.py:134-162) converts with `int()`,
`str()`, `bool()`; `decision_log` via `d.get` (missing → None). games.ndjson lines are
`json.dumps(g.to_json(), sort_keys=True, separators=(",", ":")) + "\n"` (reports/writer.py:55-57,
`ensure_ascii` default True) [xref reports].

`MatchRecord.to_json` (records.py:176-184; not used by src writers): keys `match_index, dut_seat, winner, dut_points, bench_points, games` (`games` = list of `game_index`).

### 2.9 Canonical state digest (`runner/digest.py`)

Constants (digest.py:20-23): `LASTING_EFFECT = 0`, `LASTING_FILTERS = 9` (positions in
`Lasting.to_json()`, engine/state.py:231-245), `TRIGGER_PROGRAM = 0` (position in `TriggerInst.to_json()`),
`LASTING_SOURCE = -1`, `ONCE_TAGS = (REDUCE_ONCE_TAG, PREVENT_ONCE_TAG) = (-7, -8)` (engine/core.py:23-24).

`content_id(obj) = sha256(repr(obj).encode()).hexdigest()[:16]` (digest.py:26-27) — 16 lowercase hex chars.

`canonical_state(st, reg=None)` (digest.py:36-49), `r = reg or get_registry()`, `d = st.to_json()` [xref engine], then in this order:
1. For each `le` in `d["lasting"]` then `d["delayed"]` (lists):
   `le[0] = content_id(r.continuous[le[0]])`; if `le[9] >= 0`: `le[9] = content_id(r.filter_sets[le[9]])`
   (a `-1` filter index stays the integer `-1`).
2. For each `f` in `d["frames"]` (dicts): `f["program_id"] = content_id(r.programs[f["program_id"]])`.
3. For each `t` in `d["pending_triggers"]`, then each `t` of each batch in `d["batches"]` (lists):
   `t[0] = content_id(r.programs[t[0]])`.
4. `d["once_used"] = sorted((_once_key(r, k) for k in d["once_used"]), key=json.dumps)` where
   `_once_key` (digest.py:30-33): if `len(k) >= 3 and k[0] in (-7, -8) and k[1] == -1` →
   `[k[0], k[1], content_id(r.continuous[k[2]]), *k[3:]]`, else `k` unchanged. The sort key is
   `json.dumps(list)` with **default** settings: `"[" + ", ".join(items) + "]"`, ints in decimal,
   strings double-quoted (ensure_ascii). Sorted by Python string order (code points).

`state_digest(st, reg=None)` (digest.py:52-56) =
`sha256(json.dumps(canonical, sort_keys=True, separators=(",", ":")).encode()).hexdigest()`
(`ensure_ascii=True`; all keys sorted recursively; `null`, `true`, `false`; no floats were found in a
canonical state — types seen: dict, list, int, str, bool, None; no non-ASCII strings).

`repr(obj)` for registry entries is CPython's dataclass/enum repr of the effect DSL [xref effects]:
- dataclass: `QualName(field=repr(value), ...)` for fields with `repr=True`, declaration order,
  separator `", "`; e.g. `StatMod(ap=2, hp=0)`;
- enum: `<EnumName.MEMBER: repr(value)>`, e.g. `<Kw.REPAIR: 'Repair'>`; DSL enums seen with str
  values: `Kw, P, Side, Loc, Duration, CardKind, Stat, Op, RuleKind, DamageKind, Ev`;
- tuples per §1.3 (one-element tuples end in `,)`), ints, `None`, `True`/`False`;
- strings: quote choice matters — strings with an apostrophe and no double quote use `"…"`, and
  non-ASCII printable characters stay literal, e.g. `"Char's Zaku Ⅱ|Zeon|3|1|"` (15 such strings in
  the registry);
- No dict/set/frozenset/float values in program/continuous/filter reprs (scanned all 623
  programs at startup); no DSL class defines a custom `__repr__`.
- Example (623 programs at startup, longest repr 1697 chars):
  `Program(instrs=(Recover(ref=This(), amount=KwAmount(keyword=<Kw.REPAIR: 'Repair'>, ref=This())),), label='keyword:Repair')`.

### 2.10 Replays (`runner/game.py:169-194`, `runner/replayfile.py`)

`replay_game(decks, seed, chooser, actions, *, turn_limit, max_actions)` (game.py:169-182):
`new_game(decks, seed, chooser=chooser, turn_limit=..., max_actions=...)`, then
`apply(st, Action.from_json(list(a)))` for every action. `replay(record, config)` uses
`seat_decks(config, record.match_index)` and the record's seed/chooser/actions/limits.

Constants: `REPLAY_FORMAT = "gcg-sim-replay"`, `REPLAY_VERSION = 1`, `CARD_CHOICES = {SELECT, DISCARD, EXCESS}` (replayfile.py:20-23).

`_label(st, uid)` (replayfile.py:30-34): `str(uid)` if not `0 <= uid < len(st.cards)`, else `f"{card_number} {name}"`.

`describe_action(st, dec, a)` (replayfile.py:45-65) → `f"{text} [{a}]"`, where `{a}` is
`Action.__str__` = `f"{kind.value}({','.join(str(x) for x in (a, b, c) if x != -1)})"`
(engine/state.py:30-32; only `NO_ARG = -1` is dropped, `PLAYER_TARGET = -2` is printed) and `text` is:

| Action kind | text |
|---|---|
| `play_unit`, `play_base`, `play_command` | `play {label(a)}` |
| `pair` | `pair {label(a)} with {label(b)}` |
| `activate` | `activate {label(a)}` |
| `attack` | `attack {target} with {label(a)}`, `target = "the player"` if `b == -2` else `label(b)` |
| `block` | `block with {label(a)}` |
| `go_first` | `P{a} goes first` |
| `select` | `_describe_choice`: if `dec.kind is ARRANGE and b != -1` → `f"{'bottom' if a else 'top'}: {label(b)}"` (Python truthiness: any non-zero `a` = bottom); elif `dec.kind in CARD_CHOICES and dec.ctx("mode") == -1 and dec.ctx("rest_sub") == -1` → `select {label(a)}`; else `select option {a}` |
| anything else | `kind.value` (e.g. `keep`, `end_main`, `pass`, `yes`, `no`, `order`, `done`, `no_block`, `redraw`) |

`Decision.ctx(key, default=-1)` returns the first matching value in `context` (engine/state.py:45-49).

`replay_with_log(decks, seed, chooser, actions, *, turn_limit, max_actions)` (replayfile.py:75-101):
1. `new_game(...)`; `UnimplementedCardError` → `ReplayError(f"the replay uses a card this version cannot play: {exc}")`.
2. For `index, raw` in `enumerate(actions)`: `action = Action.from_json(list(raw))` (**before** the pending check);
   `dec = st.pending`; if None → `ReplayError("the replay has more actions than the game had decisions")`;
   append `f"T{st.turn} P{dec.player} {dec.kind.value}: {describe_action(st, dec, action)}"`;
   `apply`; `IllegalActionError` → `ReplayError(f"action {index} {action} is not legal in the replayed game")`.
3. Append `f"T{st.turn} game over: winner P{st.winner} ({reason})"`, `reason = end_reason.value` or
   `"not finished"` (winner None prints `PNone`).

Example log lines (real run): `T0 P1 choose_first: P1 goes first [go_first(1)]`,
`T0 P1 redraw: keep [keep()]`, `T1 P1 main: end_main [end_main()]`,
`T16 P0 main: attack GD01-004 Guncannon with GD01-031 Gelgoog [attack(14,51)]`,
`T11 P0 main: activate GD04-122 Jaburo [activate(45,765,0)]`, `T23 game over: winner P1 (battle_damage)`.

`replay_document(record, config, label)` (replayfile.py:104-150) re-simulates with
`replay_with_log` and returns (key order as built; the writer sorts keys,
`json.dumps(indent=2, sort_keys=True, ensure_ascii=False) + "\n"`, reports/writer.py:33-34):
`format`, `version`, `label`, `match_index`, `game_index`, `seed`, `chooser`, `turn_limit`,
`max_actions`, `seats` (list for seat 0, 1 of `{seat, role, deck (Deck.name), agent, agent_seed,
main: {card_number: count} sorted by card number, resources: same}`), `actions`, `expected`
(`winner` (role label), `winner_seat` (**from the re-simulation**, `st.winner`), `end_reason`,
`turns`, `final_state_sha256`), `log` (list of lines), `decision_log` (list or null).

`check_replay(doc)` (replayfile.py:164-205):
1. `doc.get("format") != "gcg-sim-replay" or doc.get("version") != 1` → `ReplayError("not a gcg-sim-replay v1 document")`.
2. Inside `try`: `seats = sorted(doc["seats"], key=lambda s: int(s["seat"]))` (stable);
   `decks = (_decklist(seats[0]), _decklist(seats[1]))` where `_decklist` expands
   `{card: count}` into `tuple(sorted(n for n, c in counts.items() for _ in range(int(c))))` for
   `main` then `resources` (replayfile.py:68-72); then `replay_with_log(decks, int(doc["seed"]),
   None if doc["chooser"] is None else int(doc["chooser"]), doc["actions"],
   turn_limit=int(doc["turn_limit"]), max_actions=int(doc["max_actions"]))` — arguments evaluated
   left to right (seed, chooser, actions, turn_limit, max_actions), so a missing key is reported in
   that order; then `expected = doc["expected"]`.
   `ReplayError` re-raised; `KeyError, TypeError, IndexError, ValueError` →
   `ReplayError(f"malformed replay document: {type(exc).__name__}: {exc}")` (e.g.
   `malformed replay document: KeyError: 'seats'`; deck-construction failures arrive as
   `DeckConstructionError` (a ValueError); unknown card number → `KeyError: 'XX'`).
3. `actual = {"winner_seat": st.winner, "end_reason": value or None, "turns": st.turn, "final_state_sha256": state_digest(st)}`;
   `mismatches = tuple(f"{k}: expected {expected.get(k)!r}, replay gave {v!r}" for k, v in actual.items() if expected.get(k) != v)`
   (key order as listed; Python `repr` and Python `!=`).
4. `ReplayCheck(ok=not mismatches, mismatches, winner_seat, end_reason, turns, final_state_sha256, log=tuple(log))`.

---

## 3. CLI (`src/gcg_sim/cli.py`)

### 3.1 Entry point, exit codes, top-level flow

- Console script `gcg-sim = "gcg_sim.cli:main"` (pyproject.toml:14-15); the wrapper calls
  `sys.exit(main())`. There is no `gcg_sim/__main__.py` and no `if __name__ == "__main__"` in cli.py.
- Exit codes (cli.py:33): `EXIT_OK = 0`, `EXIT_INVALID = 1`, `EXIT_USAGE = 2`, `EXIT_INTERRUPTED = 130`.
- `main(argv=None)` (cli.py:386-399):
  1. `args = build_parser().parse_args(argv)`; `SystemExit` → return `exc.code` if int else 2
     (argparse exits 0 after `--help`, 2 on usage errors; it has already printed).
  2. `_dispatch` (cli.py:376-383): `benchmark` → `cmd_benchmark`, `validate` → `cmd_validate`,
     `replay` → `cmd_replay`, anything else (only `data status`) → `cmd_data_status`.
  3. `CliError` → print `f"error: {exc}"` to **stderr**, return 1.
  4. `KeyboardInterrupt` → print `interrupted: workers stopped, no reports written` to stderr,
     return 130 (any phase of a command, including report writing).
  5. Any other exception propagates: Python traceback on stderr, process exit status 1.
- All `print` calls end lines with `\n`; stdout/stderr are text streams (UTF-8 in practice).
- Test hook (cli.py:32, 162-164): env `GCG_SIM_AGENT_FACTORY`, if set and non-empty →
  `FactoryRef(value)` replaces the search AI for `benchmark`.

### 3.2 Parser (cli.py:68-95)

`ArgumentParser(prog="gcg-sim", description="Gundam Card Game deck benchmarking simulator.")`;
Python 3.15 defaults: `allow_abbrev=True`, `suggest_on_error=True`, `color=True`, `exit_on_error=True`,
`add_help=True` (printed via `inspect.signature`). `sub = add_subparsers(dest="command", required=True)`.

| Command | Argument | Kind / type | Default | Validation / choices | help |
|---|---|---|---|---|---|
| `benchmark` | `deck_under_test` | positional, `Path` | — | — | `deck file of the deck under test` |
| | `benchmark_deck` | positional, `Path` | — | — | `deck file of the benchmark deck` |
| | `--matches MATCHES` | `_positive_int` | **required** | ≥ 1 | `number of matches` |
| | `--seed SEED` | `_seed` | 0 | 0 ≤ v < 2^64 | `master seed (default 0)` |
| | `--workers WORKERS` | `_positive_int` | 1 | ≥ 1 | `worker processes` |
| | `--format {bo3,bo1}` | str choices | `bo3` | `FORMATS` | `match format` |
| | `--ai-preset {fast,standard,strong}` | str choices | `standard` | `AI_PRESETS` | `AI strength` |
| | `--out OUT` | `Path` | `Path("gcg-sim-out")` | — | `report directory` |
| | `--decision-log` | store_true | False | — | `record AI decision logs` |
| | `--replays REPLAYS` | `_non_negative_int` | 3 | ≥ 0 | `replays saved per result` |
| `validate` | `deck` | positional, `Path` | — | — | (none) |
| `data` → `status` | `--json` | store_true | False | — | `print JSON instead of text` |
| `replay` | `replay_json` | positional, `Path` | — | — | (none) |
| | `--log` | store_true | False | — | `print the game log` |

Subcommand help strings: `benchmark` = `play a deck under test against a benchmark deck`;
`validate` = `check a deck file against the construction rules`; `data` = `packaged data`
(its sub-parsers: `dest="data_command", required=True`); `status` = `data snapshot, rules, B&R, coverage, conflicts`;
`replay` = `re-simulate a saved replay and verify its outcome`.
`turn_limit` and `max_actions` are not exposed (always 200 / 20000).

Type converters (cli.py:40-65), all built on `_int(text) = int(text, 0)`:
- `_int`: `ValueError` → `ArgumentTypeError(f"not an integer: {text!r}")`.
- `_positive_int`: `< 1` → `must be at least 1, got {value}`.
- `_non_negative_int`: `< 0` → `must be 0 or more, got {value}`.
- `_seed`: outside `[0, 2**64)` → `must be in [0, 2**64), got {value}`.
`int(text, 0)` grammar (verified): optional surrounding whitespace, optional `+`/`-`; prefixes
`0x`/`0X`, `0o`/`0O`, `0b`/`0B`; single underscores between digits (`1_0` = 10; `1__0`, `_1` invalid);
a non-zero decimal must not have a leading zero (`010` invalid, `00` = 0); Unicode decimal digits
accepted (`'٣'` = 3); `1.5`, `1e3`, `''` invalid. The error message shows the original text with
Python repr quotes, e.g. `not an integer: '1.5'`.

### 3.3 argparse behaviour (Python 3.15)

- Usage errors: `usage: …\n` (the usage of the parser that failed) then
  `{prog}: error: {message}\n` on stderr, exit 2. Sub-parser prog is `gcg-sim benchmark`,
  `gcg-sim validate`, `gcg-sim data`, `gcg-sim data status`, `gcg-sim replay`.
- Unrecognized arguments are reported by the **top-level** parser:
  `usage: gcg-sim [-h] {benchmark,validate,data,replay} ...` + `gcg-sim: error: unrecognized arguments: {args joined by space}`.
- Messages seen:
  - `the following arguments are required: command` (no subcommand)
  - `the following arguments are required: data_command`
  - `the following arguments are required: deck_under_test, benchmark_deck, --matches` (subset in this order)
  - `argument --matches: expected one argument`
  - `argument --matches: must be at least 1, got 0`; `argument --matches: not an integer: 'x'`
  - `argument --seed: must be in [0, 2**64), got -1` (negative-looking values are accepted as option values)
  - `argument --format: invalid choice: 'bo5', maybe you meant 'bo3'? (choose from 'bo3', 'bo1')`
  - `argument --format: invalid choice: 'BO3' (choose from 'bo3', 'bo1')`
  - `argument --ai-preset: invalid choice: 'weak' (choose from 'fast', 'standard', 'strong')`
  - `argument command: invalid choice: 'bogus' (choose from 'benchmark', 'validate', 'data', 'replay')`
  - `argument --decision-log: ignored explicit argument '1'`
  Suggestion rule: `difflib.get_close_matches(value, choices, 1)` (cutoff 0.6), only when every
  choice is a str (argparse.py:2828-2835).
- Option prefixes are accepted (`--m 1`, `--mat 1` = `--matches 1`); repeated options: last wins;
  options and positionals may be interleaved; `--` ends option parsing (`validate -- --weird`
  reads file `--weird`); `-h/--help` prints help to **stdout**, exit 0, as soon as it is reached
  (`gcg-sim -h benchmark` prints the top-level help).
- Help/usage wrapping width = `shutil.get_terminal_size().columns - 2` (argparse.py:188-191):
  env `COLUMNS`, else the terminal size of stdout, else 80. With `COLUMNS=200` the benchmark usage
  becomes one line of optionals plus a second line of positionals. Colour: ANSI-coloured when
  the stream is a TTY and colours aren't disabled (`NO_COLOR`, `PYTHON_COLORS=0`, `TERM=dumb`);
  `FORCE_COLOR` forces colour. Not coloured when piped.

Verbatim outputs at the default width (80 columns, not a TTY):

`gcg-sim --help` (stdout, exit 0):
```
usage: gcg-sim [-h] {benchmark,validate,data,replay} ...

Gundam Card Game deck benchmarking simulator.

positional arguments:
  {benchmark,validate,data,replay}
    benchmark           play a deck under test against a benchmark deck
    validate            check a deck file against the construction rules
    data                packaged data
    replay              re-simulate a saved replay and verify its outcome

options:
  -h, --help            show this help message and exit
```

`gcg-sim benchmark --help`:
```
usage: gcg-sim benchmark [-h] --matches MATCHES [--seed SEED]
                         [--workers WORKERS] [--format {bo3,bo1}]
                         [--ai-preset {fast,standard,strong}] [--out OUT]
                         [--decision-log] [--replays REPLAYS]
                         deck_under_test benchmark_deck

positional arguments:
  deck_under_test       deck file of the deck under test
  benchmark_deck        deck file of the benchmark deck

options:
  -h, --help            show this help message and exit
  --matches MATCHES     number of matches
  --seed SEED           master seed (default 0)
  --workers WORKERS     worker processes
  --format {bo3,bo1}    match format
  --ai-preset {fast,standard,strong}
                        AI strength
  --out OUT             report directory
  --decision-log        record AI decision logs
  --replays REPLAYS     replays saved per result
```

`gcg-sim validate --help`:
```
usage: gcg-sim validate [-h] deck

positional arguments:
  deck

options:
  -h, --help  show this help message and exit
```

`gcg-sim data --help`:
```
usage: gcg-sim data [-h] {status} ...

positional arguments:
  {status}
    status    data snapshot, rules, B&R, coverage, conflicts

options:
  -h, --help  show this help message and exit
```

`gcg-sim data status --help`:
```
usage: gcg-sim data status [-h] [--json]

options:
  -h, --help  show this help message and exit
  --json      print JSON instead of text
```

`gcg-sim replay --help`:
```
usage: gcg-sim replay [-h] [--log] replay_json

positional arguments:
  replay_json

options:
  -h, --help   show this help message and exit
  --log        print the game log
```

Usage error example (`gcg-sim benchmark a b`, stderr, exit 2):
```
usage: gcg-sim benchmark [-h] --matches MATCHES [--seed SEED]
                         [--workers WORKERS] [--format {bo3,bo1}]
                         [--ai-preset {fast,standard,strong}] [--out OUT]
                         [--decision-log] [--replays REPLAYS]
                         deck_under_test benchmark_deck
gcg-sim benchmark: error: the following arguments are required: --matches
```

### 3.4 Deck loading shared by `validate` and `benchmark` (cli.py:102-126)

`_read_deck(path)`: `load_deck(path)` [xref deck] with:
- `OSError` → `cannot read deck file {path}: {exc.strerror or exc}` (e.g. `No such file or directory`, the C library text);
- `DeckError` → `{path}: {exc}` (e.g. `…/g.txt: deck 'g' has 1 problem(s):\n  - line 1: expected '<count> <card_number> [name]', got 'garbage line'`);
- `UnicodeDecodeError` → `{path}: not UTF-8 text ({exc.reason} at byte {exc.start})` (e.g. `not UTF-8 text (invalid start byte at byte 3)`).

`_legal_deck(path)`: `violations = validate_deck(deck)` [xref deck]; if any →
`CliError("\n".join([f"{path}: {len(v)} violation(s)"] + [f"  [{v.code}] {v.message}" for v in violations]))`.
Output example: `error: …/ill.txt: 1 violation(s)` / `  [MAIN_SIZE] the deck has 49 cards; it must have exactly 50 (rule 6-1-1)`.
`{path}` is `str(Path(arg))` (pathlib-normalised, §Hazards).

### 3.5 `validate` (cli.py:129-136)

stdout: `f"{args.deck}: legal ({len(deck.main)} cards, {len(deck.resources)} resources; {colors})"`, exit 0,
where `colors = "/".join(sorted({db[n].color.value for n in deck.main if color is not None}))` or
`no colors` (cli.py:113-116). Example: `examples/decks/red-green-zeon.txt: legal (50 cards, 10 resources; Green/Red)`.

### 3.6 `benchmark` (cli.py:143-229)

Order: parse → `_legal_deck(deck_under_test)` → `_legal_deck(benchmark_deck)` → `_writable_dir(out)` →
`BenchmarkConfig(dut, bench, matches, seed, workers, fmt=format, ai_preset, out_dir=out, decision_log, replays)` →
`run_benchmark(config, ProgressPrinter(stderr), agent_factory=_agent_factory())` → `write_reports(run, out)` [xref reports] → summary on stdout.

`_writable_dir(path)` (cli.py:176-185; creates nothing): walk `existing = path` up through
`.parent` while `not existing.exists() and existing != existing.parent` (relative paths stop at
`.`); if `existing` is not a directory → `cannot write reports to {path}: {existing} is not a directory`;
if `os.access(existing, W_OK | X_OK)` fails → `cannot write reports to {path}: permission denied on {existing}`.

Run errors (cli.py:204-216):
- `AgentUnavailableError` → `{exc}; the benchmark needs the search AI`
- `GameFailure` → `simulation failed: {exc}`
- `OSError` from `write_reports` → `cannot write reports to {out}: {exc.filename or out}: {exc.strerror or exc}`

`ProgressPrinter(stream=stderr)` (cli.py:143-159): `tty = stream.isatty()`, `last_decile = -1`.
- TTY: `print(f"\rmatches {done}/{total}", end="\n" if done == total else "", flush=True)` (in place).
- Not a TTY: `decile = 10 * done // total`; if `decile != last_decile`: store it and print
  `matches {done}/{total}` + `\n`. Printed `done` values: total 3 → 1,2,3; total 25 →
  1,3,5,8,10,13,15,18,20,23,25; total 100 → 1,10,20,…,100.
- Note: in TTY mode an error after progress lands on the same line (`\rmatches 3/10error: …`).

Success stdout (cli.py:217-228), exit 0:
```
{dut.name} (deck under test) vs {bench.name} (benchmark)
matches: DUT {dut_wins}-{bench_wins} (draws {draws}), win rate {100*rate:.1f}% [95% CI {100*lo:.1f}%-{100*hi:.1f}%] over {n}
games: DUT {…same format, over games…}
reports:
  results  {out/results.json}
  summary  {out/summary.md}
  games    {out/games.ndjson}
  timing   {out/timing.json}
  replays  {len(paths.replays)} file(s) in {out/replays}
```
Label column is `f"  {label:8} {path}"` (left-justified to 8). `matches`/`games` blocks come
from `match_outcomes(run.matches)` / `game_outcomes(run.games)` (keys `n`, `dut_wins`,
`bench_wins`, `draws`, `dut_win_rate.rate`, `dut_win_rate.ci95 = [lo, hi]`; reports/aggregate.py:28-44)
[xref reports]; the games line counts `GameRecord.winner` (actual winners), not match points.
Real example:
```
blue-white-federation (deck under test) vs red-green-zeon (benchmark)
matches: DUT 1-2 (draws 0), win rate 33.3% [95% CI 6.1%-79.2%] over 3
games: DUT 2-4 (draws 0), win rate 33.3% [95% CI 9.7%-70.0%] over 6
reports:
  results  /private/tmp/claude-501/port01/out/results.json
  summary  /private/tmp/claude-501/port01/out/summary.md
  games    /private/tmp/claude-501/port01/out/games.ndjson
  timing   /private/tmp/claude-501/port01/out/timing.json
  replays  5 file(s) in /private/tmp/claude-501/port01/out/replays
```
with stderr `matches 1/3`, `matches 2/3`, `matches 3/3`.

### 3.7 `data status` (cli.py:236-331)

`data_status()` (cli.py:263-291) builds `{"versions": versions(), "snapshot": {k: manifest().get(k) for k in (dataset_version, source_commit, built_at, card_count, set_count, ruling_count, rules_faq_count, errata_count, product_count)}, "banlist": {effective_date, banned: sorted(...), restricted: {card_number: max_copies}, banned_pairs: [list(p.cards) …] (list order kept), attribute_pair_rules: [r.id …]}, "coverage": _coverage(), "conflicts": _conflicts()}` [xref reports/versions, deck/official, cards/db].
`_coverage` (cli.py:236-248): `card_numbers = len(reg.db.real_cards())`, `implemented` = those whose
`reg.cards[def_id].script is not None`, `by_type = {type: {"cards", "implemented"}}` with types sorted.
`_conflicts` (cli.py:251-260): `resolutions` (len of overrides.json `resolutions`), `card_data_overrides`,
`card_numbers_overridden` (distinct), `curated_conflicts` (len of curated_conflicts.json `conflicts`).

`--json`: `print(json.dumps(status, indent=2, sort_keys=True, ensure_ascii=False))` (cli.py:328): 2-space
indent, `": "` and `","` at line ends, keys sorted, non-ASCII literal, trailing `\n` from print.

Text (cli.py:294-322), one line each, exact templates:
```
gcg-sim {package} (results schema {results_schema})
card data: gcg-api dataset {dataset_version}
  source commit {source_commit}, built {built_at}
  {card_count} printings, {card_numbers} card numbers, {set_count} sets, {ruling_count} rulings, {rules_faq_count} rules FAQ entries, {errata_count} errata
rules: Comprehensive Rules Ver. {rules_version} (effective {rules_effective_date})
banned & restricted list: effective {effective_date} ({len(banned)} banned, {len(restricted)} restricted, {len(banned_pairs)} banned pairs, {len(attribute_pair_rules)} attribute pair rule(s))
BO3 match rules: effective {bo3_rules_effective_date}
implementation coverage: {implemented}/{card_numbers} card numbers ({pct:.1f}%)
  {TYPE}: {implemented}/{cards}            (one line per type, sorted type names)
conflicts: {resolutions} resolutions in overrides.json ({card_data_overrides} card-data overrides on {card_numbers_overridden} card numbers); {curated_conflicts} curated conflicts
```
`pct = 100 * implemented / card_numbers if card_numbers else 0.0`. Current output, e.g.
`gcg-sim 0.1.0 (results schema 1.1.0)`, `implementation coverage: 1152/1152 card numbers (100.0%)`,
`  EX BASE: 30/30`, `conflicts: 402 resolutions in overrides.json (166 card-data overrides on 164 card numbers); 333 curated conflicts`.

### 3.8 `replay` (cli.py:338-370)

1. `doc = json.loads(path.read_text(encoding="utf-8"))`:
   `OSError` → `cannot read replay file {path}: {exc.strerror or exc}`;
   `UnicodeDecodeError` → `{path}: not UTF-8 text ({reason} at byte {start})`;
   `json.JSONDecodeError` → `{path} is not valid JSON: {exc}` where `str(exc)` =
   `{msg}: line {lineno} column {colno} (char {pos})`, e.g.
   `Expecting property name enclosed in double quotes: line 2 column 1 (char 2)`.
2. Not a dict → `{path} is not a replay document`.
3. `check_replay(doc)`; `ReplayError`/`ValueError`/`LookupError` → `cannot replay {path}: {exc}`
   (e.g. `cannot replay r3.json: not a gcg-sim-replay v1 document`).
4. stdout: if `--log`: `"\n".join(check.log)`; then for seats sorted by `int(seat["seat"])`:
   `seat {seat}: {role} {deck} ({agent})` (values printed with Python `str()`); then
   `result: winner seat {winner_seat} ({end_reason}) after {turns} turns`.
5. If mismatches: `CliError("replay does NOT match the recorded outcome:\n  " + "\n  ".join(mismatches))` → exit 1
   (after the stdout lines above). Else stdout `replay matches the recorded outcome and final state`, exit 0.

Real example (stdout):
```
seat 0: bench red-green-zeon (test-random)
seat 1: dut blue-white-federation (test-random)
result: winner seat 1 (battle_damage) after 23 turns
replay matches the recorded outcome and final state
```

### 3.9 Message catalogue (stderr unless noted; every `CliError` gets the prefix `error: `)

| Situation | Text | Exit |
|---|---|---|
| `--help` | help on stdout | 0 |
| argparse usage error | `usage: …` + `{prog}: error: {msg}` | 2 |
| deck unreadable | `error: cannot read deck file {path}: {strerror}` | 1 |
| deck parse error | `error: {path}: {DeckError}` | 1 |
| deck not UTF-8 | `error: {path}: not UTF-8 text ({reason} at byte {start})` | 1 |
| deck illegal | `error: {path}: {n} violation(s)` + lines `  [{code}] {message}` | 1 |
| out not a directory | `error: cannot write reports to {out}: {existing} is not a directory` | 1 |
| out not writable | `error: cannot write reports to {out}: permission denied on {existing}` | 1 |
| AI missing | `error: the AI package 'gcg_sim.ai' is not available; the benchmark needs the search AI` | 1 |
| game failure | `error: simulation failed: {GameFailure}` (formats in §2.4) | 1 |
| report write error | `error: cannot write reports to {out}: {filename or out}: {strerror}` | 1 |
| Ctrl-C | `interrupted: workers stopped, no reports written` (no `error:` prefix) | 130 |
| replay unreadable | `error: cannot read replay file {path}: {strerror}` | 1 |
| replay not UTF-8 | `error: {path}: not UTF-8 text ({reason} at byte {start})` | 1 |
| replay bad JSON | `error: {path} is not valid JSON: {JSONDecodeError}` | 1 |
| replay not object | `error: {path} is not a replay document` | 1 |
| replay invalid | `error: cannot replay {path}: {ReplayError/ValueError/LookupError}` | 1 |
| replay mismatch | `error: replay does NOT match the recorded outcome:` + `  {k}: expected {e!r}, replay gave {v!r}` lines | 1 |
| anything else | Python traceback | 1 |

---

## 4. Hazards (Python-semantic traps, with locations)

RNG / seeds
1. rng.py:21 — `seed & MASK64`: negative or ≥2^64 Python ints wrap; Rust must mask (`i128 as u64`) wherever Python passes unmasked ints (`GameState.seed` keeps the raw value, engine/state.py:476).
2. rng.py:24-27 — arithmetic is mod 2^64 via masking; Rust must use `wrapping_add`/`wrapping_mul` (plain ops panic in debug).
3. rng.py:34 — `limit = 2^64 - 2^64 % n` overflows u64 when `n` is a power of two (limit = 2^64); use u128 or the `rem == 0` special case.
4. rng.py:35-38 — `randrange(1)` consumes a draw; never short-circuit `n == 1` or `len == 1` choices.
5. rng.py:44-46 — Fisher–Yates direction (high→low) and `j ∈ [0, i]`; `rand::seq::SliceRandom::shuffle` is not equivalent.
6. rng.py:41 — use `(x >> 11) as f64 * 2^-53`; never convert the full 64-bit value to f64.
7. rng.py:58 — `repr()` of the tuple: bool prints `True` (≠ `1`), strings use `'…'` unless they contain `'` without `"`; only `master` is masked, path ints are printed as given; `.encode()` is UTF-8.
8. rng.py:59 — first 8 bytes of the digest, **big-endian**.
9. ai/base.py:64-65 — per-decision counter increments before deriving (first decision k=1) and counts single-option decisions (ai/agents.py:86-88); agents are rebuilt every game (runner/game.py:126).
10. config.py:80 — agent seeds are keyed by **seat**, not role; the DUT's seat alternates with `m % 2` (config.py:66).
11. engine/game.py:153 — the die roll consumes RNG only when `chooser is None` (replays must pass the recorded `chooser`, not `chooser_seat`).

Runner
12. game.py:92 — `st.pending is not decision` is an identity check; Rust needs a version/identity token, not structural equality.
13. game.py:96-97 — decision-log slicing by per-seat counts; entries interleave in decision order.
14. game.py:130-136 — `except GameFailure, AgentUnavailableError:` (PEP 758 tuple); `except Exception` lets KeyboardInterrupt through; the message embeds `type(exc).__name__` and `str(exc)` (KeyError str is the quoted repr of the key).
15. game.py:49-50 — BOTH_DEFEATED scores for `1 - st.active` only in bo3; in bo1 → `None` → match `"draw"`.
16. match.py:36-38 — `break` happens **before** updating `chooser`; `max(points.values())` over a 2-entry dict.
17. instrument.py:42,44-45 — `dict` used as an insertion-ordered set and `set`s; every output is re-sorted (instrument.py:96, game.py:161), so order doesn't matter, but membership semantics do.
18. instrument.py:81 — history reset detection by list identity (`is not`); the engine replaces `st.history` at engine/game.py:190 and :336. Rust: track a history generation counter and rescan from index 0 when it changes.
19. instrument.py:96, game.py:161 — `sorted()` of card-number strings = code-point order (same as byte order for UTF-8/ASCII).
20. records.py:107-132 — `to_json` key order is insertion order but every writer uses `sort_keys=True`; seeds go up to 2^64-1 (serialize as u64, not i64/f64).
21. run.py:73-77 — `set` of futures plus `wait()`: completion order is non-deterministic; results are keyed by match index and re-ordered (run.py:106); progress reports counts. Which failure is reported in parallel runs is timing-dependent.
22. run.py:102 — `except BaseException` also catches KeyboardInterrupt and exceptions raised by the `progress` callback → terminate workers and re-raise.
23. run.py:61-62 — uses the private `_executor_manager_thread` and `terminate_workers()` (Python ≥3.14).
24. run.py:94 — `spawn` workers each rebuild the effect registry in first-use order (process-local indices); results must not depend on registry index order.
25. run.py:30, 36-38 — wall clock only for timing.json (non-reproducible by design).
26. agents.py:80 — `functools.cache` (hash of the ref string): caching only, no ordering effect. agents.py:67 — sha256 of the path for a module name (no game effect).
27. agents.py:108 — `describe_factory` output contains Python repr quotes (`gcg_sim.ai.make_agent('mcts')`) and goes into results.json.

Digest / replays
28. digest.py:27 — `content_id` hashes CPython `repr()` of DSL dataclasses: dataclass format, `<Enum.MEMBER: 'value'>`, `(x,)` one-element tuples, the `"…'…"` quote switch, and literal non-ASCII (`Ⅱ`) encoded as UTF-8. Every DSL type needs a byte-exact repr in Rust (or precomputed ids).
29. digest.py:48 — `once_used` sort key is `json.dumps` with default separators `", "`/`": "` and `ensure_ascii=True`, compared as strings (`"[-7, …"` < `"[12, …"` because `-` < `1`).
30. digest.py:55 — the digest JSON uses `sort_keys=True`, separators `(",", ":")`, `ensure_ascii=True` (non-ASCII → `\uXXXX` with surrogate pairs; DEL 0x7f → `\u007f`); serde_json does neither escape by default.
31. digest.py:42-43 — mixed int/str in the same position (`-1` stays int, valid indices become hex strings).
32. replayfile.py:39 — `'bottom' if a.a else 'top'`: Python truthiness (any non-zero, including −1, is "bottom").
33. replayfile.py:65 + engine/state.py:30-32 — `Action.__str__` drops only `-1`, prints `-2`.
34. replayfile.py:70 — `range(int(c))`: negative counts give zero copies; `int()` truncates floats and parses numeric strings (also at replayfile.py:169-177).
35. replayfile.py:90-93 — `Action.from_json` runs before the "more actions than decisions" check, so the error you get depends on that order.
36. replayfile.py:166, 195 — Python equality: `version` `1.0` or `true` pass (`True == 1 == 1.0`); expected `turns: 23.0` matches `23`. `!r` prints Python reprs (`'battle_damage'`, `None`).
37. replayfile.py:169 — `sorted(..., key=int(seat))` is stable; more than two seats → only the first two are used; fewer → IndexError → "malformed".
38. replayfile.py:171-178 — argument evaluation order decides which missing key is reported.
39. replayfile.py:182 vs cli.py:355 — exceptions not in the caught sets (e.g. AttributeError when `expected` is not an object, KeyError from `seat['role']` at cli.py:359-360 outside any try) end in a traceback with exit 1.

CLI
40. cli.py:63 — `int(text, 0)` grammar (prefixes, underscores, leading-zero rule, whitespace, Unicode digits).
41. cli.py:69-95 — argparse 3.15 defaults: prefix abbreviations, last-wins repeats, `suggest_on_error` (difflib cutoff 0.6), colour on TTYs, usage wrapping from `COLUMNS`/terminal width.
42. cli.py:106, 215, 343 — `exc.strerror` is the platform C `strerror` text (Rust `io::Error` Display adds ` (os error N)`).
43. cli.py:110, 348 — `UnicodeDecodeError.reason` strings (`invalid start byte`, `invalid continuation byte`, `unexpected end of data`, …) and `start` byte offset follow CPython's UTF-8 decoder.
44. cli.py:340 — `read_text` uses universal newlines; `json.loads` accepts `NaN`/`Infinity`/`-Infinity`, arbitrary-size ints, and duplicate keys (last wins); a leading BOM is a JSONDecodeError whose str is `Unexpected UTF-8 BOM (decode using utf-8-sig): line 1 column 1 (char 0)`; column/char positions count characters, not bytes.
45. cli.py:172, 313 — `:.1f` rounds the exact binary value half-to-even (`0.25` → `0.2`, `0.35` → `0.3`, `0.05` → `0.1`); check that Rust `{:.1}` agrees on exact ties; `100 * rate` is a float multiply done first.
46. cli.py:156 — `10 * done // total` integer floor division (non-negative).
47. cli.py:154 — TTY progress uses `\r` with no newline until the last match.
48. cli.py:180 — the parent walk stops at `.` for relative paths (`Path("a").parent == Path(".")`).
49. cli.py:123, 132, 217-228, 350-356 — paths print as `str(Path)`, which is normalised: `./gcg-sim-out/` → `gcg-sim-out`, `a//b` → `a/b`, `Path('.') / 'results.json'` → `results.json` (Rust `PathBuf` keeps `./` and trailing slashes).
50. cli.py:227 — `{label:8}` left-justifies to width 8.
51. cli.py:298 — `pct` falls back to `0.0` when there are no cards (no ZeroDivisionError).
52. cli.py:328 — `json.dumps(indent=2, sort_keys=True, ensure_ascii=False)`: with an indent, the item separator is `","` (no trailing space) and the key separator is `": "`.
53. cli.py:391 — `SystemExit.code` passthrough; a KeyboardInterrupt during `parse_args` is not caught.
54. cli.py:397-399 — the interrupt message is printed even if some report files were already written (results.json is written last, reports/writer.py:109).
55. No `sum()`/`math.fsum`/`round()`/`min()` on floats, no `hash()` ordering, no mutable default arguments in the scope files. The one `max` (match.py:36) is over ints, so first-vs-last tie order doesn't matter.

---

## 5. Open questions

1. Must the Rust CLI reproduce argparse output byte-for-byte (usage wrapping that depends on
   `COLUMNS`, TTY colours, difflib suggestions, abbreviation handling, `int(text, 0)` grammar), or
   only exit codes plus the `CliError` texts? The task says byte-identical results.json/games.ndjson/replays;
   CLI stdout/stderr are not covered.
2. `GCG_SIM_AGENT_FACTORY` names a Python `module:attr` or `file.py:attr`. Does the Rust port need an
   equivalent test hook (e.g. a built-in seeded random agent called `test-random`), and what should
   `describe_factory` print for it?
3. results.json `ai.factory` is `gcg_sim.ai.make_agent('mcts')` (agents.py:108). For byte identity the
   Rust port must emit this Python-flavoured string literally; please confirm.
4. `content_id` (digest.py:27) needs a byte-exact Python `repr` of every effect-DSL object reachable
   from `Registry.continuous`, `filter_sets` and `programs`, including programs registered lazily
   during play. Should the effects inventory own a repr serializer, or should Rust ship a generated
   table of content ids?
5. Python assigns registry indices in per-process first-use order (digest.py:3-8). I couldn't tell
   from the runner code whether any engine ordering depends on those indices (e.g. sorting lasting
   effects or `once_used` keys by index). That belongs to the engine/effects inventories, and it
   decides whether Rust may number registry entries differently.
6. Error texts that embed Python exception messages (`GameFailure` "… {TypeName}: {exc}", `KeyError`
   reprs, `JSONDecodeError`, `UnicodeDecodeError.reason`, `strerror`) can't be byte-identical in
   general. Which of them, if any, are in scope?
7. In parallel runs, which failure gets reported when several matches fail depends on timing
   (run.py:73-77). Should Rust fix this (e.g. always report the lowest failing match index, as the
   serial path does)?
8. timing.json and the `code_digest` in `versions()` (shown by `data status`) depend on the Python
   runtime and source code. Their Rust values are outside this scope (reports inventory).
