# Official deck-building and tournament sources

Research notes on the official sources behind the simulator's deck validation and match procedure.
The sources are the English (US) site `https://www.gundam-gcg.com/en/` and the English (Asia) site
`https://www.gundam-gcg.com/asia-en/`. Everything was retrieved on 2026-09-25, between 12:46Z and
13:05Z.

- Raw pages are in `data/official_raw/`. Each page is saved as `<slug>.html` or `.pdf` exactly as
  fetched, next to a `<slug>.txt` text extraction.
- The provenance fragment is `data/sources.d/official.json`, with 34 entries (URL, retrieval
  time, sha256, local path).
- The normalized data is in `src/gcg_sim/data/official/`:
  - `banlist.json`
  - `deck_construction.json`
  - `floor_rules.json`
  - `bo3_match_rules.json`
  - `edition_language.json`
  - `rules_version.json`

Each quote in these JSON files was checked by machine against the matching `.txt` extraction; all
matched. Each card number was checked against `src/gcg_sim/data/gcgapi/cards.ndjson` (dataset
`25-676f1bf…`). All 28 numbers exist, and every name matches exactly.

## 1. Source map

| Topic | Official source | Date on the source |
| --- | --- | --- |
| Rules hub | `/en/rules/` | CR "Updated September 11, 2026"; TRM "Updated July 25, 2025"; Tournament Rules "Updated November 19, 2025"; BO3 "Updated July 3, 2026" |
| Current Banned/Restricted list | `/en/news/01_279.html` | page dated September 25, 2026 |
| B&R announcement, July 2026 | `/en/news/01_277.html` | announced 2026-07-10, effective 2026-07-24 (US) |
| B&R announcement, April 2026 | `/en/news/01_234.html` | announced 2026-03-27, effective 2026-04-01 |
| Comprehensive Rules | `/en/pdf/comprehensiverules_en.pdf` | Ver. 1.9.0, Sep 11, 2026 |
| Tournament Rules Manual (TRM); the site labels it "Organized Play Tournament Rules Manual" | `/en/pdf/floor_rule_en.pdf` | PDF says "Last Updated: July 10th, 2025" |
| Sanctioned Tournament Floor Rules (site label "Tournament Rules") | `/en/pdf/tournament-rules_en.pdf` | Ver. 1.1.0, last edited November 19, 2025 |
| BO3 Match Rules | `/en/news/best-of-three.html` | July 03, 2026 |
| Language usage rules | `/en/news/lang_card_rule.html` | current as of July 11, 2025 |
| Deck-building guide / play guide | `/en/news/decks-build.html`, `/en/welcome/playguide.php` | 2025-07-25 / undated |
| Preparing to Play FAQ | `/en/rules/faqs/list.php?sub_category=Preparing+to+Play` | Q1–Q12, July 04, 2025 |
| Errata notices | `/en/news/01_204.html`, `02_157.html`, `02_193.html` | 2026-01-30, 2026-04-10, 2026-09-04 |
| Edition Beta | `/en/news/003.html`, `/en/products/limitedbox-beta.html`, `/en/events/WkndBetaBattle-2025.html` | 2024-12 to 2025-05 |

Pages that were read but not kept in the repo:
- News listing pages 2–14. Every entry through 2026-09-25 was checked.
- 4 more event pages and the decks page.
- The Asia news entry `region-number-system`.
- The JP Newtype Challenge PDF, searched for Edition Beta rules; it has none.

**Nothing newer than the July 2026 announcement exists except the current-list page itself.**
The RULES-tagged news through 2026-09-25 consists of 01_279 (2026-09-25), 01_277 (2026-07-10), BO3
(2026-07-03), Team Battle, the Illustrator notice, the language rules, Sealed Battle and Battle
Royale. The April 2026 notice (01_234) is filed under NEWS.

## 2. Banned / Restricted list (current)

The rules hub defines the terms:

> "When a card is banned, that means that not even one copy of that card can be included in either
> your deck or your sideboard, and when a card is restricted, that means that only a specified number
> of copies of that card may be included in your deck and your sideboard."
>
> "Cards with the same card number and different illustrations are still considered the same card."

The current list (`01_279`, dated 2026-09-25):

| Category | Cards |
| --- | --- |
| Banned | GD01-020 Anksha |
| Restricted 〈2〉 | ST02-016 Corsica Base |
| Banned pair | A: ST01-010 Amuro Ray / B: ST05-010 Mikazuki Augus |
| Banned pair | A: GD01-008 Guntank / B: GD05-015 M1 Astray Shrike |
| Attribute banned pairs (22 listed) | GD01-035, GD01-060, GD01-085, GD02-013, GD02-080, GD03-032, GD03-063, GD04-078, GD05-014, GD05-027, GD05-042, GD05-062, GD05-077, ST01-005, ST04-008, ST05-004, ST05-009, ST06-004, ST09-005, ST10-005, **ST11-008, ST12-010** (these two are new since July) |

The attribute rule:

> "All combinations of cards that match the above description "a Unit card that is Lv.2 with cost 1,
> 2 AP, and 2 HP, and without effects" are included as banned pairs, and no more than four copies of
> one card matching this description can be used in a deck."

The July 2026 notice adds: "Additionally, matching cards printed in GD05 and after will also be added
to the above banned pair listing at the time of their release."

**Predicate check.** In the card data, `UNIT ∧ level 2 ∧ cost 1 ∧ ap 2 ∧ hp 2 ∧ effect == "-"`
matches exactly the 22 listed card numbers. No alternate printing disagrees with its canonical
printing. `banlist.json` stores the rule as a predicate, `$matched` binding included, alongside the
enumerated list. A scratch evaluator applied it to 10 sample decks and got the expected result each
time. For example, 4×GD01-035 passes, while GD02-080 Nemo with ST10-005 Nemo is rejected.

**Exceptions stated in the announcements:**
- **ST05, July 2026:** "We recognize that two cards matching the description are included in the
  Iron Bloom [ST05] starter deck, and players will still be able to use this deck in official and
  sanctioned tournaments so long as no changes are made to its lineup."
- **ST02, April 2026:** "We recognize that three copies of ST02-016 Corsica Base are included in the
  Wings of Advance [ST02] starter deck, and players will still be able to use this deck in official
  and sanctioned tournaments so long as no changes are made to its contents."

**Effective dates.**
- The July list took effect on 2026-07-24 in the US. The Asia site says 2026-07-25.
- The US notice also says: "Official Season 1 Regional Tournaments already scheduled for July will
  not be affected".
- Corsica Base has been restricted since 2026-04-01.
- The current-list page has no separate effective date. `banlist.json` uses 2026-09-25, the page
  date, which is also the ST11/ST12 release date.

**Format distinctions.** The list applies because "there are occasions, such as official or
sanctioned tournaments, when certain cards will be designated as banned or restricted". No other
constructed format exists. Store events recommend BO1, and the Season 2
Regionals are "Constructed BO1".

## 3. Deck construction

Every official statement agrees; `deck_construction.json` quotes each one.

- CR 6-1-1: "A deck consists of exactly 50 cards, and a resource deck consists of exactly 10 cards."
  - 6-1-1-1: Unit, Pilot, Command and Base cards only.
  - 6-1-1-2: "either one or two card colors".
  - 6-1-1-3: up to four copies per card number.
  - 6-1-1-4/5: the resource deck holds Resource cards only, in any number of copies.
- CR 6-1-2: each player prepares one EX Base and one EX Resource token. Other tokens are prepared as
  needed, outside the deck (FAQ Q7).
- FAQ Q1–Q6 restate these rules. FAQ Q4 adds that different card numbers with the same name each get
  their own 4 copies.
- TRM 2.4: "Up to two unique card colors (red, blue, green, white, purple) can be included in a deck.
  Cards of a tertiary or more colors cannot be added to a deck."
- Card data: every UNIT, PILOT, COMMAND and BASE card number has exactly one color. No colorless
  main-deck card exists.
- Tournament-only rules the simulator does not model:
  - the 7-day ban on newly released card numbers at Premier events;
  - sleeves, card condition and deck lists.

## 4. Floor rules (shuffle, cut, first player, redraw, game start)

**Shuffle.** Sanctioned Floor Rules Section 8.I: "Decks must be thoroughly shuffled at the start of
each game." Then "allow your opponent to cut or shuffle your deck to confirm it is ready. … After your
opponent has finished cutting or shuffling your deck, you cannot cut or shuffle it again. Shuffling
by your opponent is not necessary provided that both players agree to skip it."

**Cut by proxy.** Split the deck into three roughly equal stacks. The opponent chooses the order,
and the stacks are recombined in that order.

**Simulator model.** Shuffling uses a seeded Fisher–Yates permutation. The confirming cut or
shuffle is not modelled, because it leaves a uniform permutation uniform.

**First player.** CR 6-2-1-4: "The winner decides who becomes Player One." FAQ Q9 adds that the
winner chooses "before looking at their starting hand". TRM 4.6 says the choice happens after both
decks are shuffled.

**Redraw.** Each player may redraw once, Player One first (CR 6-2-1-6). "return your entire hand to
the bottom of your deck and draw five new cards … Then, shuffle your deck." (6-2-1-6-1). Player Two
decides after Player One has announced (6-2-1-7). FAQ Q10 says the same. There is no partial
mulligan.

**Game start order.** Steps 6-2-1-1 to 6-2-5:
1. Present the decks.
2. Shuffle.
3. Place the resource deck.
4. Decide Player One.
5. Draw 5.
6. Player One redraws or keeps, then Player Two.
7. Place 6 shields, one at a time. FAQ Q8: the top card of the deck becomes the bottom Shield.
8. Place the EX Base.
9. Player Two places an EX Resource.
10. Player One's turn begins.

**Deck checks.** The TRM defines judge deck checks and Game Loss penalties. The simulator's
equivalent is to validate both decks before the match.

## 5. BO3 match rules

- "up to three one-on-one games are played within a 60-minute time limit, and the first player to
  win two games wins the match."
- "During the second and third games, Players One and Two are chosen by the player who lost the
  previous game."
- TRM 4.6 agrees: "the player who loses the preceding game chooses whether to go first in the next
  game."
- A 10-card sideboard is allowed, with 4 copies and 2 colors counted across deck + sideboard. The
  simulator ignores it, as instructed.
- The simulator also ignores time limits and extra turns. Officially, time running out in Player
  One's turn gives 3 extra turns and in Player Two's turn 2, followed by tiebreaks.

**Draws.**
- In the rules: CR 11-2-1 defeats every player who meets a defeat condition together. CR 1-2-1
  names a winner only when exactly one player is defeated.
- In Swiss rounds: a drawn match scores 1 point.
- In single elimination and top cut, where BO3 is used, TRM 5.2 applies: "If there is a tie due to
  all players fulfilling loss conditions simultaneously, the current turn-player loses the game."
- Suggestion: record simultaneous defeat as a draw in single-game statistics, and apply the TRM 5.2
  rule inside BO3. The lead decides.

## 6. Language and edition rules

- **Language.**
  - English cards are for North America, Latin America, Europe, Oceania and seven Asian regions.
  - In those seven Asian regions, Japanese and English cards may be mixed.
  - Resource and token cards of any language are allowed everywhere.
  - TRM 2.4.1: "Players in North America, Latin America, Europe and Oceania must use English
    language cards for the main deck at all tournament events."
  - None of this affects the simulator: cards are identified by card number, and the text comes from
    the English data.
- **Edition Beta.** No official statement on tournament legality was found on the US site, the
  Asia site or in the FAQ search.
  - The Producer Letter calls it a beta and says "We hope to include improvements from everyone's
    reactions in the full release."
  - Beta events used "Only Edition Beta Cards and Demo Deck Cards".
  - Card data holds 85 Edition Beta printings. Only two card numbers exist solely in Edition Beta:
    RP-001 (Resource) and EXBP-001 (EX Base). Both are resource or token cards, which may be used in
    any language or edition.
  - 19 Beta printings have gameplay fields that differ from their canonical printing. Most use older
    wording, for example Breach damages "the first card" instead of "a card".
  - Suggestion: validate by card number and always use canonical text plus errata.
- **Errata.** "For the applicable cards, the above shall be regarded as the correct wording."
  - T-013 and GD04-067 are in the gcg-api `errata.json` and reflected in the card data.
  - **ST12-001 Gundam Epyon (2026-09-04, adds 【Once per Turn】) is missing** from `errata.json` and
    from `cards.ndjson`. The official card-database page also still shows the old text.

## 7. Comprehensive Rules version

The rules page and the PDF header both say **Ver. 1.9.0, Updated Sep 11, 2026**. The PDF's HTTP
`Last-Modified` header is Fri, 11 Sep 2026 03:38:04 GMT, and its sha256 is `ba9c335c…`.

The Asia PDF is byte-identical to the US PDF. The PDF and the repo's
`gundam-card-game-comprehensive-rules.md` contain the same 566 rule ids. Section 6 was compared word
for word and matches.

## 8. English (Asia) differences

- The B&R list has the same contents. The July list took effect **2026-07-25** in Asia instead of
  2026-07-24. The Asia notice also leaves out the US paragraph about Season 1/2 Regionals and deck
  resets.
- The BO3, April B&R and language pages match the US versions, apart from minor wording.
- The Asia site has no TRM. Its "Floor Rules" link goes to the multi-title *BANDAI CARD GAMES Floor
  Rules Ver.1.1.1* (2026-02-24) on carddass.com. That PDF is 3,046,969 bytes, so only its text is
  kept in the repo; its hash is in the lock fragment. Compared with the US rules:
  - the final time-out tiebreak is one round of rock-paper-scissors, not a double loss;
  - "Some tournaments may allow sideboards to be used in addition to decks.";
  - cards must be "in the language permitted for the area";
  - the 7-day rule applies to Newtype Challenge events.
- The Comprehensive Rules PDF is identical.

## 9. Ambiguities and conflicts

`banlist.json` → `ambiguities` records the B&R items. They are summarized here.

1. **Predicate vs enumerated list for the vanilla pair rule.** The two are identical today.
   Suggestion: enforce the predicate, and have the data refresh fail if the two diverge.
2. **"without effects" is not defined.** Suggestion: a card has no effects when its effect text is
   empty or "-"; keywords count as effects. No current card is affected either way.
3. **The starter-deck exceptions (ST02, ST05) cannot be checked offline.** Neither the official
   pages nor gcg-api contain the starter decklists. The current-list page does not restate the
   exceptions, but nothing revokes them. Suggestion: do not apply them; name them in the violation
   message.
4. **Effective date and region.** The US date is 2026-07-24 and the Asia date 2026-07-25. The
   current page carries no effective date. The simulator validates against a single current list.
5. **Scope of the B&R list.** The Comprehensive Rules do not mention it; it applies to official and
   sanctioned tournaments. Suggestion: enforce it by default.
6. **Whether banned pairs cover the sideboard.** The text does not say; this does not matter without
   sideboards.
7. **Sideboards.** TRM 2.4 (2025-07-10) says "No side decks are permitted." The BO3 page
   (2026-07-03) allows a 10-card sideboard. The newer page governs BO3; the simulator uses no
   sideboard either way.
8. **Which game follows a draw in BO3.** No rule covers who chooses Player One after a drawn game.
   Suggestion: use the random game-1 procedure.
9. **The ST12-001 errata is missing from the pinned card data.** Suggestion: add an
   `overrides.json` entry that adds 【Once per Turn】 to the 【During Pair･Lv.5 or Higher Pilot】
   effect.
10. **Date labels differ.** The site labels the TRM "Updated July 25, 2025" (HTTP Last-Modified
    2025-07-25). The PDF itself says "Last Updated: July 10th, 2025".

The deck construction and redraw rules agree across CR 6-1/6-2, the live FAQ Q1–Q12 and the gcg-api
`rules-faq.json`. The live Preparing to Play FAQ matches the snapshot exactly, all 12 entries.

## 10. Method

- Fetching used `curl -sSL` with a descriptive User-Agent, one request at a time with pauses.
- HTML text was extracted with a stdlib `html.parser` script. PDF text was extracted with `pypdf`
  run in a throwaway environment (`uv run --no-project --with pypdf`), so it is not a project
  dependency.
- The JSON is generated with sorted keys. The generator checks that every quote appears in the
  extraction and that every card number and name exists in the card data. Rerunning it produced
  byte-identical output.
