# 10 — The `tests/` tree, testkit, golden/fuzz/traceability tooling (Python reference inventory for the Rust port)

Scope: every tracked file under `tests/`, `src/gcg_sim/testkit.py`, `src/gcg_sim/tools/{marks,traceability,golden,fuzz}.py`,
`src/gcg_sim/effects/golden.py`, `src/gcg_sim/engine/invariants.py` (used by fuzz), and how each test category should be
covered by the Rust port. Facts come from reading the code at branch `rust-port` (HEAD d9d41af plus untracked port files) and
from these commands, run on 2026-11-01 (collection only, no test was executed):

<!-- doc-check: skip inventory record of commands already run; re-running adds a fuzz run to the docs gate -->
```bash
uv run pytest --collect-only -q          # 3985 tests collected in 0.77s
uv run pytest --collect-only -q -m slow  # 7/3985 tests collected (3978 deselected)
uv run python -m gcg_sim.tools.fuzz --games 3 --seed 1 --workers 1   # sample output in §6.5
```

Cross-references: RNG/derive_seed/digest/replay formats are in `01-rng-runner-cli.md` (§1, §2.9, §2.10); card DB in
`03-data-deck.md`; DSL/registry/golden internals in `04-effects-ir.md`; state/view in `05-engine-state-view.md`; engine
core in `06a`/`06b`; MCTS in `07-ai-search.md`.

---

## 0. Quick facts

| Item | Value | Source |
| --- | --- | --- |
| Collected tests | 3985 (7 marked `slow`) | collect commands above |
| Tracked files under `tests/` | 127 (`git ls-files tests`) | — |
| Untracked stale dirs | `tests/bench/`, `tests/determinism/`, `tests/perf/` and several `tests/ai/__pycache__/*.pyc` (e.g. `test_budgets`, `test_determinize_*`, `exactness`, `toy_games`, `worlds`) contain **only `__pycache__`**, no sources. Ignore them; they are not part of the suite. | `ls tests/bench tests/determinism tests/perf` |
| Rules in index | 579 entries: 395 testable, 184 N/A; every testable rule tagged; 19 N/A rules also carry tags; 0 unknown tags | `load_rules_index()`, `marks.scan` |
| FAQ entries | 119, all 119 tagged; `tests/meta/faq_na.json` is `{}` | `tests/meta/faq_na.json` |
| Card rulings | 368; 360 tagged; 8 N/A in `tests/meta/rulings_na/*.json` | §4.3 |
| Real cards | 1152 (538 compiled, 302 binding, 312 vanilla); all 840 non-vanilla tagged; 847 distinct `card` tag values (extra: ST13-003, T-001, T-002, T-003, T-006, T-007, T-013) | §4.4 |
| Tag occurrences (dedup by mark,value,file,test) | card 2197, rule 1694, ruling 397, faq 225 | `marks.scan(Path("tests"))` |
| Golden files | 27 prefixes, 1152 cards | `tests/effects/golden/*.json` |
| Plugins | pytest ≥9.1.1, pytest-xdist ≥3.8.0, hypothesis ≥6.170.0 (dev group) | `pyproject.toml` `[dependency-groups]` |

---

## 1. pytest configuration and global fixtures

- `pyproject.toml:53-62`: `testpaths = ["tests"]`, `addopts = "-ra --strict-markers --import-mode=importlib"`, markers
  `rule(id)`, `card(number)`, `faq(num)`, `ruling(card_num)`, `slow`. **No `-m "not slow"` in addopts**: a plain
  `uv run pytest -n 8` (CLAUDE.md) runs the 10,000-game fuzz and the 800 MCTS strength games too. `scripts/verify_all.sh:23`
  runs `uv run pytest -m "" -n auto -q`.
- `--strict-markers`: any unregistered marker is a collection error.
- `--import-mode=importlib`: test modules import each other as `tests.ai.decks`, `tests.ai.test_information_set`,
  `tests.paranoid` etc. (`tests/ai/test_root_moves.py:14-21`, `tests/conftest.py:11`); `tests/` and `tests/ai/` have no
  `__init__.py` (only `tests/rules_index/__init__.py`, `tests/sources/__init__.py`).
- Root conftest `tests/conftest.py:13-21`: autouse fixture `paranoid_view`. If env `GCG_SIM_PARANOID_VIEW` is set
  (non-empty), installs `tests.paranoid.CacheChecker` on every test and asserts `not checker.stale` at teardown.
- Environment variables read by tests: `GCG_SIM_PARANOID_VIEW` (conftest.py:15), `GCG_SIM_TEST_WORKERS`
  (`tests/slow/test_ai_strength.py:24-25`, default `max(1, cpu_count-1)`), `GCG_SIM_AGENT_FACTORY` (set by
  `tests/cli/conftest.py:20-21,31-33`, deleted in `tests/cli/test_cli.py:185,291`), `PYTHONHASHSEED` (set to "1"/"2" by
  `tests/cli/test_console_script.py:64-72`).
- Hypothesis property tests (examples are random per run; there is no `settings.register_profile`/`derandomize`; a local
  `.hypothesis/` example DB exists): `tests/ai/test_agents.py:151-152` (`max_examples=8`, seed in `[0, 2**32]`),
  `tests/deck/test_parse.py:142-143` (`max_examples=60`), `tests/reports/test_stats.py:36,46`,
  `tests/runner/test_config.py:26` (`master ∈ [0, 2**64-1]`, `m ∈ [0,10000]`, `g ∈ [0,2]`).

### 1.1 Paranoid cache checker (`tests/paranoid.py`)

`CacheChecker.install` monkeypatches five engine functions (`paranoid.py:59-64`):
`view.derived`, `core.emit`, `core._damage_scan`, `core.rules_management`, `view.resolve_ref`. Checks (counter names are
the `checks` keys asserted in `tests/rules/test_engine_caches.py:43-47`):

| Counter key | When counted | Check | Stale message |
| --- | --- | --- | --- |
| `"cached view"` | `derived(st)` returned the object already in `st._derived` (`paranoid.py:67-71`) | `view_signature(V._compute(st)) == view_signature(dv)`; and if `dv.by_event is not None`, `index_signature(V.trigger_index(st, fresh)) == index_signature(dv.by_event)` | `"view, turn {t}, step {STEP_NAME}"`, `"trigger index, turn {t}, step {STEP_NAME}"` |
| `"unlistened event"` | `st.winner is None and ev not in core.listened_events(st) and not st.delayed and not lki` (`paranoid.py:90`) | `V.trigger_index(st, V.derived(st)).get(ev)` must be falsy | `"unlistened {ev.value} has candidates, turn {t}"` |
| `"no printed lethal damage"` | `_damage_scan` returned `(damaged=True, lethal=False)` | no BATTLE/BASE card of either player with `damage >= V.hp_of(...)` | `"{uid} destroyed by rules, turn {t}"` |
| `"unchanged state"` | `st.winner is None and st.phase is not Phase.SETUP and st._rules_checked == st.version` | on a clone with `_rules_checked=-1`, full rules management must leave `(winner, version, effect_hits, len(pending_triggers))` unchanged | `"rules management skipped a change, turn {t}"` |
| `"view-free reference"` | `not V.selects(ref)` | `V.resolve(st, V.derived(st), ctx, ref) == got` | `"reference {ref} resolved without the view, turn {t}"` |

`view_signature(dv)` (`paranoid.py:24-39`) = `(dv.ap, dv.hp, dv.kw, dv.traits, {u: [(r.rule, r.controller, r.source,
r.key, r.aux) ...]}, dv.cost_mod, dv.level_mod, sorted(dv.linked), {u: [a.aid ...]})` — a ready-made definition of "the
whole derived view", reusable for differential export (§10.2).

---

## 2. Module inventory

Counts are collected test items (parametrized cases counted separately). Tag columns are tag occurrences found by the
static scanner (`marks.scan`, §3.1), so parametrized cases tagged via `pytest.param(..., marks=pytest.mark.card(n))` with a
non-literal `n` are not counted (§3.1 hazard).

| module | collected | slow | rule | card | faq | ruling | purpose |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `tests/test_no_placeholders.py` | 2 | 0 | 0 | 0 | 0 | 0 | meta: no TODO/FIXME/XXX/HACK/NotImplementedError, no placeholder bodies in `src/` (§4.1) |
| `tests/test_rules_coverage.py` | 3 | 0 | 0 | 0 | 0 | 0 | meta: every testable rule tagged, N/A reasons non-empty, tags name real rules (§4.2) |
| `tests/test_faq_ruling_coverage.py` | 2 | 0 | 0 | 0 | 0 | 0 | meta: every FAQ entry and ruling tagged or N/A (§4.3) |
| `tests/effects/test_card_coverage.py` | 316 | 0 | 0 | 0 | 0 | 0 | meta: every card implemented / tagged / vanilla has no abilities / modal effects complete (§4.4) |
| `tests/effects/test_golden.py` | 27 | 0 | 0 | 0 | 0 | 0 | golden: compiled behaviour per set prefix (§5) |
| `tests/rules/test_battle.py` | 64 | 0 | 88 | 26 | 16 | 1 | rules 5-22, 7-5-4/8 (attacks, battles), 7-6-3/8-4/9 (action steps) |
| `tests/rules/test_cardinfo.py` | 63 | 0 | 79 | 0 | 0 | 0 | rules section 2 (card information) and 5 (terminology) |
| `tests/rules/test_damage.py` | 54 | 0 | 99 | 0 | 0 | 0 | damage, recovery, counters, reduction (5-5, 5-6, 5-18, 5-21), tokens (2-9-3, 2-10-3, 5-17), rules management (11) |
| `tests/rules/test_effects.py` | 55 | 0 | 65 | 0 | 0 | 11 | effect activation/resolution: 1-3-7, 5-16, 5-20, section 10 |
| `tests/rules/test_engine_caches.py` | 3 | 0 | 4 | 0 | 0 | 0 | paranoid checker over 16 fuzz games (seed 11) + 2 greedy games; late `Scenario.add` still triggers |
| `tests/rules/test_faq.py` | 125 | 0 | 241 | 0 | 123 | 0 | one or more engine tests per rules-FAQ entry |
| `tests/rules/test_keywords.py` | 77 | 0 | 91 | 0 | 0 | 0 | rules section 13 (keyword effects, keywords) |
| `tests/rules/test_lead_examples.py` | 8 | 0 | 18 | 1 | 0 | 0 | reference rule tests (template) |
| `tests/rules/test_pairing.py` | 72 | 0 | 106 | 0 | 13 | 2 | rule 3 card types, pairing/linking (2-4-3..2-12, 3-2-6, 3-3, 3-4-6, 5-9) |
| `tests/rules/test_turn.py` | 73 | 0 | 96 | 0 | 16 | 0 | 1-1, 1-2, 1-3, 6-1-2, 6-2, 7 (phase/step order, draw/resource/main/end) |
| `tests/rules/test_zones.py` | 53 | 0 | 66 | 0 | 13 | 1 | 4-1..4-9 locations, limits, visibility (checked against `engine.observe`) |
| `tests/cards/test_lead_examples.py` | 4 | 0 | 1 | 4 | 0 | 1 | reference card tests (GD01-001) |
| `tests/cards/test_wp_eb01_a.py` | 100 | 0 | 41 | 103 | 0 | 12 | EB01-001..046 |
| `tests/cards/test_wp_eb01_b.py` | 124 | 0 | 62 | 123 | 0 | 16 | EB01-047..090 |
| `tests/cards/test_wp_gd01_a.py` | 123 | 0 | 41 | 114 | 3 | 24 | GD01-001..072 |
| `tests/cards/test_wp_gd01_b.py` | 154 | 0 | 48 | 154 | 7 | 13 | GD01-073..130 |
| `tests/cards/test_wp_gd02_a.py` | 153 | 0 | 18 | 137 | 0 | 23 | GD02-001..085 |
| `tests/cards/test_wp_gd02_b.py` | 138 | 0 | 40 | 114 | 1 | 8 | GD02-086..130 |
| `tests/cards/test_wp_gd03_a.py` | 139 | 0 | 19 | 144 | 4 | 30 | GD03-001..083 |
| `tests/cards/test_wp_gd03_b.py` | 144 | 0 | 53 | 113 | 0 | 19 | GD03-084..132 |
| `tests/cards/test_wp_gd04_a.py` | 143 | 0 | 26 | 151 | 0 | 21 | GD04-001..074 |
| `tests/cards/test_wp_gd04_b.py` | 138 | 0 | 61 | 140 | 0 | 23 | GD04-075..130 |
| `tests/cards/test_wp_gd05_a.py` | 155 | 0 | 15 | 153 | 0 | 59 | GD05-001..070 |
| `tests/cards/test_wp_gd05_b.py` | 178 | 0 | 22 | 159 | 0 | 45 | GD05-071..130 |
| `tests/cards/test_wp_st01_04.py` | 140 | 0 | 54 | 141 | 7 | 19 | starters ST01-ST04 |
| `tests/cards/test_wp_st05_08.py` | 137 | 0 | 49 | 139 | 0 | 14 | starters ST05-ST08 |
| `tests/cards/test_wp_st09_11.py` | 95 | 0 | 65 | 95 | 5 | 22 | starters ST09-ST11 |
| `tests/cards/test_wp_st12_14.py` | 145 | 0 | 61 | 151 | 0 | 33 | starters ST12-ST14 |
| `tests/cards/test_wp_tokens.py` | 35 | 0 | 32 | 35 | 4 | 0 | Unit tokens T-001..T-029 (rule 5-17) |
| `tests/ai/test_agents.py` | 70 | 0 | 0 | 0 | 0 | 0 | agents return legal actions for every decision kind, determinism, decision-log format, canonical moves, no-op pruning |
| `tests/ai/test_baseline.py` | 1 | 0 | 0 | 0 | 0 | 0 | **recorded full-game digest** of the baseline MCTS (§7.1) |
| `tests/ai/test_compare.py` | 12 | 0 | 0 | 0 | 0 | 0 | `gcg_sim.ai.compare`: agent specs, deck sets, mirrored specs, worker-count independence, CLI |
| `tests/ai/test_decks.py` | 3 | 0 | 0 | 0 | 0 | 0 | AI test decks legal and implemented |
| `tests/ai/test_evaluation_roles.py` | 8 | 0 | 0 | 0 | 0 | 0 | `roles` evaluation feature set, fitted weights, tuning options |
| `tests/ai/test_fast_preset.py` | 3 | 0 | 0 | 0 | 0 | 0 | fast preset = standard minus budget/noise (`BUDGET_RATIO = 4.5`); weakened deck legal |
| `tests/ai/test_information_set.py` | 52 | 0 | 0 | 0 | 0 | 0 | criterion 7c: decision invariant under `permute_hidden` (§7.2) |
| `tests/ai/test_information_set_policy.py` | 26 | 0 | 0 | 0 | 0 | 0 | same with every policy option on |
| `tests/ai/test_information_set_weights.py` | 50 | 0 | 0 | 0 | 0 | 0 | same with `roles` weights, mcts and greedy |
| `tests/ai/test_policy_knobs.py` | 18 | 0 | 0 | 0 | 0 | 0 | `SearchConfig.policy_*` knobs: burst score, rule-polarity table, zero-AP face attack |
| `tests/ai/test_puzzles.py` | 36 | 0 | 0 | 0 | 0 | 0 | criterion 7b tactical puzzles (§7.3) |
| `tests/ai/test_puzzles_shipped.py` | 12 | 0 | 0 | 0 | 0 | 0 | 2026-10 puzzles played by the shipped preset (§7.3) |
| `tests/ai/test_puzzles_targets.py` | 4 | 0 | 0 | 0 | 0 | 0 | effect-target puzzles, seed sweeps vs baseline (§7.3) |
| `tests/ai/test_root_moves.py` | 52 | 0 | 0 | 0 | 0 | 0 | root-move settings (noop branching, resource-aware, strict attack prune, rest targets, merge equivalent) with negative controls |
| `tests/ai/test_search_options.py` | 33 | 0 | 0 | 0 | 0 | 0 | `target_extension`, `observation_keys`; info-set invariance with both on |
| `tests/ai/test_search_side_effects.py` | 2 | 0 | 0 | 0 | 0 | 0 | search never mutates the true state (`state_digest` before/after) |
| `tests/ai/test_throughput.py` | 3 | 3 | 0 | 0 | 0 | 0 | throughput tool reports positive speed (wall clock) |
| `tests/ai/test_tuning.py` | 8 | 0 | 0 | 0 | 0 | 0 | logistic fit, linear solve, tuning reproducibility, CLI JSON |
| `tests/slow/test_ai_strength.py` | 2 | 2 | 0 | 0 | 0 | 0 | criterion 7a: MCTS standard vs random/greedy, 400 games each (§7.4) |
| `tests/slow/test_robustness.py` | 2 | 1 | 6 | 0 | 0 | 0 | criterion 6: 10,000 fuzz games (slow) + 40-game smoke (§6.4) |
| `tests/runner/test_config.py` | 13 | 0 | 0 | 0 | 0 | 0 | seed derivation, seats, config validation messages |
| `tests/runner/test_digest.py` | 2 | 0 | 0 | 0 | 0 | 0 | digest independent of registry numbering |
| `tests/runner/test_instrument.py` | 8 | 0 | 0 | 0 | 0 | 0 | per-card instrumentation vs action stream |
| `tests/runner/test_match.py` | 8 | 0 | 0 | 0 | 1 | 0 | BO3 sequencing, simultaneous defeat, turn-limit draws |
| `tests/runner/test_pool.py` | 6 | 0 | 0 | 0 | 0 | 0 | worker independence, progress, failure propagation, interrupts |
| `tests/runner/test_replay.py` | 9 | 1 | 0 | 0 | 0 | 0 | game records replay to the recorded digest; replay-document tamper detection |
| `tests/reports/test_reports.py` | 19 | 0 | 0 | 0 | 0 | 0 | report files: schema, contents, reproducibility, replays |
| `tests/reports/test_hypotheses.py` | 7 | 0 | 0 | 0 | 0 | 0 | tuning-signal generation thresholds |
| `tests/reports/test_stats.py` | 9 | 0 | 0 | 0 | 0 | 0 | Wilson intervals, percentiles, two-proportion test, Holm |
| `tests/cli/test_cli.py` | 34 | 0 | 0 | 0 | 0 | 0 | every CLI command via `main([...])`, messages, exit codes |
| `tests/cli/test_console_script.py` | 5 | 0 | 0 | 0 | 0 | 0 | installed `gcg-sim`: byte-identical outputs for 1 vs 2 workers, `PYTHONHASHSEED` independence, fresh-process replays, Ctrl-C |
| `tests/deck/test_examples.py` | 13 | 0 | 6 | 0 | 0 | 0 | example decks legal/documented/playable |
| `tests/deck/test_parse.py` | 20 | 0 | 0 | 0 | 0 | 0 | deck-file grammar, alt-art ids, name checks, digest |
| `tests/deck/test_validate.py` | 32 | 0 | 21 | 0 | 12 | 0 | one test per legality violation class (rules 2-1-2, 3-x-2, 6-1-1-x; FAQ Q1-Q7) |
| `tests/rules_index/test_rules_index.py` | 104 | 0 | 0 | 0 | 0 | 0 | Comprehensive Rules markdown parser / `rules_index.json` / N/A file |
| `tests/sources/test_conflicts.py` | 42 | 0 | 0 | 0 | 0 | 0 | source-conflict detector (`gcg_sim.sources.conflicts`) |
| `tests/tools/test_refresh.py` | 15 | 0 | 0 | 0 | 0 | 0 | `/gcg-refresh-data` core (`gcg_sim.tools.refresh`) |

Directory totals: ai 393, cards 2245, cli 39, deck 65, effects 343, reports 35, rules 647, rules_index 104, runner 46, slow 4,
sources 42, tools 15, root meta 7.

### 2.1 Non-test support files

| File | Content |
| --- | --- |
| `tests/ai/decks.py` | `DECKS = (FEDERATION_DECK, SEED_DECK)`; each main list = `expand(counts)` = numbers in **sorted** order, each repeated by count (`decks.py:63-64`); resources `("R-001",)*10`. Counts: `decks.py:15-36` (Federation, 50) and `decks.py:39-60` (SEED, 50). |
| `tests/ai/situations.py` | One builder per `DecisionKind` except `PAYMENT` (`SITUATIONS`, `situations.py:147-160`) plus `EXTRA_SELECTS` (`select_up_to_two`, `select_mode`, `:162-165`). `choose_first()` = `new_game(DECKS, 5)`; `redraw()` = `new_game(DECKS, 6)` + `options[0]`. |
| `tests/runner/random_agent.py` | `SeededRandomAgent` (name `"test-random"`, `SplitMix64(spec.seed)`, picks `options[rng.randrange(len(options))]`; log entry `{"turn","player","decision","chosen","alternatives":[{"action": o.to_json(),"value":0.0,"visits":0} for every option]}` — no `"label"` key) and `IllegalActionAgent` (name `"test-illegal"`, returns `Action(ATTACK, 10**6, 10**6)`). Loaded by path `"<abs path>/random_agent.py:make_random_agent"` (`tests/runner/conftest.py:11`, `tests/cli/conftest.py:13`, `tests/reports/conftest.py:15`). |
| `tests/runner/conftest.py` | session fixtures `random_factory`, `federation`/`zeon`/`tekkadan` (examples/decks), `config = BenchmarkConfig(federation, zeon, matches=6, seed=1234)`. |
| `tests/reports/conftest.py` | session `run = run_benchmark(BenchmarkConfig(federation, zeon, matches=60, seed=3, replays=2), agent_factory=random)`; `written`; `make_record` synthetic `GameRecord` factory. |
| `tests/deck/conftest.py` | `base_counts` (from `blue-white-federation.txt`), `make_deck` (`"{count} {number}"` lines, `name="test-deck"`). |
| `tests/cli/conftest.py` | `random_agents` (sets `GCG_SIM_AGENT_FACTORY`), `console_script` (`Path(sys.executable).parent / "gcg-sim"`), `script_env`. |
| `tests/data/decks/red-green-zeon-weakened.txt` | deliberately weakened deck for `test_fast_preset.py:41`. |
| `tests/meta/faq_na.json`, `tests/meta/rulings_na/*.json` | N/A reasons (§4.3). |
| `tests/effects/golden/*.json` | golden files (§5). |

---

## 3. Markers and traceability

### 3.1 Static tag scanner (`src/gcg_sim/tools/marks.py`)

Coverage gates and traceability read tags from source with `ast`, never from a pytest run (`marks.py:1-5`).

- `Tag(mark, value, path, test, line)` frozen dataclass (`marks.py:14-20`).
- A call is a mark iff its `func` is `Attribute(attr=<name>, value=Attribute(attr="mark", value=Name(id="pytest")))`
  (`marks.py:23-33`); `import pytest as p` or `from pytest import mark` would NOT be seen.
- Values (`marks.py:36-47`): each positional arg that is a `str` constant, or a `list`/`tuple` literal whose `str`
  constant elements are taken (non-constant elements skipped). Keyword args and any non-literal (Name, f-string,
  call) are **silently ignored**.
- Traversal (`marks.py:98-116`): files = `sorted(root.rglob("test_*.py"))` (helper modules like `decks.py`,
  `paranoid.py` are not scanned); for each top-level node `v.visit(node)`.
  - `FunctionDef`/`AsyncFunctionDef`/`ClassDef` (`marks.py:63-89`): `test = "::".join([*scope, name])`; decorator calls are
    recorded first, then **every** mark call anywhere inside the node (`ast.walk`, including nested functions,
    `pytest.param(..., marks=...)` inside decorator arguments, function bodies) is attributed to this outermost test.
  - Module-level `Assign` (`marks.py:91-95`): every mark call inside the assignment is recorded with `test="<module>"`
    (intended for `pytestmark = ...`, but any assignment counts).
  - Other module-level statements fall to `generic_visit`, so definitions nested in `if`/`with` are still visited.
- `path` = `str(path.relative_to(root.parent))` when `root.parent in path.parents`, else `str(path)` (`marks.py:103-105`):
  `"tests/cards/test_wp_gd01_a.py"` for `root = Path("tests")` or the absolute tests dir.
- De-duplication on `(mark, value, path, test)`, first occurrence kept, order preserved (`marks.py:109-116`).
- `values(tags, mark)` → `set[str]` (`marks.py:119-120`).

### 3.2 `docs/RULES_TRACEABILITY.md` generator (`src/gcg_sim/tools/traceability.py`)

Usage (CLAUDE.md): `uv run pytest -n 8 --junitxml=build/junit.xml` then
`uv run python -m gcg_sim.tools.traceability --junit build/junit.xml` (the module docstring uses `pytest -m ""`).
CLI (`traceability.py:91-105`): `--tests` (default `Path("tests")`), `--junit`, `--out` (default
`Path("docs/RULES_TRACEABILITY.md")`), `--check`; paths are relative to CWD. Always writes the file, prints
`wrote {out}` to stdout, prints at most 50 problems to stderr, exits 1 only with `--check` and problems.

`junit_outcomes(path)` (`traceability.py:22-36`): for each `<testcase>`: `name = case.get("name").split("[", 1)[0]`
(parametrization stripped), `file = classname.replace(".", "/") + ".py"`, key `f"{Path(file).name}::{name}"` (basename
only); status `failed` if any child tag is `failure` or `error`, else `skipped` if a `skipped` child, else `passed`; a key
already `failed` stays `failed` (failure is sticky across parametrized cases).

`build(tests_root, junit)` (`traceability.py:39-88`), exact output:

```
# Rules traceability
<blank>
Comprehensive Rules Ver. {ri.version} ({ri.date}). Generated by `gcg_sim.tools.traceability` from
`@pytest.mark.rule` tags and the JUnit results of the full test run.      <- " and the JUnit ... run." only with --junit, else "."
Summary: {tested} rules tested, {na} N/A with reasons, {missing} missing.  <- inserted at index 4
<blank>
<blank>
| rule | status | tests / reason |
| --- | --- | --- |
...one row per rule id in index (document) order...
```

Rows: N/A rule → `| {rid} | N/A | {reason} |` (reason inserted raw). Testable: `tests = sorted(set(by_rule[rid]))`
where entries are `f"{basename}::{last component of Tag.test}"`; none → `| {rid} | MISSING | |` and problem
`"rule {rid} has no test"`; else `| {rid} | tested ({len}) | {first 4 in backticks joined by ", "}{" (+{len-4})" if len>4} |`.
With JUnit, a rule none of whose tests has outcome `passed` (unknown → `"unknown"`) adds problem
`"rule {rid} has no passing test: {tests}"` (Python list repr). Text = `"\n".join(lines) + "\n"`.
Current values: version `1.9.0`, date `2026-09-11`, summary `395 rules tested, 184 N/A with reasons, 0 missing`.
Regenerating without JUnit reproduces the committed file except line 4 (verified by diff into scratch).

---

## 4. Meta tests

### 4.1 `tests/test_no_placeholders.py`

- `test_no_banned_markers` (`:12-19`): every `src/**/*.py` (sorted), every line, regex
  `\b(TODO|FIXME|XXX|HACK)\b|NotImplementedError`; failure lists `"{relpath}:{lineno}: {stripped line}"`.
- `test_no_placeholder_function_bodies` (`:22-54`): every `FunctionDef`/`AsyncFunctionDef` whose body, after removing
  string-constant expression statements (docstrings), is exactly one `pass` or one `...`, unless decorated with a bare
  Name `overload` or `abstractmethod` (`@typing.overload` as Attribute is not exempt).
- Rust analog: a test over `crates/**/*.rs` banning `todo!`, `unimplemented!`, `TODO`, `FIXME`, `XXX`, `HACK`.

### 4.2 `tests/test_rules_coverage.py`

`ri.testable_ids()` must each be in `values(scan(TESTS, ("rule",)), "rule")`; `all(ri.na_reason(rid) for rid in
ri.na_ids())`; every tagged rule id exists (`ri.get(v) is not None`). Testable = index entries not in
`rules_na.json` (`rules/index.py:233-247`).

### 4.3 `tests/test_faq_ruling_coverage.py`

- FAQ: `rules-faq.json` (cached gcgapi) entries' `num` (`"Q1"`...) must be tagged `faq` or a key of
  `tests/meta/faq_na.json`; all N/A reasons non-blank.
- Rulings: ids `f"{r['card_number']}:{r['num']}"` from `rulings.json` must be tagged `ruling` or a key in the union of
  `tests/meta/rulings_na/*.json` (sorted glob, later files override earlier keys, `:15-20`).
- Format of every N/A file: one JSON object `{id: reason}`. Non-empty files today: `wp_eb01_a.json`
  (EB01-005:Q312, EB01-009:Q314), `wp_eb01_b.json` (EB01-074:Q330), `wp_gd05_a.json` (GD05-034:Q486),
  `wp_st09_11.json` (ST10-001:Q302, ST10-008:Q305, ST10-008:Q306), `wp_st12_14.json` (ST14-001:Q464); all are
  multiplayer-only rulings. `wp_st05_08.json` does not exist.

### 4.4 `tests/effects/test_card_coverage.py`

| test | rule |
| --- | --- |
| `test_every_card_is_implemented` (`:23-30`) | every registry entry with `script is None` must be a `TOKEN:` card |
| `test_every_non_vanilla_card_has_a_behaviour_test` (`:33-43`) | every real card whose script source is not `"vanilla"` has a `card` tag |
| `test_card_tags_name_real_cards` (`:46-49`) | every `card` tag resolves via `reg.db.get` |
| `test_vanilla_cards_have_no_abilities[number]` (`:52-61`) | 312 cases, `sorted` vanilla real card numbers; `abilities == () and unit_abilities == ()` |
| `test_every_modal_effect_offers_every_printed_mode` (`:78-95`) | every `ChooseMode` (searched recursively through `steps`/`then`/`otherwise` tuples and option bodies) has ≥2 options; if text matches `re.search(r"choose (?:1|one) of the following", effect, re.I)`, `effect.count("■")` must be one of the mode counts |

Note: parametrize lists are computed at import (collection) time from `get_registry()`, so collection builds the registry.

### 4.5 `tests/rules_index/`, `tests/sources/`, `tests/tools/`

Python data-pipeline tooling (rules markdown parser, conflicts detector, refresh skill). Not part of the game engine; see
§10 for the recommendation (keep in Python).

---

## 5. Golden tests (`tools/golden.py`, `effects/golden.py`, `tests/effects/test_golden.py`)

### 5.1 Builder

`build(prefix=None)` (`tools/golden.py:22-32`): for every `cdef in reg.db.real_cards()` (DB order), `p = prefix_of(number)`
= `number.split("-", 1)[0]` (`effects/golden.py:54-55`); `out.setdefault(p, {})[number] = card_golden(cdef,
reg.cards[cdef.def_id], binding_module(number))`.

`card_golden` (`effects/golden.py:40-51`), keys (final order irrelevant, `sort_keys=True` at dump):
`name` = `cdef.name`; `text_hash` = `sha256(normalize(effect).encode()).hexdigest()[:16]` (`effects/text.py:94-95`); if
`entry.script is None`: `error` = `entry.error` and stop; else `source` = `"binding"` if a binding module exists else
`entry.script.source` (`"compiled"`/`"vanilla"`); `binding` = last dotted component of the module (e.g. `"wp_gd01_a"`)
only for bindings; `abilities` = `node_to_json(script.abilities)`; `unit_abilities` only if non-empty.

`node_to_json(x)` (`effects/golden.py:19-37`):
1. dataclass instance → `{"_": type name}` then fields in declaration order; a field is **omitted when `v == default`**
   (`default_factory()` is called for factory fields); otherwise `node_to_json(v)`.
2. `enum.Enum` → `.value`.
3. `tuple`/`list` → list.
4. `dict` → `{str(k): node_to_json(v) for k, v in sorted(x.items())}` (sorted by the original key objects).
5. anything else returned as is (int, str, bool, None, float).

### 5.2 Serialization and files

`dump(data) = json.dumps(data, indent=1, sort_keys=True, ensure_ascii=False) + "\n"` (`tools/golden.py:35-36`): indent 1
space, item separator `","` + newline, key separator `": "`, raw UTF-8 (files contain e.g. `Ξ`, `Ⅱ`, `Ⅲ`, `’`, `ō`).
One file per prefix `tests/effects/golden/{prefix}.json`. Card counts per prefix: EB01 90, EXB 3, EXBP 27, EXR 11,
EXRP 18, GD01 130, GD02 130, GD03 132, GD04 130, GD05 130, R 41, RP 67, ST01..ST04 16 each, ST05..ST08 15 each, ST09 10,
ST10..ST14 16 each, T 29 (sum 1152; sources: compiled 538, binding 302, vanilla 312).

CLI (`tools/golden.py:39-61`): `--write`, `--check`, `--prefix`, `--dir` (default `tests/effects/golden`, CWD-relative).
Without `--write` it checks (also when neither flag is given). For each prefix in `sorted(data)`: write, or compare the
file text (missing file = `""`) and print `golden out of date: {path}` to stderr. `--write` prints
`wrote {len(data)} golden files to {dir}`. Exit 1 if any mismatch. Stale files for prefixes that no longer exist are not
detected.

`tests/effects/test_golden.py`: `DATA = build()` at import; `test_golden_matches[prefix]` for `sorted(DATA)` (27 cases);
on mismatch reports the sorted card numbers whose parsed JSON differs (first 20).

Sample (`GD01-002`, abridged): `{"abilities":[{"_":"PlayModifier","cost":0,"costs":[{"_":"DestroyCards","count":1,"sel":{...}}],"level":0},{"_":"Triggered","steps":[...],"trigger":{"_":"Trigger","event":"attacks"}}],"binding":"wp_gd01_a","name":"Unicorn Gundam (Destroy Mode)","source":"binding","text_hash":"f124a6c4ef85b1ab"}`.

---

## 6. Fuzz tooling (`src/gcg_sim/tools/fuzz.py`) and invariants

### 6.1 Pools

- `resource_pool()` (`fuzz.py:28-30`): `sorted` card numbers of real cards with `card_type is CardType.RESOURCE`
  (108: R 41, RP 67).
- `main_deck_pool()` (`fuzz.py:33-43`): `{Color: sorted numbers}` of real cards with type in
  `MAIN_DECK_TYPES = {UNIT, PILOT, COMMAND, BASE}`, `color is not None`, and a script. Sizes: Blue 208, Green 210,
  Red 184, White 202, Purple 152 (956; tokens have type `UNIT TOKEN`, color None, so excluded). Dict built in `Color`
  declaration order (Blue, Green, Red, White, Purple; `cards/model.py:40-45`).

### 6.2 `deck_for(index, seed) -> DeckList` (`fuzz.py:46-71`), exact algorithm

```
pool   = main_deck_pool()
colors = sorted(pool, key=lambda c: c.value)          # by string value: Blue, Green, Purple, Red, White
rng    = SplitMix64(derive_seed(seed, "fuzz-deck", index))
c1 = colors[index % 5]
c2 = colors[(index // 5 + index) % 5]
cards = pool[c1] + (pool[c2] if c2 is not c1 else [])
start = (index // 5) * 13 % max(1, len(cards))          # precedence: ((index//5)*13) % max(...)
rotated = cards[start:] + cards[:start]
main = []
for n in rotated:
    if len(main) >= 50: break                           # checked BEFORE drawing: one randrange per appended number
    main += [n] * min(1 + rng.randrange(4), 50 - len(main))
i = 0
while len(main) < 50:                                   # no RNG use here
    n = rotated[i % len(rotated)]
    if main.count(n) < 4: main.append(n)
    i += 1
res = resource_pool(); r0 = (index * 10) % len(res)
resources = tuple((res[r0:] + res[:r0])[:10])
return DeckList(tuple(main), resources)
```

Vector: `deck_for(0, 1).main[:12] == ('EB01-001','EB01-001','EB01-001','EB01-002','EB01-002','EB01-003','EB01-003','EB01-003','EB01-003','EB01-004','EB01-004','EB01-004')`, 50 cards, resources `('R-001',...,'R-010')`.
The decks are not run through deck validation (colour mix and card copies are ≤4 by construction; `new_game` performs its
own copy check, `tests/rules/test_turn.py:1283`).

### 6.3 `play_one(index, seed, check_every=1) -> dict` (`fuzz.py:83-119`)

1. `d0 = deck_for(2*index, seed)`, `d1 = deck_for(2*index+1, seed)`.
2. `game_seed = derive_seed(seed, "fuzz-game", index)`; agent `rng = SplitMix64(derive_seed(seed, "fuzz-agent", index))`.
3. `out = {"index": index, "seed": game_seed, "cards": sorted(set(d0.main)|set(d1.main)|set(d0.resources)|set(d1.resources))}`.
4. `st = new_game((d0, d1), game_seed)`; `init = initial_multiset(st)`.
5. Loop `while st.winner is None`: `opts = st.pending.options if st.pending else ()`; `a = opts[rng.randrange(len(opts))]`
   (empty options → `ValueError("randrange() requires n > 0")`, caught below); append `a.to_json()`; `apply(st, a)`;
   `n += 1`; `if n % check_every == 0: check(st, init)`. After the loop `check(st, init)` again.
6. Success: `out.update(ok=True, actions=n, end=st.end_reason.value if st.end_reason else "none", turns=st.turn)`; if
   `end_reason is EndReason.TURN_LIMIT` → `ok=False, error="game ended by the engine safety cap (not a rules-defined end)"`.
7. Any exception (`except (InvariantViolation, Exception)`) → `ok=False, error=f"{type(exc).__name__}: {exc}",
   trace=traceback.format_exc(limit=8), actions_so_far=len(actions), last_actions=actions[-5:]`.

Vector: `play_one(0, 1)` → `index 0, seed 3464613713843929015, ok True, actions 104, end "battle_damage", turns 22`.

### 6.4 `run(games, seed, workers)` and CLI

`run` (`fuzz.py:122-138`): indices `0..games-1`; `workers <= 1` → in-process list comprehension; else
`ProcessPoolExecutor(max_workers=workers)` (default start method) `ex.map(play_one, indices, [seed]*games, chunksize=16)`
(results in index order). Aggregation in index order: `games += 1`, `cards_seen |= o["cards"]`, ok → `actions +=`,
`ends[str(o["end"])] += 1` (a `Counter`, insertion order = first occurrence); else `failures.append(o)`.

CLI `main` (`fuzz.py:141-166`): `--games` 1000, `--seed` 1, `--workers` 1, `--show` 5. Prints
`json.dumps(summary, indent=1)` with keys in this order: `games`, `actions`, `ends` (`dict(res.ends)`), `failures`
(count), `cards_covered` (`len(cards_seen & pool)`), `cards_in_pool` (`len(pool)`, pool = main-deck pool ∪ resource pool
= 1064), `cards_missing` (`sorted(pool - cards_seen)[:20]`). For the first `--show` failures, prints to stderr
`json.dumps({k: f.get(k) for k in ("index","seed","error","last_actions")})` (default separators) then the trace.
Exit 1 iff any failure. Sample (`--games 3 --seed 1`): `games 3, actions 349, ends {"battle_damage": 3}, failures 0,
cards_covered 172, cards_in_pool 1064, cards_missing ["EB01-039", ...20 items]`.

Users: `tests/slow/test_robustness.py:15-24` (`run(10_000, seed=2026, workers=max(1, cpu-1))`, asserts no failures,
`set(res.ends) <= {"battle_damage","deck_out","both_defeated"}`, every pool card seen; rule tags
1-2-1, 1-2-2-1, 11-1-2, 4-5-4, 4-6-3, 4-4-2), `test_small_fuzz_smoke` (`run(40, seed=5, workers=1)`),
`tests/rules/test_engine_caches.py:28` (`run(16, seed=11, workers=1)` under the paranoid checker).

### 6.5 Invariants checked after every action (`engine/invariants.py:29-109`)

Errors are collected in this order and raised as `InvariantViolation("; ".join(errors[:10]))`:
1. For p in (0,1), for zone in `Zone` minus `OUTSIDE` (enum order), each listed uid: `zone` field matches
   (`"uid {u} listed in {Z} but zone={Z'}"`), owner matches (`"uid {u} owned by {o} listed under player {p}"`).
2. For each card in uid order: OUTSIDE → must not be listed, must be a token; else listed exactly once; tokens only in
   `core.TOKEN_ZONES` (= {BATTLE, RESOURCE_AREA, SHIELD, BASE}, `core.py:27`); `damage >= 0`; damage only in BATTLE/BASE; PAIRED pilot ↔ BATTLE unit pair consistency; BATTLE card
   must be a unit type; non-PAIRED/non-BATTLE with `pair >= 0` is an error; BASE only bases; RESOURCE_AREA/RESOURCE_DECK only
   resources.
3. Per player: BATTLE ≤ `core.BATTLE_LIMIT` (6), BASE ≤ `core.BASE_LIMIT` (1), resource area ≤ `RESOURCE_LIMIT` (15) and EX
   resources ≤ `EX_RESOURCE_LIMIT` (5) (`core.py:16-19`).
4. Card multiset `Counter((owner, db.base_def_id(def_id)))` of non-tokens equals the initial one (message uses `Counter`
   subtraction reprs).
5. If no winner: a pending decision with ≥1 option; and if `phase is not SETUP and not st.frames`, no BATTLE/BASE card
   with `damage >= hp_of` unless it has rule `CANT_BE_DESTROYED`.

---

## 7. AI tests

### 7.1 `tests/ai/test_baseline.py` — the strongest byte-identity check that already exists

```
GAME_SEED = 4242; AGENT_SEED = 17
RECORDED_DIGEST = "5cff789d2a8bb26cfef4db98b1ef1ef8f37c3bf63210114781d357bd215f3b96"
agents[seat] = make_agent("mcts", seed=17+seat, log=True, config=BASELINE_PRESETS["standard"], weights=BASELINE_WEIGHTS)
st = new_game(DECKS, 4242); loop while st.pending: p = pending.player; action = agents[p].choose(st, p); actions.append(action.to_json()); apply
payload = {"actions": actions, "logs": [agent.decision_log() for agent in agents], "winner": st.winner, "turn": st.turn}
digest = sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()
assert (st.winner, st.turn, len(actions)) == (1, 20, 115) and digest == RECORDED_DIGEST
```

(`test_baseline.py:20-51`). `json.dumps` here uses default separators `", "`/`": "`, `ensure_ascii=True`, keys sorted at
every level. Each log entry (`ai/base.py:79-99`): `turn`, `player`, `decision` (kind value), `chosen` (`[kind, a, b,
c]`), `label` (`describe(...)` from the chooser's view), `alternatives`: the top `LOG_ALTERNATIVES = 5` of
`sorted(alternatives, key=lambda x: (-x.visits, -x.value))` (stable), each `{"action", "label", "value": round(value, 4),
"visits"}`. The Rust MCTS must reproduce labels (`card_label`/`describe`, `ai/base.py:103-...`), Python `round(x, 4)`, and
Python float `repr` inside `json.dumps`. AI-deck card names are all ASCII (checked), so no `\uXXXX` escapes occur in
this payload, but labels in other positions can contain e.g. `Zaku Ⅱ` (GD01-035), which `ensure_ascii=True` escapes.

### 7.2 Information-set invariance (`test_information_set*.py`, parts of `test_root_moves.py`, `test_search_options.py`)

`midgame_states()` (`test_information_set.py:27-43`, `@cache`d, shared objects): `GAME_SEEDS = (101, 202, 303, 404, 505)`,
`STATES_PER_GAME = 5`; per seed: agents `greedy(seed)`, `greedy(seed+1)`; `st = new_game(DECKS, seed)`; while pending and
taken < 5: if `st.turn >= 3 and len(options) > 1`: if `contested % 4 == 0` append `st.clone()` (taken += 1);
`contested += 1`; then apply the pending player's greedy choice. → 25 states (contested indices 0, 4, 8, 12, 16 per game).
`PERMUTATION_SEEDS = (1, 2, 3)`, `AGENT_SEEDS = (17, 29)`; for state `index` the agent seed is `AGENT_SEEDS[index % 2]`.
`_decision(kind, seed, st)` = `(action, decision_log()[-1]["alternatives"])` of a fresh `make_agent(kind, seed=seed,
log=True)`; for each permutation seed, `_decision(kind, seed, permute_hidden(st, p, s))` must equal the original
(move AND logged statistics). Negative control monkeypatches `mcts.determinize` and `noop.determinize` to return
`st.clone()` and requires ≥ half of the states to change.

Variants: `test_information_set_policy.py` (all policy options on), `test_information_set_weights.py` (`ALTERNATIVES =
("roles",)`, mcts+greedy), `test_root_moves.py:500-508` (all five root settings), `test_search_options.py:233-241`
(`target_extension=3.0, observation_keys=True` on 25 midgame states + the 2 `PUZZLES` positions in dict order + 1
`opponent_may_hold_a_trick()` = 28 cases).

### 7.3 Puzzles (every puzzle)

Common: `Scenario()` defaults (active 0, turn 3, seed 0, deck size 20, filler `GD01-060` Zaku Mariner 2/2 vanilla); P0 is
the side under test unless noted. In `test_puzzles.py` and `test_puzzles_shipped.py`, `agents(seed) = (make_agent("mcts",
seed=seed), make_agent("mcts", seed=seed+100))` (preset `standard` = all 2026-10 settings on, `DEFAULT_WEIGHTS`), seeds
`SEEDS = (1, 2)`. `play_turns(st, players, turns=1)` (`test_puzzles.py:30-35`) applies `players[p].choose` until
`st.turn >= start_turn + turns` or no decision is pending. `choice(st, agent)` = one `agent.choose(st, pending.player)`.

`tests/ai/test_puzzles.py` (18 functions × 2 seeds = 36):

| # | test (line) | position | assertion |
| --- | --- | --- | --- |
| 1 | `test_lethal_attack_the_exposed_player` (47) | P0 GM ST01-005 2/2; P1 Strike Dagger ST04-005 rested, 0 shields; P0 2 filler shields | after 1 turn `winner == 0` |
| 2 | `test_lethal_goes_face_instead_of_taking_a_free_kill` (58) | P0 2× GM; P1 Demi Trainer ST01-008 (1/1 Blocker) rested, 1 shield; P0 3 shields | `winner == 0` |
| 3 | `test_lethal_rests_the_blocker_before_attacking` (71) | P0 3 resources, GM, hand Siege Ploy ST02-014 (rest 1 enemy Unit ≤5 HP); P1 active Demi Trainer, 0 shields; P0 3 shields | `winner == 0` |
| 4 | `test_lethal_pairs_the_linking_pilot_to_attack_on_deploy_turn` (84) | P0 4 resources, Gundam ST01-001 deployed this turn (link Amuro Ray), hand Amuro Ray ST01-010; P1 empty, 0 shields; P0 3 shields | `winner == 0` |
| 5 | `test_pairing_chooses_the_pilot_and_unit_that_link` (96) | P0 4 resources (3 rested), GM, Gundam ST01-001 deployed this turn, hand Amuro Ray + Suletta Mercury ST01-011 (no link); P1 1 shield; P0 3 shields | `cards[amuro].pair == gundam` and `winner == 0` |
| 6 | `test_burst_aware_attack_order` (111) | P0 Big Zam GD01-027 5/6, Strike Dagger 3/2; P1 one shield ST02-014, deck 20× ST02-014 (+20 filler under it); P0 3 shields | `winner == 0` |
| 7 | `test_holds_removal_for_next_turns_lethal` (130) | P0 4 resources (3 rested), Gundam GD01-013 deployed this turn, hand Siege Ploy; P1 Demi Trainer, 2 resources; P0 4 shields | after 1 turn Siege Ploy still in HAND; after 2 more turn boundaries `winner == 0` |
| 8 | `test_takes_the_favourable_trade` (152) | P0 Gundam GD01-013 3/4; P1 Strike Dagger rested; P0 2 shields, P1 5 | after 1 turn dagger in TRASH, gundam in BATTLE |
| 9 | `test_does_not_attack_into_a_bigger_blocker` (165) | P0 GM; P1 Aile Strike Gundam ST04-001 4/4 Blocker; 6 shields each | GM still in BATTLE after 1 turn |
| 10 | `test_blocks_to_survive` (177) | `Scenario(active=1)`; P1 GM attacks (testkit `attack`); P0 Demi Trainer, 0 shields; P1 2 shields | P0 agent (`agents(seed)[0]`) returns `BLOCK(blocker)` |
| 11 | `test_blocks_when_the_blocker_wins_the_fight` (188) | active=1; P1 Strike Dagger attacks; P0 Launcher Strike GD01-072 3/4 Blocker; 5 shields each | `BLOCK(blocker)` |
| 12 | `test_uses_an_action_step_trick_to_survive` (200) | active=1; P1 GM attacks P0 (0 shields, no blocker); P0 3 resources, hand Unforeseen Incident ST01-014 (AP-3); pending must be ACTION_STEP | after `play_turns` 1 turn `winner is None` |
| 13 | `test_removal_targets_the_most_valuable_unit_it_can_destroy` (220) | P0 2 resources, plays Close Combat ST03-013 (2 damage); P1 Aile Strike 4/4 with 2 damage, Strike Dagger 3/2, Moebius Zero ST04-003 2/4; pending SELECT | `SELECT(aile)` |
| 14 | `test_excess_trashes_the_weakest_unit` (234) | P0 2 resources, 5× Gundam GD01-013 + Demi Trainer in battle, plays Strike Dagger; pending EXCESS | `SELECT(weak)` (the Demi Trainer) |
| 15 | `test_takes_a_free_card_from_burst` (248) | `situations.burst()`: P0 GM attacks into P1 shield Amuro Ray, `pass_all` → BURST for P1 | `agents(seed)[1]` returns `YES` |
| 16 | `test_chooses_to_play_first_with_proactive_decks` (254) | `new_game(DECKS, 5)` | chooser's agent returns `GO_FIRST(chooser)` |
| 17 | `test_mulligans_a_hand_of_uncastable_cards` (305) | `_find_opening(4, 5)`: first seed in `range(2000)` where, after `new_game((MULLIGAN_DECK,)*2, seed)` + `options[0]`, the pending player's hand holds 4-5 HEAVY cards | `agents(seed)[0]` returns `REDRAW` |
| 18 | `test_keeps_a_hand_with_early_plays` (311) | `_find_opening(0, 0)` | `KEEP` |

`MULLIGAN_DECK` (`:265-281`): CHEAP ×4 = ST01-005, ST01-008, ST02-009, GD01-008, GD01-011, GD01-021, GD01-022, EB01-051,
GD01-079, GD01-085 (in this order) + HEAVY ×2 = GD01-065, EB01-001, EB01-002, EB01-004, EB01-005; resources 10× R-001.

`tests/ai/test_puzzles_shipped.py` (12), same agents/seeds:

| test | position builder | assertion |
| --- | --- | --- |
| `test_rests_the_blocker_while_a_bigger_unit_is_already_rested[seed]` | `test_root_moves.rests_the_blocker(rested_decoy=True)` (`test_root_moves.py:86-99`): P0 3 resources, GM, hand Siege Ploy; P1 Demi Trainer + rested Strike Dagger, deck 28 ZEON cycle, 0 shields; P0 3 shields | after 1 turn `winner == 0` |
| `test_rests_the_active_unit_not_the_rested_one[seed-hidden]` (hidden ∈ {0, 4}) | `amuro_rests(extra_units=("ST01-005",) if hidden else (), hidden=hidden)` (`test_root_moves.py:216-238`): P0 4 resources, host Gundam GD01-013, extras, Amuro in hand; P1 Schuzrum-Galluss GD04-040 rested, Zaku Ⅱ GD01-035 active, 4 resources, `hidden` hand cards `ZEON[(2i)%7]` known=False, deck `ZEON[i%7]` for i<28; P0 5 filler shields; P1 shields `ZEON[(3i)%7]` i<5; Amuro paired onto host → SELECT | `agents(seed)[0].choose(st, 0) == SELECT(zaku)` |
| `test_rests_the_active_unit_with_a_hidden_hand[seed]` | `test_puzzles_targets.rest_the_active_unit()` | chosen == `SELECT(zaku)` |
| `test_protects_the_attacked_unit[seed-attacked_is_big]` | `test_policy_knobs._protection(attacked_is_big)` (`:131-146`): active=1; P0 4 resources, hand The Blue Giant ST03-014, GM rested, Gundam GD01-013 rested (2 damage if big); P1 GM attacks the target | play until `st.battle is None`; target still in BATTLE |

`ZEON = ("ST03-008","GD01-059","GD01-035","GD01-030","GD01-031","ST03-013","GD01-115")`.

`tests/ai/test_puzzles_targets.py` (4): `PUZZLES` (insertion order matters, `:89-93`) `{name: (builder, seeds, required)}`:
`rest_the_active_unit` (16, 14) and `damage_the_unit_that_cannot_repair` (24, 16).
- `rest_the_active_unit` (`:46-65`): as `amuro_rests` with hidden=4 but P0 also has GM; P1 hand `ZEON[(2i)%7]` i<4
  known=False; want `SELECT(zaku)`.
- `damage_the_unit_that_cannot_repair` (`:68-86`): P0 4 resources, Barbatos 2nd Form ST05-002, Mikazuki Augus ST05-010 in
  hand; P1 Guncannon GD01-004 rested (Repair 1), GM rested, 4 resources, hand `FEDERATION[(3i+1)%14]` i<5 known=False, deck
  `FEDERATION[i%14]` i<28, shields `FEDERATION[(5i)%14]` i<5 (list at `:26-41`); pair Mikazuki → SELECT; want `SELECT(gm)`.
- `passes(build, seeds, config)` (`:96-104`): one `build()`; for seed in `1..seeds`:
  `make_agent("mcts", seed=seed, config=config, weights=BASELINE_WEIGHTS).choose(start.clone(), player) == want`.
- `test_close_target_calls_get_more_iterations[name]`: `passes(EXTENDED) >= required` with
  `EXTENDED = replace(BASELINE_PRESETS["standard"], target_extension=3.0)`; negative control: `passes(BASELINE) < required`.

Related setting-level positions (not "puzzles" by name but same style): `test_root_moves.py` (`holds_removal`,
`zero_ap_scenario`, `age_device`, `identical_attackers`, Freedom Gundam / G-Falcon DX / Ribo Colony / Doritea guards,
seed sweep `SEEDS = range(31001, 31009)`, `NOOP_SEED = 31999`), `test_policy_knobs.py` (Moment of Rest, Blue Giant,
zero-AP face attack), `test_search_options.py` (`kill_the_damaged_unit`, `opponent_may_hold_a_trick`).

### 7.4 Slow AI strength (`tests/slow/test_ai_strength.py`)

`GAMES = 400`, `MCTS = AgentSpec("mcts", "standard")`, decks `tests.ai.decks.DECKS`.
`play_all(mirrored_specs(400, seed, MCTS, opponent, DECKS), workers)` (`ai/selfplay.py:88-118`): game `i` →
`block, variant = divmod(i, 4)`; `swap_decks, challenger_seat = divmod(variant, 2)`; game seed
`derive_seed(seed, "block", block, "decks", swap_decks)`; agent seeds `derive_seed(seed, "agent", i, 0/1)`; workers >1 use a
`spawn` pool, `chunksize=1`. Each game: `new_game`, loop `while pending and decisions < DECISION_LIMIT (20000)`;
`winner = -1` if none. Win rate counts draws as non-wins.
- `test_default_preset_beats_random`: seed 7001, `rate >= 0.90`, both challenger seats present, ≥400 outcomes.
- `test_default_preset_beats_greedy`: seed 7002, Wilson 95% lower bound `> 0.50`.
Printed line: `"\nmcts-standard vs {kind}: {wins} wins, {draws} draws in {n} games = {rate:.3f} (Wilson 95% [{lo:.3f}, {hi:.3f}]), mean turns {turns:.1f}"`.

`tests/ai/test_throughput.py` (slow): `measure("standard", games=2, workers=2, seed=3)`, CLI JSON, `measure("fast", 1, 1, 3)`;
asserts only positive speeds (wall clock).

---

## 8. Runner, report, CLI and deck tests (summary of what they pin)

- `tests/runner/test_replay.py`: `run_benchmark(BenchmarkConfig(tekkadan, federation, matches=40, seed=2024))` ≥80 games
  each replay (`replay(g, config)`) to `state_digest == g.final_state_sha256`, same turns/winner/end reason; slow variant
  over all 6 ordered example-deck pairs, `matches=60, seed=i, fmt="bo3"`; record JSON round-trip; replay-document tamper
  messages: `"not a gcg-sim-replay v1 document"`, `"malformed replay document: KeyError"`, `"action 1 attack(999,999)"`,
  `"more actions than the game had"`, `"malformed replay document"`, `"winner_seat: expected..."`,
  `"turns: expected {t+1}, replay gave {t}"`.
- `tests/runner/test_digest.py`: renumbering registry tables in reverse leaves `canonical_state`/`state_digest`
  unchanged while `to_json()` differs.
- `tests/runner/test_config.py:20-23`: `game_seed(7,3,1) == derive_seed(7,"match",3,"game",1)`,
  `agent_seed(7,3,1,0) == derive_seed(7,"match",3,"game",1,"agent",0)`; `dut_seat(m)` = `[0,1,0,1]` for m 0..3.
- `tests/cli/test_console_script.py`: `benchmark TEKKADAN FEDERATION --matches 12 --seed 77` with `--workers 1` vs `2`:
  `results.json`, `games.ndjson`, `summary.md` and every `replays/*` byte-identical; `timing.json` must differ; same with
  `PYTHONHASHSEED=1` vs `2`; each saved replay verifies in a fresh process with last stdout line
  `"replay matches the recorded outcome and final state"`; Ctrl-C: first stderr line starts `"matches 1/100000"`, exit
  130, stderr contains `"interrupted: workers stopped, no reports written"`, empty stdout, no `results.json`.
- `tests/reports/*`, `tests/deck/*`, `tests/cli/test_cli.py`: exact user-facing messages, exit codes, schema validity;
  see `01-rng-runner-cli.md` §3.9 and `02-reports.md`, `03-data-deck.md` for the strings.

---

## 9. Testkit API (`src/gcg_sim/testkit.py`)

Constants: `A = ActionKind`; `FILLER_UNIT = "GD01-060"` (Zaku Mariner, vanilla Lv2 cost 1, 2/2); `FILLER_RESOURCE =
"R-001"` (`testkit.py:30-32`). `__all__ = ["NO_ARG", "PLAYER_TARGET", "Scenario", "Zone"]` (`:439`) but tests import the
helpers by name. Most-imported names (files): `Scenario` 38, `play` 35, `attack` 32, `pass_all`/`zone_of`/`has_action`/
`to_next_turn` 28, `ap` 26, `keywords` 25, `yes` 24, `no`/`select` 23, `activate`/`act`/`block` 21, `options` 19, `hp` 18,
`end_main` 14, `order` 10, `pass_` 9, `numbers_in` 7, `select_if_asked` 6, `card_numbers` 5, `card_text`/`uids_in`/
`yes_if_asked`/`vanilla_deck` 4, `choose_option` 3, `legal_kinds` 2, `arrange_if_asked` 1. `Scenario(` appears 1748× in
`tests/cards`, 608× in `tests/rules`, 58× in `tests/ai`.

### 9.1 `Scenario` (builds a mid-game state, bypassing setup)

`__init__(*, active=0, turn=3, seed=0, deck_size=20)` (`:38-49`): `st = GameState(seed)` (rng `SplitMix64(seed)`);
`st.turn = turn`; `st.active = active`; `st.first_player = active if turn % 2 == 1 else 1 - active`;
`_decks = [[], []]`, `_custom_deck = [False, False]`, `_started = False`.

`add(player, number, zone=Zone.BATTLE, *, rested=False, damage=0, pilot=None, deployed_this_turn=False, known=True) -> uid`
(`:52-82`), in this order:
1. `cdef = db[number]`; `get_registry().require(cdef.def_id)` (raises `UnimplementedCardError` for scriptless cards).
2. `uid = core.new_card(st, def_id, player, zone, rested=rested)` — `uid = len(st.cards)`, card appended, `known =
   BOTH_KNOW (0b11)`, `entered_turn = st.turn`, uid **appended** to `st.zones[player][zone]`, `st.touch()`
   (`core.py:119-131`).
3. `c.damage = damage`; `c.entered_turn = st.turn if deployed_this_turn else st.turn - 2`.
4. `c.known = BOTH_KNOW if known or zone.is_public else (1 << player)`; if `zone is HAND`: `BOTH_KNOW if known else
   (1 << player)` (so hand cards are **known to both players by default**).
5. If `pilot`: `p = self.add(player, pilot, Zone.PAIRED)` (recursive; pilot uid = next uid), `cards[p].pair = uid`,
   `c.pair = p`.
6. `st.touch()`; if already started, `_record_decklists()`.
Each `add` therefore bumps `st.version` by 2 (+2 for a pilot). `zone_seq` stays 0 for scenario cards.

| Method | Behaviour |
| --- | --- |
| `add_raw(player, number, zone)` (`:84-91`) | `core.new_card` only (no `require`, no damage/known/entered_turn changes: `known=BOTH_KNOW`, `entered_turn=st.turn`); re-records decklists if started |
| `hand(player, *numbers)` / `trash(player, *numbers)` | `add` each into HAND / TRASH, returns uids |
| `shields(player, *numbers)` (`:99-106`) | `add(..., Zone.SHIELD, known=False)` then `known = 0`; first number = first appended (zone index 0 = top shield per docstring) |
| `deck(player, *numbers)` (`:108-111`) | stores the list (first = top) and sets the custom flag |
| `resources(player, n, *, rested=0, ex=0)` (`:113-119`) | `n`× R-001 into RESOURCE_AREA, the first `rested` of them rested; then `ex`× `db.ex_resource` (EXR-001, a token) active |
| `base(player, number=None, *, damage=0)` (`:121-123`) | `add` into BASE; default `db.ex_base` (EXB-001, a token) |
| `resource_deck(player, n)` (`:125-127`) | `n`× R-001 into RESOURCE_DECK with `known=False` → `known = 1 << player` (owner-known; NOT reset to 0, unlike shields and deck) |
| `_build_decks()` (`:130-142`) | for p in (0, 1): `numbers = list(_decks[p])`; filler count = `max(0, deck_size - len(numbers))` if not custom else **`deck_size`** (a custom deck gets a full `deck_size` of filler appended below it); create each with `core.new_card(..., Zone.DECK)` and `known = 0` (entered_turn = `st.turn`); then `_record_decklists()` |
| `_record_decklists()` (`:144-158`) | over `st.cards` in uid order, skip tokens; RESOURCE type → `res[owner]`, everything else (Units, Pilots, Commands, Bases) → `decks[owner]`; `st.decklists = (tuple(sorted(decks[0])), tuple(sorted(decks[1])))` (def ids), same for `resource_decklists` |
| `start(step=Step.MAIN)` (`:160-182`) | `_build_decks()`; `_started = True`; `st.phase` from `{ACTIVE_STEP: START, START_STEP: START, DRAW_STEP: DRAW, RESOURCE_STEP: RESOURCE, MAIN: MAIN, END_ACTION: END, END_STEP: END, HAND_STEP: END}[step]` (other steps → `KeyError`); `st.step = step`; for `END_ACTION`: `priority = standby`, `passes = 0`; `st.touch()`; `advance(st)`; returns `st` |

Verified sample (`Scenario(); add(0,"ST01-005"); add(1,"ST04-005",rested=True); shields(0, filler, filler); start()`):
uids 0, 1, [2, 3]; deck uids 4..23 (P0) and 24..43 (P1); card 0 json `[0, 913, 0, 3, 0, 0, -1, 0, 1, 3]`; shield
`[2, 208, 0, 4, 0, 0, -1, 0, 1, 0]`; deck card `[4, 208, 0, 0, 0, 0, -1, 0, 3, 0]`; `turn 3, active 0, first_player 0,
phase 4, step 40`; pending `{"player":0,"kind":"main","options":[["attack",0,-2,-1],["attack",0,1,-1],["end_main",-1,-1,-1]],"prompt":"main phase","context":[]}`;
`rng.state 0`; `version 49` (8 from four adds + 40 deck cards + 1 start touch); history empty.

### 9.2 Module functions

| Function | Exact behaviour |
| --- | --- |
| `vanilla_deck(color="Red", resource="R-001")` (`:185-199`) | `units = sorted(numbers of real UNIT cards with is_vanilla and color.value == color)`; `main = tuple(n for n in units for _ in range(4))[:50]`; `ValueError(f"not enough vanilla {color} Units for a legal deck")` if <50; resources `(resource,)*10`. `is_vanilla` = `effect.strip() in ("", "-")` (`cards/model.py:140-141`) |
| `card_text(number, text)` (`:202-217`) | context manager: `compile_card(dataclasses.replace(cdef, effect=text))`; `override_script(reg, cdef, script)` (rebuilds the entry, re-indexes events, `registry.py:209-216`); `core.clear_listen_cache()`; on exit `restore_entry` + `clear_listen_cache()`. New programs/continuous effects interned during the override stay in the registry tables |
| `options(st)` | `st.pending.options if st.pending else ()` |
| `_fail(st, want)` (`:228-234`) | `AssertionError(f"no legal action {want}; pending={kind} options=[{opts}] winner={st.winner}")`, `kind = dec.kind.value` or `"none"`, `opts = ", ".join(str(o))` with `Action.__str__` = `f"{kind.value}({','.join(str(x) for x in (a,b,c) if x != -1)})"` (`state.py:30-32`) |
| `act(st, kind, a=None, b=None, c=None)` (`:237-254`) | applies the **first** option (in `pending.options` order) with `o.kind is kind` and each non-None field equal; else raises `_fail(st, f"{kind.value}(a={a}, b={b}, c={c})")` |
| `has_action(st, kind, a=None, b=None)` | any option matching kind/a/b |
| `play(st, uid, *, onto=None, ex=0)` (`:263-272`) | `onto` → `act(PAIR, uid, onto, ex)`; UNIT → `act(PLAY_UNIT, uid, None, ex)`; BASE → `act(PLAY_BASE, uid, None, ex)`; anything else (Command, or a Pilot without `onto`) → `act(PLAY_COMMAND, uid, None, ex)`. `c == ex` is always required |
| `activate(st, host, ability=None, ex=0)` | `act(ACTIVATE, host, ability, ex)` |
| `attack(st, attacker, target=PLAYER_TARGET)` | `act(ATTACK, attacker, target)` (`PLAYER_TARGET = -2`) |
| `end_main`, `pass_`, `yes`, `no` | `act(END_MAIN)`, `act(PASS)`, `act(YES)`, `act(NO)` |
| `block(st, blocker)` | `None` → `act(NO_BLOCK)`; else `act(BLOCK, blocker)` |
| `yes_if_asked(st)` | `act(YES)` iff pending kind is `YES_NO` |
| `select(st, *uids, done=None)` (`:311-322`) | `act(SELECT, u)` for each; then `act(DONE)` iff `done is not False` and pending and DONE offered and (`done` truthy or pending kind is `SELECT`) |
| `arrange_if_asked(st)` | while pending kind `ARRANGE`: `apply(st, options[0])` |
| `select_if_asked(st, *uids, done=None)` | `offered = {o.a for SELECT options}`; calls `select` iff pending and `set(uids) & offered` |
| `choose_option(st, index)` | `act(SELECT, index)` |
| `order(st, index=0)` | `act(ORDER, options(st)[index].a)` — index into **all** options |
| `pass_all(st, *, max_steps=50)` | up to 50 times: no pending → return; ACTION_STEP → PASS; BLOCK → NO_BLOCK; else return |
| `to_next_turn(st)` (`:363-380`) | if pending MAIN → `end_main`; then up to 200 times: none → return; MAIN → return; ACTION_STEP → PASS; BLOCK → NO_BLOCK; DISCARD → `act(SELECT, options[0].a)`; anything else → raise `_fail(st, "to reach the next main phase")`. Returns silently after 200 iterations |
| `zone_of`, `uids_in`, `numbers_in`, `names`, `card_numbers` | field reads (`uids_in` returns a list copy of the zone list) |
| `ap`, `hp` | `V.ap_of(st, V.derived(st), uid)`, `V.hp_of(...)` |
| `keywords(st, uid)` | `{k.value: v for k, v in V.keywords(V.derived(st), uid).items()}` |
| `legal_kinds(st)` | set of option kinds |
| `expect_illegal(st, action)` | `apply`; returns on `IllegalActionError`; else `AssertionError(f"{action} was accepted but should be illegal")` (dataclass repr of `Action`) |
| `run_random(st, seed, *, max_actions=5000)` | `rng = SplitMix64(seed)`; up to 5000 times: no pending → return; `apply(st, opts[rng.randrange(len(opts))])` |

Tests also read engine internals directly (counts over `tests/cards` / `tests/rules`): `st.cards` 1120/350, `st.zones`
423/291, `st.pending` 319/310, `st.active`, `st.winner`, `st.turn`, `st.end_reason`, `st.battle`, `st.step`, `st.phase`,
`st.history`, `st.batches`, `st.decklists`, `st.lasting`, `st.delayed`, `st.frames`, `st.redraws`; view functions
`V.derived`, `V.cdef`, `V.play_cost`, `V.is_linked`, `V.rules_of`, `V.traits_of`, `V.play_level`, `V.hp_of`, `V.matches`,
`V.level_of`, `V.cost_of`, `V.has_kw`, `V.link_satisfied`, `V.stat`, `V.Ctx`. A few tests mutate state directly between
actions (e.g. `tests/cards/test_wp_eb01_b.py:84`, `test_wp_gd02_a.py:76-77`, `test_wp_gd03_a.py:47-48`,
`test_wp_gd05_b.py:748`, `test_wp_gd05_a.py:448`, `tests/rules/test_effects.py:1192`).

Behaviour-altering monkeypatches inside engine tests: `force_link` fixture (`tests/cards/test_wp_gd05_a.py:101-113`)
replaces `V.link_satisfied` for 4 tests (GD05-035, GD05-044, GD05-069, GD05-068). Observation-only spies on `core.emit`:
`tests/rules/test_turn.py:148-166` (`events` fixture recording `(ev, phase, step, turn, active, hand size, rested uids)`),
`tests/rules/test_zones.py:521-536`, `tests/cards/test_wp_gd03_b.py:1798-1808`, `tests/cards/test_wp_gd04_b.py:709-719`.
Test-only card text via `card_text`/`override_script` (10 test functions): `tests/rules/test_turn.py:848,870,935,1181,1251`
(through the `card_text` fixture at `:169-182`), `tests/rules/test_damage.py:791`, `tests/ai/test_policy_knobs.py:167`,
`tests/ai/test_root_moves.py:188` (`card_text`) and `:312,379` (`command_script` builds a `d.CardScript` directly and calls
`override_script`, `:363-373`). (`test_effects.py:261` and `test_cardinfo.py:302` only have "card_text" in their names.)

---

## 10. How the Rust port should cover each category

Principle: the Python suite is the specification of behaviour; the Rust port must match Python **bit for bit**, so
the most economical coverage of the 2,900 engine tests is a differential oracle generated by running the Python suite,
not a hand translation of 45k lines of assertions. Port by hand only what checks cross-language artefacts (formats,
digests, CLI) or needs Rust-side harnesses.

| Category (Python tests) | Recommendation | Export needed from Python |
| --- | --- | --- |
| Engine rules `tests/rules` (647) and card behaviour `tests/cards` (2245) | **Differential traces** (§10.1) replayed by a Rust harness. Port only `tests/rules/test_lead_examples.py` and `tests/cards/test_lead_examples.py` (12 tests) natively to validate the Rust testkit port (uid order, versions, filler decks). | Per-test trace: scenario state before `advance`, every `apply` (state before/after, pending, action), probe results, overrides, expected exceptions. |
| Tests using `card_text`/`override_script` (10, §9.2) | Differential traces that carry the override script in golden form; Rust must load scripts from JSON (or port the compiler). | `node_to_json` of the overriding `CardScript` per override. |
| `force_link` tests (4, GD05-035/044/068/069) | Rust needs a test-only hook equivalent to patching `link_satisfied`, or mark these traces "patched" and replay with the hook. | Flag + list of forced unit numbers. |
| Golden (27) | If Rust consumes compiled IR: load every `tests/effects/golden/*.json`, deserialize all nodes, re-serialize with the §5.2 rules and compare bytes. If Rust ports the compiler: produce identical files. | none (files exist); keep `uv run python -m gcg_sim.tools.golden --check` as the Python gate. |
| Coverage meta (`test_rules_coverage`, `test_faq_ruling_coverage`, `test_card_coverage`) | Keep in Python (they gate Python tags). Rust-side replacement: "every Python test id has a replayed trace and every trace passes"; plus a vanilla/implemented check over the Rust registry. | test-id list with tags (`marks.scan`) joined to trace files. |
| `test_no_placeholders` | Rust analog over `crates/` (§4.1). | none |
| Traceability doc | Keep generated from Python tags; optionally add a Rust column (trace replay outcome per test id). | junit from Rust trace replay keyed `basename::function`. |
| Engine caches / paranoid (`test_engine_caches.py`, `GCG_SIM_PARANOID_VIEW`) | Port as a Rust debug feature (`paranoid`) asserting cached view == fresh compute, unlistened events have no candidates, skipped rules checks change nothing; run on fuzz 16 games seed 11 + 2 greedy games (TUNING_DECKS, seeds 1, 2 / +10). | none (counts are Python-internal; Rust need not match the counter values). |
| Fuzz / robustness (`test_robustness.py`, `tools/fuzz.py`) | Port `fuzz` exactly (§6). Differential: Python per-game `(index, seed, cards, actions list, end, turns, final state_digest)` for e.g. seed 2026 games 0-499 and seed 5 games 0-39; Rust must reproduce each action list (random agent consumes RNG only through `randrange(len(options))`, so equality of the action stream implies equal option counts and order). Then Rust runs 10,000 games natively and compares the summary JSON bytes with `python -m gcg_sim.tools.fuzz --games 10000 --seed 2026`. | NDJSON per game; the summary JSON. |
| Invariants | Port `invariants.check` (§6.5) and run it after every action in fuzz and in trace replay. | none |
| `test_baseline.py` | **Port directly** (single test; the recorded digest is the acceptance check for the Rust baseline MCTS). | none (constant in the test); for debugging export the payload JSON itself. |
| Puzzles (`test_puzzles*.py`, 52) | Port directly on the Rust testkit (assertions are cheap) **and** compare full decision logs: export, for every `agent.choose` call in these tests, the decision log entry with **all** alternatives (not truncated to 5) and values as float bit patterns. | Per-call `{state digest, player, chosen, alternatives[all]}`. |
| Information-set tests (128 + 25 + 28) | Port directly once `permute_hidden`/`information_set_key` are ported; additionally export the 25 `midgame_states` (canonical JSON) and the expected `(action, alternatives)` per agent seed so a Rust mismatch is attributable. | canonical states + decisions. |
| AI internals (`test_root_moves`, `test_policy_knobs`, `test_search_options`, `test_agents` pruning tests) | Differential on function outputs per scenario: `canonical(st, dec)` keys/actions, `prune_dominated(...)` per flag set, `noop.without_effect(...)` key sets (seed 31999 / 3), `policy.option_scores` (float bits), `policy.choose(st, None, 0.0, knobs)`, `targeting._polarity_at` per pool site, `mcts.search` result. Port the negative-control assertions only after outputs match. | per-scenario JSON of those outputs. |
| `test_tuning.py`, `test_evaluation_roles.py` | Port; compare fitted weights as exact float bits against Python exports (logistic fit and linear solve are float-order sensitive). | weights/scores as bit patterns. |
| Slow AI strength (2) | Covered transitively: once Rust MCTS is byte-identical, the 400-game outcomes are identical. Gate: export Python `GameOutcome` lists for seeds 7001/7002 (index, challenger, winner, turns, first_player, decisions) and compare the first N games in CI, all 800 occasionally. | outcome NDJSON. |
| `test_throughput.py` (3, slow) | Rust benchmark (hyperfine/criterion), not a test. | none |
| Runner/report/CLI (`tests/runner`, `tests/reports`, `tests/cli`) | Port as Rust tests (messages, exit codes, schema, round trips) and add byte-for-byte comparisons of `results.json`, `games.ndjson`, `summary.md`, `replays/*` against Python CLI output for fixed seeds (e.g. `benchmark TEKKADAN FEDERATION --matches 12 --seed 77`). Rust needs a built-in equivalent of `tests/runner/random_agent.py` (`test-random`, `test-illegal`), since Python factories are loaded by file path. | Python CLI output trees for the fixed invocations. |
| Deck parse/validate (`tests/deck`) | Port (user-facing messages); differential over example decks and the synthetic invalid decks built by `make_deck`. | `(deck text, DeckError message / validation messages, digest)` triples. |
| `tests/rules_index`, `tests/sources`, `tests/tools` | Keep Python-only (data pipeline); Rust consumes the generated data files. | none |
| Hypothesis tests (5 functions) | Rust `proptest` equivalents for the properties; for parity, export a fixed sample of inputs/outputs from Python (Hypothesis inputs differ per run). | sampled vectors. |

### 10.1 Proposed per-test differential trace export (Python side)

A pytest plugin (e.g. under `port/oracle/`, run with `-p` in one process per test file and without `-n`, so registry
numbering is reproducible) that records, per test node id, one JSON object per line:

1. `scenario`: patch `gcg_sim.testkit.advance` to capture `canonical_state(st)` immediately **before** `advance` in
   `Scenario.start`, then the state after. Also capture `new_game` calls (decks, seed).
2. `apply`: wrap `gcg_sim.engine.game.apply` **and every module-level alias** of it (`testkit.apply`, test modules,
   `ai.*`, `runner.*` import it by name — patch each `sys.modules[m].apply is original`), but record only calls made
   outside AI search (e.g. skip while inside `Agent.choose`). Record: `before` (`canonical_state` when it differs from the
   previous `after`, otherwise only its digest — this captures direct test mutations and late `Scenario.add` calls),
   `pending.to_json()`, `action.to_json()`, `after` digest, exception type/message if `apply` raised
   (`IllegalActionError` messages are part of behaviour).
3. `view`: after each apply, a JSON form of `tests.paranoid.view_signature(V.derived(st))` (enum → value, dict keys
   stringified and sorted) so derived AP/HP/keywords/rules/costs are compared even when tests only assert them through
   `ap()`/`hp()`/`keywords()`.
4. `probe`: return values of testkit query helpers and the `V.*` functions called from test modules (name, args, result).
5. `override`: `node_to_json` of scripts installed by `card_text`/`override_script`, with enter/exit points.
6. `events` (optional): the `core.emit` sequence `(ev.value, subject, player, extra)` for tests that spy on events.

Rust harness: for each trace, rebuild the scenario state from the exported canonical JSON (mapping content ids back to
its own registry entries), check `pending` options equal (same order), apply the action, compare `canonical_state`
bytes (including `version`, `rng`, `history`, `effect_hits`), compare the view signature, and evaluate probes.

### 10.2 Exports that already exist

`state_digest`/`canonical_state` (`runner/digest.py:36-56`), `GameState.to_json`/`from_json` (`engine/state.py:575-644`),
`Decision.to_json`, `Action.to_json`, game records and replay documents (`runner/records.py`, `runner/replayfile.py`),
decision logs (`ai/base.py:79-99`), golden files, the fuzz summary. `port/oracle/export_vectors.py` (untracked, written by
another workstream) already exports RNG/float/json vectors to `crates/gcg-core/tests/vectors.json`.

---

## 11. Hazards (Python semantics in this area, with locations)

1. **Static tag scanner ignores non-literal arguments**: `pytest.mark.card(n)` inside `pytest.param` list comprehensions
   (`tests/cards/test_wp_gd02_b.py:95,122`, `tests/cards/test_wp_gd03_b.py:128,169`) records nothing (`marks.py:36-47`);
   those cards are covered by other literal tags today. A Rust re-implementation of the gate must keep the same rule or the
   counts in §0 change.
2. **Tag attribution**: every mark call anywhere inside a test function or class is attributed to the outermost
   definition, and any module-level assignment containing a mark call is recorded as `"<module>"` (`marks.py:63-95`).
3. **Traceability keys are `basename::function`** (`traceability.py:29-30,44-45`); `tests/cards/test_lead_examples.py`
   and `tests/rules/test_lead_examples.py` share a basename (no function name collides today). Parametrization is stripped
   and a failure is sticky (`:28,34-35`).
4. **Three different JSON formats**: golden `json.dumps(indent=1, sort_keys=True, ensure_ascii=False)+"\n"`
   (`tools/golden.py:36`); baseline digest `json.dumps(payload, sort_keys=True)` with default `", "`/`": "` separators and
   `ensure_ascii=True` (`tests/ai/test_baseline.py:49`); state digest `sort_keys=True, separators=(",", ":")`, ASCII
   escapes (`runner/digest.py:55`); fuzz summary `indent=1` without `sort_keys` (insertion order, `fuzz.py:150-159`).
5. **`node_to_json` omits fields equal to their default using Python `==`** (`effects/golden.py:27`): `False == 0`,
   `True == 1`, `0.0 == 0`, `() == ()`; a field whose default is `0` holding `False` (or vice versa) is omitted. Dict nodes
   are sorted by the original key objects before `str(k)` (`:36`).
6. **`Counter` insertion order** decides the key order of `"ends"` in the fuzz summary (`fuzz.py:135,153`): first
   end reason seen in index order comes first.
7. **Sorting by enum value, not declaration order**: `sorted(pool, key=lambda c: c.value)` → Blue, Green, Purple, Red,
   White (`fuzz.py:50`) whereas the pool dict is built in declaration order Blue, Green, Red, White, Purple
   (`fuzz.py:35`, `cards/model.py:40-45`).
8. **RNG consumption in `deck_for`**: exactly one `randrange(4)` per number appended in the first loop; the length check
   happens before drawing (`fuzz.py:58-61`); the fill loop uses no RNG. `play_one` uses its own RNG stream only for
   `randrange(len(options))` (`fuzz.py:97`), so option count and order must match.
9. **Exceptions as control flow**: fuzz catches every exception and turns it into a failure record including
   `traceback.format_exc(limit=8)` (`fuzz.py:111-118`); a Rust port must return errors (not panic) to produce the same
   `ok=False` records; the trace text cannot match and should be excluded from comparisons.
10. **Turn-limit end is a fuzz failure** (`fuzz.py:107-110`), and `play_one` has no own action cap (relies on the
    engine's `turn_limit`/`max_actions`).
11. **First matching option wins** in `testkit.act` (`testkit.py:245-253`) and fuzz/random agents index into
    `pending.options`: legal-option **order** is behaviour.
12. **uid allocation and `version` counting in the testkit**: uids are `len(st.cards)` at creation (`core.py:122`);
    pilots get the uid after their unit; deck cards are created at `start()` after all `add` calls, P0 before P1
    (`testkit.py:130-142`). Every `new_card` and every `add` calls `touch()` (`version += 1`, `state.py:529-531`), and
    `version` is part of `to_json` and therefore of `state_digest` (`state.py:610`, `runner/digest.py:39`).
13. **Testkit visibility defaults**: hand cards default to known-to-both (`testkit.py:72-74`); `resource_deck` cards are
    owner-known (`1 << player`), not unknown (`:125-127`); shields and deck cards are `known = 0` (`:104,141`).
14. **Custom decks get a full `deck_size` filler** below them, not a top-up (`testkit.py:133-138`).
15. **`to_next_turn` returns silently after 200 iterations**, `pass_all` after 50 (`testkit.py:348-380`); `order(index)`
    indexes all options, not only ORDER options (`:344-345`); `play` always constrains `c == ex` (`:263-272`).
16. **Process-global registry with lazy, first-use numbering** (`effects/registry.py:115-138,401-403`); `card_text`
    and `override_script` append new programs that are never removed (`registry.py:209-216`). Raw `to_json()` program /
    effect / filter indices therefore depend on test order and xdist distribution; compare `canonical_state`
    (content ids = `sha256(repr(entry))[:16]`, `runner/digest.py:26-27`), which itself depends on Python dataclass
    `repr` of DSL nodes (see `01-rng-runner-cli.md` §2.9).
17. **Module-level `@cache` state shared across tests**: `midgame_states()` returns the same `GameState` objects to
    every test that uses it (`tests/ai/test_information_set.py:27`); `DATA = build()` and parametrize lists run at
    collection (`tests/effects/test_golden.py:12`, `test_card_coverage.py:52-55`).
18. **Name-bound imports defeat monkeypatching**: `testkit` does `from gcg_sim.engine.game import ... apply`
    (`testkit.py:26`), so patching `gcg_sim.engine.game.apply` alone would not intercept testkit actions (matters for the
    §10.1 exporter). The existing spies patch `core.emit`, which engine modules call through the `core` module.
19. **Hypothesis inputs are random per run** (no derandomize profile; `.hypothesis/` DB is local), see §1.
20. **Stable sorts with tuple keys**: decision-log ranking `sorted(alternatives, key=lambda x: (-x.visits, -x.value))`
    keeps original order on ties (`ai/base.py:84`); Rust `sort_by` is stable too, but `-x.value` on floats must use the
    same total order as Python's `<` for the values that occur (no NaN expected).
21. **`round(x, 4)`** (correctly rounded, half-to-even on the exact binary value) in every logged value
    (`ai/base.py:95`), then float `repr` in JSON.
22. **`bool` arithmetic**: `wins = sum(o.winner == o.challenger ...)` (`tests/slow/test_ai_strength.py:31-32`),
    `solved += agent.choose(...) == want` (`tests/ai/test_puzzles_targets.py:103`), `won += st.winner == 0`
    (`tests/ai/test_root_moves.py:126`).
23. **Dict insertion order of test tables**: `PUZZLES` order defines `_target_positions()` and the parametrized index
    range in `test_search_options.py:42-43,229-234`; `SETTING_SCENARIOS` order defines test ids
    (`test_root_moves.py:474-483`).
24. **Process start methods**: fuzz uses the default `ProcessPoolExecutor` context (`fuzz.py:128`, spawn on macOS),
    selfplay forces `spawn` (`ai/selfplay.py:117`); both preserve index order through `map`.
25. **Invariant messages** embed `Counter` subtraction reprs (`engine/invariants.py:93`) and enum `.name`s; they appear
    only in failures.
26. **`slow` is not deselected by default** (`pyproject.toml:55`): the documented `uv run pytest -n 8` includes the
    10k-game fuzz and 800 MCTS games.
27. **No floor-division/modulo with negative operands** in this area: all of `fuzz.py:52-55,69` and
    `ai/selfplay.py:100-101` operate on non-negative ints (Rust `/` and `%` agree for these).

---

## 12. Open questions

1. Trace volume: the number of `apply` calls made by the 2,900 engine tests was not measured (tests were not run), so
   the size of the §10.1 export and its runtime are unknown.
2. Whether the Rust port re-implements the text compiler or loads compiled scripts from Python-exported JSON; this
   decides how `card_text` tests and golden files are covered.
3. The exact JUnit `classname` produced under `--import-mode=importlib` was not observed (no test run); traceability
   assumes `tests.rules.test_turn` → basename `test_turn.py`.
4. Wall-clock cost of the AI tests (puzzles, information-set suites, strength tests) under Python was not measured; this
   affects how often Python-side exports can be regenerated.
5. `view_signature` field types (`dv.ap`, `dv.hp`, `dv.kw`, ... dict vs list, key types) need to be fixed by the view
   inventory (`05-engine-state-view.md`) before a canonical JSON form for differential export can be specified.
6. The ordering semantics of `Decision.options` within each decision kind (needed because the testkit and random agents
   pick by position) are documented in the engine inventories, not here; this file assumes them.
7. Whether `docs/port/tests.json` (named in `docs/port/progress.md` as the machine-readable test status) should key on
   pytest node ids (with parametrization ids such as `[0-mcts]`, `[EB01]`) or on `basename::function` as traceability
   does; it does not exist yet.
