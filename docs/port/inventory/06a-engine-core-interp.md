# 06a — Engine core (`engine/core.py`) and effect interpreter (`engine/interp.py`)

Scope: `src/gcg_sim/engine/core.py` (946 lines) and `src/gcg_sim/engine/interp.py` (1660 lines)
on branch `rust-port`. The trigger-batch code lives in `engine/game.py` but is documented here
because it is the consumer of everything `core.py` queues. Helpers from `engine/view.py`,
`engine/state.py`, `effects/program.py` and `effects/registry.py` are described only as far as
this area needs them (their own inventories own the details).

Conventions: `NO_ARG = -1`, `PLAYER_TARGET = -2` (`engine/types.py:138-139`). "touch" means
`st.touch()` = `st.version += 1; st._derived = None` (`state.py:529-531`). `R` = the global
registry `V.reg()`. "derived"/`dv` = `V.derived(st)`. Every `path:line` is in `src/gcg_sim/`
unless stated otherwise.

There is **no floating point anywhere in core.py or interp.py**: every quantity is a Python
`int`. The only randomness is `st.rng` (SplitMix64), listed exhaustively in section 15.

---------------------------------------------------------------------------------------------

## 1. Constants (core.py:14-27, interp.py:25-54)

| Name | Value | Use |
| --- | --- | --- |
| `BOTH_KNOW` | `0b11` (3) | `CardInstance.known` bitmask, bit `1 << player` |
| `HAND_LIMIT` | 10 | `system:hand_limit` program (interp.py:1584) |
| `BATTLE_LIMIT` | 6 | Unit excess (interp.py:884, 1003) |
| `BASE_LIMIT` | 1 | Base excess (interp.py:885) |
| `RESOURCE_LIMIT` | 15 | resource area cap (interp.py:1037, 1054) |
| `EX_RESOURCE_LIMIT` | 5 | EX Resource cap (interp.py:1037) |
| `SHIELD_COUNT` | 6 | setup (game.py) |
| `START_HAND` | 5 | setup (game.py) |
| `REDUCE_ONCE_TAG` | -7 | once-key tag, damage reduction |
| `PREVENT_ONCE_TAG` | -8 | once-key tag, damage prevention |
| `REST_SUB_TAG` | -9 | once-key tag, rest substitution |
| `TOKEN_ZONES` | {BATTLE(3), RESOURCE_AREA(2), SHIELD(4), BASE(5)} | zones a token may exist in |
| `BURST_PROMPT` | `"activate Burst?"` | also `registry.py:150` |
| `HOLD_RULES` | `"__hold_rules"` | key in `Frame.ints` |
| `EngineError` | exception class | internal bug signal (core.py:30) |

`Zone` (types.py:8-22): DECK=0, RESOURCE_DECK=1, RESOURCE_AREA=2, BATTLE=3, SHIELD=4, BASE=5,
REMOVAL=6, HAND=7, TRASH=8, RESOLVING=9, OUTSIDE=10, PAIRED=11.
Public zones (types.py:33-43): RESOURCE_AREA, BATTLE, BASE, REMOVAL, TRASH, RESOLVING, PAIRED.
(DECK, RESOURCE_DECK, SHIELD, HAND, OUTSIDE are not public.)

`V.zone_loc_code(zone)` (view.py:952-954) = index of the `d.Loc` in enum order
(BATTLE 0, PAIRED 1, BASE 2, SHIELDS 3, SHIELD_AREA 4, RESOURCE_AREA 5, HAND 6, TRASH 7, DECK 8,
DECK_TOP 9, RESOURCE_DECK 10, REMOVAL 11, FIELD_UNITS_AND_BASES 12). Zone mapping (verified by
running it): DECK→8, RESOURCE_DECK→10, RESOURCE_AREA→5, BATTLE→0, SHIELD→3, BASE→2, REMOVAL→11,
HAND→6, TRASH→7, PAIRED→1, RESOLVING→-1, OUTSIDE→-1.

---------------------------------------------------------------------------------------------

## 2. Data records touched (field order = JSON order; full definitions in state.py)

- `CardInstance` (state.py:71-141): uid, def_id, owner, zone, rested, damage, pair (uid of
  partner or -1), zone_seq, entered_turn, known. JSON: `[uid, def_id, owner, int(zone),
  int(rested), damage, pair, zone_seq, entered_turn, known]`.
- `TriggerInst` (state.py:265-322): program_id, controller, host, host_seq, card_uid,
  ability_key `(provider def_id, aid, unit_text 0/1)`, event, burst=False, once_key=(),
  optional=False. `optional` is never set to True anywhere. `host_seq` is used only inside the
  once key; it is never read at resolution time.
- `Frame` (state.py:325-411): program_id, controller, host, host_seq, card_uid, kind, event=(),
  pc=0, vars={}, did=True, answer=False, waiting=False, buf=[], ints={}, once_key=(),
  acted=False. JSON sorts `vars` and `ints` by key (state.py:382, 387).
- `Decision` (state.py:35-68): player, kind, options (tuple of Action), prompt, context
  (tuple of (str, int)).
- `Action` (state.py:14-32): kind, a=-1, b=-1, c=-1. Equality is field-wise (frozen dataclass);
  `game.apply` checks `action in dec.options` (game.py:785).
- `HistoryEvent` (state.py:414-430): kind, player, by, uid, def_id, source=-1.
- `Lasting` (state.py:194-262): effect_key, controller, source_uid, targets ((uid, seq)...),
  duration (string value), created_turn, battle_id, player=-1, uses=0, filters_key=-1, aux=-1,
  serial=-1.
- `Snapshot` (core.py:485-491): `abilities: tuple[AbilityEntry,...]`, `seq: int` (LKI).

`GameState` counters this area mutates: `version`, `event_group` (starts 0, state.py:505),
`next_lasting_serial` (starts 0), `once_used` (set of int tuples), `history` (list),
`effect_hits` (dict uid→source uid), `pending_triggers` (list), `batches` (list of lists),
`frames` (stack), `delayed`, `lasting`, `pending`, `_rules_checked` (starts -1, **not
serialized**, copied by clone; state.py:511, 571).

---------------------------------------------------------------------------------------------

## 3. Zone movement and new-card semantics

### 3.1 `move(st, uid, dst, *, bottom=False, reveal=False, rested=False)` (core.py:55-81)

```
c = cards[uid]; src = c.zone
if c.pair >= 0:
    other = cards[c.pair]
    if src is BATTLE:            # a Unit leaving with its Pilot (rule 3-3-6)
        other.pair = -1; c.pair = -1
        _move_single(other, dst, bottom, reveal, rested=False)   # PILOT MOVES FIRST
    elif src is PAIRED:          # a Pilot leaving alone: Unit stays, both unpaired
        other.pair = -1; c.pair = -1
_move_single(c, dst, bottom, reveal, rested)
touch()                          # exactly ONE touch per move() call, after both singles
```

Consequence of "pilot first": to the top of an ordered zone the Unit ends above its Pilot
(both `insert(0)`), to the bottom/append the Pilot ends above the Unit (Unit is last). The
Pilot is always moved with `rested=False`.

### 3.2 `_move_single(st, c, dst, *, bottom, reveal, rested)` (core.py:84-106)

Exact order:
1. `src = c.zone`; `_remove_from_zone(st, c)` (core.py:38-44): no-op if `c.zone is OUTSIDE`, or
   if `c.zone is RESOLVING` and uid is not in `zones[owner][RESOLVING]`; otherwise
   `zones[owner][c.zone].remove(uid)` (first occurrence; ValueError if absent).
2. If `dst is not BATTLE`: `c.def_id = db.base_def_id(c.def_id)` (a "deployed as a Unit" variant
   reverts to the printed card; identity for all other defs; db.py:279-281).
3. `c.zone_seq += 1`; `c.damage = 0`; `c.rested = rested`; `c.entered_turn = st.turn`.
4. Knowledge (`_knowledge_on_enter`, core.py:47-52), using `src` from step 1:
   - `dst.is_public or reveal` → `known = 3`;
   - elif `dst is HAND` → `known = 3 if src.is_public else known | (1 << owner)`;
   - else (DECK, RESOURCE_DECK, SHIELD, OUTSIDE when not revealed) → unchanged.
5. If the (possibly reverted) def is a token (`CardType` UNIT_TOKEN, EX_BASE, EX_RESOURCE;
   model.py:21-22) and `dst not in TOKEN_ZONES`: `c.zone = OUTSIDE; c.known = 3; return`
   (not appended to any list; zone_seq/damage/rested/entered_turn were already updated).
6. `c.zone = dst`; list = `zones[c.owner][dst]` (always the OWNER's zone list);
   if `dst in (DECK, SHIELD, RESOURCE_DECK) and not bottom` → `list.insert(0, uid)` (top);
   else `list.append(uid)`. Note the default `bottom=False` puts a card on TOP of a deck.

### 3.3 Other primitives

- `remove_from_game(st, uid)` (core.py:109-116): `_remove_from_zone`; zone=OUTSIDE;
  `zone_seq += 1`; known=3; touch. (damage/rested/entered_turn untouched.)
- `new_card(st, def_id, owner, zone, *, rested=False) -> uid` (core.py:119-131):
  `uid = len(st.cards)`; `CardInstance(uid, def_id, owner, OUTSIDE)` (zone_seq 0, damage 0,
  pair -1); `known = 3`; append to `st.cards`; `zone = zone`; `rested`; `entered_turn = st.turn`;
  `zones[owner][zone].append(uid)` (always append, even for DECK); touch. No event.
- `shuffle_deck(st, player)` (core.py:134-139): `st.rng.shuffle(zones[player][DECK])` in place;
  then for every deck uid `known = 0`; touch. (RNG, section 15.)
- `_place_ordered(st, p, order, *, bottom)` (interp.py:1194-1202): `deck.remove(u)` for each u in
  `order`; then `deck.extend(order)` if bottom else `deck[0:0] = order` (order[0] becomes the
  top card); one touch. Does not change `known` or `zone_seq`.
- `trash_excess(st, uid)` (interp.py:904-910): `move(uid, TRASH)`; `record("excess", owner,
  owner, uid)`; then `if def is token and zone is TRASH: remove_from_game` (dead in practice:
  a token moved to TRASH is already OUTSIDE after `_move_single` step 5). Not a destruction:
  no DESTROYED event (rule 5-10-4). A paired Pilot goes to the trash with the Unit.

### 3.4 Pairing — `pair(st, pilot, unit, *, by)` (interp.py:1368-1393)

1. `from_zone = cards[pilot].zone`
2. `move(pilot, PAIRED)` (public → known=3; appended to `zones[pilot.owner][PAIRED]`; touch)
3. `cards[pilot].pair = unit; cards[unit].pair = pilot; touch()` (second touch)
4. `record("paired", cards[unit].owner, by, pilot)` (uid = the PILOT)
5. `dv = derived`; `group = next_group()`; `linked = unit in dv.linked`
6. emit `PAIRED(subject=unit, player=unit.owner, pilot, by, group, from_loc=zone_loc_code(from_zone), linked=int(linked))`
7. if linked: emit `LINKED(subject=unit, player=unit.owner, pilot, by, group)` (same group).

`can_pair(st, dv, unit)` (interp.py:1340-1344): `zone is BATTLE and pair < 0 and no
CANT_BE_PAIRED rule on unit`.

---------------------------------------------------------------------------------------------

## 4. `version` / touch catalogue (state digest includes `version`)

`version` is serialized (state.py:610) and hashed into `final_state_sha256`
(runner/digest.py:52-55), so the Rust port must touch at exactly these points, the same number
of times:

| Site | Count |
| --- | --- |
| `move` (core.py:81) | 1 per call (not per card, even when a Pilot follows) |
| `remove_from_game` (core.py:116) | 1 |
| `new_card` (core.py:130) | 1 |
| `shuffle_deck` (core.py:139) | 1 |
| `apply_damage` (core.py:611) | 1 (only when damage is actually applied) |
| `_h_rest_continue` (interp.py:709) | 1 per card rested |
| `_h_set_active` (interp.py:726) | 1 per card set active |
| `_h_deploy_card` as_unit (interp.py:931) | 1 per card converted to its Unit variant |
| `_enter_rested` (interp.py:986) | 1 per card forced rested |
| `_h_set_resources_active` (interp.py:1078) | 1 if any changed |
| `_h_rest_resources` (interp.py:1098) | 1 if any changed |
| `_place_ordered` (interp.py:1202) | 1 |
| `_h_recover` (interp.py:1242) | 1 per card healed |
| `add_lasting` (interp.py:1320) | 1 (not when it returns early for THIS_BATTLE outside a battle) |
| `pair` (interp.py:1374) | 1 (+1 from its move) |
| `pay_generic` (interp.py:1469) | 1 at end when cost > 0 and payable (+1 per EX Resource via remove_from_game) |
| `_h_change_target` (interp.py:1516) | 1 |
| `_h_custom` (interp.py:1574) | 1 after EVERY custom step, whatever it returned |
| `return_looked_bottom` (effects/builtins.py:28) | 1 when it returns True |

Mutations that do NOT touch: `record()` (history), `queue_trigger`/pending triggers,
`once_used`, `known` bits (`_h_look_top`, `_h_reveal`, `_h_to_deck` knowledge clear),
`st.delayed.append` (`_h_delayed`), `effect_hits.clear()`, frame fields, `event_group`.

---------------------------------------------------------------------------------------------

## 5. Events

### 5.1 `next_group(st)` (core.py:158-160)

`st.event_group += 1; return st.event_group`. Serialized (state.py:606). Every call site is
listed in 5.4; several call it unconditionally even when nothing happens (Rust must still
increment).

### 5.2 `emit(st, ev, subject=-1, *, player=-1, lki=None, **extra)` (core.py:316-356)

```
if st.winner is not None: return                       # events after game end are dropped
group = extra.pop("group", -1); if group == -1: group = st.event_group   # current value
event = (("subject", subject), ("player", player), ("group", group)) + tuple(sorted(extra.items()))
if ev not in listened_events(st) and not st.delayed and not lki: return
dv = derived(st); lki = lki or {}
candidates = [(host, a) for (host, a) in trigger_index(st, dv).get(ev, ()) if host not in lki]
for uid in sorted(lki, key=lambda u: (cards[u].owner != st.active, u)):    # active player first, then uid
    candidates += [(uid, a) for a in lki[uid].abilities if a.trigger_event is ev]
for host, a in candidates:
    owner = cards[host].owner
    if not _trigger_ok(st, dv, a.ability.trigger, host, owner, event): continue
    if a.ability.cond is not TRUE:
        ctx = Ctx(owner, host, _card_uid_for(st, a, host), event=event)     # vars = {}
        if not cond(st, dv, ctx, a.ability.cond): continue
    snap = lki.get(host)
    queue_trigger(st, a, host, owner, event, host_seq=snap.seq if snap else -1)
_delayed_triggers(st, dv, ev, event, lki)
_keyword_triggers(st, dv, ev, event, subject)
```

Event tuple layout (byte-exact): first three fixed pairs `subject`, `player`, `group`, then the
remaining keyword arguments sorted by key (ASCII; keys are unique). Example from a live run:
DESTROYED = `(("subject",121),("player",0),("group",21),("battle",1),("by",-1),("linked",0),("pilot",-1),("source",-1))`.
`player` is always present (default -1). The tuple is stored verbatim in `TriggerInst.event` /
`Frame.event` and serialized as a list of `[key, value]` pairs.

`listened_events(st)` (core.py:295-313): union of `R.always_events` (computed in
registry.py:356-378: `{TURN_END, DESTROYS_BY_BATTLE}` + every DelayedTrigger event + every token
script's trigger events; currently exactly `{destroys_by_battle, turn_end}`) and
`R.events_by_def[def_id]` for every def_id in both `st.decklists` and both
`st.resource_decklists`, plus the events of each def's Unit variant (`db.unit_variant`).
`events_by_def` follows AbilityGrant transitively (registry.py:381-398). The process cache
`_LISTEN_CACHE` (cleared above 256 entries) is a pure memo. The early return is a pure
optimisation only if Rust computes the identical set; port it exactly (a non-token card created
by `new_card` that is not in the decklists would be silently ignored by Python).

Trigger index order (`V.trigger_index`, view.py:314-333): players `(st.active, 1 - st.active)`;
per player zones BATTLE, BASE, PAIRED, HAND, TRASH; per zone list order; per card the order of
`dv.abilities[uid]` (own location entries in script order, then the paired Pilot's
`unit_entries` when the card is a paired BATTLE Unit, then granted abilities; gated ones
filtered; view.py:180-216); only abilities with `trigger_event is not None` (i.e. `d.Triggered`).

### 5.3 Payload key meanings

`subject` (card the event is about), `player` (usually the subject's owner), `group`
(simultaneity id), `by` (player whose effect/action caused it, -1 for rules/battle), `source`
(card uid causing it), `target`, `amount`, `battle` (1 = battle damage/destruction), `pilot`,
`linked`, `from_loc` (zone_loc_code of origin), `ex_used`, `as_command`, `by_effect`, `token`,
`ex_resource`, `breach`, `defender`, `attacker`, `blocker`.

### 5.4 Every emit site (all files; verified with an AST scan of `src/gcg_sim`)

Keys after `subject, player, group` are listed in their sorted (serialized) order. "group=cur"
means no explicit group: the event carries `st.event_group` as it is at that moment.

| Event (`Ev` value) | Site | subject / player | extra keys (sorted) | group |
| --- | --- | --- | --- | --- |
| `damaged` | core.py:612 | damaged uid / its owner | amount, battle, by, source | cur |
| `deals_damage` | core.py:623 (only if source>=0) | source / source owner | amount, battle, by, target | cur |
| `shield_destroyed` | core.py:730 | shield uid / `player` arg | battle, by, source | destroy_shields group |
| `destroys_shield_card` | core.py:741 (only if source>=0) | source / source owner | battle, target | same |
| `destroyed` | core.py:811 | uid / owner | battle, by, linked, pilot, source (+lki) | destroy group |
| `destroys_by_battle` or `destroys_shield_card` | core.py:914 | effect-damage source / its owner | battle(=0), target (+lki) | new group per call |
| `drawn` | interp.py:511 | uid / p | by_effect | draw group |
| `discarded` | interp.py:573 | uid / p | by | discard group |
| `rested` | interp.py:711 | uid / owner | by | rest group |
| `set_active` | interp.py:730 | uid / owner | by | set-active group |
| `returned_to_hand` | interp.py:769 | uid / owner | by (+lki of that uid) | return group |
| `exiled` | interp.py:844 | uid / owner | by | exile group |
| `exiled` (EX Resource spent) | interp.py:1468 | uid / p | by(=p), ex_resource(=1) | cur |
| `added_to_hand` | interp.py:871 | uid / owner | by | add group |
| `deployed` (card) | interp.py:967 | uid / owner | by, ex_used, from_loc | deploy group |
| `deployed` (token) | interp.py:1009 | uid / p | by, token(=1) | token group |
| `ex_resource_placed` | interp.py:1041 | new uid / p | by | cur |
| `resource_placed` | interp.py:1058 | uid / p | by | cur |
| `shield_to_hand` | interp.py:1112 | uid / p | by | group |
| `recovered` | interp.py:1244 | uid / owner | amount (healed) | cur |
| `ap_reduced` | interp.py:1272 | uid / owner | by | apply group |
| `paired` | interp.py:1379 | unit / unit owner | by, from_loc, linked, pilot | pair group |
| `linked` | interp.py:1391 | unit / unit owner | by, pilot | pair group |
| `command_played` (PlayCard) | interp.py:1427 | uid / f.controller | ex_used(=0) | cur |
| `command_played` (ActivateMain) | interp.py:1484 | uid / f.controller | as_command(=1), ex_used(=0) | cur |
| `cost_paid` (PayCost) | interp.py:1567 | f.host / p | amount | cur |
| `command_resolved` | interp.py:143 (finish_frame) | f.card_uid / f.controller | — | cur |
| `turn_start` | game.py:241 (after next_group) | -1 / active | — | cur |
| `turn_end` | game.py:302 (after next_group) | -1 / active | — | cur |
| `command_played` (hand) | game.py:947 | uid / p | ex_used | cur |
| `cost_paid` (activation) | game.py:896 | host / p | amount | cur |
| `attacks` | battle.py:31 (after next_group) | attacker / owner | defender, target | cur |
| `blocks` | battle.py:88 | blocker / owner | attacker | block group |
| `blocked` | battle.py:89 | attacker / owner | blocker | same |
| `destroys_shield_card` (battle) | battle.py:121 | src / owner | battle(=1), target (+lki) | group |
| `destroys_by_battle` (battle) | battle.py:136 | src / owner | battle(=1), breach, target (+lki) | group |
| `battle_end` | battle.py:258 (after next_group) | attacker / owner | target | cur |
| `deployed` (EX Base) | effects/builtins.py:42 | uid / p | by | cur |
| `support_used` | effects/builtins.py:71 | f.host / f.controller | target | cur |
| `cost_paid` | effects/bindings/wp_gd02_b.py:68 | f.host / f.controller | amount | cur |
| `ap_reduced` | effects/bindings/wp_gd04_a.py:106 | uid / owner | by | group |
| `command_resolved` | effects/bindings/wp_gd05_b.py:102 | uid / f.controller | — | cur |

`left_battle_area` and `burst_activated` exist in `Ev` (dsl.py:692-727) but are never emitted.

---------------------------------------------------------------------------------------------

## 6. Trigger collection

### 6.1 `_trigger_ok(st, dv, trig, host, host_owner, event)` (core.py:194-233), checks in order

1. `subject = payload("subject")` (`_payload` returns first matching key, else -1; core.py:187).
2. `whose_turn` not None: `want = host_owner if whose_turn is P.YOU else 1 - host_owner` (every
   other P value, including ACTIVE/STANDBY, means "opponent"); fail if `st.active != want`.
3. `self_only` (default True): fail if `subject != host`.
   elif `trig.subject` (a `Sel`) not None: fail unless `_subject_matches(...)` (core.py:163-184):
   subject >= 0; if `not include_self` subject != host; side FRIENDLY → subject owner ==
   host_owner, ENEMY → !=, ANY → no check; then if `sel.filters`:
   `V.matches(st, dv, Ctx(host_owner, host, event=event), subject, sel.filters)`.
   **`sel.loc` and `sel.top_n` are ignored.**
   elif `not include_self and subject == host`: fail.
4. `pilot_filters`: `pilot = payload("pilot")`; fail if `pilot < 0` or not
   `matches(Ctx(host_owner, host), pilot, filters)` (no event in ctx).
5. `target_filters`: same with `payload("target")`.
6. `battle_only` not None: fail if `bool(payload("battle") == 1) != battle_only`.
7. `by_enemy` not None: `by = payload("by")`; fail if `(by >= 0 and by != host_owner) != by_enemy`.

### 6.2 `queue_trigger(st, a, host, controller, event, card_uid=-1, host_seq=-1)` (core.py:248-284)

```
if host_seq == -1: host_seq = cards[host].zone_seq
group = payload(event, "group")
for t in st.pending_triggers:                    # NOT st.batches
    if t.ability_key[1] == a.aid and t.host == host and payload(t.event,"group") == group and group >= 0:
        return                                   # rule 10-1-6-3: once per simultaneous group
once = (a.aid, host, host_seq) if a.ability.once_per_turn else ()
if once and once in st.once_used: return
append TriggerInst(program_id=a.program_id, controller=controller, host=host, host_seq=host_seq,
                   card_uid=card_uid if card_uid != -1 else _card_uid_for(st, a, host),
                   ability_key=(a.def_id, a.aid, int(a.unit_text)), event=event, once_key=once)
```

`emit` never passes `card_uid`, so it is always `_card_uid_for` (core.py:240-245): if
`a.unit_text` and the host's `pair >= 0` and `cards[pair].def_id == a.def_id` → the Pilot uid,
else host. For LKI hosts the pair has already been cleared by `move`, so a destroyed Unit's
Pilot-granted trigger gets `card_uid = host` (the Unit). `controller` is always the host's
owner. The dedup compares only `ability_key[1]` (aid) and host, not the whole key.

### 6.3 Last-known information (LKI)

`ability_snapshot(st, uids)` (core.py:494-496) = `{u: Snapshot(tuple(dv.abilities.get(u, ())),
cards[u].zone_seq) for u in uids}` (dict in `uids` order; derived computed fresh). Callers:
`destroy` (all targets, taken before any move), `return_to_hand` (BATTLE/BASE targets),
`rules_management` (sources of effect damage, `sorted(set(...))`), battle.py:111.
In `emit`, LKI hosts' abilities come from the snapshot (filtered by `trigger_event is ev`), they
are excluded from the live trigger index, and their once key uses `snap.seq` (the pre-move
zone_seq).

### 6.4 Delayed triggers — `_delayed_triggers` (core.py:359-403)

Created by `_h_delayed` (section 13). For each `le in st.delayed` in list order:
1. `spec = R.continuous[le.effect_key]` (an `AbilityGrant(card_number, index)`);
   `entry = db.get(card_number)`, skip if None.
2. `dt = _find_delayed(R.cards[def].own + R.cards[def].unit, spec.ability_index)`
   (core.py:411-421: walks each ability's `steps` attribute — `getattr(ab, "steps", ())`, so
   Burst uses its raw steps, Activated its steps without costs — with `_walk_delayed`
   (core.py:424-438): pre-order, a DelayedTrigger is counted then its own `steps` are walked;
   If → then, otherwise; May/IfYouDo/Simultaneous/ForEach/Repeat → steps; ChooseMode → each
   option body; index counts across all abilities). Skip if None or `dt.trigger.event is not ev`.
3. `host = le.source_uid`; `bound = [u for (u, seq) in le.targets if seq_of(u) == seq]` where
   `seq_of` uses the LKI snapshot seq if `u in lki`, else the current zone_seq (core.py:406-408).
4. If `dt.trigger.self_only and le.targets`: `ok = subject in bound and
   _trigger_ok(st, dv, dt.trigger, subject, cards[subject].owner, event)`; else
   `ok = _trigger_ok(st, dv, dt.trigger, host, le.controller, event)`.
5. If ok: `pid = R.program(dt.steps, f"{card_number}#delayed")` (already registered at registry
   build, registry.py:262, so this returns the existing id) and append
   `TriggerInst(pid, le.controller, host, cards[host].zone_seq, host, (-1, -1, 0), event)`
   (no dedup, no once key, no cond). The Lasting is NOT consumed; it fires every time until
   `_expire` (game.py:208-220) drops it.

### 6.5 Keyword triggers — `_keyword_triggers` (core.py:441-482)

- `TURN_END`: for uid in `zones[st.active][BATTLE]` (list order): if `kw_amount(dv, uid, REPAIR) > 0`
  and `damage > 0` → append `TriggerInst(R.repair_program, st.active, uid, zone_seq, uid,
  (-2, 0, 0), event)`. Program 0 = `(Recover(This, KwAmount(REPAIR)),)` (registry.py:58).
- `DESTROYS_BY_BATTLE` with subject >= 0: `owner = subject.owner`, `amount = payload("breach")`,
  `victim = payload("target")`; return if `amount <= 0 or owner != st.active or victim < 0`;
  return if victim's current def is not a Unit type; return if the victim owner (`1 - owner`)
  has no BASE and no SHIELD cards; else append `TriggerInst(R.breach_program, owner, subject,
  subject.zone_seq, subject, (-3, 0, 0), event)`. Program 1 =
  `(DamageShieldArea(OPP, EventAmount("breach")),)`. Events emitted by rules management have no
  `breach` key (payload -1) so they never trigger Breach.

### 6.6 Burst triggers — inside `destroy_shields` (core.py:700-749)

```
if not uids: return                      # no next_group
group = next_group()
for uid in uids:                         # in the order given
    c.known = 3
    if R.cards[c.def_id].has_burst:
        move(uid, RESOLVING, reveal=True)
        pending_triggers.append(TriggerInst(program_id=R.abilities[burst_aid].program_id,
            controller=player, host=uid, host_seq=c.zone_seq (after move), card_uid=uid,
            ability_key=(c.def_id, burst_aid, 0),
            event=(("subject",uid),("player",player),("group",group)), burst=True))
    else:
        move(uid, TRASH, reveal=True)
    record("shield_destroyed", player, by, uid)
for uid in uids:
    emit SHIELD_DESTROYED(uid, player=player, battle=int(battle), source, by, group=group)
    if source >= 0: emit DESTROYS_SHIELD_CARD(source, player=source.owner, target=uid, battle=int(battle), group=group)
```

So all Burst TriggerInsts precede the triggers of that destruction's events in
`pending_triggers`. A Burst program is `(May(steps, prompt="activate Burst?"),)`
(registry.py:182-183).

### 6.7 TriggerInst creation order (summary)

Within one `emit`: (1) live trigger-index candidates, (2) LKI hosts sorted by
`(owner != active, uid)` with snapshot ability order, (3) delayed triggers in `st.delayed` order,
(4) Repair/Breach. Across a multi-card operation the order is the order of the `emit` calls
listed per instruction below (usually: all moves first, then one emit per card in target order;
`draw` is the exception — it emits after each card).

---------------------------------------------------------------------------------------------

## 7. Batches and resolution order (game.py:667-727, 734-760, 813-815)

- `advance` loop priority (game.py:736-760): (1) `max_actions` cap; (2) if `st.frames`:
  `run_top_frame`; (3) elif `st.pending_triggers`: `_form_batch` — the WHOLE list becomes a new
  batch pushed on `st.batches` (a stack) and `pending_triggers = []`; (4) elif `st.batches`:
  `_start_next_trigger`; (5) else setup step, or `rules_management` then (if no new
  frames/triggers) the next turn step. Hence triggers queued while a frame or trigger resolves
  form a NEW batch that is fully resolved before the rest of the older batch (stack order).
- `_trigger_candidates(st, batch)` (game.py:673-681): indices of `burst` instances if any; else
  indices whose `controller == st.active`; else all indices.
- `_start_next_trigger` (game.py:684-709): empty batch → pop it. If more than one candidate and
  all candidates equal the first in `(program_id, ability_key, burst, host)`, keep only the first.
  One candidate → `_resolve_trigger(batch.pop(index))`. Otherwise
  `Decision(player=batch[cands[0]].controller, ORDER_TRIGGER, options=(Action(ORDER, i) for i in cands), prompt="order triggers", context=())`.
- Answer (game.py:813-815): `_resolve_trigger(st, st.batches[-1].pop(action.a))` — `a` is the
  index into the batch at the time of the decision; the choice is re-offered after each
  resolution.
- `_resolve_trigger` (game.py:712-727): if `once_key` and in `once_used` → dropped silently; kind
  = "burst" if burst else "trigger"; burst → `record("burst_revealed", controller, controller,
  card_uid)`; `push_frame(program_id, controller, host, card_uid, kind, event)`;
  `f.once_key = t.once_key`.

---------------------------------------------------------------------------------------------

## 8. Damage (core.py:503-767)

`damage_card(uid, amount, source, battle, by)` = `apply_damage(*resolve_damage(...))`
(core.py:570-573). Battle damage in battle.py calls `resolve_damage` for both directions before
applying either (8-5-3-2).

`resolve_damage(st, uid, amount, *, source, battle, by) -> (dest, final)` (core.py:576-599):
```
if amount <= 0: return uid, 0
if cards[uid].zone not in (BATTLE, BASE): return uid, 0
dv = derived
if battle:
    for r in rules_of(dv, uid, REDIRECT_BATTLE_DAMAGE):      # first usable rule wins
        dest = r.aux
        if dest >= 0 and dest != uid and cards[dest].zone is BATTLE:
            return resolve_damage(dest, amount, ...)          # recursion
if _prevented(uid, amount, thresholds=False): return uid, 0
amount = _reduce(uid, amount)
if zone is BASE: amount -= shield_area_reduction(owner, source, battle, by)
if amount <= 0 or _prevented(uid, amount, thresholds=True): return uid, 0
return uid, amount
```

`_prevented(st, dv, uid, amount, source, battle, by, *, thresholds=False)` (core.py:503-529):
for `r` in `rules_of(dv, uid, CANT_RECEIVE_DAMAGE)` (list order: constants in `dv.abilities`
order, then lasting effects in `st.lasting` order — view.py:425-455): `limit = r.rule.amount if
isinstance(amount, int) else 0`; `limited = limit > 0`; skip if `limited != thresholds` or
(`limited and amount > limit`); if `_damage_rule_applies`: if `once_per_turn`: key =
`(-8, *r.key, uid, cards[uid].zone_seq)`; skip rule if key in once_used, else ADD it; return True.
Return False. **Side effect: consumes the once key even when called from a "would it?" query**
(`shields_receiving_damage`).

`_damage_rule_applies(st, dv, r, uid, source, battle, by)` (core.py:532-550): damage_kind BATTLE
needs battle; EFFECT needs not battle; `owner = cards[uid].owner`; source_side ENEMY fails if
`by < 0 or by == owner`; FRIENDLY fails if `by != owner`; `source_filters` fail if `source < 0`
or not `matches(Ctx(owner, uid), source, filters)`.

`_reduce` (core.py:553-567): for r in `rules_of(REDUCE_DAMAGE)`: `if amount <= 0: break`; skip if
not applies; `ikey = (-7, *r.key, uid, zone_seq)`; if once_per_turn: skip if used else add;
`amount -= value(st, dv, Ctx(r.controller, r.source), r.rule.amount)`. Return `max(0, amount)`.

`r.key` (view.py:436, 451): constant → `(aid, host)`; lasting → `(-1, effect_key, serial)`.

`apply_damage(st, uid, amount, *, source, battle, by)` (core.py:602-633): return 0 if
`amount <= 0` or zone not BATTLE/BASE; `damage += amount`; if `not battle and source >= 0`:
`effect_hits[uid] = source`; touch; emit DAMAGED; if source >= 0 emit DEALS_DAMAGE; return amount.

Shield-area player rules — `_shield_area_rules(st, player, kind, source, battle, by)`
(core.py:636-664): `[]` if no lasting; for `le in st.lasting` with `le.player == player` whose
continuous is a `RuleGrant` of `kind`: damage_kind checks as above; source_side ENEMY skip if
`by < 0 or by == player` (no FRIENDLY check here); source_filters skip if `source < 0` or not
`matches(Ctx(player, source), source, filters)`; collect the rule.
`shield_area_protected` = any SHIELD_AREA_PROTECTION rule; `shield_area_reduction` =
`sum(r.amount for r in rules if isinstance(r.amount, int))` (ints only; non-int amounts ignored).

`shields_receiving_damage(st, uids, amount, ...)` (core.py:681-697): `[]` if no uids;
`left = amount - shield_area_reduction(owner of uids[0], ...)`; `[]` if `left <= 0`; else
`[u for u in uids if not _prevented(u, amount) and not _prevented(u, left, thresholds=True)]`
(short-circuit: the threshold check runs only if the first returned False).

`damage_shield_area(st, player, amount, *, cards, source, battle, by)` (core.py:752-767):
return if `amount <= 0`; return if protected; if the player has a Base →
`damage_card(base[0], amount, ...)` and return; else
`destroy_shields(player, shields_receiving_damage(zones[player][SHIELD][:cards], amount, ...))`.

---------------------------------------------------------------------------------------------

## 9. Destruction and rules management

### 9.1 `destroy(st, uids, *, battle, by, source=-1) -> list` (core.py:774-827)

```
dv = derived; targets = []
for u in uids:                                   # duplicates are NOT removed
    if zone not in (BATTLE, BASE, PAIRED): continue
    if any(_destroy_rule_applies(r) for r in rules_of(dv, u, CANT_BE_DESTROYED)): continue
    targets.append(u)
if not targets: return []                        # no next_group
lki = ability_snapshot(targets)
info[u] = {owner, pilot: pair if zone is not PAIRED else -1, linked: int(u in dv.linked),
           ap: (unused), was_zone: (unused)}
group = next_group()
for u in targets: move(u, TRASH); record("destroyed", owner, by, u, source)
for u in targets: emit DESTROYED(u, player=owner, lki={all targets' snapshots}, battle=int(battle),
                                 by, source, pilot, linked, group=group)
for u in targets: if token and zone is TRASH: remove_from_game(u)     # dead (tokens are OUTSIDE)
return targets
```
`_destroy_rule_applies` (core.py:830-839): kind BATTLE needs battle, EFFECT needs not battle,
source_side ENEMY fails if `by < 0 or by == owner`. A destroyed Unit's Pilot goes to the trash
with it (moved first) but gets no DESTROYED event unless it is itself a target.

### 9.2 `set_winner(st, losers, reason)` (core.py:842-854)

No-op if a winner exists. Two losers → `winner = -1`, `end_reason = BOTH_DEFEATED`; else
`winner = 1 - loser`, `end_reason = reason`. Then `phase = GAME_OVER(6)`, `step = GAME_OVER(99)`,
`pending = None`. Frames, batches and pending triggers are NOT cleared (they remain in the final
state digest).

### 9.3 `rules_management(st)` (core.py:857-892)

```
if winner is not None or phase is SETUP or st._rules_checked == st.version: return
loop:
    losers = {p for p in (0,1) if not zones[p][DECK]}
    if losers: set_winner(losers, DECK_OUT); return
    if not R.hp_reduction_possible:              # currently always taken (value False)
        damaged, lethal = _damage_scan(st)
        if not damaged: _rules_checked = version; return          # effect_hits NOT cleared
        if not lethal: effect_hits.clear(); _rules_checked = version; return
    dv = derived
    doomed = [uid for p in (active, 1-active) for z in (BATTLE, BASE) for uid in zones[p][z]
              if cards[uid].damage >= hp_of(dv, uid)]
    hits = {u: effect_hits[u] for u in doomed if u in effect_hits}
    effect_hits.clear()
    if not doomed: return                        # _rules_checked NOT set
    sources_lki = ability_snapshot(sorted(set(hits.values())))
    destroyed = destroy(doomed, battle=False, by=-1)              # source -1
    if not destroyed: return
    _emit_destroys_with_effect_damage(destroyed, hits, sources_lki)
```
`_damage_scan` (core.py:917-937): over p in (0, 1), zones BATTLE then BASE: `hp` = printed def
HP; `damaged |= damage > 0 or hp <= 0`; for a paired BATTLE Unit add the Pilot's printed HP;
`lethal |= damage >= hp`; returns early when both are True.
`hp_reduction_possible` (registry.py:367-369) is True iff some script contains a `StatMod` whose
`hp` is not a non-negative int.

`_emit_destroys_with_effect_damage(st, destroyed, hits, lki)` (core.py:895-914): `group =
next_group()` (always, even if nothing is emitted); for u in destroyed: `src = hits.get(u, -1)`;
skip if `src < 0`, same owner as u, or src's CURRENT def type is not a Unit; `src_lki = {src:
lki[src]}` if src is not in BATTLE now, else None; `ev = DESTROYS_SHIELD_CARD` if u's def is a
Base type else `DESTROYS_BY_BATTLE`; `emit(ev, src, player=src.owner, lki=src_lki, target=u,
battle=0, group=group)`. No history record.

Call sites of `rules_management`: after every executed/resumed instruction unless the frame
holds rules (interp.py:125-126, 174-175) and in `advance` before each turn step (game.py:755).

### 9.4 `effect_hits`

Set only by `apply_damage` for effect damage with `source >= 0` (last write wins). Cleared in
`rules_management` (the "damaged but not lethal" fast path, and the full path after computing
`hits`). NOT cleared on the "nothing damaged" path, so stale entries can survive (e.g. a card
that was healed or left play). Serialized sorted by uid (state.py:607). Rust must reproduce the
`_rules_checked` short-circuit too, because skipping a check also skips `effect_hits.clear()`.

---------------------------------------------------------------------------------------------

## 10. Once-per-turn bookkeeping (`st.once_used`, reset to `set()` at each turn change, game.py:335)

| Key | Added | Checked |
| --- | --- | --- |
| `(aid, host, host_seq)` triggered `once_per_turn` | `finish_frame` when `f.once_key and f.acted` (interp.py:136-137) | `queue_trigger` (core.py:270-272), `_resolve_trigger` (game.py:713) |
| `(aid, host, zone_seq)` activated `once_per_turn` | at activation, before the frame runs (game.py:891-892) | `_activation_ok` (game.py:439) |
| `(-8, *rule_key, uid, zone_seq)` | `_prevented` when a once rule prevents | `_prevented` |
| `(-7, *rule_key, uid, zone_seq)` | `_reduce` | `_reduce` |
| `(-9, substitute_uid, turn)` | `_r_rest` when a substitute with any once rule is chosen (interp.py:673-674) | `_rest_substitutes` (interp.py:648) |
| binding-specific keys | effects/bindings/wp_gd05_a.py:106-118 | same |

A triggered once-per-turn ability whose frame never "acted" (e.g. every optional part declined)
is not consumed and may trigger again. Serialization sorts keys as lists (state.py:598).

---------------------------------------------------------------------------------------------

## 11. History events (`record`, core.py:142-151)

`record(st, kind, player, by=-1, uid=-1, source=-1)` appends `HistoryEvent(kind, player, by, uid,
def_id, source)` where `def_id = cards[uid].def_id` read AT RECORD TIME (after any move, so a
Unit variant has already reverted) or -1. No touch, but `len(history)` is part of the derived
view key (view.py:229). Cleared at each turn change (game.py:336) and at setup end
(game.py:190). Kinds recorded in this area:

| kind | site | player, by, uid, source |
| --- | --- | --- |
| `shield_destroyed` | core.py:728 | shield owner, by, shield |
| `destroyed` | core.py:808 | owner, by, uid, source |
| `draw` | interp.py:510 | p, p, uid |
| `discard` | interp.py:571 | p, by, uid |
| `resource_set_active` | interp.py:729 (SetActive on a resource) | owner, f.controller, uid, f.host |
| `resource_set_active` | interp.py:1076 (SetResourcesActive) | p, f.controller, uid, f.host |
| `excess` | interp.py:908 | owner, owner, uid |
| `deployed` | interp.py:964 / 1006 | owner, by, uid / p, f.controller, token |
| `ex_resource` | interp.py:1040 | p, by, new uid |
| `shield_to_hand` | interp.py:1110 | p, f.controller, uid |
| `paired` | interp.py:1375 | unit owner, by, PILOT uid |
| `command_activated` | interp.py:1426, 1483 | f.controller, f.controller, uid |
| `ex_exiled` | interp.py:1467 | p, p, uid (after remove_from_game) |
| `cost_paid` | interp.py:1566 | p, p, f.host |

Elsewhere: `burst_revealed` (game.py:717), `activate`, `cost_paid`, `play`, `played_with_ex`,
`command_activated` (game.py:882-946), `attack`, `block`, `destroys_by_battle`,
`destroys_by_battle_paired`, `player_damage` (battle.py), `deployed` (builtins.py:41),
`cost_paid` (wp_gd02_b.py:67).

---------------------------------------------------------------------------------------------

## 12. Interpreter execution model (interp.py:57-175)

### 12.1 Programs

A program is `Program(instrs: tuple, label: str)` built by `flatten` (effects/program.py:91-181).
Layout rules (verified by flattening a sample):
- `If(cond, then, otherwise)`: `JumpIfNot(cond, else_start)`, then-body, and if `otherwise`
  non-empty `Jump(end)` + else-body. Without otherwise the JumpIfNot targets the end of then.
- `May(steps, prompt, player)`: `AskMay(prompt, player, target_if_no=end)`, body.
- `Simultaneous(steps)`: `HoldRules(1)`, body, `HoldRules(-1)`.
- `IfYouDo(steps)`: `JumpIfNotDid(end)`, body.
- `ChooseMode(options, chooser)`: `ModeSelect(labels, starts, chooser, end)`, then for each
  option its body followed by `Jump(end)` (also after the last body).
- `ForEach(ref, var, steps)`: `LoopInit(key, ref, None)`, `LoopNext(key, var, end)` (= head),
  body, `Jump(head)`; `Repeat(times, steps)`: `LoopInit(key, None, times)`,
  `LoopNext(key, "", end)`, body, `Jump(head)`. `key = f"__loop{n}"`, n counted per program in
  pre-order (outer before inner).
- Every other Step node is its own instruction. `SetDid` exists but the builder never emits it.

Registry programs are interned by structural equality of the step tuple
(`Registry.program`, registry.py:115-122): equal tuples share one id and the label of the first
registration. Order at build: 0 `keyword:Repair`, 1 `keyword:Breach`, 2 `keyword:Support`, then
cards in DB order (abilities own then unit; each delayed-trigger body right after its ability).
System programs are registered lazily on first use (`system_program`, interp.py:1589-1594) and
get the next free ids (623/624/625 in the current data when first used in order deploy, pair,
hand_limit; process-local).

### 12.2 Frames

`push_frame(st, program_id, *, controller, host, card_uid=-1, kind, event=(), vars=None)`
(interp.py:61-88): `host_seq = cards[host].zone_seq if host >= 0 else 0`;
`card_uid = card_uid if card_uid != -1 else host`; `vars = dict(vars or {})` (copy); then for
each event pair whose key starts with `"it"` and `key[2:].isdigit()`, append its value to
`vars["it"]` in event order (no current event has such keys). Appended to `st.frames`.

Frame kinds: `"trigger"`, `"burst"` (game.py:715), `"activated"` (game.py:883, 903),
`"command"` (game.py:949, interp.py:1433), `"system"` (game.py:309, 930, 940, 954),
`"play_sub"` (interp.py:1415), `"activate_main_sub"` (interp.py:1490).

`ctx_of(f) = Ctx(f.controller, f.host, f.card_uid, f.vars, f.event)` (interp.py:57-58). The
Ctx holds the SAME dict object as `f.vars` (view.py:43), so a custom hook writing `ctx.vars`
writes the frame.

### 12.3 `run_top_frame(st)` (interp.py:109-132)

```
f = st.frames[-1]; prog = R.programs[f.program_id]
while f.pc < len(prog.instrs):
    ins = prog.instrs[f.pc]
    status = execute(st, f, ins)                 # handler by exact type; EngineError if none
    if status is PUSHED or (status is NEXT and f.did and type(ins) not in NON_ACTIONS):
        f.acted = True
    if status is NEXT: f.pc += 1
    if over(st): return
    if "__hold_rules" not in f.ints: rules_management(st)       # runs even after WAIT
    if over(st): return
    if status in (WAIT, PUSHED): return
st.frames.pop(); finish_frame(st, f)
```
`Status`: NEXT=0, JUMPED=1, WAIT=2, PUSHED=3 (internal). JUMPED instructions set `pc`
themselves and continue the loop. `NON_ACTIONS` (interp.py:41-54): Jump, JumpIfNot, AskMay,
JumpIfNotDid, ModeSelect, LoopInit, LoopNext, SetDid, HoldRules, Choose, ChooseMode, BindVar.
An empty program is popped and finished immediately. `HOLD_RULES` only affects the frame that
holds it; a sub-frame pushed inside a `Simultaneous` block runs rules management normally.

### 12.4 `resume(st, action)` (interp.py:160-175)

Called by `game.apply` whenever `st.frames[-1].waiting` (game.py:790-791), after
`st.pending = None; action_count += 1`. `f.waiting = False`; `ins = instrs[f.pc]`; resumer by
type (EngineError if none); same `acted` rule; NEXT → `pc += 1`; rules_management unless held;
no `over` checks. `game.apply` then calls `advance`, which re-enters `run_top_frame`.
Resumers (interp.py:1648-1658): AskMay, ModeSelect, Choose, Discard, DeployCard (excess),
DeployToken (excess), Arrange, Rest, ToDeck.

### 12.5 `finish_frame(st, f)` (interp.py:135-150)

1. `if f.once_key and f.acted: once_used.add(f.once_key)`.
2. kind in ("command", "burst"): if `cards[f.card_uid].zone is RESOLVING`: `move(card_uid,
   TRASH)` and, for "command" only, emit `COMMAND_RESOLVED(card_uid, player=f.controller)`.
   (A Burst/Command that moved itself elsewhere is left there; no event.)
3. elif kind == "activate_main_sub" and frames remain: parent = `frames[-1]`; `parent.pc += 1`;
   `parent.did = f.did`.
4. elif kind == "play_sub" and frames remain: `parent.pc += 1` (parent.did stays True).

### 12.6 Frame field semantics

- `pc`: index of the instruction to execute/resume.
- `did`: result flag of the last instruction that set it; starts True. Read by `JumpIfNotDid`
  (IfYouDo), by `DidLast` conditions (`_h_jump_if_not` passes `f.did`, interp.py:188) and by the
  `acted` rule. Choose/Discard/AskMay(NO)/ModeSelect(no mode) set it; AskMay YES leaves it
  unchanged; BindVar/Loop*/HoldRules/Jump* never change it.
- `answer`: set True/False by the AskMay resumer (interp.py:221, 223); never read (serialized).
- `waiting`: True while the frame's decision is pending (`_decide`).
- `buf`: scratch list for multi-step choices (Choose picks, Discard picks, ToDeck/Arrange
  orderings); reset when the instruction finishes.
- `ints`: `"__hold_rules"` depth; loop counters `"__loopN"`; Rest substitution state
  `"__rest_i"`, `"__rest_as{i}"`; `"ex_used"` (set by game.py:933 on the deploy system frame).
- `vars`: bound card tuples: Choose vars, `__costN`, `__loopN` item lists, ForEach vars,
  `"__todeck"` (ToDeck ordering in progress), `"__play"` (PlayCard; write-only),
  `"card"`/`"pilot"`/`"unit"` for system programs, `"discarded"`, `"tokens"`, `"milled"`,
  `"looked"` defaults.
- `acted`: see 12.3; drives the once-per-turn consumption and AI heuristics (ai/actions.py:178).

### 12.7 Decisions — `_decide(st, f, player, kind, options, prompt, context=())` (interp.py:91-102)

Sets `f.waiting = True`, `st.pending = Decision(player, kind, tuple(options), prompt, context)`,
returns WAIT. Complete list raised from effects:

| Instruction | player | kind | options (order) | prompt | context |
| --- | --- | --- | --- | --- | --- |
| AskMay | `player_of(ins.player)` | BURST if prompt == "activate Burst?" else YES_NO | YES, NO | `ins.prompt or "you may"` | () |
| ModeSelect | `player_of(chooser)` | SELECT | `SELECT(i)` for each available mode i ascending | "choose mode" | (("mode",1),) |
| Choose | `player_of(ins.chooser)` | SELECT | see 13.2 + DONE last | f"choose {ins.var}" | (("min",mn),("max",n)) |
| Discard (first) | chooser (= p if ins.chooser is None) | DISCARD | `SELECT(u)` for candidates in hand order | "discard" | (("count",n),) |
| Discard (next) | same | DISCARD | candidates with uid > max(picked), or all candidates if none | "discard" | (("count", n - picked),) |
| Rest substitution | owner of the target | SELECT | `SELECT(u)` for substitutes, then DONE | "rest instead?" | (("rest_sub", target),) |
| ToDeck ordering | the owner | ARRANGE | `SELECT(u)` for that owner's remaining cards | "order (top first)" | (("owner", owner),) |
| Excess (Unit or Base) | the player | EXCESS | `SELECT(u)` for zone cards not protected | "battle area excess" (also for bases) | (("zone", 3 or 5),) |
| Arrange top_or_bottom (>1 card) | `player_of(ins.player)` | ARRANGE | SELECT(0), SELECT(1) | "top(0) or bottom(1)" | (("all",1),) |
| Arrange per card | same | ARRANGE | SELECT(0, uid), SELECT(1, uid) (b = card) | "top(0) or bottom(1)" | (("card", uid),) |
| Arrange any order | same | ARRANGE | `SELECT(u)` for remaining | "next card (top first)" | (("order",1),) |

Other decisions (game.py): ORDER_TRIGGER "order triggers"; CHOOSE_FIRST "choose Player One";
REDRAW "redraw?"; MAIN "main phase"; ACTION_STEP "action step"; BLOCK "block?" (strings
observed in a live run). `DecisionKind.PAYMENT` is never raised. AI code reads context keys
`order`, `card`, `mode`, `rest_sub` (ai/policy.py:335-353, ai/base.py:130-131,
runner/replayfile.py:40).

---------------------------------------------------------------------------------------------

## 13. Instruction semantics (every handler in `_HANDLERS`, interp.py:1597-1646)

Shorthand: `refs(x)` = `V.resolve_ref(st, ctx_of(f), x)` (computes derived only when the ref
selects cards); `val(x)` = `V.value(st, derived, ctx_of(f), x)`; `P(x)` =
`V.player_of(st, ctx_of(f), x)` (YOU→controller, OPP→1-controller, ACTIVE→st.active,
STANDBY→1-st.active; view.py:585-592). "→ NEXT" means the handler returns NEXT.

### 13.1 Control flow

- `Jump(target)`: `pc = target`, JUMPED (interp.py:182-184).
- `JumpIfNot(cond, target)`: if `cond(st, derived, ctx, cond, did=f.did)` → NEXT, else
  `pc = target`, JUMPED (187-191).
- `JumpIfNotDid(target)`: did → NEXT else jump (194-198).
- `SetDid(value)`: `did = value` → NEXT (201-203).
- `HoldRules(delta)`: `ints["__hold_rules"] += delta`, deleted when 0 → NEXT (206-210).
- `AskMay`: decision (12.7). Resume YES: `answer = True` → NEXT (body follows). NO:
  `answer = False; did = False; pc = target_if_no` → JUMPED (219-226).
- `ModeSelect(labels, targets, chooser, end)` (263-279): available modes =
  `[i for i in range(len(labels)) if targets_available(instrs, targets[i], st, ctx_of(f))]`; if
  none → `did = False; pc = end` JUMPED; else ALWAYS a decision (even with one option).
  Resume: `pc = targets[action.a]` JUMPED.
- `LoopInit(key, ref, times)` (282-290): with ref: `vars[key] = tuple(V.resolve(st, derived,
  ctx, ref))`, `ints[key] = 0`; else `ints[key] = -max(0, val(times))` → NEXT.
- `LoopNext(key, var, end)` (293-307): `i = ints.get(key, 0)`; if `var` non-empty (ForEach):
  `i >= len(vars.get(key, ()))` → `pc = end` JUMPED, else `vars[var] = (items[i],)`,
  `ints[key] = i + 1` → NEXT. If `var == ""` (Repeat): `i >= 0` → jump to end, else
  `ints[key] = i + 1` → NEXT. Loop vars/ints are never cleaned up.
- `BindVar(var, ref)`: `vars[var] = refs(ref)` → NEXT (1521-1523).

### 13.2 Choose (interp.py:314-470)

`_h_choose`:
1. `group = choice_group(instrs, pc)` (391-404): this Choose plus each immediately following
   Choose whose `distinct_from` intersects the set of vars accumulated so far (stops at the first
   non-matching instruction).
2. If `any(choice_short(st, ctx_of(f), g) for g in group)`: for every g in group
   `vars[g.var] = ()`; `did = False`; `buf = []`; `pc += len(group)` → JUMPED.
3. `cands = _choose_candidates` (314-323): `V.select(st, dv, ctx, ins.sel)` (order: players
   `(controller, 1-controller)` for ANY / the one side, then location list order with filters;
   view.py:577-632); minus uids bound in any `distinct_from` var; if `ins.targeting`, minus cards
   with `_cant_be_chosen(st, dv, u, f.controller)` (326-332: any CANT_BE_CHOSEN rule makes it
   unchoosable, except a rule with `source_side ENEMY` when `f.controller == owner`).
4. `n, mn = choose_bounds` (335-342): `n = val(count)`; `mn = n if min_count is None else
   val(min_count)`; `optional` → `mn = 0`; return `(max(0, n), max(0, min(mn, n)))`.
5. `buf = []`; if `n == 0 or not cands` → finish (empty).
6. If `ins.random`: `pool = list(cands)`; repeat `min(n, len(pool))` times:
   `chosen.append(pool.pop(st.rng.randrange(len(pool))))`; `buf = chosen` → finish.
   (RNG is consumed even when every candidate will be taken, and even for `randrange(1)`.)
7. If `len(cands) <= mn or (len(cands) <= n and mn == n)`: `buf = cands` → finish.
8. Else decision with `_choose_options` (345-351): `last = cands.index(buf[-1]) if buf else -1`;
   options `SELECT(u)` for candidates at index > last and not picked, in candidate order; then
   `DONE` if `len(buf) >= mn`.

`_finish_choose` (444-448): `vars[var] = tuple(buf)`; `did = bool(buf)`; `buf = []` → NEXT.

`_r_choose` (451-470): DONE → finish. Else `buf.append(action.a)`; recompute `cands`, `n, mn`;
`remaining = [u for u in cands if u not in buf and cands.index(u) > cands.index(buf[-1])]`; if
`len(buf) >= n or not remaining` → finish; else re-decide with the same prompt and
`(("min",mn),("max",n))` → WAIT.

### 13.3 Target legality (rules 10-1-8-1-1, 10-1-8-1-2, 10-2-2)

- `PUBLIC_TARGET_LOCS` (354-363) = {BATTLE, BASE, RESOURCE_AREA, TRASH, PAIRED,
  FIELD_UNITS_AND_BASES}. `is_target_choice(ins) = ins.targeting and ins.sel.loc in
  PUBLIC_TARGET_LOCS` (366-368).
- `choice_short(st, ctx, ins, exclude=frozenset())` (371-388): False unless a non-optional
  target choice; `n = value(count)`; `need = n if min_count is None else value(min_count)`;
  False if `need <= 0`; True iff fewer than `need` cards of `select(sel)` are outside `exclude`
  and choosable (`_cant_be_chosen` with `ctx.controller`). `distinct_from` is NOT applied.
- `targets_available(instrs, start, st, ctx)` (235-260): `dv = derived`; walks at most
  `len(instrs) + 1` steps from `start`: `pc >= len` → True; Jump → follow; JumpIfNot → follow
  the branch `cond(st, dv, ctx, cond)` takes (did defaults to True); ModeSelect → `any` over its
  targets (recursive); AskMay, JumpIfNotDid or LoopInit → True (stop); Choose with `after_then`
  → True; Choose with `choice_short` → False; anything else → `pc += 1`. Exhaustion → True.
  Callers: `_mode_available` (frame ctx with vars and event) and game.py:389-407/470 (command
  playability and activated abilities, ctx `Ctx(p, uid[, card_uid])`, no event).

### 13.4 Card and zone actions

- `Draw(count, player, each_player)` (489-497): each_player →
  `drawn = draw(active, n, by_effect=True) + draw(1-active, n, by_effect=True)`; else
  `draw(P(player), n, by_effect=True)`; `did = drawn > 0`.
  `draw(st, p, n, *, by_effect=False)` (500-512): `group = next_group()` (always); repeat
  `max(0, n)` times: stop if deck empty; `uid = deck[0]`; `move(uid, HAND)`;
  `record("draw", p, p, uid)`; emit `DRAWN(uid, player=p, group, by_effect=int)` immediately.
  Returns the count.
- `Discard(count, player, chooser, random, filters, var)` (515-573): `p = P(player)`;
  `n = val(count)`; candidates = hand order (filtered by `matches(ctx_of(f))` when filters);
  `buf = []`; `n <= 0 or no candidates` → `vars[var] = ()`, `did = False`. random → pool/pop as
  in Choose → discard. `len(cands) <= n` → discard all (hand order). Else decision (12.7).
  Resume: `buf.append(a)`; candidates minus buf; if `len(buf) >= n` or none left → discard
  `buf` in pick order; else re-decide. `_do_discard`: `discard(p, uids, by=f.controller)`;
  `vars[var] = tuple(uids)`; `buf = []`; `did = bool(uids)`.
  `discard(st, p, uids, by)` (567-573): `group = next_group()`; all moves to TRASH with
  `record("discard", p, by, u)`; then DISCARDED per card.
- `Damage(ref, amount)` (576-593): `targets = refs(ref)`; `amount = val(amount)`;
  `next_group()` (always; the DAMAGED events use it as "cur"); for u: if u is in SHIELD:
  if `shields_receiving_damage([u], amount, source=f.host, battle=False, by=f.controller)` →
  `destroy_shields(owner, [u], ...)` (its own new group), dealt; continue. Else dealt if
  `damage_card(u, amount, source=f.host, battle=False, by=f.controller) > 0`. `did = dealt`.
- `DamageShieldArea(player, amount, cards)` (596-604) / `DamagePlayer(player, amount)` (607-613,
  cards=1): `p = P(player)`; `had = base or shields non-empty`; `damage_shield_area(...)`;
  `did = had and amount > 0` (True even if protected/prevented).
- `Destroy(ref)` (616-623): refs in BATTLE/BASE/PAIRED → `did = bool(destroy(targets,
  battle=False, by=f.controller, source=f.host))`.
- `Rest(ref)` (626-713), with rest substitution (rule 10-1-9):
  `_rest_targets` = refs in BATTLE/BASE/RESOURCE_AREA and not rested (recomputed on every call).
  `_h_rest_continue`: `done = ints.get("__rest_i", 0)`; while `done < len(targets)`: skip
  indices that already have `"__rest_as{done}"`; `chosen = {ints[k] for k starting with
  "__rest_as"}`; `subs = _rest_substitutes(st, f, targets[done], frozenset(targets + chosen))`;
  if subs: `ints["__rest_i"] = done` and decide (12.7); else `done += 1`. Then pop
  `"__rest_i"`; `final = [ints.pop(f"__rest_as{i}", t) for i, t in enumerate(targets)]`;
  `group = next_group()` (always); for u in final still in BATTLE/BASE/RESOURCE_AREA and not
  rested: `rested = True`, touch, `did = True`, emit `RESTED(u, player=owner, by=f.controller,
  group)`. `did` False if nothing was rested.
  `_rest_substitutes` (626-652): `[]` if target owner != f.controller or `f.host < 0`;
  `kind = "base"` if target is in BASE else `"unit"` (also for resources); for z in (BATTLE,
  BASE), u in owner's zone: skip target, `taken`, rested; first REST_SUBSTITUTE rule on u with
  (`rule.name` empty or == kind) and (`source_filters` empty or `matches(Ctx(owner, u),
  f.host, filters)`) and not (`once_per_turn` and `(-9, u, turn)` used) → u qualifies (break).
  Resume (668-676): `i = ints.get("__rest_i", 0)`; SELECT → `ints[f"__rest_as{i}"] = a` and, if
  any REST_SUBSTITUTE rule on `a` is once_per_turn, add `(-9, a, turn)`; DONE stores nothing;
  `ints["__rest_i"] = i + 1`; continue.
- `SetActive(ref)` (716-732): `dv = derived` ONCE before the loop; `group = next_group()`
  (always); for u in refs: in BATTLE/BASE/RESOURCE_AREA and rested and no CANT_BE_SET_ACTIVE rule
  in the pre-loop `dv` → `rested = False`, touch, did, if resource
  `record("resource_set_active", ...)`, emit SET_ACTIVE.
- `ReturnToHand(ref)` (735-778): `dv = derived`; targets = refs in
  BATTLE/BASE/PAIRED/TRASH/RESOLVING and `_returnable` (no CANT_BE_RETURNED rule, except an
  ENEMY-side rule when `f.controller == owner`). `return_to_hand(targets, by)`: False if empty;
  `lki = ability_snapshot([u in BATTLE/BASE])`; `group = next_group()`; for u: skip if already
  OUTSIDE, else `move(u, HAND)`; then for each moved u: emit `RETURNED_TO_HAND(u,
  player=owner, by, group, lki={u: snap} or {})`. `did = bool(moved)`. A returned Unit's Pilot
  goes to hand too (no event unless targeted).
- `ToDeck(ref, bottom=True, shuffle=False)` (781-835): first entry (no `"__todeck"` var):
  `targets = refs not OUTSIDE`; `pilots = [pair of each BATTLE target with a pair]` (captured
  before moving); `move(u, DECK, bottom=ins.bottom)` for each target; `did = bool(targets)`.
  If shuffle: `shuffle_deck(p)` for p in `sorted(owners)` → NEXT. Else
  `moved = [u for u in (*targets, *pilots) if zone is DECK]`; `together` = moved cards whose
  owner has more than one card in `moved`; none → NEXT; else `known &= 1 << owner` for each,
  `vars["__todeck"] = tuple(sorted(together, key=(owner, uid)))`, `buf = []`, then
  `_todeck_next`: owners pending = sorted owners with unplaced cards; for the first: if more than
  one remaining → ARRANGE decision; else `_place_ordered(owner, [buf cards of that owner] +
  remaining, bottom=ins.bottom)` and continue with the next owner. Done → delete
  `"__todeck"`, `buf = []` → NEXT. Resume: `buf.append(a)`; `_todeck_next`.
- `Exile(ref)` (838-846): targets not OUTSIDE; `group = next_group()` (always); move all to
  REMOVAL; then EXILED each. `did = bool(targets)`.
- `ToTrash(ref)` (849-860): targets not OUTSIDE; field = those in BATTLE/BASE → `destroy(field,
  battle=False, by=f.controller, source=f.host)` if any; the rest → `move(u, TRASH)` (no event,
  no record). `did = bool(destroyed or rest)`.
- `AddToHand(ref, reveal)` (863-873): targets not in HAND/OUTSIDE; `group = next_group()`
  (always); `move(u, HAND, reveal=ins.reveal)` all; then ADDED_TO_HAND each.
- `DeployCard(ref, rested, pay_cost, ignore_level, as_unit)` (923-976; `pay_cost` and
  `ignore_level` are ignored): if as_unit, each ref with a Unit variant not in
  BATTLE/BASE/OUTSIDE gets `def_id = variant` and a touch. `cards = _deployable(refs)` (Unit or
  Base type, not in BATTLE/BASE/OUTSIDE); none → did False. Group by owner; for owners ascending:
  `units = #unit types`, `bases = rest`; `ex_u = max(0, |battle| + units - 6)`,
  `ex_b = max(0, |base| + bases - 1)`; if ex_u → EXCESS decision on BATTLE excluding this
  owner's deploying cards; elif ex_b → if exactly one base, `trash_excess` it automatically and
  continue, else EXCESS decision on BASE. Then `deploy_cards(cards, rested, by=f.controller,
  ex_used=ints.get("ex_used", 0))`; `did = True`. Resume (`_r_excess`, 899-901):
  `trash_excess(a)` then re-execute the whole handler.
  `deploy_cards` (956-976): `group = next_group()`; `froms` = zones before; for each: `move(u,
  BATTLE if unit type else BASE, rested=rested)`, `record("deployed", owner, by, u)`;
  `_enter_rested(cards)` (979-986: `dv` computed once; each card not rested with a
  DEPLOYED_RESTED rule → rested, touch); then DEPLOYED each.
- `DeployToken(token_key, count, rested, player, var, also)` (989-1012):
  `p = P(player)`; `batches = [(token_key, val(count)), *also]`; `n = sum(max(0, k))`; `n <= 0`
  → did False. `ex_u = max(0, |battle| + n - 6)`; if `ex_u` and the battle area is non-empty →
  EXCESS decision (nothing protected; resume re-executes). `group = next_group()`; for each
  (key, k): `tdef = db.token_for(token_spec(key))`; create
  `max(0, min(k, 6 - len(made)))` tokens with `new_card(tdef, p, BATTLE, rested)` +
  `record("deployed", p, f.controller, uid)`; `_enter_rested(made)`; DEPLOYED(token=1) each;
  `vars[var] = tuple(made)`; `did = bool(made)`. `token_spec(key)` (1018-1024):
  `name, traits, ap, hp, text = key.split("|", 4)`; traits split on "/" dropping empties.
- `PlaceExResource(player, rested)` (1027-1042): False if area has >= 15 cards or >= 5 EX
  Resources; else `new_card(ex_resource, p, RESOURCE_AREA, rested)`, `record("ex_resource", p,
  by, uid)`, emit EX_RESOURCE_PLACED (cur group).
- `PlaceResource(player, rested)` (1045-1059): False if resource deck empty or area >= 15; else
  top card `move(uid, RESOURCE_AREA, rested)`, emit RESOURCE_PLACED (cur group). No record.
- `SetResourcesActive(count, player)` (1062-1080): area sorted stably by `is EX_RESOURCE`
  (normal first, area order kept); activate up to n rested ones, each with
  `record("resource_set_active", ...)`; one touch if any; NO event.
- `RestResources(count, player=OPP)` (1083-1100): same ordering; rest up to n active ones; one
  touch; no record, no event.
- `ShieldToHand(count, player)` (1103-1114): `shields = zones[p][SHIELD][:max(0, n)]` (top
  first); `group = next_group()` (always); move each to HAND + record; then SHIELD_TO_HAND each.
- `AddToShields(ref, count, player, top=True)` (1117-1127): cards = top `n` of the deck (ref
  None) or refs not OUTSIDE; `move(u, SHIELD, bottom=not top)` in order (with top=True the
  order is reversed: the last card moved ends on top). No event/record.
- `Mill(count, player, var)` (1130-1138): top `n` deck cards to TRASH in order;
  `vars[var] = tuple(cards)`. No event/record.
- `LookTop(count, player, var, reveal)` (1141-1150): `cards = tuple(deck[:max(0, n)])`;
  `known |= 3 if reveal else 1 << f.controller`; `vars[var] = cards`; no touch.
- `Arrange(ref, mode, player)` (1153-1218): `cards = refs in DECK` (none → did False);
  `p = owner of cards[0]`. Modes `top_or_bottom` / `top_or_bottom_each`: if `top_or_bottom` and
  more than one card → "all" decision; resume `_place_ordered(p, cards, bottom=a == 1)` →
  did True. Otherwise per card: if no remaining → `buf = []`, did True; else decision for
  `remaining[0]`; resume `u = b if b != -1 else cards[0]`, `_place_ordered(p, [u], bottom=a ==
  1)`, `buf.append(u)`, rerun. Other modes (`top_any_order`, `bottom_any_order`): when at most
  one card remains, `_place_ordered(p, buf + remaining, bottom=mode == "bottom_any_order")`,
  did True; else "next card" decision, resume appends to `buf` and reruns. No known-bit change.
- `Reveal(ref)` (1221-1225): `known = 3` for refs; did True; no touch.
- `Shuffle(player)` (1228-1231): `shuffle_deck(P(player))`; did True.
- `Recover(ref, amount)` (1234-1246): for u in refs in BATTLE/BASE: if `damage > 0 and amount >
  0`: `healed = min(damage, amount)`, subtract, touch, did, emit RECOVERED(amount=healed).
- `Apply(ref, effect, duration, aux)` (1249-1283): `live = tuple((u, zone_seq) for u in refs if
  zone != OUTSIDE)`; empty → did False; aux given but resolves empty → did False, else
  `aux = first`; `before[u] = ap_of(derived, u)`; `add_lasting(_frozen(effect), f.controller,
  f.host, live, duration, aux=aux)`; `did = True` (even if add_lasting declined a THIS_BATTLE
  effect outside a battle). If the UNFROZEN effect is a StatMod with `ap != 0` (any non-int
  Value counts as non-zero): `dv = derived`, `group = next_group()`, emit AP_REDUCED for each
  live BATTLE card whose AP dropped. `_frozen` (1276-1283): StatMod with a non-int ap or hp →
  `StatMod(val(ap), val(hp))`; KeywordGrant with non-int amount → evaluated; others unchanged.
  `add_lasting` (1286-1320): return if THIS_BATTLE and (no battle or battle ended); append
  `Lasting(R.cont_key(effect), controller, source, targets, duration.value, st.turn,
  battle_id or -1, player, uses, R.filters_key(filters) if filters else -1, aux,
  serial=st.next_lasting_serial)`; `next_lasting_serial += 1`; touch.
- `ApplyPlayer(player, effect, duration, filters, uses)` (1323-1337): `add_lasting(effect`
  (not frozen)`, f.controller, f.host, (), duration, player=P(player), uses, filters)`; did True.
- `Pair(pilot, unit)` (1347-1365): first of each ref; did False if either empty, the pilot's def
  is not pilot-capable (`PILOT` or a `COMMAND` with `pilot_name`), `can_pair` fails, or the pilot
  is OUTSIDE; else `pair(pilot, unit, by=f.controller)`, did True.
- `PlayCard(ref, free, ignore_level, cost_delta)` (1396-1439; `ignore_level` ignored): first
  ref; unless free: `cost = max(0, play_cost(dv, uid) + val(cost_delta))` paid by the CARD
  OWNER via `pay_generic` (fail → did False). Unit/Base: `vars["__play"] = (uid,)`, push
  `system:deploy` (kind "play_sub", host uid, vars `{"card": (uid,)}`), did True → PUSHED (pc
  advanced by finish_frame). Command with a command ability: `move(uid, RESOLVING,
  reveal=True)`, `record("command_activated")`, emit COMMAND_PLAYED(ex_used=0), push its program
  (kind "command"), `f.pc += 1` here, did True → PUSHED. Other types (Pilot) → did False.
- `pay_generic(st, p, cost, ex_used=-1)` (1442-1470): True if `cost <= 0` (no touch). Lists, in
  RESOURCE_AREA order, active normal and active EX Resources; `ex_used < 0` →
  `max(0, cost - len(normal))`; False if `ex_used > len(ex) or cost - ex_used > len(normal) or
  ex_used > cost`. Rest the first `cost - ex_used` normals; for the first `ex_used` EX:
  `rested = True`, `remove_from_game`, `record("ex_exiled")`, emit EXILED(ex_resource=1); touch.
- `ActivateMain(ref=ThisCard)` (1473-1492): first ref; no command ability → did False; else
  `record("command_activated")`, emit COMMAND_PLAYED(as_command=1, ex_used=0), push the command
  program (kind "activate_main_sub", host uid) → PUSHED. The card is not moved.
- `StartBattle(attacker, target)` (1495-1505; `damage_only` field ignored): first in-play of
  each (BATTLE/BASE); missing → did False; else `battle.damage_only_battle(...)`, did True.
- `ChangeAttackTarget(target)` (1508-1518): no battle, ended, or no in-play target → did False;
  else `b.target = t`, `b.target_seq = zone_seq`, touch, did True.
- `DelayedTrigger(trigger, steps, duration, bind_vars)` (1526-1559): owner card =
  `R.cards[cards[f.card_uid].def_id]`; index = position of the first structurally equal
  DelayedTrigger in the same walk as `_find_delayed`; not found → EngineError. THIS_BATTLE
  outside a battle → did False. `bound = [(u, zone_seq) for v in bind_vars for u in vars[v]]`;
  append to `st.delayed` a `Lasting(cont_key(AbilityGrant(card_number, index)), f.controller,
  f.host, bound, duration.value, st.turn, battle_id or -1)` with defaults for the rest (serial
  stays -1, `next_lasting_serial` unchanged, no touch); did True.
- `PayCost(amount, player)` (1562-1568): `did = pay_generic(P(player), amount)`; if did and
  amount > 0 and `f.host >= 0`: `record("cost_paid")` + emit COST_PAID.
- `CustomStep(name, params)` (1571-1575): `did = CUSTOM_STEPS[name](st, f, ctx_of(f),
  dict(params))`; touch; → NEXT. Custom steps never raise decisions. Registered names
  (registration via `effects/bindings/__init__.py:41-51`, modules imported sorted by name):
  `return_looked_bottom`, `deploy_ex_base`, `support_used` (effects/builtins.py) and
  `wp_eb01_a_look_own_top`, `wp_eb01_b_look_top_own`, `wp_gd02_b_pay_deploy_cost`,
  `wp_gd03_a_pay_cost_of`, `wp_gd03_a_ap_minus_count`, `wp_gd04_a_copy_keywords`,
  `wp_gd04_a_ap_down_by_named_trash_units`, `wp_gd05_a_use_once`,
  `wp_gd05_b_destroy_first_shield_area_cards`, `wp_gd05_b_resolving_command_to_trash`.

Never in programs: If, May, IfYouDo, Simultaneous, ForEach, Repeat, ChooseMode (flattened away),
`d.Replacement` abilities (compiled but no engine path runs them; no card uses one). Instructions
with no current use in the registered data: DamagePlayer, RestResources, AddToShields, Shuffle,
SetDid, random Choose/Discard (census of all 626 programs).

### 13.5 System programs (interp.py:1581-1585)

- `"deploy"`: `(DeployCard(Var("card")),)`
- `"pair"`: `(Pair(Var("pilot"), Var("unit")),)`
- `"hand_limit"`: `(Discard(count=Sum((HandSize(), -10))),)` (pushed with host -1, game.py:308)

### 13.6 Cost programs (registry.py:153-187)

`cost_steps(costs)` emits, for the cost at index i (`var = f"__cost{i}"`, i counts every cost,
including ① ones): RestSelf → `Rest(This())`; RestCards → `Choose(var, Sel(side, loc,
(*filters, IsRested(False)), top_n), count, targeting=False)` + `Rest(Var(var))`; DestroyCards →
`Choose(var, sel, count, targeting=False)` + `Destroy(Var(var))`; ExileCards → same + `Exile`;
DiscardCards → `Discard(count=count, filters=sel.filters, var=var)`; ReturnSelf →
`ReturnToHand(ThisCard())`; DestroySelf → `Destroy(This())`; TrashSelf → `ToTrash(ThisCard())`;
PayResources → nothing (the engine pays ① at activation, game.py:888-896).
An Activated ability's program is `cost_steps(costs) + steps`; a PlayModifier's extra costs are a
separate program (`cost_program_id`) pushed as a "system" frame ABOVE the play frame, so it runs
first (game.py:953-954). Support: program 2 = Rest(This), Choose("t", friendly BATTLE units other
than This), Apply(Var t, StatMod(ap=KwAmount(SUPPORT)), THIS_TURN), CustomStep("support_used")
(registry.py:60-65). `support_cost_program` stays -1.

---------------------------------------------------------------------------------------------

## 14. Derived-view reads that must stay snapshot-exact

Derived is a pure function of the state, so Rust may cache it by (version, view key), but these
handlers hold one `dv` across mutations and must use that pre-mutation view:
`_h_set_active` (CANT_BE_SET_ACTIVE for all cards), `_enter_rested`, `destroy` (CANT_BE_DESTROYED,
`linked`), `shields_receiving_damage` (both `_prevented` calls), `_h_return_to_hand`
(`_returnable`), `emit` (one `dv` for every candidate, cond, delayed and keyword check),
`pair` (link computed after the moves and touches), `_h_apply` (`before` AP from the pre-lasting
view, comparison with a fresh one).

---------------------------------------------------------------------------------------------

## 15. Every game-RNG consumption point (`st.rng`, SplitMix64)

`SplitMix64` (rng.py:15-48): `state = seed & (2^64-1)`; `next_u64`: `state += 0x9E3779B97F4A7C15`
(mod 2^64); `z = state; z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9; z = (z ^ (z >> 27)) *
0x94D049BB133111EB; return z ^ (z >> 31)` (all mod 2^64). `randrange(n)`: ValueError if n <= 0;
`limit = 2^64 - (2^64 % n)` (a 65-bit value when n is a power of two or 1); loop
`x = next_u64()`; return `x % n` if `x < limit`. `shuffle(items)`: for `i` from `len - 1` down
to 1: `j = randrange(i + 1)`; swap `items[i], items[j]`.

In this scope, in execution order of the instruction that runs:
1. `core.shuffle_deck` (core.py:136): one `shuffle` of the player's whole DECK list. Reached by
   `Shuffle` (interp.py:1229), `ToDeck(shuffle=True)` (interp.py:797-798, owners ascending),
   and `game._redraw` (game.py:200).
2. `Choose(random=True)` (interp.py:422-427): `min(n, len(cands))` calls of
   `randrange(len(pool))` with the pool shrinking (pop at the drawn index).
3. `Discard(random=True)` (interp.py:532-535): same pattern on the discard candidates.
4. Custom step `return_looked_bottom` (effects/builtins.py:23): one `shuffle` of the looked
   cards still in the deck (in their `vars["looked"]` order), appended to the bottom; if more
   than one card, `known = 0` for each.

Outside this scope: `new_game` deck shuffle + cut + chooser roll (game.py:137-153). No other
`st.rng` use in `src/gcg_sim/engine` or the bindings (grep).

---------------------------------------------------------------------------------------------

## 16. Hazards (Python semantics the Rust port must reproduce)

1. **randrange limit overflow**: `(1 << 64) - ((1 << 64) % n)` needs 65 bits (rng.py:34); with
   u64 compute `2^64 mod n` as `(u64::MAX % n + 1) % n` and treat a zero remainder as "accept
   every x". `randrange(1)` still consumes one `next_u64` (verified by running it).
2. **RNG consumed before the auto-choose shortcut** in random Choose/Discard
   (interp.py:422-431, 532-537): the random branch runs before the "take all" checks.
3. **Negative list indexing**: `st.cards[host]` with `host == -1` silently reads the last card
   in `_delayed_triggers` (core.py:398) if a delayed trigger was created by a host-less frame.
   Every other `st.cards[...]` in scope is guarded by `>= 0` checks.
4. **`list.remove` removes the first occurrence and raises if absent** (core.py:44,
   interp.py:1197, builtins.py:21). Moving a card already in its destination zone (e.g. a Pilot
   that is also a ToDeck/ReturnToHand target after following its Unit) removes and re-adds it and
   bumps `zone_seq` twice.
5. **Default `bottom=False` = top of deck** for `move(..., Zone.DECK)` (core.py:103-104);
   `ToDeck`'s DSL default is `bottom=True` (dsl.py:916).
6. **Pilot moves before its Unit** inside `move` (core.py:73-76); ordering within ordered zones
   follows.
7. **Event payload order** `subject, player, group, *sorted(extra)` (core.py:331-333); keys
   are str, compared by code point. Event tuples end up in `TriggerInst.event`/`Frame.event` and
   the final-state digest.
8. **`sorted(lki, key=(owner != active, uid))`** (core.py:341): `False < True`, so the active
   player's LKI hosts come first.
9. **Stable sort by bool key** in SetResourcesActive/RestResources (interp.py:1067, 1088): normal
   Resources first, original order kept within each group.
10. **`dict` insertion order**: `lki` dicts (targets order), `hits` (doomed order), `by_player`
    (sorted before use), `info`, `before`, `froms`, `f.ints`/`f.vars` (serialized sorted). The
    rules_of lists depend on view.py's `dv.abilities` insertion order (players 0 then 1).
11. **Sets**: `once_used` (membership only; serialized sorted), `owners`/`pending_owners`
    (sorted before iteration), `chosen`, `excl`, `names`, `losers` (size/unpack only),
    `listened_events` (membership only). No set is iterated unsorted in this scope.
12. **Short-circuit side effects**: `_prevented` mutates `once_used`
    (core.py:523-527); `shields_receiving_damage` calls it twice with `and` short-circuit
    (core.py:692-697); `resolve_damage` checks prevention before reduction; `_reduce` stops at
    `amount <= 0` before consuming further once keys (core.py:557-558).
13. **Unconditional `next_group()`** even when nothing happens: draw, discard, Damage, Rest,
    SetActive, ReturnToHand (non-empty targets), Exile, AddToHand, deploy_cards, DeployToken,
    ShieldToHand, Apply (StatMod with ap not literally 0), pair, destroy (non-empty targets),
    destroy_shields (non-empty uids), `_emit_destroys_with_effect_damage`. `event_group` is
    serialized.
14. **`eff.ap != 0` on a non-int Value** is True (dataclass inequality vs int), so Apply calls
    `next_group()` and scans AP even when the value evaluates to 0 (interp.py:1266).
15. **`isinstance(x, int)` accepts `bool`** (core.py:518, 678; interp.py:1279, 1281): a `True`
    amount would count as 1. No current data uses bool amounts.
16. **Identity vs equality**: `ab.cond is not d.TRUE` (core.py:349) is an identity check on the
    singleton `And(())`; an equal-but-distinct `And(())` is evaluated (and is still True).
    `dt == ins` in `_h_delayed` (interp.py:1536) is structural equality: two identical delayed
    triggers on one card share the first index.
17. **Structural interning with first-use ids**: `R.program`, `R.cont_key`, `R.filters_key`
    (registry.py:115-138) key dicts by frozen dataclasses (`hash()` of dataclass trees). Ids
    of lazily interned items (system programs, lasting `effect_key`, `filters_key`) depend on
    first use in the PROCESS, across games. `runner/digest.py` replaces them with
    `sha256(repr(obj))[:16]`, so the Rust digest needs CPython `repr` of `Program` and DSL nodes
    (including the first registrant's label of shared programs).
18. **Version counting** (section 4): `version` is serialized and hashed; `pair` touches twice,
    `_h_custom` always touches, `pay_generic` touches once per EX Resource plus once at the end.
19. **`_rules_checked` cache is behaviour**: it skips `effect_hits.clear()` and is set only on
    two of the five in-loop exit paths ("nothing damaged", "nothing lethal"; not on deck-out,
    "no doomed", "nothing destroyed"; core.py:864-891); it is not serialized (resets to -1 on
    load, state.py:511, 614-651).
20. **Rules management runs after a WAIT** (interp.py:125-130): it can end the game (clearing
    `st.pending`) or queue triggers while a decision is pending.
21. **`_r_choose` uses `cands.index(...)`** (interp.py:457): ValueError if the picked uid is no
    longer a candidate; `_choose_options` uses the first-occurrence index too (interp.py:347).
22. **Discard follow-up options use uid order** (`u > max(buf)`), the first decision uses hand
    order, and an empty result falls back to every remaining candidate (interp.py:550-553).
23. **Exceptions as control flow / bug signals**: `EngineError` for a missing handler, resumer
    or unregistered delayed trigger (interp.py:156, 166, 1540); `assert isinstance(...)`
    (core.py:258, 345, 373); `(loser,) = losers` (core.py:849).
24. **`emit` drops everything once `st.winner` is set** (core.py:326), including delayed and
    keyword triggers.
25. **Mutable aliasing**: `ctx_of(f)` shares `f.vars` (view.py:43); `push_frame` copies the
    passed vars (`dict(vars or {})`, interp.py:81); `f.buf` is reassigned, never aliased.
26. **Tuple/list slicing makes copies**: `zones[p][SHIELD][:n]`, `deck[:n]` are snapshots taken
    before the moves (interp.py:1106, 1121, 1133, 1144); `damage_shield_area` slices the shields
    before destroying (core.py:765).
27. **`str.isdigit`** in `push_frame` (interp.py:84) also accepts non-ASCII digits; all event
    keys are ASCII.
28. **`max(0, ...)` clamps**: counts (`range(max(0, n))`), Repeat (`-max(0, times)`),
    `choose_bounds`, `_reduce`, `play_cost + cost_delta`. Python ints never overflow; counters
    (`version`, `event_group`, `zone_seq`, serials) must not wrap in Rust (use i64/u64).

---------------------------------------------------------------------------------------------

## 17. Open questions

1. `ToDeck` when a targeted Unit's Pilot is also a target: `moved`/`together` contain the Pilot
   twice (interp.py:800-808), which gives duplicate `SELECT` options in the ARRANGE decision.
   Whether this is reachable with current cards is not established.
2. `resolve_damage` redirect recursion (core.py:588-591) has no cycle guard (A→B and B→A
   redirect rules would recurse without bound). Not known to be reachable.
3. Whether any reachable game state ends with frames/batches/pending triggers on the stack
   (they are kept by `set_winner` and hashed into `final_state_sha256`): e.g. a deck-out during
   an effect's Draw. If so, the Rust digest needs the exact `Frame`/`TriggerInst` JSON and
   program content ids (hazard 17).
4. The `RESOLVING`-but-not-in-list branch of `_remove_from_zone` (core.py:39-42) has no
   identified producer; probably defensive.
5. A delayed trigger created by a frame whose `card_uid` card does not carry the DelayedTrigger
   in its own/unit text (e.g. a delayed body that itself creates a delayed trigger while
   hosted by a Unit with Pilot text) raises EngineError (interp.py:1539-1540); not known to occur.
6. `TriggerInst.optional`, `Frame.answer` and `vars["__play"]` are written/serialized but never
   read; keep them only for the serialized state.
