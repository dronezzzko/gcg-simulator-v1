# 05 — Engine types, game state, and the derived view

Scope: `src/gcg_sim/engine/types.py` (155 lines), `src/gcg_sim/engine/state.py` (651 lines),
`src/gcg_sim/engine/view.py` (958 lines). Every fact below was read from the code on branch
`rust-port` (HEAD d9d41af) or printed with short `uv run python -c` snippets. `path:line`
citations are relative to the repo root. Abbreviations: `types` = `src/gcg_sim/engine/types.py`,
`state` = `src/gcg_sim/engine/state.py`, `view` = `src/gcg_sim/engine/view.py`,
`dsl` = `src/gcg_sim/effects/dsl.py`, `registry` = `src/gcg_sim/effects/registry.py`,
`model` = `src/gcg_sim/cards/model.py`.

Byte-identity depends on this layer because the game record's `final_state_sha256` is
`state_digest(st)` (`src/gcg_sim/runner/game.py:162`), which hashes
`json.dumps(canonical_state(st), sort_keys=True, separators=(",", ":"))`
(`src/gcg_sim/runner/digest.py:52-56`). `canonical_state` is `GameState.to_json()` with
process-local registry indices replaced by content ids (`src/gcg_sim/runner/digest.py:36-49`).
Every serialized field, including `version`, `event_group` and `action_count`, therefore has to
match exactly.

---------------------------------------------------------------------------------------------

## 1. Enumerations (`types`)

### 1.1 `Zone(IntEnum)` (`types:8-30`)

| name | value | notes |
| --- | --- | --- |
| DECK | 0 | ordered, index 0 = top |
| RESOURCE_DECK | 1 | ordered |
| RESOURCE_AREA | 2 | |
| BATTLE | 3 | |
| SHIELD | 4 | shield section, ordered, index 0 = top |
| BASE | 5 | base section of the shield area |
| REMOVAL | 6 | |
| HAND | 7 | |
| TRASH | 8 | |
| RESOLVING | 9 | Commands/Bursts being resolved |
| OUTSIDE | 10 | tokens outside the game. `zones[p][OUTSIDE]` is always empty (see §4) |
| PAIRED | 11 | Pilot under a Unit |

`len(Zone) == 12`, so `N_ZONES = 12` (`state:11`). A Zone member is used directly as a list
index (`state:515`, `view:261` and others). Iterating over `Zone` follows definition order,
which is also value order.

Zone sets (frozensets, used only for membership tests, never iterated in game logic):
- `PUBLIC_ZONES = {RESOURCE_AREA, BATTLE, BASE, REMOVAL, TRASH, RESOLVING, PAIRED}` (`types:33-43`)
- `FIELD_ZONES = {RESOURCE_AREA, BATTLE, SHIELD, BASE, PAIRED}` (`types:44`)
- `ORDERED_ZONES = {DECK, RESOURCE_DECK, SHIELD}` (`types:45`; no users)
- `Zone.is_public` / `Zone.is_field` are membership tests on these sets (`types:24-30`).

### 1.2 `Phase(IntEnum)` (`types:48-55`)
SETUP=0, START=1, DRAW=2, RESOURCE=3, MAIN=4, END=5, GAME_OVER=6.

### 1.3 `Step(IntEnum)` (`types:58-98`). The values are not contiguous.
SETUP_CHOOSE_FIRST=0, SETUP_DRAW=1, SETUP_REDRAW_P1=2, SETUP_REDRAW_P2=3, SETUP_SHIELDS=4,
ACTIVE_STEP=10, START_STEP=11, START_STEP_DONE=12, DRAW_STEP=20, DRAW_STEP_DONE=21,
RESOURCE_STEP=30, RESOURCE_STEP_DONE=31, MAIN=40, ATTACK_TRIGGERS=50, ATTACK_END_CHECK=51,
BLOCK=52, BLOCK_END_CHECK=53, BATTLE_ACTION=54, BATTLE_ACTION_END_CHECK=55, DAMAGE=56,
DAMAGE_DONE=57, BATTLE_END=58, BATTLE_END_DONE=59, END_ACTION=60, END_STEP=61, HAND_STEP=62,
CLEANUP_STEP=63, TURN_END=64, END_STEP_DONE=65, HAND_STEP_DONE=66, CLEANUP_STEP_DONE=67,
GAME_OVER=99.

### 1.4 `DecisionKind(StrEnum)` (`types:101-114`)
CHOOSE_FIRST="choose_first", REDRAW="redraw", MAIN="main", BLOCK="block",
ACTION_STEP="action_step", ORDER_TRIGGER="order_trigger", BURST="burst", YES_NO="yes_no",
SELECT="select", EXCESS="excess", DISCARD="discard", ARRANGE="arrange", PAYMENT="payment".

### 1.5 `ActionKind(StrEnum)` (`types:117-135`). Comments give the meaning of the parameters.
GO_FIRST="go_first" (a = player who becomes Player One), KEEP="keep", REDRAW="redraw",
PLAY_UNIT="play_unit" (a=uid, c=EX resources used), PLAY_BASE="play_base" (a=uid, c=EX),
PAIR="pair" (a=pilot-capable uid, b=unit uid, c=EX), PLAY_COMMAND="play_command" (a=uid, c=EX),
ACTIVATE="activate" (a=host uid, b=ability key index, c=EX), ATTACK="attack"
(a=attacker uid, b=target uid or PLAYER_TARGET), END_MAIN="end_main", BLOCK="block"
(a=blocker uid), NO_BLOCK="no_block", PASS="pass", SELECT="select" (a=uid or option index),
DONE="done", YES="yes", NO="no", ORDER="order" (a=index into pending trigger list).

### 1.6 Sentinels (`types:138-139`)
`PLAYER_TARGET = -2` (the attack target is the player). `NO_ARG = -1` (no value, used for every
"none" int field).

### 1.7 `Duration(StrEnum)` (`types:142-147`)
THIS_TURN="this_turn", THIS_BATTLE="this_battle", OPPONENT_NEXT_TURN="opponent_next_turn",
YOUR_NEXT_TURN="your_next_turn", WHILE_ON_FIELD="while_on_field". `Lasting.duration` stores the
**string value**, not the enum (`state:206`, `src/gcg_sim/engine/interp.py:1309`).

### 1.8 `EndReason(StrEnum)` (`types:150-155`)
BATTLE_DAMAGE="battle_damage", DECK_OUT="deck_out", CONCEDE="concede",
BOTH_DEFEATED="both_defeated", TURN_LIMIT="turn_limit".

### 1.9 DSL and card enums the view consumes (defined outside this scope, listed for completeness)

- `P(StrEnum)` (`dsl:20-26`): YOU="you", OPP="opponent", ACTIVE="active", STANDBY="standby".
- `Side(StrEnum)` (`dsl:29-32`): FRIENDLY="friendly", ENEMY="enemy", ANY="any".
- `Loc(StrEnum)` (`dsl:35-48`), with **`LOC_CODES` = the enumerate index in definition order**
  (`view:936`): BATTLE="battle"→0, PAIRED="paired"→1, BASE="base"→2, SHIELDS="shields"→3,
  SHIELD_AREA="shield_area"→4, RESOURCE_AREA="resource_area"→5, HAND="hand"→6, TRASH="trash"→7,
  DECK="deck"→8, DECK_TOP="deck_top"→9, RESOURCE_DECK="resource_deck"→10, REMOVAL="removal"→11,
  FIELD_UNITS_AND_BASES="units_and_bases"→12 (verified by printing).
- `Op(StrEnum)` (`dsl:51-57`): LE="<=", GE=">=", EQ="==", LT="<", GT=">", NE="!=".
- `Kw(StrEnum)` (`dsl:60-69`): REPAIR="Repair", BREACH="Breach", SUPPORT="Support",
  BLOCKER="Blocker", FIRST_STRIKE="First Strike", HIGH_MANEUVER="High-Maneuver",
  SUPPRESSION="Suppression". `STACKING_KEYWORDS = {REPAIR, BREACH, SUPPORT}` (`dsl:72`).
- `Stat(StrEnum)` (`dsl:75-81`): LV="lv", COST="cost", AP="ap", HP="hp", DAMAGE="damage",
  REMAINING_HP="remaining_hp".
- `CardKind(StrEnum)` (`dsl:84-94`): UNIT="unit", PILOT="pilot", COMMAND="command", BASE="base",
  RESOURCE="resource", TOKEN="token", UNIT_TOKEN="unit_token", EX_RESOURCE="ex_resource",
  EX_BASE="ex_base", LINK_UNIT="link_unit".
- `RuleKind(StrEnum)` (`dsl:585-622`, 29 members): cant_attack, cant_attack_player,
  cant_attack_units, cant_block, cant_be_blocked, may_attack_active, attack_on_deploy_turn,
  cant_be_set_active, stays_rested_in_start_phase, deployed_rested, cant_be_paired,
  cant_receive_damage, reduce_damage, cant_be_destroyed, cant_be_chosen, cant_be_returned,
  no_blocker_vs, must_attack, play_cost_delta, play_level_delta, attack_target_fixed,
  cant_be_attacked, force_attack_target, shield_area_protection, shield_area_reduction,
  ap_cant_be_reduced, redirect_battle_damage, rest_substitute, custom. The member name is the
  upper-case form of the value.
- `DamageKind` (`dsl:625-628`): ANY="any", BATTLE="battle", EFFECT="effect".
- `Ev(StrEnum)` (`dsl:692-727`, 33 members, in definition order): deployed, attacks, blocks,
  blocked, destroyed, paired, linked, damaged, destroys_by_battle, destroys_shield_card,
  deals_damage, cost_paid, support_used, ap_reduced, shield_destroyed, turn_start, turn_end,
  ex_resource_placed, resource_placed, discarded, returned_to_hand, command_played,
  command_resolved, rested, set_active, recovered, drawn, added_to_hand, shield_to_hand,
  battle_end, left_battle_area, burst_activated, exiled.
- `Timing` (`dsl:1250-1253`): MAIN="main", ACTION="action", MAIN_OR_ACTION="main_or_action".
- `Where` (`dsl:1256-1262`): FIELD="field", HAND="hand", TRASH="trash", ANY="any".
- `Gate` (`dsl:1265-1270`): NONE="none", PAIRED="paired", LINKED="linked".
- `CardType(StrEnum)` (`model:10-34`): UNIT="UNIT", PILOT="PILOT", COMMAND="COMMAND",
  BASE="BASE", RESOURCE="RESOURCE", UNIT_TOKEN="UNIT TOKEN", EX_BASE="EX BASE",
  EX_RESOURCE="EX RESOURCE". `is_token` = {UNIT_TOKEN, EX_BASE, EX_RESOURCE};
  `is_unit` = {UNIT, UNIT_TOKEN}; `is_base` = {BASE, EX_BASE};
  `is_resource` = {RESOURCE, EX_RESOURCE}.
- `Color(StrEnum)` (`model:40-45`): BLUE="Blue", GREEN="Green", RED="Red", WHITE="White",
  PURPLE="Purple". `CardDef.color` may be `None` (196 defs).
- `CardDef.zones` values seen in the data: `("Space","Earth")`, `()`, `("Earth",)`, `("Space",)`.

---------------------------------------------------------------------------------------------

## 2. Plain data records (`state`)

All of these classes are mutable unless marked frozen. A frozen record (Action, Decision) can
be shared by reference. Mutable records are copied field by field in `clone()`.

### 2.1 `Action` (frozen dataclass, slots; `state:14-32`)

Fields, in order: `kind: ActionKind`, `a: int = -1`, `b: int = -1`, `c: int = -1`.
- `to_json()` → `[kind.value, a, b, c]`, for example `["go_first", 1, -1, -1]` (`state:23-24`).
- `from_json(l)` → `Action(ActionKind(l[0]), int(l[1]), int(l[2]), int(l[3]))` (`state:27-28`).
- `__str__` → `f"{kind.value}({','.join(str(x) for x in (a,b,c) if x != -1)})"`
  (`state:30-32`). Arguments equal to -1 are **dropped by value**, so
  `Action(X, -1, 3)` prints `x(3)` and `Action(ATTACK, 5, -2)` prints `attack(5,-2)`.
- Equality and hashing compare all four fields (frozen dataclass).

### 2.2 `Decision` (frozen dataclass, slots; `state:35-68`)

Fields, in order: `player: int`, `kind: DecisionKind`, `options: tuple[Action, ...]`,
`prompt: str = ""`, `context: tuple[tuple[str, int], ...] = ()`.
- `ctx(key, default=-1)`: linear scan of `context`; returns the value of the **first** pair whose
  key matches, else `default` (`state:45-49`).
- `to_json()` → dict with keys in this order: `player`, `kind` (string value), `options`
  (list of Action JSON), `prompt`, `context` (a list of `[key, value]` lists) (`state:51-58`).
  Sample: `{"player": 1, "kind": "choose_first", "options": [["go_first", 1, -1, -1],
  ["go_first", 0, -1, -1]], "prompt": "choose Player One", "context": []}`.
- `from_json` casts `player` with int, `kind` with DecisionKind, `prompt` with str, and
  `context` to `(str(k), int(v))` (`state:61-68`).
- Prompts may contain non-ASCII text. One example is the DSL prompt `"pay ①?"`. The digest's
  `json.dumps` keeps the default `ensure_ascii=True` and writes it as `"pay ①?"`.

### 2.3 `CardInstance` (plain class with `__slots__`; `state:71-141`)

One physical card or token. `uid` is equal to the card's index in `GameState.cards` (§4).

| field | type | init (`state:92-102`) | meaning |
| --- | --- | --- | --- |
| uid | int | ctor | index into `st.cards` |
| def_id | int | ctor | index into the CardDB / `Registry.cards`. **Mutable**: AI determinization (`src/gcg_sim/engine/observe.py:76-80,126-127`) and unit variants (`src/gcg_sim/engine/interp.py:930`) rewrite it |
| owner | int | ctor | 0/1, never changes |
| zone | Zone | ctor | current zone |
| rested | bool | False | |
| damage | int | 0 | |
| pair | int | -1 | for a Unit in BATTLE: the pilot uid; for a card in PAIRED: the unit uid; otherwise -1 |
| zone_seq | int | 0 | incremented on every zone change (rule 4-1-5); stale (uid, seq) bindings stop applying |
| entered_turn | int | 0 | `st.turn` when the card last entered a zone |
| known | int | 0 | bitmask `1 << player` of players who know its identity; `BOTH_KNOW = 3` (`src/gcg_sim/engine/core.py:14`) |

- `__slots__` is alphabetical (`state:79-90`), but this has no effect on behaviour.
- `copy()` copies all 10 fields into a new object (`state:104-116`).
- `to_json()` → a list of 10 ints, in this exact order:
  `[uid, def_id, owner, int(zone), int(rested), damage, pair, zone_seq, entered_turn, known]`
  (`state:118-130`). `rested` is written as **0/1**, not true/false.
- `from_json` → `CardInstance(d[0], d[1], d[2], Zone(d[3]))`, then `rested=bool(d[4])` and the
  remaining fields assigned from the list as is (no int() cast) (`state:133-141`).

### 2.4 `Battle` (mutable dataclass, slots; `state:144-191`)

Fields, in order: `attacker: int`, `attacker_seq: int`, `target: int` (uid or -2),
`target_seq: int`, `defender: int`, `battle_id: int`, `blocked: bool = False`,
`damage_only: bool = False`, `attack_triggers: bool = True`, `ended: bool = False`.
- After construction the engine mutates only `target`, `target_seq`, `blocked` and `ended`
  (`src/gcg_sim/engine/battle.py:82-84,245`, `src/gcg_sim/engine/interp.py:1514-1515`).
- `target_seq` is 0 when the target is the player (`src/gcg_sim/engine/battle.py:22`).
- `copy()` copies all fields positionally (`state:159-171`).
- `to_json()` → `[attacker, attacker_seq, target, target_seq, defender, battle_id,
  int(blocked), int(damage_only), int(attack_triggers), int(ended)]`. The four bools are written
  as **0/1** (`state:173-185`). Sample: `[62, 2, -2, 0, 0, 2, 0, 0, 1, 0]`.

### 2.5 `Lasting` (mutable dataclass, slots; `state:194-262`)

A lasting effect in `st.lasting` (created by `Apply`/`ApplyPlayer`), or a delayed trigger in
`st.delayed` (created by `DelayedTrigger`; `src/gcg_sim/engine/interp.py:1548-1557`).

Fields, in order: `effect_key: int` (index into `Registry.continuous`), `controller: int`,
`source_uid: int`, `targets: tuple[tuple[int,int], ...]` ((uid, zone_seq) pairs),
`duration: str` (a Duration value), `created_turn: int`, `battle_id: int` (-1 outside a battle),
`player: int = -1` (>= 0 marks a player-level effect), `uses: int = 0`, `filters_key: int = -1`
(index into `Registry.filter_sets`), `aux: int = -1` (an auxiliary uid, for example the redirect
destination), `serial: int = -1` (`st.next_lasting_serial` at creation; `st.delayed` entries
keep -1, `src/gcg_sim/engine/interp.py:1548-1556`).
- `copy()` creates a new object. The `targets` tuple is shared, which is safe because tuples are
  immutable (`state:215-229`).
- `to_json()` → `[effect_key, controller, source_uid, [[uid, seq], ...], duration, created_turn,
  battle_id, player, uses, filters_key, aux, serial]` (`state:231-245`). Sample:
  `[0, 1, 66, [[62, 2]], "this_turn", 6, 2, -1, 0, -1, -1, 0]`.
- `from_json` casts every field with int, `duration` with str, and targets to int pairs
  (`state:248-262`).
- **`effect_key` and `filters_key` are process-local.** `Registry.continuous` and `filter_sets`
  are **empty when the registry is built** (printed: `continuous 0`, `filter_sets 0`). They grow
  in first-use order as lasting effects are created (`registry:124-138`,
  `src/gcg_sim/engine/interp.py:1305,1314,1549`). The same game can therefore serialize
  different keys in different processes. The digest replaces them with
  `sha256(repr(R.continuous[k]))[:16]` (`src/gcg_sim/runner/digest.py:26-27,40-43`). Examples of
  the reprs that get hashed: `StatMod(ap=-2, hp=0)`, which hashes to `2e97e5d28d17bd2b`;
  `RuleGrant(rule=RuleMod(kind=<RuleKind.CANT_BLOCK: 'cant_block'>, amount=0,
  damage_kind=<DamageKind.ANY: 'any'>, source_filters=(), source_side=<Side.ANY: 'any'>,
  once_per_turn=False, name=''))`, which hashes to `192922184e7d3592`.

### 2.6 `TriggerInst` (mutable dataclass, slots; `state:265-322`)

Fields, in order: `program_id: int`, `controller: int`, `host: int`, `host_seq: int`,
`card_uid: int`, `ability_key: tuple[int,int,int]`, `event: tuple[tuple[str,int], ...]`,
`burst: bool = False`, `once_key: tuple[int, ...] = ()`, `optional: bool = False`.
- The comment at `state:274` says `ability_key` is "(provider def_id, ability index, unit-text
  flag)". The construction sites actually store **`(a.def_id, a.aid, int(a.unit_text))`**: the
  global ability id, not the per-card index (`src/gcg_sim/engine/core.py:280`). Two other
  constructions exist: `(-1, -1, 0)` (`src/gcg_sim/engine/core.py:400`) and
  `(c.def_id, entry.burst_aid, 0)` (`src/gcg_sim/engine/core.py:721`).
- `to_json()` → `[program_id, controller, host, host_seq, card_uid, [k0,k1,k2], [[key, val], ...],
  int(burst), [once_key...], int(optional)]` (`state:294-306`). The bools are written as 0/1.
- `program_id` is also process-local, and the digest replaces it
  (`src/gcg_sim/runner/digest.py:46-47`).

### 2.7 `Frame` (mutable dataclass, slots; `state:325-411`)

A resolving program: a program counter plus bound variables.

Fields, in order: `program_id: int`, `controller: int`, `host: int`, `host_seq: int`,
`card_uid: int`, `kind: str`, `event: tuple[tuple[str,int],...] = ()`, `pc: int = 0`,
`vars: dict[str, tuple[int,...]] = {}`, `did: bool = True`, `answer: bool = False`,
`waiting: bool = False`, `buf: list[int] = []`, `ints: dict[str,int] = {}`,
`once_key: tuple[int,...] = ()`, `acted: bool = False`.
- `kind` values seen in the engine: "system", "command", "burst", "activated",
  "activate_main_sub", "play_sub" (`src/gcg_sim/engine/interp.py:101-148,309,883-954,1415-1490`).
- `ev(key, default=-1)` returns the first match in `event` (`state:366-370`).
- `copy()`: `vars`, `buf` and `ints` are shallow-copied into new containers (their values are
  immutable ints or tuples); `event` and `once_key` tuples are shared (`state:346-364`).
- `to_json()` → dict with keys in this order: `program_id, controller, host, host_seq, card_uid,
  kind, event (list of [k,v]), pc, vars, did, answer, waiting, buf, ints, once_key, acted`
  (`state:372-390`). Here `vars` is `{k: list(v)}` in **sorted key order** and `ints` is a dict
  in sorted key order. Unlike the other records, `did`, `answer`, `waiting` and `acted` are
  written as **JSON true/false**. Sample:
  `{"program_id": 510, "controller": 1, "host": 66, "host_seq": 2, "card_uid": 66,
  "kind": "command", "event": [], "pc": 0, "vars": {}, "did": true, "answer": false,
  "waiting": true, "buf": [], "ints": {}, "once_key": [], "acted": false}`.
- `from_json` builds `vars` and `ints` in JSON order, which is sorted, so the original insertion
  order is lost (`state:393-411`). No game logic depends on that order: the engine only does
  `get` and membership tests on these dicts, and the one `.values()` loop calls `any()`
  (`src/gcg_sim/ai/actions.py:292`, `src/gcg_sim/engine/interp.py:686`).

### 2.8 `HistoryEvent` (mutable dataclass, slots; `state:414-430`)

Fields, in order: `kind: str`, `player: int`, `by: int`, `uid: int`, `def_id: int`,
`source: int = -1`. Instances are created only by `core.record` (`src/gcg_sim/engine/core.py:142-151`).
`record` sets `def_id = st.cards[uid].def_id if uid >= 0 else -1` and **does not call touch()**.
- `to_json()` → `[kind, player, by, uid, def_id, source]`. Kinds seen in a sample: "draw",
  "play", "deployed", "paired", "attack", "block", "cost_paid", "resource_set_active",
  "ex_exiled".

---------------------------------------------------------------------------------------------

## 3. `GameState` (`state:433-651`)

### 3.1 Fields and defaults (`state:475-511`)

`__slots__` holds 36 names (`state:436-473`). Initial values from `__init__(seed)`:

| field | init | notes |
| --- | --- | --- |
| seed | `seed` (stored **unmasked**) | |
| rng | `SplitMix64(seed)` (state = `seed & (2^64-1)`, `src/gcg_sim/rng.py:20-21`) | |
| cards | `[]` | `list[CardInstance]`, index = uid |
| zones | `[[[] for _ in 12] for _ in 2]` | `zones[player][zone]` = ordered `list[int]` of uids |
| turn | 0 | set to 1 at the end of setup (`src/gcg_sim/engine/game.py:186`) |
| active | 0 | turn player |
| first_player | 0 | |
| setup_chooser | 0 | |
| phase | Phase.SETUP | |
| step | Step.SETUP_CHOOSE_FIRST | |
| pending | None | `Decision \| None` |
| battles | `[]` | stack; `battle` property = last one |
| next_battle_id | 1 | |
| next_lasting_serial | 0 | |
| priority | 0 | |
| passes | 0 | |
| lasting | `[]` | `list[Lasting]`, ordered |
| delayed | `[]` | `list[Lasting]`, ordered |
| pending_triggers | `[]` | `list[TriggerInst]` |
| batches | `[]` | `list[list[TriggerInst]]` (may contain empty lists, sample `[[]]`) |
| frames | `[]` | `list[Frame]` (stack) |
| once_used | `set()` | `set[tuple[int, ...]]`, tuples of varying length |
| history | `[]` | `list[HistoryEvent]`, reset to `[]` at each turn change |
| winner | None | `int \| None`; -1 = draw / turn limit |
| end_reason | None | `EndReason \| None` |
| decklists | `((), ())` | per player, a **sorted** tuple of def_ids (`src/gcg_sim/engine/game.py:151`) |
| resource_decklists | `((), ())` | same, sorted |
| redraws | `[False, False]` | |
| action_count | 0 | |
| event_group | 0 | incremented by `core.next_group` (`src/gcg_sim/engine/core.py:158-160`) |
| effect_hits | `{}` | `dict[int, int]` uid → source uid |
| turn_limit | 200 | |
| max_actions | 20000 | `new_game` overrides it with `MAX_ACTIONS_DEFAULT = 20000` (`src/gcg_sim/engine/game.py:33,154`) |
| version | 0 | incremented only by `touch()` |
| _derived | None | cached `Derived` (§5), not serialized |
| _rules_checked | -1 | the `version` at which rules management last found nothing to do (`src/gcg_sim/engine/core.py:861,871,875`), not serialized |

Accessors: `zone(p, z)` returns the live list `zones[p][z]` (`state:514-515`). `standby` is
`1 - active`. `battle` is `battles[-1]` if any, else None. `game_over` is `winner is not None`
(`state:517-527`).

### 3.2 `touch()` and `version` (`state:529-531`)

```
def touch(self): self.version += 1; self._derived = None
```

`version` has three consumers:
1. It is serialized and digested, so the **number of touch() calls must match Python exactly**.
   For example, a new 50+10 vs 50+10 game reaches `version == 120` after the 120 `new_card`
   calls (printed), and one sampled game ended at version 499.
2. The runner checks that the agent did not mutate the state:
   `st.version != version or st.pending is not decision` (`src/gcg_sim/runner/game.py:89-92`).
3. Rules management skips its scan when `_rules_checked == st.version`
   (`src/gcg_sim/engine/core.py:861`). The skip also skips `st.effect_hits.clear()`
   (`src/gcg_sim/engine/core.py:874,885`). The touch count therefore changes state contents, not
   only performance.

The touch() call sites the Rust port must mirror one-for-one (they are owned by other inventory
docs and listed here for cross-checking): `src/gcg_sim/engine/core.py:81` (move),
`:116` (remove_from_game), `:130` (new_card), `:139` (shuffle_deck), `:611` (apply_damage);
`src/gcg_sim/engine/game.py:191,220,237,298,339,871`;
`src/gcg_sim/engine/battle.py:16,28,85,256,277,282`;
`src/gcg_sim/engine/interp.py:709,726,931,986,1078,1098,1202,1242,1320,1374,1469,1516,1574`
(`:1574` runs after **every** CustomStep); `src/gcg_sim/effects/builtins.py:28`; AI-only:
`src/gcg_sim/engine/observe.py:82,130`, `src/gcg_sim/ai/actions.py:229`.

### 3.3 `clone()` (`state:534-572`), field by field

| field | clone semantics |
| --- | --- |
| seed, turn, active, first_player, setup_chooser, next_battle_id, next_lasting_serial, priority, passes, winner, action_count, event_group, turn_limit, max_actions, version, _rules_checked | value copy |
| phase, step, end_reason | the same enum member (immutable) |
| rng | `self.rng.copy()`, a new SplitMix64 with the same 64-bit state |
| cards | `[c.copy() for c in cards]`, a deep copy of every CardInstance |
| zones | `[[list(z) for z in pz] for pz in zones]`, new lists |
| pending | **shared reference** (Decision is frozen). The identity check in the runner relies on this |
| battles | `[b.copy()]`, new Battle objects |
| lasting, delayed | `[x.copy()]`, new Lasting objects (targets tuples shared) |
| pending_triggers | `[t.copy()]` |
| batches | `[[t.copy() for t in b] for b in batches]` |
| frames | `[f.copy()]` (vars/buf/ints shallow-copied) |
| once_used | `set(self.once_used)`, a new set of the same tuples |
| history | `list(self.history)`: **a shallow copy**, so the HistoryEvent objects are shared (nothing mutates them after creation) |
| decklists, resource_decklists | shared tuples |
| redraws | `list(...)` |
| effect_hits | `dict(...)` |
| **_derived** | **shared reference** to the same `Derived` object |

Sharing `_derived` is safe because a computed `Derived` is never mutated afterwards, with one
exception: `trigger_index` fills `dv.by_event` lazily (`view:317-333`). The clone and the
original then see the same `by_event`. The cache key (§5.2) and the touch-on-mutation discipline
keep this consistent. In Rust, use something like `Rc<Derived>` with a `OnceCell` for
`by_event`. A Rust clone must also copy `version` and `_rules_checked`.

### 3.4 `to_json()` canonical form (`state:575-611`)

The result is a dict with these **34 keys in this insertion order**: `seed, rng, cards, zones,
turn, active, first_player, setup_chooser, phase, step, pending, battles, next_battle_id,
next_lasting_serial, priority, passes, lasting, delayed, pending_triggers, batches, frames,
once_used, history, winner, end_reason, decklists, resource_decklists, redraws, action_count,
event_group, effect_hits, turn_limit, max_actions, version` (printed). `_derived` and
`_rules_checked` are omitted.

Value forms:
- `rng`: `self.rng.state`, a u64 integer. `seed`: the raw int.
- `cards`: a list of CardInstance lists (§2.3), in uid order.
- `zones`: a `2 × 12` nested list of uid lists, including the always-empty OUTSIDE list.
- `phase`, `step`: ints.
- `pending`: Decision JSON, or `null`.
- `once_used`: `sorted(list(k) for k in once_used)`. Python compares lists lexicographically
  element by element, and a shorter prefix sorts first. Example (printed):
  `[[-8,3,4],[0,5],[1],[1,2],[1,2,3]]`. Rust `Vec<i64>` `Ord` gives the same order. The digest
  then **re-sorts** the once keys with `key=json.dumps`, i.e. by the string
  `"[a, b, c]"` with the default `", "` separators, after content-id substitution
  (`src/gcg_sim/runner/digest.py:48`). That is a plain string sort, which puts `"-"` (0x2d)
  before digits and compares `"[10, ..."` < `"[2, ..."`.
- `winner`: int or `null`. `end_reason`: string value or `null`.
- `decklists`, `resource_decklists`: `[[...], [...]]`. `redraws`: `[bool, bool]` written as
  true/false.
- `effect_hits`: `[[uid, src], ...]` sorted by uid.
- Bool encodings differ by record: CardInstance, Battle and TriggerInst use 0/1; Frame and
  redraws use true/false.

For the digest only: `json.dumps(..., sort_keys=True, separators=(",", ":"))` with the default
`ensure_ascii=True` (`src/gcg_sim/runner/digest.py:55`). Dict key order inside `to_json`
therefore does not affect the digest. It still matters for any non-sorted dump.

### 3.5 `from_json()` (`state:613-651`)

`from_json` rebuilds every field with int/str casts. Quirks:
- It starts from `GameState(int(d["seed"]))` and then overwrites `rng` with
  `SplitMix64(int(d["rng"]))`.
- **`_derived = None` and `_rules_checked = -1` are not restored.** A round-tripped state
  therefore re-runs rules management at the next `rules_management` call, possibly clearing
  `effect_hits` where the original would not (`src/gcg_sim/engine/core.py:861-875`). Do not use
  JSON round-trips to fork states when byte-identity is required.
- `once_used` is rebuilt as a set of int tuples. `effect_hits` is rebuilt in sorted-key
  insertion order, and nothing iterates it (`src/gcg_sim/engine/core.py:884` iterates `doomed`).
- `from_json(to_json(st))` reproduces `to_json` exactly (checked on a finished sample game).
- No code in `src/` calls `GameState.from_json`; only tests and tools may.

---------------------------------------------------------------------------------------------

## 4. Identity and ordering invariants a compact Rust layout must keep

- **uid = index in `st.cards`.** `core.new_card` sets `uid = len(st.cards)`, appends the card,
  then appends the uid to `zones[owner][zone]` and touches the state
  (`src/gcg_sim/engine/core.py:119-131`). Cards are never removed from `st.cards`; a token that
  leaves goes to `Zone.OUTSIDE` and keeps its slot.
- **Initial uid assignment** (`src/gcg_sim/engine/game.py:128-151`):
  1. For p in 0, 1: shuffle p's main-deck def_ids with `st.rng.shuffle`, then cut at
     `k = 1 + rng.randrange(len-1)`.
  2. Create p0's main deck (uids 0..49, in deck order, so uid 0 is the top card), then p1's main
     deck (50..99).
  3. Create p0's resource deck (100..109) in decklist order, **not shuffled**, then p1's
     (110..119).
  4. Setup later appends, for p in (first_player, 1-first_player), the EX Base token after moving
     6 shields (uids 120 and 121), then an EX Resource for the second player (uid 122)
     (`src/gcg_sim/engine/game.py:180-185`).
- **Zone list order.** A move to DECK, SHIELD or RESOURCE_DECK inserts at index 0 unless
  `bottom=True`; every other destination appends (`src/gcg_sim/engine/core.py:102-106`).
  Removal is `list.remove(uid)`, which deletes the first occurrence and keeps the order of the
  rest (`src/gcg_sim/engine/core.py:38-44`). Tokens moved to a non-token zone are **not** listed
  anywhere: their `zone` becomes OUTSIDE (`src/gcg_sim/engine/core.py:97-100`), so
  `zones[p][OUTSIDE]` stays empty.
- **Iteration orders in the view** (§5) are always: player 0 then 1 (base view and constants),
  or the active player then the other (trigger index). Within a player the zone order is fixed,
  and within a zone the list order is used. Python dict insertion order follows the same scan,
  so a Rust port can use dense per-uid arrays and iterate in the zone-scan order instead of
  keeping insertion-ordered maps. The exceptions, where per-uid **list order** must be kept, are
  `rules[uid]`, `abilities[uid]`, `granted[uid]` and `traits[uid]` (each is a sequence, and
  duplicates are allowed in abilities and granted).
- Integer widths: Python ints are unbounded. `seed`, `rng`: u64 (seeds come from `derive_seed`,
  range `[0, 2^64)`; `src/gcg_sim/rng.py:52-59`). `version`, `event_group`, `action_count`:
  monotonic counters, so use i64/u64. AP/HP sums can be **negative before finalization** (§5.6),
  so use signed types.

---------------------------------------------------------------------------------------------

## 5. The derived view (`view`)

### 5.1 Data structures

**`Ctx`** (`view:27-50`), the evaluation context. Slots: `controller`, `host` (default -1),
`card_uid`, `vars` (default: a new `{}`), `event` (default `()`).
`card_uid = card_uid if card_uid != -1 else host`, so an explicitly passed -1 also falls back to
`host` (`view:42`). `ev(key, default=-1)` returns the first match in `event` (`view:46-50`).

**`RuleInst`** (`view:53-63`). Slots: `rule: RuleMod`, `controller: int`, `source: int`,
`key: tuple[int, ...]`, `aux: int = -1`.
- For a constant ability: `key = (aid, host)`, `source = host`, `aux = -1`, and
  `controller = host owner` (`view:431,436`).
- For a lasting effect: `key = (-1, effect_key, serial)`, `source = le.source_uid`,
  `aux = le.aux`, `controller = le.controller` (`view:443-455`).
- `key` feeds the once-per-turn keys `(PREVENT_ONCE_TAG=-8 | REDUCE_ONCE_TAG=-7, *key, uid,
  zone_seq)` in `st.once_used` (`src/gcg_sim/engine/core.py:23-24,524,561`).

**`Derived`** (`view:66-106`). Slots:

| field | type | contents |
| --- | --- | --- |
| ap, hp | `dict[uid, int]` | only for cards in BATTLE or BASE |
| kw | `dict[uid, dict[Kw, int]]` | only BATTLE/BASE. A key's presence means the card has the keyword; the value is the stacking amount (1 for non-stacking) |
| traits | `dict[uid, tuple[str,...]]` | only BATTLE/BASE |
| rules | `dict[uid, list[RuleInst]]` | **any zone** (RuleGrant does not check zones) |
| cost_mod, level_mod | `dict[uid, int]` | any zone; a key exists only after a non-zero contribution |
| linked | `set[uid]` | BATTLE units whose paired pilot satisfies the link |
| abilities | `dict[uid, list[AbilityEntry]]` | active abilities per host, only for hosts in BATTLE, BASE, PAIRED, HAND, TRASH with a non-empty list |
| by_event | `dict[Ev, list[(uid, AbilityEntry)]] \| None` | lazy trigger index (§5.8) |
| key | tuple | the `_view_key` the view was computed for, default `()` |

`same_values(other)` (`view:94-106`) is the fixpoint convergence test. It is True iff all of
the following hold:
- `ap`, `hp`, `kw`, `traits`, `cost_mod` and `level_mod` are equal as dicts. Python dict
  equality ignores insertion order, compares values exactly, and treats "absent" and "present
  with 0" as different.
- `{k: [r.rule for r in v]}` over `rules` is equal: each per-uid list is compared **in order**,
  RuleMod by structural dataclass equality. Controller, source, key and aux are ignored.
- `{k: [a.aid for a in v]}` over `abilities` is equal, per-uid lists in order.
- **`linked`, `by_event` and `key` are not compared.**

`AbilityEntry` (`registry:23-33`) is frozen: `aid, card_number, def_id, index, unit_text,
ability, program_id, cost_program_id=-1, trigger_event=None`. `CardEntry` (`registry:36-54`)
supplies, per def_id: `own`/`unit` aid tuples, `field_entries`, `hand_entries`,
`trash_entries`, `paired_entries`, `unit_entries`, `gated`, `timings`, `has_burst`. They are
built at `registry:264-283`:
- `field_entries` = own abilities with `where in (FIELD, ANY)`.
- `hand_entries` = own abilities with `where in (HAND, ANY)`.
- `trash_entries` = own abilities with `where in (TRASH, ANY)`.
- `paired_entries` = own abilities with `where is ANY`.
- `unit_entries` = unit-text (pilot-granted) abilities with `where in (FIELD, ANY)`.
- `where` is the ability's `where` field for Constant, Triggered and Activated abilities and
  **FIELD for every other kind** (Keyword, Command, Burst, Replacement, NameAlias,
  PlayModifier; `registry:287-290`). So a Burst or Command ability of a card on the field
  appears in its `abilities` list.
- `gated` = any own or unit ability has `gate != NONE`, where the gate of Constant, Triggered,
  Activated, Keyword and Replacement abilities is their `gate` field, else NONE
  (`registry:283,293-296`).

### 5.2 Caching and invalidation (`view:223-239`)

```
def _view_key(st):
    b = st.battles[-1] if st.battles else None
    battle = (b.battle_id, b.target, b.ended) if b is not None else ()
    return (st.active, st.turn, len(st.battles), battle, len(st.history))

def derived(st):
    dv = st._derived
    key = _view_key(st)            # computed BEFORE _compute
    if dv is None or dv.key != key:
        dv = _compute(st); dv.key = key; st._derived = dv
    return dv
```

A view is recomputed on the next `derived(st)` call when either of these happens:
- `st.touch()` was called (it sets `_derived = None`). Every card, zone and lasting-effect
  mutation in the engine is followed by a touch (§3.2 list).
- The key changed. `core.record` appends history **without** touching, so `len(history)` is
  part of the key. Turn and active-player changes, battle push/pop, re-targeting and
  `ended = True` all change the key as well.

Nothing else invalidates the view. If state the view reads changes with no touch and no key
change, later reads return the stale view until the next invalidation. Whether a stale view
exists at a given moment also depends on **when** `derived()` was last called. For example,
`resolve_ref` skips `derived()` for references that select nothing (§5.10), and invariant checks
call it (`src/gcg_sim/engine/invariants.py:104`). For byte-identity the Rust port must either
reproduce this exact cache policy (lazy compute on first `derived()` call, invalidate on touch
or key change) or prove that every mutation of view inputs is followed by a touch. I found no
engine site that mutates a view input without a following touch before the next read (see
Open questions).

View inputs: `st.cards[*].{def_id, owner, zone, pair, rested, damage, entered_turn, zone_seq}`,
`st.zones`, `st.lasting` (all fields, but **not** `st.delayed`), the last battle's
`{attacker, target, ended}`, `st.active`, `st.turn` and `st.history`, plus whatever custom hooks
read. The custom hooks used inside constants are `wp_gd03_a_has_destroyed_effect` (it reads
`dv.abilities`; `src/gcg_sim/effects/bindings/wp_gd03_a.py:68`), `wp_gd03_b_unique_names` and
`wp_st12_14_lowest_rested_enemy_lv` (it reads `rested`).

### 5.3 Link satisfaction (`view:125-138`)

```
link_satisfied(unit_def, pilot_def):
  link = unit_def.link;  if link is None: return False
  names = pilot_def.names if pilot_def.card_type is PILOT else (pilot_def.pilot_name or "",)
  names = (*names, *R.aliases.get(pilot_def.def_id, ()))
  for part in link.name_parts:            # in order
      if any(part in n for n in names): return True      # SUBSTRING test
  return any(t in pilot_def.traits for t in link.traits)  # PRINTED pilot traits, exact match
```

`CardDef.names` = `(name, *extra_names)`, plus `pilot_name` appended if it is set and not
already present (`model:117-123`). For a Command-pilot only its `pilot_name` (and aliases) is
tested, never its card name. Traits are printed traits; trait grants do not affect linking.

### 5.4 Host abilities and gates (`view:141-216`)

```
_gate_ok(st, dv, host, gate, gate_filters):
  if gate is NONE: True
  card = st.cards[host]
  if card.zone is not BATTLE or card.pair < 0: False
  if gate is LINKED and host not in dv.linked: False
  if gate_filters: return matches(st, dv, Ctx(card.owner, host), card.pair, gate_filters)
                                                     # filters test the PILOT card
  True

_ability_gate(a): Constant/Triggered/Activated/Keyword -> (a.gate, a.gate_filters);
                  Replacement -> (a.gate, ()); else (NONE, ())

_host_abilities(st, dv, uid, granted):
  card = st.cards[uid]; entry = R.cards[card.def_id]; z = card.zone
  base = field_entries if z in (BATTLE, BASE) else hand_entries if z is HAND
         else trash_entries if z is TRASH else paired_entries if z is PAIRED else return []
  out = list(base); gated = entry.gated
  if z is BATTLE and card.pair >= 0:
      pentry = R.cards[st.cards[card.pair].def_id]
      if pentry.unit_entries: out += pentry.unit_entries; gated = gated or pentry.gated
  g = granted.get(uid)
  if g: out += [R.abilities[aid] for aid in g]; gated = True      # duplicates kept
  if not gated: return out
  return [a for a in out if gate(a) is NONE or _gate_ok(st, dv, uid, gate(a), gate_filters(a))]
```

Order within the list: own base entries, then the pilot's unit entries, then granted aids in
grant order. Order and duplicates are significant: they affect keyword sums, trigger order and
activation lists (`src/gcg_sim/engine/game.py:413-417`).

### 5.5 `_base_view(st, granted, prev)` (`view:245-311`)

```
dv = Derived()
# pass 1: stats, traits, links, empty keyword maps
for p in (0, 1):
  for uid in zones[p][BATTLE]:
    u = def(uid); ap, hp = u.ap, u.hp
    if card.pair >= 0:
      pd = def(card.pair); ap += pd.ap; hp += pd.hp
      if link_satisfied(u, pd): linked.add(uid)
    ap[uid]=ap; hp[uid]=hp; traits[uid]=u.traits (unit's printed traits only); kw[uid]={}
  for uid in zones[p][BASE]:
    b = def(uid); ap[uid]=b.ap; hp[uid]=b.hp; traits[uid]=b.traits; kw[uid]={}
gate_view = prev if prev is not None else dv     # dv = THIS partially-built view
# pass 2: abilities and keywords
for p in (0, 1):
  for uid in (*zones[p][BATTLE], *zones[p][BASE]):
    abil = _host_abilities(st, gate_view, uid, granted)
    if abil:
      abilities[uid] = abil
      for a in abil:                    # Keyword abilities only
        if a.ability is Keyword:
          if kw in STACKING: kw[uid][kw] = kw[uid].get(kw, 0) + ab.amount
          else:              kw[uid][kw] = 1
  for uid in zones[p][PAIRED]: if entry.paired_entries or uid in granted: (abil -> abilities)
  for uid in zones[p][HAND]:   if entry.hand_entries   or uid in granted: (same)
  for uid in zones[p][TRASH]:  if entry.trash_entries  or uid in granted: (same)
return dv
```

- The insertion order of `abilities` is: p0 BATTLE, p0 BASE, p0 PAIRED, p0 HAND, p0 TRASH,
  then the same five zones for p1. The insertion order of ap/hp/traits/kw is p0 BATTLE, p0 BASE,
  p1 BATTLE, p1 BASE.
- Base AP/HP are **not clamped** here.
- Keyword abilities on PAIRED, HAND or TRASH hosts do not populate `kw`.
- When `prev is None`, gate filters are evaluated against the view being built. At that point
  `linked` and the pass-1 maps are complete, but `kw` maps are only filled for hosts before the
  current one. Gate filters test the pilot (a PAIRED card), and the built-in filters read printed
  data for PAIRED cards, so the partial state only matters for custom gate filters.
- A pilot in PAIRED gets `paired_entries` (`where == ANY` own abilities) as its own host list.
  Its unit-text abilities are attached to the **Unit** host.

### 5.6 `_compute(st)`: the fixpoint (`view:409-463`)

```
granted = {}; prev = None
base = _base_view(st, granted={}, prev=None)
has_constants = any(a.ability is Constant for abil in base.abilities.values() for a in abil)
if not st.lasting and not has_constants:
    _finalize(base); return base                          # early exit (no iteration)
dv = base
for _ in range(6):                                        # at most 6 iterations
    view = prev if prev is not None else base             # READ side (previous iteration)
    new_granted = {}
    cur = _copy_base(base) if not granted else _base_view(st, granted, prev)   # WRITE side
    # (1) constants, in view.abilities insertion order
    for host, abil in list(view.abilities.items()):
        hc = st.cards[host]
        for a in abil:
            if a.ability is not Constant: continue
            ctx = Ctx(hc.owner, host)                     # controller = host OWNER, card_uid = host
            if ab.cond is not TRUE and not cond(st, view, ctx, ab.cond): continue
            targets = resolve(st, view, ctx, ab.scope)    # resolved ONCE per ability
            for eff in ab.effects:                        # in order
                _apply_cont(st, cur, view, ctx, eff, targets, (a.aid, host), host, new_granted)
    # (2) lasting effects, in st.lasting order
    for le in st.lasting:
        if le.player != -1: continue                      # player-level: not here (see 5.9)
        live = [uid for (uid, seq) in le.targets if st.cards[uid].zone_seq == seq]  # no zone check
        if not live: continue
        ctx = Ctx(le.controller, le.source_uid)
        _apply_cont(st, cur, view, ctx, R.continuous[le.effect_key], live,
                    (-1, le.effect_key, le.serial), le.source_uid, new_granted, le.aux)
    if prev is not None and cur.same_values(prev) and new_granted == granted:
        dv = cur; break
    prev = cur; granted = new_granted; dv = cur
_finalize(dv)          # clamp negative ap/hp to 0, in place
return dv
```

Exact semantics:
- `_copy_base(base)` (`view:336-344`) copies `ap`, `hp` and `traits` with `dict(...)` and `kw`
  as `{k: dict(v)}`. It **shares** `linked` and `abilities` with `base`, and leaves
  `rules`, `cost_mod`, `level_mod` and `by_event` fresh and empty.
- This is a Jacobi-style iteration. Within one iteration every read (conditions, scopes, values,
  `_ap_protected`) uses `view`, i.e. the previous iteration's **unclamped** values, or `base` in
  iteration 0. Every write goes to `cur`. `base.rules` is empty, so **AP protection never
  applies in iteration 0**.
- Iteration 0 can never terminate the loop (`prev is None`), so a compute that does not exit
  early always runs at least 2 iterations. Measured over 24 random games: every compute was
  either an early exit (1630) or exactly 2 iterations (2459).
- If the loop hits 6 iterations without converging, the result is the 6th `cur`. That `cur`'s
  abilities were built from the granted map of iteration 5, not iteration 6.
- Iteration k ≥ 1 evaluates constants from `view.abilities`, i.e. iteration k-1's abilities. A
  granted ability therefore first appears in `cur.abilities` (and `kw`) one iteration after it
  is granted. If the granted ability is itself a Constant, its effects apply one iteration later
  still.
- `new_granted == granted` is dict-of-lists equality: key order is ignored, list order matters.
- `_finalize` (`view:477-483`) sets `ap[k] = 0` where the value is below 0, and the same for
  `hp`. It runs only on the returned view.

### 5.7 `_apply_cont(st, dv=cur, view, ctx, eff, targets, rule_key, source, granted_out, aux)` (`view:347-406`)

| effect | behaviour |
| --- | --- |
| `StatMod(ap, hp)` | `ap = value(view, eff.ap) if eff.ap != 0 else 0`, and the same for hp. For a non-int Value node, `!= 0` is True, so it is evaluated. Both are evaluated once, before the target loop. For each target t, in order: if `ap < 0 and owner(t) != ctx.controller and _ap_protected(view, t)`, add `hp` to `dv.hp[t]` (if t is in `dv.hp`) and skip the AP. Otherwise, if `t in dv.ap`, add to both `ap` and `hp`. Targets outside BATTLE/BASE are ignored. |
| `KeywordGrant(kw, amount)` | `amt = value(view, amount) if amount != 0 else 0`. For each t with `dv.kw.get(t)` not None: if kw is stacking, `kws[kw] = kws.get(kw, 0) + amt` (**a 0 amount still creates the key**, so `has_kw` becomes True); otherwise `kws[kw] = 1`. |
| `TraitGrant(traits)` | For each t in `dv.traits`: `dv.traits[t] = cur + tuple(x for x in eff.traits if x not in cur)`. The check is against `cur` (the traits before this grant), not against the traits being added, so a duplicate inside `eff.traits` is added twice. |
| `RuleGrant(rule)` | For **every** target (any zone): `dv.rules.setdefault(t, []).append(RuleInst(rule, ctx.controller, source, rule_key, aux))`. |
| `CostMod(cost, level)` | `cm` and `lm` are evaluated like StatMod. For each t (any zone): `if cm: cost_mod[t] = get(t, 0) + cm`, and the same for level_mod. A zero contribution creates no key. |
| `AbilityGrant(card_number, idx)` | `entry = R.db.get(card_number)`; return if None. `aids = [*R.cards[entry.def_id].own, *R.cards[entry.def_id].unit]`. If `idx < len(aids)` (a negative idx would pass and index Python-style from the end), append `aids[idx]` to `granted_out.setdefault(t, [])` for every target t (any zone). |

`_ap_protected(view, uid)` = any `RuleInst` in `view.rules.get(uid, ())` has kind
`AP_CANT_BE_REDUCED` (`view:466-468`).

Implication for `ThisCard`: for constants, `ctx = Ctx(owner, host)`, so `card_uid == host`.
For a pilot's unit-text constant attached to a Unit, `ThisCard` resolves to the **Unit**.
Triggered abilities map it to the pilot (`src/gcg_sim/engine/core.py:240-245`).

### 5.8 `trigger_index(st, dv)` (`view:314-333`)

The result is cached on `dv.by_event` the first time it is built:
```
for p in (st.active, 1 - st.active):                       # ACTIVE player first
  for z in (BATTLE, BASE, PAIRED, HAND, TRASH):
    for uid in zones[p][z]:
      for a in dv.abilities.get(uid) or []:
        if a.trigger_event is not None: idx.setdefault(a.trigger_event, []).append((uid, a))
```
Only Triggered abilities have `trigger_event` (`registry:253`). Consumers use `.get(ev, ())`
(`src/gcg_sim/engine/core.py:339`, `src/gcg_sim/ai/actions.py:113,210`). Note that this player
order differs from the p0-first order of `_base_view` and the constant loop.

### 5.9 Characteristic queries (`view:490-570`)

- `ap_of(uid)` = `dv.ap[uid]` if present, else `max(0, printed ap)`.
- `hp_of(uid)` = `dv.hp[uid]` if present; else 1 if the card is in SHIELD; else
  `max(0, printed hp)`.
- `level_of(uid)` = 0 for tokens, else the printed level. `cost_of(uid)` = 0 for tokens, else
  the printed cost. Neither takes a `dv`.
- `play_cost(dv, uid)` = `max(0, cost_of + dv.cost_mod.get(uid, 0) + player_cost_mod(st, uid))`.
- `play_level(dv, uid)` = `max(0, level_of + dv.level_mod.get(uid, 0))`. Player-level level
  modifiers are **not** included.
- `player_cost_mod(st, uid)` (`view:525-545`): returns 0 if there are no lasting effects.
  Otherwise, for `le` in `st.lasting` order with `le.player == owner(uid)` and
  `R.continuous[le.effect_key]` a `CostMod`: if `filters_key != -1`, skip unless
  `matches(derived(st), Ctx(le.controller, le.source_uid), uid, R.filter_sets[filters_key])`.
  Each surviving effect adds `value(derived(st), Ctx(le.controller, le.source_uid), eff.cost)`
  to the total. It ignores `eff.level`, `le.uses` and the card's zone, and it **calls
  `derived(st)` itself** (which may compute or refresh the cache) rather than using the caller's
  `dv`.
- `keywords(dv, uid)` = `dv.kw.get(uid, {})`; `has_kw` = key presence; `kw_amount` =
  `.get(kw, 0)`.
- `traits_of(uid)` = `dv.traits[uid]` if present, else the printed traits.
- `rules_of(dv, uid, kind)` = the filtered list, in order. The order matters: for example, the
  first valid REDIRECT_BATTLE_DAMAGE wins (`src/gcg_sim/engine/core.py:588-591`).
- `is_linked(dv, uid)` = `uid in dv.linked`.
- `names_of(cd)` = `(*cd.names, *aliases)` if the card has aliases, else `cd.names`
  (`view:471-474`). Aliases come from `NameAlias` abilities, collected at registry build
  (`registry:240-241`).

---------------------------------------------------------------------------------------------

## 6. Expression evaluation (`view:577-933`)

`dv` below is whichever view the caller passes: `view` inside the fixpoint, `derived(st)`
elsewhere, or `_UNREAD` (§5.10).

### 6.1 Players and locations
- `_players(ctrl, side)`: FRIENDLY → `(ctrl,)`, ENEMY → `(1-ctrl,)`, ANY → `(ctrl, 1-ctrl)`
  (`view:577-582`).
- `player_of(ctx, p)`: YOU → `ctx.controller`, OPP → `1-ctx.controller`, ACTIVE → `st.active`,
  any other value (STANDBY) → `1-st.active` (`view:585-592`).
- `_loc_cards(player, sel)` (`view:595-623`): BATTLE, PAIRED, BASE, SHIELDS(=SHIELD zone),
  RESOURCE_AREA, HAND, TRASH, DECK, RESOURCE_DECK and REMOVAL each return that zone list.
  SHIELD_AREA returns `BASE + SHIELD`, in that order. DECK_TOP returns
  `DECK[:max(0, value(sel.top_n))]` (a slice, clamped to the list length). Any other loc
  (FIELD_UNITS_AND_BASES) returns `BATTLE + BASE`.
- `select(ctx, sel)` (`view:626-632`): for p in `_players`, for uid in `_loc_cards`, append uid
  if `not sel.filters or matches(...)`. The result is ordered by player (controller first) and
  then by zone list order. It never contains duplicates.

### 6.2 `value(ctx, v)` (`view:853-882`). Dispatch is by `isinstance`, in this order.
| node | result |
| --- | --- |
| `int` (bool also passes this check) | `v` |
| `Count(sel)` | `len(select(sel))` |
| `StatOf(ref, stat)` | `stat(resolve(ref)[0], stat)` if the ref resolves to anything, else 0 |
| `PlayerLevel(p)` | `len(zones[player_of(p)][RESOURCE_AREA])` |
| `HandSize(p)` | `len(HAND)` |
| `ShieldCount(p)` | `len(SHIELD)` (shields only, not bases) |
| `DeckSize(p)` | `len(DECK)` |
| `VarSize(var)` | `len(ctx.vars.get(var, ()))` |
| `EventAmount(key="amount")` | `max(0, ctx.ev(key, 0))` |
| `Sum(terms)` | integer `sum`, exact |
| `Times(value, factor)` | `value * factor` |
| `MinOf(terms)` | `min(...)`. **Raises `ValueError` if `terms` is empty** |
| `KwAmount(kw, ref=This())` | `kw_amount(dv, resolve(ref)[0], kw)` if non-empty, else 0 |
| otherwise (`CustomValue`) | `CUSTOM_VALUES[name](st, dv, ctx, dict(params))`; a missing name raises KeyError |

### 6.3 `cond(ctx, c, did=True)` (`view:885-933`)
| node | result |
| --- | --- |
| `And(conds)` | `all(...)`, short-circuit, in order; `TRUE = And(())` gives True (`dsl:579`) |
| `Or(conds)` | `any(...)`, short-circuit |
| `NotC(c)` | `not cond(c)` |
| `Cmp(l, op, r)` | `_cmp(value(l), op, value(r))`; left is evaluated first |
| `Exists(sel, at_least=1)` | `len(select(sel)) >= at_least` |
| `IsTurn(p=YOU)` | `st.active == player_of(p)` |
| `RefMatches(ref, filters)` | `bool(cards) and all(matches(u, filters) for u in cards)` |
| `RefEmpty(ref)` | `not resolve(ref)` |
| `AttackTargetIs(kind)` | False if there is no battle or it has ended. `kind == "player"`: `target == -2 or zone(target) is BASE`. Any other kind string: `target >= 0 and zone(target) is BATTLE`. It does **not** check that `This` is in the battle, despite the docstring |
| `EventFrom(loc)` | `ctx.ev("from_loc") == LOC_CODES[loc]` |
| `EventFlag(key, value=True)` | `bool(ctx.ev(key, 0)) == value` |
| `HappenedThisTurn(kind, player=YOU, by=None, at_least=1, filters=(), source_filters=())` | `who = player_of(player)`; `by = player_of(by) if by is not None else -1`. Count `h` in `st.history`, in order, with `h.kind == kind` and `h.player == who`, skipping it when `by not in (-1, h.by)`. If `filters` is non-empty, also require `h.uid >= 0` and `matches(h.uid, filters)`; if `source_filters` is non-empty, require `h.source >= 0` and `matches(h.source, source_filters)`. The filters test the card's **current** state. The whole history is scanned. Result: `n >= at_least` |
| `DidLast()` | returns the `did` argument |
| otherwise (`CustomCond`) | `CUSTOM_CONDS[name](st, dv, ctx, dict(params))` |

`_cmp(a, op, b)` (`view:688-699`): LE, GE, EQ, LT, GT; any other op (NE) is `a != b`.

### 6.4 `stat(uid, s)` (`view:673-685`)
AP → `ap_of`. HP → `max(0, hp_of - damage)` (current HP, FAQ Q96). LV → `level_of` (printed;
**excludes** level_mod). COST → `cost_of` (printed). DAMAGE → `damage`. Anything else
(REMAINING_HP) → `max(0, hp_of - damage)`.

### 6.5 `matches(ctx, uid, filters)` = `all(_match1(f) for f in filters)`, short-circuit and in order; empty filters give True (`view:702-705`)

`_match1` (`view:708-781`) checks the filter type with `isinstance` in this order. `card` is
`st.cards[uid]` and `cd` is its CardDef.

| filter | result |
| --- | --- |
| `IsKind(kinds)` | `any(_kind_ok(k, ctx.event) for k in kinds)` |
| `HasTrait(traits)` | `tr = dv.traits.get(uid, cd.traits)` if the card is in BATTLE/BASE, else `cd.traits`; `any(t in tr)` (exact string match) |
| `NameContains(parts)` | `any(part in n for part in parts for n in names_of(cd))`, a **substring** test (`""` matches everything) |
| `NameIs(names)` | `any(n in names for n in names_of(cd))`, exact |
| `HasColor(colors)` | `cd.color is not None and not cd.is_token and cd.color.value in colors` |
| `StatCmp(stat, op, value)` | `_cmp(stat(uid, stat), op, value(value))` |
| `IsRested(rested=True)` | `card.rested == rested` |
| `HasKeyword(kw)` | BATTLE/BASE: `kw in dv.kw.get(uid, {})`; elsewhere `kw in R.printed_keywords(def_id)` (`registry:89-113`; printed keywords sum ungated Keyword lines, ungated Support amounts and ungated self-only TRUE-cond int KeywordGrants) |
| `IsLinked(linked=True)` | `(uid in dv.linked) == linked` |
| `IsPaired(paired=True)` | `(zone is BATTLE and pair >= 0) == paired` |
| `PairedWith(filters)` | `zone is BATTLE and pair >= 0 and matches(pair, filters)` (same ctx) |
| `PairedTo(filters)` | `zone is PAIRED and pair >= 0 and matches(pair, filters)` |
| `IsToken(token=True)` | `cd.is_token == token` |
| `IsDamaged(damaged=True)` | `(damage > 0) == damaged` |
| `NotRef(ref)` / `IsRef(ref)` | `uid not in resolve(ref)` / `uid in resolve(ref)` |
| `IsAttacking(attacking=True)` | `(b is not None and not b.ended and b.attacker == uid) == attacking`, with `b = st.battles[-1]` |
| `IsBattling(battling=True)` | `(b ... and uid in (b.attacker, b.target)) == battling` |
| `HasTiming(timing)` | `timing in R.cards[def_id].timings`. Timing strings are a trigger event value, `"activate_" + timing`, a command timing, or `"burst"` (`registry:189-200`) |
| `HasBurst(burst=True)` | `R.cards[def_id].has_burst == burst` |
| `DeployedThisTurn(value=True)` | `(zone in (BATTLE, BASE) and entered_turn == st.turn) == value` |
| `HasZone(zones)` | `any(z in cd.zones for z in zones)` |
| `LevelCmpRef(stat, op, ref, ref_stat)` | False if `resolve(ref)` is empty, else `_cmp(stat(uid, stat), op, stat(others[0], ref_stat))` |
| `AnyOf(fs)` / `AllOf(fs)` / `Not(f)` | `any` / `all` / `not` over `_match1`, short-circuit |
| otherwise (`CustomFilter`) | `CUSTOM_FILTERS[name](st, dv, ctx, uid, dict(params))` |

`_kind_ok(card, cd, kind, event)` (`view:641-670`), with `t = cd.card_type`:
- UNIT → `t.is_unit`, in any zone.
- PILOT → `t is PILOT or (t is COMMAND and zone is PAIRED)`.
- COMMAND → `t is COMMAND and (zone is not PAIRED or _activated_as_command(event, uid))`.
- BASE → `t.is_base`. RESOURCE → `t.is_resource`. TOKEN → `t.is_token`.
- UNIT_TOKEN, EX_RESOURCE, EX_BASE → the type equals that value.
- Any other kind (LINK_UNIT) → `t.is_unit and uid in dv.linked`.

`_activated_as_command(event, uid)` = `("subject", uid) in event and ("as_command", 1) in event`.
This is tuple membership with value equality (`view:635-638`).

### 6.6 `resolve(ctx, ref)`: returns an ordered tuple of uids (`view:784-831`)
| ref | result |
| --- | --- |
| `This()` | `(ctx.host,)` if host != -1 else `()` |
| `ThisCard()` | `(ctx.card_uid,)` if != -1 else `()` |
| `Var(name)` | `ctx.vars.get(name, ())` |
| `EventCard(key="subject")` | `v = ctx.ev(key)`; `(v,)` if `v >= 0` else `()`, so -2 (player) is excluded |
| `PairedPilotOf(ref)` | for u in `resolve(ref)`: if `zone(u) is BATTLE and pair >= 0`, append the pair; elif `ctx.ev("subject") == u and ctx.ev("pilot") >= 0` and `zone(pilot) is zone(u)`, append the pilot (LKI for destroyed pairs, rule 13-2-8-2-1). No dedup |
| `PairedUnitOf(ref)` | for u: if `zone is PAIRED and pair >= 0`, append the pair |
| `BattlingWith(ref)` | `()` if there is no battle or it has ended; for u: if `u == attacker and target >= 0` append target; elif `u == target` append attacker |
| `All(sel)` | `tuple(select(sel))` |
| otherwise (`Union(refs)`) | concatenation in order with first-occurrence dedup (list membership) |

### 6.7 Location codes (`view:936-954`)
`zone_loc_code(z)` maps BATTLE→0, PAIRED→1, BASE→2, SHIELD→3, RESOURCE_AREA→5, HAND→6,
TRASH→7, DECK→8, RESOURCE_DECK→10, REMOVAL→11. RESOLVING and OUTSIDE map to -1 (printed). The
engine stores this code as the `from_loc` event key (`src/gcg_sim/engine/interp.py:974,1387`).

### 6.8 Dead code in `view`
`_ability_where` (`view:157-160`), `_WHERE_FOR_ZONE` (`view:171-177`), `_HOST_ZONES`
(`view:242`) and `params_dict` (`view:957-958`) have no callers in `src/`. They do not need to
be ported.

### 6.9 Custom hook registries (`view:116-118`)
`CUSTOM_FILTERS`, `CUSTOM_VALUES` and `CUSTOM_CONDS` are module dicts filled when the binding
modules are imported, in sorted module-name order (`src/gcg_sim/effects/bindings/__init__.py:54-97`).
Registered names (printed):
- Filters (9): is_attack_target, wp_gd03_a_has_destroyed_effect, wp_gd03_a_lowest_hp,
  wp_gd03_b_cond_holds, wp_gd03_b_battling_enemy_unit, wp_gd04_a_pilot_capable,
  wp_gd04_a_has_printed_keyword, wp_gd04_a_has_partner_in_trash, wp_gd05_b_can_pair.
- Values (5): wp_gd03_a_ap_per, wp_gd03_b_unique_names, wp_gd05_a_lowest_enemy_unit_level,
  wp_st05_08_highest_enemy_unit_lv, wp_st12_14_lowest_rested_enemy_lv.
- Conds (7): attack_target_damaged, wp_gd01_b_destroyed_with_damage, wp_gd04_b_played_with_ex,
  wp_gd05_a_once_free (reads `st.once_used`), wp_gd05_a_destroyed_by_effect,
  wp_gd05_b_played_with_ex, wp_st12_14_destroyed_by_effect_damage.

### 6.10 `selects` / `resolve_ref` / `_UNREAD` (`view:834-850`)
`selects(ref)` is False for This, ThisCard, Var and EventCard; recurses for PairedPilotOf,
PairedUnitOf and BattlingWith; is `any` over a Union's refs; and is True for anything else
(All). `resolve_ref(st, ctx, ref)` resolves against `derived(st)` only when `selects(ref)` is
True, and otherwise against `_UNREAD`, a module-global empty `Derived()`. The non-selecting refs
never read the view, so the result is identical. The only difference is whether `derived()`
runs, i.e. whether the cache is populated (see §5.2).

---------------------------------------------------------------------------------------------

## 7. Hazards (Python semantics that a straightforward Rust port would get wrong)

1. **The touch count is serialized** (`state:529-531,610`). `version` goes into the state
   digest, so every `touch()` must be mirrored one-for-one (§3.2 list). Do not add "smart"
   touches or remove redundant ones.
2. **Dependence on when the cache is computed** (`view:232-239`). The view is recomputed lazily,
   only on touch or key change. Mutating a view input without a touch leaves a stale view
   whose effect depends on when `derived()` was last called. The callers that decide this
   include `resolve_ref` (`view:848-850`), `player_cost_mod` (`view:539,544`) and invariant
   checks (`src/gcg_sim/engine/invariants.py:104`).
3. **The cache key is computed before the compute** (`view:234-237`) and holds
   `len(st.history)`, not the history's contents.
4. **`_derived` is shared between clones** (`state:570`), and `by_event` is filled lazily on the
   shared object (`view:332`).
5. **`pending` is shared, and the runner tests identity** (`state:546`,
   `src/gcg_sim/runner/game.py:92`). Rust needs pointer identity or a generation counter.
6. **History is shallow-copied** (`state:558`), with HistoryEvent objects shared between clones.
7. **dict insertion order sets the constant application order** (`view:425`):
   `list(view.abilities.items())` iterates p0 BATTLE, BASE, PAIRED, HAND, TRASH, then p1. That
   order fixes the per-uid order of `rules` and `granted`, and therefore REDIRECT selection,
   ability order and trigger order.
8. **Player order differs between functions.** `_base_view` and the constant loop go p0 then p1
   (`view:259,282`); `trigger_index` goes active player first (`view:321`).
9. **Reads come from the previous iteration** (`view:422-436`). Values, conditions, scopes and
   `_ap_protected` read `view` (previous, unclamped), while writes go to `cur`. Do not update in
   place, and do not clamp mid-loop: `_finalize` runs once at the end (`view:462`).
10. **Fixpoint limits** (`view:421,456`): at most 6 iterations, never converged at iteration 0,
    and convergence uses `same_values` plus `new_granted == granted`. Dict equality treats
    "absent" and "0" as different (cost_mod/level_mod are only created when non-zero, while
    stacking keywords are created even at 0). `linked` and the RuleInst controller, source, key
    and aux are not compared (`view:94-106`).
11. **`eff.ap != 0` on a DSL node** (`view:360-361,371,391-392`): a dataclass compared with an
    int gives NotImplemented, then falls back to identity, so `!=` is True and the node is always
    evaluated. Only a literal int 0 is skipped.
12. **`ab.cond is not d.TRUE` is an identity test** (`view:432`): `TRUE is And(())` is False
    (printed). A structurally equal `And(())` is evaluated anyway, and the result is the same
    (True).
13. **`isinstance(v, int)` accepts bool** (`view:854`): True evaluates as 1.
14. **`min()` of empty terms raises** (`view:878`). Python `sum()`/`min()` here work on ints, so
    there is no float summation issue. Use checked or i64 arithmetic for `Times`.
15. **Substring semantics** (`view:136,718`): `part in n` is a substring test, so `""` matches
    everything. `link_satisfied` uses `pilot_name or ""` for non-PILOT pilots (`view:132`).
    Python and Rust substring search agree on valid UTF-8; names contain non-ASCII such as "Ⅱ".
16. **Stacking keywords at amount 0 create the key** (`view:292-295,376-379`), so `has_kw` is
    True. Non-stacking keywords are always set to 1, whatever the amount.
17. **TraitGrant dedups only against the existing traits** (`view:384`).
18. **AbilityGrant index bound** (`view:404`): `idx < len(aids)` lets a negative idx through
    (Python indexes from the end). Granted aids are appended with duplicates kept
    (`view:406`, `view:206`).
19. **Short-circuit evaluation** (`view:705,775,777,887,889,900`): `all()`/`any()` stop early.
    This is observable only through custom hooks that raise or have side effects, and also
    through `MinOf([])` raising.
20. **Union dedup is first-occurrence and O(n²)** (`view:826-831`). PairedPilotOf,
    PairedUnitOf and BattlingWith do not dedup.
21. **`DECK_TOP` slicing clamps** (`view:618`): `[:max(0,n)]` never raises.
22. **`("as_command", 1) in event` compares by value** (`view:638`); Python's `True == 1` would
    also match.
23. **The `ability_key` comment is wrong** (`state:274` vs `src/gcg_sim/engine/core.py:280`):
    the second element is the global aid, not the per-card index.
24. **Bool encodings differ by record** (`state:124,181-184,303-305` use 0/1;
    `state:383-389,604` use true/false).
25. **`once_used` is a set of int tuples of varying length** (`state:497,598`). `sorted()`
    compares lists lexicographically with the shorter prefix first. The digest re-sorts the
    keys by their `json.dumps` *string* (`src/gcg_sim/runner/digest.py:48`), where the default
    separators are `", "`.
26. **`Frame.vars`/`ints` are sorted on serialization** (`state:382,387`), by code point, which
    equals UTF-8 byte order. `from_json` therefore changes their insertion order
    (`state:403,408`); no engine logic depends on it.
27. **`from_json` drops `_rules_checked`** (`state:615-651`), which changes when
    `effect_hits.clear()` runs (`src/gcg_sim/engine/core.py:861-875`).
28. **Process-local registry indices** (`registry:124-138`): `Lasting.effect_key`,
    `Lasting.filters_key`, `TriggerInst.program_id` and `Frame.program_id` are first-use
    indices. The digest replaces them with `sha256(repr(node))[:16]`, so Rust must reproduce
    Python's dataclass and enum `repr` exactly (for example
    `<RuleKind.CANT_BLOCK: 'cant_block'>`), or ship a precomputed content-id table.
29. **`ensure_ascii=True` in the digest** (`src/gcg_sim/runner/digest.py:55`): non-ASCII
    prompts such as `"pay ①?"` are written as `①`. Non-BMP characters would be written as
    surrogate pairs.
30. **`Action.__str__` drops -1 arguments by value** (`state:31`).
31. **The seed is stored unmasked, while the rng state is masked** (`state:476-477`,
    `src/gcg_sim/rng.py:21`).
32. **`1 - ctx.controller`** (`view:581,589`): a controller of -1 would give 2, and the zone
    index would then raise. That should never happen.
33. **`Ctx(card_uid=-1)` falls back to `host`** (`view:42`), and pilot unit-text constants
    resolve `ThisCard` to the Unit (`view:431`), unlike triggers
    (`src/gcg_sim/engine/core.py:240-245`).
34. **`player_cost_mod` details** (`view:525-545`): it ignores `CostMod.level` and `uses`,
    evaluates against `derived(st)` instead of the caller's `dv`, and iterates `st.lasting` in
    list order.
35. **`HappenedThisTurn` filters test the current card state**, not the state at the time of
    the event (`view:923-928`).
36. **`AttackTargetIs("player")` is also True for a Base target** (`view:907-909`).
37. **Hash-dependent containers are used only for membership.** `Derived.linked` (set),
    `once_used` (set) and the `PUBLIC/FIELD/TOKEN` zone frozensets are never iterated in logic.
    The exceptions are `sorted()` in `to_json`, the digest, and AI fingerprints
    (`src/gcg_sim/ai/actions.py:309`). `_cont_index`/`_filter_index` are hash-keyed dicts, but
    lookups are by equality, and the index assignment follows insertion order, not hash order.
38. **Cross-reference: the AI reads view state through `repr`.**
    `src/gcg_sim/ai/actions.py:360` sorts by `repr(r.rule)`, i.e. Python `repr` of a RuleMod.
    `src/gcg_sim/engine/observe.py:150` puts `repr(pending.to_json())` (a Python dict repr with
    `': '`/`', '` separators and single quotes) into the information-set key.
39. **`_finalize` mutates dict values while iterating** (`view:478-483`). This is legal in
    Python because the key set does not change. In Rust, iterate mutably.

---------------------------------------------------------------------------------------------

## 8. Open questions

1. **Stale reads.** I did not exhaustively prove that every mutation of a view input
   (rested, damage, pair, def_id, entered_turn, zones, lasting, battle target and ended) is
   followed by `touch()` before the next `derived()` call. The sites I read
   (`src/gcg_sim/engine/game.py:228-237`, `src/gcg_sim/engine/interp.py:704-711,1064-1099,1455-1469`,
   `src/gcg_sim/engine/battle.py:13-28,80-85`) all touch before any read. The engine/interp
   inventory should confirm this for every remaining site. If the property holds, the cache
   policy is transparent and only the touch count matters.
2. **Fixpoint depth.** Sampled games never needed more than 2 iterations. I could not find a
   card combination that reaches the 6-iteration cap. A port should still implement the cap and
   the "last cur wins" rule exactly.
3. **Custom hook purity.** The custom filters, values and conds used in constants appear to be
   pure readers. I did not read every hook body (they live in `src/gcg_sim/effects/bindings/*`,
   owned by the bindings inventory). If one has side effects, the short-circuit order and the
   number of fixpoint evaluations become observable.
4. **Python `repr` for content ids.** Byte-identical digests need Python's exact
   `repr` of `Continuous`, filter-tuple and `Program` objects, including enum reprs under
   Python 3.15 (`src/gcg_sim/runner/digest.py:26-27`). Two options: port a repr emulator, or
   precompute ids for every node that can occur. Effects frozen at runtime by
   `src/gcg_sim/engine/interp.py:1276-1283` (StatMod/KeywordGrant with computed ints) produce
   new nodes, so a static table alone is not enough. This needs a decision from the
   runner/digest owner.
5. **Process-local indices outside the digest.** Whether any non-digest output (replay files,
   decision logs) serializes raw `effect_key`/`program_id` values was not checked here. The
   runner inventory owns this.
6. **CardDef pilot modifiers.** `_base_view` adds `pd.ap`/`pd.hp` for Command-pilots exactly as
   for Pilots (`view:265-268`). Whether Command CardDefs carry non-zero ap/hp is a data question
   for the cards/db inventory.
