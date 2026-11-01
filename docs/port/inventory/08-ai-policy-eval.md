# 08 — AI playout policy, card values, evaluation, playouts, baseline players, comparison and tuning CLIs

Scope: `src/gcg_sim/ai/{policy,values,evaluation,playout,compare,selfplay,tuning,throughput}.py`
and the two baseline players `RandomAgent` and `GreedyAgent`, which live in
`src/gcg_sim/ai/agents.py:22-66`. Line numbers are for branch `rust-port` at `d9d41af`, Python
3.15.1 (`uv run python -c "import sys; print(sys.version)"` gives
`3.15.1 (main, Oct 21 2026, 17:39:16) [Clang 23.1.3 ]`).

Notation: `policy.py:93` means `src/gcg_sim/ai/policy.py` line 93. `V.` is `gcg_sim.engine.view`
(imported as `V`; policy.py:33, evaluation.py:25, values.py:9). `d.` is `gcg_sim.effects.dsl`.
`dv` is the `V.Derived` of the state (`V.derived(st)`, cached per state). "u64" is an int in
`[0, 2^64)`.

Related inventories, which this file does not repeat:
- `07-ai-search.md`. It covers MCTS, `SearchConfig` and presets, `actions.py` (canonical keys,
  `prune_dominated`, `_attack_can_matter`, `ex_used`), `targeting.py` (§15 there), `setup.py`
  (mulligan, first player), `base.py` (`AgentCore`, the decision log), and the Neumaier `sum` model
  (§12 there).
- `05-engine-state-view.md`: `Derived`, `ap_of`, `hp_of`, keywords, rules.
- `06b-engine-game-battle-observe.md`: `apply`, `determinize`, `unknown_pools`.
- `02-reports.md`: `wilson` and the JSON emitters.

---

## 1. Where each piece is used (call graph)

| Caller | Calls | Knobs / weights | RNG |
| --- | --- | --- | --- |
| MCTS rollout (mcts.py:211-212) | `play_until_turn(s, horizon, rng, cfg.rollout_temperature, cfg.max_rollout_decisions, knobs_for(cfg))` | `knobs_for(cfg)` | per-iteration generator (07 §3.3) |
| MCTS priors (mcts.py:308-316) | `policy.option_scores(s, Decision(dec.player, dec.kind, <pruned moves>, dec.prompt, dec.context), knobs_for(cfg))`, then `exp((x - top)/prior_temperature)/total` | `knobs_for(cfg)` | none |
| MCTS leaf (mcts.py:254-264) | `evaluation.score`, `evaluation.logistic` | `weights` | none |
| MCTS / noop / greedy | `burst_density(st, player)` on the **true** state | — | none |
| Greedy (agents.py:57-65) | `play_until_turn(s, st.turn + 1, None, 0.0, 200)` and then `evaluate` | `BASELINE_KNOBS`, `self.weights` | none in the playouts |
| First-player sims (setup.py:117) | `play_to_end(det, SplitMix64(seed), 0.5, 3000, knobs)` | `knobs_for(config)` | `SplitMix64(seed)` |
| noop (noop.py:56) | `policy.choose(st, None, 0.0, knobs)` | given | none |
| Tuning observer (tuning.py:148-157) | `features` / `role_features` on the **true** state | — | none |

**Important.** `option_scores` reads options **only** from its `dec` argument, never from
`st.pending`. MCTS passes a Decision rebuilt with the pruned and merged moves (mcts.py:308-311).
Because of this, `_board.attackers` (policy.py:147-153) and `_done_score`'s candidates
(policy.py:382) count only those moves. The Rust signature must take the option list explicitly.

---

## 2. `policy.py`: constants, knobs, entry points

### 2.1 Constants (policy.py:42-47)

| Name | Value |
| --- | --- |
| `EX_PENALTY` | `1.5` |
| `LETHAL` | `60.0` |
| `CLEAR_THE_WAY` | `8.0` |
| `MUST_BLOCK` | `100.0` |
| `KEEPABLE_TOP` | `2.0` |
| `IDLE_ATTACK` | `-1.0` |

Engine constants used: `core.BATTLE_LIMIT = 6` (engine/core.py:16), `SUPPORT_AID = -10`
(engine/game.py:31), `ALT_PLAY_BASE = 100` (engine/game.py:32), `PLAYER_TARGET = -2` and
`NO_ARG = -1` (engine/types.py:138-139). `ex_used(action)` (actions.py:41-45) is `0` unless
`kind ∈ {PLAY_UNIT, PLAY_BASE, PAIR, PLAY_COMMAND, ACTIVATE}` and `c >= 0`, in which case it is
`c % 100`.

### 2.2 `PolicyKnobs` (policy.py:50-60)

Frozen dataclass, with fields in this order:

| Field | Type | Default (`BASELINE_KNOBS`) | Read at |
| --- | --- | --- | --- |
| `burst_yes_score` | float | `1.0` | `_burst` (policy.py:310) |
| `rule_polarity_table` | bool | `False` | `_select` (policy.py:359-361) |
| `zero_ap_face` | bool | `False` | `_board` (policy.py:151), `_attack` (policy.py:233) |

`knobs_for(config)` (policy.py:63-69) returns
`PolicyKnobs(config.policy_burst_yes_score, config.policy_rule_polarity_table,
config.policy_zero_ap_face)`. It is memoised with `functools.cache`, which has no behavioural
effect. The values in use:

| Source | burst_yes_score | rule_polarity_table | zero_ap_face |
| --- | --- | --- | --- |
| `BASELINE_KNOBS` (greedy playouts, `play_*` default, setup default) | 1.0 | False | False |
| `PRESETS` fast / standard / strong (`_adopted`, config.py:60-77) | 4.0 | True | True |
| `BASELINE_PRESETS` standard / strong (baseline.py:18-73) | 1.0 | False | False |
| `TUNING_CONFIG` (tuning.py:50-57; not `_adopted`) | 1.0 | False | False |

### 2.3 `choose(st, rng, temperature, knobs=BASELINE_KNOBS)` (policy.py:72-83)

```
dec = st.pending                     # assert not None
if len(dec.options) == 1: return dec.options[0]        # NO rng draw, NO scoring
return pick(dec.options, option_scores(st, dec, knobs), rng, temperature)
```

### 2.4 `pick(options, scores, rng, temperature)`: the sampler (policy.py:86-101)

This is **not** an explicit softmax. There is no max-subtraction, no `exp`, and no normalisation.
It is the Gumbel-max trick, which samples exactly from `softmax(score / T)`:

```
if rng is None or temperature <= 0.0:
    return options[ FIRST index i maximising scores[i] ]      # max(range(n), key=scores.__getitem__)
best_i = 0; best_v = -inf
for i, s in enumerate(scores):                                 # option order, every option
    u = rng.random() or 1e-300                                 # exact 0.0 → 1e-300
    v = s / temperature - math.log(-math.log(u))
    if v > best_v: best_i, best_v = i, v                       # strict >: first wins ties
return options[best_i]
```

- `rng.random()` is `(next_u64() >> 11) * (1.0 / (1 << 53))` (rng.py:40-41), which lies in
  `[0, 1 - 2^-53]`. It returns exactly `0.0` only when `next_u64() < 2048`, and then `u = 1e-300`.
- Exactly **one `next_u64` per option**, in option order, is drawn for every sampled decision.
  Options with equal scores still draw. A single-option decision draws nothing (see 2.3).
- The float operations, in order: `q = s / T` (a division, not a multiplication by `1/T`);
  `a = ln(u)`; `b = -a`; `c = ln(b)`; `v = q - c`. `math.log` calls the C libm `log`, so the Rust
  port must call the same platform libm (`f64::ln`), not a pure-Rust `libm` crate. I checked this
  on this machine with a scratch Rust program, `/private/tmp/claude-501/ai08/rt/g.rs` (rustc 1.99.1,
  `-O`):
  - It reproduced vector §11.1 exactly (best index 3, state `8709371129873690750`) and the
    `logistic` vectors.
  - Over 1,000,000 `SplitMix64(7).random()` values, the wrapping sums of the bits of
    `f64::ln(-f64::ln(u))` and `f64::exp(40u-20)` were `6713190858000061441`. Python
    `math.log` / `math.exp` gave the same value.
- Decision kinds with no scorer (`ORDER_TRIGGER`, `PAYMENT`) score all zeros. With an rng the
  choice is then uniform, and it still draws `len(options)` values. With `rng=None` it is option 0.

### 2.5 `option_scores(st, dec, knobs)` (policy.py:104-108) and the scorer table (policy.py:388-400)

| `DecisionKind` (value) | Scorer |
| --- | --- |
| `MAIN` ("main") | `_main` |
| `BLOCK` ("block") | `_block` |
| `ACTION_STEP` ("action_step") | `_action_step` |
| `BURST` ("burst") | `_burst` |
| `YES_NO` ("yes_no") | `_yes` |
| `CHOOSE_FIRST` ("choose_first") | `_choose_first` |
| `REDRAW` ("redraw") | `_keep` |
| `EXCESS` ("excess") | `_excess` |
| `DISCARD` ("discard") | `_discard` |
| `ARRANGE` ("arrange") | `_arrange` |
| `SELECT` ("select") | `_select` |
| `ORDER_TRIGGER`, `PAYMENT` | none → `[0.0] * len(options)` |

Each scorer returns one float per option, in option order.

---

## 3. `values.py`: shared heuristic values

All of these are pure functions. Integer arithmetic stays integer until the first float operand.

- `hp_left(st, dv, uid)` (values.py:18-19) = `max(0, V.hp_of(st, dv, uid) - st.cards[uid].damage)`
  (int).
- `unit_value(st, dv, uid)` (values.py:22-33), evaluated in this order:
  ```
  v = 0.5 * (V.ap_of(uid) + hp_left(uid))  +  0.25 * V.level_of(st, uid)   # ints summed first; level_of = 0 for tokens
  if uid in dv.linked:        v += 1.0            # LINK_BONUS
  elif st.cards[uid].pair >= 0: v += 0.5          # PAIR_BONUS (paired, not linked)
  kws = dv.kw.get(uid)                            # dict Kw→amount
  if kws: v += 0.3 * len(kws)                     # KEYWORD_BONUS × number of distinct keywords
  ```
- `card_worth(cd)` (values.py:36-47) reads raw `CardDef` fields:

  | `card_type` | Formula (Python evaluation order) |
  | --- | --- |
  | UNIT | `(1.0 + 0.3*cost) + 0.1*(ap + hp)` |
  | PILOT | `1.3 + 0.1*(ap + hp)` |
  | COMMAND | `1.4 + 0.2*cost` |
  | BASE | `1.2` |
  | anything else (RESOURCE, UNIT TOKEN, EX BASE, EX RESOURCE) | `0.5` |

  The order matters. Re-associating the UNIT/PILOT sums changes the result for 134 of the 1152
  real cards. I measured this with `R.db.real_cards()` and the alternatives
  `1.0 + (0.3*cost + 0.1*(ap+hp))` and `1.3 + 0.1*ap + 0.1*hp`. None of them crosses
  `KEEPABLE_TOP` today, but the bits differ, and they feed into sums and comparisons. Several
  cards land on exactly `2.0`, the `_arrange` threshold. Examples: Unit cost 2 with AP 2 / HP 2
  gives `2.0`; Pilot AP 2 / HP 5 gives `2.0`; Command cost 3 gives `2.0`.
- `pile_value(st, dv, uid)` (values.py:50-54) = `unit_value` if the card's zone is `BATTLE` or
  `BASE`, else `card_worth(V.cdef(st, uid))`.
- `Fight(kills, dies)` and `fight(st, dv, attacker, defender)` (values.py:57-71):
  ```
  a_ap = ap_of(attacker); d_ap = ap_of(defender)
  kills = a_ap >= hp_left(defender) and a_ap > 0
  first_strike = Kw.FIRST_STRIKE in dv.kw.get(attacker, {})
  dies  = d_ap >= hp_left(attacker) and d_ap > 0 and not (first_strike and kills)
  ```
- `ready_blockers(st, dv, player, exclude=-1)` (values.py:74-80) lists the uids in
  `zones[player][BATTLE]` order with `u != exclude`, `not rested`, and `Kw.BLOCKER in dv.kw[u]`.
- `life(st, dv, player)` (values.py:83-87) = `len(SHIELD) + (Σ_int hp_left over BASE) / 3.0`.
- `exposed(st, player)` (values.py:90-93) = `not SHIELD and not BASE` (both zones empty).

---

## 4. Policy heuristics per decision kind

Float expressions are written with explicit parentheses that show Python's evaluation order. Every
`cost` below is `EX_PENALTY * ex_used(o)` = `1.5 * int`.

### 4.1 MAIN: `_main` (policy.py:158-161), `_board` (policy.py:133-155), `_main_option` (policy.py:164-184)

`_board` is computed once per call, with `p = dec.player` and `enemy = 1 - p`:

| Field | Value |
| --- | --- |
| `player` | `p` |
| `units` | `len(zones[p][BATTLE])` |
| `enemy_blockers` | `tuple(ready_blockers(st, dv, enemy))` (zone order) |
| `enemy_exposed` | `exposed(st, enemy)` |
| `enemy_has_base` | `bool(zones[enemy][BASE])` |
| `my_base_is_weak` | `not zones[p][BASE] or cdef(zones[p][BASE][0]).card_type is EX_BASE` |
| `enemy_max_ap` | `max(ap_of(u) for u in zones[enemy][BATTLE])`, or `0` if empty |
| `attackers` | number of **distinct** `o.a` over `dec.options` with `o.kind is ATTACK and (not knobs.zero_ap_face or _attack_can_matter(st, p, o.a))` (a set, used only for its size) |
| `knobs` | the knobs |

`blocked_from_lethal = enemy_exposed and 0 < attackers <= len(enemy_blockers)`
(policy.py:127-130).

Per option (`_main_option`), checked in this order:

| `o.kind` | Score |
| --- | --- |
| END_MAIN | `0.0` |
| ATTACK | `_attack(st, dv, b, o.a, o.b)` (no EX cost) |
| PLAY_UNIT | `_play_unit(st, b, o.a) - cost` |
| PAIR | `_pair(st, dv, o.a, o.b) - cost` (`a` is the pilot-capable card, `b` the Unit) |
| PLAY_BASE | `(4.0 if b.my_base_is_weak else -1.0) - cost` |
| PLAY_COMMAND | `s = (2.5 + 0.3 * cdef(o.a).cost) - cost`; then `if b.blocked_from_lethal and targeting.command_harms_enemy_units(st.cards[o.a].def_id): s += 8.0` |
| ACTIVATE | `(0.5 if o.b == SUPPORT_AID(-10) else 1.5) - cost` |
| anything else | `0.0` |

`cdef(o.a).cost` is the raw `CardDef.cost` (not `play_cost`).

`_play_unit(st, b, uid)` (policy.py:187-192), with `cd = cdef(uid)`:
```
s = (4.0 + 0.5*cd.cost) + 0.2*(cd.ap + cd.hp)        # raw CardDef stats
if b.units >= 6: s -= 8.0                            # BATTLE_LIMIT
```

`_pair(st, dv, pilot, unit)` (policy.py:195-206), with `pcd = cdef(pilot)`, `ucd = cdef(unit)`,
`uc = st.cards[unit]`:
```
s = (4.5 + 0.2*(pcd.ap + pcd.hp)) + 0.1*unit_value(st, dv, unit)
if V.link_satisfied(ucd, pcd):
    s += 3.0
    if uc.entered_turn == st.turn and not uc.rested: s += 1.0
if uc.rested: s -= 1.5
```

`_unblockable(dv, attacker)` (policy.py:209-212) = `Kw.HIGH_MANEUVER in dv.kw.get(attacker, {})`
or `V.rules_of(dv, attacker, RuleKind.CANT_BE_BLOCKED)` is non-empty.

`_block_risk(st, dv, b, attacker, target)` (policy.py:215-227):
```
if _unblockable(attacker): return 0.0
worst = 0.0; a_val = unit_value(attacker)
for bl in b.enemy_blockers:                     # zone order
    if bl == target: continue
    f = fight(attacker, bl)
    if f.dies:
        gain = 0.5*unit_value(bl) if f.kills else 0.0
        worst = max(worst, (1.0 + 0.5*a_val) - gain)    # Python max: keeps `worst` on ties
return worst
```

`_attack(st, dv, b, attacker, target)` (policy.py:230-250). `risk = _block_risk(...)` is computed
first, always.
- If `target == PLAYER_TARGET(-2)`:
  1. If `knobs.zero_ap_face and not _attack_can_matter(st, b.player, attacker)`, return `-1.0`
     (IDLE_ATTACK).
  2. If `b.enemy_exposed`:
     - `_unblockable(attacker) or not b.enemy_blockers` → `60.0` (no risk subtracted);
     - else `b.blocked_from_lethal` → `2.0` (no risk subtracted);
     - else `30.0 - risk` (`LETHAL / 2` is exactly `30.0`).
  3. Otherwise: `s = 2.5 - (0.3 if b.enemy_has_base else 0.0)`. Then
     `if b.enemy_max_ap >= hp_left(attacker): s -= 0.4`. Return `s - risk`.
     These are two separate float subtractions: with both penalties `s` is
     `1.8000000000000003` (`2.5-0.3` = `2.2`, then `-0.4`), **not** `2.5-0.7` (= 1.8).
- Unit target:
  ```
  f = fight(attacker, target)
  s = (3.0 + 0.5*unit_value(target)) if f.kills else 0.3      # conditional binds loosest
  if f.dies: s -= (1.0 + 0.5*unit_value(attacker))
  breach = V.kw_amount(dv, attacker, Kw.BREACH)                 # int, 0 if absent
  if f.kills and breach and not exposed(st, 1 - b.player): s += 0.5*breach
  return s - risk
  ```

### 4.2 BLOCK: `_block` (policy.py:257-285)

`battle = st.battle` (the **last** element of `st.battles`, state.py:522-523). The code uses
`attacker = battle.attacker`, `target = battle.target`, `p = dec.player`, and
`at_player = target == PLAYER_TARGET or st.cards[target].zone is BASE`.
```
if at_player:
    saved = 100.0 if (target == PLAYER_TARGET and exposed(st, p))
            else 1.2 + (2.0 if life(st, dv, p) <= 2 else 0.0)      # 3.2 or 1.2
else:
    tf = fight(attacker, target)
    saved = (1.0 + 0.5*unit_value(target)) if tf.kills else 0.0
per option:
    not BLOCK (i.e. NO_BLOCK) → 0.0
    BLOCK(a=blocker): f = fight(attacker, o.a); s = saved
        if f.kills: s -= (1.0 + 0.5*unit_value(o.a))
        if f.dies:  s += (1.0 + 0.5*unit_value(attacker))
```

### 4.3 ACTION_STEP: `_action_step` (policy.py:288-298)

`in_battle = bool(st.battles)`.

| Option | Score |
| --- | --- |
| PASS | `0.0` |
| PLAY_COMMAND | `(0.3 if in_battle else -1.0) - 1.5*ex_used(o)` |
| anything else (ACTIVATE) | `(0.2 if in_battle else -0.5) - 1.5*ex_used(o)` |

### 4.4 Effect choices

| Kind | Scorer | Score per option |
| --- | --- | --- |
| YES_NO | `_yes` (policy.py:305-306) | YES `1.0`, else `0.0` |
| BURST | `_burst` (policy.py:309-310) | YES `knobs.burst_yes_score` (1.0 or 4.0), else `0.0` |
| CHOOSE_FIRST | `_choose_first` (policy.py:313-314) | `1.0 if o.a == dec.player else 0.0` (always prefers going first) |
| REDRAW | `_keep` (policy.py:317-318) | KEEP `1.0`, else `0.0` |
| EXCESS | `_excess` (policy.py:321-323) | `-pile_value(st, dv, o.a)`. The options are `SELECT(uid)` (interp.py:890-896) |
| DISCARD | `_discard` (policy.py:326-331) | `w = card_worth(cdef(o.a))`; `-w` if `st.cards[o.a].owner == dec.player` else `+w`. The options are `SELECT(uid)` (interp.py:539, 551-553) |

**ARRANGE** `_arrange` (policy.py:334-346). Decision contexts are built at interp.py:1153-1192.
`dec.ctx(key)` returns `NO_ARG = -1` when the key is missing (state.py:45-49).
- `ctx("order") == 1` ("next card (top first)", options `SELECT(uid)`): each option scores
  `_deck_worth(st, o.a, dec.player)` = `card_worth(cdef(uid))` if the card's owner is the
  chooser, else its negation.
- Otherwise `card = ctx("card")`:
  - `card < 0` (the "top_or_bottom" all-cards prompt, ctx `("all", 1)`, options `SELECT(0)` =
    top and `SELECT(1)` = bottom): `[0.1 if o.a == 0 else 0.0]`.
  - else (options `SELECT(0, card)` / `SELECT(1, card)`):
    `keep_on_top = _deck_worth(card) >= 2.0`, and the score is
    `1.0 if (o.a == 0) == keep_on_top else 0.0`.

**SELECT** `_select` (policy.py:349-378):
1. `ctx("mode") == 1` (a "choose mode" prompt, interp.py:274): all `0.0`.
2. `protected = ctx("rest_sub")`. If `>= 0` (a "rest instead?" substitute prompt, interp.py:690-699,
   with options `SELECT(sub)…, DONE`):
   `keep = pile_value(protected)`. DONE scores `0.0`; every other option scores
   `(keep - pile_value(o.a)) - 0.5`.
3. Otherwise, `polarity = targeting.choice_polarity(st, knobs.rule_polarity_table)`, which gives
   HARMS=-1 / UNKNOWN=0 / HELPS=1 (see 07 §15). `battling = None`. If `knobs.rule_polarity_table
   and targeting.choice_rule_lasts_this_battle(st)`, then `battling = ()` when `st.battle is None`,
   else `(battle.attacker, battle.target)`.
   For each option:
   - if `o.kind is not SELECT` or `not 0 <= o.a < len(st.cards)`: `_done_score(st, dec, polarity)`;
   - else, with `worth = pile_value(o.a)` and `mine = st.cards[o.a].owner == dec.player`:
     `wants = (polarity == HELPS) if mine else (polarity != HELPS)`. Then
     `if mine and polarity == UNKNOWN: wants = True`. In other words, UNKNOWN wants every card.
     - if `battling is not None and mine and o.a not in battling`: `-worth`;
     - else `worth if wants else -worth`.

`_done_score(st, dec, polarity)` (policy.py:381-385) is recomputed for each non-card option:
```
cands = [o.a for o in dec.options if o.kind is SELECT and 0 <= o.a < len(st.cards)]
mine = bool(cands) and st.cards[cands[0]].owner == dec.player       # FIRST candidate only
unwanted = (mine and polarity == HARMS) or (not mine and polarity == HELPS)
return 1.0 if unwanted else -0.25
```

`targeting.command_harms_enemy_units(def_id)` is used in 4.1 and 6.2. It is documented in 07 §15.
It always uses `rule_table=False`.

---

## 5. `playout.py` (playout.py:12-41)

```
play_until_turn(st, turn, rng, temperature, max_decisions, knobs=BASELINE_KNOBS) -> int:
    n = 0
    while st.winner is None and st.turn < turn and n < max_decisions and st.pending is not None:
        apply(st, policy.choose(st, rng, temperature, knobs), check=False); n += 1
    return n
play_to_end(st, rng, temperature, max_decisions, knobs=BASELINE_KNOBS) -> int:
    same loop without the `st.turn < turn` test
```

Both mutate `st` in place. The conditions are checked in the order shown. Both players' decisions
are made by the policy, and the engine's own randomness comes from `st.rng`. Vector:
`new_game(DECKS, 99)` followed by `play_until_turn(st, 3, None, 0.0, 200)` returns 8 and stops at
turn 3 on a `main` decision (§11).

---

## 6. `evaluation.py`: features, weights, logistic

### 6.1 Feature names (evaluation.py:30-56)

`FEATURES` ("base", 14 features, index: name):
`0 tempo, 1 life, 2 danger, 3 can_kill, 4 ap, 5 hp, 6 linked, 7 ready, 8 hand_units, 9 hand_pilots,
10 hand_commands, 11 hand_bases, 12 level, 13 bursts`.

`ROLE_FEATURES` ("roles", 17 features): `FEATURES` without `can_kill`, then 4 more:
`0 tempo, 1 life, 2 danger, 3 ap, 4 hp, 5 linked, 6 ready, 7 hand_units, 8 hand_pilots,
9 hand_commands, 10 hand_bases, 11 level, 12 bursts, 13 can_kill_turn_player, 14 can_kill_other,
15 inv_race_turn_player, 16 inv_race_other`.

`FEATURE_SETS = {"base": FEATURES, "roles": ROLE_FEATURES}`. Constants (evaluation.py:57-59):
`BASE_LIFE_HP = 3.0`, `DANGER_LIFE = 3.0`, `RACE_CAP = 6`.

### 6.2 `_side(st, dv, x)` (evaluation.py:143-202)

The fields are listed in order. `zones = st.zones[x]`.

| Field | Computation |
| --- | --- |
| `shields` | `len(zones[SHIELD])` |
| `has_base` | `bool(zones[BASE])` |
| `life` | `shields + base_hp / 3.0`, where `base_hp = Σ_int max(0, hp_of(u) - damage)` over BASE |
| `units` | `len(zones[BATTLE])` |
| `ap` | `Σ_int ap_of(u)` over BATTLE |
| `hp` | `Σ_int max(0, hp_of(u) - damage)` over BATTLE |
| `linked` | count of BATTLE `u` with `u in dv.linked` |
| `blockers` | count of BATTLE `u` that are not rested and have `Kw.BLOCKER in dv.kw[u]` |
| `ready` | count of BATTLE `u` that are not rested |
| `hand` | `(units, pilots, commands, bases)`. For each HAND card, `_HAND_SLOT.get(cdef.card_type, 3)` with `{UNIT:0, PILOT:1, COMMAND:2, BASE:3}`. Any other type counts as slot 3 |
| `level` | `len(zones[RESOURCE_AREA])` (counts EX Resources too) |
| `answers` | count of HAND cards with `card_type is COMMAND and max(cd.level, cd.cost) <= level and command_harms_enemy_units(cd.def_id)` (raw CardDef level/cost) |

The opponent's hand is read from `st`. In MCTS and greedy that is the **determinized** hand. The
tuning observer uses the true hand.

### 6.3 `_can_kill(attacker, defender, attacker_active)` (evaluation.py:205-212)

```
attackers = attacker.ready if attacker_active else attacker.units
blockers  = max(0, defender.blockers - attacker.answers)
hits_needed = defender.shields + (1 if defender.has_base else 0) + 1
return 1.0 if attackers - blockers >= hits_needed else 0.0
```

### 6.4 `_vector(s, o, active, density)` (evaluation.py:215-228): 14 floats

`(0.0, s.life, max(0.0, 3.0 - s.life), _can_kill(s, o, active), float(s.ap), float(s.hp),
float(s.linked), float(s.ready), float(hand[0]), float(hand[1]), float(hand[2]), float(hand[3]),
float(s.level), density * s.shields)`.

`max(0.0, x)` returns `0.0` unless `x > 0.0`. `density * s.shields` is a float times an int.

### 6.5 `features(st, player, density)`: "base" (evaluation.py:231-241)

```
me = _side(player); opp = _side(1 - player); mine_active = st.active == player
a = _vector(me, opp, mine_active, density[player])
b = _vector(opp, me, not mine_active, density[1 - player])
out = [a[i] - b[i] for i in 0..14]; out[0] = 1.0 if mine_active else -1.0
```

### 6.6 `_race(st, dv, attacker, defender)` (evaluation.py:244-277): int in `1..6`

```
blockers = #{u in defender BATTLE : Kw.BLOCKER in dv.kw[u] and not rules_of(u, CANT_BLOCK)}  # rested NOT checked
unblockable, blockable = [], []
for u in attacker BATTLE (zone order):
    ap = ap_of(u)
    if ap <= 0 or rules_of(u, CANT_ATTACK) or rules_of(u, CANT_ATTACK_PLAYER): continue
    (unblockable if Kw.HIGH_MANEUVER in dv.kw[u] or rules_of(u, CANT_BE_BLOCKED) else blockable).append(ap)
blockable.sort(reverse=True)                 # ints, descending
through = unblockable + blockable[blockers:]
if not through: return 6
base_hp = Σ_int max(0, hp_of(u) - damage) over defender BASE
mean_ap = sum(through) / len(through)        # int / int → float (true division)
base_hits = math.ceil(base_hp / mean_ap) if defender BASE non-empty else 0   # int / float → float, ceil → int
hits = base_hits + len(defender SHIELD) + 1
return min(6, math.ceil(hits / len(through)))  # int / int → float, ceil
```

Replicate the float divisions and `ceil`. An exhaustive check over hp 0-15, Σap 1-59 and 1-12
attackers showed no case where they differ from exact rational ceil-division, but the Rust code
should still follow the Python form.

### 6.7 `role_features(st, density)`: "roles" (evaluation.py:280-300)

```
tp = st.active; other = 1 - tp
a = _vector(_side(tp), _side(other), True,  density[tp])
b = _vector(_side(other), _side(tp), False, density[other])
diff = [a[i] - b[i]]
return [1.0, diff[1], diff[2], diff[4], …, diff[13], a[3], b[3], 1.0/_race(tp, other), 1.0/_race(other, tp)]
```

That is 1 + 2 + 10 + 2 + 2 = 17 entries. `diff[0]` and `diff[3]` are dropped, and tempo is
always `+1.0`.

### 6.8 `burst_density(st, observer)` (evaluation.py:132-140)

For `owner` in `(0, 1)`: `pool, _ = unknown_pools(st, observer, owner)` (the main-deck def-id pool
the hidden cards can be; see 06b), then `bursts = count(def_id in pool with
R.cards[def_id].has_burst)`. `has_burst` is set when the card's own text has a 【Burst】 ability
(effects/registry.py:259-261). The density is `bursts / len(pool)` (int/int true division) if the
pool is non-empty, else `0.0`. The function returns `(d0, d1)`. Callers compute it once at the
root, from the true state.

### 6.9 `score`, `logistic`, `evaluate` (evaluation.py:303-326)

```
score(st, player, weights, density):
    if weights.feature_set == "roles":
        r = role_features(st, density); z = sum(w*x for w, x in zip(weights.values, r))
        return z if player == st.active else -z
    f = features(st, player, density); return sum(w*x for w, x in zip(weights.values, f))
logistic(z) = 1.0/(1.0 + exp(-z)) if z >= 0 else (e := exp(z)) / (1.0 + e)
evaluate(st, player, weights, density):
    if st.winner is not None: 1.0 if winner == player, 0.0 if winner == 1 - player, else 0.5 (draw = -1)
    else logistic(score(...))
```

- **`sum()` is CPython's Neumaier-compensated float sum** over the 14 or 17 products, taken in
  feature order. Each product `w*x` is rounded first. The reference algorithm is in 07 §12, which
  checked it against 3.15.1 on 200,000 lists:
  `f = 0.0 + p0; c = 0.0; for p in rest: t = f + p; c += (f - t) + p if |f| >= |p| else (p - t) + f; f = t;
  if c != 0 and finite(c): f += c`.
  A naive left fold is **wrong**.
- `-z` with `z == 0.0` gives `-0.0`. `logistic(-0.0)` takes the `z >= 0` branch, giving `0.5`.
- `exp` is libm `exp` (the same caveat as `ln` in 2.4).

### 6.10 `Weights` and the shipped tables (evaluation.py:62-129)

`Weights(values: tuple[float,...], feature_set: str = "base")`, frozen. `__post_init__` raises
`ValueError("unknown feature set 'x'; choose from ['base', 'roles']")` for an unknown set, and
`ValueError("expected 14 weights, got 3")` (or 17) for a wrong length. `as_dict()` gives
`{name: value}` in feature order.

`HAND_WEIGHTS` ("base"):
```
tempo 0.0, life 0.35, danger -0.30, can_kill 1.0, ap 0.10, hp 0.10, linked 0.20, ready 0.20,
hand_units 0.30, hand_pilots 0.25, hand_commands 0.25, hand_bases 0.25, level 0.15, bursts 0.30
```
`TUNED_WEIGHTS` ("base"; this is `BASELINE_WEIGHTS`, baseline.py:74):
```
tempo -0.2017, life 0.4285, danger -0.4398, can_kill 0.5341, ap 0.1131, hp 0.1574, linked 0.2101,
ready 0.532, hand_units 0.7157, hand_pilots 0.4885, hand_commands 0.3727, hand_bases 0.5741,
level 0.1643, bursts 0.5532
```
`ROLE_WEIGHTS` ("roles"; this is `DEFAULT_WEIGHTS`, the default for greedy and MCTS, evaluation.py:129):
```
tempo -0.7451, life 0.206, danger -0.459, ap 0.077, hp 0.231, linked 0.2152, ready 0.3452,
hand_units 0.7646, hand_pilots 0.5453, hand_commands 0.5425, hand_bases 0.5805, level 0.3053,
bursts 0.8482, can_kill_turn_player 1.2665, can_kill_other -0.418, inv_race_turn_player 3.2487,
inv_race_other -0.8534
```
Rust should parse these from the same decimal literals (Rust `f64` literal parsing is correctly
rounded, as Python's is). The bit patterns, printed with `float.hex()`:
- ROLE: `-0x1.7d7dbf487fcb9p-1, 0x1.a5e353f7ced91p-3, -0x1.d604189374bc7p-2, 0x1.3b645a1cac083p-4,
  0x1.d916872b020c5p-3, 0x1.b8bac710cb296p-3, 0x1.617c1bda5119dp-2, 0x1.8779a6b50b0f2p-1,
  0x1.17318fc504817p-1, 0x1.15c28f5c28f5cp-1, 0x1.29374bc6a7efap-1, 0x1.38a0902de00d2p-2,
  0x1.b2474538ef34dp-1, 0x1.44395810624ddp+0, -0x1.ac083126e978dp-2, 0x1.9fd566cf41f21p+1,
  -0x1.b4f0d844d013bp-1`.
- TUNED: `-0x1.9d14e3bcd35a8p-3, 0x1.b6c8b43958106p-2, -0x1.c25aee631f8a1p-2, 0x1.11758e219652cp-1,
  0x1.cf41f212d7732p-4, 0x1.425aee631f8a1p-3, 0x1.ae48e8a71de6ap-3, 0x1.10624dd2f1aa0p-1,
  0x1.6e703afb7e910p-1, 0x1.f4395810624ddp-2, 0x1.7da5119ce075fp-2, 0x1.25f06f6944674p-1,
  0x1.507c84b5dcc64p-3, 0x1.1b3d07c84b5ddp-1`.

---

## 7. Baseline players (agents.py:18-66)

Both use `AgentCore._begin` (base.py:60-65). It increments `_decisions` **before** seeding, so
the first decision uses 1, and the generator is
`SplitMix64(derive_seed(seed, name, "decision", _decisions))` with `name = "random"` or
`"greedy"`. Vector: `derive_seed(0, 'greedy', 'decision', 1) = 8131539456476094380` and
`derive_seed(0, 'random', 'decision', 1) = 11792374867952853053`.

### 7.1 `RandomAgent.choose` (agents.py:28-32)

`chosen = dec.options[rng.randrange(len(dec.options))]`. There is **no single-option shortcut**,
so `randrange(1)` still consumes one `next_u64`. `randrange(n)` is rejection sampling
(rng.py:30-38): `limit = 2^64 - (2^64 % n)`, then loop `x = next_u64()` until `x < limit`, and
return `x % n`. The alternatives are `[Alternative(o, 1.0/len(options), 0) for o in options]`.

### 7.2 `GreedyAgent.choose` (agents.py:48-66): one-ply look-ahead

```
dec, rng = _begin(st, player)
if len(dec.options) == 1:  return finish(options[0], [])                         # 0 draws
if dec.kind is REDRAW:     action, stats = mulligan_choice(st, player, 48, rng)   # GREEDY_MULLIGAN_SAMPLES (07 §13.2)
if dec.kind is CHOOSE_FIRST: return finish(Action(GO_FIRST, player), [])         # always goes first, 0 draws
det = determinize(st, player, rng.next_u64())          # exactly 1 draw: ONE determinization for all moves
density = burst_density(st, player)                    # true state (it reads only what `player` knows)
scored = []
for _, action in prune_dominated(st, dec, canonical(st, dec)):   # default flags: strict_attacks=False, rest_targets=False
    s = det.clone(); apply(s, action, check=False)
    play_until_turn(s, st.turn + 1, None, 0.0, 200)    # GREEDY_MAX_DECISIONS; BASELINE_KNOBS; argmax policy for BOTH players
    scored.append(Alternative(action, evaluate(s, player, self.weights, density), 1))
best = FIRST index of max value                        # max(range(len(scored)), key=...)
return finish(scored[best].action, scored)
```

- Every move starts from a clone of the same `det`, including `det.rng`, so all moves share the
  same random numbers. Engine randomness during the playout comes from the clone's `rng`.
- The playout stops at the first decision of turn `st.turn + 1`, when the game ends, or after 200
  moves. The state is then evaluated wherever it is. A finished game scores exactly
  `1.0 / 0.0 / 0.5`, with no win decay.
- Greedy defaults to `DEFAULT_WEIGHTS = ROLE_WEIGHTS`, so `evaluate` uses the turn player's
  viewpoint and negates when `player != s.active`.
- With `prune_dominated` defaults, MAIN drops attacks for which `_attack_can_matter` is false, and
  BURST drops "NO" on a free self-add 【Burst】. It always keeps at least one move (07 §10).
  `canonical` keeps the first key and the representative that uses the fewest EX (07 §8).
  Greedy calls neither `merge_equivalent` nor `noop`.
- The decision log is sorted by `(-visits, -value)` (all visits are 1, so value descending, stable
  among ties) and holds `round(value, 4)` (07 §14). The vector is in §11.

---

## 8. `selfplay.py`: game specs, mirrored blocks, worker pool

Constants (selfplay.py:22-23): `DECISION_LIMIT = 20000`, `MIRROR_BLOCK = 4`.

- `AgentSpec(kind: str, preset: str = "standard", config: SearchConfig | None = None, weights:
  Weights | None = None)` (selfplay.py:26-34).
- `GameSpec(index, game_seed, agent_seeds: (int, int), agents: (AgentSpec, AgentSpec), decks:
  (DeckList, DeckList), challenger: int)` (selfplay.py:37-47).
- `GameOutcome(index, challenger, winner, turns, first_player, decisions)` (selfplay.py:50-57).

`run_game(agents, decks, seed, observer=None)` (selfplay.py:60-75):
```
st = new_game(decks, seed)          # default chooser=None, turn_limit=200
decisions = 0
while st.pending is not None and decisions < 20000:
    if observer: observer(st)       # TRUE state, before every decision
    p = st.pending.player
    apply(st, agents[p].choose(st, p))    # check=True (legality checked)
    decisions += 1
return st, decisions
```

`play(spec)` (selfplay.py:78-85) builds seat 0 and then seat 1 with
`make_agent(a.kind, preset=a.preset, seed=agent_seeds[i], config=a.config, weights=a.weights)`. It
returns `GameOutcome(index, challenger, -1 if winner is None else winner, st.turn,
st.first_player, decisions)`. The engine itself sets `winner = -1` for draws (game.py:328, 738;
core.py:846).

`mirrored_specs(games, master, challenger, opponent, decks)` (selfplay.py:88-110). For
`i in range(games)`:
```
block, variant = divmod(i, 4); swap_decks, challenger_seat = divmod(variant, 2)
deck_pair = (decks[1], decks[0]) if swap_decks else decks
agents = (challenger, opponent) if challenger_seat == 0 else (opponent, challenger)
game_seed = derive_seed(master, "block", block, "decks", swap_decks)
agent_seeds = (derive_seed(master, "agent", i, 0), derive_seed(master, "agent", i, 1))
```

| i % 4 | swap | challenger seat | decks | challenger plays | game seed shared with |
| --- | --- | --- | --- | --- | --- |
| 0 | 0 | 0 | (d0, d1) | d0 from seat 0 | i%4 = 1 |
| 1 | 0 | 1 | (d0, d1) | d1 from seat 1 | i%4 = 0 |
| 2 | 1 | 0 | (d1, d0) | d1 from seat 0 | i%4 = 3 |
| 3 | 1 | 1 | (d1, d0) | d0 from seat 1 | i%4 = 2 |

`play_all(specs, workers)` (selfplay.py:113-118): serial if `workers <= 1`. Otherwise it uses
`ProcessPoolExecutor(spawn)` with `pool.map(play, specs, chunksize=1)`, which returns results in
spec order. Results do not depend on the worker count. Seed vectors:
`derive_seed(1,'block',0,'decks',0) = 1790404901509573176`, `(1,'block',0,'decks',1) =
3225975318560718282`, `(1,'agent',0,0) = 4336833438630444047`, `(1,'agent',0,1) =
17273804586326140040`.

`derive_seed(master, *path)` (rng.py:52-59) is
`int.from_bytes(sha256(repr((master & (2^64-1), *path)).encode())[:8], "big")`. The repr strings
used here look like `(1, 'block', 0, 'decks', 0)`: ints in decimal, str in single quotes, `", "`
separators. The `divmod` results are Python ints, so they print as `0`/`1`, never `False`/`True`.

---

## 9. `compare.py`: head-to-head CLI

`uv run python -m gcg_sim.ai.compare CHALLENGER OPPONENT [--games 400] [--seed 1] [--workers 1]
[--decks tests] [--out PATH]` (compare.py:199-214). The argparse description is the first
docstring line: "Head-to-head comparison of two AI versions in mirrored blocks of four games."

### 9.1 `parse_agent(text)` (compare.py:79-113)

1. `head, _, overrides = text.partition(":")` splits at the first colon.
2. The head is resolved in this order:
   - `head in ("random", "greedy")` → kind = head, no config.
   - `head in PRESETS` (fast, standard, strong) → `"mcts"` with `PRESETS[head]`.
   - `head` starts with `"baseline-"` and the rest is in `BASELINE_PRESETS` (standard, strong)
     → `"mcts"` with `BASELINE_PRESETS[rest]` and `weights = BASELINE_WEIGHTS` (= TUNED).
   - Anything else raises `ValueError("unknown agent 'x'; choose from ['random', 'greedy', 'fast',
     'standard', 'strong', 'baseline-standard', 'baseline-strong']")`.
3. `for item in filter(None, overrides.split(","))` (empty items are skipped).
   `field, sep, value = item.partition("=")`.
   - If `sep` is missing: `ValueError("expected field=value, got 'x'")`.
   - `field == "weights"`: if `value` is not in `WEIGHTS` or kind is `"random"`, raise
     `ValueError("weights must be one of ['baseline', 'default', 'hand', 'roles'] for greedy or MCTS")`.
     The table is `WEIGHTS = {"default": ROLE, "hand": HAND, "baseline": TUNED, "roles": ROLE}`
     (compare.py:52-57).
   - If `config is None` (random or greedy) or the field is not a `SearchConfig` field:
     `ValueError("'greedy' has no search setting 'exploration'")`.
   - Otherwise `changes[field] = converter(value)`. Field types come from
     `str(dataclasses.field.type)`. These are the string annotations `'int' | 'float' | 'str' |
     'bool'`, because config.py uses `from __future__ import annotations`. The converters are
     Python `int`, `float`, `str`, and `_flag`. `_flag` lowercases the text, maps
     `"1"/"true"/"yes"` to True and `"0"/"false"/"no"` to False, and otherwise raises
     `ValueError("not a boolean: 'x'")`. A repeated field: the last value wins.
4. `config = dataclasses.replace(config, **changes)` runs only if both are present, and it re-runs
   `__post_init__`. The function returns `AgentSpec(kind, config=config, weights=weights)`, with
   preset left at `"standard"`.

An override `name=...` changes the agent name `mcts-<name>`, and therefore every per-decision seed.

### 9.2 `load_decks(ref)` (compare.py:116-124)

- `"examples"`: `sorted(Path("examples/decks").glob("*.txt"))`, relative to the cwd. This
  currently gives blue-white-federation, purple-white-tekkadan, red-green-zeon.
- `DECK_SETS`: `"tests"` → `"tests.ai.decks:DECKS"` (FEDERATION, SEED; the cwd must be the repo
  root) and `"tuning"` → `"gcg_sim.ai.tuning:TUNING_DECKS"`.
- If `":" in ref and not ref.endswith(".txt")`, the result is `getattr(import_module(mod), attr)`
  converted to a tuple. Otherwise it is a comma-separated list of deck files read with
  `to_decklist(load_deck(p))`.

The Rust port has to embed the `tests` and `tuning` deck lists. Python module refs cannot be
ported.

### 9.3 `comparison_specs(games, seed, challenger, opponent, decks)` (compare.py:127-143)

`pairs = list(itertools.combinations(decks, 2))`, which gives (0,1), (0,2), …, (1,2), ….
- If there are no pairs: `ValueError("need at least two decks")`.
- If `games <= 0 or games % (4*len(pairs))`:
  `ValueError("games must be a positive multiple of {4*len(pairs)}")`.
- With exactly one pair, `mirrored_specs(games, seed, …)` uses the seed itself.
- Otherwise, for each pairing `p`: `master = derive_seed(seed, "pairing", p)`, then
  `mirrored_specs(games // len(pairs), master, …)`. Each spec is re-indexed with
  `dataclasses.replace(s, index=len(out))`. Its agent seeds keep the **local** `i` within the
  pairing. Vector: `derive_seed(1,'pairing',0) = 6922874961443424655`.

### 9.4 `summarize(specs, outcomes)` and `_rate` (compare.py:146-178)

```
_rate(wins, n) = {"wins": wins, "n": n, "rate": round(wins/n, 4) if n else None,
                  "ci95": [round(lo, 4), round(hi, 4)]}       # (lo, hi) = wilson(wins, n), z = 1.96
won[i] = outcomes[i].winner == outcomes[i].challenger         # draws (-1) are non-wins
result = {**_rate(sum(won), len(outcomes)),                   # keys: wins, n, rate, ci95
          "draws": count(winner == -1),
          "mean_turns": round(sum(turns) / max(1, len), 2),
          "by_challenger_seat": {"0": _rate(...), "1": _rate(...)},
          "by_challenger_deck": {"deck0": …, "deck1": …}}
```

`by_challenger_deck` names each distinct `spec.decks[spec.challenger]` (DeckList value equality)
as `deck{k}`, in order of first appearance across the specs. Two equal decks share one name.
`wilson` (reports/stats.py:12-25):
```
p = s/n; z2 = z*z (= 3.8415999999999997); denom = 1 + z2/n
centre = (p + z2/(2*n)) / denom
half = (z * sqrt((p*(1-p))/n + z2/(4*n*n))) / denom
lo = 0.0 if s == 0 else max(0.0, centre - half); hi = 1.0 if s == n else min(1.0, centre + half)
n == 0 → (0.0, 1.0)
```

### 9.5 `compare()` report and output (compare.py:181-214)

The report keys, in order: `challenger`, `opponent`, `decks` (the raw strings), `seed`, `games`,
`result`, and `outcomes` (`[[challenger, winner, turns, first_player], …]` in spec order).
- `--out` writes `json.dumps(report, indent=1) + "\n"`.
- stdout gets `json.dumps(report without "outcomes", indent=2)` and print's newline.

Python's `json.dumps` with an indent:
- Every list and dict element goes on its own line, indented by `indent × depth` spaces, with `,`
  ending each line. The key separator is `": "`. Empty containers print as `[]` / `{}`.
- Strings are escaped with `ensure_ascii=True`. Floats use `float.__repr__` (`0.0`, `0.4899`,
  `1e-05`). `None` prints as `null`.

A vector is in §11.

---

## 10. `tuning.py` and `throughput.py` CLIs (brief, exact)

### 10.1 Tuning data (tuning.py:50-98)

`TUNING_CONFIG = SearchConfig(name="tuning", max_iterations=48, min_iterations=16,
iterations_per_option=6, first_player_games=8, mulligan_samples=32)`. Every other field keeps its
dataclass default: exploration 0.3, prior_weight 0.5, prior_temperature 1.5, horizon 2,
rollout_temperature 0.5, value_scale 2.0, max_rollout_decisions 250, policy_burst_yes_score 1.0,
all bool flags False, target_extension 1.0, target_extension_z 2.0.

Other constants:
- `START_WEIGHTS = {"hand": HAND, "tuned": TUNED}`, `L2 = 1.0`, `NEWTON_STEPS = 30`.
- `RESOURCES = ("R-001",)*10`.
- The decks: `ZEON_COUNTS`, `WING_COUNTS`, `TEKKADAN_COUNTS` (tuning.py:65-82, each 50 cards).
  `_deck(counts)` = `DeckList(tuple(n for n, c in sorted(counts.items()) for _ in range(c)),
  RESOURCES)`, which sorts by card-number string.
- `TUNING_DECKS = (zeon, wing, tekkadan)`.
- `MATCHUPS` is all 9 ordered pairs `(a, b)` for `a in 0..3`, `b in 0..3`, mirrors included, in
  row-major order.
- `Sample = (tuple[float, ...], float)`.

### 10.2 Self-play collection (tuning.py:101-169)

- `tuning_games(games, master, weights, config, feature_set)` builds, for each `i`:
  - `MATCHUPS[i % 9]`;
  - `seed = derive_seed(master, "tuning-game", i)`;
  - `agent_seeds = (derive_seed(master, "agent", i, 0), derive_seed(master, "agent", i, 1))`;
  - the same weights for both seats.
- `collect(game)`: two `MctsAgent(agent_seeds[k], config=config, weights=weights[k])`. The
  observer fires on the **first `MAIN` decision of each turn**. It checks
  `dec.kind is MAIN and st.turn != last_turn`, with `last_turn` starting at 0, and records
  `(st.active, features)`:
  - "roles": `tuple(role_features(st, density))`;
  - otherwise `tuple(features(st, st.active, density))`;
  - in both cases with `density = burst_density(st, st.active)` on the TRUE state.

  When the game ends, the function returns `[]` if `st.winner not in (0, 1)` (draws and unfinished
  games). Otherwise it returns `[(x, 1.0 if player == winner else 0.0)]` in recording order.
- `play_games` serial or spawn-pool `map`, in index order.

### 10.3 Logistic regression (tuning.py:176-231), all pure Python floats

```
log_loss(samples, w):
    total = 0.0
    for x, y in samples:
        p = min(1.0 - 1e-12, max(1e-12, logistic(Σ_neumaier(a*b for a, b in zip(w, x)))))   # 1.0-1e-12 == 0.999999999999
        total -= (y*log(p) + (1.0 - y)*log(1.0 - p))
    return total / max(1, len(samples))

fit_logistic(samples, l2=1.0, steps=30, feature_set="base"):
    n = len(FEATURE_SETS[feature_set]); w = [0.0]*n
    repeat steps times:
        grad = [l2*wi for wi in w]; hess = [[l2 if i == j else 0.0 for j] for i]
        for x, y in samples (in order):
            p = logistic(Σ_neumaier(w[k]*x[k])); r = p - y; s = p*(1.0 - p)
            for i in 0..n:
                xi = x[i]; if xi == 0.0: continue          # skip the whole row (also for -0.0)
                grad[i] += r*xi                            # plain sequential +=
                sxi = s*xi
                for j in 0..n: hess[i][j] += sxi*x[j]      # (s*xi)*x[j], NOT s*(xi*x[j])
        step = _solve(hess, grad)
        w = [wi - si]
        if max(|si|) < 1e-9: break                         # checked AFTER the update
    return Weights(tuple(round(wi, 4) for wi in w), feature_set)

_solve(a, b):   # Gaussian elimination with partial pivoting on a copy [row + [rhs]]
    for col in 0..n:
        pivot = FIRST r in col..n maximising |m[r][col]|  # max(range, key) → ties to lowest index
        swap m[col], m[pivot]
        for r in col+1..n:
            f = m[r][col] / m[col][col]
            if f:                                          # skip when f == ±0.0
                for c in col..=n: m[r][c] -= f*m[col][c]   # includes c == col
    for r in n-1 down to 0:
        x[r] = (m[r][n] - Σ_neumaier(m[r][c]*x[c] for c in r+1..n)) / m[r][r]   # empty Σ is int 0
```

### 10.4 The `tune` / `validate` drivers and the CLI (tuning.py:303-402)

`tune(games, rounds, seed, workers, start, config, feature_set)`. For each round `r`:
1. `specs = tuning_games(games, derive_seed(seed, "round", r), weights, config, feature_set)`.
2. `pooled_train` gets the samples of the even-indexed games and `pooled_all` gets all samples, in
   game order. `test` is the samples of the odd-indexed games.
3. `fitted = fit_logistic(pooled_train)`.
4. `before = log_loss(test, weights.values)` if `weights.feature_set == feature_set`, else None.
5. The round record, with keys in this order: `{"round", "positions" (Σ samples this round),
   "heldout_logloss_before" (round 4 or None), "heldout_logloss_after" (round 4), "weights"
   (as_dict)}`.
6. `weights = fitted`.

After the rounds: `final = fit_logistic(pooled_all)` if any samples were collected, else the
current weights. The report keys, in order: `seed, games, rounds, positions, weights (list),
feature_set`, then `validation` if requested, then `features` (the name list).

`duel_games(games, master, challenger, opponent, config)`. For each `i`:
- `block, variant = divmod(i, 4)` and `swap, seat = divmod(variant, 2)`;
- matchup `MATCHUPS[(block + swap) % 9]`, which swaps the matchup and not the deck order;
- `seed = derive_seed(master, "duel", block, swap)`;
- `agent_seeds = derive_seed(master, "duel-agent", i, k)`.

A duel scores 1.0 for a challenger win, 0.5 when `winner not in (0, 1)`, and 0.0 otherwise.
`validate` returns `{"games", "preset": config.name, "tuned_score": sum(scores) (Neumaier; exact for
halves), "tuned_rate": round(sum/max(1, games), 4)}`, with
`master = derive_seed(seed, "validate")` (= `6938204797002195958` for seed 2026). It always plays
against `HAND_WEIGHTS`. The game count is not checked against multiples of 4.

CLI flags: `--games 240 --rounds 2 --seed 2026 --workers 1 --validate 0 --validate-preset tuning
--preset tuning --start hand --feature-set base`. The preset choices are
`["tuning", "fast", "standard", "strong", "baseline-standard", "baseline-strong"]`.
`preset_config` maps `"tuning"` to `TUNING_CONFIG` and `baseline-X` to `BASELINE_PRESETS[X]`;
any other name is a key of `PRESETS`. The CLI prints `json.dumps(report, indent=2)`. Vector:
`derive_seed(2026,'round',0) = 12737583975458888324`.

### 10.5 `throughput.py` (throughput.py:21-58)

`measure(preset, games, workers, seed=0, decks=(TUNING_DECKS[0], TUNING_DECKS[1]))`:
`AgentSpec("mcts", preset)` plays itself through `mirrored_specs(games, seed, …)`, with
`workers = max(1, min(workers, games))`. It reads the wall clock with `time.perf_counter()` only to
report speed. The output keys, in order:
- `preset`, `games`, `workers`;
- `wall_seconds` (round 1), `games_per_core_minute` (= `round(games / (wall*workers/60.0), 3)`),
  `core_seconds_per_game` (round 1);
- `mean_turns` (= `round(Σturns/games, 1)`), `mean_decisions` (round 1).

`games=0` raises ZeroDivisionError. The CLI flags are `--preset standard` (choices fast, standard,
strong), `--games 8`, `--workers 1`, `--seed 0`, and it prints with `indent=2`. Only `mean_turns`
and `mean_decisions` are deterministic.

---

## 11. Acceptance vectors (run on this machine, Python 3.15.1)

The scripts were run from the repo root with `uv run python <script>`. They are reproduced
verbatim in §11.1. `vectors.py` and `cover.py` import `DECKS` from `tests/ai/decks.py`, which
they put on `sys.path`.

1. **`pick`**: `options = [PASS(0..3)]`, `scores = [1.0, 2.0, 0.5, 2.0]`, `SplitMix64(42)`,
   `T = 0.5`. The result is **index 3**, and the generator state afterwards is
   `8709371129873690750` (4 draws). The `(u, v)` pairs as hex:
   `(0x1.7bae644c5fd6dp-1, 0x1.9a89ff8e6137ap+1)`, `(0x1.477f199d93378p-3, 0x1.b26d916488140p+1)`,
   `(0x1.1d499d5c4c3e6p-2, 0x1.826b2a882fb8cp-1)`, `(0x1.607387fc392b8p-2, 0x1.f7c07f8759084p+1)`.
   With `rng=None` the result is **index 1** (the first of the tied maxima). `SplitMix64(42)`'s
   first three `next_u64` values are `13679457532755275413, 2949826092126892291,
   5139283748462763858`.
2. **`logistic`**:

   | z | hex | repr |
   | --- | --- | --- |
   | 0.0 and -0.0 | `0x1.0p-1` | 0.5 |
   | 2.0 | `0x1.c2f7d5a8a79c9p-1` | 0.8807970779778823 |
   | -2.0 | `0x1.e84152bac31aep-4` | 0.11920292202211755 |
   | 0.3 | `0x1.261d545e46a8bp-1` | 0.574442516811659 |
   | -37.5 | `0x1.dd5c566301ec8p-55` | 5.175555005801869e-17 |
   | 800.0 | | 1.0 |
   | -800.0 | | 0.0 |

3. **One random-vs-random game**: `RandomAgent(7)` vs `RandomAgent(8)`,
   `new_game(DECKS, 12345)`. It lasts 101 decisions, winner 1, turn 25.
   - Before each decision the script records a dict with `n`, `kind`, `base` (option_scores
     with BASELINE_KNOBS), `adopted` (`knobs_for(PRESETS["standard"])`), `density`, `features`,
     `roles`, `score_roles`, `score_tuned` and `eval_roles`. Floats are written as
     `float.hex()`; the scores and features are computed with `player = dec.player`.
   - The SHA-256 of the concatenated `json.dumps(rec, sort_keys=True)` is
     `ed729d1efd8bd75a513fb2a89eba83ced433d8d4c2ca86fa5342f22999aa45f3`.
   - Decision 10 (`main`, 3 options): `base = [4.0, 2.2, 0.0]`, density
     `[0x1.5555555555555p-2, 0x1.c28f5c28f5c29p-3]`, `score_roles = 0x1.5234c474804c3p+0`,
     `score_tuned = 0x1.0e8f6cefed636p+0`.
   - Decision 50 (`main`): `base = [0x1.8cccccccccccdp+2, 0x1.e666666666666p+2,
     0x1.8cccccccccccdp+2, 0x1.0cccccccccccdp+1, 0x1.0cccccccccccdp+1, 0.0]`,
     `score_roles = 0x1.666920b1e7570p+0`.
4. **20 random games** (`RandomAgent(100+g)` vs `RandomAgent(200+g)`, `new_game(DECKS, 5000+g)`,
   `g = 0..19`). The decision counts are choose_first 20, redraw 40, main 1483, action_step 114,
   select 169, block 71, burst 35, excess 11, order_trigger 19, discard 11. The adopted knobs
   differ from the baseline knobs on every burst decision and on 1 main decision. The SHA-256 of
   the per-decision records `json.dumps([kind, base hex list, adopted hex list, score_roles hex,
   score_tuned hex])` is `18b42cb9d7917ddcbe4af568dc842d779bbc67ad396f29a146c6793df95e856e`.
5. **Policy playouts from `new_game(DECKS, 99)`**:

   | Call | Moves | Winner | Turn | Generator state after |
   | --- | --- | --- | --- | --- |
   | `play_to_end(st, SplitMix64(5), 0.5, 3000, adopted)` | 124 | 1 | 26 | `7509503583082984561` |
   | the same call with BASELINE_KNOBS | 125 | 1 | 26 | `11864189148019829915` |
   | `play_to_end(st, None, 0.0, 3000)` | 116 | 0 | 23 | — |

6. **Greedy vs greedy**: `GreedyAgent(1, log=True)` vs `GreedyAgent(2, log=True)` through
   `run_game(..., DECKS, 777)`. It lasts 69 decisions, winner 1, turn 11. The SHA-256 of
   `json.dumps([log0, log1])` is
   `9241c6142b26faadd6d1b782c7f4aed57f259331a9090fb35090971eb2d67b62`. The second log entry of
   player 0 (turn 2, main) chose `end_main` with alternatives `end_main 0.6199`,
   `play_unit 0 0.611`, `play_base 3 0.4769`, `play_base 2 0.4598`. The first entry (turn 0,
   redraw) chose keep, with alternatives redraw `18.2344` (48 visits) and keep `20.5` (1 visit).
7. **Random vs random**: `RandomAgent(1)` vs `RandomAgent(2)` through `run_game(..., DECKS, 777)`.
   It lasts 99 decisions, winner 1, turn 19.
8. **compare**: `uv run python -m gcg_sim.ai.compare random greedy --games 4 --seed 1 --out F`.
   The challenger wins 0 of 4, with `ci95 [0.0, 0.4899]`, per seat and per deck
   `ci95 [0.0, 0.6576]`, and `mean_turns 13.5`. The outcomes are
   `[[0,1,13,1],[1,0,17,0],[0,1,13,1],[1,0,11,0]]`. The file's SHA-256 is
   `c5297e8e7f244ce126c2d51714faa594101d2f6730dc5cf524283ff57c03c90e`.
9. **Wilson / `_rate`**:

   | (s, n) | ci95 |
   | --- | --- |
   | (0, 4) | `[0.0, 0.4899]` |
   | (1, 4) | `[0.0456, 0.6994]` (raw `0x1.7570e706c9af8p-5`, `0x1.66130805854aap-1`) |
   | (4, 4) | `[0.5101, 1.0]` |
   | (217, 400) | `[0.4935, 0.5907]`, rate 0.5425 |

10. **`fit_logistic`**. The synthetic data is
    `r = SplitMix64(3)` and, for `i in range(200)`:
    `x = (1.0 if i % 2 else -1.0,) + tuple(float(r.randrange(7)) - 3.0 for _ in range(13))` and
    `y = 1.0 if r.random() < 0.5 + 0.05*x[1] else 0.0`.
    - Weights: `(-0.3224, 0.1385, -0.0176, 0.0401, -0.0887, -0.0033, -0.1171, 0.0187, 0.1124,
      0.1919, 0.0065, -0.2394, 0.005, -0.0367)`.
    - `log_loss` = `0x1.3b23218d0e1bfp-1` (0.615502403706209).
    - `_solve([[4,1,0],[1,3,1],[0,1,2]], [1,2,3])` = `[0x1.c71c71c71c71dp-3,
      0x1.c71c71c71c717p-4, 0x1.71c71c71c71c8p+0]`.
11. **Float constants that must come out bit-identical**: `2.5-0.3` → `2.2`;
    `(2.5-0.3)-0.4` → `1.8000000000000003`; `1.2+2.0` → `3.2`; `1.0-1e-12` → `0.999999999999`;
    `1.96*1.96` → `3.8415999999999997`.

### 11.1 Generator scripts (verbatim, so the digests can be regenerated)

<details><summary><code>vectors.py</code> (verbatim; run from the repo root with <code>uv run python</code>)</summary>

```python
"""Acceptance vectors for the policy / evaluation port (08-ai-policy-eval.md)."""

import hashlib
import json
import math
import sys

sys.path.insert(0, "tests/ai")

from gcg_sim.ai import evaluation as E
from gcg_sim.ai import policy as P
from gcg_sim.ai.agents import RandomAgent
from gcg_sim.ai.config import PRESETS
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.state import Action
from gcg_sim.engine.types import ActionKind
from gcg_sim.rng import SplitMix64

from decks import DECKS  # noqa: E402

# 1. pick() with a fixed generator
opts = [Action(ActionKind.PASS, i) for i in range(4)]
scores = [1.0, 2.0, 0.5, 2.0]
rng = SplitMix64(42)
trace = []
r2 = SplitMix64(42)
for s in scores:
    u = r2.random() or 1e-300
    v = s / 0.5 - math.log(-math.log(u))
    trace.append((u.hex(), v.hex()))
print("pick", P.pick(opts, scores, rng, 0.5), rng.state, trace)
print("argmax", P.pick(opts, scores, None, 0.5))

# 2. a random-vs-random game: at every decision dump policy scores and evaluation numbers
adopted = P.knobs_for(PRESETS["standard"])
agents = (RandomAgent(7), RandomAgent(8))
st = new_game(DECKS, 12345)
h = hashlib.sha256()
n = 0
samples = []
while st.pending is not None and n < 400:
    dec = st.pending
    dens = E.burst_density(st, dec.player)
    rec = {
        "n": n,
        "kind": dec.kind.value,
        "base": [x.hex() for x in P.option_scores(st, dec)],
        "adopted": [x.hex() for x in P.option_scores(st, dec, adopted)],
        "density": [x.hex() for x in dens],
        "features": [x.hex() for x in E.features(st, dec.player, dens)],
        "roles": [x.hex() for x in E.role_features(st, dens)],
        "score_roles": E.score(st, dec.player, E.ROLE_WEIGHTS, dens).hex(),
        "score_tuned": E.score(st, dec.player, E.TUNED_WEIGHTS, dens).hex(),
        "eval_roles": E.evaluate(st, dec.player, E.ROLE_WEIGHTS, dens).hex(),
    }
    h.update(json.dumps(rec, sort_keys=True).encode())
    if n in (0, 10, 50, 120):
        samples.append(rec)
    p = dec.player
    apply(st, agents[p].choose(st, p))
    n += 1
print("decisions", n, "winner", st.winner, "turn", st.turn)
print("digest", h.hexdigest())
for r in samples:
    print(json.dumps(r))

# 3. a seeded policy playout (Gumbel sampling) to the end
from gcg_sim.ai.playout import play_to_end, play_until_turn
from gcg_sim.ai.agents import GreedyAgent
from gcg_sim.ai.selfplay import run_game, mirrored_specs, AgentSpec, GameOutcome
st = new_game(DECKS, 99)
r = SplitMix64(5)
moves = play_to_end(st, r, 0.5, 3000, adopted)
print("playout adopted", moves, st.winner, st.turn, r.state)
st = new_game(DECKS, 99)
r = SplitMix64(5)
moves = play_to_end(st, r, 0.5, 3000)
print("playout baseline", moves, st.winner, st.turn, r.state)
st = new_game(DECKS, 99)
moves = play_to_end(st, None, 0.0, 3000)
print("playout argmax", moves, st.winner, st.turn)
st = new_game(DECKS, 99)
moves = play_until_turn(st, 3, None, 0.0, 200)
print("until turn 3", moves, st.turn, st.pending.kind.value)

# 4. greedy vs greedy with decision logs
g0, g1 = GreedyAgent(1, log=True), GreedyAgent(2, log=True)
st, decisions = run_game((g0, g1), DECKS, 777)
logs = json.dumps([g0.decision_log(), g1.decision_log()])
print("greedy", decisions, st.winner, st.turn, hashlib.sha256(logs.encode()).hexdigest())
print("greedy first log entries", json.dumps(g0.decision_log()[:3]))

# 5. random vs random
r0, r1 = RandomAgent(1, log=True), RandomAgent(2, log=True)
st, decisions = run_game((r0, r1), DECKS, 777)
print("random", decisions, st.winner, st.turn)
```

</details>

<details><summary><code>cover.py</code> (verbatim; run from the repo root with <code>uv run python</code>)</summary>

```python
import collections, hashlib, json, sys
sys.path.insert(0, "tests/ai")
from decks import DECKS
from gcg_sim.ai import evaluation as E, policy as P
from gcg_sim.ai.agents import RandomAgent
from gcg_sim.ai.config import PRESETS
from gcg_sim.engine.game import apply, new_game

adopted = P.knobs_for(PRESETS["standard"])
kinds = collections.Counter()
diffs = collections.Counter()
h = hashlib.sha256()
for g in range(20):
    agents = (RandomAgent(100 + g), RandomAgent(200 + g))
    st = new_game(DECKS, 5000 + g)
    while st.pending is not None:
        dec = st.pending
        b = P.option_scores(st, dec)
        a = P.option_scores(st, dec, adopted)
        kinds[dec.kind.value] += 1
        if a != b:
            diffs[dec.kind.value] += 1
        dens = E.burst_density(st, dec.player)
        rec = [dec.kind.value, [x.hex() for x in b], [x.hex() for x in a],
               E.score(st, dec.player, E.ROLE_WEIGHTS, dens).hex(),
               E.score(st, dec.player, E.TUNED_WEIGHTS, dens).hex()]
        h.update(json.dumps(rec).encode())
        apply(st, agents[dec.player].choose(st, dec.player))
print(dict(kinds))
print("adopted != baseline:", dict(diffs))
print(h.hexdigest())
```

</details>

<details><summary><code>tune_vec.py</code> (verbatim; run from the repo root with <code>uv run python</code>)</summary>

```python
import json
from gcg_sim.rng import SplitMix64
from gcg_sim.ai import tuning as T
from gcg_sim.ai.compare import _rate, parse_agent, comparison_specs
from gcg_sim.reports.stats import wilson
from gcg_sim.ai.selfplay import mirrored_specs, AgentSpec

r = SplitMix64(3)
samples = []
for i in range(200):
    x = (1.0 if i % 2 else -1.0,) + tuple(float(r.randrange(7)) - 3.0 for _ in range(13))
    y = 1.0 if r.random() < 0.5 + 0.05 * x[1] else 0.0
    samples.append((x, y))
w = T.fit_logistic(samples)
print("fit", w.values)
print("logloss", T.log_loss(samples, w.values).hex(), T.log_loss(samples, w.values))
print("solve", [v.hex() for v in T._solve([[4.0, 1.0, 0.0], [1.0, 3.0, 1.0], [0.0, 1.0, 2.0]], [1.0, 2.0, 3.0])])
for s, n in [(0, 4), (1, 4), (4, 4), (217, 400), (0, 0)]:
    print("wilson", s, n, [v.hex() for v in wilson(s, n)], _rate(s, n) if n else None)
print(parse_agent("standard:exploration=0.2,weights=hand"))
print(parse_agent("baseline-standard").config.name, parse_agent("greedy"))
for bad in ["foo", "greedy:exploration=1", "random:weights=hand", "standard:x", "standard:weights=zzz"]:
    try:
        parse_agent(bad)
    except ValueError as e:
        print("err", bad, "->", e)
```

</details>

---

## 12. Hazards (Python-semantic traps), with path:line

**First-maximum and tie rules**
- policy.py:93: `max(range(n), key=scores.__getitem__)` returns the **first** maximal index.
  Rust's `Iterator::max_by` returns the last. Use a manual loop with a strict `>`.
- agents.py:65: the greedy `best` is the first maximal value (same rule).
- tuning.py:221: the `_solve` pivot is the first maximal `|m[r][col]|` (lowest row on ties).
- policy.py:99: the Gumbel argmax uses strict `>` from `-inf`. Earlier options win ties.
- policy.py:226: `max(worst, x)` keeps `worst` when the two are equal. The value is identical, so
  this has no visible effect, but do not use `f64::max` if signed zero could matter. Here it
  cannot (`x == 0` is always `+0.0`).
- evaluation.py:219: `max(0.0, DANGER_LIFE - life)` returns `0.0` unless the second argument is
  `> 0.0`.
- tuning.py:179: `max(1e-12, ·)` and `min(1-1e-12, ·)` replace the value only when it is strictly
  greater (or smaller) than the bound.

**Sampling and RNG**
- policy.py:97: `rng.random() or 1e-300`. The value `0.0` is falsy, so `u = 0.0` becomes `1e-300`.
- policy.py:98: `math.log` and evaluation.py:305-306 `math.exp` are C libm calls. Byte identity
  needs the same libm (Apple libm on macOS, glibc on Linux). `f64::ln` / `f64::exp` call the
  platform libm. Do not use the `libm` crate.
- policy.py:81-82: a single option returns before any RNG draw. RandomAgent (agents.py:30) **does**
  draw for a single option.
- agents.py:57: greedy makes exactly one `next_u64` per multi-option non-REDRAW, non-CHOOSE_FIRST
  decision, and none in its playouts (`rng=None`).

**Float evaluation order**
- policy.py:170-205, 239-249, 269-283; values.py:25-32, 40-46: the order of float operations is
  exactly as written; see §4 and §3. `card_worth` changes for 134 of 1152 real cards under
  re-association. Policy subtractions are sequential, e.g. `(2.5 - 0.3) - 0.4 =
  1.8000000000000003`.
- policy.py:244, 272, 371: the conditional expression binds loosest:
  `s = (3.0 + 0.5*v) if kills else 0.3`, `saved = (1.0 + 0.5*v) if kills else 0.0`,
  `wants = (pol == HELPS) if mine else (pol != HELPS)`.

**Neumaier `sum()`**
- evaluation.py:314 and 317 (`score`), tuning.py:179 and 197 (dot products), tuning.py:230 (back
  substitution) and tuning.py:362-363 (`sum(scores)`) are all CPython float `sum()`, which uses
  Neumaier compensation. See 07 §12 for the exact algorithm, including `0 + x0`, the final
  `if c != 0 and isfinite(c)`, and an empty sum giving int `0`.
- The plain `+=` accumulations in tuning.py:204 and 208 are **not** compensated.
- Integer sums (`sum(1 for …)`, `sum(through)`, base HP, `sum(won)` of bools, turns) are exact
  ints.

**Integer → float divisions**
- evaluation.py:139, 274, 277 and compare.py:151, 175 divide an int by an int (`/`), which is
  correctly rounded for these sizes. `(a as f64) / (b as f64)` matches.
- evaluation.py:275 divides an int by a float and applies `ceil`. evaluation.py:192 and values.py:87
  compute `int + int/3.0`.

**`round()`**
- tuning.py:213, 335-336, 363; compare.py:151-152, 175; throughput.py:43-47; base.py:95 use
  Python `round(x, nd)`. It is correctly rounded from the exact binary value (via dtoa) and then
  converted back to a float. Rust `format!("{:.4}")` followed by a parse matched Python on
  8 samples, including exact ties (see Open question 1).
- `round(x, 4)` can also yield `-0.0`, which JSON prints as `-0.0`.

**JSON**
- compare.py:213-214, tuning.py:402, throughput.py:58: `json.dumps` with `indent=1` or `indent=2`,
  default separators (`","` + newline, `": "`), `ensure_ascii=True`, and float `repr`
  (`1e-05`, `0.0`, `1.0`). `None` prints as `null`. Python `int` and `bool` values never mix here.
- The key order is dict insertion order (§9.5, §10.4).

**Lenient CLI parsing**
- compare.py:71-74: Python's `int()` and `float()` accept surrounding whitespace, `_` digit
  separators, a leading `+`, Unicode digits, and (`float`) `inf`/`nan`/`infinity`.
- `_flag` uses `str.lower()`, which is Unicode lowercasing.
- `text.partition(":")` splits at the **first** colon. `filter(None, …)` drops empty items. For a
  repeated field, the last value wins.

**Annotation strings**
- compare.py:76: `str(f.type)` works only because config.py has
  `from __future__ import annotations` (the types are strings `'int'`, `'float'`, `'str'`,
  `'bool'`).

**Ordering**
- compare.py:119: `sorted(Path.glob)` sorts paths by their parts, which is plain string order for
  one directory. compare.py:131: `itertools.combinations` order. tuning.py:86: `sorted(dict.items())`
  sorts by card-number string. tuning.py:94: the MATCHUPS order is row-major.
- compare.py:166-171: `deck_names` is a dict keyed by `DeckList` (`hash` of tuples of str, which is
  salted per process). Only membership is used. The names follow insertion order, so the output
  is deterministic. Equal decks collapse into one name.

**`hash()` and `id()`**
- policy.py:63 (`@cache` on `knobs_for`), targeting.py:54-55 and 93 (module-level memo dicts), and
  `_HAND_SLOT` (evaluation.py:159, keyed by a StrEnum whose hash is the str hash) are pure
  memoisation or lookup. They do not affect any result or any order.
- No `id()`-based ordering in scope.

**Unordered collections**
- policy.py:147-153: a set comprehension, used only for `len`. evaluation.py:171 and values.py:26
  test membership in `dv.linked` (a set). There is no iteration over sets or frozensets in scope.
- `dv.kw` dict: `len(kws)` (values.py:31-32) counts **distinct keyword keys**, not amounts.

**Negative indexes**
- policy.py:366 and 382: the guard `0 <= o.a < len(st.cards)` prevents Python's negative indexing
  (`st.cards[-1]` would silently return the last card). Elsewhere (`_excess`, `_discard`, ARRANGE
  order mode, BLOCK `o.a`, and `st.cards[target]` in policy.py:264 when the target is not -2) the
  engine guarantees uid arguments. A Rust port indexing with `usize` should assert this.
- policy.py:338: `ctx("card")` returns `-1` when the key is missing.

**Unbounded ints and seed masking**
- rng.py:21, 58: CLI `--seed` values are Python ints of any size. They are masked
  `& (2^64-1)` before seeding and inside `derive_seed`'s repr. A seed of `-1` therefore repr's as
  `18446744073709551615`.
- `derive_seed` repr of a str path element uses Python `repr`: single quotes, but double quotes if
  the string contains `'` and no `"`, with backslash escapes. Non-ASCII printable characters stay
  literal and are UTF-8 encoded. This matters for a user `name=` override in compare
  (compare.py:110).

**Mutable defaults, closures, exceptions**
- No mutable defaults in scope: the default arguments `BASELINE_KNOBS`, `TUNING_CONFIG` and the
  `Weights` objects are frozen.
- tuning.py:146-152: the closure mutates the list `last_turn`. That is intended.
- Exceptions are used only for errors (ValueError on parse or validation), never for control flow.

**Uneven division**
- compare.py:141: `games // len(pairs)` is exact, because divisibility was checked first.
- throughput.py:44-47: division by `games` raises ZeroDivisionError for `--games 0`.
- tuning.py:363: `max(1, games)`.

**State aliasing**
- agents.py:61: greedy clones `det` per move, and `apply` mutates in place. A Rust port must clone
  the world, including its rng, per move.
- playout.py: both functions mutate their argument.

---

## Open questions

1. **Exact `round(x, ndigits)` emulation.** It is only partly verified. On 8 samples, including
   the exact binary ties `0.125→2`, `0.375→2`, `2.5→0`, `0.5→0`, `1.5→0` and `0.0625→3`, Rust
   1.99.1 `format!("{:.*}", n, x)` followed by a parse matched Python `round` (`0.12`, `0.38`,
   `2`, `0`, `2`, `0.062`; `-0.00001→4` gives `-0.0000`, i.e. `-0.0`). That is evidence, not a
   proof for all inputs, and a broader differential test is still needed.
2. **libm portability.** Byte-identical `ln`/`exp` (policy.py:98, evaluation.py:305-306,
   mcts.py:313, tuning.py:180) holds only when Python and Rust use the same libm on the same
   platform. On this macOS machine they match: 1M-sample check, §2.4. I did not determine
   whether the port must also match across macOS and Linux. If it must, both sides would need the
   same correctly-rounded implementation, and Python cannot be changed.
3. **Embedding Python-only deck sets.** `compare --decks tests` imports `tests.ai.decks:DECKS`,
   and any `module:attr` ref imports a Python module. The Rust CLI cannot do either. It is
   undecided whether the Rust CLI should embed these lists or accept only `examples`, `tuning` and
   file lists.
4. **Lenient numeric parsing.** It is undecided whether the Rust `compare` override parser must
   accept Python's lenient `int()`/`float()` syntax (underscores, Unicode digits, `inf`/`nan`), or
   whether rejecting it counts as an acceptable deviation. No shipped config or doc uses these
   forms.
5. **`_race` ignores rested Blockers.** It counts the defender's Blockers whether rested or not
   (evaluation.py:249-253), while `_side.blockers` counts only ready ones. This looks deliberate
   (the race assumes boards stay as they are, and the defender's units stand up), but it is not
   documented. Port it as written.
6. **`throughput.py` wall-clock fields** (`wall_seconds`, `games_per_core_minute`,
   `core_seconds_per_game`) cannot be byte-identical. I assume the port's acceptance excludes them.
