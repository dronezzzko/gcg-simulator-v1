# Comprehensive Rules Index

A machine-readable index of the Gundam Card Game Comprehensive Rules, keyed by rule number, with
each rule classified as testable or N/A for the 1v1 engine.

| Item | Value |
| --- | --- |
| Source | `src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md` (byte-identical to the repo-root copy) |
| Version / date | **Ver. 1.9.0**, "Updated Sep 11, 2026" (parsed as `2026-09-11`) |
| sha256 | `bf2f3d5c7a1a04d9860d97a38c316309da14707f5e7012f9241885b8635a50b6` |
| Parser / API | `src/gcg_sim/rules/index.py` |
| Generated index | `src/gcg_sim/data/rules/rules_index.json` (schema_version 1) |
| N/A classification | `src/gcg_sim/data/rules/rules_na.json` (schema_version 1, hand-maintained) |
| Tests | `tests/rules_index/test_rules_index.py` |
| Provenance fragment | `data/sources.d/rules.json` |

## Official version check (2026-09-25)

- `https://www.gundam-gcg.com/en/rules/` (fetched 2026-09-25T12:50:42Z, sha256 `73b0f15d…c024c`):
  the COMPREHENSIVE RULES block reads "Updated September 11, 2026." and links
  `../pdf/comprehensiverules_en.pdf?260917`. The page shows no version number.
- The linked PDF (fetched 2026-09-25T12:51:13Z, sha256 `ba9c335c…3b68`, 573,021 bytes,
  Last-Modified 11 Sep 2026 03:38:04 GMT, 35 pages) reads **"Ver. 1.9.0 / Updated Sep 11, 2026"**.
  **No newer version is published; the local 1.9.0 file is current.** The PDF was kept only in the
  scratchpad.
- A word-level comparison of the PDF text with the markdown found **no differences in rule text**.
  The markdown leaves out only the PDF's closing revision history. The 1.9.0 entry of that history
  reads: 3-3-4 and 13-1-5-4 revised; 5-22-4, 5-22-4-1 and 12-3-15 added.

## Usage

```bash
uv run python -m gcg_sim.rules.index --write   # regenerate rules_index.json (deterministic)
uv run python -m gcg_sim.rules.index --check   # exit 1 if rules_index.json is stale
```

```python
from gcg_sim.rules.index import load_rules_index

rules = load_rules_index()          # parses the packaged markdown + rules_na.json (cached)
rules.version, rules.date           # "1.9.0", datetime.date(2026, 9, 11)
rules["7-5-2-2-3"].text             # rule text, markdown escapes removed
rules.children("7-2-3")             # ("7-2-3-1", "7-2-3-2"), in document order
rules.ids()                         # all 579 ids in document order (= numeric order)
rules.testable_ids()                # the 399 ids that need a @pytest.mark.rule test
rules.na_reason("12-2-7")           # reason string, or None when the rule is testable
```

Each entry has these fields: `id`, `kind` (`section` / `heading` / `rule`), `title` (sections and
headings only; for those, `text` repeats the title), `text`, `section` (the top-level number),
`section_title` (the nearest titled section or heading at or above the rule), `parent_id`, `depth`,
`examples` (the `> Ex:` lines, prefix removed), `line_no` and `feature_area`. `rules_index.json`
adds `children`, `status` (`na` / `testable`) and `na_reason` for each rule, plus counts. Editing
`rules_na.json` makes the index stale, so run `--write` afterwards; `--check` and the tests catch a
stale index.

The parser is strict: any unrecognised line after `## Comprehensive Rules`, a duplicate id, a
missing parent, or an N/A entry for an unknown id raises `RulesIndexError`. If a future rules
update changes the file format, the refresh therefore fails loudly and never drops rules without
notice.

## Counts per section

| Section | Title | Entries | Sections/headings | Numbered rules | N/A | Testable |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | Game Overview | 21 | 4 | 17 | 5 | 16 |
| 2 | Card Information | 61 | 17 | 44 | 25 | 36 |
| 3 | Card Types | 52 | 6 | 46 | 6 | 46 |
| 4 | Game Locations | 50 | 10 | 40 | 10 | 40 |
| 5 | Essential Game Terminology | 98 | 25 | 73 | 29 | 69 |
| 6 | Preparing to Play | 26 | 3 | 23 | 6 | 20 |
| 7 | Game Progression | 47 | 17 | 30 | 18 | 29 |
| 8 | Attacking and Battles | 36 | 8 | 28 | 8 | 28 |
| 9 | Action Steps | 11 | 1 | 10 | 1 | 10 |
| 10 | Effect Activation and Resolution | 54 | 9 | 45 | 10 | 44 |
| 11 | Rules Management | 20 | 6 | 14 | 6 | 14 |
| 12 | Multiplayer Battle | 32 | 1 | 31 | 32 | 0 |
| 13 | Keyword Effects and Keywords | 71 | 24 | 47 | 24 | 47 |
| **Total** | | **579** | **131** | **448** | **180** | **399** |

Structural checks, all enforced by tests: document order equals numeric order; every parent
exists; the children of every entry are numbered 1..n with no gaps; heading levels match depth
(`####` = depth 2, `#####` = 3, `######` = 4). Eight rules carry examples: 1-3-2-1, 3-2-6-4,
5-19-1, 13-1-1-2, 13-1-2-5, 13-1-3-2, 13-2-10-2 and 13-2-12-2.

## N/A policy and results (180 N/A, 399 testable)

The classification is deliberately conservative. A rule is N/A only in these cases:

| N/A category | Count |
| --- | ---: |
| multiplayer (section 12): each rule has its own reason naming the 1v1 counterpart | 32 |
| grouping heading: section or heading titles (every one has children) | 130 |
| grouping lead-in: bold rules that only introduce a list of children | 6 |
| presentational / physical: no digital or engine-observable counterpart | 8 |
| informational: "typically" / "mainly" statements with no universal claim | 4 |
| **Total N/A** | **180** |

Everything with engine-observable behaviour stays testable. That includes deck construction,
preparing to play (including the die-roll/RPS winner choice in 6-2-1-4), public and private
information, concession (1-2-4, 1-2-5), tokens (all of 5-17 apart from its two lead-ins), counter
removal, and plain definitions such as 2-2-1 and 10-1-1 that the card-data and compiler tests can
cite. The test `test_engine_observable_rules_stay_testable` guards a sample of these rules.

N/A rules that are neither headings nor multiplayer:

| Rule | Reason |
| --- | --- |
| 1-2-2 | grouping lead-in ('The following are all game conditions that result in defeat'); covered by children 1-2-2-1..1-2-2-2 |
| 2-5-4 | informational: says which card types typically print traits; trait behaviour is covered by 2-5-1..2-5-3 and 2-5-5 |
| 2-6-2 | informational: zones are 'mainly' printed on Unit and Base cards; zone behaviour is covered by 2-6-1 |
| 2-7-2 | informational: AP is 'mainly' printed on Unit and Base cards; AP behaviour is covered by 2-7-1, 2-7-3 and 3-2-5-1 |
| 2-8-3 | informational: HP is 'mainly' printed on Unit and Base cards; HP behaviour is covered by 2-8-2, 2-8-4 and 3-2-5-2 |
| 2-13-1 | presentational: card artwork has no game effect |
| 2-14-1 | presentational: the illustrator's name has no game effect |
| 2-15-1 | presentational: the copyright notice has no game effect |
| 2-16-1 | presentational: rarity has no game effect (printings of one card number are the same card, 2-1-1) |
| 5-2-3 | post-game physical cleanup (cards returned to their owners); no in-game state |
| 5-17-2 | grouping lead-in ('Tokens have the following rules'); covered by children 5-17-2-1..5-17-2-5 |
| 5-17-3 | grouping lead-in naming the two start-of-game tokens; covered by children 5-17-3-1..5-17-3-2 and setup rules 6-2-3 and 6-2-4 |
| 5-18-2 | physical: which object (chips or a die) marks counters; the engine stores damage counters as an integer (behaviour covered by 5-5-1-1, 5-18-1 and 5-18-3) |
| 6-1-2-1 | physical: bring the token cards your deck's effects need; the simulator creates tokens from card data on demand (token behaviour covered by 5-17) |
| 6-2-1 | grouping lead-in ('each player follows the steps listed below'); covered by children 6-2-1-1..6-2-1-7 |
| 6-2-1-4-1 | physical: dice must be acceptable to both players; the simulator decides Player One with its seeded PRNG (6-2-1-4) |
| 7-5-2-2 | grouping lead-in ('follow the steps listed below'); covered by children 7-5-2-2-1..7-5-2-2-4 |
| 10-3-1 | grouping lead-in ('follow the steps listed below'); covered by children 10-3-1-1..10-3-1-4 |

Rules deliberately left testable although they touch the physical game: 4-2-3 and 4-3-3 (cards
"move one at a time" but count as simultaneous), 5-4-1-1 and 5-4-1-2 (vertical/horizontal is the
active/rested state), 5-5-1 and 5-5-1-1 (damage counters are the damage state), and 6-2-2 (the
overlapping placement defines shield order).

## Testable rules by feature area

`A..B` means every **testable** rule from A to B in document order. N/A rules in between are
excluded, and every rule in a range has the listed area.

| Feature area | Testable | Rule ids |
| --- | ---: | --- |
| fundamentals | 9 | 1-1-1, 1-3-1..1-3-6 |
| win-loss | 7 | 1-2-1..1-2-5, 7-3-1-1 |
| card-info | 26 | 2-1-1, 2-2-1..2-4-2-1, 2-5-1..2-5-3, 2-6-1..2-7-1, 2-8-1..2-8-2, 2-9-1..2-9-2, 2-9-4..2-10-2, 2-11-1..2-11-2, 2-11-4 |
| card-types | 18 | 3-1..3-2-1, 3-2-3..3-2-5-2, 3-4-1, 3-4-3..3-4-5, 3-4-7..3-5-1, 3-5-3..3-6-1 |
| pairing-link | 31 | 2-4-3, 2-5-5, 2-7-3, 2-8-4, 2-11-3, 2-12-1..2-12-2, 3-2-6..3-3-1, 3-3-3..3-3-9-2-1, 3-4-6..3-4-6-4, 5-9-1 |
| deck-construction | 13 | 2-1-2, 3-2-2, 3-3-2, 3-4-2, 3-5-2, 3-6-2, 6-1-1..6-1-1-5 |
| setup | 13 | 6-1-2..6-2-5 |
| zones | 24 | 4-1-1..4-1-2, 4-1-5..4-1-6, 4-2-1, 4-2-3..4-3-1, 4-3-3..4-4-2-1, 4-5-1, 4-5-4, 4-6-1..4-6-3, 4-6-4-2..4-7-1, 4-8-1, 4-8-4..4-9-1 |
| information | 16 | 4-1-3..4-1-4, 4-1-7, 4-2-2, 4-3-2, 4-4-3, 4-5-2..4-5-3, 4-5-5, 4-6-3-1..4-6-4-1, 4-7-2, 4-8-2..4-8-3, 4-9-2 |
| terminology | 26 | 5-1-1..5-4-2, 5-7-1..5-8-1, 5-10-1..5-15-1-1, 5-19-1 |
| damage | 16 | 5-5-1..5-6-3, 5-18-1..5-18-3, 5-21-1..5-21-2-1 |
| tokens | 19 | 2-9-3, 2-10-3, 5-17-1..5-17-4-4 |
| turn-structure | 26 | 7-1-1..7-3-1, 7-4-1..7-5-3-1, 7-5-5-1..7-6-2, 7-6-4-1..7-6-7 |
| battle | 33 | 5-22-1..5-22-4-1, 7-5-4-1, 8-1..8-3-5, 8-5-1..8-6-2 |
| action-step | 13 | 7-6-3-1, 8-4-1..8-4-2, 9-1..9-5 |
| effects-triggers | 48 | 1-3-7, 5-16-1, 5-20-1..5-20-2, 10-1-1..10-3-5 |
| rules-management | 14 | 11-1-1..11-5-2-1 |
| keywords | 47 | 13-1-1-1..13-2-13-2 |
| multiplayer | 0 | (none; all 32 are N/A) |

## Internal inconsistencies in the rules file

The official 1.9.0 PDF has every item below as well, so none of them comes from the markdown
conversion. **Root cause of the stale cross-references:** Ver. 1.1.0 (July 24, 2025) inserted
"3. Card Types", which moved every later section up by one. Ten references still use the old
numbers (old 6 = Game Progression → 7, old 7 = Attacking and Battles → 8, old 8 = Action Steps → 9,
old 9 = Effects → 10, old 12 = Keywords → 13). Ver. 1.3.0 "corrected errors" in 8-5-3-2-2 and
similar rules but missed these ten. The eleventh, "5-17. Counters", is off by one inside section 5
(5-17 is Token; Counter is 5-18). In each case the title quoted in the reference identifies the
intended rule unambiguously. Traceability should cite the intended rule; the source file is never
edited.

### Wrong cross-references (11 of the 34 references in the file)

| Rule | Text in the file | Intended |
| --- | --- | --- |
| 2-7-1 | "(See 7. Attacking and Battles)" (7 is Game Progression) | 8. Attacking and Battles |
| 3-4-5 | "(See 12-2. Keywords)" (12-2 is Battle royale) | 13-2. Keywords |
| 3-4-7 | "(See 12-2-5. 【Burst】)" (12-2-5 is Battle royale victory conditions) | 13-2-5. 【Burst】 |
| 5-5-1 | "(See 5-17. Counters)" (5-17 is Token) | 5-18. Counter |
| 5-10-3 | "(See 12-2-5. 【Burst】)" | 13-2-5. 【Burst】 |
| 8-4-1 | "(See 8. Action Steps)" (8 is Attacking and Battles; 8-4-1 is inside it) | 9. Action Steps |
| 8-5-2-3-1 | "(See 12-2-5. 【Burst】)" | 13-2-5. 【Burst】 |
| 8-5-2-4-2 | "(See 12-1-5. \<First Strike\>)" (12-1-5 does not exist; the sibling 8-5-3-2-2 correctly says 13-1-5) | 13-1-5. \<First Strike\> |
| 8-6-2 | "(See 6-5. Main Phase)" (6-5 does not exist) | 7-5. Main Phase |
| 13-2-1-1 | "activated effect (see 9-1-7)" (9-1-7 does not exist) | 10-1-7. Activated Effects |
| 13-2-2-1 | "activated effect (see 9-1-7)" | 10-1-7. Activated Effects |

The other 23 references all resolve correctly (checked automatically by comparing each quoted
title with the target's title).

### Typos and terminology

| Rule | Text in the file | Intended |
| --- | --- | --- |
| 11-3-1 | "the HP … becomes zero less after receiving damage" | "becomes zero or less" (matches 5-5-2 and 5-10-1: damage ≥ HP destroys) |
| 5-20-2 | "in card text text cannot be resolved" | "in card text cannot be resolved" |
| 1-3-1 | "exceptions that arise due a card's effects" | "due to a card's effects" |
| 4-1-1-2 | "When referring to them to all together" | "When referring to them all together" |
| 9-3-2 | "【Activate ･Action 】" (stray spaces) | "【Activate･Action】" |
| 13-2-11-1 | "when a pilot that meets the link condition is set" ("set" is not a defined term; also "【When Linked】is" has no space) | "…is paired" (5-9; see 3-2-6-2 and 13-2-12-1) |
| 13-1-8-1 | "you may exile (number) of (G Generation) cards in your trash from the game" ("exile" is never defined; the rules define "Remove", 5-12, into the removal area). 58 card numbers in the pinned data use "exile … from the game", and no card text says "removal area". | Treat exiling a non-token card as **remove** (5-12-1: place it in the removal area). An exiled token (for example the EX Resource reminder text "exile it from the game when paying a cost") leaves the game (5-17-3-2-3, 5-17-4) and does not go to the removal area. |

### Ambiguities and gaps the engine must decide (recorded for docs/ASSUMPTIONS.md)

| Rule(s) | Issue | Recommended engine interpretation |
| --- | --- | --- |
| 3-5-4, 3-5-4-1, 8-5-2-4, 8-5-2-4-2, 13-1-5-2 | 3-5-4 says Bases have AP, and 13-1-5-2 speaks of "battle damage from the Unit or Base targeted for attack". But 8-5-2-4 has only the attacker dealing damage to a Base, no Base in the pinned card data prints AP, and the EX Base has 0 AP (5-17-3-1-1). | Bases never deal battle damage to the attacker. First Strike against a Base changes nothing observable, apart from ordering if an effect ever gives a Base AP. |
| 10-2-2-1 vs 4-7-2 | The list of public locations whose cards can be "chosen as targets" (battle area, base section, resource area, trash) leaves out the removal area, which 4-7-2 declares public. | Follow 10-2-2-1 literally: removal-area cards are not "targets" for the 10-1-8-1-1 / 10-2-2 legality checks. |
| 11-2-1, 1-2-1 | 11-2-1 defeats every player who meets a defeat condition at the same time, but 1-2-1 assumes exactly one loser. A 1v1 double defeat (for example both decks empty after a symmetric draw effect) has no stated result. | Record a draw (no winner) and report it separately in statistics. |
| 4-4-2, 4-4-2-1 vs 11 | The resource area is capped at 15 Resources and 5 EX Resources, but section 11 has excess management only for the battle area (11-4) and the base section (11-5). The rules also leave open whether EX Resources count toward the 15. | Placing a Resource or EX Resource beyond its cap is impossible and is skipped (1-3-2). The 15 cap counts all Resources including EX. |
| 6-2-2 vs 4-6-4-1 | Shields are "placed so each overlaps the previous one, starting with the card nearest to you". Which card is the "top" Shield (4-6-4-1) depends on how the cards physically overlap. | The last card placed is on top; the Shield taken first is the 6th card drawn for shields. Hidden information makes this matter only for determinism. |

## Rules that need special engine attention

**Trigger timing and ordering**
- 10-1-6-5, 10-1-6-6, 10-1-6-7: simultaneous triggers resolve all of the active player's first (in
  the order the owner chooses), then the standby player's. A trigger that fires *during* resolution
  jumps ahead and resolves at once (nested, not queued). Rules FAQ Q109 adds that when several new
  triggers fire together, the active player's go first.
- 10-1-6-8, 10-1-6-8-1: 【Burst】 goes before every other pending trigger. New triggers during Burst
  resolution take priority over the remaining Bursts. Q110 covers Breach destroying a Burst shield.
- 10-1-6-3: several simultaneous events that meet one trigger condition trigger it **once**. For
  example, two friendly Units destroyed together trigger "when a friendly Unit is destroyed" once.
- 10-1-6-4: a trigger still resolves after its source has left its location.
  13-2-8-2 and 13-2-8-2-1: 【Destroyed】 resolves from the trash using last-known information.
- 10-1-6-1-1, 13-2-13-1, 13-2-13-2: triggers fire every time unless 【Once per Turn】, which is
  counted per card copy.
- 7-1-3, 7-2-2, 7-6-2, 7-5-1: phases and steps do not advance while triggers are pending. Main-phase
  actions are legal only when no triggered effect is waiting.
- 7-2-3-2: everything readies at once during the active step. 7-6-6-1: "during this turn" effects
  end in the cleanup step, and any resulting triggers or rules management (for example a Unit whose
  HP buff expires with lethal damage on it) resolve there.
- 8-2-3 and 8-6-1: when "during this battle" effects start and stop.

**Battle flow**
- 8-2-4, 8-3-5, 8-4-2: if the attacker or the attack target leaves, skip to the battle end step.
- 8-3-1 to 8-3-4, 13-1-4-1, 13-1-6-1: at most one Blocker per attack, and the Blocker is rested.
  The original target cannot block for itself. High-Maneuver stops every Blocker. 5-22-2: after a
  block the previous target is no longer battling.
- 5-22-3, 5-22-4 and their -1 rules: a battle started by an effect skips the attack step, so
  【Attack】 / "when attacks" do not trigger. 13-1-5-4 (revised in 1.9.0): First Strike still
  applies when an effect performs "only the damage step".
- 8-5-3-2, 8-5-3-2-3: unit-vs-unit damage is simultaneous, and if both die they die at the same
  time (their 【Destroyed】 triggers are simultaneous and follow active-player-first ordering; Q56
  puts Breach before Destroyed). 13-1-2-3: Breach still fires when both Units die.
- 8-5-3-2-2, 13-1-5-1, 13-1-5-2: First Strike works only for the **attacking** Unit. The target is
  destroyed by rules management (11-1-2) before it can strike back. First Strike vs Bases is
  covered in the ambiguity table above.
- 8-5-2-2, 8-5-2-3, 8-5-2-3-1, 3-5-3, 8-5-2-4: an attack on the player damages the Base if there
  is one, otherwise the top Shield (1 HP), otherwise the player (immediate defeat, 1-2-2-1).
  5-5-6: excess damage never carries over to another Shield. 13-1-7-1 to 13-1-7-4: Suppression
  destroys two Shields simultaneously, reveals them together, and their owner orders the Bursts.
- 13-1-2-1 to 13-1-2-4: Breach works only during your turn, hits the first card of the shield area
  (Base first), and does nothing if the shield area is empty.
- 5-10-3, 13-2-5-1 to 13-2-5-3: reveal the destroyed Shield, choose whether to use its Burst, and
  resolve it before the card goes to the trash. The card then goes to the trash unless it moved (for
  example a Burst that deploys itself as a Base). Q83: Shields added to the hand never Burst.

**Effect resolution**
- 5-20-1, 5-20-2, 10-1-8-1-2: after "If you do", the rest resolves only if the earlier part
  resolved; after "Then", it resolves regardless. A Command cannot be played if the portion before
  "Then" / "If you do" has no legal target.
- 10-1-8-1-1, 10-2-2, 10-2-2-1, 10-3-3, 10-3-3-1: targets are chosen when the instruction is
  reached, not when the card is played. Only players and cards in public locations count as
  "targets". An effect whose target cannot be chosen does not activate.
- 10-1-9-1, 10-1-9-1-1, 1-2-5: substitution effects ("B instead of A") replace the event.
  Conceding is never a substitution.
- 1-3-2, 1-3-2-1, 1-3-2-2, 10-1-3: do as much as possible. Putting something into a state it is
  already in does nothing and triggers nothing. Zero or negative repetitions do nothing.
- 1-3-3, 10-1-5-6, 10-2-3: "can't" effects beat "must" effects and beat other constant effects.
  10-1-5-7: a constant effect with a conditional target applies the moment a matching target appears.
- 1-3-4: simultaneous choices are made active player first. 1-3-5: chosen numbers are at least 1.
  1-3-6: add up the modifiers, then clamp at 0.
- 1-3-7: text resolves in printed order. 10-3-4: text with no stated subject refers to its own
  card or its owner. 10-3-5: choosing a card from the deck.
- 13-1-1-2, 13-1-2-5, 13-1-3-2: Repair, Breach and Support stack additively. 13-1-4-2, 13-1-5-3,
  13-1-6-2, 13-1-7-2: Blocker, First Strike, High-Maneuver and Suppression do not stack.
- 13-1-8-1, 13-1-8-2: Development is an optional exile cost gated by "If you do"; its effect is the
  text after ■ (on exile, see the terminology table).
- 13-2-1-1, 13-2-3-1, 13-2-4-2: 【Activate･Main】 and 【Main】 cannot be used during a battle.
  A Command's 【Pilot】 side cannot be paired during an action step.
- 9-2 to 9-5: an action step ends only after two passes in a row, and the standby player acts
  first.

**Zones, identity and tokens**
- 4-1-5: a card that changes location becomes a new object and loses all applied effects.
  3-3-6: a paired Pilot follows its Unit to the same location.
- 4-1-6, 4-1-7: the owner orders cards placed simultaneously, and the order stays hidden when they
  go into a private location.
- 5-10-1, 5-10-4, 5-12-2, 11-4-2-1, 11-5-2-1: an effect that moves a field card to the trash is a
  **destroy**. Trashing for excess management and removing a card are not.
- 11-4-2, 11-4-2-2, 11-5-2: deploying into a full battle area or base section trashes an equal
  number of existing cards (chosen by the player) before the new ones arrive (Q29 for Bases).
- 5-17-2-5, 5-17-2-5-1, 5-17-4: a token that leaves the field moves to its destination for a
  moment (so "when destroyed" or "when returned to hand" still trigger), then leaves the game
  entirely (it does not go to the removal area). 5-17-2-2: Unit tokens can be paired.
  5-17-2-3, 5-17-2-4: tokens have no color, and their Lv and cost are 0.
- 2-9-1, 2-9-4, 5-17-3-2-3: the Lv requirement counts every Resource, including EX Resources and
  rested ones. An EX Resource used to pay a cost leaves the game.
- 3-2-4, 3-2-6-3, 13-2-11-1: new Units cannot attack that turn unless they are Link Units.
  3-2-6-4, 2-2-3: name matching by substring. 2-4-3, 2-5-5, 3-3-7: a Pilot's color and traits are
  never added to its Unit. 3-3-9-2, 3-3-9-2-1, 2-11-3: which part of a Pilot's text the Unit gains.
  3-4-6-2 to 3-4-6-4: Commands with 【Pilot】.
- 5-5-5, 5-21-2, 5-21-2-1: zero damage is not dealt at all (no "dealt damage" triggers).
  Reductions stack (Q488). 5-6-2, 5-6-3: recovery cannot exceed the damage taken, and an undamaged
  Unit cannot recover.

**Win/loss and setup**
- 1-2-2-2, 7-3-1-1, 11-2-1-2 (Q17): a player loses the moment their deck is empty, even after
  drawing the last card; they do not wait for a failed draw. Any effect that empties the deck counts.
- 1-2-2-1, 11-2-1-1: only **battle** damage to a player with an empty shield area defeats them.
  Effect damage does not.
- 1-2-3, 11-1-2: rules management runs immediately; see the ambiguity table for double defeat.
- 6-2-1-6-1, 6-2-1-7: to redraw, put the hand on the bottom of the deck, draw 5, then shuffle.
  Player One decides first.
- 6-2-2 to 6-2-4: six Shields go down before the EX Base (both players) and the EX Resource
  (Player Two only). 7-3-1: Player One also draws on turn 1 (Q15).
- 4-8-4, 7-6-5-1: the hand limit of 10 is enforced only in your own hand step.
