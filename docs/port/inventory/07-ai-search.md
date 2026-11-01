# 07 — AI search: the reference ISMCTS, root pruning, pre-game choices, agents

Scope: `src/gcg_sim/ai/{mcts,config,baseline,actions,noop,setup,agents,base,targeting}.py` and
`src/gcg_sim/runner/agents.py`. Where those files call into `policy.py`, `playout.py`,
`evaluation.py`, `rng.py`, or the engine (`engine/observe.py`, `engine/view.py`, `engine/game.py`),
this file documents only the call contract: what is passed, in what order, and what is read back.
Line numbers are for branch `rust-port` at `d9d41af`, Python 3.15.1.

Notation: `mcts.py:122` means `src/gcg_sim/ai/mcts.py` line 122. `runner/agents.py` means
`src/gcg_sim/runner/agents.py`. A "u64" is a Python int in `[0, 2^64)`.

---

## 1. Agents and per-decision seeding

### 1.1 `make_agent` (agents.py:104-127)

```
make_agent(kind, *, preset="standard", seed=0, log=False, config=None, weights=None)
```

1. `preset` is checked first, for every kind (agents.py:117-118). The error is
   `ValueError("unknown AI preset 'x'; choose from ['fast', 'standard', 'strong']")`. The list is
   `sorted(PRESETS)` and is printed with Python list repr.
2. `chosen_weights = DEFAULT_WEIGHTS if weights is None else weights`. `DEFAULT_WEIGHTS` is
   `ROLE_WEIGHTS`, feature set `"roles"` (evaluation.py:107-129).
3. `"random"` → `RandomAgent(seed, log=log)`. `"greedy"` → `GreedyAgent(seed, log, weights)`.
   `"mcts"` → `MctsAgent(seed, config=PRESETS[preset] if config is None else config, log, weights)`.
4. Any other kind raises
   `ValueError("unknown agent kind 'foo'; choose from ['random', 'greedy', 'mcts']")`. The list is
   `list(AGENT_KINDS)`, and `AGENT_KINDS = ("random", "greedy", "mcts")` (agents.py:101).

Agent `name` (this string is in the seed path): `"random"`, `"greedy"`, or `f"mcts-{config.name}"`
(agents.py:26, 45, 81). **The baseline configs reuse the names `"standard"` and `"strong"`**
(baseline.py:21, 47), so `BASELINE_PRESETS["standard"]` is also named `"mcts-standard"`.
`GameRecord.agents` records these names (runner/game.py:147).

### 1.2 `AgentCore` (base.py:43-76)

State: `name`, `seed` (int, as given), `_log_enabled`, `_entries` (list of dicts), `_decisions`
(int, starts at 0).

`_begin(st, player)` (base.py:60-65):
1. `dec = st.pending`. If `dec is None or dec.player != player`, raise
   `ValueError(f"{self.name}: no decision pending for player {player}")`.
2. `self._decisions += 1`. The counter goes up for every call, including decisions with a
   single option. The runner calls `choose` for every pending decision of the seat
   (runner/game.py:87-91).
3. Return `(dec, SplitMix64(derive_seed(self.seed, self.name, "decision", self._decisions)))`.

`derive_seed(master, *path)` (rng.py:52-59) is
`int.from_bytes(sha256(repr((master & MASK64, *path)).encode()).digest()[:8], "big")`. The
material is the Python tuple repr: `(17, 'mcts-standard', 'decision', 1)`. That is a decimal int,
`', '` separators, and strings in single quotes. For the agent names in use, the encoding is
plain ASCII.

Test vectors (computed with `uv run python`):

| call | value |
|---|---|
| `derive_seed(1, 'match', 0, 'game', 0, 'agent', 0)` (runner agent seed, runner/config.py:79-80) | `17449794008030232776` |
| `derive_seed(17449794008030232776, 'mcts-standard', 'decision', 1)` | `17683307912657752582` |
| same, decision 2 / 3 | `18101817166437973065` / `374240451198152903` |
| `derive_seed(17, 'mcts-standard', 'decision', 1)` | `3302900377033519914` |
| `SplitMix64(17683307912657752582).next_u64()` ×3 | `7365148325066467191, 16197301673173457939, 15328836568995161108` |
| `SplitMix64(0).next_u64()` ×2 | `16294208416658607535, 7960286522194355700` |
| `SplitMix64(0).random()` ×2 | `0.8833108082136426, 0.43152799704850997` |
| `derive_seed(-1,'x') == derive_seed(2**64-1,'x')` | both `15212809081249037171` (master is masked) |

`_finish(st, player, chosen, alternatives)` (base.py:67-76) appends `log_entry(...)` only when
logging is on. It reads `st` before the action is applied. It returns `chosen`.

### 1.3 `MctsAgent.choose` (agents.py:85-98): dispatch order

```
dec, rng = _begin(st, player)
if len(dec.options) == 1:            -> finish(dec.options[0], [])          # no RNG use
if dec.kind is REDRAW:               -> mulligan_choice(st, player, config.mulligan_samples, rng)
if dec.kind is CHOOSE_FIRST:         -> first_player_choice(st, player, config.first_player_games,
                                                            rng, knobs_for(config))
else                                 -> search(st, player, config, weights, rng)
finish(result.action, result.alternatives)
```

### 1.4 `GreedyAgent.choose` (agents.py:48-66)

```
dec, rng = _begin(st, player)
if len(options)==1: finish(options[0], [])
if REDRAW: mulligan_choice(st, player, GREEDY_MULLIGAN_SAMPLES=48, rng)
if CHOOSE_FIRST: finish(Action(GO_FIRST, player), [])            # always goes first, no RNG
det = determinize(st, player, rng.next_u64())                    # 1 draw
density = burst_density(st, player)                              # from the TRUE state
for _, action in prune_dominated(st, dec, canonical(st, dec)):   # default flags: strict_attacks=False, rest_targets=False
    s = det.clone(); apply(s, action, check=False)
    play_until_turn(s, st.turn + 1, None, 0.0, GREEDY_MAX_DECISIONS=200)   # BASELINE_KNOBS
    scored.append(Alternative(action, evaluate(s, player, weights, density), 1))
best = first index of max(scored[i].value)                       # max(range, key) → FIRST max
```
Greedy uses no `without_effect` and no `merge_equivalent`.

### 1.5 `RandomAgent.choose` (agents.py:28-32)

`chosen = dec.options[rng.randrange(len(dec.options))]`. It draws even when there is one option:
`randrange(1)` uses one `next_u64`. Alternatives are
`[Alternative(o, 1.0/len(options), 0) for o in options]`.

### 1.6 `runner/agents.py`

- `AI_MODULE = "gcg_sim.ai"`, `DEFAULT_AGENT_KIND = "mcts"` (lines 24-25).
- `AgentSpec(role: str, seat: int, preset: str, seed: int, log: bool)`, frozen, in this field
  order (lines 28-36). The runner fills it with `seed=agent_seed(master, m, g, seat)` and
  `log=config.decision_log` (runner/game.py:66-75).
- `default_agent_factory(spec)` (lines 56-63) imports `gcg_sim.ai` lazily. If that fails it raises
  `AgentUnavailableError("the AI package 'gcg_sim.ai' is not available")`. It returns
  `ai.make_agent("mcts", preset=spec.preset, seed=spec.seed, log=spec.log)`, with default weights
  `ROLE_WEIGHTS`. `role` and `seat` are not used.
- `resolve_factory(ref)` (lines 80-93) is `functools.cache`d. It splits on the last `:`. Errors:
  `ValueError(f"agent factory must look like 'module:attr' or 'file.py:attr': {ref!r}")` and
  `ValueError(f"agent factory {ref!r} is not a callable attribute")`. A target ending in `.py` is
  loaded as module `_gcg_sim_agent_factory_{sha256(str(resolved_path)).hexdigest()[:12]}`
  (lines 66-77). Otherwise it uses `importlib.import_module`.
- `FactoryRef(ref)` is a picklable callable that resolves once per process (lines 96-103).
- `describe_factory(factory)` (lines 106-114) goes into results.json `"factory"`
  (reports/results.py:125):
  - the default factory gives `gcg_sim.ai.make_agent('mcts')` (verified);
  - `FactoryRef('/a/b/f.py:make')` gives `f.py:make`, and `FactoryRef('pkg.mod:make')` gives
    `pkg.mod:make`;
  - anything else gives `f"{factory.__module__}.{factory.__qualname__}"`, with `?` / the type's
    qualname as fallbacks.

---

## 2. `SearchConfig` (config.py:13-57) and presets

Constants: `WIN_DECAY = 0.01`, `LEAF_FLOOR = 0.05` (config.py:9-10); `TIE_TOLERANCE = 1e-9`
(mcts.py:41).

Field declaration order. This is also dataclass `__eq__`/`__hash__` order. `knobs_for` is
`@cache`d on it (policy.py:63), which is pure memoisation.

| # | field | type | default | consumer |
|---|---|---|---|---|
| 1 | name | str | — | agent name `mcts-{name}` (agents.py:81) → seed path |
| 2 | max_iterations | int | — | `budget` |
| 3 | min_iterations | int | — | `budget` |
| 4 | iterations_per_option | int | — | `budget` |
| 5 | exploration | float | 0.3 | UCB `c` (mcts.py:290) |
| 6 | prior_weight | float | 0.5 | UCB `w` (mcts.py:291) |
| 7 | prior_temperature | float | 1.5 | prior softmax (mcts.py:314) |
| 8 | horizon_turns | int | 2 | `ctx.horizon` (mcts.py:119) |
| 9 | rollout_temperature | float | 0.5 | rollout softmax (mcts.py:212) |
| 10 | value_scale | float | 2.0 | leaf log-odds divisor (mcts.py:263) |
| 11 | max_rollout_decisions | int | 250 | rollout move cap (mcts.py:212) |
| 12 | first_player_games | int | 24 | play/draw sims (agents.py:94) |
| 13 | mulligan_samples | int | 96 | mulligan samples (agents.py:90) |
| 14 | policy_burst_yes_score | float | 1.0 | `PolicyKnobs.burst_yes_score` (policy.py:66) |
| 15 | noop_branch_choices | bool | False | `without_effect(branch_choices=)` (mcts.py:104) |
| 16 | noop_resource_aware | bool | False | `without_effect(resource_aware=)` (mcts.py:105) |
| 17 | strict_attack_prune | bool | False | `prune_dominated(strict_attacks=)` (mcts.py:93) |
| 18 | prune_noop_rest_targets | bool | False | `prune_dominated(rest_targets=)` (mcts.py:94) |
| 19 | merge_equivalent_moves | bool | False | `merge_equivalent` at root (mcts.py:109) |
| 20 | observation_keys | bool | False | child keys + pass handling (mcts.py:233, 268) |
| 21 | target_extension | float | 1.0 | `_iteration_cap` (mcts.py:178) |
| 22 | target_extension_z | float | 2.0 | `_close_call` z (mcts.py:148) |
| 23 | policy_rule_polarity_table | bool | False | `PolicyKnobs.rule_polarity_table` (policy.py:67) |
| 24 | policy_zero_ap_face | bool | False | `PolicyKnobs.zero_ap_face` (policy.py:68) |

`__post_init__` (config.py:48-53) raises `ValueError` when `WIN_DECAY * horizon_turns >= LEAF_FLOOR`
(float compare; `0.01*5 >= 0.05` is True, `0.01*4` is False, so `horizon_turns <= 4` is valid). The
message is
`f"horizon_turns={h} is too long: a win at the horizon would not outrank every evaluated position (WIN_DECAY * horizon_turns < LEAF_FLOOR)"`.

`budget(n_options)` (config.py:55-57) is
`max(min_iterations, min(max_iterations, iterations_per_option * n_options))`. Integer
arithmetic; `min_iterations` wins if it exceeds `max_iterations`.

`_adopted(config)` (config.py:60-77) is `dataclasses.replace` with: `value_scale=1.0`,
`policy_burst_yes_score=4.0`, every `noop_*`/`*_prune`/`merge_*`/`observation_keys`/`policy_*`
bool set to True, and `target_extension=3.0`.

### 2.1 Every field of every shipped config (printed from the live objects)

`PRESETS` is `config.py:80-111`; `BASELINE_PRESETS` is `baseline.py:18-73`, and
`BASELINE_WEIGHTS = TUNED_WEIGHTS` (baseline.py:74). `tuning` is `TUNING_CONFIG`
(tuning.py:50-57), included for completeness.

| field | fast | standard | strong | baseline standard | baseline strong | tuning |
|---|---|---|---|---|---|---|
| name | 'fast' | 'standard' | 'strong' | 'standard' | 'strong' | 'tuning' |
| max_iterations | 64 | 640 | 1280 | 320 | 1280 | 48 |
| min_iterations | 16 | 160 | 320 | 80 | 320 | 16 |
| iterations_per_option | 7 | 64 | 128 | 32 | 128 | 6 |
| exploration | 0.15 | 0.3 | 0.15 | 0.3 | 0.15 | 0.3 |
| prior_weight | 0.5 | 0.5 | 0.5 | 0.5 | 0.5 | 0.5 |
| prior_temperature | 1.5 | 1.5 | 1.5 | 1.5 | 1.5 | 1.5 |
| horizon_turns | 2 | 2 | 2 | 2 | 2 | 2 |
| rollout_temperature | 0.3 | 0.5 | 0.3 | 0.5 | 0.3 | 0.5 |
| value_scale | 1.0 | 1.0 | 1.0 | 2.0 | 2.0 | 2.0 |
| max_rollout_decisions | 250 | 250 | 250 | 250 | 250 | 250 |
| first_player_games | 8 | 24 | 48 | 24 | 48 | 8 |
| mulligan_samples | 96 | 96 | 192 | 96 | 192 | 32 |
| policy_burst_yes_score | 4.0 | 4.0 | 4.0 | 1.0 | 1.0 | 1.0 |
| noop_branch_choices | True | True | True | False | False | False |
| noop_resource_aware | True | True | True | False | False | False |
| strict_attack_prune | True | True | True | False | False | False |
| prune_noop_rest_targets | True | True | True | False | False | False |
| merge_equivalent_moves | True | True | True | False | False | False |
| observation_keys | True | True | True | False | False | False |
| target_extension | 3.0 | 3.0 | 3.0 | 1.0 | 1.0 | 1.0 |
| target_extension_z | 2.0 | 2.0 | 2.0 | 2.0 | 2.0 | 2.0 |
| policy_rule_polarity_table | True | True | True | False | False | False |
| policy_zero_ap_face | True | True | True | False | False | False |

Budgets for n = 1, 2, 3, 5, 10, 20, 30 root moves: fast `16,16,21,35,64,64,64`; standard
`160,160,192,320,640,640,640`; strong `320,320,384,640,1280,1280,1280`; baseline-standard
`80,80,96,160,320,320,320`; baseline-strong is the same as strong.

`PolicyKnobs` (policy.py:50-60) has fields `burst_yes_score: float = 1.0`,
`rule_polarity_table: bool = False`, `zero_ap_face: bool = False`. `BASELINE_KNOBS = PolicyKnobs()`.
`knobs_for(config)` maps fields 14, 23 and 24. The search passes `knobs_for(cfg)` everywhere
(rollout, priors, noop, play/draw). The mulligan uses no knobs. Greedy and the default arguments of
`play_until_turn`/`without_effect` use `BASELINE_KNOBS`.

---

## 3. `search()` step by step (mcts.py:82-151)

Inputs are `st` (the TRUE state; it is never mutated), `player`, `config`, `weights`, and `rng`
(the per-decision `SplitMix64`). `dec = st.pending`. Both `assert dec is not None` and
`assert dec.player == player` hold.

### 3.1 Root moves (mcts.py:89-112)

```
root_moves = prune_dominated(st, dec, canonical(st, dec),
                             strict_attacks=cfg.strict_attack_prune,
                             rest_targets=cfg.prune_noop_rest_targets)          # §8, §10
if any(a.kind in {PLAY_COMMAND, ACTIVATE} for _, a in root_moves):              # SPENDING
    seed = rng.next_u64()                                                         # RNG DRAW #0 (conditional)
    idle = without_effect(st, player, dec, root_moves, seed, knobs_for(cfg),
                          branch_choices=cfg.noop_branch_choices,
                          resource_aware=cfg.noop_resource_aware, weights=weights)  # §11
    root_moves = [m for m in root_moves if m.key not in idle] or root_moves      # never empty
if cfg.merge_equivalent_moves:
    root_moves = merge_equivalent(st, dec, root_moves)                          # §9
if len(root_moves) == 1:
    return SearchResult(root_moves[0].action, [], 0)                            # no more draws
```
- The noop seed is drawn whenever a spending move survives pruning. That includes decisions
  where `without_effect` returns `set()` straight away (kind not MAIN/ACTION_STEP), and the case
  where only one root move exists.
- With a single remaining move, the agent plays the **canonical representative** (fewest EX),
  not `dec.options[0]`. The logged alternatives are `[]`.
- Every root-move function sees the TRUE `st`.

### 3.2 Context (mcts.py:113-120)

`_Search(player, config, weights, density=burst_density(st, player), root_turn=st.turn,
horizon=st.turn + config.horizon_turns)`. Density is computed once, from the true state.

### 3.3 Budget, seeds, cap: RNG draw order (mcts.py:121-126)

```
budget = config.budget(len(root_moves))
W = [rng.next_u64() for _ in range(budget)]          # world seeds 0..budget-1
R = [rng.next_u64() for _ in range(budget)]          # rollout seeds 0..budget-1
cap = _iteration_cap(config, dec, budget)            # §7
W += [rng.next_u64() for _ in range(cap - budget)]   # world seeds budget..cap-1
R += [rng.next_u64() for _ in range(cap - budget)]   # rollout seeds budget..cap-1
```
So the order is **W[0:B], R[0:B], W[B:cap], R[B:cap]**. All seeds are drawn up front, whether or
not the search stops early. Total draws per search = `[1 if noop seed] + 2*cap`. This was checked
on 262 fast-preset searches across 4 games: the rng state after the search equalled
`s0 + draws*0x9E3779B97F4A7C15 mod 2^64` in every case. Because the extra seeds come after the
first `2B`, an extended search is bit-identical to the unextended one until the extension starts
(tests/ai/test_search_options.py:78-95).

### 3.4 World cache and determinisation reuse (mcts.py:127-129, 154-162)

`worlds = {0: determinize(st, player, W[0])}`; `first_world = worlds[0]`.
`_world(worlds, st, player, W, k)` returns a **clone** of `worlds[k]`, creating it lazily with
`determinize(st, player, W[k])` on first use. A base world is never mutated. Determinisation is
pure in `(st, player, seed)`, so lazy versus eager creation does not change results. Inside
`determinize` (engine/observe.py:56-82), hidden identities are shuffled with `SplitMix64(seed)`
and the copy's engine RNG becomes `SplitMix64(derive_seed(seed, "hidden-rng"))`. Every clone of
world k starts with that same engine RNG state.

### 3.5 Root children (mcts.py:130-134)

`_add_children(root, first_world, dec, root_moves, config)` (§4.3) sets root priors **once**,
computed in `first_world` (a determinisation, not the true state) over the **reduced** option
tuple. `keys = [_child_key(dec, key, config) for key in root_moves]`.
`children = [root.children[k] for k in keys]`.
`slot = {id(child): i}` is an id()-based lookup only; use the index in Rust.
`per_visit = [[] for each child]`.

### 3.6 Main loop (mcts.py:135-151): exact control flow

```
limit = budget; done = 0
while done < limit:
    child, action, fresh = _select(root, first_world, dec, root_moves, config)   # §4.4
    k = child.visits                                    # BEFORE this visit: the visit number, 0-based
    world = _world(worlds, st, player, W, k)            # clone of world k
    apply(world, action, check=False)                   # root move applied here
    before = child.total
    _iterate(ctx, child, fresh, world, SplitMix64(R[k]))   # §4.5-§6
    if cap > budget:
        per_visit[slot(child)].append(child.total - before)   # FLOAT DIFFERENCE, see hazards
    done += 1
    if done == limit or _settled(root, keys, limit - done):
        if limit == cap or not _close_call(children, per_visit, config.target_extension_z):
            break
        limit = min(cap, limit + budget)
return _result(root, keys, root_moves, done)
```
- **Common random numbers.** The k-th visit (0-based) of *every* root move is played in world k
  with rollout seed `R[k]`, so k never exceeds `done < cap`.
- `_settled` is evaluated only when `done != limit` (short-circuit `or`).
- When `cap == budget` (every non-target decision, and every baseline decision), `limit == cap`
  always holds, so the loop breaks at the first settled/limit check and `_close_call` is never
  run.
- An extension can also start early: if `_settled` fires before `limit` is reached and the
  position is a close call, `limit` still grows by `budget`.

`_settled(root, keys, remaining)` (mcts.py:320-328):
1. `visits = sorted([child.visits for each key present], reverse=True)`.
2. If fewer than `len(keys)` keys are present, return False. This cannot happen at the root.
3. Return `visits[0] - visits[1] > remaining` (strict).

### 3.7 Final choice (mcts.py:331-339)

`stats[i] = Alternative(action_i, child_i.mean(), child_i.visits)`, in root-move order. Here
`mean() = total/visits if visits else 0.0` (mcts.py:56-57). `best` is the **first** index that
maximises `(visits, mean)` as a tuple: most visits, then higher mean, then root-move order.
`SearchResult(stats[best].action, stats, done)`. `iterations` is not logged.

---

## 4. Tree

### 4.1 Data (mcts.py:47-62)

`_Child(mover: int, prior: float, visits: int = 0, total: float = 0.0, avail: int = 0,
node: _Node | None = None)`. `_Node(children: dict[ChildKey, _Child])`. The dict is only used for
lookups and is never iterated, so a HashMap is fine.

### 4.2 Child keys (mcts.py:267-268)

`ChildKey = (dec.player, dec.kind.value if cfg.observation_keys else "", move_key)`, where
`move_key` is the 4-tuple from `actions.action_key` (§8) computed **in the state the decision
is in** (the true state at the root, the walked world below it). With `observation_keys=False`,
a card's main-phase and action-step plays share one child.

### 4.3 `_add_children(node, s, dec, moves, cfg)` (mcts.py:305-317): priors

```
scores = policy.option_scores(s, Decision(dec.player, dec.kind, tuple(a for _, a in moves),
                                          dec.prompt, dec.context), knobs_for(cfg))
top = max(scores)
w_i = math.exp((x_i - top) / cfg.prior_temperature)       # in move order
total = sum(w)                                            # CPython sum(): Neumaier, see §12
for (key, _), w_i in zip(moves, w): node.children.setdefault(ChildKey, _Child(dec.player, w_i / total))
```
- The scorer receives only the canonical (and at the root, pruned and merged) options. This can
  change the scores; for example `_board.attackers` counts distinct attackers among the passed
  options (policy.py:147-153). Decision kinds without a scorer (`ORDER_TRIGGER`, `PAYMENT`) score
  all zeros, which gives a uniform prior of `1.0/n` (policy.py:104-108).
- `setdefault`: a child that already exists **keeps its old prior**. Priors of new children are
  normalised over all moves legal in *this* world.

### 4.4 `_select(node, s, dec, moves, cfg)` (mcts.py:271-302): UCB1 + progressive prior

```
keys = [ChildKey(dec, key) for key, _ in moves]
if any(k not in node.children for k in keys): _add_children(node, s, dec, moves, cfg)
avail = [(children[k], action) for k, (_, action) in zip(keys, moves)]   # move order
for child in avail: child.avail += 1                                      # BEFORE scoring
unvisited = [c for c in avail if c.visits == 0]
if unvisited: return FIRST argmax prior among unvisited, fresh=True
ucb(ch) = ch.mean() + c * math.sqrt(math.log(ch.avail) / ch.visits) + w * ch.prior / (ch.visits + 1)
return FIRST argmax ucb over avail, fresh=False
```
- `c = cfg.exploration`, `w = cfg.prior_weight`. Evaluation order:
  `((mean + (c * sqrt(log(float(avail)) / float(visits)))) + ((w * prior) / float(visits + 1)))`.
- **Untried order**: the root priors are fixed, so the first `min(n, budget)` root iterations try
  root moves once each, in descending prior. Equal priors are tried in root-move order. Below the
  root the same rule applies among the unvisited children legal in the current world.
- **Avail counts**: at the root every child's `avail` goes up every iteration, so
  `avail == iteration number (1-based)` for all root children. Below the root only the children
  of moves legal in the current world go up, which is how the bandit stays fair when a move
  exists only in some worlds.

### 4.5 `_iterate` and `_descend` (mcts.py:205-235): one new child per iteration

```
_iterate(ctx, first, fresh, s, rng):
    path = [first]
    if not fresh: _descend(ctx, _node_of(first), s, path)       # _node_of creates node lazily
    play_until_turn(s, ctx.horizon, rng, cfg.rollout_temperature, cfg.max_rollout_decisions, knobs_for(cfg))
    value = _leaf_value(s, ctx)
    for child in path:                                           # path order; duplicates possible
        child.visits += 1
        child.total += value if child.mover == ctx.player else 1.0 - value

_descend(ctx, node, s, path):
    while s.winner is None and s.turn < ctx.horizon and s.pending is not None:
        dec = s.pending
        moves = canonical(s, dec)                 # no pruning/merging below the root
        if len(moves) == 1: apply(s, moves[0].action, check=False); continue   # no node, no RNG
        child, action, fresh = _select(node, s, dec, moves, ctx.config)
        apply(s, action, check=False)
        path.append(child)
        if fresh: return                          # the one new child of this iteration
        if ctx.config.observation_keys and _unobserved_pass(dec, action, ctx.player):
            continue                              # stay in the SAME node
        node = _node_of(child)
```
- `_unobserved_pass(dec, action, player)` is
  `dec.player != player and dec.kind is ACTION_STEP and action.kind is PASS` (mcts.py:238-245).
- The descent can end without a fresh child (horizon reached or game over). Then that iteration
  adds no child.
- The tree walk does **not** consume the rollout RNG. Only the rollout uses it. Moves applied
  during the walk do not count towards `max_rollout_decisions`.
- With `observation_keys`, the same `_Child` can appear twice in `path` if the opponent passes an
  action step twice without a searcher node in between. Each occurrence is backed up. I did not
  observe this in 3 instrumented fast games (6385 descents, max path length 5), but the code
  allows it.

---

## 5. Rollout

`play_until_turn(st, turn, rng, temperature, max_decisions, knobs)` (playout.py:12-26) runs
`while st.winner is None and st.turn < turn and n < max_decisions and st.pending is not None:
apply(st, policy.choose(st, rng, temperature, knobs), check=False); n += 1`.

**Leaf detection**: the rollout stops at the first state with `winner` set, or `turn >= root_turn
+ horizon_turns` (the first pending decision of turn T+2, usually the turn player's first main
decision; any earlier start/draw-phase decision of that turn also stops it), or after 250 rollout
moves (the state is then evaluated wherever it is).

`policy.choose(st, rng, T, knobs)` (policy.py:72-83) returns `dec.options[0]` straight away when
there is one option, **with no RNG draw**. Otherwise it calls
`pick(dec.options, option_scores(st, dec, knobs), rng, T)` over the **full** `dec.options`.

`pick` (policy.py:86-101) uses the Gumbel-max trick, which is an exact softmax(score/T) sample:
```
if rng is None or T <= 0.0: return options[FIRST argmax scores]
best_i = 0; best_v = -inf
for i, s in enumerate(scores):                 # one draw per option, in option order
    u = rng.random() or 1e-300                 # random() = (next_u64() >> 11) * 2**-53 ; exact 0.0 → 1e-300
    v = s / T - math.log(-math.log(u))
    if v > best_v: best_i, best_v = i, v       # strict: first max wins ties
return options[best_i]
```
Exactly `len(options)` draws happen per sampled decision. The engine's own randomness (shuffles
during the rollout) comes from the world's `st.rng`, not from this generator.

---

## 6. Leaf value (mcts.py:254-264)

```
if s.winner is not None:
    elapsed = WIN_DECAY * (s.turn - ctx.root_turn)          # 0.01 * float(int)
    if s.winner == player:       return 1.0 - elapsed
    return elapsed if s.winner == 1 - player else 0.5       # winner == -1 (draw) → 0.5
z = score(s, player, weights, density) / config.value_scale
return LEAF_FLOOR + (1.0 - 2.0 * LEAF_FLOOR) * logistic(z)  # 0.05 + 0.9 * σ(z); (1.0-2.0*0.05) == 0.9 bit-exact (0x1.ccccccccccccdp-1)
```
`logistic(z)` (evaluation.py:303-307) is `1/(1+exp(-z))` if `z >= 0`, otherwise
`e=exp(z); e/(1+e)`. `score` (evaluation.py:310-317) is `sum(w*x ...)` over the feature vector (a
CPython float `sum`, so Neumaier; see §12). For `"roles"` weights it is negated when
`player != st.active`. The evaluation inventory owns the features.

Backup: a child whose `mover == player` adds `value`, any other child adds `1.0 - value`.

---

## 7. Target extension (mcts.py:165-202)

`is_target_decision(dec)` (mcts.py:165-169) is
`dec.kind in {SELECT, EXCESS, DISCARD, ARRANGE} and uid_argument(dec) and dec.ctx("rest_sub") < 0`.
`ctx` returns `NO_ARG = -1` when the key is missing.

`uid_argument(dec)` (base.py:125-131) is True for EXCESS and DISCARD. For ARRANGE it is
`ctx("order") == 1`, for SELECT it is `ctx("mode") != 1`, and otherwise it is False.

`_iteration_cap` (mcts.py:172-178) returns `budget` if the decision is not a target decision.
Otherwise it returns `max(budget, round(budget * target_extension))`, where `round` is Python
`round(float)` → int, ties to even. With 3.0 the product is exact; with 1.0, `cap == budget`.

`_close_call(children, per_visit, z)` (mcts.py:181-202):
```
leader = FIRST argmax over i of (children[i].visits, children[i].mean())     # same rule as the final choice
L = per_visit[leader]
for i, O in enumerate(per_visit):                        # root-move order
    if i == leader or not O: continue                    # never-visited moves are ignored
    n = min(len(L), len(O))
    if n < 2: return True
    d = [L[j] - O[j] for j in range(n)]                  # pairs: visit j of both moves shared world j and seed R[j]
    if max(abs(x) for x in d) <= 1e-9: continue          # identical in every shared world: a tie that more iterations would not break
    mean = sum(d) / n                                    # Neumaier sum
    var  = sum((x - mean) ** 2 for x in d) / (n - 1)     # Neumaier sum; ** is libm pow, see hazards
    if mean <= z * math.sqrt(var / n): return True       # within z paired standard errors
return False
```
`per_visit[i][j]` is `child.total_after - child.total_before` for the root child's j-th visit,
not the leaf value itself (see hazards).

---

## 8. Move canonicalisation (actions.py:31-85)

`Key = (kind.value: str, a: int, b: int, variant: int)`. Constants: `_DEF_OFFSET = 1 << 24`;
`ALT_PLAY_BASE = 100` (engine/game.py:32: `c = EX + 100*(modifier index + 1)`);
`NO_ARG = -1`; `PLAYER_TARGET = -2`. The paid kinds are
`{PLAY_UNIT, PLAY_BASE, PAIR, PLAY_COMMAND, ACTIVATE}`, and `_BY_DEFINITION` is
`{HAND, TRASH, DECK, REMOVAL, SHIELD, RESOURCE_DECK}`.

`ex_used(action)` (actions.py:41-45) returns 0 if the kind is not paid or `c < 0`, and
`c % 100` otherwise.

`_ident(st, uid, decider)` (actions.py:48-54) returns `uid` if it is out of `[0, len(cards))`. If
the card's zone is in `_BY_DEFINITION` and `card.known & (1 << decider)` is set, it returns
`(1<<24) + card.def_id`. Otherwise it returns `uid`.

`action_key(st, dec, action)` (actions.py:57-69), with `p = dec.player` (the mover, which can be
the opponent below the root):

| kind | key |
|---|---|
| SELECT | `(v, _ident(a) if uid_argument(dec) else a, b, -1)` |
| PLAY_UNIT / PLAY_BASE / PAIR / PLAY_COMMAND | `(v, _ident(a), _ident(b), c // 100 if c >= 0 else -1)` |
| ACTIVATE | `(v, _ident(a), b, c // 100 if c >= 0 else -1)` |
| ATTACK, BLOCK | `(v, a, b, -1)` |
| everything else | `(v, a, b, c)` |

`canonical(st, dec)` (actions.py:72-85) walks `dec.options` in order. The first time a key appears
it is recorded (key order = first appearance) together with that action. A later action with the
same key replaces the representative only if `ex_used(new) < ex_used(cur)` (strict), so the first
action with the fewest EX wins. It returns `[(key, best[key]) for key in order]`.

---

## 9. `merge_equivalent` and public fingerprints (actions.py:280-393)

`merge_equivalent(st, dec, moves)` (actions.py:379-393) computes `dv = V.derived(st)`, keeps the
first move of each distinct `_public_move` value, and preserves order. The `seen` set is used only
for membership tests.

`_public_move` (actions.py:366-376):
- ATTACK: `(v, fp(a), fp(b))`
- BLOCK: `(v, fp(a), b)`
- SELECT with `uid_argument`: `(v, fp(a), b)`
- PAIR: `(v, key[1], fp(b), key[3])`
- anything else: `key`.

`_fingerprint(st, dv, uid)` (actions.py:334-363):
1. uid outside `[0, len(cards))` gives `("arg", uid)`. This covers `PLAYER_TARGET`.
2. With `pilot = card.pair`: if the zone is not in `{BATTLE, BASE}`, or `_in_flight(uid)`, or
   (`pilot >= 0` and `_in_flight(pilot)`), the result is `("card", uid)`, which never merges.
3. Otherwise the result is the tuple `("field", owner, def_id, zone, rested, damage,
   entered_turn == st.turn, ap_of, hp_of, tuple(sorted((kw.value, n) for kw, n in
   dv.kw.get(uid, {}).items())), tuple(sorted(dv.traits.get(uid, ()))), uid in dv.linked,
   tuple(sorted(a.aid for a in dv.abilities.get(uid, ()))), tuple(sorted((repr(r.rule),
   r.controller, r.source == uid) for r in dv.rules.get(uid, ()))), _references(uid), paired)`.
   Here `paired = (cards[pilot].def_id, _references(pilot))` if `pilot >= 0`, else `()`.

`_in_flight(st, uid)` (actions.py:284-295) is True if any of these name the card:
- a battle with `uid in (attacker, target)`;
- a frame with `uid in (host, card_uid)`, or `uid in f.buf`, or any event value `== uid`, or `uid`
  in any `f.vars` value tuple;
- a trigger in `pending_triggers` plus every batch, with `uid in (host, card_uid)` or any event
  value `== uid`.

`_references(st, uid)` (actions.py:298-331) builds three parts:
- `history`: `(e.kind, e.player, e.by, e.uid == uid, e.source == uid)` for every history event
  with `uid in (e.uid, e.source)`, in history order.
- `once`: `sorted(tuple(_SELF if x == uid else x for x in key) for key in st.once_used if uid in key)`,
  with `_SELF = -(1 << 40)`.
- `lasting`: `sorted((effect_key, controller, duration, created_turn, battle_id, player, uses,
  filters_key, aux, (uid, seq) in targets, source_uid == uid) for le in (*lasting, *delayed)
  if (uid, seq) in le.targets or le.source_uid == uid)`, with `seq = card.zone_seq`.

All fingerprints are only compared for **equality**. The `sorted` calls exist to make the
comparison order-insensitive, so Rust may use any total order, provided element equality matches
Python `==`.

---

## 10. `prune_dominated` (actions.py:242-277)

These are tried in order and the first matching branch decides. The result is `kept or moves`,
so at least one move is always kept.

1. **Free burst**: `dec.kind is BURST and _free_card_burst(st)` keeps only `YES`.
   `_free_card_burst` (actions.py:91-103) requires `st.frames` to be non-empty and
   `uid = frames[-1].card_uid >= 0`. With `entry = reg.cards[cards[uid].def_id]`, it also requires
   `entry.burst_aid >= 0`, and the ability must be a `d.Burst` whose
   `steps == (d.AddToHand(d.ThisCard()),)`.
2. **Rest targets**: this branch applies when `rest_targets and kind is SELECT and uid_argument(dec)
   and _chosen_only_to_rest(st) and not _resting_can_trigger(st) and not
   _resting_changes_more_than_the_unit(st, dec)`. It drops SELECTs on an enemy card in BATTLE that
   is already rested (`_rests_a_rested_enemy`, actions.py:235-239).
   - `_chosen_only_to_rest` (actions.py:170-199) needs `frames[-1].waiting`. If `f.once_key and
     not f.acted` it returns False (this check is not cached). The rest is memoised on
     `(program_id, pc)` in the global `_rest_only`. `ins = instrs[pc]` must be a `d.Choose` with
     `sel.side is ENEMY` and `chooser is P.YOU`. Let `uses` be the pcs after `pc` whose
     instruction `_contains` a `d.Var` named `ins.var`. The result is True when there is exactly
     one use, that instruction is a `d.Rest`, and no `JumpIfNotDid`/`DidLast` appears in
     `instrs[use+1:]`. `_contains` (actions.py:153-160) recurses into tuples and dataclass fields
     in declaration order; it does not look inside lists, dicts or strings.
   - `_resting_can_trigger` (actions.py:205-211) is True if `st.delayed` is non-empty, or if any
     `RESTED` trigger host has its zone in `{BATTLE, BASE, PAIRED}`.
   - `_resting_changes_more_than_the_unit` (actions.py:214-232) uses
     `world = determinize(st, dec.player, 0)` and `before = derived(world)`. For each SELECT option
     on an enemy, BATTLE, active card, it clones the world, sets `rested = True`, calls `touch()`,
     and checks whether `derived(...).same_values(before)` is False. If so it returns True.
3. **Strict attacks**: `kind is MAIN and strict_attacks` runs `_drop_idle_attacks`
   (actions.py:139-150). Take `zero = {key of ATTACK moves with ap_of(st, attacker) <= 0}` from the
   true state. If `zero` is empty, return the moves unchanged. Otherwise use
   `world = determinize(st, player, 0)` and keep a move if its key is not in `zero` or
   `_attack_matters(world, player, a, b)` holds. `_attack_matters` (actions.py:123-133) clones the
   world, records `queued = len(pending_triggers)`, and runs `declare_attack(trial, a, b)`. It is
   True if a newly queued trigger has `controller == player`, otherwise it is
   `bool(action_step_options(trial, player))`.
4. **Loose attacks**: `kind is MAIN` keeps non-attacks, and keeps attacks where
   `_attack_can_matter(st, player, attacker)` holds (actions.py:106-120). That function is True
   when `ap > 0`, or a player-owned host is listed under the `ATTACKS` trigger index, or
   `_activated_list(st, player, ACTION)` is non-empty, or any hand card passes
   `_command_playable(..., ACTION)`.
5. Otherwise the moves are returned unchanged.

The trial determinisations use `_TRIAL_WORLD_SEED = 0` (actions.py:136) and never touch the
search RNG.

---

## 11. `noop.without_effect` (noop.py)

Constants (noop.py:35-39): `SPENDING = {PLAY_COMMAND, ACTIVATE}`,
`TOP_LEVEL = {MAIN, ACTION_STEP}`, `MAX_DECISIONS = 400`, `PASS = Action(PASS, -1, -1, -1)`,
`_SPENT_ZONES = {RESOURCE_AREA, OUTSIDE}`. The internal policy step is
`policy.choose(st, None, 0.0, knobs)`, a deterministic first argmax (noop.py:55-56).

```
without_effect(st, player, dec, moves, seed, knobs, *, branch_choices, resource_aware, weights) -> set[Key]
    spending = [(k, a) for k, a in moves if a.kind in SPENDING]                   # move order
    if dec.kind not in TOP_LEVEL or not spending: return set()
    world = determinize(st, player, seed)
    lines = _Lines(world, player, turn=world.turn + 1, knobs, resource_aware)
    density = None; out = set()
    for key, action in spending:
        played = world.clone(); _resolve(played, action, knobs)
        spent = action.a if action.kind is PLAY_COMMAND else -1
        if lines.idle_line(played, spent) is None: continue                      # has an effect → keep
        if branch_choices:
            density = density or burst_density(st, player)                        # lazily, TRUE state
            if _a_branch_beats_idling(lines, action, spent, weights, density): continue
        out.add(key)
    return out
```
- `_resolving(st)` (noop.py:59-64) is `winner is None and pending is not None and (frames or
  pending.kind not in TOP_LEVEL)`.
- `_resolve(st, a)` (noop.py:67-74) applies `a`, then applies policy steps while `_resolving` and
  `n < 400`.
- `_continue(st, turn)` (noop.py:99-107) applies and records policy steps while
  `winner is None and st.turn < turn and pending is not None and len(actions) < 400`.
- `_replay(st, actions, turn)` (noop.py:110-125) uses `queue = list(actions)` and loops while
  `winner None, turn <, pending, n < 400`, incrementing `n` first each time:
  - if `queue[0] in pending.options` (Action equality), apply `queue.pop(0)`;
  - elif the pending kind is ACTION_STEP and `PASS in options`, apply `PASS`;
  - elif the queue is non-empty, return False;
  - else apply a policy step.

  It returns `not queue`.
- `_Lines.continue_and_replay(line)` (noop.py:179-184) computes
  `actions = _continue(line, turn)`, which mutates `line`. It then takes `idled = world.clone()`
  and returns `idled` if `_replay(idled, actions)` succeeds, else None.
- `_Lines.same(line, idled, spent)` (noop.py:186-189) is `_only_costs_differ(...) and (not
  resource_aware or _resources_only_paid(...))`.
- `idle_line(line, spent)` (noop.py:191-194) returns the idled state, or None if it is None or not
  `same`.
- `_signature(st, spent)` (noop.py:42-52) is the tuple `(winner, turn, active, cards, lasting,
  decks)`, where:
  - `cards = tuple((uid, zone, rested, damage, pair, known) for c in st.cards (uid order) if
    c.uid != spent and c.zone not in _SPENT_ZONES)`;
  - `lasting = tuple((effect_key, targets, duration, player) for le in (*lasting, *delayed))`,
    which is **list order and unsorted**;
  - `decks = (tuple(zones[0][DECK]), tuple(zones[1][DECK]))`.
- `_only_costs_differ(played, idled, spent, player)` (noop.py:213-227): compare both signatures.
  Require `a[:3] == b[:3]`, `a[4:] == b[4:]`, and the same number of card tuples. Then zip the card
  tuples. Equal pairs pass. A differing pair must share `uid` and `zone` and have equal
  `(damage, pair, known)`. The card must belong to `player`, be rested in the played line, and be
  active in the idled line. Anything else returns False.
- `_resources(st, p)` (noop.py:230-232) is `(frozenset(zones[p][RESOURCE_AREA]),
  count of non-rested cards in it)`.
- `_resources_only_paid` (noop.py:235-240) is `mine.set <= mine_idle.set` (a **subset** test),
  `mine.active <= mine_idle.active`, and `theirs == theirs_idle` (set and count).
- `_a_branch_beats_idling` (noop.py:197-210) loops over
  `for line in _branches(world, action, player, knobs)`:
  - `idled = continue_and_replay(line)`;
  - if `idled is None`, take `idled = world.clone()` and run `_continue(idled, turn)`. The idle
    line is then the plain policy from the root, which may itself play the move;
  - elif `same(line, idled, spent)`, skip this branch;
  - if `evaluate(line, player, weights, density) > evaluate(idled, ...)` (strict), return True.

  Return False at the end.
- `_branches(world, action, player, knobs)` (noop.py:77-96) clones the world, applies the action,
  and walks the policy while resolving (at most 400 steps). At the first pending
  `SELECT by player with len(options) > 1` (over the **full** options), it returns one branch per
  option, each cloned and run through `_resolve(branch, option)`. If no such choice comes up it
  returns `[]`.

---

## 12. Float-order and arithmetic rules (byte equivalence)

**CPython `sum()` of floats is Neumaier-compensated.** I confirmed this model against the Python
3.15.1 `sum` on 200,000 random float lists: 0 mismatches, against 94,627 mismatches for naive
`+=`. The rule applies to every `sum()` listed below. Reference algorithm:
```rust
fn py_fsum_builtin(xs: &[f64]) -> f64 {   // CPython builtin sum() over an all-float iterable
    if xs.is_empty() { return 0.0; }        // Python returns int 0 (never reached in scope)
    let mut f = 0.0 + xs[0];                // int 0 + x0  (−0.0 becomes +0.0)
    let mut c = 0.0;
    for &x in &xs[1..] {
        let t = f + x;
        if f.abs() >= x.abs() { c += (f - t) + x } else { c += (x - t) + f }
        f = t;
    }
    if c != 0.0 && c.is_finite() { f += c }
    f
}
```
Uses in scope:
- the prior normaliser `total = sum(weights)` (mcts.py:315);
- `_close_call`'s `sum(diffs)` and `sum((x-mean)**2 ...)` (mcts.py:198-199);
- `first_player_choice`'s `sum(as_first)`, `sum(as_second)`, `sum(diffs)` and the variance sum
  (setup.py:102-109);
- `evaluation.score` (evaluation.py:314, 317), which reaches every leaf value.

The mulligan `total += ...` (setup.py:79) and opening `score += ...` (setup.py:50, 57) are plain
sequential additions.

Other exact formulas, in evaluation order:
- UCB: `(mean + c*sqrt(ln(avail)/visits)) + (w*prior)/(visits+1)` (mcts.py:296-298).
- Prior: `exp((x - top)/T) / total`.
- Rollout: `s/T - ln(-ln(u))`.
- Leaf: `0.05 + 0.9*σ(z)` with `z = score/value_scale`. A win scores `1.0 - 0.01*Δturn`, a loss
  `0.01*Δturn`.
- `_close_call`: `mean <= z*sqrt(var/n)`.
- First player: `se = sqrt(var/len)`, and go second iff `mean < 0 and -mean > 2.0*se`.
- Mulligan: `average = total/samples` (0.0 if `samples == 0`), and keep iff
  `current >= 0.9*average`. The `0.9*average` product is computed twice with the same result.
- Opening score: `(weight*0.5)*(ap+hp)` for Units and `weight*(0.5*(ap+hp) + bonus)` for Pilots,
  with ints promoted to float.
- Integer→float promotion is exact everywhere in scope (small ints).
- `math.log(int)` converts the int to a double and calls libm `log`.

---

## 13. Pre-game choices (setup.py)

Constants (setup.py:22-27): `OPENING_TURNS=4`, `KEEP_RATIO=0.9`, `START_HAND=5`,
`FIRST_PLAYER_TEMPERATURE=0.5`, `MAX_GAME_DECISIONS=3000`, `SIGNIFICANCE=2.0`.

### 13.1 CHOOSE_FIRST: `first_player_choice(st, player, games, rng, knobs)` (setup.py:86-111)

```
first = Action(GO_FIRST, player); second = Action(GO_FIRST, 1 - player)
seeds = [rng.next_u64() for _ in range(games)]                # games draws
as_first  = [_result(st, player, first,  s, knobs) for s in seeds]   # all "first" games, then
as_second = [_result(st, player, second, s, knobs) for s in seeds]   # all "second" games
stats = [Alternative(first, sum(as_first)/max(1,games), games), Alternative(second, sum(as_second)/max(1,games), games)]
diffs = [a - b ...]; if len(diffs) < 2: return first, stats
mean = sum(diffs)/n; var = sum((x-mean)**2)/(n-1); se = sqrt(var/n)
return (second if mean < 0 and -mean > 2.0*se else first), stats
```
`_result(st, player, choice, seed, knobs)` (setup.py:114-120):
1. `det = determinize(st, player, seed)`.
2. `apply(det, choice)` with **check=True**.
3. `play_to_end(det, SplitMix64(seed), 0.5, 3000, knobs)`. The **same seed** is used for the
   determinisation and for the policy RNG.
4. Return 1.0 if `winner == player`, 0.5 if `winner in (-1, None)`, else 0.0.

`play_to_end` (playout.py:29-40) is `play_until_turn` without the turn bound. The config's
`rollout_temperature` is not used here.

### 13.2 REDRAW: `mulligan_choice(st, player, samples, rng)` (setup.py:63-83)

```
hand_ids = [cards[u].def_id for u in zones[player][HAND]]          # zone order
first = st.first_player == player
current = opening_score([db.by_id(i) for i in hand_ids], first)
rest = Counter(st.decklists[player]); rest.subtract(hand_ids)
pool = sorted(rest.elements())                                       # def_id ascending; counts <= 0 dropped
total = 0.0
for _ in range(samples): rng.shuffle(pool); total += opening_score(first 5 of pool, first)   # pool is NOT reset: shuffles compound
average = total/samples if samples else 0.0
stats = [Alternative(KEEP, current, 1), Alternative(REDRAW, 0.9*average, samples)]
return (KEEP if current >= 0.9*average else REDRAW), stats
```
`shuffle` (rng.py:43-46) is Fisher-Yates from the end: `for i in len-1 down to 1:
j = randrange(i+1); swap`. `randrange(n)` (rng.py:30-38) rejects `x >= 2^64 - (2^64 % n)` and
returns `x % n`.

### 13.3 `opening_score(hand, first)` (setup.py:30-60)

```
cards = sorted(hand, key=(-cost, card_number))      # stable; card_number str compared by code point
unpaired = []; ex = 0 if first else 1; score = 0.0
for turn in 1..=4:
    weight = 5 - turn; budget = turn + ex; spent = 0
    for c in [c for c in cards if c.card_type is UNIT]:                 # snapshot; UNIT only (not tokens)
        if c.level <= turn + ex and c.cost <= budget - spent:
            spent += c.cost; cards.remove(c); unpaired.append(c)
            score += weight * 0.5 * (c.ap + c.hp)
    for c in [c for c in cards if c.card_type is PILOT]:                # PILOT only
        if unpaired and c.level <= turn + ex and c.cost <= budget - spent:
            unit = first u in unpaired with link_satisfied(u, c), else unpaired[0]
            unpaired.remove(unit); spent += c.cost; cards.remove(c)
            score += weight * (0.5 * (c.ap + c.hp) + (1.0 if link_satisfied(unit, c) else 0.0))
    if spent > turn: ex = 0
return score
```
`list.remove` removes the first element that is `==`. `CardDef` is a frozen dataclass whose
first field is `def_id` (cards/model.py:92-96), so this is the first element with the same
`def_id`.

---

## 14. Decision log (base.py:79-122)

`log_entry(st, player, chosen, alternatives)` is called before `apply`. In-memory key order:
```
{"turn": st.turn, "player": player, "decision": dec.kind.value,
 "chosen": chosen.to_json(),                      # [kind.value, a, b, c]
 "label": describe(st, dec, chosen, player),
 "alternatives": [{"action": alt.action.to_json(), "label": describe(st, dec, alt.action, player),
                   "value": round(alt.value, 4), "visits": alt.visits} for alt in ranked]}
```
`ranked = sorted(alternatives, key=lambda x: (-x.visits, -x.value))[:5]` (`LOG_ALTERNATIVES = 5`).
The sort is stable, so ties keep the agent's order: root-move order (MCTS), option order (random,
greedy), `[keep, redraw]`, or `[first, second]`.

Alternative values by source:
- MCTS: the mean.
- Mulligan: `KEEP` gets `current` with visits 1; `REDRAW` gets `0.9*average` with visits
  `samples`. Redraw therefore usually sorts first.
- Play/draw: the score mean with visits `games`.
- Greedy: `evaluate` with visits 1.
- Random: `1.0/n` with visits 0.

`describe(st, dec, action, observer)` (base.py:112-122):
- For kinds in `{PLAY_UNIT, PLAY_BASE, PAIR, PLAY_COMMAND, ACTIVATE, ATTACK, BLOCK}`:
  `f"{kind.value} {card_label(a)}"`, plus `f" -> {card_label(b)}"` for PAIR and ATTACK.
- For SELECT with `uid_argument(dec)`: `f"select {card_label(a)}"`.
- Otherwise `str(action)`, which is `f"{kind.value}({','.join(str(x) for x in (a,b,c) if x != -1)})"`
  (engine/state.py:30-32). Only `-1` is omitted; `-2` is printed. Examples: `pass()`,
  `attack(3,-2)`, `select(0)`, `play_unit(5,0)`, `go_first(1)`.

`card_label(st, uid, observer)` (base.py:103-109) returns `"player"` if `uid == -2`. It returns
`f"#{uid}"` if the uid is out of range or `is_hidden_from(st, uid, observer)` (zone in
`{DECK, HAND, SHIELD, RESOURCE_DECK}` and the observer's known bit is unset). Otherwise it returns
`f"{card_number} {name}"`.

Real entries from a fast-preset sample game (seed 4242, agent seeds 17/18):
```
{"turn": 0, "player": 0, "decision": "choose_first", "chosen": ["go_first", 0, -1, -1], "label": "go_first(0)", "alternatives": [{"action": ["go_first", 0, -1, -1], "label": "go_first(0)", "value": 0.875, "visits": 8}, {"action": ["go_first", 1, -1, -1], "label": "go_first(1)", "value": 0.625, "visits": 8}]}
{"turn": 0, "player": 0, "decision": "redraw", "chosen": ["redraw", -1, -1, -1], "label": "redraw()", "alternatives": [{"action": ["redraw", -1, -1, -1], "label": "redraw()", "value": 12.7125, "visits": 96}, {"action": ["keep", -1, -1, -1], "label": "keep()", "value": 12.5, "visits": 1}]}
{"turn": 4, "player": 1, "decision": "main", "chosen": ["play_unit", 62, -1, 0], "label": "play_unit ST04-004 Moebius", "alternatives": [{"action": ["play_unit", 62, -1, 0], "label": "play_unit ST04-004 Moebius", "value": 0.4953, "visits": 9}, {"action": ["attack", 61, -2, -1], "label": "attack ST04-005 Strike Dagger -> player", "value": 0.5048, "visits": 7}, {"action": ["end_main", -1, -1, -1], "label": "end_main()", "value": 0.4929, "visits": 4}]}
```
Where the log is serialised (outside this scope, listed for the formatter):
- `games.ndjson` uses `json.dumps(sort_keys=True, separators=(",",":"))` with the default
  `ensure_ascii=True` (reports/writer.py:56). 32 card names contain non-ASCII characters (for
  example `Zaku Ⅱ`), which this writes as `Ⅱ`.
- Replays use `indent=2, sort_keys=True, ensure_ascii=False` (reports/writer.py:34, 83).
- Floats use Python `repr`, the shortest round-trip form. It is fixed notation for
  `1e-4 <= |x| < 1e16`, always with a `.0` when integral: `1.0`, `0.0001`, `12.7125`. Outside that
  range it is `1e-05` or `1e+16`.

---

## 15. `targeting.py` (policy helper in scope)

`HARMS=-1, UNKNOWN=0, HELPS=1`.
- `_HARMFUL_STEPS = (Damage, Destroy, Rest, ReturnToHand, ToDeck, Exile, ToTrash)`.
- `_HELPFUL_STEPS = (Recover, SetActive, AddToHand, DeployCard, PlayCard)`.
- None of these classes inherit from each other (checked through the MRO).

Functions:
- `choice_polarity(st, rule_table)` (lines 58-71) returns UNKNOWN unless `frames[-1].waiting`.
  It is memoised on `(program_id, pc, rule_table)` and computes `_polarity_at(instrs, pc,
  rule_table)`.
- `_first_step(instrs, pc)` (lines 126-137) returns None unless `instrs[pc]` is a `Choose`. Then,
  for each later instruction in order, it skips any whose `ref` attribute is not a
  `Var`/`Union` that `_uses` `ins.var` (`Union` is checked recursively). The first instruction
  that is harmful, helpful or an `Apply` is returned. Otherwise it returns None.
- `_polarity_at` (lines 115-123) maps a harmful step to HARMS, a helpful step to HELPS, an `Apply`
  to `_effect_polarity(effect)`, and anything else to UNKNOWN.
- `_effect_polarity` (lines 148-160):
  - `StatMod`: `total = sum` of the int-valued `ap`/`hp` (non-int Values are ignored). 0 gives
    UNKNOWN, >0 gives HELPS, <0 gives HARMS.
  - `KeywordGrant`/`TraitGrant`/`AbilityGrant` give HELPS.
  - `RuleGrant`: with `rule_table` it is `_rule_polarity`; otherwise `rule.kind.value` starting
    with `"cant_"` gives HARMS and anything else gives HELPS. So `cant_be_blocked`,
    `cant_receive_damage` and similar count as HARMS in the baseline.
  - Anything else gives UNKNOWN.
- `_rule_polarity` (lines 163-166): for `PLAY_COST_DELTA`/`PLAY_LEVEL_DELTA` with a non-zero int
  amount, negative gives HELPS and positive gives HARMS. Otherwise it reads `RULE_POLARITY[kind]`
  (lines 22-52). The table covers all 29 `RuleKind`s (verified; no KeyError possible today):
  - HARMS: cant_attack, cant_attack_player, cant_attack_units, cant_block, cant_be_set_active,
    stays_rested_in_start_phase, deployed_rested, cant_be_paired, must_attack,
    attack_target_fixed.
  - UNKNOWN: play_cost_delta, play_level_delta, custom.
  - HELPS: all others.
- `choice_rule_lasts_this_battle(st)` (lines 74-90) is memoised on `(program_id, pc)`. It checks
  that the first step is an `Apply` of a `RuleGrant` with `duration is THIS_BATTLE`.
- `command_harms_enemy_units(def_id)` (lines 96-112) is memoised on `def_id`. With
  `aid = reg.cards[def_id].command_aid >= 0`, it is True if any instruction is a `Choose` with
  `sel.side is ENEMY` and `_polarity_at(instrs, pc) == HARMS`. That call uses the default
  `rule_table=False`.

---

## 16. Hazards (Python-semantic traps), with path:line

1. **First-max ties** (Rust `max_by` returns the LAST max). Use a strict `>` scan everywhere:
   - mcts.py:187 (`_close_call` leader on `(visits, mean)`);
   - mcts.py:288 (unvisited by prior);
   - mcts.py:301 (UCB);
   - mcts.py:313 (`max(scores)`; the value only);
   - mcts.py:338 (final move on `(visits, mean)`);
   - agents.py:65 (greedy);
   - policy.py:93 (deterministic argmax);
   - policy.py:99 (Gumbel `v > best_v`).
2. **`sum()` is Neumaier-compensated**, not naive. See §12 for the algorithm. Sites: mcts.py:315,
   198, 199; setup.py:102, 103, 108, 109; evaluation.py:314, 317 (leaf values).
3. **`x ** 2` calls libm `pow(x, 2.0)`, which is not `x*x`.** On this macOS host, `x**2 != x*x` in
   233 of 200,000 random doubles. In every mismatch `x*x` was the correctly rounded value and
   CPython returned libm `pow` exactly. LLVM rewrites `powf(2.0)`/`powi(2)` into `x*x`, so Rust
   must call the platform `pow` with a non-constant exponent (for example via
   `std::hint::black_box(2.0)`). Sites: mcts.py:199, setup.py:109.
4. **Transcendentals depend on libm**: `math.exp` (mcts.py:314, evaluation.py:305-306),
   `math.log` (mcts.py:297, policy.py:98). Rust must use the same libm as the reference platform
   to be bit-identical. `math.sqrt` (mcts.py:200, 297, setup.py:110) is correctly rounded
   everywhere.
5. **`per_visit` stores `child.total - before`** (mcts.py:145), a rounded float difference that
   can differ from the leaf value in the last bits. Reproduce it literally.
6. **`round()`**: mcts.py:178 is `round(float) -> int` with ties to even (`f64::round_ties_even`).
   base.py:95 `round(v, 4)` rounds the *exact binary* value to 4 decimals, so `round(0.00015, 4)`
   gives `0.0001` and `round(0.12345, 4)` gives `0.1235`. Rust needs a correctly rounded
   decimal-to-4-places step followed by a parse back to f64. Do not use
   `(v*1e4).round()/1e4`.
7. **Float formatting in logs**: Python `repr` (shortest round-trip, `1.0` not `1`, exponent
   outside `[1e-4, 1e16)`). `games.ndjson` escapes non-ASCII as `\uXXXX`, with surrogate pairs
   for astral characters (reports/writer.py:56). Labels come from base.py:109 and can contain
   such characters.
8. **The RNG draw order is part of the contract**: mcts.py:102 (conditional noop seed), then
   122, 123, 125, 126 (`W[0:B]`, `R[0:B]`, `W[B:cap]`, `R[B:cap]`). The tree walk draws nothing.
   policy.py:97 draws once per option; a single-option `choose` draws nothing (policy.py:81-82).
   setup.py:98 draws `games` seeds; setup.py:78 shuffles one compounding pool; agents.py:57 is
   greedy's single draw; agents.py:30 always draws.
9. **The decision counter increments on every `choose`**, including forced single-option
   decisions (base.py:64). `derive_seed` material is a Python tuple `repr` (rng.py:58).
10. **`id()`-keyed dict** (mcts.py:133): use indices.
11. **Possible duplicate children in a backup path** with `observation_keys` (mcts.py:233-234,
    215-217). Each occurrence is updated, so keep the path as a list of arena indices with
    duplicates.
12. **`setdefault`** (mcts.py:317): existing children keep their first prior. Priors are
    computed over the *reduced* option tuple, in the world being walked (mcts.py:310), and at
    the root in `first_world`, not in `st` (mcts.py:130).
13. **`... or root_moves`** (mcts.py:108) and **`kept or moves`** (actions.py:277) fall back to
    the unfiltered list when filtering empties it.
14. **Set/frozenset/dict use is membership-only, so no iteration-order dependence**:
    - membership sets: mcts.py:42 (`_TARGET_KINDS`), actions.py:146 (`zero`), actions.py:386
      (`seen`), noop.py:152 (`out`), noop.py:232 (`frozenset(area)` compared by subset and
      equality, **not** ordering); `node.children` and `worlds` are lookup-only.
    - Python string `hash()` is randomised per process, but it only affects these sets, so
      results do not depend on `PYTHONHASHSEED`.
    - `once_used` is a `set`, iterated at actions.py:309 and then sorted.
    - `dv.kw` (a dict) is iterated at actions.py:356 and then sorted.
15. **Unsorted list order that matters**: noop.py:48-50 (`lasting`/`delayed` in engine list
    order; the Rust engine must keep the same order); noop.py:44 (cards in uid order);
    actions.py:303-307 (history order); canonical key order is first-appearance order
    (actions.py:75-85).
16. **Tuple equality semantics**: actions.py:360 compares rules by `repr(r.rule)` strings, which
    are type-sensitive (`True` is not `1` and `1.0` is not `1` in repr). The 68 distinct
    registry `RuleMod` reprs contain no `0x` addresses (scanned). noop.py:223 compares
    `rest != list(y[3:])` as list against list. noop.py:117 and 119 use dataclass equality on
    `Action` over all four fields.
17. **`list.remove` removes the first `==` element** (setup.py:48, 54, 56); `CardDef` equality
    amounts to `def_id` equality. `next(..., unpaired[0])` (setup.py:53) is a first-match
    fallback.
18. **`sorted` key and stability**: setup.py:37 (`(-cost, card_number)` with code-point string
    comparison); setup.py:75 (ints); base.py:84 (stable, `(-visits, -value)`; `-0.0 == 0.0` ties
    keep order).
19. **`//` and `%`** only run on `c >= 0` (actions.py:45, 64), so negative-operand floor
    semantics never come up. `c < 0` maps to 0 EX and variant `-1`.
20. **Unbounded ints**: seeds are u64 (masked; rng.py:21, 58). `_SELF = -(1 << 40)` and
    `_DEF_OFFSET + def_id` fit in i64. Agent seeds can be anything up to `2^64 - 1`; the decimal
    in the `repr` material is the *masked* value.
21. **Booleans as ints**: `c.known & (1 << decider)` truthiness (actions.py:52); `isinstance(v,
    int)` also accepts `bool` (targeting.py:150, 164).
22. **Global memo caches** (actions.py:167, targeting.py:54-55, 93, policy.py:63 `@cache`) are
    pure functions of the static registry, so they never change results. `_chosen_only_to_rest`
    checks `once_key/acted` *before* the cache (actions.py:178).
23. **`check=False`** applies everywhere in the search and noop, but `first_player_choice` uses
    `apply(det, choice)` with checking (setup.py:116).
24. **Exceptions**: none are used for control flow in scope. Errors raised:
    `ValueError` (base.py:63, agents.py:118, 127, config.py:50, runner/agents.py:85, 92),
    `AgentUnavailableError` (runner/agents.py:61), `ImportError` (runner/agents.py:73). The
    `assert`s at mcts.py:87-88 and noop.py:86 are invariants.
25. **Default arguments are immutable** (`BASELINE_KNOBS`, `DEFAULT_WEIGHTS`, both frozen).
    There is no default-argument mutation in scope.
26. **Same seed reused** for determinisation and rollout in play/draw sims (setup.py:115-117).
    The trial worlds use the fixed seed 0 (actions.py:136, 149, 219).

---

## 17. Acceptance vectors for the Rust port

- **Baseline replay** (tests/ai/test_baseline.py:22-50):
  - `new_game(DECKS, 4242)`, agents `make_agent("mcts", seed=17+seat, log=True,
    config=BASELINE_PRESETS["standard"], weights=BASELINE_WEIGHTS)`.
  - Expect `(winner, turn, len(actions)) == (1, 20, 115)`.
  - `sha256(json.dumps({"actions", "logs", "winner", "turn"}, sort_keys=True))` (default
    separators `", "`/`": "`, `ensure_ascii=True`) must equal
    `5cff789d2a8bb26cfef4db98b1ef1ef8f37c3bf63210114781d357bd215f3b96`.
- **Standard preset, `new_game(DECKS, 4242)`, agents `make_agent("mcts", preset="standard",
  seed=17+seat)`**: the first three `search` calls. Values are given as float hex (mean) and
  visits, in root-move order:

| agent seed, decision # | rng state at entry | turn/kind | iterations | alternatives |
|---|---|---|---|---|
| 18, 2 | 10333726690911006628 | 2 main | 160 | `play_unit(61,1)` 0x1.b00be33f8f4adp-2 ×79; `end_main` 0x1.b577bf6413a4fp-2 ×81 |
| 17, 4 | 5522025450367495518 | 3 main | 135 (settled: 81−54 > 25) | `play_base(31,0)` 0x1.13ef2f7e301fep-1 ×54; `end_main` 0x1.22d62543c78f1p-1 ×81 |
| 18, 3 | 12087617093001095045 | 4 main | 154 of 256 (7 options → 4 moves) | `play_unit(50,0)` 0x1.0a26943782b1cp-1 ×117; `play_unit(61,0)` 0x1.8433a1a5f5fb9p-2 ×13; `play_unit(62,0)` 0x1.92b6d97cadff7p-2 ×14; `end_main` 0x1.7a544a7976106p-2 ×10 |

  For all three, the rng state after the search is the entry state plus `2*budget` draws:
  `6107142279843301860`, `1295441039299790750`, `1635733220550857093`.

---

## Open questions

1. **Reference platform for libm.** `x**2` (libm `pow`), `exp` and `log` results depend on the C
   library: macOS libm `pow(x, 2)` differs from `x*x` about 0.1% of the time, and glibc's
   behaviour differs again. Which platform's Python output is the golden reference, and must the
   Rust port link the same libm (or a bit-exact port of it)?
2. **The CPython `sum()` algorithm** was confirmed empirically (200k lists) on 3.15.1, not by
   reading the C source of this exact build. Mixed int/float iterables (int items in the float
   path) are not covered; none occur in scope.
3. **Duplicate children in one backup path** (observation_keys, §4.5) is possible by code but was
   not observed. Is there an engine sequence that produces two opponent action-step passes with
   no searcher decision between them? Either way, Rust must replicate the double update.
4. **`repr(r.rule)` equality** (actions.py:360) is assumed to equal structural equality of
   `RuleMod` values with type-sensitive scalars. A future `Value` type whose repr is lossy or
   address-based would break that.
5. **Engine contracts used here** are documented in the engine inventories, not here:
   `determinize`, `V.derived`, `same_values`, `trigger_index`, `declare_attack`,
   `action_step_options`, `_activated_list`, `_command_playable`, `apply(check=False)` side
   effects, and the exact turn/pending transitions that define "leaf". Their exact semantics
   decide byte equality.
6. **`policy.option_scores`** (every scorer) and the evaluation features are outside this file.
   The prior and rollout formulas above depend on them bit for bit.
7. **`_a_branch_beats_idling`** (noop.py:203-205): when the replay diverges, the "idle" line is
   the plain policy from the root world, which may replay the move under test. Is that intended?
   It is documented as-is; the port must copy it.
