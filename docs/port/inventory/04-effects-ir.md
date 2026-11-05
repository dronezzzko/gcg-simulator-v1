# 04 — Effects IR: DSL, programs, registry, hooks, golden files

Scope: `src/gcg_sim/effects/*` (`dsl.py`, `text.py`, `program.py`, `registry.py`, `builtins.py`,
`scripts.py`, `golden.py`, `compiler/` overview) and `src/gcg_sim/effects/bindings/*` (binding API
and every custom hook). Engine consumers are cited where a DSL node is evaluated, but the
interpreter and view internals are documented in their own inventory files.

Snapshot: branch `rust-port` at `d9d41af`, Python 3.15.1. All numbers below were measured with
`uv run python` against `get_registry()` (scratch scripts, not committed).

`src/gcg_sim/effects/` has **no `__init__.py`** (implicit namespace package);
`effects/bindings/` and `effects/compiler/` do.

---

## 1. Module map and load order

| File | Lines | Role |
| --- | --- | --- |
| `effects/dsl.py` | 1399 | Every DSL node type (137 frozen, slotted dataclasses) and 13 enums. Pure data. |
| `effects/program.py` | 181 | Control-flow instruction classes and `flatten()` (step tree to linear program). |
| `effects/registry.py` | 406 | `Registry`: programs, abilities, card entries, continuous/filter tables, event index. |
| `effects/scripts.py` | 24 | `script_for(cdef)`: binding first, else compiler (`scripts.py:12-16`). Imports `builtins` for its side effect (`scripts.py:6,19`). |
| `effects/builtins.py` | 72 | 5 engine-level custom hooks used by compiler templates and keyword programs. |
| `effects/text.py` | 135 | Text normalization, `text_hash`, ability-line splitting. Compile time only. |
| `effects/golden.py` | 55 | `node_to_json` and per-card golden record. |
| `effects/compiler/` | 2773 | Text-to-DSL compiler (`__init__.py` 67, `abilities.py` 42, `grammar.py` 2300, `nouns.py` 364). |
| `effects/bindings/__init__.py` | 96 | `@card`, `@custom_step/cond/filter/value`, `load_bindings()`. |
| `effects/bindings/wp_*.py` | 17 modules | 302 per-card bindings and 29 custom hooks. |

Load order (all process-global, `functools.cache`d):

1. `get_registry()` (`registry.py:400-402`) calls `build_registry(get_card_db())`.
2. `build_registry` imports `scripts.script_for` lazily (`registry.py:324`); importing `scripts`
   imports `builtins` (registers 5 hooks) and `bindings` (package only).
3. The first `script_for` call runs `load_bindings()` (`bindings/__init__.py:90-96`): imports every
   module in `bindings/` whose name does not start with `_`, **sorted by module name**, and returns a
   copy of `_BINDINGS`. Each import registers `@card` bindings and custom hooks as a side effect.
4. Hook decorators write into engine-module dicts: `interp.CUSTOM_STEPS` (`interp.py:36`),
   `view.CUSTOM_CONDS/CUSTOM_FILTERS/CUSTOM_VALUES` (`view.py:116-118`).

---

## 2. Implementation counts (measured)

| Quantity | Value |
| --- | --- |
| `CardDB` definitions (`len(db)`) | 1153 = 1152 real card numbers + 1 synthetic Unit variant `UNIT:GD05-089:3/3` |
| Synthetic inline tokens (`TOKEN:*`) | 0 (every inline token definition matched a T- card) |
| Registry card entries | 1153 (one per def_id, same order) |
| Entries with `script is None` (unimplemented) | **0** |
| Bound (`@card` binding wins) | **302** (all real cards; `source="binding"`) |
| Compiled by the text compiler | **538** |
| Vanilla (no abilities) | **313** = 312 real + the synthetic variant |
| Implemented in total | 1153 / 1153; non-vanilla real cards: 840 |
| `CardScript.source == "mixed"` | 0 (value exists in the docstring only, `dsl.py:1398`) |
| Abilities (`reg.abilities`) | 1260 |
| Programs at build end (`reg.programs`) | 623 (3 keyword + 615 ability + 3 `#delayed` + 2 `#alt`) |
| Instructions in all programs | 1830 (max 14 per program) |
| Abilities with a program | 980, sharing 615 distinct program ids (dedupe) |
| Abilities whose program label names another card | 364 (label = first registrant) |
| `reg.continuous` / `reg.filter_sets` at build end | 0 / 0 (allocated lazily during games) |
| `reg.aliases` | 3: `GD02-098 -> ("Char Aznable",)`, `ST12-011 -> ("Zechs Merquise",)`, `ST12-012 -> ("Marida Cruz",)` |
| `reg.always_events` | `{destroys_by_battle, turn_end}` |
| `reg.hp_reduction_possible` | `False` |
| Cards with `gated=True` / `has_burst` / `command_aid>=0` | 112 / 252 / 157 |
| Cards whose `events_by_def` is non-empty | 480 |

By card type (effective source): BASE binding 32 / compiled 40; COMMAND binding 60 / compiled 97;
PILOT binding 50 / compiled 77; UNIT binding 158 / compiled 317 / vanilla 126; UNIT TOKEN
binding 2 / compiled 7 / vanilla 20; RESOURCE vanilla 108; EX BASE vanilla 30; EX RESOURCE vanilla
29 (`compiler/__init__.py:16` skips RESOURCE, EX RESOURCE, EX BASE text entirely).

Bindings per module: wp_eb01_a 17, wp_eb01_b 15, wp_gd01_a 11, wp_gd01_b 16, wp_gd02_a 14,
wp_gd02_b 13, wp_gd03_a 31, wp_gd03_b 26, wp_gd04_a 27, wp_gd04_b 18, wp_gd05_a 24, wp_gd05_b 26,
wp_st01_04 12, wp_st05_08 12, wp_st09_11 15, wp_st12_14 23, wp_tokens 2 (sum 302).

Ability kinds (own / unit-text): Triggered 417/97, Burst 252/0, Command 157/0, Constant 124/26,
Keyword 122/1, Activated 53/4, PlayModifier 4/0, NameAlias 3/0, Replacement 0/0.
Keyword abilities by keyword: Blocker 48, Breach 26, Support 14, Repair 19, Suppression 8,
High-Maneuver 6, First Strike 2 (all `gate=none`). No `Activated.support > 0` exists in the data.

DSL classes never referenced by any card script (17): `AddToShields`, `DamagePlayer`, `DeckSize`,
`DeployedThisTurn`, `ForEach`, `HasBurst`, `HasTiming`, `HasZone`, `KwAmount` (used only by the
engine keyword programs, `registry.py:58,63`), `NameIs`, `PairedUnitOf`, `Replacement`,
`RestResources`, `ReturnSelf`, `Shuffle`, `TrashSelf` (+ `CardScript` itself is the root).
They must still be ported: tests and future data use them.

---

## 3. DSL catalog (`effects/dsl.py`)

All node classes are `@dataclass(frozen=True, slots=True)`. Equality and hashing are structural
(field-by-field, same class required). Field order below is declaration order, which is also the
order of `repr()` and of `dataclasses.fields()` (used by `golden.node_to_json` and
`registry._walk_nodes`). `X=v` means default `v`. "Consumer" is the evaluating function.

### 3.1 Enums

All DSL enums are `StrEnum` (members compare equal to their string value, `repr` is
`<Cls.NAME: 'value'>`). Declaration order matters only where noted.

| Enum (line) | Members `NAME = "value"` |
| --- | --- |
| `P` (`dsl.py:20`) | `YOU="you"`, `OPP="opponent"`, `ACTIVE="active"`, `STANDBY="standby"` |
| `Side` (`:29`) | `FRIENDLY="friendly"`, `ENEMY="enemy"`, `ANY="any"` |
| `Loc` (`:35`) | `BATTLE="battle"`, `PAIRED="paired"`, `BASE="base"`, `SHIELDS="shields"`, `SHIELD_AREA="shield_area"`, `RESOURCE_AREA="resource_area"`, `HAND="hand"`, `TRASH="trash"`, `DECK="deck"`, `DECK_TOP="deck_top"`, `RESOURCE_DECK="resource_deck"`, `REMOVAL="removal"`, `FIELD_UNITS_AND_BASES="units_and_bases"` |
| `Op` (`:51`) | `LE="<="`, `GE=">="`, `EQ="=="`, `LT="<"`, `GT=">"`, `NE="!="` |
| `Kw` (`:60`) | `REPAIR="Repair"`, `BREACH="Breach"`, `SUPPORT="Support"`, `BLOCKER="Blocker"`, `FIRST_STRIKE="First Strike"`, `HIGH_MANEUVER="High-Maneuver"`, `SUPPRESSION="Suppression"` |
| `Stat` (`:75`) | `LV="lv"`, `COST="cost"`, `AP="ap"`, `HP="hp"`, `DAMAGE="damage"`, `REMAINING_HP="remaining_hp"` |
| `CardKind` (`:84`) | `UNIT="unit"`, `PILOT="pilot"`, `COMMAND="command"`, `BASE="base"`, `RESOURCE="resource"`, `TOKEN="token"`, `UNIT_TOKEN="unit_token"`, `EX_RESOURCE="ex_resource"`, `EX_BASE="ex_base"`, `LINK_UNIT="link_unit"` |
| `RuleKind` (`:585`) | see 3.7 (29 members) |
| `DamageKind` (`:625`) | `ANY="any"`, `BATTLE="battle"`, `EFFECT="effect"` |
| `Ev` (`:692`) | `DEPLOYED="deployed"`, `ATTACKS="attacks"`, `BLOCKS="blocks"`, `BLOCKED="blocked"`, `DESTROYED="destroyed"`, `PAIRED="paired"`, `LINKED="linked"`, `DAMAGED="damaged"`, `DESTROYS_BY_BATTLE="destroys_by_battle"`, `DESTROYS_SHIELD_CARD="destroys_shield_card"`, `DEALS_DAMAGE="deals_damage"`, `COST_PAID="cost_paid"`, `SUPPORT_USED="support_used"`, `AP_REDUCED="ap_reduced"`, `SHIELD_DESTROYED="shield_destroyed"`, `TURN_START="turn_start"`, `TURN_END="turn_end"`, `EX_RESOURCE_PLACED="ex_resource_placed"`, `RESOURCE_PLACED="resource_placed"`, `DISCARDED="discarded"`, `RETURNED_TO_HAND="returned_to_hand"`, `COMMAND_PLAYED="command_played"`, `COMMAND_RESOLVED="command_resolved"`, `RESTED="rested"`, `SET_ACTIVE="set_active"`, `RECOVERED="recovered"`, `DRAWN="drawn"`, `ADDED_TO_HAND="added_to_hand"`, `SHIELD_TO_HAND="shield_to_hand"`, `BATTLE_END="battle_end"`, `LEFT_BATTLE_AREA="left_battle_area"`, `BURST_ACTIVATED="burst_activated"`, `EXILED="exiled"` |
| `Timing` (`:1250`) | `MAIN="main"`, `ACTION="action"`, `MAIN_OR_ACTION="main_or_action"` |
| `Where` (`:1256`) | `FIELD="field"`, `HAND="hand"`, `TRASH="trash"`, `ANY="any"` |
| `Gate` (`:1265`) | `NONE="none"`, `PAIRED="paired"`, `LINKED="linked"` |
| `Duration` (`engine/types.py:142`, imported by dsl) | `THIS_TURN="this_turn"`, `THIS_BATTLE="this_battle"`, `OPPONENT_NEXT_TURN="opponent_next_turn"`, `YOUR_NEXT_TURN="your_next_turn"`, `WHILE_ON_FIELD="while_on_field"` |

Numeric codes derived from declaration order:

- `view.LOC_CODES = {loc: i for i, loc in enumerate(d.Loc)}` (`view.py:936`): battle 0, paired 1,
  base 2, shields 3, shield_area 4, resource_area 5, hand 6, trash 7, deck 8, deck_top 9,
  resource_deck 10, removal 11, units_and_bases 12. Used for the `from_loc` event payload
  (`view.zone_loc_code`, `view.py:952-954`; `-1` for zones without a Loc) and `EventFrom`
  (`view.py:910-911`).
- `STACKING_KEYWORDS = frozenset({REPAIR, BREACH, SUPPORT})` (`dsl.py:72`), membership only.
- Sorted order of `Kw` (by string value, used by `wp_gd04_a.py:78`): Blocker, Breach,
  First Strike, High-Maneuver, Repair, Support, Suppression.

Sentinels: `TRUE: Cond = And(())` (`dsl.py:579`); bindings and grammar also use `Or(())` as
"never" (`wp_eb01_a.py:21`, `grammar.py:28`).

### 3.2 Values (`type Value = int | ...`, `dsl.py:180-195`)

Consumer: `view.value(st, dv, ctx, v) -> int` (`view.py:853-882`). A bare Python `int` is a
Value (`view.py:854-855`, `isinstance(v, int)`, so a `bool` would also pass; none occur).

| Class (line) | Fields | Semantics (consumer line) |
| --- | --- | --- |
| `Count` (`:101`) | `sel: Sel` | `len(select(sel))` (`view.py:856`) |
| `StatOf` (`:108`) | `ref: Ref; stat: Stat` | `stat()` of the first resolved card, else 0 (`:858-860`) |
| `PlayerLevel` (`:114`) | `player: P=YOU` | number of cards in that player's resource area (`:861`) |
| `HandSize` (`:121`) | `player: P=YOU` | hand length (`:863`) |
| `ShieldCount` (`:126`) | `player: P=YOU` | shield section length (`:865`) |
| `DeckSize` (`:131`) | `player: P=YOU` | deck length (`:867`) |
| `VarSize` (`:136`) | `var: str` | `len(ctx.vars.get(var, ()))` (`:869`) |
| `EventAmount` (`:143`) | `key: str="amount"` | `max(0, ctx.ev(key, 0))` (`:871`) |
| `Sum` (`:150`) | `terms: tuple[Value, ...]` | integer `sum` (`:873`) |
| `Times` (`:155`) | `value: Value; factor: int` | `value * factor` (`:875`) |
| `MinOf` (`:161`) | `terms: tuple[Value, ...]` | `min` of terms; raises on empty (`:877`) |
| `KwAmount` (`:166`) | `keyword: Kw; ref: Ref=This()` (default_factory) | `kw_amount(dv, first card, keyword)`, else 0 (`:879-881`) |
| `CustomValue` (`:174`) | `name: str; params: tuple[tuple[str, object], ...]=()` | `CUSTOM_VALUES[name](st, dv, ctx, dict(params))` (`:882`) |

`view.stat(st, dv, uid, s)` (`view.py:673-686`): AP = `ap_of`; HP and REMAINING_HP =
`max(0, hp_of - damage)` (FAQ Q96: HP means current HP); LV = `level_of` (0 for tokens); COST =
`cost_of` (0 for tokens); DAMAGE = `card.damage`.

### 3.3 Filters (`type Filter`, `dsl.py:362-390`)

All filters in a tuple are conjunctive: `view.matches` = `all(_match1(...))` (`view.py:702-705`),
evaluated left to right with short-circuit.

| Class (line) | Fields | Semantics (`view._match1` line) |
| --- | --- | --- |
| `IsKind` (`:201`) | `kinds: tuple[CardKind, ...]` | any kind matches via `_kind_ok` (`view.py:712`, `_kind_ok` `:641-670`; PILOT includes a Command in the PAIRED zone; COMMAND excludes a paired Command unless the event has `("subject",uid)` and `("as_command",1)`; LINK_UNIT = unit in `dv.linked`) |
| `HasTrait` (`:208`) | `traits: tuple[str, ...]` | any trait in current traits (derived traits only in BATTLE/BASE, else printed) (`:714-716`) |
| `NameContains` (`:215`) | `parts: tuple[str, ...]` | any part is a substring of any name incl. aliases (`:717`) |
| `NameIs` (`:222`) | `names: tuple[str, ...]` | any name (incl. aliases) equals one of `names` (`:719`) |
| `HasColor` (`:229`) | `colors: tuple[str, ...]` | non-token with `color.value in colors` (`:721`) |
| `StatCmp` (`:234`) | `stat: Stat; op: Op; value: Value` | `_cmp(stat(uid), op, value)` (`:723`) |
| `IsRested` (`:241`) | `rested: bool=True` | `card.rested == rested` (`:725`) |
| `HasKeyword` (`:246`) | `keyword: Kw` | BATTLE/BASE: in derived keywords; elsewhere: in `printed_keywords` (`:727-730`) |
| `IsLinked` (`:251`) | `linked: bool=True` | `(uid in dv.linked) == linked` (`:731`) |
| `IsPaired` (`:256`) | `paired: bool=True` | `(zone is BATTLE and pair>=0) == paired` (`:733`) |
| `PairedWith` (`:261`) | `filters: tuple[Filter, ...]` | Unit in BATTLE whose paired Pilot matches (`:735-740`) |
| `PairedTo` (`:268`) | `filters: tuple[Filter, ...]` | Pilot in PAIRED whose Unit matches (`:741-746`) |
| `IsToken` (`:275`) | `token: bool=True` | `cd.is_token == token` (`:747`) |
| `IsDamaged` (`:280`) | `damaged: bool=True` | `(damage > 0) == damaged` (`:749`) |
| `NotRef` (`:285`) | `ref: Ref` | uid not in `resolve(ref)` (`:751`) |
| `IsRef` (`:292`) | `ref: Ref` | uid in `resolve(ref)` (`:753`) |
| `IsAttacking` (`:297`) | `attacking: bool=True` | live battle and `attacker == uid` (`:755-757`) |
| `IsBattling` (`:302`) | `battling: bool=True` | live battle and uid in (attacker, target) (`:758-760`) |
| `HasTiming` (`:307`) | `timing: str` | `timing in reg.cards[def_id].timings` (`:761`) |
| `HasBurst` (`:314`) | `burst: bool=True` | `entry.has_burst == burst` (`:763`) |
| `DeployedThisTurn` (`:319`) | `value: bool=True` | in BATTLE/BASE and `entered_turn == st.turn` (`:765`) |
| `HasZone` (`:324`) | `zones: tuple[str, ...]` | any printed zone string matches (`:767`) |
| `LevelCmpRef` (`:331`) | `stat: Stat; op: Op; ref: Ref; ref_stat: Stat` | compare own stat with first resolved card's stat; False if none (`:769-773`) |
| `AnyOf` (`:341`) | `filters: tuple[Filter, ...]` | disjunction (`:774`) |
| `AllOf` (`:346`) | `filters: tuple[Filter, ...]` | conjunction (`:776`) |
| `Not` (`:351`) | `filter: Filter` | negation (`:778`) |
| `CustomFilter` (`:356`) | `name: str; params=()` | `CUSTOM_FILTERS[name](st, dv, ctx, uid, dict(params))` (`:780-781`) |

### 3.4 Selector

| Class (line) | Fields | Semantics |
| --- | --- | --- |
| `Sel` (`:393`) | `side: Side; loc: Loc; filters: tuple[Filter, ...]=(); top_n: Value=0` | `view.select` (`view.py:626-632`): players from `_players(controller, side)` (`:577-582`: FRIENDLY `(c,)`, ENEMY `(1-c,)`, ANY `(c, 1-c)`), then each player's list from `_loc_cards` (`:595-623`) in zone-list order, filtered. `SHIELD_AREA` = BASE list + SHIELD list; `DECK_TOP` = `deck[:max(0, value(top_n))]`; `FIELD_UNITS_AND_BASES` = BATTLE + BASE (the fall-through `return`). |

`Trigger.subject` uses only `side` and `filters` of its `Sel`; `loc`/`top_n` are ignored
(`core._subject_matches`, `core.py:163-184`).

### 3.5 References (`type Ref`, `dsl.py:462-464`)

Consumer: `view.resolve(st, dv, ctx, ref) -> tuple[int, ...]` (`view.py:784-831`), ordered.
`view.resolve_ref` computes the derived view only when `selects(ref)` (`view.py:834-850`).

| Class (line) | Fields | Semantics |
| --- | --- | --- |
| `This` (`:411`) | — | `(ctx.host,)` unless `NO_ARG` (`view.py:785`); for Pilot-granted text, the paired Unit |
| `ThisCard` (`:416`) | — | `(ctx.card_uid,)` (`:787`) — the physical card owning the text |
| `Var` (`:421`) | `name: str` | `ctx.vars.get(name, ())` (`:789`) |
| `EventCard` (`:426`) | `key: str="subject"` | payload value if `>= 0` (`:791-793`); keys used: blocker, pilot, source, subject, target |
| `PairedPilotOf` (`:433`) | `ref: Ref` | each card's pair if in BATTLE and paired; LKI fallback via event `subject`/`pilot` when the pilot is in the same zone as the card (`:794-805`) |
| `PairedUnitOf` (`:438`) | `ref: Ref` | for Pilots in PAIRED, their Unit (`:806-812`) |
| `BattlingWith` (`:443`) | `ref: Ref` | attacker->target (if `target >= 0`), target->attacker, in a live battle (`:813-823`) |
| `All` (`:450`) | `sel: Sel` | `tuple(select(sel))` (`:824-825`) |
| `Union` (`:457`) | `refs: tuple[Ref, ...]` | concatenation with first-seen dedupe (list `in` check) (`:826-831`) |

### 3.6 Conditions (`type Cond`, `dsl.py:562-577`)

Consumer: `view.cond(st, dv, ctx, c, did=True) -> bool` (`view.py:885-933`).

| Class (line) | Fields | Semantics |
| --- | --- | --- |
| `Cmp` (`:470`) | `left: Value; op: Op; right: Value` | `_cmp(value(left), op, value(right))` (`:892`; `_cmp` `:688-699`) |
| `Exists` (`:477`) | `sel: Sel; at_least: int=1` | `len(select) >= at_least` (`:894`) |
| `IsTurn` (`:483`) | `player: P=YOU` | `st.active == player_of(player)` (`:896`) |
| `RefMatches` (`:488`) | `ref: Ref; filters: tuple[Filter, ...]` | non-empty and every resolved card matches (`:898-900`) |
| `RefEmpty` (`:496`) | `ref: Ref` | resolves to nothing (`:901`) |
| `AttackTargetIs` (`:501`) | `kind: str` ("unit" or "player") | live battle; "player": target is `PLAYER_TARGET` (-2) or a Base; otherwise target `>=0` in BATTLE (`:903-909`) |
| `EventFrom` (`:508`) | `loc: Loc` | `ctx.ev("from_loc") == LOC_CODES[loc]` (`:910`) |
| `EventFlag` (`:515`) | `key: str; value: bool=True` | `bool(ctx.ev(key, 0)) == value` (`:912`); keys used: by_effect, ex_used, linked |
| `HappenedThisTurn` (`:523`) | `kind: str; player: P=YOU; by: P\|None=None; at_least: int=1; filters=(); source_filters=()` | counts `st.history` records with `h.kind == kind`, `h.player == who`, `by` match, uid/source filters (`:914-930`). Kinds used: command_activated, cost_paid, deployed, destroyed, destroys_by_battle_paired, discard, paired, resource_set_active |
| `DidLast` (`:536`) | — | returns the `did` argument (`:931`); `JumpIfNot` passes `f.did` (`interp.py:188`) |
| `And` (`:541`) | `conds: tuple[Cond, ...]` | `all` (short-circuit) (`:886`) |
| `Or` (`:546`) | `conds: tuple[Cond, ...]` | `any` (`:888`) |
| `NotC` (`:551`) | `cond: Cond` | negation (`:890`) |
| `CustomCond` (`:556`) | `name: str; params=()` | `CUSTOM_CONDS[name](st, dv, ctx, dict(params))` (`:933`) |

### 3.7 Rule modifications

`RuleMod` (`dsl.py:631`): `kind: RuleKind; amount: Value=0; damage_kind: DamageKind=ANY;
source_filters: tuple[Filter, ...]=(); source_side: Side=ANY; once_per_turn: bool=False;
name: str=""`. Carried by `RuleGrant`; per-card instances become `view.RuleInst(rule, controller,
source, key, aux)` (`view.py:53-63`) in `dv.rules[uid]`; player-level ones are read directly from
`st.lasting` (`core._shield_area_rules`, `core.py:634-660`).

| `RuleKind` member = value | Uses in data | Engine consumer |
| --- | --- | --- |
| `CANT_ATTACK="cant_attack"` | 12 | `game.py:484` |
| `CANT_ATTACK_PLAYER="cant_attack_player"` | 12 | `game.py:513` |
| `CANT_ATTACK_UNITS="cant_attack_units"` | 0 | `game.py:494` |
| `CANT_BLOCK="cant_block"` | 2 | `battle.py:62` |
| `CANT_BE_BLOCKED="cant_be_blocked"` | 0 | `battle.py:51` |
| `MAY_ATTACK_ACTIVE="may_attack_active"` | 26 | `game.py:495` |
| `ATTACK_ON_DEPLOY_TURN="attack_on_deploy_turn"` | 4 | `game.py:488` |
| `CANT_BE_SET_ACTIVE="cant_be_set_active"` | 1 (+T-014) | `game.py:233`, `interp.py:723` |
| `STAYS_RESTED_IN_START_PHASE="stays_rested_in_start_phase"` | 6 | `game.py:234` |
| `DEPLOYED_RESTED="deployed_rested"` | 2 | `interp.py:984` |
| `CANT_BE_PAIRED="cant_be_paired"` | 3 | `interp.py:1343` |
| `CANT_RECEIVE_DAMAGE="cant_receive_damage"` | 25 | `core.py:517` |
| `REDUCE_DAMAGE="reduce_damage"` | 16 | `core.py:556` |
| `CANT_BE_DESTROYED="cant_be_destroyed"` | 1 | `core.py:787`, `invariants.py:105` |
| `CANT_BE_CHOSEN="cant_be_chosen"` | 0 | `interp.py:328` |
| `CANT_BE_RETURNED="cant_be_returned"` | 0 | `interp.py:736` |
| `NO_BLOCKER_VS="no_blocker_vs"` | 0 | `battle.py:54` |
| `MUST_ATTACK="must_attack"` | 0 | **none** |
| `PLAY_COST_DELTA="play_cost_delta"` | 0 | **none** |
| `PLAY_LEVEL_DELTA="play_level_delta"` | 0 | **none** |
| `ATTACK_TARGET_FIXED="attack_target_fixed"` | 0 | **none** |
| `CANT_BE_ATTACKED="cant_be_attacked"` | 2 | `game.py:497` |
| `FORCE_ATTACK_TARGET="force_attack_target"` | 5 | `game.py:481`, `game.py:530` |
| `SHIELD_AREA_PROTECTION="shield_area_protection"` | 2 | `core.py:671` |
| `SHIELD_AREA_REDUCTION="shield_area_reduction"` | 1 | `core.py:677` |
| `AP_CANT_BE_REDUCED="ap_cant_be_reduced"` | 1 | `view.py:468` |
| `REDIRECT_BATTLE_DAMAGE="redirect_battle_damage"` | 2 | `core.py:588` |
| `REST_SUBSTITUTE="rest_substitute"` | 2 | `interp.py:641`, `interp.py:673` (`name` is "unit"/"base" filter) |
| `CUSTOM="custom"` | 0 | **none** (only `ai/targeting.py:51`) |

("Uses in data" counts `RuleMod` nodes across all card scripts.)

### 3.8 Continuous effects (`type Continuous`, `dsl.py:686`)

Consumer for card-targeted effects: `view._apply_cont` (`view.py:347-406`), called for Constant
abilities (`view.py:429-436`, rule key `(aid, host)`) and lasting effects (`view.py:437-452`, rule
key `(-1, effect_key, serial)`). Values are evaluated against the previous fixed-point view.

| Class (line) | Fields | Semantics |
| --- | --- | --- |
| `StatMod` (`:648`) | `ap: Value=0; hp: Value=0` | adds AP/HP; a negative AP from a non-controller is skipped if `AP_CANT_BE_REDUCED` (HP still applied) (`view.py:359-369`); `ap != 0` uses Python `!=` (a node is always `!= 0`) |
| `KeywordGrant` (`:654`) | `keyword: Kw; amount: Value=0` | stacking keywords add `amount`, others set to 1 (`:370-379`) |
| `TraitGrant` (`:660`) | `traits: tuple[str, ...]` | appends traits not already present, in order (`:380-384`) |
| `RuleGrant` (`:665`) | `rule: RuleMod` | appends a `RuleInst` (`:385-389`) |
| `AbilityGrant` (`:670`) | `card_number: str; ability_index: int` | grants `[*ce.own, *ce.unit][ability_index]` of that card (index over own then unit abilities); unknown card number ignored (`:398-406`). **Also reused as the key of armed delayed triggers** (`interp.py:1549`) |
| `CostMod` (`:678`) | `cost: Value=0; level: Value=0` | adds to `dv.cost_mod`/`dv.level_mod` when non-zero (`:390-397`); player-level CostMod via `ApplyPlayer` summed in `view.player_cost_mod` (`view.py:525-545`) |

### 3.9 Triggers

`Trigger` (`dsl.py:730`): `event: Ev; self_only: bool=True; subject: Sel|None=None;
whose_turn: P|None=None; pilot_filters=(); target_filters=(); battle_only: bool|None=None;
by_enemy: bool|None=None; include_self: bool=True`.
Consumer `core._trigger_ok` (`core.py:194-233`), checks in this order: `whose_turn` (YOU means the
host owner's turn, **any other P value means the opponent's turn**, `core.py:203`); `self_only`
(subject == host) else `subject` Sel (side + filters, `include_self`) else `include_self`;
`pilot_filters` on payload `pilot`; `target_filters` on payload `target`; `battle_only` vs
`payload("battle") == 1`; `by_enemy` vs `by >= 0 and by != host_owner`.

### 3.10 Costs (`type Cost`, `dsl.py:804-814`)

| Class (line) | Fields | Lowered by `registry.cost_steps` (`registry.py:153-176`) | Affordability (`game.py:446-466`, `_costs_payable` `game.py:592-603`) |
| --- | --- | --- | --- |
| `RestSelf` (`:753`) | — | `Rest(This())` | host not rested and in BATTLE/BASE |
| `PayResources` (`:758`) | `amount: int` | nothing (paid by the engine on activation, `game.py:888-890`) | summed, `payment_choices` |
| `RestCards` (`:765`) | `sel: Sel; count: int` | `Choose("__cost{i}", sel + IsRested(False), count, targeting=False)`, `Rest(Var)` | `len(select(sel + IsRested(False))) >= count` |
| `DestroyCards` (`:771`) | `sel: Sel; count: int` | `Choose(var, sel, count, targeting=False)`, `Destroy(Var)` | `len(select) >= count` |
| `DiscardCards` (`:777`) | `sel: Sel; count: int` | `Discard(count=count, filters=sel.filters, var=var)` (side/loc ignored: own hand) | `len(select) >= count` |
| `ExileCards` (`:783`) | `sel: Sel; count: int` | `Choose(...)`, `Exile(Var)` | `len(select) >= count` |
| `ReturnSelf` (`:789`) | — | `ReturnToHand(ThisCard())` | host in BATTLE/BASE/PAIRED/HAND/TRASH |
| `DestroySelf` (`:794`) | — | `Destroy(This())` | same |
| `TrashSelf` (`:799`) | — | `ToTrash(ThisCard())` | same |

`i` in `__cost{i}` is the index over **all** costs, including `PayResources` and `RestSelf`
(`registry.py:157-158`); e.g. costs `(PayResources(1), ExileCards(...))` bind `__cost1`. The
variable name surfaces in the decision prompt `f"choose {var}"` (`interp.py:439,467`).

### 3.11 Steps (`type Step`, `dsl.py:1197-1244`, 46 classes)

"Handler" is the `_HANDLERS` entry (`interp.py:1597-1646`); "R" marks a resumer in `_RESUMERS`
(`interp.py:1648-1658`), i.e. the step can wait for a decision. Structural steps (If, May,
Simultaneous, IfYouDo, ChooseMode, ForEach, Repeat) have **no handler**: `flatten` replaces them
(section 4). Every handler sets `f.did`, which `IfYouDo`/`DidLast` read.

| Class (line) | Fields | Semantics | Handler |
| --- | --- | --- | --- |
| `Choose` (`:820`) | `var: str; sel: Sel; count: Value=1; min_count: Value\|None=None; optional: bool=False; chooser: P=YOU; distinct_from: tuple[str, ...]=(); targeting: bool=True; random: bool=False; after_then: bool=False` | binds `var`; group with following `distinct_from` Chooses; mandatory public targeting short-circuits the group; `random` pops `pool[randrange(len(pool))]`; auto-select when forced | `interp.py:407`, R `:451` |
| `ChooseMode` (`:842`) | `options: tuple[tuple[str, tuple[Step, ...]], ...]; chooser: P=YOU` | modal choice | flattened to `ModeSelect` |
| `Draw` (`:850`) | `count: Value=1; player: P=YOU; each_player: bool=False` | draw; `each_player` = active player then the other | `:489` |
| `Discard` (`:860`) | `count: Value=1; player: P=YOU; chooser: P\|None=None; random: bool=False; filters=(); var: str="discarded"` | discard from hand, binds `var` | `:523`, R `:543` |
| `Damage` (`:870`) | `ref: Ref; amount: Value` | effect damage from host to each card; shields go through shield damage | `:576` |
| `DamageShieldArea` (`:878`) | `player: P; amount: Value; cards: int=1` | damage first card(s) of the shield area | `:596` |
| `DamagePlayer` (`:887`) | `player: P; amount: Value` | as `DamageShieldArea` with `cards=1` | `:607` |
| `Destroy` (`:893`) | `ref: Ref` | destroy cards in BATTLE/BASE/PAIRED | `:616` |
| `Rest` (`:898`) | `ref: Ref` | rest active cards in BATTLE/BASE/RESOURCE_AREA; may offer `REST_SUBSTITUTE` choices | `:664`, R `:668` |
| `SetActive` (`:903`) | `ref: Ref` | set rested cards active unless `CANT_BE_SET_ACTIVE` | `:716` |
| `ReturnToHand` (`:908`) | `ref: Ref` | return from BATTLE/BASE/PAIRED/TRASH/RESOLVING unless `CANT_BE_RETURNED` | `:743` |
| `ToDeck` (`:913`) | `ref: Ref; bottom: bool=True; shuffle: bool=False` | to owners' decks; per-owner ordering decision for 2+ cards | `:781`, R `:833` |
| `Exile` (`:920`) | `ref: Ref` | move to REMOVAL, emit EXILED | `:838` |
| `ToTrash` (`:927`) | `ref: Ref` | field cards are destroyed, others moved | `:849` |
| `AddToHand` (`:934`) | `ref: Ref; reveal: bool=False` | move to hand | `:863` |
| `DeployCard` (`:940`) | `ref: Ref; rested: bool=False; pay_cost: bool=False; ignore_level: bool=True; as_unit: bool=False` | deploy Unit/Base cards (`pay_cost`/`ignore_level` are not read by the handler); `as_unit` swaps `def_id` to the Unit variant | `:923`, R (excess) `:899` |
| `DeployToken` (`:952`) | `token_key: str; count: Value=1; rested: bool=False; player: P=YOU; var: str="tokens"; also: tuple[tuple[str, int], ...]=()` | deploy tokens; key format `"{name}\|{traits joined by /}\|{ap}\|{hp}\|{text}"` (`cards/tokens.py:27-29`, parsed `interp.py:1018-1024`) | `:989`, R (excess) `:899` |
| `PlaceExResource` (`:965`) | `player: P=YOU; rested: bool=False` | new EX Resource if under limits (15 / 5) | `:1027` |
| `PlaceResource` (`:971`) | `player: P=YOU; rested: bool=False` | top of resource deck to resource area | `:1045` |
| `SetResourcesActive` (`:979`) | `count: Value=1; player: P=YOU` | set rested Resources active, normal before EX (stable sort) | `:1062` |
| `RestResources` (`:985`) | `count: Value=1; player: P=OPP` | rest active Resources, normal before EX | `:1083` |
| `ShieldToHand` (`:991`) | `count: Value=1; player: P=YOU` | top shields to hand | `:1103` |
| `AddToShields` (`:999`) | `ref: Ref\|None=None; count: Value=1; player: P=YOU; top: bool=True` | deck top (or ref) to shields | `:1117` |
| `Mill` (`:1009`) | `count: Value; player: P=YOU; var: str="milled"` | deck top to trash, binds `var` | `:1130` |
| `LookTop` (`:1018`) | `count: Value; player: P=YOU; var: str="looked"; reveal: bool=False` | binds top cards; `known \|= 1<<controller` (or both if reveal) | `:1141` |
| `Arrange` (`:1028`) | `ref: Ref; mode: str; player: P=YOU` | modes `top_or_bottom`, `top_or_bottom_each`, `top_any_order`, `bottom_any_order` (data uses the 1st and 4th) | `:1153`, R `:1205` |
| `Reveal` (`:1038`) | `ref: Ref` | `known = BOTH_KNOW` (3) | `:1221` |
| `Shuffle` (`:1043`) | `player: P=YOU` | `core.shuffle_deck` (RNG) | `:1228` |
| `Recover` (`:1048`) | `ref: Ref; amount: Value` | heal in-play cards | `:1234` |
| `Apply` (`:1054`) | `ref: Ref; effect: Continuous; duration: Duration=THIS_TURN; aux: Ref\|None=None` | lasting effect on live targets; non-int StatMod/KeywordGrant amounts are frozen to ints first (`_frozen`, `interp.py:1276-1283`); emits AP_REDUCED | `:1249` |
| `ApplyPlayer` (`:1067`) | `player: P; effect: Continuous; duration: Duration=THIS_TURN; filters=(); uses: int=0` | player-level lasting effect (no freezing) | `:1323` |
| `Pair` (`:1078`) | `pilot: Ref; unit: Ref` | pair first pilot with first unit | `:1347` |
| `PlayCard` (`:1086`) | `ref: Ref; free: bool=False; ignore_level: bool=False; cost_delta: Value=0` | play as if from hand; pushes `system:deploy` or the Command program | `:1396` (PUSHED) |
| `ActivateMain` (`:1096`) | `ref: Ref=ThisCard()` (default_factory) | run a Command's program as a sub-frame | `:1473` (PUSHED) |
| `StartBattle` (`:1103`) | `attacker: Ref; target: Ref; damage_only: bool=True` | `battle.damage_only_battle` (`damage_only` not read) | `:1495` |
| `ChangeAttackTarget` (`:1113`) | `target: Ref` | retarget the live battle | `:1508` |
| `If` (`:1118`) | `cond: Cond; then: tuple[Step, ...]; otherwise: tuple[Step, ...]=()` | conditional | flattened |
| `May` (`:1125`) | `steps; prompt: str=""; player: P=YOU` | yes/no | flattened to `AskMay` |
| `Simultaneous` (`:1134`) | `steps` | no rules management inside | flattened to `HoldRules` |
| `IfYouDo` (`:1142`) | `steps` | only if `f.did` | flattened to `JumpIfNotDid` |
| `ForEach` (`:1149`) | `ref: Ref; var: str; steps` | loop over resolved cards | flattened to loops |
| `Repeat` (`:1158`) | `times: Value; steps` | counted loop | flattened to loops |
| `BindVar` (`:1164`) | `var: str; ref: Ref` | `f.vars[var] = resolve(ref)` | `:1521` |
| `DelayedTrigger` (`:1172`) | `trigger: Trigger; steps; duration: Duration=THIS_TURN; bind_vars: tuple[str, ...]=()` | arms a delayed trigger as `Lasting(effect_key=cont_key(AbilityGrant(card_number, n)))` in `st.delayed` | `:1526` |
| `PayCost` (`:1183`) | `amount: int; player: P=YOU` | `pay_generic`; record + emit COST_PAID when paid and `amount > 0` | `:1562` |
| `CustomStep` (`:1191`) | `name: str; params=()` | `CUSTOM_STEPS[name](st, f, ctx, dict(params))`, `f.did = result`, then **always `st.touch()`** | `:1571` |

### 3.12 Abilities and scripts (`type Ability`, `dsl.py:1373-1383`)

| Class (line) | Fields | Consumer |
| --- | --- | --- |
| `Keyword` (`:1273`) | `keyword: Kw; amount: int=0; gate: Gate=NONE; gate_filters=()` | `view._base_view` (`view.py:291-296`: stacking add `amount`, others set 1); Repair/Breach triggers `core.py:441-480`; Support activation `game.py:880-883` (`aid == SUPPORT_AID` at `game.py:881`) (`SUPPORT_AID = -10`, `game.py:31`) |
| `Constant` (`:1281`) | `effects: tuple[Continuous, ...]; scope: Ref=This(); cond: Cond=TRUE; where: Where=FIELD; gate=NONE; gate_filters=()` | `view._compute` (`view.py:425-436`) |
| `Triggered` (`:1293`) | `trigger: Trigger; steps; cond: Cond=TRUE; once_per_turn: bool=False; where=FIELD; gate=NONE; gate_filters=()` | `core.emit` (`core.py:342-357`), `queue_trigger` (`core.py:248-286`) |
| `Activated` (`:1306`) | `timing: Timing; costs: tuple[Cost, ...]; steps; cond=TRUE; once_per_turn=False; where=FIELD; gate=NONE; gate_filters=(); support: int=0` | `game._activation_ok` (`game.py:436-470`), `_play_or_activate` (`game.py:875-892`) |
| `Command` (`:1321`) | `timing: Timing; steps; cond: Cond=TRUE` | `game.py:398-407`, `game.py:949`, `interp.py:1422,1480` |
| `Burst` (`:1330`) | `steps` | program wraps steps in `May(prompt="activate Burst?")` (`registry.py:182-183`); `core.py:716-721` |
| `Replacement` (`:1337`) | `event: Ev; steps; scope: Ref=This(); cond=TRUE; once_per_turn=False; optional: bool=True; gate=NONE` | program registered (`registry.py:184`) but **no engine code executes it**; only `view._ability_gate` reads `gate` (`view.py:166`) |
| `NameAlias` (`:1350`) | `names: tuple[str, ...]` | `reg.aliases` (`registry.py:240-241`), `view.names_of` (`view.py:471-474`), link check (`view.py:134`) |
| `PlayModifier` (`:1357`) | `cond=TRUE; costs=(); level: int\|None=None; cost: int\|None=None; pair_filters=()` | `game.play_modifiers`/`_alt_play_options` (`game.py:586-630`), `game.py:914-916` (cost program) |
| `CardScript` (`:1386`) | `card_number: str; abilities=(); unit_abilities=(); source: str="compiled"; notes: str=""` | registry root. For Pilots, `unit_abilities` are gained by the paired Unit. |

`where` exists only on Constant/Triggered/Activated (`registry.py:287-290`, `view.py:157-160`).
`gate` exists on Constant/Triggered/Activated/Keyword/Replacement (`registry.py:293-296`);
`gate_filters` on all of those except Replacement (`view.py:163-168`). Gate check:
`view._gate_ok` (`view.py:141-154`).

---

## 4. Program model (`effects/program.py`)

A resolving effect is `(program_id, pc, vars)`. `Program(instrs: tuple[Instr, ...], label: str)`
(`program.py:90-93`). `Instr = d.Step | Jump | JumpIfNot | AskMay | JumpIfNotDid | ModeSelect |
LoopInit | LoopNext | SetDid | HoldRules` (`program.py:76-87`).

| Instr (line) | Fields | Handler semantics (`interp.py`) |
| --- | --- | --- |
| `Jump` (`:14`) | `target: int` | `pc = target`, JUMPED (`:182-184`) |
| `JumpIfNot` (`:19`) | `cond: Cond; target: int` | NEXT if `cond(..., did=f.did)` else jump (`:187-191`) |
| `AskMay` (`:25`) | `prompt: str; player: P; target_if_no: int` | decision `[YES, NO]`, kind BURST iff `prompt == "activate Burst?"` else YES_NO, prompt text `prompt or "you may"` (`:213-216`); NO sets `answer=False`, `did=False`, jumps (`:219-226`) |
| `JumpIfNotDid` (`:32`) | `target: int` | NEXT if `f.did` else jump (`:194-198`) |
| `ModeSelect` (`:37`) | `labels: tuple[str, ...]; targets: tuple[int, ...]; chooser: P; end: int` | options `SELECT(i)` for each mode whose mandatory targets exist (`targets_available`); none -> `did=False`, jump to `end`; prompt "choose mode", context `(("mode",1),)` (`:263-280`) |
| `LoopInit` (`:45`) | `key: str; ref: Ref\|None; times: Value\|None` | ForEach: `vars[key] = resolve(ref)`, `ints[key] = 0`; Repeat: `ints[key] = -max(0, value(times))` (`:282-290`) |
| `LoopNext` (`:54`) | `key: str; var: str; end: int` | if `var` (ForEach): bind `vars[var] = (items[i],)` or jump to `end`; else (Repeat): `i >= 0` -> jump, else `i += 1` (`:293-309`) |
| `SetDid` (`:64`) | `value: bool` | `f.did = value` (`:201-203`). **Never emitted by `flatten`.** |
| `HoldRules` (`:69`) | `delta: int` | `ints["__hold_rules"] += delta`, key deleted at 0; rules management is skipped while the key exists (`:206-210`, `:125-126`) |

### 4.1 `flatten(steps, label)` (`program.py:96-181`)

`_Builder.out` is a list; `emit` appends and returns the index; placeholders (`None`) are patched.
Rules, applied recursively in step order:

- `If(cond, then, otherwise)`: `j = emit(None)`; emit `then`; if `otherwise` is non-empty:
  `j_end = emit(None)`; `out[j] = JumpIfNot(cond, here)`; emit `otherwise`;
  `out[j_end] = Jump(here)`. Else `out[j] = JumpIfNot(cond, here)` (`:116-125`).
- `May(steps, prompt, player)`: `j = emit(None)`; emit steps; `out[j] = AskMay(prompt, player, here)`
  (`:126-129`).
- `Simultaneous(steps)`: `HoldRules(1)`, steps, `HoldRules(-1)` (`:130-133`).
- `IfYouDo(steps)`: `j = emit(None)`; steps; `out[j] = JumpIfNotDid(here)` (`:134-137`).
- `ChooseMode(options, chooser)`: `sel = emit(None)`; for each option: record `start = here`, emit
  body, emit placeholder end-jump; then `end = here`; every end-jump becomes `Jump(end)` (the last
  option also gets one); `out[sel] = ModeSelect(labels, starts, chooser, end)` (`:138-152`).
- `ForEach(ref, var, steps)`: `key = f"__loop{loop_n}"`, `loop_n += 1` (counter per program, in
  emission order); `LoopInit(key, ref, None)`; `head = emit(None)`; steps; `Jump(head)`;
  `out[head] = LoopNext(key, var, here)` (`:153-160`).
- `Repeat(times, steps)`: same with `LoopInit(key, None, times)` and `LoopNext(key, "", here)`
  (`:161-168`).
- Any other step is emitted as itself (`:169-170`).

Jump targets may equal `len(instrs)` (falls off the end, frame finishes). Example programs:

```
EB01-008#a0  [JumpIfNot(Exists)->7, AskMay('Development 1: ...?', you)->7, Choose, Exile,
              JumpIfNotDid->7, Choose, Recover]
GD03-115#a0  [Choose, JumpIfNot(Cmp)->4, Apply, Jump->5, Apply]
GD04-100#u0  [AskMay('', you)->5, LoopInit(__loop0, ref=None, times=EventAmount('amount')),
              LoopNext(__loop0, '', end=5), Apply, Jump->2]
GD05-102#a0  [ModeSelect(('return','recover'), (1,4), end=7), Choose, ReturnToHand, Jump->7,
              Choose, Recover, Jump->7]
EB01-023#a0  [HoldRules(1), CustomStep, JumpIfNot->5, AskMay('', active)->5, AddToHand, Arrange,
              CustomStep, JumpIfNot->10, AskMay('', standby)->10, AddToHand, Arrange, HoldRules(-1)]
```

Instruction counts across all 623 programs: Choose 507, JumpIfNot 212, Apply 209, Damage 105,
AskMay 91, Rest 83, JumpIfNotDid 66, AddToHand 53, Draw 49, CustomStep 42, DeployToken 38,
ReturnToHand 37, LookTop 34, Destroy 33, Exile 30, SetActive 30, ShieldToHand 29, Discard 26,
Recover 23, Jump 22, DeployCard 17, PlaceExResource 13, Mill 8, ToDeck 8, Pair 8, Arrange 7,
ModeSelect 6, BindVar 5, PlaceResource 5, HoldRules 4, ToTrash 4, DamageShieldArea 3,
ChangeAttackTarget 3, ActivateMain 3, ApplyPlayer 3, DelayedTrigger 3, StartBattle 3, Reveal 2,
PlayCard 2, PayCost 1, LoopInit 1, LoopNext 1, SetResourcesActive 1.

AskMay prompts in data: `''`, `'Development 1: exile 1 (G Generation) cards from your trash?'`,
`'Development 2: exile 2 (G Generation) cards from your trash?'`,
`'Development 2: exile 2 (G Generation) cards?'`,
`'Development 3: exile 3 (G Generation) cards from your trash?'`, `'activate Burst?'`,
`'exile 7 (Vulture) cards?'`, `'pair that card from your trash?'`, `'pay ①?'`,
`'place it into your trash?'`, `'return the blue Pilot?'`.
ModeSelect labels in data: `Launcher Strike Gundam`, `Sword Strike Gundam`,
`damage_friendly_and_enemy_lv5_or_higher`, `deploy 1`, `deploy 2`, `destroy_enemy_lv2_or_lower`,
`enemy_unit_ap_minus_3`, `option1`, `option2`, `recover`,
`rest_1_to_2_enemy_units_with_3_or_less_hp`, `return`.

### 4.2 Execution contract (for the Rust interpreter)

- `run_top_frame` (`interp.py:109-132`): loop while `pc < len`; after each instruction, set
  `f.acted = True` if status PUSHED, or NEXT with `f.did` and the instruction type not in
  `NON_ACTIONS` (`interp.py:41-54`: all program.py instrs + `Choose`, `ChooseMode`, `BindVar`);
  NEXT increments `pc`; JUMPED does not; check game over; run rules management unless
  `"__hold_rules"` is in `f.ints`; return on WAIT/PUSHED. At the end, pop and `finish_frame`.
- `finish_frame` (`interp.py:135-150`): `once_used.add(once_key)` only if `acted`; command/burst
  frames move a RESOLVING card to trash (+COMMAND_RESOLVED for commands); `activate_main_sub`
  bumps the parent `pc` and copies `did`; `play_sub` bumps the parent `pc`.
- `execute`/`resume` dispatch on the exact Python type (`_HANDLERS.get(type(ins))`,
  `interp.py:153-178`); an unknown type raises `EngineError`.
- Status enum (`interp.py:28-32`): `NEXT=0, JUMPED=1, WAIT=2, PUSHED=3`.
- System programs (`interp.py:1581-1594`): `deploy = (DeployCard(Var("card")),)`,
  `pair = (Pair(Var("pilot"), Var("unit")),)`,
  `hand_limit = (Discard(count=Sum((HandSize(), -10))),)` (`core.HAND_LIMIT = 10`). Registered
  **lazily on first use** with labels `system:deploy`, `system:pair`, `system:hand_limit`; none of
  their step tuples equals a card program, so they get fresh ids 623, 624, 625 in first-use order
  within the process.

---

## 5. Registry (`effects/registry.py`)

### 5.1 Data structures

`AbilityEntry` (`registry.py:23-33`, frozen): `aid: int; card_number: str; def_id: int;
index: int; unit_text: bool; ability: Ability; program_id: int (-1 = no steps);
cost_program_id: int = -1; trigger_event: Ev|None = None` (set only for Triggered).

`CardEntry` (`registry.py:36-54`, mutable): `def_id; card_number; script: CardScript|None;
error: str|None; own: tuple[int,...]=(); unit: tuple[int,...]=(); command_aid=-1; burst_aid=-1;
has_burst=False; timings: frozenset[str]; field_entries; hand_entries; trash_entries;
paired_entries; unit_entries: tuple[AbilityEntry,...]; gated: bool=False`.

`Registry` (`registry.py:68-147`): `db; programs: list[Program]; abilities: list[AbilityEntry];
cards: list[CardEntry]; continuous: list[Continuous]; filter_sets: list[tuple[Filter,...]];
_cont_index; _filter_index; _program_index` (equality-keyed dicts); `aliases: dict[int,
tuple[str,...]]; hp_reduction_possible; events_by_def: dict[int, frozenset[Ev]];
always_events: frozenset[Ev]; repair_program; breach_program; support_program;
support_cost_program (=-1, never assigned); _printed_kw` (lazy cache).

### 5.2 Id assignment (exact order)

`build_registry(db)` (`registry.py:323-340`):

1. `repair_program = program(REPAIR_STEPS, "keyword:Repair")` -> 0, where
   `REPAIR_STEPS = (Recover(This(), KwAmount(REPAIR)),)` (`:58`).
2. `breach_program = program(BREACH_STEPS, "keyword:Breach")` -> 1,
   `BREACH_STEPS = (DamageShieldArea(OPP, EventAmount("breach")),)` (`:59`).
3. `support_program = program(SUPPORT_STEPS, "keyword:Support")` -> 2,
   `SUPPORT_STEPS = (Rest(This()), Choose("t", Sel(FRIENDLY, BATTLE, (NotRef(This()),))),
   Apply(Var("t"), StatMod(ap=KwAmount(SUPPORT)), THIS_TURN), CustomStep("support_used"))`
   (`:60-65`).
4. For each `cdef` in `db` iteration order (= `def_id` order, `cards/db.py:286-287`; real cards
   sorted by card number, then synthetic tokens, then Unit variants): `script_for(cdef)`; on
   `UnimplementedCardError` store `error=str(exc)`; on ValueError/AssertionError/TypeError/KeyError/
   IndexError/AttributeError store `"binding error: {type}: {exc}"` (`:331-337`). Append the
   `CardEntry` (so `cards[i].def_id == i`).
5. `_build_entry` (`:223-284`), for `(unit_text=False, abilities)` then `(True, unit_abilities)`,
   for each `idx, a`:
   1. `steps = _ability_steps(a)` (`:179-186`): Activated -> `cost_steps(costs) + steps`;
      Burst -> `(May(steps, prompt="activate Burst?"),)`; Triggered/Command/Replacement ->
      `steps`; else None. If not None: `pid = program(steps, f"{card_number}#{'u' if unit_text
      else 'a'}{idx}")`.
   2. If PlayModifier with costs: `cost_pid = program(cost_steps(costs), f"{card_number}#alt{idx}")`.
   3. If NameAlias: `aliases[def_id] += names` (tuple concatenation in ability order).
   4. `aid = len(abilities)`; append the `AbilityEntry`.
   5. `command_aid`/`burst_aid` = the **last** own Command/Burst (assignment overwrites).
   6. For each DelayedTrigger body found by `_delayed_steps(a)` (DFS pre-order over
      `_ability_steps(a)`: DelayedTrigger appends its `steps` then recurses into them; If ->
      then, otherwise; May/IfYouDo/Simultaneous/ForEach/Repeat -> steps; ChooseMode -> each body,
      `:299-320`): `program(body, f"{card_number}#delayed")`.
6. Then entry lists (`:264-283`): `field_entries` = own with where FIELD or ANY;
   `hand_entries` = HAND or ANY; `trash_entries` = TRASH or ANY; `paired_entries` = ANY only;
   `unit_entries` = unit abilities with FIELD or ANY; `gated` = any own/unit ability with a gate.
   `timings` = frozenset of Triggered event values, `"activate_" + timing`, Command timing values,
   `"burst"` (`:189-200`).
7. `_index_events(reg)` (`:356-378`): `always = {TURN_END, DESTROYS_BY_BATTLE}` plus every
   `DelayedTrigger.trigger.event` in any script plus all trigger events of token cards;
   `events_by_def[def_id]` = events of all `Trigger` nodes in the script, plus (transitively) those
   of every card named by an `AbilityGrant` (`:381-397`); `hp_reduction_possible` = any `StatMod`
   whose `hp` is not a non-negative int.

Dedupe: `program(steps, label)` (`:115-122`) looks up `_program_index[steps]` by **structural
equality** of the step tuple; a hit returns the existing id and **keeps the first label**.
`cont_key(c)` (`:124-130`) and `filters_key(f)` (`:132-138`) dedupe the same way and append in
first-use order. Ability ids are never deduped (one per ability occurrence).

Runtime allocation (after build):

| Table | Call site | Order |
| --- | --- | --- |
| `programs` | `interp.system_program` (`interp.py:1589-1594`) | first use per process (`_SYSTEM_IDS` is module-global) |
| `programs` | `core._delayed_triggers` `R.program(dt.steps, ...)` (`core.py:392`) | always a dedupe hit on the build-time `#delayed` program |
| `continuous` | `interp.add_lasting` (`interp.py:1305`), `_h_delayed` (`interp.py:1549`) | first use per process, across all games in that process |
| `filter_sets` | `interp.add_lasting` (`interp.py:1314`) for non-empty `ApplyPlayer.filters` | same (no ApplyPlayer in current data has filters) |

`runner/digest.py:1-56` exists because of this: the state digest replaces those indices with
`content_id = sha256(repr(obj).encode()).hexdigest()[:16]` (`digest.py:26-27`).

Test helpers `override_script`/`restore_entry` (`registry.py:209-220`) replace a `CardEntry` in
place and re-run `_index_events`; they append new abilities/programs (never removed).
`printed_keywords(def_id)` (`:89-113`): sum of ungated `Keyword.amount`, ungated
`Activated.support`, and ungated, unconditional, `This`-scoped `Constant` `KeywordGrant` int
amounts; cached in `_printed_kw` (not invalidated by `override_script`).

### 5.3 Ids that reach outputs

- `aid`: once-per-turn keys `(aid, host, host_seq)` (`core.py:236-237`, `game.py:439,892`),
  `TriggerInst.ability_key = (def_id, aid, int(unit_text))` (`core.py:280`, burst `core.py:721`),
  `RuleInst.key = (aid, host)` (`view.py:436`), and the ACTIVATE action argument `b`
  (`game.py:880`), which is logged in game records (`runner/game.py:101-102`). **Rust must use
  the identical aid numbering.**
- `def_id`: everywhere in state.
- program ids, continuous keys and filter keys: replaced by content ids in the digest
  (`digest.py:40-48`), so their numeric values need not match, but their `repr` must (section 9).

---

## 6. Binding API and custom hooks

### 6.1 API (`bindings/__init__.py`)

- `card(*card_numbers)` (`:31-39`): registers `fn` in `_BINDINGS[n] = (fn.__module__, fn)`;
  re-registering the same number with a different function raises `DuplicateBindingError`.
  `BindingFn = Callable[[CardDef], CardScript]`. Bindings are pure data builders; many call
  `compile_parts(c)` / `route_abilities` to reuse compiled lines.
- `custom_step(name)` -> `interp.CUSTOM_STEPS`, signature
  `(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool` (`interp.py:35`).
- `custom_cond(name)` -> `view.CUSTOM_CONDS`, `(st, dv, ctx, params) -> bool` (`view.py:114`).
- `custom_filter(name)` -> `view.CUSTOM_FILTERS`, `(st, dv, ctx, uid, params) -> bool` (`view.py:112`).
- `custom_value(name)` -> `view.CUSTOM_VALUES`, `(st, dv, ctx, params) -> int` (`view.py:113`).
- Duplicate names with a different function raise `DuplicateBindingError` (`:46-47` etc.).
- Dispatch is by name string; `params` arrive as `dict(node.params)` (a fresh dict per call).
- `scripts.script_for` (`scripts.py:12-16`): `load_bindings().get(card_number)` wins; else
  `compile_card`. `binding_module(n)` returns the module path (used by golden files).
- `Ctx` (`view.py:27-49`): `controller, host, card_uid (defaults to host), vars, event`;
  `ctx.ev(key, default=-1)` returns the **first** matching payload key. `interp.ctx_of(f)` shares
  the frame's `vars` dict (`interp.py:57-58`), so hooks that write `f.vars` are visible to later
  steps.

### 6.2 Every hook (34: 13 steps, 7 conds, 9 filters, 5 values; all referenced by data)

Complexity: L = a few lines over existing engine primitives; M = needs a non-trivial engine
primitive (payment, destroy, lasting effects, events) with exact ordering.

| # | Name | Kind | File:line | Params (data) | Reads | Mutates / RNG | Used by | Cx |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `return_looked_bottom` | step | `builtins.py:13` | — | `f.vars["looked"]`, card zones | removes each looked card (still in DECK) from the owner's deck (`list.remove`), `st.rng.shuffle(looked)`, appends at the bottom (end of list); if more than 1 card, sets `known=0` on each; `st.touch()`; returns False if none | compiler template `grammar.py:1016` + 6 binding modules (51 occurrences) | L (RNG) |
| 2 | `deploy_ex_base` | step | `builtins.py:32` | — | controller's BASE | `trash_excess` every current Base (`interp.py:904-910`), `core.new_card(ex_base def, BASE)`, `record("deployed", p, p, uid)`, `emit(DEPLOYED, uid, player=p, by=p)`; True | `grammar.py:1224` (GD04-110) | M |
| 3 | `is_attack_target` | filter | `builtins.py:46` | — | `st.battle` | — (live battle and `target == uid`) | `nouns.py:336` (EB01-033) | L |
| 4 | `attack_target_damaged` | cond | `builtins.py:54` | — | battle, target damage | — (live battle, `target >= 0`, `damage > 0`) | `grammar.py:338` (GD04-044) | L |
| 5 | `support_used` | step | `builtins.py:65` | — | `f.vars["t"]` | `emit(SUPPORT_USED, f.host, player=controller, target=t[0])`; False if no target | keyword Support program (`registry.py:64`) | L |
| 6 | `wp_eb01_a_look_own_top` | step | `wp_eb01_a.py:61` | `player` (P value: "active"/"standby"), `var` | that player's deck top | `known \|= 1 << player` on the top card; `f.vars[var] = top[:1]`; returns `bool(top)` | EB01-023 | L |
| 7 | `wp_eb01_b_look_top_own` | step | `wp_eb01_b.py:240` | `player` ("opponent"), `var` ("top1") | same as #6 | same as #6 (identical body) | EB01-078 | L |
| 8 | `wp_gd01_b_destroyed_with_damage` | cond | `wp_gd01_b.py:117` | — | event `battle`, `by` | — : `ev("battle",0) == 1 or ev("by") == -1` | GD01-094 | L |
| 9 | `wp_gd02_b_pay_deploy_cost` | step | `wp_gd02_b.py:54` | `var` ("t1") | first card of `vars[var]` still in TRASH | `cost = max(0, cost_of + dv.cost_mod.get(uid,0))`; `pay_generic(controller, cost)`; if paid, `cost > 0` and `host >= 0`: `record("cost_paid", c, c, host)`, `emit(COST_PAID, host, player=c, amount=cost)` | GD02-096, GD02-110 | M |
| 10 | `wp_gd03_a_ap_per` | value | `wp_gd03_a.py:52` | `per` (4) | host AP | — : `ap_of(host) // per`, 0 if no host | GD03-033 | L |
| 11 | `wp_gd03_a_has_destroyed_effect` | filter | `wp_gd03_a.py:62` | — | `dv.abilities[uid]` | — : any Triggered with `event is DESTROYED and self_only` | GD03-037 | L |
| 12 | `wp_gd03_a_lowest_hp` | filter | `wp_gd03_a.py:79` | — | owner's BATTLE | — : uid in BATTLE and its REMAINING_HP equals the minimum | GD03-049 | L |
| 13 | `wp_gd03_a_pay_cost_of` | step | `wp_gd03_a.py:92` | `var` ("t1") | first card of `vars[var]` (no zone check) | `interp.execute(st, f, PayCost(max(0, cost_of + cost_mod)))` (records/emits as `_h_pay_cost`); returns `f.did` | GD03-051 | M |
| 14 | `wp_gd03_a_ap_minus_count` | step | `wp_gd03_a.py:111` | `var` ("t1"), `sel` (Sel node) | `len(select(sel))` | `interp.execute(st, f, Apply(Var(var), StatMod(ap=-n), THIS_TURN))` (creates a continuous key at runtime, emits AP_REDUCED); False if `n <= 0` or var empty | GD03-071 | M |
| 15 | `wp_gd03_b_cond_holds` | filter | `wp_gd03_b.py:64` | `cond` (a Cond node, `Cmp` in data) | — | — : `V.cond(st, dv, ctx, cond)` (card-independent) | GD03-109, GD03-114 | L |
| 16 | `wp_gd03_b_battling_enemy_unit` | filter | `wp_gd03_b.py:72` | — | battle | — : uid is attacker/target, the other side `>= 0`, has a different owner, is in BATTLE and is a Unit type | GD03-102 | L |
| 17 | `wp_gd03_b_unique_names` | value | `wp_gd03_b.py:96` | `sel` (Sel node) | names of selected cards | — : number of distinct `CardDef.name` strings | GD03-089 | L |
| 18 | `wp_gd04_a_pilot_capable` | filter | `wp_gd04_a.py:50` | — | card def | — : `is_pilot_capable` (`cards/model.py:134-137`) | GD04-021 | L |
| 19 | `wp_gd04_a_has_printed_keyword` | filter | `wp_gd04_a.py:62` | — | `printed_keywords` | — : dict non-empty (a 0-amount keyword counts) | GD04-067 | L |
| 20 | `wp_gd04_a_copy_keywords` | step | `wp_gd04_a.py:69` | `var` ("t1") | `printed_keywords(chosen[0])`, host zone | for `kw, amount in sorted(kws.items())`: `add_lasting(KeywordGrant(kw, amount if stacking else 0), controller, host, ((host, host.zone_seq),), THIS_TURN)`; False if no choice or host not in BATTLE; returns `bool(kws)` | GD04-067 | M |
| 21 | `wp_gd04_a_ap_down_by_named_trash_units` | step | `wp_gd04_a.py:84` | `name` ("Gundam Virtue"), `var` ("t1") | count of friendly trash Units with `name` in their name | `add_lasting(StatMod(ap=-n), ...)` on `vars[var]` cards in BATTLE; then one `next_group`, `emit(AP_REDUCED, u, player=owner, by=controller, group=g)` for each whose AP dropped | GD04-057 | M |
| 22 | `wp_gd04_a_has_partner_in_trash` | filter | `wp_gd04_a.py:112` | `trait` ("UN") | owner's TRASH | — : another card with `trait` in printed traits | GD04-071 | L |
| 23 | `wp_gd04_b_played_with_ex` | cond | `wp_gd04_b.py:52` | — | `st.history` | — : last record `kind=="play"` with `uid == ctx.card_uid`; True iff the next record is `"played_with_ex"` for the same uid | GD04-106, GD04-108 | L |
| 24 | `wp_gd05_a_lowest_enemy_unit_level` | value | `wp_gd05_a.py:97` | — | enemy BATTLE | — : `min(level_of, default=0)` | GD05-002 | L |
| 25 | `wp_gd05_a_once_free` | cond | `wp_gd05_a.py:110` | `tag` (-205006) | `st.once_used` | — : `host >= 0` and `(tag, host, zone_seq) not in once_used` | GD05-006 | L |
| 26 | `wp_gd05_a_use_once` | step | `wp_gd05_a.py:116` | `tag` (-205006) | — | `once_used.add((tag, host, zone_seq))`; True | GD05-006 | L |
| 27 | `wp_gd05_a_destroyed_by_effect` | cond | `wp_gd05_a.py:122` | optional `by` ("you") | event `by`, `battle` | — : False if `battle != 0` or `by < 0`; else `params.get("by") != "you" or by == controller` | GD05-053, GD05-054 | L |
| 28 | `wp_gd05_b_destroy_first_shield_area_cards` | step | `wp_gd05_b.py:70` | `count` (2) | opponent BASE/SHIELD | computes `base = BASE[:count]` and `shields = SHIELD[:count-len(base)]` **before** destroying; `core.destroy(base, battle=False, by=c, source=host)`; `core.destroy_shields(p, shields, battle=False, source=host, by=c)` | GD05-107 | M |
| 29 | `wp_gd05_b_resolving_command_to_trash` | step | `wp_gd05_b.py:90` | — | `f.card_uid` zone | if RESOLVING: `core.move(uid, TRASH)`; if `f.kind == "command"`: `emit(COMMAND_RESOLVED, uid, player=c)`; True | GD05-112/113/121/122 | L |
| 30 | `wp_gd05_b_can_pair` | filter | `wp_gd05_b.py:106` | — | — | — : `interp.can_pair` (`interp.py:1340-1344`) | GD05-112/113/121/122 | L |
| 31 | `wp_gd05_b_played_with_ex` | cond | `wp_gd05_b.py:115` | — | `st.history` | — : scan newest to oldest records with `uid == card_uid`: first `"played_with_ex"` -> True, first `"play"` -> False; else False | GD05-118 | L |
| 32 | `wp_st05_08_highest_enemy_unit_lv` | value | `wp_st05_08.py:46` | — | enemy BATTLE | — : `max(level_of, default=0)` | ST08-001 | L |
| 33 | `wp_st12_14_destroyed_by_effect_damage` | cond | `wp_st12_14.py:83` | — | event, `st.frames[-1]`, its program | — : False if `ev("battle",0)` truthy, `ev("by") >= 0`, or no frames; else `0 < f.pc <= len(instrs) and isinstance(instrs[f.pc-1], Damage)` | ST13-012 | L (layout-dependent) |
| 34 | `wp_st12_14_lowest_rested_enemy_lv` | value | `wp_st12_14.py:96` | — | rested enemy BATTLE Units | — : `min(level_of, default=-1)` | ST14-001 | L |

Notes:

- Every custom step runs inside `_h_custom` (`interp.py:1571-1575`): `f.did = fn(...)`, then
  `st.touch()` unconditionally, then NEXT. A custom step never waits (`interp.py:4-5`).
- `wp_gd03_a_pay_cost_of` and `wp_gd03_a_ap_minus_count` re-enter the interpreter with a
  synthesized node via `interp.execute` (dispatch by type), so the Rust hook must call the same
  handler logic (including events, groups and lasting serials).
- RNG consumption among hooks: only `return_looked_bottom` (`builtins.py:23`), Fisher-Yates
  `for i in range(n-1, 0, -1): j = randrange(i+1)` (`rng.py:43-46`; `randrange` rejection
  sampling `rng.py:30-38`), i.e. `n-1` randrange calls for `n` cards (0 when `n <= 1`).
- Other RNG points in DSL steps (for completeness): `Choose(random=True)` (`interp.py:424-427`),
  `Discard(random=True)` (`interp.py:533-534`), `ToDeck(shuffle=True)` (`interp.py:796-799`, owners
  sorted ascending), `Shuffle` (`interp.py:1228-1231`), all via `core.shuffle_deck`
  (`core.py:134-139`) or `randrange`.

---

## 7. Text normalization (`effects/text.py`) — compile time and golden only

`text_hash(text) = sha256(normalize(text).encode("utf-8")).hexdigest()[:16]` (`text.py:94-95`).
`normalize` (`text.py:67-91`), exact order:

1. Replace `・` (U+30FB) -> `･` (U+FF65), `\r\n` -> `\n`, `’` -> `'`, `“` -> `"`, `”` -> `"`.
2. Replace `【Activate ･Main】`, `【Activate ･Action】`, `【Activate･ Main】`, `【Activate･ Action】`
   with the unspaced forms.
3. Replace `：` (U+FF1A) -> `:`.
4. `re.sub(r"(\w)' (s|t|re|ll|ve|d)\b", r"\1'\2", t)` (Unicode `\w`, `\b`).
5. `[Suppression]` -> `<Suppression>`.
6. `strip_reminders` (`text.py:30-64`): at `(` not preceded by `]`, find the matching `)` by depth;
   drop the group iff matched and `_is_reminder(inner)` (`text.py:17-27`: empty -> keep;
   `(...)` with one `(` -> keep; matches `[^.()]*` and no word `this|the|a|an|when|while|to|of|your`
   (case-insensitive) -> keep (trait); else drop if it ends with `.` or `.)` or has >= 4
   whitespace-separated words). At `[`, a token definition matching
   `\[[^\[\]]+\]\(\((?:[^()]|\([^()]*\))*\)` is copied verbatim.
7. Split on `\n`; per line collapse `[ \t　]+` to one space, `strip()`; drop empty lines;
   join with `\n`.

`split_abilities` (`text.py:111-135`): `""`/`"-"` -> no abilities; a line starting with `■` is
appended to the previous line with `\n`; lines equal to `-` are skipped; leading `【...】` markers
are peeled repeatedly (`^【([^】]+)】`, marker stripped; a following `/` is dropped).

## 8. Compiler overview (`effects/compiler/`)

Pipeline: `compile_card(cdef)` (`compiler/__init__.py:53-67`) -> `compile_parts` (`:19-29`: skip
RESOURCE/EX RESOURCE/EX BASE; per `RawAbility`, catch `CompileError`) -> `compile_ability`
(`abilities.py:32-42`: `【Pilot】` lines produce nothing; marker-less keyword-only lines via
`parse_keyword_line`, regex `abilities.py:16-18`; else `grammar.compile_marked`,
`grammar.py:2188-2211`: handles `Once per Turn`, `During Pair[･qualifier]`, `During Link`, then
`_compile_timed` per timing marker `grammar.py:2214+` or `compile_untagged`). Any `CompileError`
makes the whole card raise `UnimplementedCardError("; ".join(f"{line!r}: {err}"))`
(`compiler/__init__.py:59-65`). `route_abilities` (`:32-50`) sends a Pilot's Burst/NameAlias/
PlayModifier and HAND/TRASH abilities to `abilities`, everything else to `unit_abilities`.
`grammar.py` is 2300 lines with ~154 functions (sentence templates, triggers, costs, constants,
durations) and ~124 lines using `re`; `nouns.py` (364 lines) parses selectors (`parse_selector`,
`nouns.py:142`). The compiler is deterministic text processing over sorted data; its output is
fully captured by the registry, so a Rust port that loads an exported IR does not need it.

## 9. Golden files (`tests/effects/golden/`)

- 27 files, one per card-number prefix `card_number.split("-", 1)[0]` (`golden.py:54-55`): EB01,
  EXB, EXBP, EXR, EXRP, GD01-GD05, R, RP, ST01-ST14, T; 884 KB total; 1152 records (real cards
  only, `tools/golden.py:25`; the synthetic Unit variant is excluded).
- File text: `json.dumps(data, indent=1, sort_keys=True, ensure_ascii=False) + "\n"`, UTF-8
  (`tools/golden.py:35-36,53`). With `indent`, separators are `","` and `": "`.
- Top level: `{card_number: record}`. Record (`golden.py:40-51`): `name`, `text_hash`; if no
  script: `error`; else `source` (`"binding"` if a binding module exists, else
  `script.source`), `binding` (module short name, e.g. `wp_gd01_a`) when bound, `abilities`
  (always present, possibly `[]`), `unit_abilities` only when non-empty. Key sets observed:
  `{abilities,name,source,text_hash}` 773, `+binding` 252, `+unit_abilities` 77,
  `+binding+unit_abilities` 50.
- `node_to_json` (`golden.py:19-37`): dataclass -> `{"_": ClassName, field: value, ...}` omitting
  every field whose value `==` its default (`f.default` or `f.default_factory()`); enum -> `.value`;
  tuple/list -> list; dict -> keys sorted, `str(k)`; else unchanged. Pair tuples (params,
  `ChooseMode.options`, `DeployToken.also`) become 2-element lists. Example:
  `Draw()` -> `{"_": "Draw"}`; `Sel(FRIENDLY, BATTLE, (UNIT,))` ->
  `{"_":"Sel","filters":[{"_":"IsKind","kinds":["unit"]}],"loc":"battle","side":"friendly"}`.
- `text_hash` of an empty normalized text is `e3b0c44298fc1c14` (EX Base records).
- Check: `uv run python -m gcg_sim.tools.golden --check`; `tests/effects/test_golden.py` compares
  the exact text.

The golden format is **not** sufficient as an execution IR: it elides defaults (needs the Rust
side to know every default), and it lacks aids, program ids/labels, dedupe, entry lists, the
token map and the synthetic Unit variant.

---

## 10. Assessment: exporting the Registry as a versioned JSON IR

### 10.1 Serializability (verified by walking every node of every script and program)

- Leaf types found: `int` 3135, `bool` 4894, `str` 2216, `None` 2711, enum members of all 13 DSL
  enums plus `Duration`. **No floats, callables, lambdas, closures, regexes, sets, dicts, or
  object-identity-bearing values** inside any node. Every enum-typed field holds an enum member
  (no plain strings); no `bool` sits in a `Value` position.
- `params: tuple[tuple[str, object], ...]` values observed: `int` (`per`, `count`, `tag`), `str`
  (`player`, `var`, `name`, `trait`, `by`), a `Cond` node (`Cmp`, in `wp_gd03_b_cond_holds`), and a
  `Sel` node (`wp_gd03_a_ap_minus_count`, `wp_gd03_b_unique_names`). All serializable as
  `[key, value]` with value = number | string | node.
- `default_factory` lambdas (`KwAmount.ref`, `ActivateMain.ref`, `Constant.scope`,
  `Replacement.scope`) only produce `This()`/`ThisCard()`; a full (non-elided) export needs no
  defaults at all.
- Identity: `d.TRUE` is compared with `is` (`core.py:349`, `game.py:443`, `game.py:622`,
  `view.py:432`) only as a fast path; `And(())` evaluates to True anyway, so equality semantics
  suffice.
- Strings: 862 distinct (scripts, programs, labels); non-ASCII only `Ⅱ` (U+2161) and `①`
  (U+2460), both printable; none contains `"`, a backslash or a control character; several token
  keys contain `'` (e.g. `"Char's Zaku Ⅱ|Zeon|3|1|"`).
- A naive JSON round trip (enums decoded as plain strings) compares `==` equal to the originals
  for all 623 programs because `StrEnum == str`; **`repr` would differ**, so the Rust side must
  type enum fields.

Conclusion: the whole registry is plain data and serializes losslessly.

### 10.2 Proposed IR shape

```
{
  "ir_version": 1,
  "source": {"git": "<sha>", "python": "3.15.1", "sources_hash": "<tools.sources hash>"},
  "programs":  [{"label": "keyword:Repair", "content_id": "b3df0ca881a27647",
                 "instrs": [<node>, ...]}, ...],          // index = program_id, 0..622
  "system_programs": {"deploy": [<node>], "pair": [<node>], "hand_limit": [<node>]},
  "abilities": [{"aid", "def_id", "card_number", "index", "unit_text", "program_id",
                 "cost_program_id", "trigger_event", "ability": <node>}],   // index = aid
  "cards": [{"def_id", "card_number", "source", "own", "unit", "command_aid", "burst_aid",
             "has_burst", "timings", "field", "hand", "trash", "paired", "unit_entries",
             "gated", "events"}],                          // index = def_id; entry lists as aids
  "aliases": {"<def_id>": ["Char Aznable"]},
  "always_events": ["destroys_by_battle", "turn_end"],
  "hp_reduction_possible": false,
  "keyword_programs": {"repair": 0, "breach": 1, "support": 2},
  "token_keys": {"<token_key>": <def_id>},                 // CardDB._token_by_key
  "unit_variants": {"<def_id>": <variant def_id>},
  "hooks": {"step": [...13 names], "cond": [...7], "filter": [...9], "value": [...5]}
}
```

Node encoding: `{"_": "<ClassName>", <every field in declaration order>}`; enum -> its value
string (decoded by field type); tuple -> array; pair tuples -> 2-element arrays; `None` -> null;
`Value` = JSON integer or node object (distinguished by JSON type). Hooks are referenced by their
`name` string exactly as in the DSL; the Rust loader should fail fast on any name not in its native
hook table. Emit with `json.dumps(sort_keys=True, separators=(",", ":"), ensure_ascii=False)` for a
stable, diffable artifact, and record its sha256.

### 10.3 Size (measured, full form, no elision)

| Part | Bytes (compact JSON) |
| --- | --- |
| programs (623, with labels and content ids) | 336,578 |
| abilities (1260, ability node without `steps`) | 414,513 |
| cards (1153) | 288,041 |
| other tables | < 200 |
| **Total** | **1,039,370 (~1.0 MB); gzip -9: 67,436 bytes** |

Golden-style default elision would cut scripts from 725 KB to 408 KB and programs from 317 KB to
219 KB; hash-consing shared subtrees would shrink it further. Load time is negligible either way.

### 10.4 What must still be ported by hand

1. The interpreter handlers and evaluators (`interp.py`, `view.py`, `core.py`) — they give the
   nodes meaning; the IR only carries data.
2. The 34 custom hooks (section 6.2): 26 are a few lines (L), 8 need engine primitives with exact
   event/serial ordering (M). Total roughly 400 lines of Rust.
3. `flatten` is not needed if programs are exported, but the Rust interpreter must keep the exact
   flattened layout because three things read it: `choice_group` adjacency (`interp.py:391-404`),
   `targets_available` (`interp.py:235-260`) and hook #33 (`instrs[pc-1]`).
4. The three system programs (or load them from `system_programs`), registered lazily or eagerly:
   their numeric ids are canonicalized in the digest, but their `content_id` includes the label
   `system:<name>`.
5. Python-compatible `repr` for DSL and program nodes, required by `state_digest`
   (`digest.py:26-27`), which feeds `final_state_sha256` in game records (`runner/game.py:162`,
   `runner/records.py:128`) and replays (`runner/replayfile.py:185-190`). Build-time programs can
   use the exported `content_id`, but continuous effects are created at runtime and need a
   repr: frozen `StatMod(ap=<int>, hp=<int>)`, `KeywordGrant(keyword=<Kw>, amount=<int>)`,
   `AbilityGrant(card_number='<str>', ability_index=<int>)`, hook-created `StatMod(ap=-n, hp=0)`,
   plus any `Apply`/`ApplyPlayer` effect node as-is (RuleGrant/TraitGrant/CostMod with nested
   filters and Values) and `ApplyPlayer.filters` tuples. Recommend implementing a generic repr over
   the typed Rust nodes and verifying it against the exported `content_id` of all 623 programs.
   Format rules (verified on 3.15.1):
   - dataclass: `ClassName(f1=<repr>, f2=<repr>)`, all fields in declaration order, `", "`
     separator; no fields -> `This()`.
   - enum: `<Kw.REPAIR: 'Repair'>` (class name, member name, value repr).
   - str: Python `repr` quoting: single quotes, unless the string contains `'` and no `"`, then
     double quotes (`"Char's Zaku Ⅱ|Zeon|3|1|"`); printable non-ASCII as-is; escapes for `\\`,
     `\n`, `\r`, `\t`, other non-printables (none occur today).
   - tuple: `()`, `(x,)`, `(x, y)`; `None`, `True`, `False`; ints in decimal with `-`.
   - Examples: `Program(instrs=(Recover(ref=This(), amount=KwAmount(keyword=<Kw.REPAIR: 'Repair'>,
     ref=This())),), label='keyword:Repair')` -> `b3df0ca881a27647`;
     `StatMod(ap=-2, hp=0)`; `CustomCond(name='wp_gd05_a_once_free', params=(('tag', -205006),))`.
   - The digest blob is `repr(obj).encode()` (UTF-8), sha256, first 16 hex chars.
6. `token_spec` parsing of `DeployToken.token_key` (`interp.py:1018-1024`) and
   `CardDB.token_for` — or use the exported `token_keys` map.
7. The compiler, golden writer and `text.py` only if Rust must regenerate the IR itself; otherwise
   keep Python as the IR producer and treat the IR as a versioned build artifact.

---

## 11. Hazards (Python-semantic traps in this area)

1. **Ability ids are observable.** aids appear in once keys, `TriggerInst.ability_key`,
   `RuleInst.key` and the logged ACTIVATE action (`core.py:236-237,280,721`, `view.py:436`,
   `game.py:880`, `runner/game.py:101-102`). Do not renumber; load them.
2. **First-use, process-global ids.** `continuous`/`filter_sets` (`registry.py:124-138` from
   `interp.py:1305,1314,1549`) and system programs (`interp.py:1586-1594`) are numbered in the
   order a process first needs them, across games. Raw `effect_key`/`program_id` values therefore
   differ between processes; only the canonical digest is stable (`digest.py:1-8`). The AI sorts
   tuples that start with `effect_key` (`ai/actions.py:312-329`); this is only used for equality,
   so any injective numbering is equivalent.
3. **Digest depends on `repr()`** of DSL nodes and `Program` (`digest.py:26-27`), including the
   **label of the first registrant** of a deduplicated program (`registry.py:116-121`; 364
   abilities run a program labelled with another card's number).
4. **Structural-equality dedupe** (`registry.py:116,125,133`): Python `==` treats `True == 1`,
   `0 == False` and `StrEnum.X == "x"` as equal and requires the same dataclass class. A Rust
   `PartialEq` on typed nodes matches today's data (no bools in Value slots, all enum fields typed),
   but would diverge if such mixing appeared.
5. **`is` identity on `d.TRUE`** (`core.py:349`, `game.py:443,622`, `view.py:432`): fast path only;
   semantics equal to evaluating `And(())`.
6. `view.value` accepts `bool` as `int` (`view.py:854-855`); `MinOf(())` raises (`view.py:877`);
   `Sum(())` is 0 (`view.py:873`). Python ints are unbounded; values here are small (i64 is safe).
7. `min(..., default=0)`, `max(..., default=0)`, `min(..., default=-1)` in hooks
   (`wp_gd05_a.py:103`, `wp_st05_08.py:51`, `wp_st12_14.py:102`); `lowest_hp` compares values only,
   so the first-minimum rule does not matter (`wp_gd03_a.py:88-89`).
8. Floor division `ap_of(host) // per` (`wp_gd03_a.py:59`): operands are non-negative
   (`view.py:477-494`), but use `div_euclid` to be safe.
9. `sorted(kws.items())` sorts by `Kw` string value (`wp_gd04_a.py:78`): Blocker, Breach,
   First Strike, High-Maneuver, Repair, Support, Suppression. This fixes the lasting-effect serials
   and the cont_key allocation order.
10. Binding modules import in `sorted(pkgutil.iter_modules)` order (`bindings/__init__.py:92`);
    this only affects duplicate-binding errors and hook-dict insertion order (lookup only).
11. `dict(params)` per call (`interp.py:1573`, `view.py:781,882,933`): duplicate keys -> last wins.
12. `LoopNext` decides ForEach vs Repeat by `if ins.var:` (`interp.py:295`); a `ForEach(var="")`
    would run as a Repeat.
13. `Trigger.whose_turn`: only `P.YOU` means "host owner's turn"; every other value means the
    opponent's turn (`core.py:203`). `Trigger.subject.loc`/`top_n` are ignored (`core.py:163-184`).
14. `_loc_cards` returns the live zone lists (`view.py:595-623`); `select` builds a new list, but
    `SHIELD_AREA` and `FIELD_UNITS_AND_BASES` concatenate BASE+SHIELD and BATTLE+BASE in that order.
15. `HasKeyword` reads derived keywords for BATTLE/BASE cards and `printed_keywords` elsewhere
    (`view.py:727-730`); `printed_keywords` is cached and not invalidated by `override_script`
    (`registry.py:93-113,209-215`).
16. Event payloads: `(("subject",s),("player",p),("group",g), *sorted(extra.items()))`
    (`core.py:331-333`); `Ctx.ev` returns the first match (`view.py:45-49`).
17. Program-layout dependence: `choice_group` (`interp.py:391-404`), `targets_available`
    (`interp.py:235-260`), hook `wp_st12_14_destroyed_by_effect_damage` (`wp_st12_14.py:91-93`).
18. Delayed triggers are identified by `(card_number, n)` where `n` counts DelayedTrigger nodes in
    DFS order over `getattr(ability, "steps", ())` of `own + unit` abilities, matched by `==`
    (`interp.py:1528-1538`, `core.py:411-438`); two identical DelayedTriggers on one card collapse
    to the first index. Delayed triggers and `AbilityGrant`s of synthetic card numbers (`TOKEN:`,
    `UNIT:`) are silently ignored because `db.get` returns None (`core.py:374-376`,
    `view.py:399-401`); none exist today.
19. `AbilityGrant.ability_index` indexes `[*own, *unit]` (`view.py:403`), not own abilities only.
20. `cost_steps` variable names use the index over all costs (`registry.py:157-158`); the name is
    visible in the prompt `f"choose {var}"` (`interp.py:439,467`).
21. `"activate Burst?"` is defined twice (`registry.py:150`, `interp.py:25`); `AskMay` becomes a
    BURST decision by string equality (`interp.py:215`).
22. `command_aid`/`burst_aid` keep the last matching ability (`registry.py:257-261`).
23. `_h_custom` always calls `st.touch()` and overwrites `f.did` with the hook result
    (`interp.py:1571-1575`).
24. `return_looked_bottom` uses `list.remove` (first occurrence) and resets `known` only when more
    than one card is returned (`builtins.py:21-27`); RNG consumption is `n-1` randrange calls.
25. `wp_gd05_b_destroy_first_shield_area_cards` slices both lists before destroying anything
    (`wp_gd05_b.py:79-80`).
26. Two different "played with EX" conditions (`wp_gd04_b.py:52-62` vs `wp_gd05_b.py:115-124`)
    scan history differently; port both verbatim.
27. Dead or unreachable members: `SetDid` never emitted (`program.py:64-66`); `Replacement` never
    executed (only registered, `registry.py:184`); RuleKinds MUST_ATTACK, PLAY_COST_DELTA,
    PLAY_LEVEL_DELTA, ATTACK_TARGET_FIXED, CUSTOM have no engine consumer; `support_cost_program`
    is always -1 (`registry.py:86`); `DeployCard.pay_cost/ignore_level`, `PlayCard.ignore_level`
    and `StartBattle.damage_only` are not read anywhere in `src/` outside the compiler (grep), but
    they are part of `repr` and therefore of program content ids.
28. Exceptions as control flow at build time: `build_registry` catches six exception types
    (`registry.py:331-337`) and `compile_parts` catches `CompileError` (`compiler/__init__.py:25-28`);
    error strings embed `repr(line)`. Zero errors today.
29. Unordered sets: `CardEntry.timings`, `events_by_def`, `always_events`, `_trigger_events`'
    working sets (`registry.py:189-200,356-397`) are only used for membership
    (`core.py:302-309`, `view.py:762`); safe. The `_program_index/_cont_index/_filter_index` dicts
    hash nodes (string hashes are randomized per process) but are never iterated.
30. LOC_CODES depend on `Loc` declaration order (`view.py:936`).
31. Golden elision uses `v == default` (`golden.py:27`); output uses `indent=1, sort_keys=True,
    ensure_ascii=False` (`tools/golden.py:35-36`).
32. Text normalization relies on Python `str.strip()`/`split()` (Unicode whitespace, including
    U+001C-U+001F that Rust's `char::is_whitespace` excludes) and Unicode `\w`/`\b`
    (`text.py:14,83,88`); relevant only if Rust reimplements `text_hash` or the compiler.
33. No float arithmetic, `sum()` of floats, `round()`, float formatting, `hash()`-based ordering,
    `id()` ordering or mutable default arguments exist in this area.

---

## 12. Open questions

1. Will the Rust engine load a Python-exported IR (recommended) or must it also compile card text
   and reproduce golden files natively? This decides whether `text.py` and the 2773-line compiler
   need porting.
2. The IR must be regenerated whenever data or bindings change; where it lives (committed
   artifact vs build step) and how its hash is checked against `tools.sources` is not decided.
3. Do any outputs besides `final_state_sha256` serialize raw `program_id`/`effect_key`/
   `filters_key` values (for example a debug state dump in replays)? I found only digest use in
   `runner/game.py:162` and `runner/replayfile.py:185`; the state/runner inventory should confirm.
4. If the Rust port registers system programs eagerly (fixed ids 623-625) instead of lazily, AI
   caches keyed by `(program_id, pc)` (`ai/actions.py:180`, `ai/targeting.py:65,80`) see
   different keys. I believe this cannot change behaviour (pure memoization), but the AI
   inventory should confirm.
5. Python `repr` escaping rules for strings with control characters, backslashes or both quote
   kinds are not exercised by current data; a Rust repr implementation should either implement
   CPython's full `unicode_repr` rules or reject such strings at IR load time.
