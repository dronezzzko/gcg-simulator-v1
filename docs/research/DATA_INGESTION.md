# gcg-api snapshot: ingestion profile

Profile of the pinned card data, written so the card model (`CardDef`) can be frozen.
Every number below comes from `docs/research/data_profile.json`, which is regenerated
deterministically (sorted keys, no timestamps) by:

<!-- doc-check: skip needs a full git clone of gcg-api at f57b7c0 (network); the refresh skill runs it -->
```bash
uv run python scripts/ingest_profile.py --full-clone <gcg-api clone at f57b7c0>
# provenance fragment as well:
uv run python scripts/ingest_profile.py --full-clone <clone> \
    --sources-out data/sources.d/gcgapi.json --retrieved-at 2026-09-25T12:40:00Z
```

Without `--full-clone` the script profiles only the packaged copy and sets `full_clone` to null.
It checks the normalization contract below against 14 fixed examples before profiling and fails
if one no longer holds.

| Snapshot | Value |
| --- | --- |
| Upstream | https://github.com/yzRobo/gcg-api |
| Pinned data commit | `f57b7c0b0ebc4c13d359649de19750c793ecbefc` ("data: weekly refresh (run 25)", 2026-09-21T12:12:05Z) |
| `dataset_version` | `25-676f1bf3b752718dab59923a855819aad85ec361` (the sha is the code commit that *produced* the data, per upstream README) |
| `built_at` | 2026-09-21T12:11:55.812Z |
| Retrieved | `git clone` at 2026-09-25T12:40:00Z (git reflog) |
| Packaged copy | `src/gcg_sim/data/gcgapi/`: 9 files, all byte-identical to the clone and to the raw.githubusercontent.com URLs at the pin |

## 1. Verified figures

| Figure | Value | Note |
| --- | --- | --- |
| Printings (records, unique `product_id`) | 1,912 | 767 are alt printings (`_pN`) |
| Distinct card numbers | 1,148 | `R-001`, `EXB-001`, `EXR-001` have no base printing (only `_pN` records) |
| Rulings (`rulings.json`) | 368 | all 368 have non-empty `answer`; 282 card numbers; `num` values unique |
| Rules FAQ (`rules-faq.json`) | 119 | 33 categories, all answers non-empty |
| Errata (`errata.json`) | 2 | both `applied`, verified (see §6) |
| Products (`products.json`) | 45 | 1 has `category_tag` null |
| Sets (`sets/en/index.json`) | 28 | card counts sum to 1,912 and match per-`set_code` printings |
| Distinct raw `effect` strings | 817 (printings), 782 (one per card number) | |
| Distinct effect strings, level 1 (typographic) | 811 / 782 | |
| Distinct effect strings, level 2 (level 1 + reminder text removed) | 788 / 776 | includes `-` (vanilla) and `""` (Resources, EX tokens) |
| Card numbers whose printings differ in `effect` | 35 raw, 29 level 1, 11 level 2 | |
| Card numbers whose printings differ in any gameplay field | 43 | §5 |
| `manifest.json` counts | all 6 match the files | |

"One per card number" uses the base printing (`product_id == card_number`); for the three card
numbers without one it uses the lowest `_pN` that is not Edition Beta (`R-001_p6`, `EXB-001_p6`,
`EXR-001_p6`).

### Per card type

| card_type | printings | card numbers |
| --- | --- | --- |
| UNIT | 1,000 | 600 |
| PILOT | 261 | 127 |
| COMMAND | 287 | 157 |
| BASE | 105 | 72 |
| RESOURCE | 153 | 107 |
| UNIT TOKEN | 44 | 29 |
| EX BASE | 32 | 29 |
| EX RESOURCE | 30 | 27 |

### Per set_code and per card-number prefix

`set_code` is the product a printing came from, not the card's own set: 219 printings have a
`set_code` different from their card-number prefix (reprints in SC01/ST09/GD05 and so on). Use
`card_number` for identity (rule 2-1-1).

| key | by `set_code`: printings / card numbers | by card-number prefix: printings / card numbers |
| --- | --- | --- |
| GD01 | 254 / 139 | 268 / 130 |
| GD02 | 226 / 141 | 209 / 130 |
| GD03 | 242 / 157 | 213 / 132 |
| GD04 | 205 / 149 | 175 / 130 |
| GD05 | 203 / 148 | 185 / 130 |
| EB01 | 108 / 92 | 106 / 90 |
| ST01 | 57 / 19 | 62 / 16 |
| ST02 | 47 / 18 | 50 / 16 |
| ST03 | 46 / 18 | 52 / 16 |
| ST04 | 50 / 19 | 58 / 16 |
| ST05 | 40 / 16 | 46 / 15 |
| ST06 | 42 / 16 | 40 / 15 |
| ST07 | 40 / 16 | 39 / 15 |
| ST08 | 34 / 16 | 33 / 15 |
| ST09 | 39 / 26 | 20 / 10 |
| ST10 | 35 / 19 | 33 / 16 |
| ST11 | 17 / 17 | 16 / 16 |
| ST12 | 16 / 16 | 16 / 16 |
| ST13 | 17 / 17 | 16 / 16 |
| ST14 | 16 / 16 | 16 / 16 |
| SC01 | 51 / 51 | (no own numbers) |
| RP | 66 / 66 | 66 / 66 |
| R | 3 / 1 | 87 / 41 |
| T | 15 / 12 | 44 / 29 |
| EXB | 2 / 1 | 6 / 3 |
| EXBP | 23 / 23 | 26 / 26 |
| EXR | 2 / 1 | 14 / 11 |
| EXRP | 16 / 16 | 16 / 16 |

## 2. Bulk-file equivalence (full clone)

| Check | Result |
| --- | --- |
| `data/cards.json` vs `cards.ndjson` | 1,912 records, identical and in the same order |
| `data/cards/en/*.json` (28 files) vs `cards.ndjson` | 1,912 records, same multiset of records; every record sits in the file named after its `set_code` |
| Packaged files vs clone | all 9 byte-identical (sha256) |

No differences, so leaving `cards.json` and `cards/en/*.json` out of the package loses nothing.

## 3. Effect-text normalization

Keep the raw `effect` as the source of truth. The normalized forms are comparison keys, for
dedup, divergence detection, and matching text templates.

**Level 1 (typographic):** fold full-width ASCII forms U+FF01–U+FF5E to ASCII (brackets, the
`：` cost colon, digits); map `・` (U+30FB) to `･` (U+FF65, the majority form, also used in the
rules); curly quotes to straight; delete U+200B/U+FEFF; U+3000/U+00A0 to space; close spaced
apostrophes before `s`/`t` (`owner' s` → `owner's`, `can' t` → `can't`; `players' hands` is
left alone); trim spaces just inside `"..."`; unify line breaks, collapse space runs, trim
lines, drop blank lines; drop spaces after a line-leading chain of `【...】` markers. The
lenticular brackets `【】` and circled cost digits `①②④` are kept.

**Level 2 (level 1 plus reminder text removed, rule 2-11-4):** remove every balanced top-level
`(...)` group that meets all of these conditions:
- not inside `[...]`, which protects names such as `[Zeong (Head)]`, `[Gaia (GQ)]`, `[Amate Yuzuriha (Machu)]`;
- not inside `【...】`, which protects `【When Paired･(Newtype) Pilot】`;
- not directly after `]`, which protects token definitions such as `[Hy-Gogg]((Cyclops Team)･AP2･HP1)`;
- not an exact trait from the 78-trait vocabulary, which protects `(Dawn of Fold)`;
- contains at least one word starting with a lowercase letter.

After removal, whitespace is tidied again. Two kinds of non-trait parenthetical survive on
purpose: `(AP3･HP3)` (GD05-089 Master Asia deploys itself with these stats) and
`(Dianna Counter)` (a trait reference that no card carries yet).

Removed in this snapshot: 20 distinct reminder strings. The most common are Resource
`(Rest a Resource when paying a cost.)` (153 printings), Blocker (120), Breach in three
wordings (66 + 11 + 9), Repair (63 + 1), High-Maneuver (41), and EX Base/EX Resource setup
text. Three are explanatory notes, not keyword reminders: `(Don't treat it as a Pilot.)`
(GD05-089), `(You choose first, then your opponent.)` (ST12-013) and `(This card that is being
activated is not counted.)` (ST14-014). Rule 2-11-4 says they have no game effect, but they state
intended semantics, so effect implementers should read the raw text.

## 4. Edition Beta and other printing markers

- **Edition Beta** is exactly `set_name == "Edition Beta"`, which in this snapshot equals
  `where_to_get == "Edition Beta"`: 83 printings, 70 card numbers. All are `_pN` alt printings,
  so none is a base printing. Their `set_code` is the card's own prefix (GD01, ST01–ST04, T, R,
  EXB, EXR) and their rarity carries no marker.
- `block_icon == "β"` covers those 83 plus 3 promos (`RP-001`, `EXBP-001`, `EXRP-001`, set_name
  "Promotion card"), so it over-matches. Other values: `"1"` (1,453), `"2"` (367), `"-"` (6).
- `sp == "SP"` on 28 printings (special parallels); otherwise null. `rarity` values: C, C +, C ++,
  U, U +, R, R +, LR, LR +, LR ++, LKC +, LKU +, LKR +, P. None of these affect gameplay.

## 5. Divergent printings (43 card numbers)

Rule: the base printing is canonical for every gameplay field. Categories: 18 reminder-text-only,
8 punctuation-only, 11 reworded reprint, 5 Edition Beta stats, 1 other, 0 errata-related.

| card_number | divergent printings | class | detail |
| --- | --- | --- | --- |
| EXB-001 | _p5 (Beta), _p7 (SC01) | reminder-text-only | the whole text is reminder text ("as your shield area's base" vs "into your shield area") |
| EXR-001 | _p5 (Beta), _p7 (SC01) | reminder-text-only | reminder wording ("active"; "remove" vs "exile") |
| GD01-030 | _p1 (Beta), _p2 (SC01), _p3 (promo) | reminder-text-only | Breach reminder variants |
| GD01-034, GD01-041 | _p1 (Beta) | reminder-text-only | Breach reminder "a card" vs "the first card" |
| ST02-001 | _p1 (Bonus), _p2/_p3 (Beta) | reminder-text-only | Breach reminder |
| ST02-012 | _p1 (Bonus), _p2 (Beta) | reminder-text-only (+ punctuation) | Breach reminder; _p1 `hp_raw` "1" vs "+1" (hp equal) |
| GD05-001/005/006/017/020/037/049/055/059/067/068 | the GD05 `_p1`/`_p2` parallels | reminder-text-only | the parallels omit the reminder text |
| GD01-068 | _p2 (EVX05) | punctuation-only | "owner' s" |
| GD01-081 | _p1 (Beta) | punctuation-only | whitespace |
| GD03-027 | _p1 (promo) | punctuation-only | name `Z’Gok E` vs `Z'Gok E` |
| GD03-076 | _p1 (Boost Kit) | punctuation-only | "owner' s" |
| GD03-088 | _p1 (R +) | punctuation-only | `ap_raw` "2" vs "+2" (ap equal) |
| GD03-118 | _p2, _p3 (store packs) | punctuation-only | "owner' s", `" Awakened Potential"` |
| ST02-014 | _p2, _p3 (ST08) | punctuation-only | "card' s" |
| ST07-010 | _p2, _p3 (store packs) | punctuation-only | "opponent' s" |
| GD01-005 | _p1 (GD01 R +), _p2 (Beta), _p3 (launch promo) | reworded | base `【During Link】【Destroyed】Return this Unit's paired Pilot…Then, discard 1.`; _p1/_p2 `【During Pair】【Destroyed】If this is a Link Unit, return…`; _p3 mixes both. Same intent; the base is stricter (the whole effect, including the discard, only while linked) |
| GD01-087 | _p2 (store pack) | reworded (case only) | "this unit" plus an older Repair reminder |
| GD01-088 | _p1, _p2 (Beta) | reworded | `【When Linked】Draw 1.` vs `【When Paired】If this is a Link Unit, draw 1.`, equivalent |
| GD01-089, GD01-091 | _p1 (Beta) | reworded | "While this Unit has…" vs "If this Unit has…", equivalent |
| GD01-090 | _p2 (store pack) | reworded | "AP can't be reduced by enemy effects" vs "does not decrease due to the opponent's effects", equivalent |
| ST01-015 | _p1 (Bonus), _p2 (Beta) | reworded | "no Units" vs "0 Units", equivalent |
| ST02-010 | _p1 (Bonus), _p2 (Beta) | reworded | `【During Link】This Unit gets AP+1 and HP+1.` vs "…while linked." Equivalent, but `timing_markers` loses During Link |
| ST03-011 | _p1 (Bonus) | reworded | sentence split ("also gains <High-Maneuver>"), equivalent |
| ST03-001, ST05-001 | _p2/_p3 (SC01) | reworded (line order) | same lines in a different order, equivalent |
| T-001 | _p1 (Beta) | Edition Beta stats | AP/HP null vs 3/3 |
| T-002 | _p1 (Beta) | Edition Beta stats | 3/3 vs 2/2 |
| T-003 | _p1 (Beta) | Edition Beta stats | 2/2 vs 1/1 |
| T-006 | _p1 (Beta) | Edition Beta stats | AP 1 vs 3 |
| R-001 | _p4 (Beta) | Edition Beta stats | a Resource with AP 3 / HP 1 |
| GD01-051 | _p1 (Store Tournament Participant Pack 01) | other | link `Trait [Enhanced Human]` vs `(Cyber-Newtype) Trait`; "Enhanced Human" is neither a trait nor a pilot name |

The Edition Beta stat values are not real Beta stats. Four of the five (`T-002_p1`, `T-003_p1`,
`T-006_p1`, `R-001_p4`) exactly equal the base stats of the record just before them in
`cards.ndjson` (T-001, T-002, T-003, T-006). That points to a one-row shift in the source listing
(`misc.edition_beta_stat_offsets`). The token definitions in current card text agree with the
base T-cards.

## 6. Errata

Both entries are `applied`, and every affected printing reads the corrected text:
- `T-013` trait is `(Cyclops Team)`, 1 of 1 printing.
- `GD04-067` effect says "from any player's trash", 2 of 2 printings.

No printing still has the `before` text. The token definitions that create Hy-Gogg (GD03-024,
GD03-108) already say `(Cyclops Team)`, and ruling GD04-067 Q277 ("can choose from my opponent's
trash: Yes") matches the erratum. **No errata listed in `errata.json` are missing from the current text.**
(The ST12-001 erratum of 2026-09-04 is not in gcg-api at all; it is applied through
`errata-unapplied:ST12-001:news-02_193` in `overrides.json`, see docs/CONFLICTS.md.) Upstream notes
that Bandai does not update card pages for errata, so any future erratum has to go through
`errata.json` or our `overrides.json`.

## 7. Field profile and card-model recommendations

**Identity.** Key cards by `card_number` and keep `product_id` only as a printing alias. Take
gameplay fields from the base printing (§1 rule). Never take them from Edition Beta printings.

**card_type.** Eight values: UNIT, PILOT, COMMAND, BASE, RESOURCE, UNIT TOKEN, EX BASE,
EX RESOURCE. No `UNIT・TOKEN` spellings remain.

**color.** Blue/Green/Red/White/Purple. It is null exactly for RESOURCE, UNIT TOKEN, EX BASE and
EX RESOURCE (259 printings), meaning colorless (rules 2-4-2, 5-17-2-3).

**level / cost.** Integers 1–10. They are null only for RESOURCE and tokens; treat them as 0 when
referenced (rules 2-9-3, 2-10-3, 5-17-2-4).

**AP/HP.** gcg-api turns a printed `-` into null. The data shows that a zero AP arrives the same
way (inferred, not stated upstream): no UNIT or UNIT TOKEN has `ap_raw` "0", and the evidence is
in the table.

| card_type | ap | hp | model |
| --- | --- | --- | --- |
| UNIT | int; null on 13 printings / 10 numbers (EB01-013, EB01-052, GD01-048, GD01-061, GD02-011, GD02-034, GD02-047, GD03-060, ST05-003, ST08-009) | always int | null AP = 0 |
| UNIT TOKEN | null on T-012, T-014, T-001_p1 | null only on T-001_p1 | null AP = 0: the token definitions in card text say `AP0` for Daughtress (T-012) and Ad Balloon (T-014). T-001_p1 is the Beta defect |
| BASE | always null | int 4–7 | AP = 0 (Bases have AP per 3-5-4) |
| EX BASE | null | 3 | AP 0, HP 3: rule 5-17-3-1-1 says 0 AP, and all 32 printings carry null |
| PILOT | int | int | ints are the paired-unit modifiers; `ap_raw`/`hp_raw` are "+N" except GD03-088_p1, GD05-088, GD05-091 (no "+", same value). Use the ints |
| COMMAND | int on the 89 printings with 【Pilot】, else null | same | modifiers (`ap_raw` +0/+1/+2, `hp_raw` +0/+1/+2, plus one "1") |
| RESOURCE, EX RESOURCE | null | null | no stats; `R-001_p4` (3/1) is the defect above |

**zone.** `Space`, `Earth`, `Space Earth` (both) on UNIT/BASE; `-` elsewhere. Parse into a set.

**trait / traits.** `trait` is `(A)`, `(A) (B)`, `(A) (B) (C)` or `-`. It agrees with `traits[]`
on every printing, so use `traits[]`. The vocabulary has 78 traits; one contains a lowercase word
(`Dawn of Fold`). Traits sit on UNIT/PILOT/BASE/UNIT TOKEN, on the 89 【Pilot】 Commands, and on
two plain Commands (GD05-110 Darkness Finger, GD05-120 Shining Finger, trait `(Special Move)`).

**link.** Parse the `link` string, not `link_refs`. Grammar: alternatives separated by ` / `.
Each alternative is `[Name part]` (satisfied by a pilot whose name contains it, rule 3-2-6-4) or
`(Trait) Trait`; `(A) / (B) Trait` is shorthand for trait A or trait B. Shapes in the data:
`[N]` 534, `(T) Trait` 218, `[N] / [N]` 26, `(T) Trait / [N]` 9, `(T) Trait / (T) Trait` 8,
`(T) / (T) Trait` 1, `-` 1,115, and one anomaly (`Trait [Enhanced Human]`, GD01-051_p1, not a
base printing). Only UNIT has links. `link_refs` loses the name/trait distinction. It lists names
before traits (reordered on 9 printings) and adds a spurious `Machu` for `[Amate Yuzuriha (Machu)]`
(14 printings). Name parts can contain parentheses, so strip `[...]` before scanning for
`(Trait)`, as the current `parse_link` does. `[Ericht Samaya]` (GD05-026) matches no pilot in the
snapshot; the other alternative `[Prospera Mercury]` does.

**keyword_effects.** Every entry is `{keyword, value}`, deduplicated per card. It is a mention
list and should not be read as the card's own keywords. It includes grants ("All your … Units
gain <Repair 1>"), conditions ("While this Unit has <Repair>", value null), and lists (GD04-067
names 7 keywords). On base printings, 124 keyword mentions start a line (after leading
markers), and 135 appear only inline. Vocabulary: Blocker, Breach (1–5), Repair (1–3), Support
(1–3), High-Maneuver, Suppression, First Strike. GD02-053 (both printings) writes `[Suppression]`
in square brackets, so the field misses it. Derive keywords from compiled effects instead.

**timing_markers.** A flattened, deduplicated set of 12 tokens (Burst, Main, Deploy, Action,
Attack, Once per Turn, When Paired, During Link, Activate, During Pair, When Linked, Destroyed).
It drops `【Pilot】`, `Development N`, and pairing qualifications such as
`【When Paired･Lv.4 or Higher Pilot】`, `【During Pair･(Vulture) Pilot】`,
`【When Paired･(Cyber-Newtype)/(Newtype) Pilot】` and `【During Pair･Red Pilot】`. The raw text has
40 distinct `【...】` strings after level 1. Parse the markers from the text. Compound separators
appear as both `･` and `・` in the source (`【Deploy・Development 2】`).

**Command 【Pilot】.** 89 printings / 68 card numbers. The pilot name exists only in the text,
as the last line `【Pilot】[Name]` (always the last line). Its traits sit in `trait`/`traits`, and
its AP/HP modifiers sit in `ap`/`hp` (see `ap_raw`/`hp_raw`). A Command pilot has no text of its
own. That name is a second card name (rule 3-4-6-1). GD05-104 grants a `■【During Link】【Destroyed】…`
effect from its Action text; that is not pilot text.

**Pilot text halves (rule 3-3-9).** The two texts are joined into one `effect`. On all 127 pilot
base printings the first rule line (after an alias line) is `【Burst】…`. Recommended split:
`【Burst】` lines and the alias line belong to the Pilot card (3-3-9-1); every other line is
granted to the paired Unit unless it names a location (3-3-9-2-1). This split is inferred from
the text, not encoded in the data.

**Multiple names (rule 2-2-4).** Three pilots carry "This card's name is also treated as [X]":
GD02-098 Quattro Bajeena = Char Aznable, ST12-011 Milliardo Peacecraft = Zechs Merquise, ST12-012
Ple-Twelve = Marida Cruz. The 68 【Pilot】 Commands also have a second name. `A & B` names (12,
e.g. `Garrod Ran & Tiffa Adill`) are single names; they match by substring (2-2-3, 3-2-6-4).
Model `names: tuple[str, ...]`.

**Names.** For matching, key names on NFKC plus removing U+200B plus `’`→`'`. That handles
`GD05-111` "Airframe​ Seizure", curly apostrophes in GD02-102 and GD03-027 (the promo
GD03-027_p1 uses a straight one), and `Ⅱ` (U+2161) in names such as `Char's Zaku Ⅱ`, while other
names use ASCII "II" (`Gundam Mk-II`). Token names contain `(...)` and ` / ` (`Zeong (Head)`,
`GQuuuuuuX (Omega Psycommu)`, `Bit / Funnel`), so never split names on `/`.

**Tokens.** Text defines tokens as `[Name]((Trait)…･APn･HPn[･ability])`, in 29 distinct
definitions. Each matches exactly one UNIT TOKEN card number (T-001…T-029) by name, and every
T-card is defined somewhere. After reading null AP as 0 and removing reminder text, all
definitions equal the T-card base printing: traits, AP, HP, and abilities (`<Blocker>`,
`<Breach 1>`, "can't be paired with a Pilot", and so on). The only mismatches are the Beta
printings T-001_p1, T-002_p1, T-003_p1 and T-006_p1. Build token defs from the T-card base
printing and assert them against the text definitions. The dot before an ability can be `・`
(Wire-Guided Arm). Token level/cost count as 0 and tokens are colorless (5-17-2).

**EX tokens.** EX BASE (29 card numbers / 32 printings) and EX RESOURCE (27 / 30) are art
variants with reminder-only text. Model them as the two rules objects of 5-17-3. "EX Resource"
appears in the text of 25 card numbers (placing or using EX Resources), and "EX Base" in
1 (GD04-110).

**RESOURCE.** 107 card numbers / 153 printings. Every effect is the reminder
`(Rest a Resource when paying a cost.)`, which is empty after level 2. No Resource has an ability.

**Vanilla.** `effect == "-"` on 125 UNIT and 20 UNIT TOKEN card numbers (182 + 31 printings).
Level 2 keeps `-`; map it to "no abilities".

**Cosmetic fields.** Ignore `rarity`, `sp`, `block_icon`, `where_to_get`, `source_title`,
`image_url` and `detail_url` for gameplay. Use `set_name`/`where_to_get` only to recognize
Edition Beta.

## 8. Side-file notes

- `rulings.json` stores the answer text for all 368 rulings. Upstream `MAINTENANCE.md` still says
  answers are deliberately not stored. The data is fine; only upstream's docs are out of date.
  Rulings are keyed by `card_number` + `num`, and `num` is unique across the file.
- `products.json` has one entry with `category_tag` null. Nothing gameplay-relevant.
