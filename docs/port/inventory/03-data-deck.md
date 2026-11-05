# 03 — Card data, card DB, tokens, decks, B&R list, `data status`

Port inventory for the Rust rewrite. Scope: `src/gcg_sim/cards/*`, `src/gcg_sim/deck/*`, the
packaged data under `src/gcg_sim/data/`, the repo-root `data/` folder, and the parts of
`cli.py` / `reports/versions.py` / `sources/conflicts.py` / `rules/index.py` that the CLI and
reports use at runtime. Every statement is taken from the code (path:line) or from running it
(Python 3.15.1, `unicodedata.unidata_version == "17.0.0"`, branch `rust-port`, HEAD `d9d41af`).

Why it matters for byte-identical replays: the **def_id** of every card definition, the
**sorted order of `Deck.main`** (the order the engine shuffles), the **token key strings**, and
every user-visible message come from this layer.

---

## 0. What is read at runtime

| File (under `src/gcg_sim/data/` unless noted) | Read at runtime by | Used for |
| --- | --- | --- |
| `gcgapi/cards.ndjson` | `cards/db.py:58` | every printing → card DB |
| `overrides.json` | `cards/db.py:65`, `cli.py:252`, `reports/versions.py:129` | canonical printing + field overrides; `data status` count; `conflict_resolutions` |
| `curated_conflicts.json` | `cli.py:254`, `reports/versions.py:85` | `data status` count; card numbers per conflict id for `conflict_resolutions` |
| `gcgapi/manifest.json` | `reports/versions.py:57,79` | `versions()`, `data status` snapshot block |
| `official/deck_construction.json` | `deck/official.py:76`, `reports/versions.py:69` | deck sizes, copy limit, types, colors; `deck_rules_effective_date` |
| `official/banlist.json` | `deck/official.py:114`, `reports/versions.py:68` | B&R list; `banlist_effective_date` |
| `official/rules_version.json` | `reports/versions.py:58` | `rules_version` (`latest_version`), `rules_effective_date` |
| `official/bo3_match_rules.json` | `reports/versions.py:72` | only `effective_date` (`bo3_rules_effective_date`) |
| everything under `src/gcg_sim/` (code + data) | `reports/versions.py:43-52` | `code_digest` (SHA-256 over all package files, §7.2) |

**Not read at runtime by the CLI, engine or reports** (only by dev tools `tools/*`,
`sources/conflicts.py`, `rules/index.py`, tests): `gcgapi/{errata.json, products.json,
rulings.json, rules-faq.json, sets/en/index.json, schema.sql, LICENSE-DATA}`,
`official/{edition_language.json, floor_rules.json}`, `rules/*` (CR markdown,
`rules_index.json`, `rules_na.json`), and the whole repo-root `data/` folder
(`SOURCES.lock.json`, `conflicts.json`, `sources.d/`, `official_raw/`). They still feed
`code_digest` when they live inside `src/gcg_sim/` (root `data/` does not).
`sources/conflicts.py` imports no other `gcg_sim` module (its docstring, `sources/conflicts.py:3-6`)
and nothing in `cli.py`/`reports/`/`engine/` imports `gcg_sim.sources` or `gcg_sim.rules`
(grep: only `tools/traceability.py:16` and tests import `gcg_sim.rules.index`).
`products.json` is a product (booster/starter) catalogue, not a printing alias table; the
"product id" of a printing is the `product_id` field of `cards.ndjson`.

Data are read with `importlib.resources.files("gcg_sim.data").joinpath(...).read_text(encoding="utf-8")`
(`cards/db.py:50-54`). Rust: embed the files byte-for-byte (`include_str!`/`include_bytes!`).

---

## 1. Packaged data formats

### 1.1 `gcgapi/cards.ndjson` (2011 lines, one printing per line)

Parsing: `[json.loads(line) for line in raw.splitlines() if line.strip()]` (`cards/db.py:57-59`).
The file ends with `\n`; it contains none of the extra `str.splitlines()` separators
(`\r \x0b \x0c \x1c \x1d \x1e \x85    `) and no BOM (checked), so splitting on `\n`
and skipping whitespace-only lines is equivalent today.

Every record has the same 29 keys in this order (checked over all 2011 lines):

| key | JSON type (count) | used by `_make_def`? |
| --- | --- | --- |
| `product_id` | str | yes → `canonical_product_id`, product alias map |
| `card_number` | str | yes (grouping key; `CardDef.card_number`) |
| `name` | str | yes |
| `set_code` | str | yes |
| `set_name` | str | no |
| `rarity` | str | yes |
| `card_type` | str | yes (`CardType(...)`) |
| `color` | str 1745 / null 266 | yes |
| `level` | int 1745 / null 266 | yes |
| `cost` | int 1745 / null 266 | yes |
| `ap` | int 1454 / null 557 | yes (non-pilot-capable) |
| `hp` | int 1611 / null 400 | yes (non-pilot-capable) |
| `ap_raw` | str 1454 / null 557 | yes (pilot-capable) |
| `hp_raw` | str 1611 / null 400 | yes (pilot-capable) |
| `zone` | str (`"Space Earth"` 934, `"-"` 846, `"Earth"` 122, `"Space"` 109) | yes |
| `trait` | str | no (the list `traits` is used) |
| `link` | str (`"-"` when none) | yes, only for UNIT / UNIT TOKEN |
| `traits` | list[str] | yes |
| `link_refs` | list | no |
| `keyword_effects` | list | no |
| `timing_markers` | list | no |
| `keywords_text` | str / null | no |
| `source_title` | str | yes |
| `block_icon` | str | no |
| `sp` | null 1983 / str 28 | no |
| `where_to_get` | str | no |
| `effect` | str (`"-"` 224 times) | yes |
| `image_url`, `detail_url` | str | no |

Value sets: `card_type` ∈ {`UNIT` 1056, `COMMAND` 306, `PILOT` 274, `RESOURCE` 155, `BASE` 109,
`UNIT TOKEN` 46, `EX BASE` 33, `EX RESOURCE` 32} (printings); `color` ∈ {Green, White, Blue,
Red, Purple, null}. All `card_number` and `product_id` values are ASCII; product id shapes are
`<card_number>` or `<card_number>_p<one digit>` (no `_p10`+ exists today). 1152 distinct
card numbers, 2011 distinct product ids. Three card numbers have no printing whose
`product_id == card_number`: `R-001`, `EXB-001`, `EXR-001` (all three have a
`canonical_product_id` override, §2.1).

`ap_raw` values on PILOT/COMMAND printings: `"+0" "+1" "+2" "+3"`, also `"1"` and `"2"`
(PILOT, no sign) and null (COMMAND).

### 1.2 `gcgapi/manifest.json`

Keys: `schema_version 1, dataset_version "28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42",
source_commit "ad9da1b2…", built_at "2026-10-12T14:37:38.154Z", card_count 2011, set_count 28,
ruling_count 368, rules_faq_count 119, errata_count 2, product_count 47, files {...},
disclaimer`. All counts shown by `data status` are copied from here, **not recomputed** from
the other files (`cli.py:264-281`), except "card numbers" (computed, §6).

### 1.3 `overrides.json`

```
{ "schema_version": 1, "description": str,
  "policies": { <policy_id>: {"summary": str, "rationale": str}, ... },   // 13 policies
  "resolutions": { <conflict_id>: Resolution, ... } }                       // 402 entries
Resolution = { "policy": str, "decision": str, "rationale": str,
               ["canonical_product_id": str], ["card_numbers": [str]], ["card_number": str],
               ["field_overrides": {field: value} | {card_number: {field: value}}] }
```

Only resolutions with a truthy `canonical_product_id` or truthy `field_overrides` change card
data (`cards/db.py:88`). In this snapshot: 50 `divergent:<number>` resolutions carry
`canonical_product_id`; 8 carry `field_overrides` (all flat `{field: value}`, none uses the
nested per-number form):

| conflict id | card_numbers | field_overrides keys |
| --- | --- | --- |
| `markers:GD02-053:unbracketed-keyword` | GD02-053 | `effect`, `keyword_effects`, `keywords_text` |
| `markers:GD03-015:ascii-colon` | GD03-015 | `effect` |
| `markers:GD03-035:ascii-colon` | GD03-035 | `effect` |
| `data-null:BASE:ap` | 72 BASE numbers | `ap: 0` |
| `data-null:EX-BASE:ap` | 30 EX BASE numbers | `ap: 0` |
| `data-null:UNIT:ap` | 10 UNIT numbers | `ap: 0` |
| `errata-unapplied:ST12-001:news-02_193` | ST12-001 | `effect` |

`keyword_effects`/`keywords_text` are not read by `_make_def`, so only `effect` and `ap` matter.
The `ap: 0` overrides are numerically no-ops (`_to_int(None)` is already 0) but they are still
recorded as applied overrides (counted by `data status`, listed in `conflict_resolutions`).

### 1.4 `curated_conflicts.json`

`{schema_version, description, conflicts: [333 objects], reviewed_without_conflict: {...}}`.
Each conflict has keys `id, kind, severity, card_numbers, title, description,
engine_behaviour, evidence` (302 have non-empty `card_numbers`). Every curated id is also a key
of `overrides.json` `resolutions`. Runtime uses only `len(conflicts)` (`cli.py:254,259`) and
`{id → frozenset(card_numbers)}` (`reports/versions.py:82-90`; entries without a truthy `id`
or that are not objects are skipped).

### 1.5 `official/deck_construction.json` (fields consumed, `deck/official.py:74-86`)

| JSON path | value | `ConstructionRules` field |
| --- | --- | --- |
| `main_deck.size` | 50 | `main_size` |
| `resource_deck.size` | 10 | `resource_size` |
| `main_deck.max_copies_per_card_number` | 4 | `max_copies` |
| `main_deck.allowed_card_types` | `["BASE","COMMAND","PILOT","UNIT"]` | `main_types` (frozenset) |
| `resource_deck.allowed_card_types` | `["RESOURCE"]` | `resource_types` (frozenset) |
| `colors.min_distinct_colors` | 1 | `min_colors` |
| `colors.max_distinct_colors` | 2 | `max_colors` |
| `effective_date` | `"2026-09-11"` | `effective_date` |

Everything else in the file (sources, `validation_checks`, `sideboard`, `tokens`, …) is
documentation only. `resource_deck.max_copies_per_card_number` is `null` and unused.

### 1.6 `official/banlist.json` (fields consumed, `deck/official.py:112-127`)

```
effective_date: "2026-09-25"
banned:      [{card_number, name, note, since, source_url}]            -> frozenset(card_number)
restricted:  [{card_number, max_copies, exceptions:[{id, description, ...}], ...}]
banned_pairs:[{id, cards:[a, b], names, note, since, source_url}]      -> BannedPair(id, (cards[0], cards[1]))
attribute_pair_rules: [{id, source_text, member_predicate, enumerated_members:[{card_number,name}],
                        max_total_copies_of_matching_cards, exceptions:[...], ...}]
```

Current content: banned `GD01-020` (Anksha); restricted `ST02-016` max 2, exception
`st02-unmodified-starter-deck`; banned pairs `(ST01-010, ST05-010)` then `(GD01-008, GD05-015)`
(file order); one attribute rule `vanilla-lv2-cost1-ap2-hp2`:
`member_predicate = {"all_of":[{"card_type":"UNIT"},{"level":2},{"cost":1},{"ap":2},{"hp":2},{"has_effect":false}]}`,
22 enumerated members (GD01-035, GD01-060, GD01-085, GD02-013, GD02-080, GD03-032, GD03-063,
GD04-078, GD05-014, GD05-027, GD05-042, GD05-062, GD05-077, ST01-005, ST04-008, ST05-004,
ST05-009, ST06-004, ST09-005, ST10-005, ST11-008, ST12-010), `max_total_copies_of_matching_cards`
4, exception `st05-unmodified-starter-deck`. `if_deck_contains`, `then_forbidden`,
`predicate_matches_in_card_data`, `ambiguities`, `verification`, `predicate_language` are not
read at runtime. Required keys: `restricted[].exceptions` (KeyError if absent,
`deck/official.py:119`); `attribute_pair_rules[].exceptions` defaults to `[]` (`:108`); only
`cards[0]` and `cards[1]` of a banned pair are read (`:123`). All lists keep file order.

`AttributePairRule.description` = first `"..."` substring of `source_text` (regex
`"([^"]+)"`, `deck/official.py:93-98`), fallback the rule id. Today:
`a Unit card that is Lv.2 with cost 1, 2 AP, and 2 HP, and without effects`.

Dataclasses (field order): `ConstructionRules(main_size, resource_size, max_copies, main_types,
resource_types, min_colors, max_colors, effective_date)`; `RuleException(id, description)`;
`Restricted(card_number, max_copies, exceptions)`; `BannedPair(id, cards)`;
`AttributePairRule(id, description, member_predicate, enumerated, max_total_copies,
exceptions)`; `Banlist(effective_date, banned, restricted, banned_pairs, attribute_rules)`
(`deck/official.py:19-66`). `construction_rules()` and `banlist()` are `@cache`d singletons.

### 1.7 `official/rules_version.json`, `official/bo3_match_rules.json`

Only `latest_version` ("1.9.0"), `effective_date` ("2026-09-11") and bo3 `effective_date`
("2026-07-03") are read (`reports/versions.py:66-74`).

---

## 2. Card DB load (`cards/db.py`)

`get_card_db()` (`cards/db.py:327-329`) is a process-wide `@cache` singleton:
`CardDB(load_raw_printings(), load_overrides())`. If `overrides.json` is missing,
`load_overrides` returns `{"schema_version": 1, "policies": {}, "resolutions": {}}`
(`cards/db.py:62-69`).

### 2.1 Algorithm (`CardDB.__init__`, `cards/db.py:119-158`) — exact order

1. `by_number`: group printings by `card_number`, preserving file order within a group (`:120-122`).
2. `override_index = _card_override_index(overrides)` (`:80-97`):
   - iterate `sorted(resolutions.items())` (by conflict id, Python code-point string order);
   - skip non-dict values; skip unless `canonical_product_id` or `field_overrides` is truthy;
   - target numbers = `res["card_numbers"]` if truthy, else `[res["card_number"]]` if truthy,
     else (if `":"` in the id) `[cid.split(":")[1]]` (second colon-separated segment);
   - append `(cid, res)` to `out[number]` for each number (list order = sorted-cid order;
     duplicates in `card_numbers` would be appended twice).
3. For each `number` in `sorted(by_number)` (code-point order; all ASCII, so byte order):
   1. `ps = sorted(printings_of_number, key=product_id)` (`:129`).
   2. For each `(cid, res)` in `override_index.get(number, [])` (sorted-cid order) (`:132-146`):
      - if `res["canonical_product_id"]` truthy: `preferred = str(it)` (last one wins);
      - `fo = res.get("field_overrides") or {}`; if `fo` is a dict containing key `number`
        whose value is a dict, `fo = fo[number]` (nested form);
      - `field_overrides.update(fo)` (later cid wins per field);
      - append `AppliedOverride(conflict_id=cid, card_number=number,
        decision=str(res.get("decision", res.get("policy", ""))),
        fields=tuple(sorted(fo)) if fo is a dict else ())`.
   3. `canon = dict(_choose_canonical(ps, preferred))` (`:100-109`):
      - `preferred` given → first printing with that `product_id`, else
        `ValueError(f"override names unknown canonical printing {preferred!r}")` (fatal);
      - else the printing whose `product_id == card_number`;
      - else the smallest `product_id` (`sorted(...)[0]`).
   4. `canon.update(field_overrides)` (shallow; replaces whole values).
   5. `cdef = _make_def(def_id=len(self._defs), canon, product_ids=tuple(p.product_id for p in ps))`;
      append to `_defs`; `_by_number[number] = cdef`; for every printing
      `_product_to_number[p.product_id] = number`.
4. `_register_inline_tokens()` (§2.3).
5. `_register_unit_variants()` (§2.4).

Result today: `len(db) == 1153`; def_ids 0..1151 = the 1152 card numbers in sorted order;
no synthetic `TOKEN:` defs (all 29 inline token specs match a printed T- card); def_id 1152 =
`UNIT:GD05-089:3/3`. First def_id per set prefix: EB01 0, EXB 90, EXBP 93, EXR 120, EXRP 131,
GD01 149, GD02 279, GD03 409, GD04 541, GD05 671, R 801, RP 842, ST01 909, ST02 925, ST03 941,
ST04 957, ST05 973, ST06 988, ST07 1003, ST08 1018, ST09 1033, ST10 1043, ST11 1059,
ST12 1075, ST13 1091, ST14 1107, T 1123 (T-029 = 1151).

Canonical printing ≠ card number only for `EXB-001 → EXB-001_p7`, `EXR-001 → EXR-001_p7`,
`R-001 → R-001_p7`.

### 2.2 `_make_def` normalisation (`cards/db.py:160-197`)

`rec` is the override-applied canonical record. Field by field (CardDef field order is
`cards/model.py:96-115`):

| CardDef field | rule |
| --- | --- |
| `def_id` | position in `_defs` |
| `card_number` | `str(rec["card_number"])` |
| `name` | `str(rec["name"])` |
| `card_type` | `CardType(rec["card_type"])` (unknown string → `ValueError`, fatal) |
| `color` | `Color(rec["color"])` if `rec.get("color")` truthy, else `None` |
| `level`, `cost` | `_to_int(v)` = `int(v) if isinstance(v, int) else 0` (`:112-113`) — null/str/float → 0; JSON `true` → 1 (bool is int) |
| `ap`, `hp` | if PILOT, or COMMAND with a pilot name: `parse_modifier(rec.get("ap_raw"/"hp_raw"))`; else `_to_int(rec.get("ap"/"hp"))` |
| `zones` | `tuple(z for z in str(rec.get("zone") or "-").split() if z != "-")` — whitespace split, `"Space Earth"` → `("Space","Earth")` |
| `traits` | `tuple(rec.get("traits") or ())` (the list, not the `trait` string) |
| `link` | `parse_link(rec.get("link"))` if `card_type` is UNIT or UNIT TOKEN, else `None` |
| `effect` | `str(rec.get("effect") or "-")` |
| `pilot_name` | PILOT → `str(rec["name"])`; COMMAND → `parse_command_pilot_name(effect)`; others `None` |
| `set_code` | `str(rec.get("set_code") or "")` |
| `rarity` | `str(rec.get("rarity") or "")` |
| `product_ids` | all product ids of the number, sorted |
| `canonical_product_id` | `str(rec["product_id"])` |
| `source_title` | `rec.get("source_title")` (str or None; e.g. `"-"` for EXB-001) |
| `extra_names` | default `()` (never set anywhere in `src/`) |

Helpers (`cards/model.py`):
- `parse_modifier(raw)` (`:77-81`): `None` or `raw.strip()` in `("", "-")` → 0; else
  `int(raw.strip().replace("+", ""))` (`"+2"`→2, `"1"`→1; a malformed value raises).
- `parse_command_pilot_name(effect)` (`:84-89`): first match of `【Pilot】\s*\[([^\]]+)\]`,
  group 1 `.strip()`; 68 COMMAND card numbers have one.
- `parse_link(raw)` (`:59-74`): `None` or `raw.strip()` in `("", "-")` → `None`. Else
  `names = tuple(m.strip() for m in re.findall(r"\[([^\]]+)\]", raw))`; remove those bracket
  groups with `re.sub(r"\[([^\]]+)\]", "", raw)`; `traits = tuple(m.strip() for m in
  re.findall(r"\(([^)]+)\)", rest))`; both empty → `ValueError(f"unparseable link condition: {raw!r}")`
  (fatal). Returns `LinkCondition(name_parts=names, traits=traits)`. Examples:
  `"[Amuro Ray]"` → (("Amuro Ray",),()); `"(Zeon) Trait"` → ((),("Zeon",));
  `"[Allelujah Haptism] / [Hallelujah Haptism]"` → two names. 175 real UNIT/UNIT TOKEN defs have `link None`.

`CardDef` properties (`cards/model.py:117-141`): `names` = `(name, *extra_names)` plus
`pilot_name` appended if truthy and not already present; `is_token` = card_type ∈ {UNIT TOKEN,
EX BASE, EX RESOURCE}; `is_unit_card` = ∈ {UNIT, UNIT TOKEN}; `is_pilot_capable` = PILOT or
(COMMAND and `pilot_name is not None`); `is_vanilla` = `effect.strip() in ("", "-")` (147 real
cards). Note `NameAlias` abilities from bindings live in `Registry.aliases`
(`effects/registry.py:240-241`), not in `CardDef.names`; deck-name matching ignores them.

Enums (`cards/model.py:10-45`; StrEnum, value = string):

| `CardType` member | value | is_token | is_unit | is_base | is_resource |
| --- | --- | --- | --- | --- | --- |
| UNIT | `"UNIT"` | | ✓ | | |
| PILOT | `"PILOT"` | | | | |
| COMMAND | `"COMMAND"` | | | | |
| BASE | `"BASE"` | | | ✓ | |
| RESOURCE | `"RESOURCE"` | | | | ✓ |
| UNIT_TOKEN | `"UNIT TOKEN"` | ✓ | ✓ | | |
| EX_BASE | `"EX BASE"` | ✓ | | ✓ | |
| EX_RESOURCE | `"EX RESOURCE"` | ✓ | | | ✓ |

Declaration order above is the enum order. `Color`: BLUE `"Blue"`, GREEN `"Green"`, RED
`"Red"`, WHITE `"White"`, PURPLE `"Purple"`. `MAIN_DECK_TYPES = {UNIT, PILOT, COMMAND, BASE}`
(`cards/model.py:37`). `GAMEPLAY_FIELDS` (`cards/db.py:24-40`) is defined but unused.

### 2.3 Inline tokens (`cards/db.py:199-241`, `cards/tokens.py`)

1. `t_cards` = defs (so far) with `card_type is UNIT_TOKEN`, def order.
2. `specs = {}`; for each def in def order, for each `TokenSpec` from
   `parse_token_specs(d.effect)` (match order): `specs.setdefault(s.key, s)` (first wins).
3. For each `key` in `sorted(specs)` (code-point order; keys contain non-ASCII e.g. `Ⅱ`):
   - `match` = T-cards with `t.name == s.name and t.ap == s.ap and t.hp == s.hp and
     set(t.traits) == set(s.traits)` (**token text is not compared**; trait order and
     duplicates ignored);
   - if any: `_token_by_key[key]` = the one with smallest `card_number`;
   - else append synthetic `CardDef(def_id=len(_defs), card_number=f"TOKEN:{name}:{ap}/{hp}",
     name, card_type=UNIT_TOKEN, color=None, level=0, cost=0, ap, hp, zones=(), traits=s.traits,
     link=None, effect=s.text or "-", pilot_name=None, set_code="TOKEN", rarity="",
     product_ids=(), canonical_product_id="")` (source_title default None) and map the key to it.
     Two unmatched specs with the same name/ap/hp but different traits/text would produce two
     defs with the **same** `card_number` string.

`TOKEN_DEF` regex (`cards/tokens.py:12-15`, verbatim):

```
\[(?P<name>[^\[\]]+)\]\(\((?P<traits>[^)]*(?:\)\s*\([^)]*)*)\)\s*[･・]\s*AP(?P<ap>\d+)\s*[･・]\s*HP(?P<hp>\d+)(?:\s*[･・]\s*(?P<text>[^()]*(?:\([^()]*\)[^()]*)*))?\)
```

(`･` = U+FF65 HALFWIDTH KATAKANA MIDDLE DOT, `・` = U+30FB KATAKANA MIDDLE DOT.)
`parse_token_specs` (`cards/tokens.py:38-52`), per `finditer` match:
- traits = `re.findall(r"\(([^)]*)\)", "(" + traits_group + ")")`, each `.strip()`, empties
  dropped (`"ZAFT) (Marine"` → `("ZAFT","Marine")`);
- `name = group.strip()`, `ap/hp = int(group)`;
- `text = (group("text") or "").strip()`; if non-empty → `normalize_token_text(text)`, else `""`.
- `normalize_token_text(t)` (`:32-35`) = `" ".join(t.replace("・","･").split()).strip().rstrip(".")`
  + `"."` if `t.strip()` non-empty (all trailing dots removed, exactly one added).
- `TokenSpec.key` (`:26-29`) = `f"{name}|{'/'.join(traits)}|{ap}|{hp}|{text}"`.
- Inverse used by the engine: `interp.py:1018-1024` `key.split("|", 4)`, traits
  `split("/")` dropping empties, `int(ap)`, `int(hp)` (so a token name or text containing `|`,
  or a trait containing `/`, would not round-trip).

40 spec occurrences, 29 distinct keys, all matched. Full map (key → card, def_id):

| key | card | def_id |
| --- | --- | --- |
| `Ad Balloon\|Civilian\|0\|1\|This Unit can't be set as active or paired with a Pilot.` | T-014 | 1136 |
| `Aile Strike Gundam\|Earth Alliance\|3\|3\|<Blocker>.` | T-008 | 1130 |
| `Alvaaron\|UN\|4\|1\|` | T-024 | 1146 |
| `Bit / Funnel\|Long-Range Weapon\|2\|2\|This Unit can't be paired with a Pilot or attack.` | T-029 | 1151 |
| `CGS Mobile Worker\|Tekkadan\|1\|1\|` | T-015 | 1137 |
| `Char's Zaku Ⅱ\|Zeon\|3\|1\|` | T-006 | 1128 |
| `Daughtress\|New UNE\|0\|1\|` | T-012 | 1134 |
| `Fatum-00\|Triple Ship Alliance\|2\|2\|<Blocker>.` | T-011 | 1133 |
| `GFreD\|Zeon\|4\|3\|` | T-020 | 1142 |
| `GOOhN\|ZAFT/Marine\|1\|1\|` | T-028 | 1150 |
| `GQuuuuuuX (Omega Psycommu)\|Clan\|3\|2\|` | T-019 | 1141 |
| `Graze Custom\|Tekkadan\|2\|2\|` | T-016 | 1138 |
| `Guncannon\|White Base Team\|2\|2\|` | T-002 | 1124 |
| `Gundam Barbatos 4th Form\|Tekkadan\|4\|4\|` | T-017 | 1139 |
| `Gundam Exia\|G Generation\|2\|2\|` | T-025 | 1147 |
| `Gundam\|White Base Team\|3\|3\|` | T-001 | 1123 |
| `Gundnode\|Quiet Zero\|2\|2\|<Breach 1>.` | T-026 | 1148 |
| `Guntank\|White Base Team\|1\|1\|` | T-003 | 1125 |
| `Hy-Gogg\|Cyclops Team\|2\|1\|` | T-013 | 1135 |
| `Launcher Strike Gundam\|Earth Alliance\|2\|4\|<Blocker>.` | T-009 | 1131 |
| `Leo\|OZ\|1\|1\|` | T-004 | 1126 |
| `Parts\|League Militaire\|1\|1\|This Unit can't choose the enemy player as its attack target.` | T-021 | 1143 |
| `Pluma\|Calamity War\|2\|1\|` | T-027 | 1149 |
| `Red Gundam\|Clan\|2\|3\|` | T-018 | 1140 |
| `Sword Strike Gundam\|Earth Alliance\|4\|2\|<Blocker>.` | T-010 | 1132 |
| `Tallgeese\|OZ\|4\|2\|` | T-005 | 1127 |
| `Wire-Guided Arm\|Zeon\|2\|1\|This Unit can't be paired with a Pilot.` | T-022 | 1144 |
| `Zaku Ⅱ\|Zeon\|1\|1\|` | T-007 | 1129 |
| `Zeong (Head)\|Zeon\|3\|1\|` | T-023 | 1145 |

(`\|` above is markdown escaping; the real separator is a bare `|`.) Example of text
normalisation: GD04-017 writes `HP1・This Unit…Pilot)` (U+30FB) → `…Pilot.`.

### 2.4 Unit variants (`cards/db.py:243-272`)

For each `base` in `list(self._defs)` (snapshot incl. synthetic tokens), `m =
re.search(r"deploy it as an? \(AP(\d+)[･・]HP(\d+)\) Unit", base.effect)`; skip if no match or
`base.card_type.is_unit`. Append `CardDef(def_id=len(_defs),
card_number=f"UNIT:{base.card_number}:{m[1]}/{m[2]}", name=base.name, card_type=UNIT,
color=base.color, level=base.level, cost=base.cost, ap=int(m[1]), hp=int(m[2]), zones=(),
traits=base.traits, link=None, effect="-", pilot_name=None, set_code=base.set_code, rarity="",
product_ids=(), canonical_product_id="")`; `_unit_variant[base.def_id] = variant.def_id`,
`_variant_base[variant.def_id] = base.def_id`. Only GD05-089 (def 759, PILOT "Master Asia")
matches today → def 1152 `UNIT:GD05-089:3/3`, AP 3 HP 3, Lv 6, cost 1, Red, traits
`("Gundam Fighter",)`, set GD05.

### 2.5 Lookup API (`cards/db.py:274-324`)

- `get(s)`: `key = s.strip()` (Python Unicode whitespace); `_by_number[key]` if present, else
  `_product_to_number.get(key)` → `_by_number[number]`, else `None`. **Case-sensitive.**
  `_by_number` holds only the 1152 real numbers: `get("UNIT:GD05-089:3/3")` and any
  `TOKEN:` key return `None`. A card number shadows an equal product id (they coincide anyway).
- `__getitem__` → `KeyError(s)` when `get` is `None`; `by_id(i)` → `_defs[i]`;
  `normalize_id(s)` → canonical card number or `None`; `__iter__`/`__len__` over all defs in
  def_id order (1153).
- `real_cards()` = defs whose `card_number` does not start with `"TOKEN:"` or `"UNIT:"`, in
  def order (1152; includes the 29 printed T- UNIT TOKENs, 30 EX BASE, 29 EX RESOURCE).
- `token_for(spec)` = `_token_by_key[spec.key]` (KeyError if unknown); `unit_variant(def_id)`;
  `base_def_id(def_id)` (identity except for variants); `ex_base` = `_by_number["EXB-001"]`,
  `ex_resource` = `_by_number["EXR-001"]`.

Real-card counts by type: UNIT 600, COMMAND 157, PILOT 127, RESOURCE 108, BASE 72, EX BASE 30,
EX RESOURCE 29, UNIT TOKEN 29 (total 1152 — README's "1152" is correct). Colorless real
cards: RESOURCE 108, EX BASE 30, EX RESOURCE 29, UNIT TOKEN 29.

### 2.6 Applied overrides (`CardDB.applied_overrides`)

`AppliedOverride(conflict_id, card_number, decision, fields)` (`cards/db.py:72-77`). Order:
outer = card number sorted, inner = conflict id sorted. 166 entries: 112 `data-null:*`
(`fields=("ap",)`), 50 `divergent:*` (`fields=()`), 3 `markers:*`, 1 `errata-unapplied:*`.
164 distinct numbers; two numbers have two entries, in this order:
`EXB-001: data-null:EX-BASE:ap, divergent:EXB-001` and
`ST01-015: data-null:BASE:ap, divergent:ST01-015`. Non-`ap` field lists:
GD02-053 `("effect","keyword_effects","keywords_text")`, GD03-015 `("effect",)`,
GD03-035 `("effect",)`, ST12-001 `("effect",)`. A resolution naming a number that is not in
the snapshot is silently ignored.

---

## 3. Deck files

### 3.1 Reading (`deck/parse.py:111-114`, `cli.py:102-110`)

`load_deck(path)`: `Path(path).read_text(encoding="utf-8-sig")` (one leading UTF-8 BOM
stripped; universal-newline mode turns `\r\n`/`\r` into `\n`), then
`parse_deck(text, name=p.stem, source=str(p))`.

CLI errors (all → stderr `error: <msg>`, exit 1, `cli.py:394-396`):
- `OSError` → `cannot read deck file {path}: {exc.strerror or exc}` (e.g.
  `No such file or directory`, `Is a directory`);
- `DeckError` → `{path}: {exc}` (multi-line, §3.4);
- `UnicodeDecodeError` → `{path}: not UTF-8 text ({exc.reason} at byte {exc.start})`.
  `exc.start` is the offset **after the stripped BOM** (verified: BOM + 11 valid bytes + `\xff`
  → `at byte 11`). Reasons (CPython UTF-8 decoder): `invalid start byte` (lead byte
  0x80–0xC1 or 0xF5–0xFF), `invalid continuation byte` (valid lead, bad following byte,
  including overlongs `E0 80`, surrogates `ED A0`, `F4 90`), `unexpected end of data`
  (truncated valid prefix at EOF). Rust mapping: `from_utf8(after_bom)` error →
  `start = valid_up_to()`; `error_len() == None` → `unexpected end of data`; else lead byte
  in 0x80..=0xC1 or 0xF5..=0xFF → `invalid start byte`; else `invalid continuation byte`.

`{path}` is `str(pathlib.Path(arg))`: `./a//b.txt` → `a/b.txt`, trailing `/` removed.
`p.stem`: last component minus its last suffix (`b.c.txt`→`b.c`, `.hidden`→`.hidden`,
`x.`→`x`, `x..txt`→`x.`, `.x.txt`→`.x`).

### 3.2 Grammar (`deck/parse.py:18-89`)

Lines = `text.splitlines()` (separators: `\n \r \r\n \x0b \x0c \x1c \x1d \x1e \x85  
 `), numbered from 1. Per line:
1. `line = strip_invisible(raw).strip()` — delete **every** U+200B, U+200C, U+200D, U+2060,
   U+FEFF, U+00AD anywhere in the line (`deck/names.py:7,25-26`), then strip Python
   whitespace (§10 H3).
2. Skip if empty or `line.startswith("#")` (no end-of-line comments: `4 GD01-001 Gundam # c`
   has name `Gundam # c`).
3. Match `^(?P<count>\d+)[xX]?\s+(?P<id>\S+)(?:\s+(?P<name>.*\S))?\s*$` (`:18`), with Python
   Unicode `\d` (category Nd: `４ GD01-001` is count 4) and Unicode `\s`/`\S`.
   No match → `SYNTAX`: `line {n}: expected '<count> <card_number> [name]', got {line!r}`.
4. `count = int(group)`; `count < 1` → `SYNTAX`: `line {n}: count must be at least 1 for {written}`
   (`written` = raw id group, not repr'd). Checked before id resolution.
5. `number = resolve_card_number(written)`; `None` → `UNKNOWN_ID`:
   `line {n}: unknown card id {written!r} (not in the card data)`.
6. If a name was written and `not names_match(name, db[number].names)` → `NAME_MISMATCH`:
   `line {n}: {written} is {db[number].name!r} in the card data, not {name!r}`
   (`Violation.cards = (number,)`).
7. Else `DeckEntry(count, card_number=number, written, name, line=n)`.

At most one problem per line, first failing step wins. If any problems: raise
`DeckError([messages in line order], deck_name=name, violations=problems)`; otherwise
`build_deck`. Duplicate lines for one number add up.

Id resolution (`deck/parse.py:23-42`): candidates in order, first hit wins where
`db.get(c)` is not None and its card_number does not start with `"TOKEN:"`:
1. `written`; 2. `written.upper()`; 3. if `"_"` in written: `base.upper() + "_" + suffix.lower()`
   where `base, _, suffix = written.partition("_")` (first `_`); 4. if
   `^(?P<base>.+?)_[pP]\d+$` matches: `base.upper()` (lazy base = shortest prefix such that the
   rest is `_p<digits>`; `A_p1_p2` → base `A_p1`).
Consequences (verified): `gd01-001`→GD01-001; `GD01-001_P1`→GD01-001 (cand. 3);
`GD01-001_p9` (no such printing) → GD01-001 (cand. 4); `GD01-001_q1` → unknown;
`exb-001_P7` → EXB-001; `R-001_p1` → R-001; `UNIT:GD05-089:3/3` → unknown.

Name match (`deck/names.py:29-38`): `normalize_name(s)` = NFKC → delete the 6 zero-width
chars → translate quotes (`‘ ’ ‚ ‛ ′ ´ \`` → `'`; `“ ” „ ″` → `"`) → `" ".join(s.split())`
→ `casefold()`; `names_match(w, names)` = any `normalize_name(n) == normalize_name(w)` over
`CardDef.names` (name + pilot name for 【Pilot】 Commands). Because NFKC runs first, `´` (U+00B4)
becomes `" ́"` and `″` (U+2033) becomes `′′` → `''` before the quote table applies
(verified: `Z´Gok`→`z ́gok`, `a″b`→`a''b`). Other verified outputs: `Zaku Ⅱ`→`zaku ii`,
`ＺＡＫＵ  ii`→`zaku ii`, `Straße`→`strasse`, `Gundam　Mk-Ⅱ`→`gundam mk-ii`.

### 3.3 Deck assembly, digest (`deck/parse.py:92-129`, `deck/model.py`)

`build_deck`: for each entry (file order), `cdef.card_type.is_resource` (RESOURCE or
EX RESOURCE) → resource deck, else main (UNIT TOKEN / EX BASE go to main);
`extend([number] * count)`. `Deck(name, main=tuple(sorted(main)), resources=tuple(sorted(resources)),
entries=tuple(entries), source)` — **main/resources are sorted card-number strings**; this is
the pre-shuffle order the engine maps to def_ids (`engine/game.py:134-141`), and replay files
rebuild it the same way (`runner/replayfile.py:68-72`, sorted expansion of counts).
`to_decklist(deck)` = `DeckList(main, resources)`.

`deck_digest(deck)` = SHA-256 hex (lowercase) of
`json.dumps({"main": counts(main), "resources": counts(resources)}, sort_keys=True,
separators=(",", ":")).encode()` with `counts = dict(sorted(Counter(...).items()))`
(ASCII-escaped by default). Test vector, `examples/decks/blue-white-federation.txt`:

```
{"main":{"GD01-001":2,"GD01-004":4,"GD01-008":4,"GD01-087":2,"GD01-099":2,"GD03-001":2,"GD04-008":2,"GD04-012":2,"GD04-084":2,"GD04-122":2,"ST01-001":4,"ST01-002":2,"ST01-004":3,"ST01-005":3,"ST01-010":4,"ST01-012":2,"ST01-014":3,"ST04-013":3,"ST08-015":2},"resources":{"R-001":10}}
a75b15f0983c82469c25de9ee34a7dc8ea420be1534ec2937ac6343565c9e108
```

Dataclasses (field order): `DeckEntry(count, card_number, written, name, line)`,
`Deck(name, main, resources, entries, source)`, `Violation(code, message, cards)`
(`deck/model.py:9-37`).

### 3.4 `DeckError` text (`deck/model.py:40-58`)

```
{subject} has {len(problems)} problem(s):
  - {problem 1}
  - {problem 2}
```

`subject` = `deck {deck_name!r}` if `deck_name` truthy else `deck`. Lines joined with `\n`, no
trailing newline. Verified CLI output:

```
error: /…/parse_errs.txt: deck 'parse_errs' has 4 problem(s):
  - line 3: count must be at least 1 for GD01-002
  - line 4: expected '<count> <card_number> [name]', got 'foo'
  - line 5: unknown card id 'XX-999' (not in the card data)
  - line 6: GD01-003 is 'Unicorn Gundam 02 Banshee (Destroy Mode)' in the card data, not 'Wrong Name'
```

A file named `it's.txt` prints `deck "it's" has …`; a name `"Gun'dam"` prints
`not '"Gun\'dam"'`; T-006 prints `is "Char's Zaku Ⅱ"` (Python `repr`, §10 H1).

---

## 4. Legality (`deck/validate.py`)

`validate_deck(deck, *, banlist=True, implemented=True)` (`:268-289`) returns violations in
this fixed order of checks; within each check the order given:

| # | code | check & order | message (exact f-string) | `cards` |
| --- | --- | --- | --- | --- |
| 1 | `UNKNOWN_ID` | `sorted({n in main+resources with db.get(n) None})` | `unknown card id {n!r} (not in the card data)` | `(n,)` |
| 2 | `NAME_MISMATCH` | entries in file order, name given, known, not matching | `line {e.line}: {e.written} is {cdef.name!r} in the card data, not {e.name!r}` | `(e.card_number,)` |
| 3 | `MAIN_SIZE` | `len(main) != 50` | `the deck has {len(main)} cards; it must have exactly {50} (rule 6-1-1)` | `()` |
| 3 | `RESOURCE_SIZE` | `len(resources) != 10` (after MAIN_SIZE) | `the resource deck has {len(res)} cards; it must have exactly {10} Resource cards (rule 6-1-1)` | `()` |
| 4 | `CARD_TYPE` | distinct known main numbers whose type value ∉ main_types, sorted; then the same for resources vs resource_types | `_type_message` below | `(n,)` |
| 5 | `COPY_LIMIT` | `sorted(Counter(main).items())`, count > 4 (main only; tokens included) | `{count} copies of {label}; at most {4} copies of cards with the same card number (all printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)` | `(n,)` |
| 6 | `COLORS` | see below | see below | extra-color numbers |
| 7 | `BANNED` | `sorted(set(main) & banned)` | `{label} is banned: no copies may be in a deck (Banned & Restricted list effective {date})` | `(n,)` |
| 8 | `RESTRICTED` | banlist file order; `Counter(main)[n] > max_copies` | `{count} copies of {label}; it is restricted to {max} copies (Banned & Restricted list effective {date}).{exception_note}` | `(n,)` |
| 9 | `BANNED_PAIR` | banlist file order; both in `set(main)` | `{label_a}, {label_b} are a banned pair and cannot be in the same deck (Banned & Restricted list effective {date})` | `p.cards` |
| 10 | `ATTRIBUTE_PAIR` | per rule in file order, see below | see below | members |
| 11 | `UNIMPLEMENTED` | `sorted(set(main) | set(resources))`, known, `registry.cards[def_id].script is None` | `{label} has no implemented effect in this simulator version, so games cannot use it ({entry.error})` | `(n,)` |

Checks 7–10 run only if `banlist`, 11 only if `implemented`. `label` = `f"{number} {cdef.name}"`
or bare `number` if unknown (`:20-22`); `labels` = `", ".join(label…)` (`:25-26`); `{date}` =
`banlist.effective_date` ("2026-09-25"). B&R checks look at the **main deck only**.
`_known(...)` drops unknown numbers (keeps copies).

`_type_message(cdef, in_resource_deck)` (`:87-102`), first applicable:
- token: `{num} {name} is a token ({type}); tokens are prepared outside the game and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)`
- resource deck: `{num} {name} is a {type} card; a resource deck is constructed with Resource cards only (rule 6-1-1-4)`
- main: `{num} {name} is a {type} card; a deck is constructed with Unit, Pilot, Command, and Base cards only (rule 6-1-1-1)`
(`{type}` = `card_type.value`, e.g. `EX BASE`.) For file-parsed decks only the token form can
occur (main: UNIT TOKEN/EX BASE; resources: EX RESOURCE).

`COLORS` (`:133-151`): `copies = Counter(color.value for known main copies with a color)`.
No violation if there are no known main cards, or `1 <= len(copies) <= 2`. Else
`ranked = sorted(copies.items(), key=(-count, color_value))`; extra colors = `ranked[2:]`;
`cards = sorted({main numbers whose color is extra})`; message:
`the deck uses {len(copies)} colors ({shown}); a deck must be constructed entirely using either one or two card colors (rule 6-1-1-2). Cards of the extra color(s): {labels(cards)}`
with `shown = ", ".join(f"{color} x{count}" for ranked)`. An all-colorless main deck
(e.g. only tokens) yields `the deck uses 0 colors (); … Cards of the extra color(s): ` (ends with
the space) and `cards=()` (verified).

`ATTRIBUTE_PAIR` (`:202-245`): `members = [n for n in sorted(set(main)) if n in rule.enumerated
or (known and matches(rule.member_predicate, cdef))]`; `total` = copies in main that are
members. **No violation iff `len(members) < 2 and total <= max_total_copies`** — so one member
with 5 copies also fires (alongside COPY_LIMIT). Message:
`{labels(members)} are each "{description}"; every combination of two such card numbers is a banned pair, so a deck may contain only one of them, at most {max_total} copies (Banned & Restricted list effective {date}).{exception_note}{note_text}`.
`note_text` = `"".join(f" Note: {note}.")` over known members (sorted order) whose note is
non-empty: listed but predicate false → `{n} is on the official list but does not match the description in the data`;
predicate true but not listed → `{n} matches the description in the card data but is not on the official list`
(unreachable today: predicate set == enumerated set).

`_exception_note(excs)` = `"".join(f" Exception {e.id!r} is not applied: the simulator cannot verify an unmodified starter deck." for e)`
(`:17,29-30`), e.g. ` Exception 'st02-unmodified-starter-deck' is not applied: …deck.`

Predicate language (`deck/official.py:135-173`): `matches(pred, card, matched=None)` iterates
`pred.items()` in **JSON key order**, short-circuits on the first false; `all_of` (list, all),
`any_of` (list, any), `not` (one predicate); leaves: `card_type` (`card_type.value == v`),
`level`, `cost`, `ap`, `hp` (int ==), `has_effect` (`not is_vanilla == v`), `color`
(`color is not None and color.value == v`), `trait` (`v in traits`), `name_contains`
(`str(v) in name`), `card_number` (`== matched if v == "$matched" else v`); anything else →
`ValueError(f"unknown banlist predicate operator {key!r}")`.

`require_legal(deck)` (`:292-300`): raises `DeckError([f"[{v.code}] {v.message}"…],
deck_name=deck.name, violations=…)`.

CLI rendering (`cli.py:119-136`):

```
error: {path}: {N} violation(s)
  [{code}] {message}
  …
```
(only the first line carries `error: `). Success (stdout, exit 0):
`{path}: legal ({len(main)} cards, {len(resources)} resources; {colors})` where `colors` =
`"/".join(sorted(distinct main color values))` or `no colors`, e.g.
`examples/decks/blue-white-federation.txt: legal (50 cards, 10 resources; Blue/White)`.
`benchmark` validates the DUT deck first, then the benchmark deck; the first failure aborts.

A verified 12-violation run (one of each legality code except UNKNOWN_ID/NAME_MISMATCH/
UNIMPLEMENTED) is reproduced in §11.

Engine-level re-check (cross-reference, owned by the engine inventory):
`engine/game.py:58-101` `deck_construction_problems` runs in every `new_game(validate=True)`
with its own messages (`deck has {n} cards; exactly 50 required (6-1-1)`, …).

---

## 5. "Implemented"

`registry.cards[def_id].script is not None` (`cli.py:239`, `deck/validate.py:255-256`).
`build_registry` (`effects/registry.py:323-340`) iterates **all** defs in def_id order and calls
`script_for(cdef)` (`effects/scripts.py:12-16`): a binding registered with `@card("<number>")`
wins, else `compile_card(cdef)` (`effects/compiler/__init__.py:53-67`; no parts → `vanilla`
script; any `CompileError` → `UnimplementedCardError("; ".join(f"{line!r}: {err}"))`).
`UnimplementedCardError` → `script=None, error=str(exc)`; `ValueError, AssertionError,
TypeError, KeyError, IndexError, AttributeError` → `error = f"binding error: {type(exc).__name__}: {exc}"`;
other exceptions propagate. Today: 1153 entries, sources compiled 538, vanilla 313 (incl. the
UNIT: variant), binding 302; zero unimplemented. Bindings load from
`effects/bindings/*` modules sorted by module name, skipping names starting with `_`
(`effects/bindings/__init__.py:91-96`).

---

## 6. `gcg-sim data status`

`data_status()` (`cli.py:263-291`) builds:

```
versions:  versions()                                   (§7.1)
snapshot:  {k: manifest.get(k)} for k in (dataset_version, source_commit, built_at, card_count,
           set_count, ruling_count, rules_faq_count, errata_count, product_count)  (missing → null)
banlist:   {effective_date, banned: sorted(frozenset), restricted: {number: max_copies} (file order),
            banned_pairs: [list(p.cards)] (file order), attribute_pair_rules: [rule.id] (file order)}
coverage:  {card_numbers: len(real_cards()), implemented: #real with script,
            by_type: {type_value: {cards, implemented}} for type_value in sorted(types present)}
conflicts: {resolutions: len(overrides["resolutions"]) (all entries),
            card_data_overrides: len(applied_overrides),
            card_numbers_overridden: len({o.card_number}),
            curated_conflicts: len(curated_conflicts["conflicts"])}
```

`--json`: `print(json.dumps(status, indent=2, sort_keys=True, ensure_ascii=False))` (item
separator `,` + newline, key separator `: `, empty containers `[]`/`{}`, trailing `\n` from
print). Text (`cli.py:294-322`), lines joined with `\n` plus print's `\n`:

```
gcg-sim {package} (results schema {results_schema})
card data: gcg-api dataset {dataset_version}
  source commit {source_commit}, built {built_at}
  {card_count} printings, {card_numbers} card numbers, {set_count} sets, {ruling_count} rulings, {rules_faq_count} rules FAQ entries, {errata_count} errata
rules: Comprehensive Rules Ver. {rules_version} (effective {rules_effective_date})
banned & restricted list: effective {date} ({len(banned)} banned, {len(restricted)} restricted, {len(banned_pairs)} banned pairs, {len(attribute_pair_rules)} attribute pair rule(s))
BO3 match rules: effective {bo3_rules_effective_date}
implementation coverage: {implemented}/{card_numbers} card numbers ({pct:.1f}%)
  {TYPE}: {implemented}/{cards}          (one line per by_type entry, sorted type value)
conflicts: {resolutions} resolutions in overrides.json ({card_data_overrides} card-data overrides on {card_numbers_overridden} card numbers); {curated_conflicts} curated conflicts
```

`pct = 100 * implemented / card_numbers` (Python true division → float) if `card_numbers`
else `0.0`; formatted with `.1f` (correctly rounded, ties-to-even on the exact binary value:
`6.25`→`6.2`, `12.75`→`12.8`). Note `product_count` is in the JSON only.
Current verbatim text output:

```
gcg-sim 0.1.0 (results schema 1.1.0)
card data: gcg-api dataset 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42
  source commit ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42, built 2026-10-12T14:37:38.154Z
  2011 printings, 1152 card numbers, 28 sets, 368 rulings, 119 rules FAQ entries, 2 errata
rules: Comprehensive Rules Ver. 1.9.0 (effective 2026-09-11)
banned & restricted list: effective 2026-09-25 (1 banned, 1 restricted, 2 banned pairs, 1 attribute pair rule(s))
BO3 match rules: effective 2026-07-03
implementation coverage: 1152/1152 card numbers (100.0%)
  BASE: 72/72
  COMMAND: 157/157
  EX BASE: 30/30
  EX RESOURCE: 29/29
  PILOT: 127/127
  RESOURCE: 108/108
  UNIT: 600/600
  UNIT TOKEN: 29/29
conflicts: 402 resolutions in overrides.json (166 card-data overrides on 164 card numbers); 333 curated conflicts
```

JSON output (verbatim key order is sorted; `code_digest` shown for HEAD `d9d41af`):
top-level `banlist, conflicts, coverage, snapshot, versions`; `banlist.restricted` =
`{"ST02-016": 2}`; `banlist.banned_pairs` = `[["ST01-010","ST05-010"],["GD01-008","GD05-015"]]`;
`versions` = `{banlist_effective_date "2026-09-25", bo3_rules_effective_date "2026-07-03",
code_digest "3a11556b77c867330bb652d1f2468903f8dc7174a02e03b8b752760239b49c0b",
data_built_at, data_source_commit, dataset_version, deck_rules_effective_date "2026-09-11",
package "0.1.0", results_schema "1.1.0", rules_effective_date "2026-09-11",
rules_version "1.9.0"}`.

Exit codes (`cli.py:33,386-399`): 0 ok; 1 `CliError` (prints `error: …` to stderr); 2 argparse
usage error (`SystemExit.code`); 130 `KeyboardInterrupt` (`interrupted: workers stopped, no reports written`).

---

## 7. Versions, code digest, conflict resolutions (results.json / summary.md)

### 7.1 `versions()` (`reports/versions.py:55-75`, `@cache`)

Insertion order: `package` (`importlib.metadata.version("gcg-sim")`, `"unknown"` if not
installed; `pyproject.toml` version `0.1.0`), `code_digest`, `results_schema` (`"1.1.0"`,
`:16`), `dataset_version`, `data_source_commit`, `data_built_at` (manifest), `rules_version`,
`rules_effective_date` (rules_version.json), `banlist_effective_date`,
`deck_rules_effective_date`, `bo3_rules_effective_date`.

### 7.2 `code_digest()` (`reports/versions.py:32-52`)

`sha256()`; collect every file under `importlib.resources.files("gcg_sim")` recursively,
skipping any entry named `__pycache__` and files ending `.pyc` (hidden files such as
`.DS_Store` are **included**); relative POSIX path with `/`; sort the `(path, file)` list by
the **whole path string** (code-point order, `"a.py" < "a/b"` because `.`<`/`); for each:
`update(path.encode("utf-8")); update(b"\0"); update(file_bytes)`. Reproduced independently
with `os.walk` (122 files → `3a11556b…49c0b`). With the editable install it hashes
`src/gcg_sim/` itself, so it changes with every Python source edit (see Open questions).

### 7.3 `conflict_resolutions(decks)` (`reports/versions.py:93-135`)

Input: `{"deck_under_test": dut.main + dut.resources, "benchmark": bench.main + bench.resources}`
(`reports/results.py:181-186`).
1. `in_decks[n]` = labels (dict order) of decks containing `n`.
2. For each `AppliedOverride o` (CardDB order) with `o.card_number in in_decks`:
   `out[(n, cid)] = entry(changes_card_data=True, fields=list(o.fields), decision=o.decision)`.
3. For `cid, res` in `sorted(overrides["resolutions"].items())` (skip non-mappings): cards =
   `set(res.card_numbers or ()) ∪ {res.card_number if truthy} ∪ set(cid.split(":")[1:])`
   (**every** segment after the first, e.g. `{"GD01-001","Q119"}`) `∪ curated[cid].card_numbers`;
   for `n` in `sorted(cards ∩ in_decks.keys())`: `out.setdefault((n, cid), entry(
   changes_card_data=False, fields=[], decision=str(res.get("decision",""))))` (step-2 entries win).
4. Return `[out[k] for k in sorted(out)]` (sorted by `(card_number, conflict_id)`).

Entry key order: `card_number, name (db[number].name), conflict_id, decks (sorted labels:
"benchmark" < "deck_under_test"), decision, fields, changes_card_data`. summary.md renders
(`reports/summary.py:211-219`):

```
## Conflict resolutions affecting these decks

- `{conflict_id}` ({card_number} {name}; {", ".join(decks)}): {decision}
```
or `- none`. The two example decks (blue-white-federation vs red-green-zeon) give 17 entries;
first: `GD01-001 / ruling:GD01-001:Q119 / ["deck_under_test"] / changes_card_data false`.

---

## 8. `sources/conflicts.py`, `rules/index.py`

Neither is used by `gcg-sim benchmark|validate|data status|replay` nor by report generation
(§0). `sources/conflicts.py` regenerates `data/conflicts.json` and `docs/CONFLICTS.md`;
`rules/index.py` parses the CR markdown + `rules_na.json` into `rules_index.json` for the
traceability tool. A Rust runtime does not need them; their only runtime footprint is being
hashed into `code_digest`.

---

## 9. Fingerprints for the Rust port

Generated with the script below (run from the repo root with `uv run python <script>`); a Rust
port that dumps the same structure must reproduce the hashes.

```python
import hashlib, json
from gcg_sim.cards.db import get_card_db
db = get_card_db()
lines = []
for d in db:
    link = None if d.link is None else [list(d.link.name_parts), list(d.link.traits)]
    rec = [d.def_id, d.card_number, d.name, d.card_type.value, d.color.value if d.color else None,
           d.level, d.cost, d.ap, d.hp, list(d.zones), list(d.traits), link, d.effect, d.pilot_name,
           d.set_code, d.rarity, list(d.product_ids), d.canonical_product_id, d.source_title]
    lines.append(json.dumps(rec, ensure_ascii=False, separators=(",", ":")) + "\n")
print(hashlib.sha256("".join(lines).encode()).hexdigest())
print(hashlib.sha256("".join(f"{d.def_id}\t{d.card_number}\n" for d in db).encode()).hexdigest())
print(hashlib.sha256("".join(f"{k}\t{v.card_number}\n" for k, v in sorted(db._token_by_key.items())).encode()).hexdigest())
print(hashlib.sha256("".join(f"{o.conflict_id}\t{o.card_number}\t{o.decision}\t{','.join(o.fields)}\n"
                              for o in db.applied_overrides).encode()).hexdigest())
```

| fingerprint | sha256 |
| --- | --- |
| full CardDef dump (1153 lines) | `c38ea9017c2109ff622745f53eaed210c526369fd8f2aeaa7b8ae17c76baa7de` |
| def_id → card_number table | `c4bb4d9269df02c745a753059cfb0f5993290600fc3202d8e54eadd1fc8cd5df` |
| token key → card map | `176b958ac9db6a04ae9c71285629fd390bf75f06e3c8fa90d065167958130105` |
| applied overrides | `613cd0b24183fe239d6806a4442f87d8ae1c0b72e8efa7cc8513123df5d8273b` |

(`json.dumps(..., ensure_ascii=False, separators=(",",":"))` matches `serde_json::to_string`:
same escapes `\" \\ \n \r \t \b \f`, other C0 controls as lowercase `\u00xx`, non-ASCII raw.)

---

## 10. Hazards (Python semantics that a Rust port must replicate)

- **H1 `repr()` of str in messages** — `deck/parse.py:50,60,66`, `deck/validate.py:30,42,56`,
  `deck/model.py:56`, `cards/db.py:105`, `cards/model.py:73`, `deck/official.py:157`.
  Algorithm: quote `'` unless the string contains `'` and no `"` (then `"`); escape `\`→`\\`,
  the chosen quote→`\'`/`\"`, `\t \n \r` as such; other chars `< 0x20` and `0x7f` → `\xhh`;
  non-ASCII kept if `str.isprintable()` (not in categories Cc, Cf, Cs, Co, Cn, Zl, Zp, or Zs
  other than U+0020), else `\xhh` (≤0xFF) / `\uhhhh` (≤0xFFFF) / `\Uhhhhhhhh`, lowercase hex.
  Verified: `"it's"`, `'a"b'`, `'a\'b"c'`, `'\xa0x‎　\x7f\t'`, `'Zaku Ⅱ'`.
  Printability depends on the Unicode version (17.0.0).
- **H2 `str.splitlines()`** — `deck/parse.py:78`, `cards/db.py:59`: splits on 11 separators
  (§3.2), not just `\n`; `4 GD01-001\x0c4 GD01-002` is two entries (verified). Rust `lines()`
  differs.
- **H3 Python whitespace** (`str.strip/split/isspace`, regex `\s` on str) includes
  U+001C–U+001F, which Rust `char::is_whitespace`/regex `\s` do not. Set:
  `09-0D 1C-1F 20 85 A0 1680 2000-200A 2028 2029 202F 205F 3000`. Used in `deck/parse.py:18,79`,
  `deck/names.py:33`, `cards/db.py:175,293`, `cards/model.py:67,79-81,89`, `cards/tokens.py:13,33`.
  `1 GD01-001\x1fGundam` parses name `Gundam` (verified).
- **H4 Unicode `\d` + `int()`** — `deck/parse.py:18,53`, `cards/tokens.py:13,47-48`,
  `cards/db.py:47,258`: `\d` = category Nd (fullwidth `４`, Arabic-Indic `٣` accepted as counts,
  verified); `int()` converts them. Rust `str::parse` does not. `int()` is unbounded but raises
  plain `ValueError` above 4300 digits — that escapes `cli._read_deck` (not caught) → traceback.
  A huge count makes `[number] * count` allocate (MemoryError/hang). Leading zeros fine (`007`→7).
- **H5 NFKC + casefold** — `deck/names.py:32-33`: Unicode 17.0.0 tables; `casefold` is full
  case folding (`ß`→`ss`), not `to_lowercase`. NFKC precedes the quote table (U+00B4, U+2033
  entries are effectively dead / produce `''`).
- **H6 `str.upper()`/`lower()`** — `deck/parse.py:25,28,31`: full Unicode case mapping on the
  written id (only matters for non-ASCII ids, which never resolve anyway, but the candidate
  strings are looked up with `db.get`).
- **H7 Sorting** — all `sorted()` here are on str or on tuples with unique str first elements
  (`cards/db.py:85,109,128-129,207,218`, `deck/parse.py:104-105,122`, `deck/validate.py:40,111,114,128,139,141,156,214,251`,
  `reports/versions.py:48,129,132,135`, `cli.py:115,246,284`): code-point order == UTF-8 byte
  order, so Rust `String` `Ord` is fine; `_p10` sorts before `_p2` (none exist today).
  `code_digest` must sort the **joined path string**, not `Path` components (Rust `Path` `Ord`
  is component-wise). `check_colors` key `(-count, color)` (`deck/validate.py:139`).
- **H8 JSON key order** — `deck/official.py:162` evaluates predicate keys in JSON order with
  short-circuit; use an order-preserving map (serde_json `preserve_order`) or arrays.
  `overrides.json` resolutions are iterated **sorted**, never in file order
  (`cards/db.py:85`, `reports/versions.py:129`). Python `json.loads` keeps the last duplicate
  key; JSON `NaN`/`Infinity` would be accepted by Python and rejected by serde (none present).
- **H9 `isinstance(v, int)`** — `cards/db.py:113`: JSON `true`→1, `false`→0, floats (`3.0`)→0.
  serde `as_i64()` gives None for booleans (→0, differs) and for `3.0` (→0, same).
- **H10 dict/`setdefault` first-wins vs `update` last-wins** — token specs keep the first spec
  per key (`cards/db.py:206`); override `preferred` and fields keep the last (`:134,138`);
  conflict entries keep applied-override entries over later ones (`reports/versions.py:128,134`).
  `restricted` in `data status` is a dict (duplicate numbers would collapse, `cli.py:285`).
- **H11 Sets** — `set(t.traits) == set(s.traits)` (`cards/db.py:215`); `frozenset` fields
  (`banned`, `main_types`, `enumerated`) are only used for membership or sorted
  (`deck/validate.py:156`); `for n in set(numbers)` in `reports/versions.py:122` does not affect
  output order. No `hash()`-order dependence in this area; `CardDef` is hashable (frozen
  dataclass) but is never iterated from a set here.
- **H12 `pathlib`** — `cli.py:106-110,123,132`, `deck/parse.py:112-114`: `str(Path(arg))`
  normalises (`./`, `//`, trailing `/`); `stem` semantics (§3.1); Rust `Path::display()` keeps
  the raw text.
- **H13 UTF-8-sig decoding** — `deck/parse.py:113`: one BOM stripped; error offset is relative
  to the post-BOM bytes; reason strings per §3.1.
- **H14 Float formatting** — `cli.py:298,313`: `{pct:.1f}` ties-to-even on the exact binary
  value; verify Rust's `{:.1}` on `6.25` gives `6.2`.
- **H15 JSON output formatting** — `cli.py:328` (`indent=2, sort_keys=True, ensure_ascii=False`),
  `deck/parse.py:128` (`sort_keys=True, separators=(",",":")`, ASCII-escaping default).
- **H16 Exceptions as control flow** — `DeckError` aggregates and is caught in `cli.py:107`;
  `UnimplementedCardError` and six builtin exception types become `script=None` in
  `effects/registry.py:331-337` (error text embeds Python type names and `str(exc)`);
  `KeyError` from `db[n]` (`cards/db.py:302`) — `str(KeyError(k))` is `repr(k)`. Fatal load
  errors: unknown canonical printing, unknown `card_type`/`color`, unparseable link.
- **H17 `@cache` singletons** — `get_card_db`, `construction_rules`, `banlist`, `versions`,
  `code_digest`, `get_registry`; tests mutate the registry via `override_script`
  (`effects/registry.py:209-220`).
- **H18 stdout encoding** — messages contain non-ASCII (`Zaku Ⅱ`, `【Pilot】`); Python prints with
  the locale encoding (UTF-8 on macOS/Linux); a non-UTF-8 console would raise.
- **H19 Regex engine** — Python `re` (backtracking, leftmost-first) vs Rust `regex`
  (leftmost-first, same captures for these patterns, none use backrefs/lookaround), but `\s`,
  `\d` class contents differ per H3/H4; spell the Python classes explicitly.

---

## 11. Verified sample: 12-violation deck (§4)

Input lines: `5 GD01-001`, `5 GD01-020`, `3 ST02-016`, `1 ST01-010`, `1 ST05-010`,
`4 GD01-035`, `1 GD01-060`, `2 T-001`, `1 EXB-001`, `1 GD01-018`, `3 R-001`, `1 EXR-001`.

```
error: /…/legal_errs.txt: 12 violation(s)
  [MAIN_SIZE] the deck has 24 cards; it must have exactly 50 (rule 6-1-1)
  [RESOURCE_SIZE] the resource deck has 4 cards; it must have exactly 10 Resource cards (rule 6-1-1)
  [CARD_TYPE] EXB-001 EX Base is a token (EX BASE); tokens are prepared outside the game and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)
  [CARD_TYPE] T-001 Gundam is a token (UNIT TOKEN); tokens are prepared outside the game and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)
  [CARD_TYPE] EXR-001 EX Resource is a token (EX RESOURCE); tokens are prepared outside the game and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)
  [COPY_LIMIT] 5 copies of GD01-001 Gundam; at most 4 copies of cards with the same card number (all printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)
  [COPY_LIMIT] 5 copies of GD01-020 Anksha; at most 4 copies of cards with the same card number (all printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)
  [COLORS] the deck uses 4 colors (Blue x15, Green x4, Purple x1, Red x1); a deck must be constructed entirely using either one or two card colors (rule 6-1-1-2). Cards of the extra color(s): GD01-060 Zaku Mariner, ST05-010 Mikazuki Augus
  [BANNED] GD01-020 Anksha is banned: no copies may be in a deck (Banned & Restricted list effective 2026-09-25)
  [RESTRICTED] 3 copies of ST02-016 Corsica Base; it is restricted to 2 copies (Banned & Restricted list effective 2026-09-25). Exception 'st02-unmodified-starter-deck' is not applied: the simulator cannot verify an unmodified starter deck.
  [BANNED_PAIR] ST01-010 Amuro Ray, ST05-010 Mikazuki Augus are a banned pair and cannot be in the same deck (Banned & Restricted list effective 2026-09-25)
  [ATTRIBUTE_PAIR] GD01-035 Zaku Ⅱ, GD01-060 Zaku Mariner are each "a Unit card that is Lv.2 with cost 1, 2 AP, and 2 HP, and without effects"; every combination of two such card numbers is a banned pair, so a deck may contain only one of them, at most 4 copies (Banned & Restricted list effective 2026-09-25). Exception 'st05-unmodified-starter-deck' is not applied: the simulator cannot verify an unmodified starter deck.
```

Note the CARD_TYPE order: main numbers sorted (`EXB-001` < `T-001`), then resource numbers.

---

## 12. Open questions

1. `code_digest` hashes the Python package files (`reports/versions.py:43-52`) and lands in
   `results.json` `versions` and `data status --json`. A Rust binary cannot reproduce it
   unless it embeds the digest string of a pinned Python tree or hashes an embedded copy of
   those 122 files. Which does the port lead want for "byte-identical results.json"?
   Same for `package` (`importlib.metadata`, "0.1.0").
2. Should the Rust parser match Python's exotic-input behaviour (H2–H6: extra line separators,
   U+001C–U+001F whitespace, non-ASCII digits, >4300-digit counts crashing, NFKC/casefold
   Unicode 17 tables), or is ASCII/UTF-8 "normal" input the parity target?
3. Python 3.15's `pathlib` path normalisation in user-visible messages (H12): replicate
   (`./a//b` → `a/b`) or accept raw-path display in Rust?
4. Synthetic `TOKEN:` defs never occur with the current data, and the `field_overrides`
   nested per-number form and `card_number` (singular) resolution key are unused. Port them for
   future data refreshes, or assert their absence?
5. `UNIMPLEMENTED` and membership-note messages are unreachable with the current data
   (100% coverage; predicate == enumeration), so they could only be checked by reading the code.
6. Not verified on a non-macOS host: `OSError.strerror` texts (`No such file or directory`,
   `Is a directory`) come from the C library and can differ by platform.
