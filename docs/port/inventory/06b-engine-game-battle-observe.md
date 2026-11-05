# 06b — Engine procedure: `game.py`, `battle.py`, `invariants.py`, `observe.py`

Scope: `src/gcg_sim/engine/game.py` (965 lines), `battle.py` (282), `invariants.py` (116),
`observe.py` (150). Branch `rust-port`, base commit d9d41af. Every fact below was read from the
code; `path:line` cites are relative to `src/gcg_sim/`. Functions from `core.py`, `interp.py`,
`view.py`, `state.py` and `rng.py` are described only as far as this area calls them (their own
inventory files own the full detail).

Conventions used throughout:

- `NO_ARG = -1`, `PLAYER_TARGET = -2` (`engine/types.py:131-132`).
- Zone lists are Python lists of uids. For `DECK`, `SHIELD`, `RESOURCE_DECK`, index 0 is the
  **top**. `core.move` to those zones inserts at index 0 unless `bottom=True`; every other
  destination appends (`engine/core.py:103-106`). Hand order is therefore arrival order.
- `st.touch()` increments `st.version` and clears the derived-view cache
  (`engine/state.py:529-531`). `version` is serialized in `GameState.to_json()`
  (`engine/state.py:610`) and therefore enters `final_state_sha256`
  (`runner/digest.py:52-56`, `runner/game.py:159`). **The Rust port must call `touch()` at exactly
  the same points**, see section 15.
- There is no floating-point arithmetic anywhere in these four files.

---

## 1. Enumerations (exact names and values)

`engine/types.py`:

| Enum | Members (name = value) |
| --- | --- |
| `Zone(IntEnum)` `:8-22` | DECK=0, RESOURCE_DECK=1, RESOURCE_AREA=2, BATTLE=3, SHIELD=4, BASE=5, REMOVAL=6, HAND=7, TRASH=8, RESOLVING=9, OUTSIDE=10, PAIRED=11 (declaration order = iteration order) |
| `Phase(IntEnum)` `:49-56` | SETUP=0, START=1, DRAW=2, RESOURCE=3, MAIN=4, END=5, GAME_OVER=6 |
| `Step(IntEnum)` `:59-96` | SETUP_CHOOSE_FIRST=0, SETUP_DRAW=1, SETUP_REDRAW_P1=2, SETUP_REDRAW_P2=3, SETUP_SHIELDS=4, ACTIVE_STEP=10, START_STEP=11, START_STEP_DONE=12, DRAW_STEP=20, DRAW_STEP_DONE=21, RESOURCE_STEP=30, RESOURCE_STEP_DONE=31, MAIN=40, ATTACK_TRIGGERS=50, ATTACK_END_CHECK=51, BLOCK=52, BLOCK_END_CHECK=53, BATTLE_ACTION=54, BATTLE_ACTION_END_CHECK=55, DAMAGE=56, DAMAGE_DONE=57, BATTLE_END=58, BATTLE_END_DONE=59, END_ACTION=60, END_STEP=61, HAND_STEP=62, CLEANUP_STEP=63, TURN_END=64, END_STEP_DONE=65, HAND_STEP_DONE=66, CLEANUP_STEP_DONE=67, GAME_OVER=99 |
| `DecisionKind(StrEnum)` `:99-112` | CHOOSE_FIRST="choose_first", REDRAW="redraw", MAIN="main", BLOCK="block", ACTION_STEP="action_step", ORDER_TRIGGER="order_trigger", BURST="burst", YES_NO="yes_no", SELECT="select", EXCESS="excess", DISCARD="discard", ARRANGE="arrange", PAYMENT="payment" |
| `ActionKind(StrEnum)` `:115-128` | GO_FIRST="go_first", KEEP="keep", REDRAW="redraw", PLAY_UNIT="play_unit", PLAY_BASE="play_base", PAIR="pair", PLAY_COMMAND="play_command", ACTIVATE="activate", ATTACK="attack", END_MAIN="end_main", BLOCK="block", NO_BLOCK="no_block", PASS="pass", SELECT="select", DONE="done", YES="yes", NO="no", ORDER="order" |
| `EndReason(StrEnum)` `:150-155` | BATTLE_DAMAGE="battle_damage", DECK_OUT="deck_out", CONCEDE="concede", BOTH_DEFEATED="both_defeated", TURN_LIMIT="turn_limit" |
| `Duration(StrEnum)` `:143-148` | THIS_TURN="this_turn", THIS_BATTLE="this_battle", OPPONENT_NEXT_TURN="opponent_next_turn", YOUR_NEXT_TURN="your_next_turn", WHILE_ON_FIELD="while_on_field" (stored as plain `str` in `Lasting.duration`) |

Notes:
- `Step.ATTACK_TRIGGERS` (50) is **never assigned** anywhere (`grep` finds only
  `engine/types.py:79`). If a state ever had it, `_turn_step` would raise
  `EngineError("unexpected step 50")` (`engine/game.py:322`; an `IntEnum` formats as its integer,
  verified). Attack triggers resolve while the step is already `ATTACK_END_CHECK`.
- `DecisionKind.PAYMENT` is never constructed anywhere in `src/`.
- Decision kinds created in this area: CHOOSE_FIRST, REDRAW, MAIN, BLOCK, ACTION_STEP,
  ORDER_TRIGGER. BURST, YES_NO, SELECT, EXCESS, DISCARD, ARRANGE come from effect frames
  (`engine/interp.py:_decide`, `:91-102`).
- `CardType(StrEnum)` (`cards/model.py:10-34`): UNIT="UNIT", PILOT="PILOT", COMMAND="COMMAND",
  BASE="BASE", RESOURCE="RESOURCE", UNIT_TOKEN="UNIT TOKEN", EX_BASE="EX BASE",
  EX_RESOURCE="EX RESOURCE"; `is_token` = {UNIT_TOKEN, EX_BASE, EX_RESOURCE}; `is_unit` = {UNIT,
  UNIT_TOKEN}; `is_base` = {BASE, EX_BASE}; `is_resource` = {RESOURCE, EX_RESOURCE};
  `MAIN_DECK_TYPES` = {UNIT, PILOT, COMMAND, BASE} (`cards/model.py:37`).
  `CardDef.is_pilot_capable` = PILOT, or COMMAND with `pilot_name is not None` (`cards/model.py:134-137`).

## 2. Constants

| Name | Value | Where |
| --- | --- | --- |
| `SUPPORT_AID` | -10 | `engine/game.py:31` (pseudo ability id for the merged <Support> activation) |
| `ALT_PLAY_BASE` | 100 | `engine/game.py:32` (`Action.c = ex_used + 100 * (modifier_index + 1)`) |
| `MAX_ACTIONS_DEFAULT` | 20000 | `engine/game.py:33` |
| `MAIN_DECK_SIZE`, `RESOURCE_DECK_SIZE`, `MAX_COPIES`, `MAX_COLORS` | 50, 10, 4, 2 | `engine/game.py:52-55` |
| `core.BOTH_KNOW` | 0b11 | `engine/core.py:14` |
| `core.HAND_LIMIT` | 10 | `engine/core.py:15` |
| `core.BATTLE_LIMIT`, `BASE_LIMIT`, `RESOURCE_LIMIT`, `EX_RESOURCE_LIMIT` | 6, 1, 15, 5 | `engine/core.py:16-19` |
| `core.SHIELD_COUNT`, `START_HAND` | 6, 5 | `engine/core.py:20-21` |
| `core.TOKEN_ZONES` | {BATTLE, RESOURCE_AREA, SHIELD, BASE} | `engine/core.py:27` |
| `observe.HIDDEN_ZONES` | frozenset{DECK, HAND, SHIELD, RESOURCE_DECK} | `engine/observe.py:19` |
| `invariants.LISTED_ZONES` | every Zone except OUTSIDE, in declaration order (0..9, 11) | `engine/invariants.py:26` |
| `GameState` defaults | `turn_limit=200`, `max_actions=20000`, `next_battle_id=1`, `turn=0`, `active=0`, `first_player=0`, `setup_chooser=0`, `priority=0`, `passes=0`, `event_group=0`, `version=0`, `_rules_checked=-1`, `redraws=[False, False]` | `engine/state.py:475-511` |
| `runner/config.py` defaults | `DEFAULT_TURN_LIMIT = 200`, `max_actions = MAX_ACTIONS_DEFAULT` | `runner/config.py:15,30-31` |

Card-data ids in the current snapshot (process-independent because they come from the card DB
load order): EX Base `EXB-001` def_id 90 (EX BASE, AP 0, HP 3); EX Resource `EXR-001` def_id 120
(printed with `uv run python -c` from `cards/db.py:319-324`).

## 3. Data shapes used here (field order = constructor/JSON order)

- `Action(kind, a=-1, b=-1, c=-1)` frozen slots dataclass (`engine/state.py:14-32`). Equality is
  field-wise. JSON `[kind.value, a, b, c]`. `str(action)` = `f"{kind.value}({','.join(str(x) for x in (a,b,c) if x != -1)})"`,
  e.g. `play_unit(2,1)` for `(PLAY_UNIT, 2, -1, 1)`, `attack(15,-2)`, `end_main()`.
- `Decision(player, kind, options: tuple[Action,...], prompt="", context=())` frozen
  (`engine/state.py:35-68`). JSON `{"player","kind","options","prompt","context"}` in that key
  order. All decisions created in `game.py` have `context=()`.
- `Battle(attacker, attacker_seq, target, target_seq, defender, battle_id, blocked=False,
  damage_only=False, attack_triggers=True, ended=False)` (`engine/state.py:144-191`).
  `blocked`, `damage_only`, `attack_triggers` are written and serialized but **never read** by any
  engine code (grep). `ended` is read by `view.py:228,757,760,815,905` and
  `interp.py` (ChangeAttackTarget, DelayedTrigger).
- `TriggerInst(program_id, controller, host, host_seq, card_uid, ability_key, event, burst=False,
  once_key=(), optional=False)` (`engine/state.py:265-322`).
- `GameState.battle` property = `battles[-1]` or `None`; `standby` = `1 - active`
  (`engine/state.py:517-523`).

## 4. Deck construction check — `deck_construction_problems(deck)` (`engine/game.py:58-100`)

Returns a list of message strings in this exact order:

```
problems = []
if len(main) != 50:  "deck has {len(main)} cards; exactly 50 required (6-1-1)"
if len(resources) != 10: "resource deck has {len(resources)} cards; exactly 10 required (6-1-1)"
colors = set()
printings = Counter(db.normalize_id(n) or n for n in main)      # alt-art product ids fold into card numbers (rule 2-1-1)
for number, n in sorted(printings.items()):                     # ascending card-number string (code point order)
    cd = db.get(number)
    if cd is None: "unknown card {number}"; continue
    if cd.card_type not in MAIN_DECK_TYPES:
        "{number} is a {cd.card_type.value} card and cannot be in the deck (6-1-1-1)"
    if n > 4: "{n} copies of {number}; at most 4 allowed (6-1-1-3)"
    if cd.color is not None: colors.add(cd.color)
if len(colors) > 2:
    names = ", ".join(sorted(c.value for c in colors))          # e.g. "Blue, Green, Red"
    "deck uses {len(colors)} colours ({names}); at most 2 (6-1-1-2)"   # British "colours"
for number in sorted(set(resources)):                            # NOT normalized
    cd = db.get(number)
    if cd is None or cd.card_type is not CardType.RESOURCE:     # EX RESOURCE is rejected too
        "{number} is not a Resource card and cannot be in the resource deck (6-1-1-4)"
```

`db.get` strips whitespace and maps product ids to card numbers (`cards/db.py:292-297`).
`new_game(validate=True)` checks player 0 then player 1 and raises on the **first** player with
problems: `DeckConstructionError(f"player {p}: " + "; ".join(problems))` (`engine/game.py:123-127`).
Exceptions: `IllegalActionError(ValueError)` `:36`, `DeckConstructionError(ValueError)` `:48`,
`core.EngineError(Exception)` (`engine/core.py:30`).

## 5. `new_game` (`engine/game.py:107-158`)

Signature: `new_game(decks: (DeckList, DeckList), seed: int, *, chooser: int|None=None,
turn_limit=200, max_actions=20000, validate=True) -> GameState`. `DeckList(main: tuple[str,...],
resources: tuple[str,...])` (`:40-45`). Decks parsed from files arrive with `main` and
`resources` **sorted by card-number string** (`deck/parse.py:103-104`); `new_game` consumes them in
the given order, so the Rust caller must pass the same order.

Exact algorithm:

1. If `validate`: section 4 for p=0, then p=1.
2. `st = GameState(seed)` → `st.rng = SplitMix64(seed)` (`seed & (2^64-1)`, `rng.py:20-21`).
   `st.turn_limit = turn_limit`.
3. For `p in (0, 1)` (player 0 fully before player 1):
   - `ids = [R.db[n].def_id for n in decks[p].main]` (`R.db[n]` raises `KeyError(n)` for an
     unknown number when `validate=False`).
   - `R.require(i)` for every id in order — raises `UnimplementedCardError(f"{card_number}: {error}")`
     if the card has no script (`effects/registry.py:143-147`). Resources are **not** required.
   - `rng.shuffle(ids)` — Fisher–Yates from the end: `for i in range(len-1, 0, -1): j =
     randrange(i+1); swap(i, j)` (`rng.py:43-46`). 49 `randrange` calls for 50 cards.
   - Cut (Floor Rules): if `len(ids) > 1`: `k = 1 + rng.randrange(len(ids) - 1)` (k ∈ [1, len-1]),
     `ids = ids[k:] + ids[:k]` (rotate left by k).
   - `res_ids[p] = [R.db[n].def_id for n in decks[p].resources]` — **no shuffle**; resource deck
     order is the DeckList order, top = index 0.
4. **Uid assignment after shuffle and cut** (`:143-150`), via `core.new_card` (uid =
   `len(st.cards)`, appended to the zone list, `known=BOTH_KNOW`, `entered_turn=st.turn=0`,
   `rested=False`, one `touch()` each; `engine/core.py:119-131`):
   - p0 main deck in shuffled order → uids 0..49, then p1 main → 50..99; each then gets
     `known = 0`. So uid `50*p + i` is deck position i (0 = top) of player p.
   - p0 resource deck → 100..109, p1 → 110..119; each `known = 1 << p` (owner knows it).
5. `st.decklists = (tuple(sorted(main_ids[0])), tuple(sorted(main_ids[1])))`,
   `st.resource_decklists` likewise (ascending def_id ints).
6. `st.setup_chooser = chooser if chooser is not None else rng.randrange(2)` — the die roll
   consumes RNG **only** when `chooser is None`, and only after both decks were shuffled and cut.
   `chooser=0` is a valid explicit value.
7. `st.max_actions = max_actions`; `phase = SETUP`; `step = SETUP_CHOOSE_FIRST`; `advance(st)`.

RNG order in `new_game`: p0 shuffle, p0 cut, p1 shuffle, p1 cut, [die roll]. Verified by an
independent re-implementation against `new_game` (scratch script, matched all 100 def_ids, the
chooser and the final `rng.state`).

After `new_game` the state has 120 cards and `version == 120` (one touch per `new_card`), and a
pending CHOOSE_FIRST decision.

## 6. Setup procedure (rule 6-2) — `_setup_step` (`engine/game.py:161-191`) and `apply`

Setup steps run from `advance` while `phase is SETUP`; **no rules management, no events, no
history records** happen during setup (core.move/new_card never emit).

| Step | Behaviour |
| --- | --- |
| `SETUP_CHOOSE_FIRST` | `pending = Decision(setup_chooser, CHOOSE_FIRST, (Action(GO_FIRST, setup_chooser), Action(GO_FIRST, 1 - setup_chooser)), "choose Player One")`. Step unchanged. |
| apply CHOOSE_FIRST (`:792-795`) | `first_player = action.a`; `active = action.a`; `step = SETUP_DRAW`. |
| `SETUP_DRAW` | for `p in (first_player, 1 - first_player)`: 5 × `core.move(st, zones[p][DECK][0], HAND)` (top card each time). Then `step = SETUP_REDRAW_P1`. Knowledge: deck→hand gives `known |= 1 << owner` (`engine/core.py:47-51`). |
| `SETUP_REDRAW_P1` / `_P2` | `p = first_player` for P1, else `1 - first_player`; `pending = Decision(p, REDRAW, (Action(KEEP), Action(REDRAW)), "redraw?")`. |
| apply REDRAW (`:796-799`) | if `action.kind is REDRAW`: `_redraw(st, dec.player)`. Then `step = SETUP_REDRAW_P2` if step was P1 else `SETUP_SHIELDS`. |
| `_redraw(p)` (`:194-201`, rule 6-2-1-6-1) | snapshot `list(zones[p][HAND])`; each → `move(uid, DECK, bottom=True)` (in hand order); then 5 × `move(zones[p][DECK][0], HAND)`; then `core.shuffle_deck(st, p)` = `st.rng.shuffle(deck)` on the whole remaining deck list (45 cards → 44 `randrange` calls), set `known = 0` on every deck card, `touch()` (`engine/core.py:134-139`); `redraws[p] = True`. |
| `SETUP_SHIELDS` (`:178-191`) | for `p in (first, second)`: 6 × `move(zones[p][DECK][0], SHIELD)` (each inserted at index 0, so the shield list ends **reversed**: shield[0] is the 6th card taken), then `new_card(EX Base def, p, BASE)`. After both: `new_card(EX Resource def, 1 - first_player, RESOURCE_AREA)` (active, `rested=False`). Then `turn = 1`, `active = first_player`, `phase = START`, `step = ACTIVE_STEP`, `history = []`, `touch()`. |

Resulting uids: first player's EX Base = 120, second player's EX Base = 121, second player's
EX Resource = 122. Shield knowledge stays 0 (deck→shield keeps prior knowledge,
`engine/core.py:52`). All setup moves set `entered_turn = 0`.

Version accounting (verified): 120 after new_game; SETUP_DRAW +10; one redraw +11 (5+5 moves +
shuffle); SETUP_SHIELDS +16 (12 moves + 3 new_card + 1 touch); turn 1 ACTIVE_STEP +1, draw +1,
resource +1 → **160** at the first MAIN decision when exactly one player redrew.

## 7. `advance(st)` (`engine/game.py:734-760`) — the main loop

```
while st.pending is None and st.winner is None:
    if st.action_count > st.max_actions:            # strict >
        winner = -1; end_reason = TURN_LIMIT; phase = GAME_OVER; step = GAME_OVER; return
    if st.frames:            I.run_top_frame(st); continue      # effects first
    if st.pending_triggers:  _form_batch(st);     continue
    if st.batches:           _start_next_trigger(st); continue
    if st.phase is SETUP:    _setup_step(st)
    else:
        core.rules_management(st)
        if st.winner is not None: return
        if st.frames or st.pending_triggers: continue
        _turn_step(st)
```

Consequences:
- Frames, then queued triggers, then trigger batches always run before the next procedure step
  (rules 7-1-3, 7-2-2, 7-6-2). Steps named `*_DONE` exist only so that the events of the previous
  step resolve before the procedure moves on.
- `rules_management` runs before **every** `_turn_step` call (and inside the interpreter after each
  instruction / resume, `engine/interp.py:125-126,174-175`). It is cached by
  `st._rules_checked == st.version` (`engine/core.py:861`); see Hazards (the cache controls when
  `effect_hits` is cleared, so it is behaviour-relevant).
- When the action cap fires, frames/batches/triggers are left in place and `pending` is `None`.
- `_turn_step` is not called when `phase is SETUP`; `_setup_step` is not called otherwise.

## 8. Step state machine — `_turn_step(st)` (`engine/game.py:223-322`)

`p = st.active`. Each call performs exactly one row; "→ X" sets `st.step = X`.

| Step | Action performed (in order) | Next |
| --- | --- | --- |
| ACTIVE_STEP (10) | `dv = derived(st)` once; for z in (BATTLE, RESOURCE_AREA, BASE), for uid in `zones[p][z]` (list order): if `rested` and no `CANT_BE_SET_ACTIVE` rule and no `STAYS_RESTED_IN_START_PHASE` rule (both from the same `dv`): `rested = False`. No events. Then `touch()` (always). PAIRED pilots are not touched. | START_STEP |
| START_STEP (11) | `next_group`; `emit(TURN_START, subject=-1, player=p)` | START_STEP_DONE |
| START_STEP_DONE (12) | `phase = DRAW` | DRAW_STEP |
| DRAW_STEP (20) | `I.draw(st, p, 1)`: `next_group`; if deck non-empty: move top → HAND, `record("draw", p, p, uid)`, `emit(DRAWN, uid, player=p, group=g, by_effect=0)` (`engine/interp.py:500-512`). Player One **does** draw on turn 1. | DRAW_STEP_DONE |
| DRAW_STEP_DONE (21) | `phase = RESOURCE` | RESOURCE_STEP |
| RESOURCE_STEP (30) | `I.place_resource(st, p, rested=False, by=p)`: if resource deck empty or RA has ≥15 cards: nothing; else move `rdeck[0]` → RESOURCE_AREA active, `emit(RESOURCE_PLACED, uid, player=p, by=p)` — **no `next_group`** (uses current group) (`engine/interp.py:1051-1059`). | RESOURCE_STEP_DONE |
| RESOURCE_STEP_DONE (31) | `phase = MAIN` | MAIN |
| MAIN (40) | `pending = Decision(p, MAIN, tuple(main_options(st)), "main phase")` | (stays MAIN; see apply) |
| ATTACK_END_CHECK (51) | `BATTLE_END if battle_broken(st, battle) else BLOCK` | BATTLE_END / BLOCK |
| BLOCK (52) | `blockers = eligible_blockers(st, b)`; if empty → BLOCK_END_CHECK; else `pending = Decision(b.defender, BLOCK, (*[Action(BLOCK, u) for u in blockers], Action(NO_BLOCK)), "block?")` | BLOCK_END_CHECK / decision |
| BLOCK_END_CHECK (53) | if broken → BATTLE_END; else → BATTLE_ACTION, `priority = standby`, `passes = 0` | |
| BATTLE_ACTION (54), END_ACTION (60) | `_action_step(st)` (section 10) | |
| BATTLE_ACTION_END_CHECK (55) | `BATTLE_END if broken else DAMAGE` | |
| DAMAGE (56) | `step = DAMAGE_DONE` **first**, then `B.damage_step(st)` | DAMAGE_DONE |
| DAMAGE_DONE (57) | — | BATTLE_END |
| BATTLE_END (58) | `step = BATTLE_END_DONE` first, then `B.end_battle(st)` | BATTLE_END_DONE |
| BATTLE_END_DONE (59) | `battles.pop()`; `touch()` | MAIN (rule 8-6-2) |
| END_STEP (61) | `next_group`; `emit(TURN_END, subject=-1, player=p)` (this is also where <Repair> triggers are queued, `engine/core.py:445-459`) | END_STEP_DONE |
| END_STEP_DONE (65) | — | HAND_STEP |
| HAND_STEP (62) | if `len(zones[p][HAND]) > 10`: `push_frame(system_program("hand_limit"), controller=p, host=-1, kind="system")` (program = `Discard(count=Sum((HandSize(), -10)))`, `engine/interp.py:1580-1584`) | HAND_STEP_DONE |
| HAND_STEP_DONE (66) | — | CLEANUP_STEP |
| CLEANUP_STEP (63) | `_expire(st)` | CLEANUP_STEP_DONE |
| CLEANUP_STEP_DONE (67) | — | TURN_END |
| TURN_END (64) | `_next_turn(st)` | ACTIVE_STEP or GAME_OVER |
| anything else | `raise core.EngineError(f"unexpected step {s}")` | |

Setup steps (0–4) are handled by `_setup_step`. GAME_OVER (99) is never dispatched (the loop
exits on `winner is not None`). `assert b is not None` guards the battle steps.

Transitions out of MAIN happen in `apply` (`_apply_main`, `engine/game.py:821-831`):
- `END_MAIN` → `phase = END`, `step = END_ACTION`, `priority = standby`, `passes = 0`.
- `ATTACK` → `B.declare_attack(st, a, b)` which sets `step = ATTACK_END_CHECK`.
- anything else → `_play_or_activate(st, active, action)`; step stays MAIN, so after the pushed
  frames and triggers resolve a fresh MAIN decision is generated.

Phase is changed only at: START_STEP_DONE (DRAW), DRAW_STEP_DONE (RESOURCE), RESOURCE_STEP_DONE
(MAIN), END_MAIN (END), `_next_turn` (START), `SETUP_SHIELDS` (START), and game end (GAME_OVER).
The battle runs with `phase == MAIN`. No code outside `game.py`/`core.rules_management` reads
`st.phase` or `st.step` (grep over `engine/`, `ai/`, `runner/`).

Observed transitions (5 random games, scratch run): exactly the rows above; ATTACK_TRIGGERS never
appears; attack-trigger `select` decisions occur with `step == ATTACK_END_CHECK`, Burst decisions
with `step == DAMAGE_DONE`.

## 9. Turn structure, expiry and the turn cap

`_expire(st, *, battle_id=-1)` (`engine/game.py:208-220`; the `battle_id` parameter is unused):

```
keep(duration, created, controller):
    "this_turn" or "this_battle"  -> False
    "opponent_next_turn"          -> not (st.turn > created and st.active != controller)
    "your_next_turn"              -> not (st.turn > created and st.active == controller)
    anything else                 -> True            # "while_on_field"
st.lasting = [le for le in st.lasting if keep(le.duration, le.created_turn, le.controller)]
st.delayed = [same filter over st.delayed]
st.touch()                                            # always
```

`_next_turn(st)` (`engine/game.py:325-339`):

```
if st.turn >= st.turn_limit:
    winner = -1; end_reason = TURN_LIMIT; phase = GAME_OVER; step = GAME_OVER; return  # no touch
turn += 1; active = 1 - active; once_used = set(); history = []
phase = START; step = ACTIVE_STEP; touch()
```

So turn `turn_limit` is played to its TURN_END, then the game is drawn; `turn` stays at
`turn_limit` and `active` stays (verified with `turn_limit=1`: 4 decisions, end at turn 1,
winner -1). `effect_hits`, `event_group`, `next_battle_id`, `battles` are **not** reset per turn.

## 10. Action steps and passing — `_action_step(st)` (`engine/game.py:342-353`)

```
if st.passes >= 2:
    st.step = BATTLE_ACTION_END_CHECK if st.step is BATTLE_ACTION else END_STEP; return
p = st.priority
opts = action_step_options(st, p)
if not opts:                     # silent pass: no Decision, no action_count, no history, no touch
    st.passes += 1; st.priority = 1 - p; return
st.pending = Decision(p, ACTION_STEP, (*opts, Action(PASS)), "action step")
```

apply ACTION_STEP (`engine/game.py:806-812`): `PASS` → `passes += 1`; otherwise `passes = 0`
and `_play_or_activate(st, dec.player, action)`. In both cases `priority = 1 - dec.player`.
The standby player always gets priority first (`priority = standby` at END_MAIN and at
BLOCK_END_CHECK). The step ends after two consecutive passes (silent or explicit).

Single-option decisions are still real decisions (e.g. MAIN with only `end_main()`), they count
toward `action_count` and appear in action logs. Only the action step has silent passes.

## 11. Legal options per decision kind (exact ordering)

`legal_actions(st)` = `st.pending.options` or `()` (`engine/game.py:774-777`).

### 11.1 CHOOSE_FIRST, REDRAW, BLOCK, ACTION_STEP, ORDER_TRIGGER

- CHOOSE_FIRST: `[GO_FIRST(chooser), GO_FIRST(1 - chooser)]` — "I go first" is option 0.
- REDRAW: `[KEEP, REDRAW]`.
- BLOCK: `BLOCK(u)` for each eligible blocker in the defender's BATTLE list order, then `NO_BLOCK`.
- ACTION_STEP: `action_step_options(st, p)` then `PASS` last.
- ORDER_TRIGGER: `ORDER(i)` for each candidate batch index i, ascending (section 13).

### 11.2 Resource helpers

`_resources(st, p)` (`:360-369`): over `zones[p][RESOURCE_AREA]`, skipping rested cards: count
EX RESOURCE type as `ex`, everything else as `normal`. Returns `(normal, ex)`.

`payment_choices(st, p, cost, resources=None)` (`:372-382`):
```
if cost <= 0: return [0]
normal, ex = resources or _resources(st, p)
lo = max(0, cost - normal); hi = min(cost, ex)
return list(range(lo, hi + 1)) if lo <= hi else []
```
i.e. the numbers of EX Resources usable, ascending (using EX is offered even when unnecessary).

`_level_ok(st, dv, p, uid)` = `len(zones[p][RESOURCE_AREA]) >= V.play_level(st, dv, uid)` — counts
every card in the area, rested or not, EX included (`:385-386`).

`V.play_cost = max(0, cost + dv.cost_mod[uid] + player_cost_mod)`, `V.play_level = max(0, level +
dv.level_mod[uid])`; tokens have cost/level 0 (`engine/view.py:507-523`).

### 11.3 MAIN — `main_options(st)` (`engine/game.py:543-583`)

```
p = active; dv = derived(st)
my_units = [u for u in zones[p][BATTLE] if I.can_pair(st, dv, u)]     # unpaired, no CANT_BE_PAIRED
resources = _resources(st, p)                                         # computed once
out = []
for uid in zones[p][HAND]:                                            # hand order
    cd = def of uid
    alternatives = _alt_play_options(st, dv, p, uid, my_units)        # computed first, appended last
    pays = payment_choices(st, p, play_cost(uid), resources) if _level_ok(uid) else []
    if not pays: out += alternatives; continue
    UNIT    : out += [PLAY_UNIT(uid, -1, c) for c in pays]
    BASE    : out += [PLAY_BASE(uid, -1, c) for c in pays]
    PILOT   : for u in my_units: out += [PAIR(uid, u, c) for c in pays]       # unit-major
    COMMAND : if _command_playable(st, p, uid, MAIN): out += [PLAY_COMMAND(uid, -1, c) for c in pays]
              if cd.pilot_name is not None:                                   # rule 3-4-6-2
                  for u in my_units: out += [PAIR(uid, u, c) for c in pays]
    (other types add nothing)
    out += alternatives
for (host, aid, cost) in _activated_list(st, p, MAIN):
    out += [ACTIVATE(host, aid, c) for c in payment_choices(st, p, cost, resources)]
out += _attack_options(st, p)
out.append(END_MAIN)                                                 # always last, always present
```

`I.can_pair(st, dv, unit)` = zone is BATTLE and `pair < 0` and no `CANT_BE_PAIRED` rule
(`engine/interp.py:1340-1344`).

`_command_playable(st, p, uid, timing)` (`:395-407`): `entry = R.cards[def_id]`; False if
`entry.command_aid < 0`; `ab = R.abilities[command_aid].ability`; False unless `ab.timing is
MAIN_OR_ACTION or ab.timing is timing`; `ctx = Ctx(p, uid)`; False if `ab.cond is not TRUE` and
`V.cond(st, V.derived(st), ctx, ab.cond)` is false; finally
`I.targets_available(R.programs[program_id].instrs, 0, st, ctx)` (rules 10-1-8-1-x, 10-2-2, Q100;
`engine/interp.py:235-260`, pure).

`play_modifiers(st, uid)` = `[R.abilities[a] for a in R.cards[def_id].own if
isinstance(R.abilities[a].ability, PlayModifier)]` in `own` order (`:586-589`).

`_alt_play_options(st, dv, p, uid, my_units)` (`:607-644`):
```
mods = play_modifiers(uid); if not mods: return []
for i, entry in enumerate(mods):                  # i counts ALL modifiers, legal or not
    pm = entry.ability; ctx = Ctx(p, uid)
    if pm.cond is not TRUE and not V.cond(st, dv, ctx, pm.cond): continue
    if not _costs_payable(st, dv, p, uid, pm.costs): continue
    level = pm.level if pm.level is not None else play_level(uid)
    if len(zones[p][RESOURCE_AREA]) < level: continue
    cost  = pm.cost  if pm.cost  is not None else play_cost(uid)
    enc = [c + 100 * (i + 1) for c in payment_choices(st, p, cost)]
    UNIT: PLAY_UNIT(uid,-1,e) for e in enc | BASE: PLAY_BASE(...) |
    COMMAND and _command_playable(MAIN): PLAY_COMMAND(...)
    if cd.is_pilot_capable:                        # independent `if`
        for u in my_units:
            if pm.pair_filters and not V.matches(st, dv, ctx, u, pm.pair_filters): continue
            out += [PAIR(uid, u, e) for e in enc]
```
`_costs_payable` (`:592-604`): for each cost of type RestCards/DestroyCards/DiscardCards/
ExileCards: `sel` (RestCards adds filter `IsRested(False)`); candidates =
`[u for u in V.select(st, dv, Ctx(p, host), sel) if u != host]`; fail if fewer than `count`. Other
cost types are ignored here.

`_activated_list(st, p, timing)` (`:410-433`) returns `(host, aid, cost)` triples:
```
dv = derived(st)
for z in (BATTLE, BASE, HAND, TRASH, PAIRED):
    for host in zones[p][z]:
        for a in dv.abilities.get(host, ()):            # derived ability order
            if not Activated: continue
            if a.timing is not MAIN_OR_ACTION and a.timing is not timing: continue
            cost = _activation_ok(st, dv, p, host, a); if cost >= 0: append (host, a.aid, cost)
        if timing is MAIN and z is BATTLE and kw_amount(host, SUPPORT) > 0
           and not host.rested and any(u != host for u in zones[p][BATTLE]):
            append (host, SUPPORT_AID = -10, 0)         # after the host's own abilities
```

`_activation_ok(st, dv, p, host, a)` (`:436-472`) returns the resource cost or -1:
```
if ab.once_per_turn and (a.aid, host, cards[host].zone_seq) in once_used: -1
card_uid = core._card_uid_for(st, a, host)       # the paired Pilot if the ability is its unit text
ctx = Ctx(p, host, card_uid)
if ab.cond is not TRUE and not V.cond(st, dv, ctx, ab.cond): -1
res = 0
for c in ab.costs:
    RestSelf: -1 if host rested or host.zone not in (BATTLE, BASE)
    PayResources: res += c.amount
    RestCards/DestroyCards/DiscardCards/ExileCards: sel (+IsRested(False) for RestCards);
        -1 if len(V.select(st, dv, ctx, sel)) < c.count          # host NOT excluded here
    ReturnSelf/DestroySelf/TrashSelf: -1 if host.zone not in (BATTLE, BASE, PAIRED, HAND, TRASH)
if res and not payment_choices(st, p, res): -1
if not I.targets_available(R.programs[a.program_id].instrs, 0, st, ctx): -1
return res
```

`_attack_options(st, p)` (`:475-516`):
```
dv; enemy_units = zones[1-p][BATTLE]
attractors = [t for t in enemy_units if rules_of(t, FORCE_ATTACK_TARGET)]
for uid in zones[p][BATTLE]:
    if uid.rested or rules_of(uid, CANT_ATTACK): continue
    deploy_limits = None
    if uid.entered_turn == st.turn and uid not in dv.linked:       # rule 3-2-6-3: Link Units may attack
        perms = rules_of(uid, ATTACK_ON_DEPLOY_TURN)
        if not perms: continue
        if all(r.rule.name == "units_only" for r in perms):
            deploy_limits = perms[0].rule.source_filters            # may be () = any unit
    unit_targets = []
    if not rules_of(uid, CANT_ATTACK_UNITS):
        active_perms = rules_of(uid, MAY_ATTACK_ACTIVE)
        for t in enemy_units:
            if rules_of(t, CANT_BE_ATTACKED): continue
            ok = t.rested or any(not r.rule.source_filters
                                 or V.matches(st, dv, Ctx(p, uid), t, r.rule.source_filters)
                                 for r in active_perms)
            if ok and deploy_limits is not None and deploy_limits:
                ok = V.matches(st, dv, Ctx(p, uid), t, deploy_limits)
            if ok: unit_targets.append(t)
    forced = _forced_targets(st, dv, uid, attractors, unit_targets)
    if forced: out += [ATTACK(uid, t) for t in forced]; continue
    if deploy_limits is None and not rules_of(uid, CANT_ATTACK_PLAYER):
        out.append(ATTACK(uid, -2))                                  # player before units
    out += [ATTACK(uid, t) for t in unit_targets]
```
Bases are never attack targets from here (only enemy BATTLE units, or the player).

`_forced_targets(st, dv, attacker, attractors, legal)` (`:519-540`):
```
must = []; if_possible = []
for t in attractors:
    if t not in legal: continue
    for r in rules_of(t, FORCE_ATTACK_TARGET):
        if r.rule.source_filters and not V.matches(st, dv, Ctx(owner_of(t), t), attacker, r.rule.source_filters): continue
        if r.rule.name == "must": must.append(t); break
        if t.rested and t not in if_possible: if_possible.append(t)
return must or if_possible        # "must" outranks "if possible" (Q289)
```

Sample (scratch run, turn 15): `['play_base(4,0)', 'play_unit(13,0)', 'play_unit(16,0)',
'pair(10,15,0)', 'pair(10,17,0)', 'pair(10,3,0)', 'play_unit(18,0)', 'attack(15,-2)',
'attack(15,92)', 'attack(0,-2)', 'attack(0,92)', ..., 'end_main()']` (unit 0 was already paired
so it is absent from the PAIR targets).

### 11.4 ACTION_STEP — `action_step_options(st, p)` (`engine/game.py:647-660`)

```
dv = derived(st); out = []
for uid in zones[p][HAND]:
    if card_type is not COMMAND or not _level_ok(uid): continue
    pays = payment_choices(st, p, play_cost(uid))            # fresh _resources(p)
    if pays and _command_playable(st, p, uid, ACTION):
        out += [PLAY_COMMAND(uid, -1, c) for c in pays]
for (host, aid, cost) in _activated_list(st, p, ACTION):   # no Support at ACTION timing
    out += [ACTIVATE(host, aid, c) for c in payment_choices(st, p, cost)]
```
No pairing (13-2-4-2), no play modifiers, no Units/Bases. `p` may be the standby player.

## 12. `apply(st, action, *, check=True)` (`engine/game.py:780-818`)

```
dec = st.pending
if dec is None: raise IllegalActionError("no decision pending")
if check and action not in dec.options:                 # tuple membership, field equality
    raise IllegalActionError(f"illegal action {action} for {dec.kind.value}")   # uses Action.__str__
st.pending = None
st.action_count += 1
if st.frames and st.frames[-1].waiting:  I.resume(st, action)     # checked FIRST, by frame state
elif kind is CHOOSE_FIRST:   (section 6)
elif kind is REDRAW:         (section 6)
elif kind is MAIN:           _apply_main(st, action)
elif kind is BLOCK:          if action.kind is BLOCK: B.block(st, action.a);  step = BLOCK_END_CHECK
elif kind is ACTION_STEP:    (section 10)
elif kind is ORDER_TRIGGER:  _resolve_trigger(st, st.batches[-1].pop(action.a))
else: raise core.EngineError(f"unhandled decision {k}")          # StrEnum formats as its value
advance(st)
```
`step(st, action)` = `s = st.clone(); apply(s, action); return s` (`:961-965`).
`concede(st, player)` (`:763-771`): no-op if `winner` set; else `frames = []`, `batches = []`,
`pending_triggers = []`, `core.set_winner(st, {player}, CONCEDE)`.

### 12.1 `_play_or_activate(st, p, action)` (`engine/game.py:874-954`)

`dv = V.derived(st)` is taken **at entry** (before reveal/payment).

ACTIVATE (`host=a, aid=b, ex=c`):
1. If `aid == -10` (Support): `record("activate", p, p, host)`;
   `push_frame(R.support_program, controller=p, host=host, kind="activated")`; return. No
   payment, `ex` ignored. Support program = `Rest(This)`, `Choose("t", friendly BATTLE, not This)`,
   `Apply(t, StatMod(ap=KwAmount(SUPPORT)), this_turn)`, `CustomStep("support_used")`
   (`effects/registry.py:60-65`).
2. `res = sum(c.amount for c in ab.costs if PayResources)`.
3. If `res` and not `I.pay_generic(st, p, res, ex)`: `EngineError("activation payment failed")`.
4. If `once_per_turn`: `once_used.add((aid, host, cards[host].zone_seq))` — at activation time.
5. `record("activate", p, p, host)`; if `res`: `record("cost_paid", p, p, host)` and
   `emit(COST_PAID, host, player=p, amount=res)` (no `next_group`).
6. `push_frame(a.program_id, controller=p, host=host, card_uid=_card_uid_for(st, a, host),
   kind="activated")`. The non-resource costs are the program's prefix (`registry.cost_steps`).

PLAY_UNIT / PLAY_BASE / PAIR / PLAY_COMMAND (`uid = a`):
1. If `c >= 100`: `alt = c // 100 - 1`; `ex = c % 100`; `entry_pm = play_modifiers(uid)[alt]`;
   `cost = pm.cost if not None else play_cost(st, dv, uid)`; `alt_cost_program =
   entry_pm.cost_program_id`. Else `ex = c`, `cost = play_cost(st, dv, uid)`, no alt program.
2. `cards[uid].known = BOTH_KNOW` (reveal, rule 7-5-2-2-1; no touch).
3. `_pay`: `I.pay_generic(st, p, cost, ex)` else `EngineError("payment failed for a legal
   action")`; then `_consume_player_cost_mods(st, uid)`.
   `pay_generic` (`engine/interp.py:1442-1469`): no-op `True` if `cost <= 0`; rests the first
   `cost - ex` active non-EX Resources in RESOURCE_AREA order; for the first `ex` active EX
   Resources: rest, `core.remove_from_game`, `record("ex_exiled", p, p, u)`,
   `emit(EXILED, u, player=p, by=p, ex_resource=1)`; then `touch()`.
4. `record("play", p, p, uid)`; if `ex > 0`: `record("played_with_ex", p, p, uid)`.
5. Kind:
   - PLAY_UNIT/PLAY_BASE: `fr = push_frame(system_program("deploy"), controller=p, host=uid,
     kind="system", vars={"card": (uid,)})`; `fr.ints["ex_used"] = ex`.
   - PAIR: `push_frame(system_program("pair"), controller=p, host=uid, kind="system",
     vars={"pilot": (uid,), "unit": (action.b,)})`.
   - PLAY_COMMAND: `core.move(uid, RESOLVING, reveal=True)`; `record("command_activated", p, p,
     uid)`; `emit(COMMAND_PLAYED, uid, player=p, ex_used=ex)` (no `next_group`);
     `push_frame(R.abilities[entry.command_aid].program_id, controller=p, host=uid,
     kind="command")`.
   - else `EngineError(f"unexpected play action {action}")`.
6. If `alt_cost_program >= 0`: `push_frame(alt_cost_program, controller=p, host=uid,
   kind="system")` — pushed **on top**, so the extra costs of the play modifier execute before the
   deploy/pair/command frame.

`_consume_player_cost_mods(st, uid)` (`:840-871`): return if `lasting` empty; for each `le` in
order, it "applies" when `le.player == owner(uid)` and `le.uses > 0` and
`R.continuous[le.effect_key]` is a `CostMod` and (`le.filters_key == -1` or
`V.matches(st, V.derived(st), Ctx(le.controller, le.source_uid), uid, R.filter_sets[filters_key])`).
Applying: `le.uses -= 1` (in place) and drop it when it reaches 0. If anything applied:
`lasting = kept list` and `touch()`. (`uses == 0` from creation means unlimited and is never
consumed.) Note the filter is evaluated on the **post-payment** state.

## 13. Triggers (`engine/game.py:667-727`)

- `_form_batch`: `batch = pending_triggers; pending_triggers = []; batches.append(batch)` (newest
  batch on top, resolves first — rule 10-1-6-7).
- `_trigger_candidates(st, batch)`: indices of Bursts if any; else indices with
  `controller == st.active`; else all indices (`list(range(len(batch)))`). Index order ascending.
- `_start_next_trigger(st)`:
  ```
  batch = batches[-1]
  if not batch: batches.pop(); return
  cands = _trigger_candidates(st, batch)
  if len(cands) > 1 and all(batch[i].program_id == first.program_id and
                            batch[i].ability_key == first.ability_key and
                            batch[i].burst == first.burst and batch[i].host == first.host
                            for i in cands):          # first = batch[cands[0]]
      cands = cands[:1]
  if len(cands) == 1: _resolve_trigger(st, batch.pop(cands[0])); return
  pending = Decision(batch[cands[0]].controller, ORDER_TRIGGER,
                     tuple(Action(ORDER, i) for i in cands), "order triggers")
  ```
  The chosen index is popped from the **current** batch list (indices shift after every pop).
- `_resolve_trigger(st, t)`: if `t.once_key` and it is in `once_used`: drop silently (already
  popped). `kind = "burst" if t.burst else "trigger"`; if Burst: `record("burst_revealed",
  controller, controller, t.card_uid)`. `f = push_frame(t.program_id, controller=t.controller,
  host=t.host, card_uid=t.card_uid, kind=kind, event=t.event)`; `f.once_key = t.once_key`.
  `push_frame` recomputes `host_seq` from the host's **current** `zone_seq`
  (`engine/interp.py:72`); `TriggerInst.host_seq` and `.optional` are not consulted here.

## 14. Battle (`engine/battle.py`)

### 14.1 Attack declaration — `declare_attack(st, attacker, target)` (`:12-32`)
```
a.rested = True; touch()
defender = 1 - a.owner
b = Battle(attacker, a.zone_seq, target, cards[target].zone_seq if target >= 0 else 0,
           defender, battle_id = st.next_battle_id)
next_battle_id += 1; battles.append(b); touch()
record("attack", a.owner, a.owner, attacker)
next_group
emit(ATTACKS, attacker, player=a.owner, target=target, defender=defender)
step = ATTACK_END_CHECK
```
Event tuple: `(("subject",attacker),("player",owner),("group",g),("defender",d),("target",t))`
(extras sorted by key, `engine/core.py:328-333`). The 【Attack】 triggers then resolve before
ATTACK_END_CHECK runs.

### 14.2 `battle_broken(st, b)` (`:35-44`, rules 8-2-4, 8-3-5, 8-4-2)
True if attacker's zone is not BATTLE or its `zone_seq != b.attacker_seq`; or, when `b.target >=
0`, the target's zone is not in (BATTLE, BASE) or `zone_seq != b.target_seq`. A player target is
never "broken".

### 14.3 Block
`eligible_blockers(st, b)` (`:47-73`):
```
dv
if has_kw(attacker, HIGH_MANEUVER) or rules_of(attacker, CANT_BE_BLOCKED): return []
no_vs = rules_of(attacker, NO_BLOCKER_VS)
for uid in zones[b.defender][BATTLE]:
    skip if rested, or uid == b.target, or not has_kw(uid, BLOCKER), or rules_of(uid, CANT_BLOCK)
    skip if any r in no_vs has (not r.rule.source_filters or
             V.matches(st, dv, Ctx(r.controller, r.source), uid, r.rule.source_filters))
    out.append(uid)
```
`block(st, blocker)` (`:76-96`): `rested = True`; `b.target = blocker`; `b.target_seq =
blocker.zone_seq`; `b.blocked = True`; `touch()`; `record("block", owner, owner, blocker)`;
`g = next_group`; `emit(BLOCKS, blocker, player=blocker.owner, attacker=b.attacker, group=g)`;
`emit(BLOCKED, b.attacker, player=attacker.owner, blocker=blocker, group=g)`.

### 14.4 `damage_step(st)` (`:196-238`, rule 8-5)
```
b = battle; dv = derived; attacker = b.attacker; a_owner = owner(attacker)
first_strike = has_kw(dv, attacker, FIRST_STRIKE)       # attacker only, read before any damage
if b.target == -2:                                       # attack on the player
    defender = b.defender; base = zones[defender][BASE]
    if (base or zones[defender][SHIELD]) and core.shield_area_protected(st, defender, attacker, True, a_owner):
        return
    if base: _attack_base(st, attacker, base[0])
    elif zones[defender][SHIELD]:
        ap = ap_of(attacker)                             # same dv
        if ap >= 1:                                      # Q35, 5-5-5
            n = 2 if has_kw(attacker, SUPPRESSION) else 1             # rule 13-1-7
            shields = core.shields_receiving_damage(st, zones[defender][SHIELD][:n], ap,
                                                    source=attacker, battle=True, by=a_owner)
            core.destroy_shields(st, defender, shields, battle=True, source=attacker, by=a_owner)
    else:
        ap = ap_of(attacker)
        if ap > 0:
            record("player_damage", defender, a_owner, attacker)
            core.set_winner(st, {defender}, BATTLE_DAMAGE)
    return
target = b.target
if cards[target].zone is BASE: _attack_base(st, attacker, target); return
_exchange(st, attacker, target, first_strike)
```
Suppression: the top `n` shields (`[:n]`, index 0 first) each receive the full AP; protection is
checked per shield; the surviving list is destroyed simultaneously by `core.destroy_shields`
(reveal, Burst triggers queued, trash, then `SHIELD_DESTROYED`/`DESTROYS_SHIELD_CARD` events,
`engine/core.py:700-749`). Suppression does nothing extra against a Base.

`_attack_base(st, attacker, base)` (`:150-162`): `core.damage_card(st, base, ap_of(attacker),
source=attacker, battle=True, by=owner(attacker))` then `_destroy_battle(st, {base: attacker})`.
Bases deal no damage back.

`_exchange(st, attacker, target, first_strike)` (`:165-193`):
```
dv; a_owner; t_owner; atk_ap = ap_of(attacker); tgt_ap = ap_of(target)
if first_strike:                                                   # rule 13-1-5
    hit = core.resolve_damage(st, target, atk_ap, source=attacker, battle=True, by=a_owner)
    core.apply_damage(st, *hit, source=attacker, battle=True, by=a_owner)
    _destroy_battle(st, {hit[0]: attacker})
    if cards[target].zone in (BATTLE, BASE) and cards[attacker].zone is BATTLE:   # 13-1-5-2
        dv = derived(st)                                           # recomputed: AP after the hit
        back = core.resolve_damage(st, attacker, ap_of(st, dv, target), source=target, battle=True, by=t_owner)
        core.apply_damage(st, *back, source=target, battle=True, by=t_owner)
        _destroy_battle(st, {back[0]: target})
    return
to_target   = resolve_damage(target,   atk_ap, source=attacker, by=a_owner)    # both resolved on the
to_attacker = resolve_damage(attacker, tgt_ap, source=target,   by=t_owner)    # pre-damage state (8-5-3-2)
apply_damage(*to_target, source=attacker, ...); apply_damage(*to_attacker, source=target, ...)
_destroy_battle(st, {to_target[0]: attacker, to_attacker[0]: target})          # 8-5-3-2-3
```
`resolve_damage` returns `(recipient_uid, amount)` after redirection (REDIRECT_BATTLE_DAMAGE),
prevention and reduction (`engine/core.py:576-599`); `apply_damage` adds damage, touches, and
emits `DAMAGED` then `DEALS_DAMAGE` (`engine/core.py:602-633`).

`_destroy_battle(st, victims: dict[uid -> destroyer uid])` (`:99-147`):
```
dv
doomed = [u for u in victims if zone(u) in (BATTLE, BASE) and damage(u) >= hp_of(u)]   # dict order
if not doomed: return []
breach = {src: kw_amount(dv, src, BREACH) for src in set(victims.values())}          # lookups only
paired = {src: cards[src].pair >= 0 for src in set(victims.values())}               # before destroy
sources_lki = core.ability_snapshot(st, sorted(set(victims.values())))
destroyed = core.destroy(st, doomed, battle=True, by=-1)
g = next_group
for u in destroyed:
    src = victims[u]
    lki = {src: sources_lki[src]} if cards[src].zone is not BATTLE else None        # Q436
    if owner(src) == owner(u): continue
    if def(u).card_type.is_base:                         # def after the move (variant reverted)
        emit(DESTROYS_SHIELD_CARD, src, player=owner(src), lki=lki, target=u, battle=1, group=g); continue
    record("destroys_by_battle", owner(src), owner(src), src)
    if paired[src]: record("destroys_by_battle_paired", owner(src), owner(src), src)   # Q452/Q453
    emit(DESTROYS_BY_BATTLE, src, player=owner(src), lki=lki, target=u, battle=1,
         breach=breach.get(src, 0), group=g)
return destroyed
```
**Breach** is not resolved here: the `breach` payload of `DESTROYS_BY_BATTLE` is read by
`core._keyword_triggers`, which queues `R.breach_program` (ability key `(-3,0,0)`) when amount >
0, the source's owner is the active player, the victim is a Unit, and the victim's owner still has
a Base or Shield (`engine/core.py:460-482`). `DESTROYS_BY_BATTLE` and `TURN_END` are always
listened (`effects/registry.py:360`), so `emit` never short-circuits them.

### 14.5 `end_battle(st)` (`:241-258`, rule 8-6-1)
`b.ended = True`; remove from `lasting` and `delayed` every entry with `duration ==
"this_battle" and battle_id == b.battle_id` (order preserved); `touch()`; `next_group`;
`emit(BATTLE_END, b.attacker, player=owner(attacker), target=b.target)`. The battle object is
popped later at BATTLE_END_DONE.

### 14.6 Damage-only battles — `damage_only_battle(st, attacker, target)` (`:261-282`)
Called from the `StartBattle` effect instruction (`engine/interp.py:1495-1506`), i.e. inside a
frame:
```
b = Battle(attacker, a.zone_seq, target, t.zone_seq, defender=t.owner, battle_id=next_battle_id,
           damage_only=True, attack_triggers=False)
next_battle_id += 1; battles.append(b); touch()
damage_step(st)
if st.winner is None: end_battle(st)
battles.pop(); touch()
```
No attack declaration, no block, no action step, no ATTACKS event, no step change.

## 15. Game end, safety caps, events, history, touch points

Winner encoding: `None` while running; 0/1 = winning player; -1 = draw. `end_reason` `None`
until set. `core.set_winner(st, losers, reason)` (`engine/core.py:842-854`): no-op if already
set; `len(losers) == 2` → winner -1, `BOTH_DEFEATED`; else winner `1 - loser`, `reason`; then
`phase = step = GAME_OVER`, `pending = None`.

| End | Where | Result |
| --- | --- | --- |
| Battle damage to a player with no Base and no Shields, AP > 0 | `battle.py:229-232` | loser = defender, `battle_damage` |
| Empty deck at any rules-management check | `core.py:864-867` | `deck_out` (both empty → `both_defeated`, winner -1) |
| Concede | `game.py:763-771` | `concede` |
| Turn cap: TURN_END of turn `turn_limit` (default 200) | `game.py:327-332` | winner -1, `turn_limit` |
| Decision cap: at the top of an `advance` iteration with `action_count > max_actions` (default 20000) | `game.py:737-742` | winner -1, `turn_limit` (same reason string) |

The decision cap therefore ends the game right after the `(max_actions + 1)`-th applied action;
verified: `max_actions=0` → 1 action applied, `=1` → 2, `=3` → 4. The over-cap action's
immediate dispatch (e.g. pushing frames) has happened, but `advance` stops before running them.
After `set_winner`, `core.emit` returns immediately (`engine/core.py:326-327`).

Events emitted in this area (`group` is always the 3rd payload entry; extras sorted by key):

| Site | `next_group` first? | Event | subject | player | extras (sorted) |
| --- | --- | --- | --- | --- | --- |
| START_STEP | yes | TURN_START | -1 | active | — |
| END_STEP | yes | TURN_END | -1 | active | — |
| `declare_attack` | yes | ATTACKS | attacker | owner | defender, target |
| `block` | yes (g captured) | BLOCKS | blocker | blocker owner | attacker |
| `block` | same g | BLOCKED | attacker | attacker owner | blocker |
| `_destroy_battle` | yes (after destroy) | DESTROYS_SHIELD_CARD | src | src owner | battle=1, target (lki kwarg separate) |
| `_destroy_battle` | same g | DESTROYS_BY_BATTLE | src | src owner | battle=1, breach, target |
| `end_battle` | yes | BATTLE_END | attacker | attacker owner | target |
| ACTIVATE with resources | no | COST_PAID | host | p | amount |
| PLAY_COMMAND | no | COMMAND_PLAYED | uid | p | ex_used |

History records (`HistoryEvent(kind, player, by, uid, def_id=cards[uid].def_id at record time,
source=-1)`, `engine/core.py:142-151`): `attack`, `block`, `destroys_by_battle`,
`destroys_by_battle_paired`, `player_damage` (player=defender, by=attacker owner),
`burst_revealed`, `activate`, `cost_paid`, `play`, `played_with_ex`, `command_activated`. History
is reset (`= []`, a new list) at SETUP_SHIELDS and at every `_next_turn`.

`touch()` call sites in this area (each +1 `version`): `game.py:191` (SETUP_SHIELDS end), `:220`
(`_expire`, always), `:237` (ACTIVE_STEP, always), `:298` (BATTLE_END_DONE), `:339`
(`_next_turn`, not on the cap path), `:871` (`_consume_player_cost_mods`, only if changed);
`battle.py:16,28` (two in `declare_attack`), `:85` (`block`), `:256` (`end_battle`), `:277,282`
(`damage_only_battle`); `observe.py:82,130`. Plus the core/interp primitives called: one per
`core.move` (pilot-follow included), `new_card`, `shuffle_deck`, `remove_from_game`,
`apply_damage` with amount > 0, `pay_generic` with cost > 0, `interp.pair`, etc. Silent passes
and decision creation never touch.

## 16. `observe.py` — information sets and determinization

`is_hidden_from(st, uid, observer)` (`:22-24`): `c.zone in HIDDEN_ZONES and not (c.known & (1 <<
observer))` (Python precedence: `&` binds tighter than `not`).

`unknown_instances(st, observer, owner)` (`:27-36`): uids in `zones[owner][z]` for z in
HIDDEN_ZONES with the observer bit clear, then `sort()` ascending (so frozenset iteration order
is irrelevant). Includes the observer's **own** deck and Shields (unknown to their owner too); the
owner's own hand and resource deck are known (bits set at draw / creation).

`unknown_pools(st, observer, owner)` (`:39-53`):
```
main = Counter(st.decklists[owner]); res = Counter(st.resource_decklists[owner])
for c in st.cards:                                   # uid order, all cards incl. OUTSIDE/tokens
    if c.owner != owner or (c.zone in HIDDEN_ZONES and not c.known & bit): continue
    def_id = db.base_def_id(c.def_id)                # Unit variant -> printed card
    if main[def_id] > 0: main[def_id] -= 1           # missing key reads 0, not inserted
    elif res[def_id] > 0: res[def_id] -= 1
return sorted(main.elements()), sorted(res.elements())     # ascending def_ids with multiplicity
```
Tokens (EX Base/Resource, unit tokens) are not in the decklists and decrement nothing.

`_fit(pool, n)` (`:98-110`): `len(pool) >= n` → `pool[:n]` (drops the **largest** def_ids, the
pool being sorted); empty pool with `n > 0` → `ValueError("cannot determinize: empty pool for
hidden cards")`; else pad by repetition `pool[0], pool[1], ...` cycling until length n.

`determinize(st, observer, seed)` (`:56-83`):
```
s = st.clone(); rng = SplitMix64(seed)               # local RNG, not the game RNG
for owner in (0, 1):
    slots = unknown_instances(s, observer, owner)
    if not slots: continue                           # no RNG use for this owner
    res_slots  = [u for u in slots if s.cards[u].zone is RESOURCE_DECK]
    main_slots = [u for u in slots if s.cards[u].zone is not RESOURCE_DECK]   # DECK, HAND, SHIELD
    main_pool, res_pool = unknown_pools(s, observer, owner)
    main_pool = _fit(main_pool, len(main_slots)); res_pool = _fit(res_pool, len(res_slots))
    rng.shuffle(main_pool); rng.shuffle(res_pool)    # order: main then resource
    assign main_pool[i] -> s.cards[main_slots[i]].def_id; res_pool[i] -> res_slots[i]   (strict zip)
s.history = _own_history(s)
s.rng = SplitMix64(derive_seed(seed, "hidden-rng"))  # replaces the game RNG (future shuffles)
s.touch()
```
RNG consumption: owner 0 main shuffle, owner 0 resource shuffle, owner 1 main, owner 1 resource;
a shuffle of k items uses k-1 `randrange` calls (0 for k ≤ 1). `known` bits, zones, decklists and
`s.seed` (the true game seed, only serialized) are unchanged. `derive_seed(master, *path)` =
first 8 bytes big-endian of `sha256(repr((master & (2^64-1), *path)).encode())` (`rng.py:52-59`);
for this call the repr is `(<int>, 'hidden-rng')` (Rust: `crates/gcg-core/src/rng.rs:108`).

`_own_history(s)` (`:86-95`): for each entry `h`: keep the same object if `h.uid < 0` or `h.def_id
== s.cards[h.uid].def_id`; else `dataclasses.replace(h, def_id=s.cards[h.uid].def_id)`. This
rewrites **any** mismatching entry, not only hidden ones (e.g. an entry recorded while a card was a
Unit variant, when the card has since reverted to its printed def_id).

`permute_hidden(st, observer, seed)` (`:113-131`, used only by tests): `s = clone`; `rng =
SplitMix64(seed)`; for owner in (0, 1) (no skip): `slots = [u for u in unknown_instances(...) if
zone is not RESOURCE_DECK]`; `ids = [def_id of slots]`; `rng.shuffle(ids)`; assign back in slot
order. Then `_own_history`, `s.rng = SplitMix64(derive_seed(seed, "permuted-rng"))`, `touch()`.

`information_set_key(st, observer)` (`:134-150`, used only by tests):
`(cards, zones, st.turn, st.active, int(st.step), repr(pending))` where `cards = tuple((uid, -1 if
is_hidden_from(...) else def_id, owner, int(zone), rested(bool), damage, pair) for c in
st.cards)`, `zones = tuple(tuple(tuple(z) for z in pz) for pz in st.zones)` (2 × 12 lists incl.
the always-empty OUTSIDE list), `pending = st.pending.to_json() if st.pending else None`.
`repr` of the dict uses Python dict repr, e.g. `{'player': 1, 'kind': 'main', 'options':
[['end_main', -1, -1, -1]], 'prompt': 'main phase', 'context': []}`; `repr(None) == 'None'`. A
Rust port only needs equality semantics (tests compare keys of two states).

Production callers of `determinize`: `ai/agents.py:57`, `ai/actions.py:149,219`,
`ai/noop.py:150`, `ai/setup.py:115`, `ai/mcts.py:128,161`; `unknown_pools`: `ai/evaluation.py:137`.

## 17. `invariants.py` — `check(st, initial=None)` (`:29-109`)

Collects messages in this order, raises `InvariantViolation("; ".join(errors[:10]))`
(`AssertionError` subclass) if any:
1. For p in (0,1), z in LISTED_ZONES, uid in `zones[p][z]`: `seen[uid] += 1`;
   `"uid {uid} listed in {z.name} but zone={c.zone.name}"` if `c.zone is not z`;
   `"uid {uid} owned by {c.owner} listed under player {p}"` if owner differs.
2. For c in `st.cards`: if `c.zone is OUTSIDE`: `"uid {uid} is OUTSIDE but listed"` if seen;
   `"non-token uid {uid} ({card_number}) left the game"` if not a token; continue. Else:
   `"uid {uid} ({card_number}) listed {n} times"` if seen ≠ 1; `"token uid {uid} in {zone.name}"`
   if token outside TOKEN_ZONES; `"uid {uid} has negative damage"`; `"uid {uid} has damage {d} in
   {zone.name}"` if damage ≠ 0 outside BATTLE/BASE; PAIRED: `"pilot uid {uid} paired
   inconsistently with {u}"` unless `pair >= 0`, the unit is in BATTLE and points back; BATTLE:
   `"unit uid {uid} has inconsistent pilot {pair}"` if paired and the pilot is not PAIRED or does
   not point back, and `"non-unit {card_number} in battle area"` if not a unit type; any other
   zone with `pair >= 0`: `"uid {uid} in {zone.name} still paired with {pair}"`; then (independent)
   `"non-base {card_number} in base section"` and `"non-resource {card_number} in {zone.name}"`
   (RESOURCE_AREA/RESOURCE_DECK).
3. For p: `"player {p} has {n} Units (limit 6)"` if BATTLE > 6; `"player {p} has {n} Bases"` if
   BASE > 1; `"player {p} resource area over limit"` if RA > 15 or EX Resources > 5.
4. If `initial` given: `Counter((owner, base_def_id(def_id)) for non-token cards)` must equal it,
   else `f"card multiset changed: {now - initial} / {initial - now}"` (Counter reprs).
5. If `winner is None`: `"no pending decision in an unfinished game"` if `pending is None`; elif no
   options `"decision {kind.value} has no options"`; and if `phase is not SETUP and not frames`:
   for p, z in (BATTLE, BASE): `"uid {uid} survives with lethal damage"` if `damage >= hp_of` and
   no `CANT_BE_DESTROYED` rule.

`initial_multiset(st)` (`:112-116`) builds the Counter of step 4. Used by `tools/fuzz.py`.

## 18. Reference vectors (all produced in this session with `uv run python`)

Decks `examples/decks/red-green-zeon.txt` (p0) vs `examples/decks/blue-white-federation.txt`
(p1), loaded with `deck.parse.load_deck` + `to_decklist`, `seed = 12345`, default caps:

- After `new_game`: `setup_chooser = 1`, `rng.state = 7774064181650859138`, `version = 120`,
  `len(cards) = 120`, pending `{'player': 1, 'kind': 'choose_first', 'options': [['go_first', 1,
  -1, -1], ['go_first', 0, -1, -1]], 'prompt': 'choose Player One', 'context': []}`;
  `decklists[0][:10] = (178, 178, 178, 178, 179, 179, 179, 179, 183, 183)`.
- Apply `go_first(1)`, `redraw` (P1), `keep` (P0): rng after the redraw
  `11343426241713698846`; at turn-1 MAIN `version = 160`, `event_group = 2`, `action_count = 3`;
  shields p0 `[10, 9, 8, 7, 6, 5]`, p1 `[94, 93, 82, 99, 73, 91]`; Bases p0 `[121]`, p1 `[120]`;
  RESOURCE_AREA p0 `[122]` (EX Resource), p1 `[110]`; history `[['draw', 1, 1, 95, 922, -1]]`;
  options `['end_main()']`.
- `derive_seed(7, "hidden-rng") = 9350294383882453514`, `derive_seed(7, "permuted-rng") =
  3847278567517088389`. On the state above, `determinize(st, 0, 7)`: opponent hand
  `[(55,156),(56,156),(57,922),(58,156),(59,922),(95,235)]`;
  `sha256(json.dumps([c.def_id for c in s.cards]).encode()).hexdigest()[:16] = 705bb3fba1022b6f`
  (default `json.dumps` separators); `permute_hidden(st, 0, 7)` same digest `3b34a62ec2f10430`.
- "Always option 0" policy from `new_game`: 69 decisions; winner 1, `battle_damage`, turn 11,
  active 1, `action_count 69`, `version 357`, `rng.state 7774064181650859138` (no shuffles),
  `event_group 123`, `next_battle_id 17`, `state_digest =
  5ce28ed973cf9e06670a38f6a320c26a9a2c9f15fa5066e8fd8a6744c2f31e9a`. First decisions
  (`turn:step:kind:player:n_options:chosen`): `0:SETUP_CHOOSE_FIRST:choose_first:P1:2:go_first(1)`,
  `0:SETUP_REDRAW_P1:redraw:P1:2:keep()`, `0:SETUP_REDRAW_P2:redraw:P0:2:keep()`,
  `1:MAIN:main:P1:1:end_main()`, `2:MAIN:main:P0:2:play_unit(2,1)`, `2:MAIN:main:P0:1:end_main()`,
  `3:MAIN:main:P1:3:play_unit(53,0)`, `3:MAIN:main:P1:2:play_unit(62,0)`,
  `3:MAIN:main:P1:1:end_main()`, `4:MAIN:main:P0:3:play_command(12,0)`,
  `4:MAIN:select:P0:2:select(53)`, `4:MAIN:main:P0:2:attack(2,-2)`, `4:MAIN:main:P0:1:end_main()`,
  `5:MAIN:main:P1:6:play_unit(54,0)`, `5:MAIN:main:P1:4:play_unit(63,0)`,
  `5:MAIN:main:P1:3:attack(62,-2)`, `5:MAIN:main:P1:1:end_main()`,
  `6:MAIN:main:P0:5:play_unit(0,0)`, `6:MAIN:main:P0:5:pair(1,2,0)`,
  `6:MAIN:main:P0:3:attack(2,-2)`, `6:MAIN:main:P0:1:end_main()`,
  `7:MAIN:main:P1:14:play_base(50,0)`, `7:MAIN:main:P1:18:play_unit(51,0)`,
  `7:MAIN:main:P1:8:activate(50,765,0)`.

## 19. Hazards (Python-semantic traps in this area)

1. **`version` is observable.** `to_json()` and `state_digest` include `version`
   (`engine/state.py:610`); every `touch()` in section 15 must be reproduced one-for-one,
   including unconditional ones (`game.py:220,237`) and the double touch in `declare_attack`
   (`battle.py:16,28`) and `damage_only_battle` (`battle.py:277,282`).
2. **Rules-management cache is behavioural.** `core.rules_management` skips when
   `_rules_checked == version` (`engine/core.py:861`), and only a run that reaches the scan clears
   `effect_hits` (`engine/core.py:874,885`). Skipping or adding a check changes which
   `DESTROYS_BY_BATTLE` events fire later. Port the cache exactly (and `_rules_checked = -1` after
   `from_json`, `engine/state.py:615` via `__init__`).
3. **Dict-literal duplicate key** in `_exchange` (`battle.py:193`): if both hits resolve to the same
   uid (redirection), `{x: attacker, x: target}` keeps the key's first position and the **last**
   value (`target`). `victims` iteration (`battle.py:102-106`) is dict insertion order.
4. `set(victims.values())` (`battle.py:109-111`) is only used for dict building and `sorted(...)`,
   so its order is irrelevant; `losers` set in `core.rules_management` is only sized/unpacked.
5. `frozenset` iteration over `HIDDEN_ZONES` (`observe.py:31`) is neutralized by `out.sort()`
   (`observe.py:35`); the Rust port may iterate in any order **then sort**.
6. `Counter` semantics in `unknown_pools` (`observe.py:41-53`): reading a missing key returns 0
   without inserting; `elements()` skips non-positive counts; `sorted()` ascending ints.
7. `_fit` trimming takes `pool[:n]` of an ascending-sorted pool (`observe.py:101-102`) — the
   highest def_ids are dropped, deterministically. Padding cycles from index 0.
8. Python `max()`/`min()` appear only on ints (`game.py:380-381`); no first/last-maximum issue in
   this area.
9. `//` and `%` in the alt-play decode (`game.py:910-911`) only see `c >= 100`, so no negative
   operand semantics arise. Do not generalize the decode to `c < 100`.
10. Lazily assigned program ids: `I.system_program("deploy"|"pair"|"hand_limit")` registers the
    program on **first use in the process** (`engine/interp.py:1589-1594`); in a fresh process the
    ids were 623/624/625 only because they were requested in that order. Frame `program_id` values
    in raw `to_json()` therefore depend on process history; `runner/digest.py:42-43` replaces them
    by content ids. `_SYSTEM_IDS` is a module-level cache that would go stale if the registry were
    rebuilt.
11. Event payload ordering: `emit` builds `(subject, player, group, *sorted(extra.items()))`
    (`engine/core.py:328-333`); sorting is by key string (code points). Whether `next_group` is
    called before an emit (table in section 15) changes `group`, which drives the
    once-per-group trigger dedup in `core.queue_trigger` (`engine/core.py:261-269`).
12. `apply` dispatches on `st.frames[-1].waiting` **before** `dec.kind` (`game.py:790`).
13. `Action` membership: `action not in dec.options` (`game.py:785`) uses field equality on a tuple
    (no hashing). `Action` *is* hashable and its hash includes `hash(str)` of the StrEnum value,
    which is randomized per process — never iterate a set/dict of Actions.
14. Silent passes (`game.py:349-352`) create no decision, do not touch, and do not increment
    `action_count`; generating a decision there would desynchronize replays.
15. Snapshot iteration: `_redraw` iterates `list(zones[p][HAND])` while moving cards out
    (`game.py:196`); SETUP_DRAW/SETUP_SHIELDS/`_redraw` re-read `zones[p][DECK][0]` each time.
16. `derived(st)` is captured **once** in ACTIVE_STEP (`game.py:227`) and reused while cards are
    being unrested (no touch inside the loop); likewise at the top of `main_options`,
    `_play_or_activate` (before payment), `damage_step` (First Strike and AP read before any
    damage) and `_exchange` (recomputed only in the First Strike branch, `battle.py:179`).
17. `_consume_player_cost_mods` mutates `le.uses` in place (`game.py:864`); `clone()` copies
    `Lasting` objects (`engine/state.py:552`), so clones are independent — keep that in Rust.
18. `clone()` shares `pending` (immutable) and the `_derived` cache object, and copies `history`
    shallowly (`engine/state.py:546,558,570`); `_own_history` therefore builds new entries with
    `dataclasses.replace` instead of mutating shared ones (`observe.py:86-95`).
19. Shield order reversal: `move(..., SHIELD)` inserts at 0 (`engine/core.py:103-104`), so shields
    are the reverse of the order they left the deck; damage hits `SHIELD[:n]`.
20. `chooser if chooser is not None else rng.randrange(2)` (`game.py:153`): explicit 0 is valid;
    RNG is consumed only when `None`.
21. Strict comparisons at the caps: `action_count > max_actions` (`game.py:737`) and `turn >=
    turn_limit` (`game.py:327`).
22. f-string formatting: `f"{s}"` of an `IntEnum` prints the integer (`game.py:322`), of a
    `StrEnum` prints its value (`game.py:817`); `f"{action}"` uses `Action.__str__`, which omits
    `-1` arguments but prints `-2` and `-10` (`engine/state.py:30-32`).
23. `information_set_key` uses `repr()` of a dict and bool fields (`observe.py:142,150`); only
    equality matters, but a Rust string form must be injective in the same way.
24. Exceptions as control flow: `IllegalActionError` is caught by `runner/replayfile.py:97` and
    `testkit.py:414`; `_fit`'s `ValueError` and `zip(strict=True)` would abort determinization;
    `EngineError` signals bugs. No integer-overflow paths: counters (`version`, `event_group`,
    `action_count`, `next_battle_id`) stay far below 2^63.
25. No `hash()`, `id()`, `random`, wall clock or float arithmetic in these four files.

## 20. Open questions

1. `Step.ATTACK_TRIGGERS` (50) and `DecisionKind.PAYMENT` are dead values. Keep them in the Rust
   enums for JSON compatibility, but is any external artifact (old replays, golden files) known to
   contain step 50? Not determined.
2. `Battle.blocked`, `damage_only`, `attack_triggers` are never read; `TriggerInst.host_seq`,
   `TriggerInst.optional` and `Frame.host_seq` are not consulted when a trigger resolves
   (`push_frame` recomputes `host_seq` from the current card). Intended, or latent bugs the port
   should preserve verbatim? (Preserving is required for byte-identical digests either way.)
3. `_own_history` rewrites history entries of non-hidden cards whose def_id changed (Unit variant
   reverted), which can make a determinized world's history differ from the true one for public
   cards. Intended?
4. `_activation_ok` counts the host itself among cost candidates (`game.py:457`) while
   `_costs_payable` excludes it (`game.py:601`). Asymmetry intended?
5. Whether raw (non-canonical) `GameState.to_json()` with process-local program/effect ids is
   written to any byte-compared output (replays, games.ndjson); if so, the Rust registry must
   reproduce Python's first-use order. Owned by the runner/digest inventory; not checked here.
6. Player One draws on turn 1 and the resource deck is never shuffled; both are faithful to the
   code (not re-checked against the Comprehensive Rules here). The port must replicate them.
