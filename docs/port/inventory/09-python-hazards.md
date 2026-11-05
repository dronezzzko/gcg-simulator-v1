# 09 — Python-semantic hazards: cross-cutting sweep of `src/gcg_sim`

Scope: every runtime module that can influence game logic, AI decisions or output bytes:
`engine/`, `effects/` (registry, program, builtins, bindings; the compiler only at load time),
`ai/`, `runner/`, `reports/`, `cards/`, `deck/`, `rng.py`, `cli.py`. The offline tools
`tools/`, `sources/`, `rules/` and the offline AI scripts `ai/tuning.py`, `ai/compare.py`,
`ai/throughput.py` and `ai/selfplay.py` are listed only in §13. Per-area detail lives in
01–08. This file is the checklist across all of them. The Rust equivalents for float sums,
`pow`, libm functions, `round`, repr and format specs, stable sorting and `max_by` ties were
**run** against CPython output on this machine. The `py_argmax` and `randrange` snippets are
direct translations of the Python code and were not run.

Line numbers are for branch `rust-port` at `d9d41af`. Reference: CPython 3.15.1 (uv), macOS
27.2, Apple M2 Pro (arm64). Rust: rustc 1.99.1, `-O`. `path:line` is relative to
`src/gcg_sim/` unless it starts with `docs/` or is `Cargo.toml`.

Method:
1. AST scan of all 101 files. It found every `set`/`frozenset` construction and every iteration
   of a set-typed name, within a file and across modules. Scripts are in
   `/private/tmp/claude-501/hazards/setscan.py` and `setscan2.py`.
2. Greps for each hazard class in the task list. Each hit was then read in context.
3. Experiments:
   - hash-seed invariance runs;
   - a walk of the objects behind every output file;
   - a Rust cross-check of 299,985 floats (`/private/tmp/claude-501/hazards/rs/check.rs`);
   - a zmij/serde_json check (`/private/tmp/claude-501/hazards/zm`);
   - a Neumaier `sum` check over 200,000 lists (`rs/sum.rs`).

---

## 0. Verdict and top hazards

**Set iteration order: no place in scope depends on it.** CPython set iteration order (of ints,
tuples, strings or enum members) never determines behaviour or output bytes. §1 lists every
iterated set and the reason each is order-independent. Byte-identical outputs across
`PYTHONHASHSEED` values confirm it (§1.4).

Ranked hazards for byte-equivalence (details in the sections named):

| # | Hazard | Where | Section |
| --- | --- | --- | --- |
| 1 | Float `sum()` is Neumaier-compensated (3.12+). Ints met after the first float are compensated too (3.15). | `ai/evaluation.py:314,317`, `ai/mcts.py:198,199,315`, `ai/setup.py:102,103,108,109` | §5.1 |
| 2 | `x ** 2` is libm `pow(x, 2.0)`, not `x*x`. Rust `powf(2.0)` and `powi(2)` fold to `x*x`. | `ai/mcts.py:199`, `ai/setup.py:109` | §5.2 |
| 3 | `max(..., key=)` returns the FIRST maximum. Rust `max_by*` returns the LAST. | `ai/mcts.py:187,288,301,338`, `ai/policy.py:93`, `ai/agents.py:65` | §4 |
| 4 | StrEnum members sort by string value, not declaration order (`Kw`). | `effects/bindings/wp_gd04_a.py:78` | §3.3 |
| 5 | Float text is CPython `repr`. Rust std breaks digit ties half-up, and serde_json uses different exponent thresholds. | every JSON float; `reports/summary.py:196` | §6 |
| 6 | `round(x)` / `round(x, n)` round half to even on the exact binary value. | `ai/mcts.py:178`, `reports/stats.py:29`, `ai/base.py:95` | §5.4, §6.3 |
| 7 | `math.erfc` is platform libm. `f64::erfc` is unstable in Rust 1.99, so FFI is needed, which conflicts with `unsafe_code = "deny"` (`Cargo.toml:18`). | `reports/stats.py:60` | §5.3 |
| 8 | `sorted(..., key=json.dumps)` sorts by JSON **text** with default separators. | `runner/digest.py:48` | §3.6 |
| 9 | The digest hashes CPython `repr()` of DSL objects. | `runner/digest.py:26-27` | §10 (see 02, 04) |
| 10 | `ensure_ascii` differs per file (`\u2161` in games.ndjson, raw `Ⅱ` in results/replays). | `reports/writer.py:34,56`, `runner/digest.py:55` | §7 |

---

## 1. Hashing, sets and frozensets

### 1.1 Facts about CPython hashing that matter

- `IntEnum` members hash like their int value: `hash(Zone.DECK) == 0`. That holds under every
  `PYTHONHASHSEED`, checked with seeds 0–3. Iteration order of a set of `Zone`, `Phase` or
  `Step` is therefore stable across processes. It is still the CPython hash-table order, which
  Rust `HashSet` does not reproduce.
- `StrEnum` members hash like their string value, and str hashes are salted per process. Seeds
  0–3 gave four different orders for `list(frozenset(ActionKind))` and `list(frozenset(d.Ev))`.
  Every enum except `Zone`, `Phase` and `Step` (`engine/types.py:8,48,58`) is a `StrEnum`:
  `effects/dsl.py:20,29,35,51,60,75,84,585,625,692,1250,1256,1265`, `engine/types.py:101,117,142,150`,
  `cards/model.py:10,40`. `engine/interp.py:28 Status` is a plain `Enum`, which hashes by name
  (salted).
- Benchmark workers are separate `spawn` processes (`runner/run.py:94`,
  `multiprocessing.get_context("spawn")`), each with its own hash salt. Iterating a set of
  strings would therefore make parallel and serial runs differ. The code avoids it everywhere
  (§1.2). The project test `test_results_do_not_depend_on_the_python_hash_seed`
  (`tests/cli/test_console_script.py:64-72`) runs the benchmark CLI with `--workers 2` under
  `PYTHONHASHSEED` 1 and 2 and asserts that the deterministic files are byte-identical.

### 1.2 Every iterated set in scope, and why its order is irrelevant

The scope has 117 set-construction sites: 38 `frozenset()`, 34 `set()`, 27 set literals and 18
set comprehensions. It has no `x in {literal}` tests. The sites below are every place where a
set is iterated: by `for`, a comprehension, `sorted`, `list`/`tuple`, unpacking, `pop` or
`next(iter())`. Every other site only uses `in`, `==`/`<=`, `len`, `add`, `update`, `|` or `&`.

| Site | Set (element type) | How it is iterated | Why order cannot matter |
| --- | --- | --- | --- |
| `engine/observe.py:29-35` | `HIDDEN_ZONES` (Zone) | comprehension over zones | `out.sort()` at :35 |
| `engine/core.py:302-309` | `set(dl)` (def_id ints) | `for`, then `evs.update(...)` | a set union; the result is only membership-tested (:334) |
| `engine/battle.py:109-110` | `set(victims.values())` (uids) | dict comprehensions `breach`, `paired` | the dicts are only read by key (:134 `paired[src]`, :144 `breach.get(src, 0)`) |
| `engine/battle.py:111`, `engine/core.py:888` | sets of uids | `sorted(...)` | sorted ints |
| `engine/game.py:74,89-90` | `colors` (Color StrEnum) | `len`, `sorted(c.value ...)` | sorted by value string |
| `engine/game.py:94` | `set(deck.resources)` (str) | `sorted` | sorted |
| `engine/interp.py:791-797`, `:815` | `owners` (int) | `sorted` | sorted |
| `engine/state.py:598` | `once_used` (int tuples) | `sorted(list(k) ...)` | sorted, and re-sorted in the digest (`runner/digest.py:48`) |
| `ai/actions.py:308-310` | `st.once_used` | `sorted(...)` | sorted, and only used for fingerprint equality |
| `effects/registry.py:190-200,361-397` | `set[str]`, `set[d.Ev]` | `add`/`update`, then frozen | used only for membership (`engine/view.py:762`, `engine/core.py:334`) |
| `runner/game.py:161` | `observer.bench_seen` (str) | `tuple(sorted(...))` | sorted |
| `runner/run.py:73-77` | `set(futures)` | `wait()` returns sets; `done` is iterated `sorted(..., key=futures.__getitem__)` | results are keyed by match index and re-ordered at :106 |
| `reports/aggregate.py:59`, `reports/results.py:60`, `cli.py:115` | `set[str]` | `sorted` | sorted |
| `reports/versions.py:122` | `set(numbers)` | `for n in set(numbers)` builds `in_decks[n]` label lists | lists grow in `decks.items()` order, not set order; `in_decks` is only looked up; output sorted at :132, :135 |
| `reports/versions.py:132` | `set & dict.keys()` | `sorted` | sorted |
| `deck/validate.py:40,141,156,214,251`, `cli.py:284` | `set[str]` / `frozenset[str]` | `sorted` | sorted |

Membership-only sites that a reader might mistake for iteration:
- `ai/noop.py:128-166`. `without_effect` returns a `set[Key]`, used only as
  `m[0] not in idle` at `ai/mcts.py:108`.
- `ai/actions.py:386-392`. `seen` keeps the **first** move of each group; the result list
  follows `moves` order.
- `engine/interp.py:686-687`. A set comprehension over `f.ints`, then `frozenset(...)`, used
  only for membership in `_rest_substitutes` (:639).
- `ai/noop.py:230-240`. `frozenset` equality and `<=`.
- `cards/db.py:215`. Set equality of traits.
- `deck/validate.py:189-199`. Subset test.
- `engine/view.py:89`. `dv.linked` is only membership-tested; nothing iterates it.

### 1.3 Rust rule

- `HashSet`/`HashMap`, with any hasher, are safe **only** for membership, lookup and equality.
- Every place above that sorts must sort in Rust with the same key (§3).
- `once_used` can be a `HashSet<Vec<i64>>` if `to_json` sorts it. A `BTreeSet<Vec<i64>>` gives
  the `state.py:598` order directly, because Vec ordering is lexicographic with shorter prefix
  first, the same as Python list ordering.

### 1.4 Evidence: outputs do not depend on the hash salt

`/private/tmp/claude-501/hazards/mini_run.py` plays 4 BO1 games with greedy agents.
`mcts_run.py` plays one BO1 match with `ai_preset="fast"` (MCTS) and a decision log. I ran each
under three `PYTHONHASHSEED` values (1/2/3 for greedy, 11/22/33 for MCTS) and compared
`shasum -a 256`. All three agreed for results.json, games.ndjson, summary.md and every replay
file:
- greedy results.json `a5f447c8…`, games.ndjson `9ea0af2f…`;
- MCTS results.json `6c91a042…`, games.ndjson `86fbfeb1…`, replay `loss-m0000-g1.json`
  `6e526438…`.

### 1.5 `hash()`, `id()`, `__hash__`, caches

- There is no explicit `hash()` call and no `__hash__`/`__eq__`/`__lt__` override in scope.
- `id()` appears only at `ai/mcts.py:133,145`: `slot = {id(child): i ...}` maps root children to
  their list index. It is a lookup only, and the root keys are unique (`ai/actions.py:72-85`
  dedups them), so no id collides. In Rust, use the child's index directly.
- `@functools.cache` sites: `effects/registry.py:400`, `effects/bindings/__init__.py:90`,
  `ai/policy.py:63` (`knobs_for(SearchConfig)`, a value-equality cache), `reports/results.py:41`,
  `runner/agents.py:80`, `cards/db.py:327`, `deck/official.py:74,112`,
  `reports/versions.py:43,55,82`. All are pure memoisation; none changes behaviour.
- `_LISTEN_CACHE` (`engine/core.py:288-313`) is cleared once it holds more than 256 entries.
  This is behaviour-neutral: the cache stores a deterministic function of the decklists.
- **Registry interning** (`effects/registry.py:115-138`). `program()`, `cont_key()` and
  `filters_key()` hand out list indices in **first-use order within the process**, through dicts
  keyed by DSL dataclasses (`__hash__` is generated by the frozen dataclass). The index
  therefore depends on process history. Logic only compares these indices for equality:
  - `engine/game.py:693`;
  - `ai/noop.py:49` (`le.effect_key`);
  - `ai/actions.py:315` (inside a sorted tuple used only for equality);
  - `engine/view.py:451` (part of a rule-source key).

  The digest replaces each index with `content_id` (`runner/digest.py:36-49`). No output exposes
  raw indices. A Rust port may number them any way it likes, as long as equal content gets an
  equal key.

---

## 2. Dict insertion order

Python dicts iterate in insertion order. Order-dependent iteration exists in the engine and is
owned by 05/06a/06b. The key cases:
- `engine/view.py:425`: constants are applied in `view.abilities` insertion order;
- `engine/core.py:338-342`: trigger candidates are built in index order, then the `lki` uids
  sorted by `(owner != active, uid)`;
- `engine/battle.py:102-106`: `victims` is iterated in insertion order;
- `ai/actions.py:75-85`: `canonical` keeps a dict plus an explicit `order` list.

Use `IndexMap` (or `Vec` plus index) wherever Python iterates a dict.

Rules that apply everywhere:
- **Dict equality ignores order.** See `engine/view.py:94-106` (`same_values`),
  `engine/view.py:456` (`new_granted == granted`) and `ai/noop.py:240`. `indexmap::IndexMap`'s
  `PartialEq` is order-insensitive too. List values inside are compared in order.
- **JSON output order never depends on dict order.** Every JSON file and both digests use
  `sort_keys=True` (§7). Exceptions are the offline tools (§13) and the CLI's
  `print(json.dumps(..., indent=2, sort_keys=True))` (`cli.py:328`, sorted).
- **`Counter` semantics.**
  - Reading a missing key returns 0 without inserting it (`engine/observe.py:48`).
  - `subtract` allows zero and negative counts. `elements()` yields keys in insertion order, each
    `count` times, and skips counts `<= 0` (`ai/setup.py:73-75`, `engine/observe.py:53`). Both
    call sites sort the result, so only the multiset matters. A Rust `BTreeMap<i64, i64>`
    expanded with `max(count, 0)` copies matches.
- **`dict.setdefault` keeps the existing value.** At `ai/mcts.py:316-317` an existing child keeps
  its old prior, even when the new move set has a different normaliser.
- **`dict.pop(k, default)`** is used at `engine/interp.py:702`.

---

## 3. Sorting

### 3.1 Semantics

`sorted()` and `list.sort()` are stable Timsort. They compare only with `<`, and call the key
once per element. `reverse=True` keeps equal elements in their original order: it is a stable
descending sort, not `sorted(...)[::-1]`. Verified: `sorted([(0,'a'),(2,'b'),(2,'c'),(1,'d')],
key=k0, reverse=True)` gives `[(2,'b'),(2,'c'),(1,'d'),(0,'a')]`.

Rust `slice::sort_by` / `sort_by_key` are stable. `v.sort_by(|a, b| b.0.cmp(&a.0))` printed the
same `[(2,'b'),(2,'c'),(1,'d'),(0,'a')]`. **Never use `sort_unstable*`** when elements with
equal keys can be told apart.

Python ordering of the types that appear in keys, and the matching Rust types:

| Python | Ordering | Rust equivalent |
| --- | --- | --- |
| `int` (unbounded) | numeric | `i64`/`u64`; all values in scope fit |
| `bool` | `False < True`, and `True == 1` | `bool` (`false < true`) |
| `str` | code-point lexicographic | `String`/`&str` `Ord`; UTF-8 byte order equals code-point order |
| `float` | IEEE `<`; `-0.0 == 0.0` | `partial_cmp`; no NaN occurs in any key in scope |
| `tuple`/`list` | lexicographic; a shorter prefix is smaller | Rust tuple / `Vec` `Ord` |
| `IntEnum` (Zone, Phase, Step) | by int value | `#[derive(Ord)]` with discriminants in ascending declaration order; the values ascend in `engine/types.py` |
| `StrEnum` | by **string value** | **not** `#[derive(Ord)]`; compare `as_str()` |

### 3.2 Every keyed or reversed sort in scope

| Site | Key / flags | Notes for Rust |
| --- | --- | --- |
| `engine/core.py:341` | `(owner != active, uid)` | bool first: same-as-active owners (`False`) first |
| `engine/interp.py:808` | `(owner, uid)` | ints |
| `engine/interp.py:1067,1088` | `card_type is EX_RESOURCE` (bool) | stable: non-EX first, area order kept within each group |
| `effects/bindings/__init__.py:92` | module `.name` | import order. Duplicates raise (`:31-38`), so order cannot change behaviour |
| `ai/evaluation.py:268` | ints, `reverse=True` | ints, so stability is invisible |
| `ai/mcts.py:322-325` | ints, `reverse=True` | value only |
| `ai/setup.py:37` | `(-c.cost, c.card_number)` | int, then str |
| `ai/base.py:84` | `(-visits, -value)`, then `[:LOG_ALTERNATIVES]` | `-0.0 == 0.0`; stable, so ties keep alternatives order |
| `runner/digest.py:48` | `key=json.dumps` | §3.6 |
| `runner/run.py:76` | match index | |
| `runner/replayfile.py:169`, `cli.py:359` | `int(s["seat"])` | |
| `reports/results.py:78-79` | `(abs(turns - median), match_index, game_index)`, then `(match_index, game_index)` | ints; `median = turns[(len-1)//2]` is an int (:77) |
| `reports/stats.py:66` | `(p_values[i], i)` | floats, then index |
| `reports/hypotheses.py:126-127,131-132` | `(-abs(delta), str(id))` | float, then str |
| `deck/validate.py:139` | `(-count, card_number)` | |
| `cards/db.py:109,129,218` | `product_id` (str) / `card_number` (str), then `[0]` | stable: ties keep JSON order (03 owns this) |

Unkeyed sorts (`sorted(x)`) are over ints, strs, `(str, int)` pairs, or tuples of int/str/bool.
The equality-only fingerprint tuples are:
- `ai/actions.py:303-330`: `_references`, whose tuples include a `Duration` StrEnum
  (`le.duration`, compared by value) and `le.aux`;
- `ai/actions.py:356-360`: `(k.value, n)`, traits, aids, `(repr(rule), controller, bool)`.

These are sorted only so that two fingerprints compare equal. **Any** consistent total order in
Rust gives the same equality result. Their exact order is irrelevant as long as the same
multiset sorts to the same sequence.

### 3.3 Hazard: `Kw` sorts by value — `effects/bindings/wp_gd04_a.py:78`

`for kw, amount in sorted(kws.items())` (GD04-067 copy keywords) orders `Kw` StrEnum members by
string value. The resulting order is `Blocker, Breach, First Strike, High-Maneuver, Repair,
Support, Suppression`. Declaration order is different: `Repair, Breach, Support, Blocker, First
Strike, High-Maneuver, Suppression` (`effects/dsl.py:63-69`). Each iteration calls `add_lasting`,
so the order fixes the serials and order of `st.lasting`, which is part of the state and the
digest. One card in the DB has two or more printed keywords (counted with
`printed_keywords(def_id)`). Rust: sort by `kw.as_str()`. `#[derive(Ord)]` on a `Kw` enum would
be wrong. The same applies to any future sort of `ActionKind`, `DecisionKind`, `Ev`, `CardType`,
`Color` or `Duration` members.

### 3.4 Strings

Card numbers and keys are ASCII. Names and traits can be non-ASCII: `Zaku Ⅱ` uses U+2161. They
are only sorted for equality (`ai/actions.py:357`). Python `str` order and Rust `String` order
agree for any valid Unicode.

### 3.5 Lists of lists — `engine/state.py:598`

`sorted(list(k) for k in self.once_used)` sorts lexicographically by number; a shorter prefix
sorts first. The keys contain negative tags: `REDUCE_ONCE_TAG = -7`, `PREVENT_ONCE_TAG = -8`
and `REST_SUB_TAG = -9` (`engine/core.py:23-25`), and `-1` as the lasting-source marker
(`runner/digest.py:22`). `BTreeSet<Vec<i64>>` gives the same order.

### 3.6 Sorting by JSON text — `runner/digest.py:48`

`sorted(canonical_once_keys, key=json.dumps)` compares the **text** of
`json.dumps(list)` with default separators `", "` (no `sort_keys`, `ensure_ascii=True`). Numeric
order is not used: `-7` sorts before `-9` because `'7' < '9'`, `10` sorts before `2`, and a
string element starts with `"`. Verified example:

```
input   [[1,2],[-9,1],[-7,-1,'0f3a',3],[10,0],[2,0],[-10,5]]
sorted  ['[-10, 5]', '[-7, -1, "0f3a", 3]', '[-9, 1]', '[1, 2]', '[10, 0]', '[2, 0]']
numeric [[-10, 5], [-9, 1], [1, 2], [2, 0], [10, 0]]   (int-only keys, for contrast)
```

Rust: build exactly `"[" + elems.join(", ") + "]"`, where an int element is its decimal form and
a string element (a 16-hex `content_id`) is `"\"" + s + "\""`. Then compare the strings
bytewise, with a stable sort. Lists that are equal after canonicalisation can come from
different raw keys. Their relative order (from the `state.py:598` pre-sort) is invisible,
because identical lists serialise identically.

---

## 4. `max()`/`min()` ties and two-argument max

CPython `max(iterable, key=f)` replaces the running best only when `f(item) > f(best)`, so the
**first** maximal element wins. `min` likewise keeps the first minimum. Rust:
`Iterator::max_by_key` / `max_by` return the **last** maximum (verified: `(2,'c')` against
Python's `(2,'b')`). `min_by_key` returns the first.

Sites where ties can occur and the first must win:

| Site | Expression |
| --- | --- |
| `ai/policy.py:93` | `max(range(len(options)), key=scores.__getitem__)`: greedy pick, first best index |
| `ai/policy.py:95-100` | explicit loop `if v > best_v` starting from `-math.inf`: first on ties |
| `ai/agents.py:65` | `max(range(len(scored)), key=lambda i: scored[i].value)` |
| `ai/mcts.py:187` | `max(range(n), key=lambda i: (children[i].visits, children[i].mean()))` |
| `ai/mcts.py:288` | `max(unvisited, key=lambda ca: ca[0].prior)` |
| `ai/mcts.py:301` | `max(avail, key=ucb)` |
| `ai/mcts.py:338` | `max(range(len(stats)), key=lambda i: (stats[i].visits, stats[i].value))` |
| `ai/setup.py:54` | `next((u for u in unpaired if link_satisfied(u, c)), unpaired[0])`: first match |
| `effects/compiler/grammar.py:1770` | `max(..., default=-1)` over indices: a value, at load time |

Value-only uses, where ties cannot be seen: `ai/mcts.py:196,313`, `ai/policy.py:146`, the
binding `min`/`max(..., default=)` calls (`wp_gd05_a.py:103`, `wp_st05_08.py:51`,
`wp_st12_14.py:102`, `wp_gd03_a.py:88`), and all int clamps `max(0, x)` / `min(a, b)`
(61 plain calls).

Rust helper, matching CPython for tuples of `(int, f64)` with no NaN:

```rust
fn py_argmax<T, K: PartialOrd>(items: &[T], key: impl Fn(&T) -> K) -> usize {
    let mut best = 0;
    let mut best_k = key(&items[0]);           // Python raises on empty; never empty here
    for (i, it) in items.iter().enumerate().skip(1) {
        let k = key(it);
        if k > best_k { best = i; best_k = k; } // strict >, so the first maximum wins
    }
    best
}
```

Rust tuple `PartialOrd` compares lexicographically, like a Python tuple. With no NaN present,
`(visits, mean) > (bv, bm)` behaves exactly like Python.

Two-argument float max: `max(0.0, DANGER_LIFE - s.life)` (`ai/evaluation.py:219`) and
`max(worst, 1.0 + 0.5 * a_val - gain)` (`ai/policy.py:226`). Python returns the **first**
argument unless the second is strictly greater: `max(-0.0, 0.0)` is `-0.0`, `max(nan, 1.0)` is
`nan` and `max(1.0, nan)` is `1.0`. Rust `f64::max` differs: `(-0.0).max(0.0)` gave `0.0`, and
NaN is ignored. Use `if b > a { b } else { a }`. A signed zero here cannot change any later
result (the value only feeds subtractions and weights before `exp`), but the helper makes the
question moot.

---

## 5. Floating point arithmetic

Only `ai/` and `reports/` (and the runner's timing) use floats. The engine and effects are
integer-only: a grep for float literals, `float` and `/` in `engine/`, `effects/` (runtime
parts), `cards/model.py`, `deck/model.py` and `runner/{game,match,records,instrument}.py` found
none, apart from `records.py:193 wall_seconds`.

### 5.1 Builtin `sum()` of floats — Neumaier compensation

The CPython 3.15.1 algorithm (start value is int `0`), validated by emulation against the
builtin on 200,000 random mixed lists (`/private/tmp/claude-501/hazards/sumcheck.py`):
1. While items are exact `int`/`bool`, accumulate them as an integer.
2. At the first float `x`, set `f = int_acc + x` (int converted to float). Now `-0.0` becomes
   `+0.0`. Set `c = 0.0`.
3. For each later item `y`, converting ints with `float(y)`:
   - `t = f + y`;
   - if `|f| >= |y|`, then `c += (f - t) + y`, else `c += (y - t) + f`;
   - `f = t`.
4. At the end, if `c != 0` and `c` is finite, `f += c`.
5. An all-int sequence returns an exact int.

**3.15 compensates ints that appear after the first float.** The model that adds those ints
uncompensated (`f += float(y)`) mismatched in 8,221 of 200,000 trials. The compensated model
mismatched in 0.

Rust implementation, verified bit-identical on 200,000 float lists (`rs/sum.rs`):

```rust
/// CPython >= 3.12 builtin sum() over f64 items (start = int 0); empty input -> 0.0 here.
fn py_sum(xs: impl IntoIterator<Item = f64>) -> f64 {
    let (mut f, mut c) = (0.0f64, 0.0f64);
    for x in xs {
        let t = f + x;
        if f.abs() >= x.abs() { c += (f - t) + x; } else { c += (x - t) + f; }
        f = t;
    }
    if c != 0.0 && c.is_finite() { f += c; }
    f
}
```

Starting from `f = 0.0` reproduces step 2: `0.0 + x0` is `x0`, or `+0.0` for `-0.0`, and adds
zero to `c`. In the same run, naive `fold(0.0, +)` mismatched in 72,434 cases and
`Iterator::sum::<f64>()` in 72,885. `Iterator::sum` also starts from `-0.0`: an empty sum and
`[-0.0].sum()` both print `-0.0`, where Python gives `0` and `0.0`.

Float `sum()` sites that reach decisions or outputs:

| Site | Items |
| --- | --- |
| `ai/evaluation.py:314` | `w * x` over the 17 `ROLE_WEIGHTS` × role features (`DEFAULT_WEIGHTS`) |
| `ai/evaluation.py:317` | `w * x` over 14 base weights × features |
| `ai/mcts.py:198` | `diffs` (per-visit value differences) |
| `ai/mcts.py:199` | `(x - mean) ** 2` (also §5.2) |
| `ai/mcts.py:315` | prior weights `exp((x - top) / T)` |
| `ai/setup.py:102,103` | `as_first`/`as_second` ∈ {0.0, 0.5, 1.0} (exact for any realistic count, but use `py_sum`) |
| `ai/setup.py:108,109` | `diffs`, `(x - mean) ** 2` |

`timing.json` uses one too (`reports/writer.py:72`), but that file is not reproducible.

Int-only `sum()` sites, which are exact: `engine/game.py:888`, `engine/view.py:874`,
`engine/interp.py:992`, `engine/core.py:678`, `engine/invariants.py:81`, `ai/values.py:86`,
`ai/evaluation.py:165,249,273,274` (`through` is `list[int]`), `ai/targeting.py:150`,
`reports/aggregate.py:69-71,89,93,94,187,201,202`, `reports/hypotheses.py:69`,
`reports/stats.py:95` (`sum(s)` of ints, then int/int true division),
`effects/compiler/grammar.py:1374,1868`.

**Sequential `+=` accumulations are NOT compensated.** Rust must use plain `+=` for:
- `ai/mcts.py:217` `child.total += ...`;
- `ai/setup.py:50,57` `score += ...` and `:79` `total += ...`;
- `ai/policy.py:191-205,239-249,281-283` `s += / -=`;
- `ai/values.py:27-32`.

There is no `math.fsum`, `statistics`, `fractions` or `decimal` anywhere in scope.

### 5.2 `**` is libm `pow`

CPython `float ** int` converts the exponent to `2.0`. For a negative base with an integral
exponent it takes `|x|` and fixes the sign itself. It then calls C `pow`. On this machine:
- `x ** 2 != x * x` for 316 of 284,674 inputs (`-0.6498612898741909 ** 2` gives
  `0.42231969607694714`, while `x*x` gives `0.4223196960769472`);
- `ctypes` `libm.pow(|x|, 2.0)` equalled CPython in all 300,000 cases.

Rust results, compiled with `-O`, on 284,674 inputs:

| Rust expression | Mismatches |
| --- | --- |
| `x * x` | 316 |
| `x.powf(2.0)` | 316 (LLVM rewrites `pow(x, 2.0)` into `x*x`) |
| `x.powi(2)` | 316 |
| `x.powf(std::hint::black_box(2.0))` | **0** |
| `extern "C" pow(x, black_box(2.0))` | **0** |

Sites: `ai/mcts.py:199` and `ai/setup.py:109`. Both are `(x - mean) ** 2`. Use
`(x - mean).powf(std::hint::black_box(2.0))`. It is safe code, and the opaque exponent stops the
fold, including under `lto = "fat"` (`Cargo.toml:25`). There is no other `**` on floats in scope.
`2**64` (`runner/config.py:59`, `cli.py:56`) is an int.

### 5.3 libm transcendental functions

| Python | Sites | Rust (verified, 0 mismatches) |
| --- | --- | --- |
| `math.exp` | `ai/evaluation.py:305,306`, `ai/mcts.py:314` | `f64::exp` |
| `math.log` | `ai/policy.py:98` (twice), `ai/mcts.py:297` (`log(ch.avail)`, an int argument, converted exactly) | `f64::ln` |
| `math.sqrt` | `ai/mcts.py:200,297`, `ai/setup.py:110`, `reports/stats.py:22,56,60` | `f64::sqrt` (IEEE correctly rounded on every platform) |
| `math.erfc` | `reports/stats.py:60` | C `erfc` via FFI |
| `math.ceil` | `ai/evaluation.py:275,277`, `reports/stats.py:77` | `x.ceil() as i64` |

- On macOS, CPython calls libSystem libm. `ctypes` `libm.exp/log/erfc` equalled `math.*` on
  300,000 inputs. Rust std `exp`/`ln` lower to the same libm calls.
- `f64::erfc` is **unstable** in rustc 1.99.1: error E0658, feature `float_erf`, issue #136321.
  An `extern "C" { fn erfc(x: f64) -> f64; }` call matched on all inputs. It needs `unsafe`,
  which the workspace denies (`Cargo.toml:18`), so a narrowly scoped `#[allow(unsafe_code)]`
  module would be required.
- Do not let these be computed at compile time from constants. LLVM's constant folder is not
  libm. The only constant argument in scope is `math.sqrt(2)` (`reports/stats.py:60`): sqrt is
  correctly rounded, so `std::f64::consts::SQRT_2` (1.4142135623730951) is identical.
- `math.exp` raises `OverflowError` above about 709.78. `logistic` (`ai/evaluation.py:303-307`)
  only calls `exp` with non-positive arguments, so it cannot overflow. The `exp` arguments at
  `ai/mcts.py:314` are `<= 0`, because `top` is the max.
- Results depend on the C library: Linux glibc can differ from macOS. This matters for the
  reference platform (Open questions).

### 5.4 Conversions, division, rounding to int

- `int / int` is true division. For operands below 2^53 CPython computes
  `(double)a / (double)b`, so Rust `a as f64 / b as f64` matches. All operands in scope are
  counts.
- In mixed `int * float` and `int + float`, the int is converted first:
  `0.5 * cd.cost` becomes `0.5 * (cost as f64)`.
- `round(x)` with one argument returns an int, rounded half to even. Rust `x.round()` rounds half
  away from zero: `2.5.round()` gave 3, against Python's 2. Use `x.round_ties_even() as i64`,
  which had 0 mismatches. Site: `ai/mcts.py:178`, `round(budget * config.target_extension)`. The
  shipped presets have `target_extension` 3.0 or 1.0, so no `.5` product arises today (checked
  for every budget in every preset). A custom config could produce one.
- `int(float)` does not occur: every `int(` in scope converts a bool or a str. `x as i64` would
  match it anyway.
- `math.ceil(hits / len(through))` (`ai/evaluation.py:277`) and `math.ceil(base_hp / mean_ap)`
  (:275) are float division followed by ceil. Use `(a as f64 / b as f64).ceil() as i64`, not
  integer ceil-division.

### 5.5 Evaluation order and FMA

Python evaluates left to right with no fusion. Keep the association exactly. For example:
- `weight * 0.5 * (c.ap + c.hp)` (`ai/setup.py:50`) is `(weight * 0.5) * (ap + hp)`;
- `4.0 + 0.5 * cd.cost + 0.2 * (cd.ap + cd.hp)` (`ai/policy.py:189`) is
  `(4.0 + 0.5*cost) + 0.2*(ap+hp)`;
- `LEAF_FLOOR + (1.0 - 2.0 * LEAF_FLOOR) * logistic(z)` (`ai/mcts.py:264`).

Rust never contracts `a*b + c` into FMA unless `mul_add` is called, so keep that true and don't
call it. A conditional expression has the lowest precedence:
- `s = 3.0 + 0.5 * uv if f.kills else 0.3` (`ai/policy.py:244`) is
  `(3.0 + 0.5*uv) if kills else 0.3`;
- the same holds at `ai/policy.py:272`;
- `child.total += value if mover == player else 1.0 - value` (`ai/mcts.py:217`).

### 5.6 Special values

- `u = rng.random() or 1e-300` (`ai/policy.py:97`) tests the float for truth, so it replaces
  only `0.0`. Rust: `if u == 0.0 { 1e-300 } else { u }`.
- `best_v = -math.inf` (`ai/policy.py:95`): `f64::NEG_INFINITY`.
- `random()` is `(next_u64() >> 11) * (1.0 / (1 << 53))` (`rng.py:41`), which is exact. Rust:
  `((x >> 11) as f64) * (1.0 / 9007199254740992.0)`.
- No NaN or Inf can arise in scope. Every division is guarded:
  - `ai/mcts.py:57` (visits);
  - `ai/setup.py:80,102,103` (`max(1, games)`);
  - `ai/evaluation.py:139` (`if pool`);
  - `reports/stats.py:33,37-39,53,57`.

---

## 6. Float to text

### 6.1 `repr(float)` — used by `json.dumps` and `str()`/f-strings

CPython repr takes the shortest round-trip digits; on an exact tie between two shortest
candidates it picks the even digit. Let `decpt` be the position of the decimal point relative to
the digit string (`d1d2… × 10^(decpt-len)`).
- If `decpt <= -4` or `decpt > 16`, use exponent form:
  - `d1`, then `.` and the rest only if there is more than one digit;
  - `e`, a sign, and at least 2 exponent digits.
  - Examples: `1e-05`, `2.5e-05`, `1e+16`.
- If `decpt <= 0`, use `0.` followed by zeros and the digits.
- If `decpt >= ndigits`, use the digits, padding zeros, then `.0`.
- Otherwise insert the point.
- `-0.0` prints as `-0.0`.

Results on 299,973 non-zero inputs:

| Digit source + Python layout | Mismatches |
| --- | --- |
| `zmij::Buffer::format_finite` (zmij 1.0.23, in the workspace lock via serde_json 1.0.154) | **0** |
| Rust std `format!("{:e}", x)` / `{:?}` / `{}` | **123** |
| raw `serde_json::to_string(&x)` | 4,998 |

- The std mismatches are all exact ties at 16–17 significant digits. For example,
  `785081882490962.25` prints as `785081882490962.2` in Python and `785081882490962.3` in Rust:
  Python rounds the tie to even, Rust rounds it up. This **corrects**
  `docs/port/inventory/02-reports.md` §2.2, which states that Rust `{:?}` has the same digits.
  It does, except on these ties.
- Ties cannot occur for values with at most 15 significant digits. Every float the reproducible
  outputs write has been through `round(x, 6)` or `round(x, 4)`, or is a constant (`alpha 0.05`).
  So std digits would work today, but use zmij.
- serde_json's own layout differs: `1e-6` for `1e-06`, `0.00001` for `1e-05`.

Tested reference implementation: `py_from_digits` and `split` in
`/private/tmp/claude-501/hazards/zm/src/main.rs`. Normalise zmij's output to `(neg, digits,
decpt)`, then apply the layout above.

### 6.2 Fixed-format specs

Verified with 0 mismatches on 299,985 inputs (|x| < 1e30), including `±0.0`, `0.5`, `1.5`,
`2.5` and `0.125`:

| Python | Rust | Sites |
| --- | --- | --- |
| `f"{x:.1f}"` | `format!("{:.1}", x)` | `reports/summary.py:12,16`, `cli.py:172,313` |
| `f"{x:+.1f}"` | `format!("{:+.1}", x)` (`-0.0` gives `-0.0`, `0.0` gives `+0.0`) | `reports/summary.py:20` |
| `f"{x:.0f}"` | `format!("{:.0}", x)` | `reports/hypotheses.py:28` |
| `f"{x:04d}"` (an int) | `format!("{:04}", n)` | `reports/results.py:83` |

Always compute `100 * x` as an `f64` multiply before formatting (02 §2.5 shows why).

### 6.3 `round(x, n)`

CPython rounds the exact binary value correctly to `n` decimals, with ties to even, and returns
the nearest double. `format!("{:.n$}", x).parse::<f64>()` matched for n = 1, 2, 3, 4, 6 on
299,985 inputs. Examples: `round(0.125, 2) == 0.12`, `round(0.375, 2) == 0.38`,
`round(2.675, 2) == 2.67`. Sites:
- `reports/stats.py:29` (`rnd`, 6 digits), reached through `:33,41,95` and
  `reports/aggregate.py:205`, `reports/hypotheses.py:48,50`;
- `ai/base.py:95` (decision-log `value`, 4 digits);
- `reports/writer.py:67,71-73` (timing only).

`round` can return `-0.0`, which serialises as `-0.0`. 02 §2.4 explains why no in-scope value
reaches it.

### 6.4 Where floats appear in outputs

Found by walking the objects before serialisation (`/private/tmp/claude-501/hazards/walk.py`,
greedy BO3 run):
- `games.ndjson` and replays: only `decision_log[].alternatives[].value`.
- `results.json`: every `rate`, `ci95[]` and `mean`, plus `seen_rate`, `loss_rate_lift`,
  `copies_per_game`, `game_rate`, `rate_when_drawn`, `rate_of_drawn_copies`, and
  `hypothesis_thresholds.alpha`.
- The final canonical state, the digest input (`/private/tmp/claude-501/hazards/digest_walk.py`):
  **no** floats, no non-ASCII strings, no tuples, no non-str keys.

---

## 7. JSON serialisation

Every `json.dumps` call in scope (`json.dump` is never used):

| Site | Arguments | Output |
| --- | --- | --- |
| `reports/writer.py:34` (`to_json_text`) | `indent=2, sort_keys=True, ensure_ascii=False`, + `"\n"` | results.json, replays, timing.json |
| `reports/writer.py:56` | `sort_keys=True, separators=(",", ":")`, + `"\n"` per line | games.ndjson (`ensure_ascii=True`) |
| `runner/digest.py:55` | `sort_keys=True, separators=(",", ":")` | sha256 digest blob (`ensure_ascii=True`) |
| `runner/digest.py:48` | `json.dumps` as a sort key: defaults `", "`/`": "`, no sort_keys | §3.6 |
| `deck/parse.py:128` | `sort_keys=True, separators=(",", ":")` | deck digest |
| `cli.py:328` | `indent=2, sort_keys=True, ensure_ascii=False` | `data status` stdout |
| `ai/tuning.py:402`, `ai/throughput.py:58`, `ai/compare.py:213-214` | `indent=2` / `indent=1`, **no** sort_keys | offline tools (§13) |

- **`sort_keys=True` sorts the original key objects before turning them into strings.**
  `json.dumps({2:'x', 10:'y'}, sort_keys=True)` gives `{"2": "x", "10": "y"}`. A Rust
  `BTreeMap<String, _>` would order `"10"` before `"2"`. The output walk found no non-str key in
  any output (`by_game_number` keys are `str(i + 1)`, `reports/aggregate.py:51-54`). Keep it that
  way, or sort int keys numerically.
- `ensure_ascii` escaping was verified: games.ndjson contains `\u2161` (85 occurrences in the
  greedy run, lowercase hex), and results/replays contain raw UTF-8. 02 §2.3 has the full rules,
  including surrogate pairs and DEL.
- `IntEnum` and `bool` inside `to_json` structures serialise as ints and `true`/`false`. A
  `StrEnum` serialises as its value. The walk found no int or str subclasses in any output,
  because every `to_json` converts with `.value`/`int(...)`.
- serde_json `preserve_order` is enabled (`Cargo.toml:15`), so the port must sort keys itself
  (02 §2.1).

---

## 8. Integers

- Python ints are unbounded; seeds and RNG state live in [0, 2^64). `SplitMix64`
  (`rng.py:20-28`) masks after each add and multiply, so Rust needs `wrapping_add` and
  `wrapping_mul` on `u64`. `seed & _MASK64` (`rng.py:21,58`) of a negative Python int gives its
  two's-complement low 64 bits, which Rust gets from `i64 as u64`. Configs reject seeds outside
  [0, 2^64) (`runner/config.py:59`, `cli.py:56`).
- **`randrange` limit** (`rng.py:34`): `limit = 2^64 - (2^64 % n)`. When `n` is a power of two,
  `limit == 2^64`, which does not fit in a `u64`, and every draw is accepted. Rust:

  ```rust
  let rem = ((u64::MAX % n) + 1) % n;          // == 2^64 mod n
  loop { let x = next_u64(); if rem == 0 || x < 0u64.wrapping_sub(rem) { return x % n; } }
  ```
- `//` and `%` sites in scope:
  - `engine/game.py:910-911` and `ai/actions.py:45,64` (`ALT_PLAY_BASE = 100`, guarded by
    `c >= 100` / `c >= 0`);
  - `effects/bindings/wp_gd03_a.py:59` (`ap_of`, clamped `>= 0` at `engine/view.py:477-483,494`,
    divided by a positive `per`);
  - `engine/observe.py:108`;
  - `runner/config.py:66,85`;
  - `reports/results.py:77`;
  - `rng.py:34,38`;
  - `cli.py:156`.

  None has a negative operand, so Rust `/` and `%` match. Python floors toward negative infinity
  (`-7 // 2 == -4`, `-7 % 2 == 1`, `7 % -2 == -1`). If an operand could ever be negative, use
  `div_euclid`/`rem_euclid` for a positive divisor.
- Bit operations: `known` is a 2-bit mask (`1 << player`, `engine/core.py:51`,
  `engine/observe.py:24,33,44`). The other constants are `_DEF_OFFSET = 1 << 24`
  (`ai/actions.py:37`) and `_SELF = -(1 << 40)` (`ai/actions.py:281`); `i64` holds them. Nothing
  else exceeds 64 bits.
- `int.from_bytes(sha256(...)[:8], "big")` (`rng.py:59`) is `u64::from_be_bytes`.
- `bool` is an int: `True == 1` and they hash alike. Positions mixing bool and int in compared
  tuples (fingerprints, `ai/noop.py:42-52`) always hold the same type, so separate Rust types keep
  equality.

---

## 9. Strings, Unicode, regex

- **NFKC plus `casefold`** (`deck/names.py:32-33`) decides whether a deck line's name matches,
  which can turn a valid deck into a parse error. 03 (H5) owns this. Use full case folding
  (`ß` becomes `ss`), not `to_lowercase`, and Unicode 17.0.0 tables (`unicodedata.unidata_version`
  under 3.15.1).
- **`str.split()`/`strip()` with no argument** use `str.isspace`. That includes U+001C–U+001F,
  which Rust `char::is_whitespace` excludes (the full set difference was computed and is exactly
  `0x1c-0x1f`). Sites: `effects/text.py:18,27,88,112,130,134`, `cards/db.py:59,175,293`,
  `cards/model.py:67,69,71,79,81,89,141`, `cards/tokens.py:33,41,42,45`, `deck/parse.py:79`,
  `deck/names.py:33`. No decoded string in the 16 packaged data JSON/JSONL files contains these
  characters or any whitespace beyond space/`\n`/`\t`/`\r`, so this is latent for card data. It
  is live for user deck files (03).
- `str.upper()` (`reports/summary.py:49`, `deck/parse.py:25,28,31`), `str.lower()`
  (`deck/parse.py:28`) and `str.capitalize()` (`reports/summary.py:177`) are used.
  `capitalize` upper-cases the first character and **lower-cases the rest**: `"hOLM"` becomes
  `"Holm"`. The input today is the constant `"holm"` (`reports/results.py:179`).
- `repr(str)` (in `derive_seed` and error messages) uses single quotes unless the string
  contains `'` and no `"`: `repr("it's") == "\"it's\""`. 01 owns the full rule.
- Regex is used only at load time or for deck parsing, never per decision:
  - card loading: `cards/{db,model,tokens}.py`;
  - registry build: `effects/text.py` and `effects/compiler/*`;
  - decks: `deck/parse.py`, `deck/official.py`;
  - stale-replay deletion: `reports/writer.py:20,89`.

  Python `\d`, `\w` and `\s` are Unicode-aware (`str` patterns); the Rust `regex` crate's
  defaults are Unicode-aware too. Note that `regex` is not in `Cargo.lock`.
- Python str order equals Rust str order (§3.4).

---

## 10. Identity, aliasing, mutation

- **Mutable default arguments:** none. An AST scan of all function and lambda defaults found 0
  list/dict/set defaults.
- **Shared objects after `clone()`.** `GameState.clone` (`engine/state.py:534-572`) copies cards,
  battles, lasting effects, triggers and frames. It shares `history` entries
  (`list(self.history)`; `HistoryEvent` is a mutable `@dataclass(slots=True)`, `:414`),
  `pending`, `decklists` and `_derived`. No code mutates a `HistoryEvent` in place: a grep for
  attribute assignment found none, and `engine/observe.py:86-94` replaces entries with
  `dataclasses.replace`. Rust deep copies are therefore equivalent.
- **`list.remove` / `list.index`** use the first equal element:
  - `engine/core.py:44`, `engine/interp.py:347,457,1197` (uids, unique);
  - `ai/setup.py:48,54,56` (`CardDef` equality; equal elements are the same object, so the
    resulting sequence is the same whichever copy goes).
- **`pool.pop(st.rng.randrange(len(pool)))`** (`engine/interp.py:426,534`): `range(min(n,
  len(pool)))` is evaluated **once** before the loop, and `len(pool)` is re-read on every draw.
  Each draw consumes one or more `next_u64` (rejection sampling).
- **Content ids** (`runner/digest.py:26-27`) hash CPython `repr()` of DSL dataclasses and enums.
  `repr(r.rule)` is also compared for equality in `ai/actions.py:360`. 02 §11 and 04 own the
  repr grammar. This sweep checked that no DSL or program dataclass has a set, dict or float
  field (introspection of `effects/dsl.py` and `effects/program.py`), so `repr` never contains
  hash-ordered or float text.
- `information_set_key` (`engine/observe.py:134-150`, `repr(pending)`) is unused in `src/`.

---

## 11. Exceptions as control flow, assertions

- `effects/registry.py:330-337`: a binding or compile failure (`UnimplementedCardError`,
  `ValueError`, `AssertionError`, `TypeError`, `KeyError`, `IndexError`, `AttributeError`) marks
  the card unimplemented with message `str(exc)` or `f"binding error: {type(exc).__name__}:
  {exc}"`. **Zero** of the 1,153 registry cards carry an error today.
- `runner/game.py:118-136`: any `Exception` during a game becomes
  `GameFailure(f"match {m} game {g+1} (seed {seed}) failed: {type(exc).__name__}: {exc}")`.
  `GameFailure` and `AgentUnavailableError` propagate unchanged (PEP 758 unparenthesised
  `except`). The message text includes CPython exception class names.
- `runner/replayfile.py:86,97,180-183`: replay errors become `ReplayError` text with
  `type(exc).__name__`.
- Python runs `assert` statements unless started with `-O`. Asserts in the engine and AI
  (`engine/core.py:345`, `ai/mcts.py:87-88`, …) raise `AssertionError`, which becomes a
  `GameFailure`. Because the registry catches `AssertionError` (:336), running under `-O` would
  change which cards load. Rust should treat them as hard runtime checks (`assert!`, not
  `debug_assert!`) wherever Python can reach them.
- `zip(..., strict=True)` raises `ValueError` on a length mismatch: `ai/evaluation.py:239,291,314,317`,
  `ai/mcts.py:283,316,335`, `engine/observe.py:76,78`, and others. `zip(..., strict=False)` at
  `reports/hypotheses.py:61` truncates.
- `_fit` raises `ValueError("cannot determinize: empty pool for hidden cards")`
  (`engine/observe.py:103-104`).

---

## 12. Process and environment dependence

- Wall clock: `runner/run.py:30,36-38,123,131` and `ai/throughput.py:35,37` reach only
  `timing.json` and RunTiming. `os.getpid()` is used only for temp file names
  (`reports/writer.py:39`). `platform` appears only in timing.json (`reports/writer.py:74-75`).
- `results.json.versions.code_digest` (`reports/versions.py:43-52`) is a sha256 over the
  installed Python package files, in sorted path order. `package_version()` reads installed
  metadata (:25-29). Neither can come from Rust code; 02 §6 marks it as always differing.
- Per-process hash salt and per-process registry indices are covered in §1.1 and §1.5. Neither
  reaches outputs.
- `os.environ.get(AGENT_FACTORY_ENV)` (`cli.py:163`) selects the agent factory.

---

## 13. Offline modules (not part of benchmark outputs)

`ai/tuning.py` (fits weights):
- float `sum` at :179, :197, :230;
- `math.log` at :180;
- `max(..., key=abs)` pivot (first maximum) at :221;
- `round(wi, 4)` at :213;
- `round` at :335, :336, :363;
- `json.dumps(indent=2)` without sort_keys at :402.

`ai/compare.py`:
- `itertools.combinations` (lexicographic by index) at :131;
- `round` at :151, :152, :175;
- `json.dumps` with `indent=1`/`indent=2` at :213-214;
- `text.lower()` at :63, :65.

`ai/throughput.py:43-47` rounds timing values.

`tools/`, `sources/` and `rules/` sort sets before output (for example `tools/refresh.py:248-322`,
`rules/index.py:202`). They are not ported.

---

## 14. Hazards (consolidated checklist)

1. Float `sum()` is Neumaier-compensated, and ints after the first float are compensated as
   floats (CPython 3.15). Use `py_sum`, never `Iterator::sum`. Sites: `ai/evaluation.py:314`,
   `ai/evaluation.py:317`, `ai/mcts.py:198`, `ai/mcts.py:199`, `ai/mcts.py:315`,
   `ai/setup.py:102`, `ai/setup.py:103`, `ai/setup.py:108`, `ai/setup.py:109`
   (`reports/writer.py:72` is timing only).
2. `x ** 2` is libm `pow`. Use `powf(black_box(2.0))`: `ai/mcts.py:199`, `ai/setup.py:109`.
3. First-wins `max` with a key: `ai/policy.py:93`, `ai/agents.py:65`, `ai/mcts.py:187`,
   `ai/mcts.py:288`, `ai/mcts.py:301`, `ai/mcts.py:338`. The explicit strict-`>` loop at
   `ai/policy.py:99` and the first-match `next()` at `ai/setup.py:54` also keep the first.
4. Two-argument `max` returns the first argument unless the second is strictly greater:
   `ai/evaluation.py:219`, `ai/policy.py:226`. Do not use `f64::max`.
5. `StrEnum` sort order is by value: `effects/bindings/wp_gd04_a.py:78` (`Kw`). Never
   `#[derive(Ord)]` on a `StrEnum` port.
6. Stable sorts with these exact keys: `engine/core.py:341`, `engine/interp.py:808`,
   `engine/interp.py:1067`, `engine/interp.py:1088`, `ai/setup.py:37`, `ai/base.py:84`,
   `reports/results.py:78`, `reports/results.py:79`, `reports/stats.py:66`,
   `reports/hypotheses.py:126`, `reports/hypotheses.py:127`, `deck/validate.py:139`,
   `cards/db.py:109`, `cards/db.py:129`, `cards/db.py:218`. Never `sort_unstable`.
7. Sorting by JSON text: `runner/digest.py:48`.
8. Lexicographic list sort with negative tags: `engine/state.py:598`.
9. `round()` rounds half to even (int result): `ai/mcts.py:178`. Use `round_ties_even`.
10. `round(x, n)`: `reports/stats.py:29`, `ai/base.py:95`. Use format with `{:.n$}`, then parse.
11. Float repr in JSON and f-strings (zmij digits plus the Python layout):
    `reports/writer.py:34`, `reports/writer.py:56`, `reports/summary.py:176`,
    `reports/summary.py:196`. serde_json's float output is wrong for Python (4,998/299,973), and
    std digits are wrong on ties (123/299,973).
12. Fixed formats: `reports/summary.py:12`, `reports/summary.py:16`, `reports/summary.py:20`,
    `reports/hypotheses.py:28`, `cli.py:172`, `cli.py:313`. Rust `{:.N}` / `{:+.N}` match.
13. `math.erfc` is platform libm, and `f64::erfc` is unstable: `reports/stats.py:60`. FFI
    conflicts with `Cargo.toml:18`.
14. `math.exp`/`log`/`sqrt` at `ai/evaluation.py:305-306`, `ai/policy.py:98`,
    `ai/mcts.py:200`, `ai/mcts.py:297`, `ai/mcts.py:314`, `ai/setup.py:110`,
    `reports/stats.py:22`, `reports/stats.py:56`. These are identical to Rust std on this
    platform. Do not constant-fold them.
15. `rng.random() or 1e-300` (`ai/policy.py:97`) replaces only `0.0`.
16. `randrange` limit `2^64` for power-of-two `n` (`rng.py:34`). SplitMix64 needs wrapping
    arithmetic (`rng.py:24-27`).
17. `sort_keys` sorts the original key objects, so int keys would sort numerically. No such key
    exists today: `reports/writer.py:34`, `reports/writer.py:56`, `runner/digest.py:55`.
18. Per-file `ensure_ascii`: `reports/writer.py:34` (False), `reports/writer.py:56` and
    `runner/digest.py:55` (True).
19. The digest hashes CPython `repr` (`runner/digest.py:26-27`), and `ai/actions.py:360`
    compares `repr` for equality.
20. Registry interning indices are process-history dependent. They are only compared for
    equality and are canonicalised in the digest: `effects/registry.py:115-138`,
    `runner/digest.py:36-49`.
21. Dict insertion order drives constant application and trigger scans: `engine/view.py:425`,
    `engine/core.py:338-342`, `engine/battle.py:102-106`, `ai/actions.py:75-85`. Use `IndexMap`.
    Dict equality is order-insensitive (`engine/view.py:94-106`), which `IndexMap` matches.
22. `Counter.subtract` can go negative, and `elements()` skips counts `<= 0`: `ai/setup.py:73-75`.
    A missing `Counter` key reads 0 without being inserted: `engine/observe.py:48`.
23. `dict.setdefault` keeps the first prior: `ai/mcts.py:316-317`.
24. `pool.pop(randrange(len(pool)))` with the loop count fixed up front:
    `engine/interp.py:425-426`, `engine/interp.py:534`.
25. `str.isspace` whitespace includes U+001C–U+001F, unlike Rust: `deck/parse.py:79`,
    `deck/names.py:33`, `cards/*`, `effects/text.py`. It is latent for card data.
26. NFKC plus full `casefold`: `deck/names.py:32-33`.
27. `str.capitalize` lower-cases the tail: `reports/summary.py:177`.
28. Exception text includes CPython class names: `runner/game.py:132-136`,
    `effects/registry.py:336-337`, `runner/replayfile.py:182-183`.
29. `assert` is live: the registry catches `AssertionError` (`effects/registry.py:336`).
30. `code_digest` and `package_version` depend on the Python package: `reports/versions.py:25-52`.
31. Set iteration: none is order-dependent (§1.2). A Rust `HashSet`/`HashMap` must never be
    iterated where Python iterates a list, a dict or a sorted set.

---

## Open questions

1. **Reference libm platform.** `x ** 2` (`pow`), `exp`, `log` and `erfc` results come from the
   C library CPython links against; Linux glibc can differ from Apple libm. The checks here prove
   Rust and CPython agree on the same macOS arm64 machine. Which platform produces the golden
   Python traces and outputs, and will Rust be compared on that same platform? Not verified
   on Linux.
2. **`erfc` and `unsafe_code = "deny"`.** FFI to libm `erfc` is the only bit-exact option found:
   `f64::erfc` is unstable, and the `libm` crate is neither available offline nor verified.
   Should the workspace allow `unsafe` in one module, or does the lead accept a different
   approach? `erfc` only feeds `p_value`, which is rounded to 6 digits and compared with
   `alpha`, so the risk window is tiny but non-zero.
3. **Neumaier for ints in the float path.** The finding that 3.15 compensates ints after the
   first float rests on the emulation test (0 against 8,221 mismatches). The CPython C source was
   not read. No in-scope float `sum()` mixes ints after floats, so this only matters if a future
   change adds such a site.
4. **Coverage of the hash-salt experiment.** My byte-identical runs used greedy agents (4 games)
   and MCTS `fast` (1 game). The project test at `tests/cli/test_console_script.py:64-72` covers
   `--workers 2`, but I did not run it here (no pytest for this task). Neither covers the
   `standard`/`strong` presets. The static analysis in §1.2 covers every code path, but no MCTS
   `standard` run across salts was made, because of this task's budget rule.
5. **`code_digest` / `package_version`** (`reports/versions.py:25-52`) cannot be computed from
   Rust sources. The lead must decide whether the Rust build embeds the Python values or
   comparisons normalise them (02 §12).
6. **Std digits versus zmij** (§6.1) is decided in favour of zmij here. 02 §2.2 should be updated
   by its owner, since its claim that Rust `{:?}` has the same digits fails on exact ties.
7. Whether user deck files can contain U+001C–U+001F or other characters on which Python and Rust
   whitespace or NFKC disagree is a 03 (deck parsing) decision. It does not affect card data.
