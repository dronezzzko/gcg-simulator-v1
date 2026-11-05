# red-green-zeon vs blue-white-federation

Deck under test (DUT) **red-green-zeon** against benchmark **blue-white-federation**: 6 BO3 matches, master seed 21, AI preset `standard` (agents `test-random` / `test-random`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt --matches 6 --seed 21 --format bo3 --ai-preset standard --replays 3`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 6 | 0 | 0 | 6 | 0.0% [0.0%, 39.0%] (n=6) |
| Games | 18 | 0 | 0 | 18 | 0.0% [0.0%, 17.6%] (n=18) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 11 | 0 | 0 | 11 | 0.0% [0.0%, 25.9%] (n=11) |
| On the draw | 7 | 0 | 0 | 7 | 0.0% [0.0%, 35.4%] (n=7) |
| Game 1 | 6 | 0 | 0 | 6 | 0.0% [0.0%, 39.0%] (n=6) |
| Game 2 | 6 | 0 | 0 | 6 | 0.0% [0.0%, 39.0%] (n=6) |
| Game 3 | 6 | 0 | 0 | 6 | 0.0% [0.0%, 39.0%] (n=6) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| turn_limit | 18 | 0 | 0 | 18 | 0.0% [0.0%, 17.6%] (n=18) |

Game length (turns): mean 9.0, median 9, p10 9, p90 9, range 9-9. Median by DUT result: win -, loss -, draw 9.

Draws: 18 (0 simultaneous defeat, 18 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 50.0% [29.0%, 71.0%] (n=18); DUT win rate after a redraw 0.0% [0.0%, 29.9%] (n=9), after keeping 0.0% [0.0%, 29.9%] (n=9). Benchmark redraw rate 55.6% [33.7%, 75.4%] (n=18).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-030 Rick Dom | 4 | 14 (77.8%) | 11 | 3.9 | 0.0% [0.0%, 21.5%] (n=14) | 0.0% [0.0%, 49.0%] (n=4) | 0.0% [0.0%, 25.9%] (n=11) | 6 (30.0%) | 50.0% |
| GD01-031 Gelgoog | 4 | 11 (61.1%) | 5 | 4.3 | 0.0% [0.0%, 25.9%] (n=11) | 0.0% [0.0%, 35.4%] (n=7) | 0.0% [0.0%, 43.4%] (n=5) | 7 (53.8%) | 50.0% |
| GD01-035 Zaku Ⅱ | 3 | 8 (44.4%) | 6 | 2.9 | 0.0% [0.0%, 32.4%] (n=8) | 0.0% [0.0%, 27.8%] (n=10) | 0.0% [0.0%, 39.0%] (n=6) | 2 (22.2%) | 57.1% |
| GD01-044 Kshatriya | 2 | 5 (27.8%) | 1 | 5.0 | 0.0% [0.0%, 43.4%] (n=5) | 0.0% [0.0%, 22.8%] (n=13) | 0.0% [0.0%, 79.3%] (n=1) | 4 (80.0%) | 100.0% |
| GD01-059 Zee Zulu | 4 | 7 (38.9%) | 6 | 2.8 | 0.0% [0.0%, 35.4%] (n=7) | 0.0% [0.0%, 25.9%] (n=11) | 0.0% [0.0%, 39.0%] (n=6) | 3 (33.3%) | 50.0% |
| GD01-092 M'Quve | 3 | 7 (38.9%) | 6 | 3.9 | 0.0% [0.0%, 35.4%] (n=7) | 0.0% [0.0%, 25.9%] (n=11) | 0.0% [0.0%, 39.0%] (n=6) | 1 (12.5%) | 60.0% |
| GD01-093 Marida Cruz | 2 | 7 (38.9%) | 4 | 4.8 | 0.0% [0.0%, 35.4%] (n=7) | 0.0% [0.0%, 25.9%] (n=11) | 0.0% [0.0%, 49.0%] (n=4) | 4 (50.0%) | 0.0% |
| GD01-115 Zeon Remnant Forces | 3 | 8 (44.4%) | 7 | 3.3 | 0.0% [0.0%, 32.4%] (n=8) | 0.0% [0.0%, 27.8%] (n=10) | 0.0% [0.0%, 35.4%] (n=7) | 1 (12.5%) | 75.0% |
| GD01-125 Zanzibar | 2 | 9 (50.0%) | 6 | 4.2 | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 39.0%] (n=6) | 4 (40.0%) | 25.0% |
| GD02-048 Zaku Ⅲ (Sleeves) | 3 | 9 (50.0%) | 5 | 4.0 | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 43.4%] (n=5) | 4 (44.4%) | 16.7% |
| GD04-040 Schuzrum-Galluss | 2 | 6 (33.3%) | 2 | 4.5 | 0.0% [0.0%, 39.0%] (n=6) | 0.0% [0.0%, 24.3%] (n=12) | 0.0% [0.0%, 65.8%] (n=2) | 4 (66.7%) | 100.0% |
| ST03-001 Sinanju | 2 | 5 (27.8%) | 0 | - | 0.0% [0.0%, 43.4%] (n=5) | 0.0% [0.0%, 22.8%] (n=13) | n/a (n=0) | 5 (100.0%) | 60.0% |
| ST03-003 Geara Zulu | 2 | 9 (50.0%) | 6 | 3.7 | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 39.0%] (n=6) | 3 (33.3%) | 42.9% |
| ST03-006 Char's Zaku Ⅱ | 3 | 11 (61.1%) | 3 | 4.0 | 0.0% [0.0%, 25.9%] (n=11) | 0.0% [0.0%, 35.4%] (n=7) | 0.0% [0.0%, 56.2%] (n=3) | 9 (75.0%) | 66.7% |
| ST03-008 Zaku Ⅱ | 4 | 12 (66.7%) | 10 | 3.5 | 0.0% [0.0%, 24.3%] (n=12) | 0.0% [0.0%, 39.0%] (n=6) | 0.0% [0.0%, 27.8%] (n=10) | 4 (23.5%) | 0.0% |
| ST03-010 Full Frontal | 2 | 7 (38.9%) | 0 | - | 0.0% [0.0%, 35.4%] (n=7) | 0.0% [0.0%, 25.9%] (n=11) | n/a (n=0) | 8 (100.0%) | 20.0% |
| ST03-012 Indignation | 2 | 9 (50.0%) | 9 | 2.7 | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 29.9%] (n=9) | 0.0% [0.0%, 29.9%] (n=9) | 1 (10.0%) | 0.0% |
| ST03-013 Close Combat | 3 | 10 (55.6%) | 10 | 3.4 | 0.0% [0.0%, 27.8%] (n=10) | 0.0% [0.0%, 32.4%] (n=8) | 0.0% [0.0%, 27.8%] (n=10) | 1 (7.1%) | 62.5% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.1.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

## Conflict resolutions affecting these decks

- `ruling:GD01-001:Q119` (GD01-001 Gundam; benchmark): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `ambiguous:breach-reminder` (GD01-030 Rick Dom; deck_under_test): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `divergent:GD01-030` (GD01-030 Rick Dom; deck_under_test): Use GD01-030. The printings differ only in parenthesized reminder text.
- `ruling:GD01-059:Q135` (GD01-059 Zee Zulu; deck_under_test): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.
- `divergent:GD01-087` (GD01-087 Sayla Mass; benchmark): Use GD01-087. The printings differ only in parenthesized reminder text.
- `ambiguous:breach-reminder` (GD01-092 M'Quve; deck_under_test): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `data-null:BASE:ap` (GD01-125 Zanzibar; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:GD01-125:Q157` (GD01-125 Zanzibar; deck_under_test): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `ambiguous:GD03-001:when-effect-destroys-enemy` (GD03-001 Gundam NT-1; benchmark): Draw only if the chosen Unit was destroyed and is no longer in the battle area right after this effect's damage and rules management.
- `ruling:GD03-001:Q209` (GD03-001 Gundam NT-1; benchmark): 'When this effect destroys an enemy Unit' is checked right after this effect's damage is dealt (rules management); later destruction does not count.
- `data-null:BASE:ap` (GD04-122 Jaburo; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `divergent:R-001` (R-001 Resource; benchmark, deck_under_test): Use R-001_p7: a Resource card has no AP or HP. The Edition Beta record R-001_p4 (AP3/HP1) is a data error.
- `ruling:ST01-001:Q113` (ST01-001 Gundam; benchmark): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `divergent:ST03-001` (ST03-001 Sinanju; deck_under_test): Use ST03-001. The SC01 reprint (_p2) lists the two effects in the other order, which changes nothing: they are separate effects (ruling Q164).
- `ruling:ST03-001:Q164` (ST03-001 Sinanju; deck_under_test): An effect gated by 【During Pair】/【During Link】/'while …' is not possessed while the gate is closed (so it does not satisfy 'Unit with a 【Destroyed】 effect' or 'Unit card with <Keyword>'); each 【During Pair】 prefix governs only its own effect.
- `ruling:ST03-010:Q145` (ST03-010 Full Frontal; deck_under_test): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `data-null:BASE:ap` (ST08-015 Davao; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).

## Replays

- `replays/draw-m0000-g1.json`: draw, match 0 game 1, 9 turns, turn_limit

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
