# gcg-api snapshot: ingestion profile

Profile of the pinned card data, written so the card model (`CardDef`) can be frozen.
Every number below comes from `docs/research/data_profile.json`, which is regenerated
deterministically (sorted keys, no timestamps) by:

<!-- doc-check: skip needs a full git clone of gcg-api at 78a64df (network); the refresh skill runs it -->
```bash
uv run python scripts/ingest_profile.py --full-clone <gcg-api clone at 78a64df>
# provenance fragment as well:
uv run python scripts/ingest_profile.py --full-clone <clone> \
    --sources-out data/sources.d/gcgapi.json --retrieved-at 2026-11-06T05:39:48Z
```

Without `--full-clone` the script profiles only the packaged copy and sets `full_clone` to null.
It checks the normalization contract below against 14 fixed examples before profiling and fails
if one no longer holds. Card numbers that the profile counts but does not list (for example the
Base token T-037 in §7) were looked up in the packaged `cards.ndjson`.

| Snapshot | Value |
| --- | --- |
| Upstream | https://github.com/yzRobo/gcg-api |
| Pinned data commit | `78a64df3519cb6c773b3d2f4a8ecad14d8096b0e` ("data: weekly refresh (run 31)", 2026-11-02T14:57:50Z) |
| `dataset_version` | `31-ebd08a4b4f8c8898bc407498229839548fb911e9` (the sha is the code commit that *produced* the data, per upstream README) |
| `built_at` | 2026-11-02T14:57:40.351Z |
| Retrieved | `git clone` at 2026-11-06T05:39:48Z (`retrieved_at` of `gcgapi:commit` in `data/sources.d/gcgapi.json`; the clone's reflog says 05:39:50Z) |
| Packaged copy | `src/gcg_sim/data/gcgapi/`: 9 files, all byte-identical (sha256) to the clone they were installed from; the raw.githubusercontent.com URLs were last compared at f57b7c0 |

## 1. Verified figures

| Figure | Value | Note |
| --- | --- | --- |
| Printings (records, unique `product_id`) | 2,205 | 918 are alt printings (`_pN`) |
| Distinct card numbers | 1,290 | `R-001`, `EXB-001`, `EXR-001` have no base printing (only `_pN` records) |
| Rulings (`rulings.json`) | 437 | all 437 have non-empty `answer`; 334 card numbers; `num` values unique |
| Rules FAQ (`rules-faq.json`) | 119 | 33 categories, all answers non-empty |
| Errata (`errata.json`) | 2 | both `applied`, verified (see §6) |
| Products (`products.json`) | 53 | 1 has `category_tag` null |
| Sets (`sets/en/index.json`) | 29 | card counts sum to 2,205 and match per-`set_code` printings |
| Distinct raw `effect` strings | 953 (printings), 895 (one per card number) | |
| Distinct effect strings, level 1 (typographic) | 947 / 895 | |
| Distinct effect strings, level 2 (level 1 + reminder text removed) | 902 / 889 | includes `-` (vanilla) and `""` (Resources, EX tokens) |
| Card numbers whose printings differ in `effect` | 59 raw, 53 level 1, 13 level 2 | |
| Card numbers whose printings differ in any gameplay field | 66 | §5 |
| `manifest.json` counts | all 6 match the files | |

"One per card number" uses the base printing (`product_id == card_number`); for the three card
numbers without one it uses the lowest `_pN` that is not Edition Beta (`R-001_p6`, `EXB-001_p6`,
`EXR-001_p6`).

### Per card type

| card_type | printings | card numbers |
| --- | --- | --- |
| UNIT | 1,174 | 679 |
| PILOT | 304 | 148 |
| COMMAND | 330 | 178 |
| BASE | 124 | 82 |
| RESOURCE | 155 | 108 |
| UNIT TOKEN | 53 | 36 |
| EX BASE | 33 | 30 |
| EX RESOURCE | 32 | 29 |

### Per set_code and per card-number prefix

`set_code` is the product a printing came from, not the card's own set: 232 printings have a
`set_code` different from their card-number prefix (reprints in SC01/ST09/GD05 and so on). Use
`card_number` for identity (rule 2-1-1).

| key | by `set_code`: printings / card numbers | by card-number prefix: printings / card numbers |
| --- | --- | --- |
| GD01 | 257 / 139 | 271 / 130 |
| GD02 | 226 / 141 | 210 / 130 |
| GD03 | 242 / 157 | 213 / 132 |
| GD04 | 210 / 149 | 180 / 130 |
| GD05 | 216 / 148 | 202 / 130 |
| GD06 | 191 / 143 | 178 / 130 |
| EB01 | 108 / 92 | 106 / 90 |
| ST01 | 57 / 19 | 62 / 16 |
| ST02 | 47 / 18 | 50 / 16 |
| ST03 | 49 / 18 | 55 / 16 |
| ST04 | 50 / 19 | 58 / 16 |
| ST05 | 40 / 16 | 46 / 15 |
| ST06 | 42 / 16 | 40 / 15 |
| ST07 | 41 / 16 | 40 / 15 |
| ST08 | 34 / 16 | 33 / 15 |
| ST09 | 41 / 26 | 22 / 10 |
| ST10 | 35 / 19 | 33 / 16 |
| ST11 | 35 / 17 | 34 / 16 |
| ST12 | 34 / 16 | 34 / 16 |
| ST13 | 33 / 17 | 32 / 16 |
| ST14 | 32 / 16 | 32 / 16 |
| SC01 | 51 / 51 | (no own numbers) |
| RP | 68 / 67 | 68 / 67 |
| R | 3 / 1 | 87 / 41 |
| T | 17 / 13 | 54 / 37 |
| EXB | 2 / 1 | 6 / 3 |
| EXBP | 24 / 24 | 27 / 27 |
| EXR | 2 / 1 | 14 / 11 |
| EXRP | 18 / 18 | 18 / 18 |

## 2. Bulk-file equivalence (full clone)

| Check | Result |
| --- | --- |
| `data/cards.json` vs `cards.ndjson` | 2,205 records, identical and in the same order |
| `data/cards/en/*.json` (29 files) vs `cards.ndjson` | 2,205 records, same multiset of records; every record sits in the file named after its `set_code` |
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
- not an exact trait from the 84-trait vocabulary, which protects `(Dawn of Fold)`;
- contains at least one word starting with a lowercase letter.

After removal, whitespace is tidied again. One kind of non-trait parenthetical survives on
purpose: `(AP3･HP3)` (GD05-089 Master Asia deploys itself with these stats; 2 printings).
`(Dianna Counter)`, the unknown-trait reference that also survived at f57b7c0, is now in the
trait vocabulary: GD06 cards and the Gozzo token (T-030) carry it.

Removed in this snapshot: 20 distinct reminder strings. The most common are Resource
`(Rest a Resource when paying a cost.)` (154 printings), Blocker (136), Breach in three
wordings (66 + 21 + 9), Repair (69 + 1), High-Maneuver (50), and EX Base/EX Resource setup
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
  "Promotion card"), so it over-matches. Other values: `"1"` (1,453), `"2"` (660), `"-"` (6).
- `sp == "SP"` on 33 printings (special parallels); otherwise null. `rarity` values: C, C +, C ++,
  U, U +, R, R +, LR, LR +, LR ++, LKC +, LKU +, LKR +, P, P +. None of these affect gameplay.

## 5. Divergent printings (66 card numbers)

Rule: the base printing is canonical for every gameplay field. Categories: 40 reminder-text-only,
8 punctuation-only, 13 reworded reprint, 5 Edition Beta stats, 0 other, 0 errata-related. The one
"other" at f57b7c0 (GD01-051_p1, link `Trait [Enhanced Human]`) now reads `(Cyber-Newtype) Trait`
like its base printing.

| card_number | divergent printings | class | detail |
| --- | --- | --- | --- |
| EXB-001 | _p5 (Beta), _p7 (SC01) | reminder-text-only | the whole text is reminder text ("as your shield area's base" vs "into your shield area") |
| EXR-001 | _p5 (Beta), _p7 (SC01) | reminder-text-only | reminder wording ("active"; "remove" vs "exile") |
| GD01-030 | _p1 (Beta), _p2 (SC01), _p3 (promo) | reminder-text-only | Breach reminder variants |
| GD01-034, GD01-041 | _p1 (Beta) | reminder-text-only | Breach reminder "a card" vs "the first card" |
| ST02-001 | _p1 (Bonus), _p2/_p3 (Beta) | reminder-text-only | Breach reminder |
| ST02-012 | _p1 (Bonus), _p2 (Beta) | reminder-text-only (+ punctuation) | Breach reminder; _p1 `hp_raw` "1" vs "+1" (hp equal) |
| GD05-001/005/006/017/020/037/049/055/059/067/068 | the GD05 `_p1`/`_p2` parallels | reminder-text-only | the parallels omit the reminder text |
| GD06-001/018/029/034/037/038/052/053/054/055/064/065/066/069/081 | the GD06 `_p1`/`_p2` parallels | reminder-text-only | the parallels omit the reminder text |
| ST12-002/004/006/007/013, ST13-007/010 | the ST12/ST13 `_p1` parallels | reminder-text-only | the parallels omit the reminder text (on ST12-013, the note `(You choose first, then your opponent.)`) |
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
| GD06-084 | _p1 (GD06 R +), _p2 (release event) | reworded (spacing only) | _p1 omits the Breach reminder; _p2 has the space in `of <Breach 1>` that the base lacks (`of<Breach 1>`; level 1 does not insert it), equivalent |
| RP-068 | _p1 (1st Anniversary Set) | reworded | `-` instead of the Resource reminder, equivalent (no ability either way) |
| ST01-015 | _p1 (Bonus), _p2 (Beta) | reworded | "no Units" vs "0 Units", equivalent |
| ST02-010 | _p1 (Bonus), _p2 (Beta) | reworded | `【During Link】This Unit gets AP+1 and HP+1.` vs "…while linked." Equivalent, but `timing_markers` loses During Link |
| ST03-011 | _p1 (Bonus) | reworded | sentence split ("also gains <High-Maneuver>"), equivalent |
| ST03-001, ST05-001 | ST03-001 _p2 (SC01) and _p3 (1st Anniversary Set); ST05-001 _p3 (SC01) | reworded (line order) | same lines in a different order, equivalent |
| T-001 | _p1 (Beta) | Edition Beta stats | AP/HP null vs 3/3 |
| T-002 | _p1 (Beta) | Edition Beta stats | 3/3 vs 2/2 |
| T-003 | _p1 (Beta) | Edition Beta stats | 2/2 vs 1/1 |
| T-006 | _p1 (Beta) | Edition Beta stats | AP 1 vs 3 |
| R-001 | _p4 (Beta) | Edition Beta stats | a Resource with AP 3 / HP 1 |

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
(The ST12-001 erratum of 2026-09-04 is still not in gcg-api at 78a64df: `errata.json` lacks it and
both printings lack the added `【Once per Turn】`. It is applied through
`errata-unapplied:ST12-001:news-02_193` in `overrides.json`, see docs/CONFLICTS.md.) Upstream notes
that Bandai does not update card pages for errata, so any future erratum has to go through
`errata.json` or our `overrides.json`.

## 7. Field profile and card-model recommendations

**Identity.** Key cards by `card_number` and keep `product_id` only as a printing alias. Take
gameplay fields from the base printing (§1 rule). Never take them from Edition Beta printings.

**card_type.** Eight values: UNIT, PILOT, COMMAND, BASE, RESOURCE, UNIT TOKEN, EX BASE,
EX RESOURCE. No `UNIT・TOKEN` spellings remain. There is no Base-token type: Axis (T-037), the
Base token that GD06-129 deploys, is typed BASE. All 36 UNIT TOKEN card numbers are `T-` cards and
T-037 is the only other one, so recognize tokens by the `T-` prefix, not by `card_type` alone.

**color.** Blue/Green/Red/White/Purple. It is null exactly for RESOURCE, UNIT TOKEN, EX BASE,
EX RESOURCE and the Base token T-037 (274 printings), meaning colorless (rules 2-4-2, 5-17-2-3).

**level / cost.** Integers 1–10. They are null only for RESOURCE and tokens (T-037 included);
treat them as 0 when referenced (rules 2-9-3, 2-10-3, 5-17-2-4).

**AP/HP.** gcg-api turns a printed `-` into null. The data shows that a zero AP arrives the same
way (inferred, not stated upstream): no UNIT or UNIT TOKEN has `ap_raw` "0", and the evidence is
in the table.

| card_type | ap | hp | model |
| --- | --- | --- | --- |
| UNIT | int; null on 14 printings / 11 numbers (EB01-013, EB01-052, GD01-048, GD01-061, GD02-011, GD02-034, GD02-047, GD03-060, GD06-032, ST05-003, ST08-009) | always int | null AP = 0 |
| UNIT TOKEN | null on T-012, T-014, T-030, T-001_p1 | null only on T-001_p1 | null AP = 0: the token definitions in card text say `AP0` for Daughtress (T-012), Ad Balloon (T-014) and Gozzo (T-030). T-001_p1 is the Beta defect |
| BASE | always null | int 2–7 | AP = 0 (Bases have AP per 3-5-4); the Base token T-037 says `AP0` in its definition |
| EX BASE | null | 3 | AP 0, HP 3: rule 5-17-3-1-1 says 0 AP, and all 33 printings carry null |
| PILOT | int | int | ints are the paired-unit modifiers; `ap_raw`/`hp_raw` are "+N" except GD03-088_p1 and GD05-091 (no "+", same value). Use the ints |
| COMMAND | int on the 102 printings with 【Pilot】, else null | same | modifiers (`ap_raw` +0/+1/+2, `hp_raw` +0/+1/+2, plus one "1") |
| RESOURCE, EX RESOURCE | null | null | no stats; `R-001_p4` (3/1) is the defect above |

**zone.** `Space`, `Earth`, `Space Earth` (both) on UNIT/BASE, except `-` on the Base token
T-037; `-` elsewhere. Parse into a set.

**trait / traits.** `trait` is `(A)`, `(A) (B)`, `(A) (B) (C)` or `-`. It agrees with `traits[]`
on every printing, so use `traits[]`. The vocabulary has 84 traits; one contains a lowercase word
(`Dawn of Fold`). Traits sit on UNIT/PILOT/BASE/UNIT TOKEN (one exception: the UNIT GD06-011 Red
Gundam has `-`), on the 102 【Pilot】 Command printings, and on six plain Commands (8 printings):
GD05-110 Darkness Finger and GD05-120 Shining Finger (`(Special Move)`), GD06-101 The Gundam Says
and GD06-114 Alphaciders (`(Alphacide)`), GD06-109 God of War Ghingnham (`(Ghingnham Fleet)`) and
GD06-116 Afterglow of Zeon (`(Zeon) (Delaz Fleet)`).

**link.** Parse the `link` string, not `link_refs`. Grammar: alternatives separated by ` / `.
Each alternative is `[Name part]` (satisfied by a pilot whose name contains it, rule 3-2-6-4) or
`(Trait) Trait`; `(A) / (B) Trait` is shorthand for trait A or trait B. Shapes in the data:
`[N]` 629, `(T) Trait` 253, `[N] / [N]` 29, `(T) Trait / [N]` 14, `(T) Trait / (T) Trait` 10,
`(T) / (T) Trait` 1, `-` 1,269. Every alternative parses (the one anomaly at f57b7c0,
`Trait [Enhanced Human]` on GD01-051_p1, is gone). Only UNIT has links. `link_refs` loses the
name/trait distinction. It lists names before traits (reordered on 14 printings) and adds a
spurious `Machu` for `[Amate Yuzuriha (Machu)]` (20 printings). Name parts can contain
parentheses, so strip `[...]` before scanning for `(Trait)`, as the current `parse_link` does.
`[Ericht Samaya]` (GD05-026) matches no pilot in the snapshot; the other alternative
`[Prospera Mercury]` does.

**keyword_effects.** Every entry is `{keyword, value}`, deduplicated per card. It is a mention
list and should not be read as the card's own keywords. It includes grants ("All your … Units
gain <Repair 1>"), conditions ("While this Unit has <Repair>", value null), and lists (GD04-067
names 7 keywords). On base printings, 142 keyword mentions start a line (after leading
markers), and 160 appear only inline. Vocabulary: Blocker, Breach (1–5), Repair (1–3), Support
(1–3), High-Maneuver, Suppression, First Strike. GD02-053 (both printings) writes `[Suppression]`
in square brackets, so the field misses it. Derive keywords from compiled effects instead.

**timing_markers.** A flattened, deduplicated set of 12 tokens (Burst, Main, Deploy, Action,
Attack, Once per Turn, When Paired, During Link, Activate, During Pair, When Linked, Destroyed).
It drops `【Pilot】`, `Development N`, and pairing qualifications such as
`【When Paired･Lv.4 or Higher Pilot】`, `【During Pair･(Vulture) Pilot】`,
`【When Paired･(Cyber-Newtype)/(Newtype) Pilot】` and `【During Pair･Red Pilot】`. The raw text has
41 distinct `【...】` strings after level 1. Parse the markers from the text. Compound separators
appear as both `･` and `・` in the source (`【Deploy・Development 2】`).

**Command 【Pilot】.** 102 printings / 77 card numbers. The pilot name exists only in the text,
as the last line `【Pilot】[Name]` (always the last line). Its traits sit in `trait`/`traits`, and
its AP/HP modifiers sit in `ap`/`hp` (see `ap_raw`/`hp_raw`). A Command pilot has no text of its
own. That name is a second card name (rule 3-4-6-1). GD05-104 grants a `■【During Link】【Destroyed】…`
effect from its Action text; that is not pilot text.

**Pilot text halves (rule 3-3-9).** The two texts are joined into one `effect`. On 146 of the 148
pilot base printings the first rule line (after an alias line) is `【Burst】…`. The other two,
GD06-086 Mashymre Cello and GD06-100 Corin Nander, open with "When playing this card from your
hand and pairing it with …, it gets cost -1." and put `【Burst】` second. Recommended split:
`【Burst】` lines and the alias line belong to the Pilot card (3-3-9-1); every other line is
granted to the paired Unit unless it names a location (3-3-9-2-1), which keeps those two
hand-cost lines on the Pilot card. This split is inferred from the text, not encoded in the data.

**Multiple names (rule 2-2-4).** Five pilots carry "This card's name is also treated as [X]":
GD02-098 Quattro Bajeena = Char Aznable, GD06-090 Vidar = Gaelio Bauduin, GD06-092 Neo Lorrnoke =
Mu La Flaga, ST12-011 Milliardo Peacecraft = Zechs Merquise, ST12-012 Ple-Twelve = Marida Cruz.
The 77 【Pilot】 Commands also have a second name. `A & B` names (12, e.g.
`Garrod Ran & Tiffa Adill`) are single names; they match by substring (2-2-3, 3-2-6-4).
Model `names: tuple[str, ...]`.

**Names.** For matching, key names on NFKC plus removing U+200B plus `’`→`'`. That handles
`GD05-111` "Airframe​ Seizure", curly apostrophes in GD02-102 and GD03-027 (the promo
GD03-027_p1 uses a straight one), and `Ⅱ` (U+2161) in names such as `Char's Zaku Ⅱ`, while other
names use ASCII "II" (`Gundam Mk-II (AEUG)`; GD06-040 is `Gundam Mk-Ⅱ`). Token names contain
`(...)` and ` / ` (`Zeong (Head)`, `GQuuuuuuX (Omega Psycommu)`, `Bit / Funnel`), so never split
names on `/`.

**Tokens.** Text defines tokens as `[Name]((Trait)…･APn･HPn[･ability])`, in 37 distinct
definitions. 36 of them match exactly one UNIT TOKEN card number (T-001…T-036) by name, and every
UNIT TOKEN card is defined somewhere. The 37th, `[Axis]((Neo Zeon)･AP0･HP5)` (a Base token from
GD06-129), has no UNIT TOKEN card: its card T-037 is typed BASE (see card_type) and matches it
(trait, AP read as 0, HP 5). After reading null AP as 0 and removing reminder text, the 36 unit
definitions equal the T-card base printing: traits, AP, HP, and abilities (`<Blocker>`,
`<Breach 1>`, "can't be paired with a Pilot", and so on). The only mismatches are the Beta
printings T-001_p1, T-002_p1, T-003_p1 and T-006_p1. Build token defs from the T-card base
printing and assert them against the text definitions. The dot before an ability can be `・`
(Wire-Guided Arm). Token level/cost count as 0 and tokens are colorless (5-17-2).

**EX tokens.** EX BASE (30 card numbers / 33 printings) and EX RESOURCE (29 / 32) are art
variants with reminder-only text, except the 1st Anniversary Set promos EXBP-035 and EXRP-017,
whose text is `-`. Model them as the two rules objects of 5-17-3. "EX Resource" appears in the
text of 25 card numbers (placing or using EX Resources), and "EX Base" in 1 (GD04-110).

**RESOURCE.** 108 card numbers / 155 printings. Every effect but one is the reminder
`(Rest a Resource when paying a cost.)` (154 printings), which is empty after level 2; RP-068_p1
prints `-` instead (§5). No Resource has an ability.

**Vanilla.** `effect == "-"` on the base printing of 140 UNIT and 23 UNIT TOKEN card numbers and
of T-037, EXBP-035 and EXRP-017 (166 card numbers). By printing: 206 UNIT, 34 UNIT TOKEN, and one
each of BASE, EX BASE, EX RESOURCE and RESOURCE (RP-068_p1). Level 2 keeps `-`; map it to "no
abilities".

**Cosmetic fields.** Ignore `rarity`, `sp`, `block_icon`, `where_to_get`, `source_title`,
`image_url` and `detail_url` for gameplay. Use `set_name`/`where_to_get` only to recognize
Edition Beta.

## 8. Side-file notes

- `rulings.json` stores the answer text for all 437 rulings. Upstream `MAINTENANCE.md` still says
  (at 78a64df) that answers are deliberately not stored. The data is fine; only upstream's docs are
  out of date. Rulings are keyed by `card_number` + `num`, and `num` is unique across the file.
- `products.json` has one entry with `category_tag` null. Nothing gameplay-relevant.
